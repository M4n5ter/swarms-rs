use std::{
    hash::{Hash, Hasher},
    ops::Deref,
    path::Path,
    sync::Arc,
};

use dashmap::DashMap;
use futures::{StreamExt, future::BoxFuture, stream};
use serde::Serialize;
use tokio::sync::mpsc;
use twox_hash::XxHash3_64;

use crate::{
    conversation::{AgentShortMemory, Role},
    llm::{
        self,
        request::{CompletionRequest, ToolDefinition},
    },
    persistence,
    tool::{Tool, ToolDyn},
};

use super::{Agent, AgentConfig, AgentError};

pub struct SwarmsAgentBuilder<M>
where
    M: llm::Model + Send + Sync,
    M::RawCompletionResponse: Send + Sync,
{
    model: M,
    config: AgentConfig,
    system_prompt: Option<String>,
    tools: Vec<ToolDefinition>,
    tools_impl: DashMap<String, Arc<dyn ToolDyn>>,
}

impl<M> SwarmsAgentBuilder<M>
where
    M: llm::Model + Clone + Send + Sync,
    M::RawCompletionResponse: Clone + Send + Sync,
{
    pub fn new_with_model(model: M) -> Self {
        Self {
            model,
            config: AgentConfig::default(),
            system_prompt: None,
            tools: vec![],
            tools_impl: DashMap::new(),
        }
    }

    pub fn config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub fn system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(system_prompt.into());
        self
    }

    pub fn add_tool<T: Tool + 'static>(mut self, tool: T) -> Self {
        self.tools.push(tool.definition());
        self.tools_impl
            .insert(tool.name().to_string(), Arc::new(tool) as Arc<dyn ToolDyn>);
        self
    }

    pub fn build(self) -> SwarmsAgent<M> {
        SwarmsAgent {
            model: self.model,
            config: self.config,
            system_prompt: self.system_prompt,
            short_memory: AgentShortMemory::new(),
            tools: self.tools,
            tools_impl: self.tools_impl,
        }
    }

    // Configuration methods

    pub fn agent_name(mut self, name: impl Into<String>) -> Self {
        self.config.name = name.into();
        self
    }

    pub fn user_name(mut self, name: impl Into<String>) -> Self {
        self.config.user_name = name.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.config.description = Some(description.into());
        self
    }

    pub fn temperature(mut self, temperature: f64) -> Self {
        self.config.temperature = temperature;
        self
    }

    pub fn max_tokens(mut self, max_tokens: u64) -> Self {
        self.config.max_tokens = max_tokens;
        self
    }

    pub fn max_loops(mut self, max_loops: u32) -> Self {
        self.config.max_loops = max_loops;
        self
    }

    pub fn enable_plan(mut self, planning_prompt: impl Into<Option<String>>) -> Self {
        self.config.plan_enabled = true;
        self.config.planning_prompt = planning_prompt.into();
        self
    }

    pub fn enable_autosave(mut self) -> Self {
        self.config.autosave = true;
        self
    }

    pub fn retry_attempts(mut self, retry_attempts: u32) -> Self {
        self.config.retry_attempts = retry_attempts;
        self
    }

    pub fn enable_rag_every_loop(mut self) -> Self {
        self.config.rag_every_loop = true;
        self
    }

    pub fn save_sate_path(mut self, path: impl Into<String>) -> Self {
        self.config.save_sate_path = Some(path.into());
        self
    }

    pub fn add_stop_word(mut self, stop_word: impl Into<String>) -> Self {
        self.config.stop_words.insert(stop_word.into());
        self
    }

    pub fn stop_words(self, stop_words: Vec<String>) -> Self {
        stop_words
            .into_iter()
            .fold(self, |builder, stop_word| builder.add_stop_word(stop_word))
    }
}

#[derive(Clone, Serialize)]
pub struct SwarmsAgent<M>
where
    M: llm::Model + Clone + Send + Sync,
    M::RawCompletionResponse: Clone + Send + Sync,
{
    model: M,
    config: AgentConfig,
    system_prompt: Option<String>,
    short_memory: AgentShortMemory,
    tools: Vec<ToolDefinition>,
    #[serde(skip)]
    tools_impl: DashMap<String, Arc<dyn ToolDyn>>,
}

// pub type ToolFunc = Box<dyn AsyncFn(serde_json::Value) -> String + Send + Sync>;

impl<M> SwarmsAgent<M>
where
    M: llm::Model + Clone + Send + Sync + 'static,
    M::RawCompletionResponse: Clone + Send + Sync,
{
    pub fn new(model: M, system_prompt: impl Into<Option<String>>) -> Self {
        Self {
            model,
            system_prompt: system_prompt.into(),
            config: AgentConfig::default(),
            short_memory: AgentShortMemory::new(),
            tools: vec![],
            tools_impl: DashMap::new(),
        }
    }

    pub async fn chat(
        &self,
        prompt: impl Into<String>,
        chat_history: impl Into<Vec<llm::completion::Message>>,
    ) -> Result<String, AgentError> {
        let request = CompletionRequest {
            prompt: llm::completion::Message::user(prompt),
            system_prompt: self.system_prompt.clone(),
            chat_history: chat_history.into(),
            tools: self.tools.clone(),
            temperature: Some(self.config.temperature),
            max_tokens: Some(self.config.max_tokens),
        };

        let response = self.model.completion(request).await?;

        let choice = response.choice.first().ok_or(AgentError::NoChoiceFound)?;
        match ToOwned::to_owned(choice) {
            llm::completion::AssistantContent::Text(text) => Ok(text.text),
            llm::completion::AssistantContent::ToolCall(tool_call) => {
                let tool_call = tool_call.function;

                let tool = Arc::clone(
                    self.tools_impl
                        .get(&tool_call.name)
                        .ok_or(AgentError::ToolNotFound(tool_call.name))?
                        .deref(),
                );

                let result = tool.call(tool_call.arguments.to_string()).await?;

                Ok(result)
            }
        }
    }

    pub fn tool(mut self, tool: impl ToolDyn + 'static) -> Self {
        let toolname = tool.name();
        let definition = tool.definition();
        self.tools.push(definition);
        self.tools_impl.insert(toolname, Arc::new(tool));
        self
    }

    pub fn system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(system_prompt.into());
        self
    }

    /// Handle error in attempts
    async fn handle_error_in_attempts(&self, task: &str, error: AgentError, attempt: u32) {
        let err_msg = format!("Attempt {}, task: {}, failed: {}", attempt + 1, task, error);
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

impl<M> Agent for SwarmsAgent<M>
where
    M: llm::Model + Clone + Send + Sync + 'static,
    M::RawCompletionResponse: Clone + Send + Sync,
{
    fn run(&self, task: String) -> BoxFuture<Result<String, AgentError>> {
        Box::pin(async move {
            self.short_memory.add(
                &task,
                &self.config.name,
                Role::User(self.config.user_name.clone()),
                &task,
            );

            // Plan
            if self.config.plan_enabled {
                self.plan(task.clone()).await?;
            }

            // Query long term memory
            // if self.long_term_memory.is_some() {
            //     self.query_long_term_memory(task.clone()).await?;
            // }

            // Save state
            if self.config.autosave {
                self.save_task_state(task.clone()).await?;
            }

            // Run agent loop
            let mut last_response = String::new();
            let mut all_responses = vec![];
            for _loop_count in 0..self.config.max_loops {
                let mut success = false;
                // let task_prompt = self.short_memory.0.get(&task).unwrap().to_string(); // Safety: task is in short_memory
                for attempt in 0..self.config.retry_attempts {
                    if success {
                        break;
                    }

                    // if self.long_term_memory.is_some() && self.config.rag_every_loop {
                    //     // FIXME: if RAG success, but then LLM fails, then RAG is not removed and maybe causes issues
                    //     if let Err(e) = self.query_long_term_memory(task_prompt.clone()).await {
                    //         self.handle_error_in_attempts(&task, e, attempt).await;
                    //         continue;
                    //     };
                    // }

                    // Generate response using LLM
                    let history = self.short_memory.0.get(&task).unwrap(); // Safety: task is in short_memory
                    last_response = match self.chat(&task, history.deref()).await {
                        Ok(response) => response,
                        Err(e) => {
                            self.handle_error_in_attempts(&task, e, attempt).await;
                            continue;
                        }
                    };
                    // needed to drop the lock
                    // if use:
                    // let history = (&(*self.short_memory.0.get(&task).unwrap())).into();
                    // we don't need to drop the lock, because the lock is owned by temporary variable
                    drop(history);

                    // Add response to memory
                    self.short_memory.add(
                        &task,
                        &self.config.name,
                        Role::Assistant(self.config.name.to_owned()),
                        last_response.clone(),
                    );

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
    ) -> BoxFuture<Result<Vec<String>, AgentError>> {
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

    fn plan(&self, task: String) -> BoxFuture<Result<(), AgentError>> {
        Box::pin(async move {
            if let Some(planning_prompt) = &self.config.planning_prompt {
                let planning_prompt = format!("{} {}", planning_prompt, task);
                let plan = self.chat(planning_prompt, vec![]).await?;
                tracing::debug!("Plan: {}", plan);
                // Add plan to memory
                self.short_memory.add(
                    task,
                    self.config.name.clone(),
                    Role::Assistant(self.config.name.clone()),
                    plan,
                );
            };
            Ok(())
        })
    }

    fn query_long_term_memory(&self, task: String) -> BoxFuture<Result<(), AgentError>> {
        todo!()
    }

    fn save_task_state(&self, task: String) -> BoxFuture<Result<(), AgentError>> {
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

    fn description(&self) -> String {
        self.config.description.clone().unwrap_or_default()
    }

    fn clone_box(&self) -> Box<dyn Agent> {
        Box::new(self.clone())
    }
}
