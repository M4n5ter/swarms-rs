use std::{
    collections::{HashMap, VecDeque},
    fmt::Display,
    path::{Path, PathBuf},
};

use chrono::Local;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::persistence::{self, PersistenceError};

#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("Json error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("FilePersistence error: {0}")]
    FilePersistenceError(#[from] PersistenceError),
}

#[derive(Serialize)]
pub struct AgentShortMemory(pub DashMap<Task, AgentConversation>);
type Task = String;

impl AgentShortMemory {
    pub fn new() -> Self {
        Self(DashMap::new())
    }

    pub async fn add(
        &self,
        task: impl Into<String>,
        conversation_owner: impl Into<String>,
        role: Role,
        message: impl Into<String>,
    ) {
        self.0
            .entry(task.into())
            .or_insert(AgentConversation::new(conversation_owner.into()))
            .add(role, message.into())
            .await;
    }
}

impl Default for AgentShortMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Serialize)]
pub struct AgentConversation {
    agent_name: String,
    save_filepath: Option<PathBuf>,
    pub history: Vec<Message>,
}

impl AgentConversation {
    pub fn new(agent_name: String) -> Self {
        Self {
            agent_name,
            save_filepath: None,
            history: Vec::new(),
        }
    }

    /// Add a message to the conversation history.
    pub async fn add(&mut self, role: Role, message: String) {
        let timestamp = Local::now().timestamp();
        let message = Message {
            role,
            content: Content::Text(format!("Time: {timestamp} \n{message}")),
        };
        self.history.push(message);

        if let Some(filepath) = &self.save_filepath {
            if (self.save_as_json(filepath, &self.history).await).is_err() {
                // TODO: log error
            };
        }
    }

    /// Delete a message from the conversation history.
    pub fn delete(&mut self, index: usize) {
        self.history.remove(index);
    }

    /// Update a message in the conversation history.
    pub fn update(&mut self, index: usize, role: Role, content: Content) {
        self.history[index] = Message { role, content };
    }

    /// Query a message in the conversation history.
    pub fn query(&self, index: usize) -> &Message {
        &self.history[index]
    }

    /// Search for a message in the conversation history.
    pub fn search(&self, keyword: &str) -> Vec<&Message> {
        self.history
            .iter()
            .filter(|message| message.content.to_string().contains(keyword))
            .collect()
    }

    // Clear the conversation history.
    pub fn clear(&mut self) {
        self.history.clear();
    }

    pub fn to_json(&self) -> Result<String, ConversationError> {
        Ok(serde_json::to_string(&self.history)?)
    }

    /// Save the conversation history to a JSON file.
    async fn save_as_json(
        &self,
        filepath: &Path,
        data: &[Message],
    ) -> Result<(), ConversationError> {
        let json_data = serde_json::to_string(data)?;
        persistence::save_to_file(json_data.as_bytes(), filepath).await?;
        Ok(())
    }

    /// Load the conversation history from a JSON file.
    async fn load_from_json(&self, filepath: &Path) -> Result<Vec<Message>, ConversationError> {
        let data = persistence::load_from_file(filepath).await?;
        let history = serde_json::from_slice(&data)?;
        Ok(history)
    }

    /// Export the conversation history to a file
    pub async fn export_to_file(&self, filepath: &Path) -> Result<(), ConversationError> {
        let data = self.to_string();
        persistence::save_to_file(data.as_bytes(), filepath).await?;
        Ok(())
    }

    /// Import the conversation history from a file
    pub async fn import_from_file(&mut self, filepath: &Path) -> Result<(), ConversationError> {
        let data = persistence::load_from_file(filepath).await?;
        let history = data
            .split(|s| *s == b'\n')
            .map(|line| {
                let line = String::from_utf8_lossy(line);
                // M4n5ter(User): hello
                let (role, content) = line.split_once(": ").unwrap();
                if role.contains("(User)") {
                    let role = Role::User(role.replace("(User)", "").to_string());
                    let content = Content::Text(content.to_string());
                    Message { role, content }
                } else {
                    let role = Role::Assistant(role.replace("(Assistant)", "").to_string());
                    let content = Content::Text(content.to_string());
                    Message { role, content }
                }
            })
            .collect();
        self.history = history;
        Ok(())
    }

    /// Count the number of messages by role
    pub fn count_messages_by_role(&self) -> HashMap<String, usize> {
        let mut count = HashMap::new();
        for message in &self.history {
            *count.entry(message.role.to_string()).or_insert(0) += 1;
        }
        count
    }
}

impl Display for AgentConversation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for message in &self.history {
            writeln!(f, "{}: {}", message.role, message.content)?;
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Content,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Role {
    User(String),
    Assistant(String),
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Content {
    Text(String),
}

impl Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::User(name) => write!(f, "{}(User)", name),
            Role::Assistant(name) => write!(f, "{}(Assistant)", name),
        }
    }
}

impl Display for Content {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Content::Text(text) => f.pad(text),
        }
    }
}

#[derive(Serialize)]
#[serde(rename = "history")]
pub struct SwarmConversation {
    pub logs: VecDeque<AgentLog>,
}

impl SwarmConversation {
    pub fn new() -> Self {
        Self {
            logs: VecDeque::new(),
        }
    }

    pub fn add_log(&mut self, agent_name: String, task: String, response: String) {
        tracing::info!("Agent: {agent_name} | Task: {task} | Response: {response}");
        let log = AgentLog {
            agent_name,
            task,
            response,
        };
        self.logs.push_back(log);
    }
}

impl Default for SwarmConversation {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
pub struct AgentLog {
    pub agent_name: String,
    pub task: String,
    pub response: String,
}
