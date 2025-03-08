use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{collections::HashMap, pin::Pin};
use tokio::sync::Mutex;

use crate::base::{Config, Structure};

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub model: Option<String>,
    pub max_loops: i32,
    pub metadata: HashMap<String, String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: None,
            description: None,
            model: None,
            max_loops: 100,
            metadata: HashMap::new(),
        }
    }
}

/// Base Agent implementation
#[derive(Clone, Debug)]
pub struct BaseAgent {
    config: AgentConfig,
    base_config: Config,
    conversation: Arc<Mutex<Vec<String>>>,
}

impl BaseAgent {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            base_config: Config::default(),
            conversation: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn add_to_conversation(&self, message: String) -> Result<()> {
        let mut conversation = self.conversation.lock().await;
        conversation.push(message);
        Ok(())
    }

    pub fn id(&self) -> &str {
        &self.config.id
    }

    pub fn name(&self) -> String {
        self.config
            .name
            .clone()
            .unwrap_or_else(|| format!("Agent-{}", self.config.id))
    }
}

impl Structure for BaseAgent {
    async fn run(&self) -> Result<()> {
        // Default implementation
        Ok(())
    }

    async fn save_to_file(&self, data: &[u8], path: std::path::PathBuf) -> Result<()> {
        tokio::fs::write(path, data).await?;
        Ok(())
    }

    async fn load_from_file(&self, path: std::path::PathBuf) -> Result<Vec<u8>> {
        Ok(tokio::fs::read(path).await?)
    }

    async fn save_metadata(&self, metadata: HashMap<String, String>) -> Result<()> {
        let data = serde_json::to_vec(&metadata)?;
        self.save_to_file(&data, self.base_config.metadata_path.clone())
            .await
    }

    async fn load_metadata(&self) -> Result<HashMap<String, String>> {
        let data = self
            .load_from_file(self.base_config.metadata_path.clone())
            .await?;
        Ok(serde_json::from_slice(&data)?)
    }

    async fn log_error(&self, error: anyhow::Error) -> Result<()> {
        let error_data = format!("{}", error);
        tokio::fs::write(self.base_config.error_path.join("error.log"), error_data).await?;
        Ok(())
    }

    async fn save_artifact(&self, artifact: Vec<u8>) -> Result<()> {
        self.save_to_file(&artifact, self.base_config.artifact_path.clone())
            .await
    }

    async fn load_artifact(&self, path: std::path::PathBuf) -> Result<Vec<u8>> {
        self.load_from_file(path).await
    }

    async fn log_event(&self, _event: String) -> Result<()> {
        // TODO: Implement event logging
        Ok(())
    }
}

impl super::agent_trait::Agent for BaseAgent {
    fn run(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn send_message(
        &self,
        message: String,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move { self.add_to_conversation(message).await })
    }

    fn receive_message(&self) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        Box::pin(async move {
            let conversation = self.conversation.lock().await;
            Ok(conversation
                .last()
                .cloned()
                .unwrap_or_else(|| String::from("No messages")))
        })
    }

    fn id(&self) -> String {
        self.id().to_string()
    }

    fn name(&self) -> String {
        self.name()
    }

    fn clone_box(&self) -> Box<dyn crate::agent_trait::Agent> {
        Box::new(self.clone())
    }
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
