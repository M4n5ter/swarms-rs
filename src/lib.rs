//! Swarms-rs is a Rust implementation of the Swarms framework for building multi-agent systems.
//! This crate provides core abstractions and implementations for agents, workflows and swarms.

#![allow(async_fn_in_trait)]
#![allow(clippy::only_used_in_recursion)]
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod agent;
pub mod async_workflow;
pub mod multi_agent_exec;
pub mod swarm;
pub mod swarming_architectures;
pub mod workflow;

pub type Step = Box<dyn Fn() -> Result<()> + Send + Sync>;

/// Core traits and structs for the base functionality
pub mod base {
    use super::*;

    /// Base trait for all swarms structures
    pub trait Structure {
        /// Run the structure
        async fn run(&self) -> Result<()>;

        /// Save data to file
        async fn save_to_file(&self, data: &[u8], path: PathBuf) -> Result<()>;

        /// Load data from file
        async fn load_from_file(&self, path: PathBuf) -> Result<Vec<u8>>;

        /// Save metadata
        async fn save_metadata(&self, metadata: HashMap<String, String>) -> Result<()>;

        /// Load metadata
        async fn load_metadata(&self) -> Result<HashMap<String, String>>;

        /// Log error
        async fn log_error(&self, error: anyhow::Error) -> Result<()>;

        /// Save artifact
        async fn save_artifact(&self, artifact: Vec<u8>) -> Result<()>;

        /// Load artifact
        async fn load_artifact(&self, path: PathBuf) -> Result<Vec<u8>>;

        /// Log event
        async fn log_event(&self, event: String) -> Result<()>;
    }

    /// Configuration for base structures
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Config {
        pub name: Option<String>,
        pub description: Option<String>,
        pub save_metadata: bool,
        pub artifact_path: PathBuf,
        pub metadata_path: PathBuf,
        pub error_path: PathBuf,
        pub workspace_dir: PathBuf,
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                name: None,
                description: None,
                save_metadata: true,
                artifact_path: PathBuf::from("./artifacts"),
                metadata_path: PathBuf::from("./metadata"),
                error_path: PathBuf::from("./errors"),
                workspace_dir: PathBuf::from("./workspace"),
            }
        }
    }

    /// Base implementation of Structure
    pub struct BaseStructure {
        pub config: Config,
    }

    impl Structure for BaseStructure {
        async fn run(&self) -> Result<()> {
            // Default implementation
            Ok(())
        }

        async fn save_to_file(&self, data: &[u8], path: PathBuf) -> Result<()> {
            tokio::fs::write(path, data).await?;
            Ok(())
        }

        async fn load_from_file(&self, path: PathBuf) -> Result<Vec<u8>> {
            Ok(tokio::fs::read(path).await?)
        }

        async fn save_metadata(&self, metadata: HashMap<String, String>) -> Result<()> {
            let data = serde_json::to_vec(&metadata)?;
            self.save_to_file(&data, self.config.metadata_path.clone())
                .await
        }

        async fn load_metadata(&self) -> Result<HashMap<String, String>> {
            let data = self
                .load_from_file(self.config.metadata_path.clone())
                .await?;
            Ok(serde_json::from_slice(&data)?)
        }

        async fn log_error(&self, error: anyhow::Error) -> Result<()> {
            let error_data = format!("{}", error);
            tokio::fs::write(self.config.error_path.join("error.log"), error_data).await?;
            Ok(())
        }

        async fn save_artifact(&self, artifact: Vec<u8>) -> Result<()> {
            self.save_to_file(&artifact, self.config.artifact_path.clone())
                .await
        }

        async fn load_artifact(&self, path: PathBuf) -> Result<Vec<u8>> {
            self.load_from_file(path).await
        }

        async fn log_event(&self, _event: String) -> Result<()> {
            // TODO: Implement event logging
            Ok(())
        }
    }
}

/// Agent related traits and implementations
pub mod agent_trait {
    use super::*;
    use std::{fmt::Debug, pin::Pin};

    /// Core agent trait
    pub trait Agent: Debug + Send + Sync {
        fn run(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
        fn send_message(
            &self,
            message: String,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
        fn receive_message(&self) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>>;

        /// Get agent ID
        fn id(&self) -> String;

        /// Get agent name
        fn name(&self) -> String;

        fn clone_box(&self) -> Box<dyn Agent>;
    }

    impl Clone for Box<dyn Agent> {
        fn clone(&self) -> Self {
            self.clone_box()
        }
    }
}

/// Swarm related traits and implementations
pub mod swarm_trait {
    use super::agent_trait::Agent;
    use super::*;

    /// Core swarm trait
    pub trait Swarm {
        /// Add an agent to the swarm
        async fn add_agent(&mut self, agent: Box<dyn Agent>) -> Result<()>;

        /// Remove an agent from the swarm
        async fn remove_agent(&mut self, agent_id: String) -> Result<()>;

        /// Run the swarm
        async fn run(&self) -> Result<()>;

        /// Broadcast a message to all agents
        async fn broadcast(&self, message: String) -> Result<()>;
    }
}

/// Workflow related traits and implementations
pub mod workflow_trait {
    use super::*;

    /// Core workflow trait
    pub trait Workflow {
        /// Run the workflow
        async fn run(&self) -> Result<()>;

        /// Add a step to the workflow
        async fn add_step(&mut self, step: Box<dyn Fn() -> Result<()> + Send + Sync>)
        -> Result<()>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = base::Config::default();
        assert!(config.save_metadata);
        assert_eq!(config.artifact_path, PathBuf::from("./artifacts"));
    }
}
