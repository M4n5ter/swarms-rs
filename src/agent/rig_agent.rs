use std::{
    hash::{Hash, Hasher},
    path::Path,
    sync::Arc,
};

use futures::{StreamExt, stream};
use rig::{
    agent::AgentBuilder,
    completion::{Chat, Prompt},
};
use serde::Serialize;
use tokio::sync::mpsc;
use twox_hash::XxHash3_64;

use crate::{
    agent::Agent,
    conversation::{AgentConversation, AgentShortMemory, Role},
    persistence,
};

use super::{AgentConfig, AgentError};

/// Wrapper for rig's Agent
#[derive(Serialize)]
pub struct RigAgent<M, L = NoMemory>
where
    M: rig::completion::CompletionModel,
    L: rig::vector_store::VectorStoreIndexDyn,
{
    #[serde(skip)]
    agent: rig::agent::Agent<M>,
    config: AgentConfig,
    short_memory: AgentShortMemory,
    #[serde(skip)]
    long_term_memory: Option<L>,
}

impl<M, L> RigAgent<M, L>
where
    M: rig::completion::CompletionModel,
    L: rig::vector_store::VectorStoreIndex,
{
    /// Create a new RigAgent
    pub fn new(
        model: M,
        config: AgentConfig,
        system_prompt: impl Into<String>,
        long_term_memory: impl Into<Option<L>>,
    ) -> Self {
        let agent = AgentBuilder::new(model)
            .preamble(&system_prompt.into())
            .temperature(config.temperature)
            .build();

        Self {
            agent,
            config,
            short_memory: AgentShortMemory::new(),
            long_term_memory: long_term_memory.into(),
        }
    }

    /// Handle error in attempts
    async fn handle_error_in_attempts(&self, task: &str, error: AgentError, attempt: u32) {
        let err_msg = format!("Attempt {}, task: {}, failed: {}", attempt, task, error);
        tracing::error!(err_msg);

        if self.config.autosave {
            let _ = self.save_task_state(task.to_owned()).await.map_err(|e| {
                tracing::error!(
                    "Failed to save agent<{}> task<{}>,  state: {}",
                    self.config.name,
                    task,
                    e
                )
            });
        }
    }
}

impl<M, L> Agent for RigAgent<M, L>
where
    M: rig::completion::CompletionModel,
    L: rig::vector_store::VectorStoreIndex,
{
    fn run(
        &self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<String, AgentError>> + Send + '_>> {
        Box::pin(async move {
            // Add task to short memory
            self.short_memory
                .add(
                    &task,
                    &self.config.name,
                    Role::User(self.config.user_name.clone()),
                    &task,
                )
                .await;

            // Plan
            if self.config.plan_enabled {
                self.plan(task.clone()).await?;
            }

            // Query long term memory
            if self.long_term_memory.is_some() {
                self.query_long_term_memory(task.clone()).await?;
            }

            // Save state
            if self.config.autosave {
                self.save_task_state(task.clone()).await?;
            }

            // Run agent loop
            let mut last_response = String::new();
            let mut all_responses = vec![];
            for _loop_count in 0..self.config.max_loops {
                let mut success = false;
                let task_prompt = self.short_memory.0.get(&task).unwrap().to_string(); // Safety: task is in short_memory
                for attempt in 0..self.config.retry_attempts {
                    if success {
                        break;
                    }

                    if self.long_term_memory.is_some() && self.config.rag_every_loop {
                        // FIXME: if RAG success, but then LLM fails, then RAG is not removed and maybe causes issues
                        if let Err(e) = self.query_long_term_memory(task_prompt.clone()).await {
                            self.handle_error_in_attempts(&task, e, attempt).await;
                            continue;
                        };
                    }

                    // Generate response using LLM
                    let history = (&(*self.short_memory.0.get(&task).unwrap())).into(); // Safety: task is in short_memory
                    last_response = match self.agent.chat(task.clone(), history).await {
                        Ok(response) => response,
                        Err(e) => {
                            self.handle_error_in_attempts(&task, e.into(), attempt)
                                .await;
                            continue;
                        }
                    };

                    // Add response to memory
                    self.short_memory
                        .add(
                            &task,
                            &self.config.name,
                            Role::Assistant(self.config.name.to_owned()),
                            last_response.clone(),
                        )
                        .await;

                    // Add response to all_responses
                    all_responses.push(last_response.clone());

                    // TODO: evaluate response
                    // TODO: Sentiment analysis

                    success = true;
                }

                if !success {
                    // Exit the loop if all retry failed
                    break;
                }

                if self.is_response_complete(last_response.clone()) {
                    break;
                }

                // TODO: Loop interval, maybe add a sleep here
            }

            // TODO: Apply the cleaning function to the responses
            // clean and add to short memory. role: Assistant(Output Cleaner)

            // Save state
            if self.config.autosave {
                self.save_task_state(task.clone()).await?;
            }

            // TODO: Handle artifacts

            // TODO: More flexible output types, e.g. JSON, CSV, etc.
            Ok(all_responses.concat())
        })
    }

    fn run_multiple_tasks(
        &mut self,
        tasks: Vec<String>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<String>, AgentError>> + Send + '_>> {
        let agent_name = self.name();
        let mut results = Vec::with_capacity(tasks.len());

        Box::pin(async move {
            let agent_arc = Arc::new(self);
            let (tx, mut rx) = mpsc::channel(1);
            stream::iter(tasks)
                .for_each_concurrent(None, |task| {
                    let tx = tx.clone();
                    let agent = Arc::clone(&agent_arc);
                    async move {
                        let result = agent.run(task.clone()).await;
                        tx.send((task, result)).await.unwrap(); // Safety: we know rx is not dropped
                    }
                })
                .await;
            drop(tx);

            while let Some((task, result)) = rx.recv().await {
                match result {
                    Ok(result) => {
                        results.push(result);
                    }
                    Err(e) => {
                        tracing::error!("| Agent: {} | Task: {} | Error: {}", agent_name, task, e);
                    }
                }
            }

            Ok(results)
        })
    }

    fn receive_message(
        &mut self,
        sender: Role,
        message: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<String, AgentError>> + Send + '_>> {
        self.run(format!("From {sender}: {message}"))
    }

    fn plan(
        &self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move {
            if let Some(planning_prompt) = &self.config.planning_prompt {
                let planning_prompt = format!("{} {}", planning_prompt, task);
                let plan = self.agent.prompt(planning_prompt).await?;
                tracing::debug!("Plan: {}", plan);
                // Add plan to memory
                self.short_memory
                    .add(
                        task,
                        self.config.name.clone(),
                        Role::Assistant(self.config.name.clone()),
                        plan,
                    )
                    .await;
            };
            Ok(())
        })
    }

    fn query_long_term_memory(
        &self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move {
            if let Some(long_term_memory) = &self.long_term_memory {
                let (_score, _id, memory_retrieval) =
                    &long_term_memory.top_n::<String>(&task, 1).await?[0];
                let memory_retrieval = format!("Documents Available: {memory_retrieval}");
                self.short_memory
                    .add(
                        task,
                        &self.config.name,
                        Role::User("[RAG] Database".to_owned()),
                        memory_retrieval,
                    )
                    .await;
            }

            Ok(())
        })
    }

    /// Save the agent state to a file
    fn save_task_state(
        &self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>> {
        let mut hasher = XxHash3_64::default();
        task.hash(&mut hasher);
        let task_hash = hasher.finish();
        let task_hash = format!("{:x}", task_hash & 0xFFFFFFFF); // lower 32 bits of the hash

        Box::pin(async move {
            let save_state_path = self.config.save_sate_path.clone();
            if let Some(save_state_path) = save_state_path {
                let mut save_state_path = Path::new(&save_state_path);
                // if save_state_path is a file, then use its parent directory
                if !save_state_path.is_dir() {
                    save_state_path = match save_state_path.parent() {
                        Some(parent) => parent,
                        None => {
                            return Err(AgentError::InvalidSaveStatePath(
                                save_state_path.to_string_lossy().to_string(),
                            ));
                        }
                    };
                }
                let path = save_state_path
                    .join(format!("{}_{}", self.name(), task_hash))
                    .with_extension("json");

                let json = serde_json::to_string_pretty(&self.short_memory.0.get(&task).unwrap())?; // TODO: Safety?
                persistence::save_to_file(&json, path).await?;
            }
            Ok(())
        })
    }

    fn is_response_complete(&self, response: String) -> bool {
        self.config
            .stop_words
            .iter()
            .any(|word| response.contains(word))
    }

    fn id(&self) -> String {
        self.config.id.clone()
    }

    fn name(&self) -> String {
        self.config.name.clone()
    }
}

impl From<&AgentConversation> for Vec<rig::message::Message> {
    fn from(conv: &AgentConversation) -> Self {
        conv.history
            .iter()
            .map(|msg| match &msg.role {
                Role::User(name) => {
                    rig::message::Message::user(format!("{}: {}", name, msg.content))
                }
                Role::Assistant(name) => {
                    rig::message::Message::assistant(format!("{}: {}", name, msg.content))
                }
            })
            .collect()
    }
}

#[derive(Default)]
pub struct NoMemory;

impl rig::vector_store::VectorStoreIndex for NoMemory {
    async fn top_n<T: for<'a> serde::Deserialize<'a> + Send>(
        &self,
        _query: &str,
        _n: usize,
    ) -> Result<Vec<(f64, String, T)>, rig::vector_store::VectorStoreError> {
        Ok(vec![])
    }

    async fn top_n_ids(
        &self,
        _query: &str,
        _n: usize,
    ) -> Result<Vec<(f64, String)>, rig::vector_store::VectorStoreError> {
        Ok(vec![])
    }
}
