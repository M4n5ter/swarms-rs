use std::{path::Path, sync::Arc};

use futures::{StreamExt, stream};
use rig::{
    agent::AgentBuilder,
    completion::{Chat, Prompt},
};
use serde::Serialize;
use tokio::{fs, sync::Mutex};
use tracing::Level;

use crate::{
    agent::Agent,
    conversation::{AgentConversation, Role},
    file_persistence::FilePersistence,
};

use super::{AgentConfig, AgentError};

/// Wrapper for rig's Agent
#[derive(Serialize)]
pub struct RigAgent<M, L = NoMemory>
where
    M: rig::completion::CompletionModel,
    L: rig::vector_store::VectorStoreIndex,
{
    #[serde(skip)]
    agent: rig::agent::Agent<M>,
    config: AgentConfig,
    short_memory: AgentConversation,
    #[serde(skip)]
    long_term_memory: Option<L>,
}

impl<M, L> RigAgent<M, L>
where
    M: rig::completion::CompletionModel,
    L: rig::vector_store::VectorStoreIndex,
{
    /// Create a new RigAgent
    ///
    /// # Example
    ///
    /// ```
    /// use swarms_rs::rig::providers::openai;
    /// use swarms_rs::agent::{rig_agent::{RigAgent, NoMemory}, AgentConfig};
    ///
    /// let openai_client = openai::Client::from_url("Your OpenAI API Key", "https://api.openai.com/v1");
    /// let agent = RigAgent::new(
    ///     openai_client.completion_model(openai::GPT_4),
    ///     AgentConfig {
    ///         name: "Agent".to_owned(),
    ///         user_name: "User".to_owned(),
    ///         temperature: 0.7,
    ///         ..Default::default()
    ///     },
    ///     "You are a helpful assistant.".to_owned(),
    ///     None::<NoMemory>,
    /// );
    /// ```
    pub fn new(
        model: M,
        config: AgentConfig,
        system_prompt: String,
        long_term_memory: Option<L>,
    ) -> Self {
        let short_memory = AgentConversation::new(config.name.clone(), system_prompt.clone());

        let agent = AgentBuilder::new(model)
            .preamble(&system_prompt)
            .temperature(config.temperature)
            .build();

        Self {
            agent,
            config,
            short_memory,
            long_term_memory,
        }
    }

    /// Handle error in attempts
    async fn handle_error_in_attempts(&mut self, error: AgentError, attempt: u32) {
        if self.config.autosave {
            let _ = self.save_state().await.map_err(|e| {
                tracing::error!("Failed to save agent<{}> state: {}", self.config.name, e)
            });
        }

        let _ = self
            .log_event(
                format!("Attempt {} failed: {}", attempt, error),
                Level::ERROR,
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to log event: {}", e);
            });
    }
}

impl<M, L> Agent for RigAgent<M, L>
where
    M: rig::completion::CompletionModel,
    L: rig::vector_store::VectorStoreIndex,
{
    fn run(
        &mut self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<String, AgentError>> + Send + '_>> {
        Box::pin(async move {
            // Add task to short memory
            self.short_memory
                .add(Role::User(self.config.user_name.to_owned()), task.clone())
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
                self.save_state().await?;
            }

            // Run agent loop
            let mut last_response = String::new();
            let mut all_responses = vec![];
            for _loop_count in 0..self.config.max_loops {
                let mut success = false;
                let task_prompt = self.short_memory.to_string();
                for attempt in 0..self.config.retry_attempts {
                    if success {
                        break;
                    }

                    if self.long_term_memory.is_some() && self.config.rag_every_loop {
                        // FIXME: if RAG success, but then LLM fails, then RAG is not removed and maybe causes issues
                        if let Err(e) = self.query_long_term_memory(task_prompt.clone()).await {
                            self.handle_error_in_attempts(e, attempt).await;
                            continue;
                        };
                    }

                    // Generate response using LLM
                    let history = (&self.short_memory).into();
                    last_response = match self.agent.chat(task.clone(), history).await {
                        Ok(response) => response,
                        Err(e) => {
                            self.handle_error_in_attempts(e.into(), attempt).await;
                            continue;
                        }
                    };

                    // Add response to memory
                    self.short_memory
                        .add(
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

            // TODO: set agent_output

            // TODO: Handle artifacts

            // TODO: More flexible output types, e.g. JSON, CSV, etc.
            Ok(all_responses.concat())
        })
    }

    fn run_multiple_tasks(
        &mut self,
        tasks: Vec<String>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<String>, AgentError>> + Send + '_>> {
        Box::pin(async move {
            let agent_arc = Arc::new(Mutex::new(self));

            let results = stream::iter(tasks)
                .then(|task| {
                    let agent_clone = Arc::clone(&agent_arc);
                    async move {
                        let mut guard = agent_clone.lock().await;
                        guard.run(task).await
                    }
                })
                .collect::<Vec<_>>()
                .await;

            results.into_iter().collect()
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
        &mut self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move {
            if let Some(planning_prompt) = &self.config.planning_prompt {
                let planning_prompt = format!("{} {}", planning_prompt, task);
                let plan = self.agent.prompt(planning_prompt).await?;
                tracing::debug!("Plan: {}", plan);
                // Add plan to memory
                self.short_memory
                    .add(Role::Assistant(self.config.name.to_owned()), plan)
                    .await;
            };
            Ok(())
        })
    }

    fn query_long_term_memory(
        &mut self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move {
            if let Some(long_term_memory) = &self.long_term_memory {
                let (_score, _id, memory_retrieval) =
                    &long_term_memory.top_n::<String>(&task, 1).await?[0];
                let memory_retrieval = format!("Documents Available: {memory_retrieval}");
                self.short_memory
                    .add(
                        Role::User("[RAG] Database".to_owned()),
                        memory_retrieval.to_owned(),
                    )
                    .await;
            }

            Ok(())
        })
    }

    fn save_state(
        &self,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>> {
        Box::pin(async move {
            let save_state_path = self.config.save_sate_path.clone();

            if let Some(save_state_path) = save_state_path {
                let save_state_path = Path::new(&save_state_path);
                let backup_path = save_state_path.with_extension("json.bak");
                let temp_path = save_state_path.with_extension("json.tmp");
                let path = save_state_path.with_extension("json");

                // Create directories if they don't exist
                if let Some(parent) = path.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent).await?;
                    }
                }

                // First save to temporary file
                let json = serde_json::to_string_pretty(self)?;
                fs::write(&temp_path, json).await?;

                // If current file exists, backup it
                if path.exists() {
                    fs::copy(&path, &backup_path).await?;
                }

                // Rename temporary file to final file
                fs::rename(&temp_path, &path).await?;

                // Clean up unneeded backup file
                if backup_path.exists() {
                    fs::remove_file(&backup_path).await?;
                }
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

    // fn clone_box(&self) -> Box<dyn Agent> {
    //     Box::new(self.clone())
    // }
}

impl<M, L> FilePersistence for RigAgent<M, L>
where
    M: rig::completion::CompletionModel,
    L: rig::vector_store::VectorStoreIndex,
{
    fn name(&self) -> String {
        self.config.name.clone()
    }

    fn metadata_dir(&self) -> Option<impl AsRef<Path>> {
        match self.config.save_sate_path {
            Some(ref path) => {
                let path = Path::new(path);
                path.parent().map(|parent| parent.join("metadata"))
            }
            None => None,
        }
    }

    fn artifact_dir(&self) -> Option<impl AsRef<Path>> {
        match self.config.save_sate_path {
            Some(ref path) => {
                let path = Path::new(path);
                path.parent().map(|parent| parent.join("artifacts"))
            }
            None => None,
        }
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
