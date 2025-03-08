use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

use crate::agent_trait::Agent;
use crate::base::{Config, Structure};
use crate::swarm_trait::Swarm;

/// Swarm configuration
#[derive(Clone, Debug)]
pub struct SwarmConfig {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub max_loops: i32,
    pub autosave: bool,
    pub logging: bool,
    pub return_metadata: bool,
    pub metadata_filename: String,
    pub rules: Option<String>,
    pub agent_ops_on: bool,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: None,
            description: None,
            max_loops: 200,
            autosave: false,
            logging: false,
            return_metadata: false,
            metadata_filename: "multiagent_structure_metadata.json".to_string(),
            rules: None,
            agent_ops_on: false,
        }
    }
}

/// Base Swarm implementation
pub struct BaseSwarm {
    config: SwarmConfig,
    base_config: Config,
    pub agents: Arc<Mutex<Vec<Box<dyn Agent>>>>,
    conversation: Arc<Mutex<Vec<String>>>,
    agents_dict: Arc<Mutex<HashMap<String, usize>>>, // Maps agent name to index in agents vector
}

impl BaseSwarm {
    pub fn new(config: SwarmConfig, agents: Vec<Box<dyn Agent>>) -> Self {
        let agents_arc = Arc::new(Mutex::new(agents));
        let agents_dict_arc = Arc::new(Mutex::new(HashMap::new()));

        // Initialize the swarm

        // We'll initialize the agents_dict in a separate function
        // since we can't use async in new()
        Self {
            config,
            base_config: Config::default(),
            agents: agents_arc,
            conversation: Arc::new(Mutex::new(Vec::new())),
            agents_dict: agents_dict_arc,
        }
    }

    /// Initialize the agents dictionary mapping agent names to indices
    pub async fn initialize(&self) -> Result<()> {
        let agents = self.agents.lock().await;
        let mut agents_dict = self.agents_dict.lock().await;

        for (index, agent) in agents.iter().enumerate() {
            agents_dict.insert(agent.name(), index);
        }

        Ok(())
    }

    /// Add a message to the conversation
    pub async fn add_to_conversation(&self, message: String) -> Result<()> {
        let mut conversation = self.conversation.lock().await;
        conversation.push(message);
        Ok(())
    }

    /// Get agent by name
    pub async fn get_agent_by_name(&self, name: &str) -> Option<Box<dyn Agent>> {
        let agents_dict = self.agents_dict.lock().await;
        let agents = self.agents.lock().await;

        if let Some(index) = agents_dict.get(name) {
            // Clone the agent to avoid borrowing issues
            return Some(agents[*index].clone());
        }
        None
    }

    /// Get agent by id
    pub async fn get_agent_by_id(&self, id: &str) -> Option<Box<dyn Agent>> {
        let agents = self.agents.lock().await;

        for agent in agents.iter() {
            if agent.id() == id {
                return Some(agent.clone());
            }
        }

        None
    }

    /// Save swarm state to JSON
    pub async fn save_to_json(&self, filename: &str) -> Result<()> {
        let metadata = self.create_metadata().await?;
        let data = serde_json::to_vec(&metadata)?;
        let path = self.base_config.metadata_path.join(filename);

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        self.save_to_file(&data, path).await
    }

    /// Create metadata for the swarm
    async fn create_metadata(&self) -> Result<HashMap<String, String>> {
        let mut metadata = HashMap::new();
        let agents = self.agents.lock().await;

        metadata.insert("swarm_id".to_string(), self.config.id.clone());
        if let Some(name) = &self.config.name {
            metadata.insert("swarm_name".to_string(), name.clone());
        }
        if let Some(description) = &self.config.description {
            metadata.insert("swarm_description".to_string(), description.clone());
        }
        metadata.insert("agent_count".to_string(), agents.len().to_string());

        Ok(metadata)
    }
}

impl Structure for BaseSwarm {
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

    async fn log_event(&self, event: String) -> Result<()> {
        if self.config.logging {
            info!("[EVENT] {}", event);
        }
        Ok(())
    }
}

impl Swarm for BaseSwarm {
    async fn add_agent(&mut self, agent: Box<dyn Agent>) -> Result<()> {
        let agent_name = agent.name();
        let mut agents = self.agents.lock().await;
        let mut agents_dict = self.agents_dict.lock().await;

        let index = agents.len();
        agents.push(agent);
        agents_dict.insert(agent_name, index);

        Ok(())
    }

    async fn remove_agent(&mut self, agent_id: String) -> Result<()> {
        let mut agents = self.agents.lock().await;
        let mut agents_dict = self.agents_dict.lock().await;

        // Find the agent index
        let mut index_to_remove = None;
        for (i, agent) in agents.iter().enumerate() {
            if agent.id() == agent_id {
                index_to_remove = Some(i);
                break;
            }
        }

        // Remove the agent if found
        if let Some(index) = index_to_remove {
            let agent = agents.remove(index);
            agents_dict.remove(&agent.name());

            // Update indices in agents_dict
            for (_, idx) in agents_dict.iter_mut() {
                if *idx > index {
                    *idx -= 1;
                }
            }
        }

        Ok(())
    }

    async fn run(&self) -> Result<()> {
        // Default implementation - run all agents
        let agents = self.agents.lock().await;

        for agent in agents.iter() {
            agent.run().await?;
        }

        Ok(())
    }

    async fn broadcast(&self, message: String) -> Result<()> {
        let agents = self.agents.lock().await;

        for agent in agents.iter() {
            agent.send_message(message.clone()).await?;
        }

        // Add to conversation
        self.add_to_conversation(format!("[BROADCAST] {}", message))
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::{AgentConfig, BaseAgent};

    use super::*;

    #[test]
    fn test_swarm_config_default() {
        let config = SwarmConfig::default();
        assert_eq!(config.max_loops, 200);
        assert!(!config.autosave);
        assert!(!config.logging);
    }

    #[tokio::test]
    async fn test_swarm_initialization() {
        let config = SwarmConfig::default();
        let agent1 = Box::new(BaseAgent::new(AgentConfig::default())) as _;
        let agent2 = Box::new(BaseAgent::new(AgentConfig::default())) as _;

        let agents = vec![agent1, agent2];
        let swarm = BaseSwarm::new(config, agents);

        swarm.initialize().await.unwrap();

        let agents_dict = swarm.agents_dict.lock().await;
        assert_eq!(agents_dict.len(), 2);
    }
}
