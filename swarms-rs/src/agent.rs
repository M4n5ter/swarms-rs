use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
use tokio::sync::broadcast;

use crate::{persistence, tool::ToolError};

pub mod swarms_agent;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serde json error: {0}")]
    SerdeError(#[from] serde_json::Error),
    #[error("Broadcast error: {0}")]
    BroadcastError(#[from] broadcast::error::SendError<Result<String, String>>),
    #[error("Persistence error: {0}")]
    PersistenceError(#[from] persistence::PersistenceError),
    #[error("Invalid save state path: {0}")]
    InvalidSaveStatePath(String),
    #[error("Completion error: {0}")]
    CompletionError(#[from] crate::llm::CompletionError),
    #[error("No choice found")]
    NoChoiceFound,
    #[error("Tool {0} not found")]
    ToolNotFound(String),
    #[error("Tool error: {0}")]
    ToolError(#[from] ToolError),
}

#[derive(Clone)]
pub struct AgentConfigBuilder {
    config: AgentConfig,
}

impl AgentConfigBuilder {
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

    pub fn max_loops(mut self, max_loops: u32) -> Self {
        self.config.max_loops = max_loops;
        self
    }

    pub fn max_tokens(mut self, max_tokens: u64) -> Self {
        self.config.max_tokens = max_tokens;
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

    pub fn build(self) -> AgentConfig {
        self.config
    }
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub user_name: String,
    pub description: Option<String>,
    pub temperature: f64,
    pub max_loops: u32,
    pub max_tokens: u64,
    pub plan_enabled: bool,
    pub planning_prompt: Option<String>,
    pub autosave: bool,
    pub retry_attempts: u32,
    pub rag_every_loop: bool,
    pub save_sate_path: Option<String>,
    pub stop_words: HashSet<String>,
}

impl AgentConfig {
    pub fn builder() -> AgentConfigBuilder {
        AgentConfigBuilder {
            config: AgentConfig::default(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Agent".to_owned(),
            user_name: "User".to_owned(),
            description: None,
            temperature: 0.7,
            max_loops: 1,
            max_tokens: 8192,
            plan_enabled: false,
            planning_prompt: None,
            autosave: false,
            retry_attempts: 3,
            rag_every_loop: false,
            save_sate_path: None,
            stop_words: HashSet::new(),
        }
    }
}

pub trait Agent: Send + Sync {
    /// Runs the autonomous agent loop to complete the given task.
    fn run(&self, task: String) -> BoxFuture<Result<String, AgentError>>;

    /// Run multiple tasks concurrently
    fn run_multiple_tasks(
        &mut self,
        tasks: Vec<String>,
    ) -> BoxFuture<Result<Vec<String>, AgentError>>;

    /// Plan the task and add it to short term memory
    fn plan(&self, task: String) -> BoxFuture<Result<(), AgentError>>;

    /// Query long term memory and add the results to short term memory
    fn query_long_term_memory(&self, task: String) -> BoxFuture<Result<(), AgentError>>;

    /// Save the agent state to a file
    fn save_task_state(&self, task: String) -> BoxFuture<Result<(), AgentError>>;

    /// Check a response to determine if it is complete
    fn is_response_complete(&self, response: String) -> bool;

    /// Get agent ID
    fn id(&self) -> String;

    /// Get agent name
    fn name(&self) -> String;

    /// Get agent description
    fn description(&self) -> String;
}
