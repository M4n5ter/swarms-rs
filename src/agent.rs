use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
};
use thiserror::Error;
use tokio::sync::broadcast;

use crate::conversation::Role;

pub mod rig_agent;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Rig prompt error: {0}")]
    RigPromptError(#[from] rig::completion::PromptError),
    #[error("Rig vector store error: {0}")]
    RigVectorStoreError(#[from] rig::vector_store::VectorStoreError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serde json error: {0}")]
    SerdeError(#[from] serde_json::Error),
    #[error("Broadcast error: {0}")]
    BroadcastError(#[from] broadcast::error::SendError<Result<String, String>>),
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
    pub metadata: HashMap<String, String>,
    pub plan_enabled: bool,
    pub planning_prompt: Option<String>,
    pub autosave: bool,
    pub retry_attempts: u32,
    pub rag_every_loop: bool,
    pub save_sate_path: Option<String>,
    pub stream_enabled: bool,
    pub stop_words: HashSet<String>,
}

impl AgentConfig {
    pub fn with_agent_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_user_name(mut self, name: impl Into<String>) -> Self {
        self.user_name = name.into();
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_max_loops(mut self, max_loops: u32) -> Self {
        self.max_loops = max_loops;
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn enable_plan(mut self) -> Self {
        self.plan_enabled = true;
        self
    }

    pub fn with_planning_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.planning_prompt = Some(prompt.into());
        self
    }

    pub fn enable_autosave(mut self) -> Self {
        self.autosave = true;
        self
    }

    pub fn with_retry_attempts(mut self, retry_attempts: u32) -> Self {
        self.retry_attempts = retry_attempts;
        self
    }

    pub fn enable_rag_every_loop(mut self) -> Self {
        self.rag_every_loop = true;
        self
    }

    pub fn with_save_sate_path(mut self, path: impl Into<String>) -> Self {
        self.save_sate_path = Some(path.into());
        self
    }

    pub fn enable_stream(mut self) -> Self {
        self.stream_enabled = true;
        self
    }

    pub fn with_stop_words(mut self, stop_words: HashSet<String>) -> Self {
        self.stop_words = stop_words;
        self
    }

    pub fn add_stop_word(&mut self, stop_word: impl Into<String>) {
        self.stop_words.insert(stop_word.into());
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Agent 1".to_owned(),
            user_name: "user".to_owned(),
            description: None,
            temperature: 0.7,
            max_loops: 1,
            metadata: HashMap::new(),
            plan_enabled: false,
            planning_prompt: None,
            autosave: false,
            retry_attempts: 3,
            rag_every_loop: false,
            save_sate_path: None,
            stream_enabled: false,
            stop_words: HashSet::new(),
        }
    }
}

/// Agent output data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub agent_id: String,
    pub agent_name: String,
    pub task_id: String,
    pub input: String,
    pub output: Option<String>,
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
    pub status: String,
    pub error: Option<String>,
}

impl Default for AgentOutput {
    fn default() -> Self {
        Self {
            agent_id: String::from(""),
            agent_name: String::from(""),
            task_id: String::from(""),
            input: String::from(""),
            output: None,
            start_time: Local::now(),
            end_time: Local::now(),
            status: String::from("Running"),
            error: None,
        }
    }
}

pub trait Agent: Send + Sync {
    /// Runs the autonomous agent loop to complete the given task.
    fn run(
        &mut self,
        task: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, AgentError>> + Send + '_>>;

    /// Run multiple tasks concurrently
    fn run_multiple_tasks(
        &mut self,
        tasks: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, AgentError>> + Send + '_>>;

    /// Receive a message from a user or another agent and process it
    fn receive_message(
        &mut self,
        sender: Role,
        message: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, AgentError>> + Send + '_>>;

    /// Plan the task and add it to short term memory
    fn plan(
        &mut self,
        task: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// Query long term memory and add the results to short term memory
    fn query_long_term_memory(
        &mut self,
        task: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// Save the agent state to a file
    fn save_state(&self) -> Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>>;

    /// Check a response to determine if it is complete
    fn is_response_complete(&self, response: String) -> bool;

    /// Get agent ID
    fn id(&self) -> String;

    /// Get agent name
    fn name(&self) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.max_loops, 100);
        assert!(config.metadata.is_empty());
    }
}
