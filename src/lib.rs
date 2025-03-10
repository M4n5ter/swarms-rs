//! Swarms-rs is a Rust implementation of the Swarms framework for building multi-agent systems.
//! This crate provides core abstractions and implementations for agents, workflows and swarms.

#![allow(async_fn_in_trait)]
#![allow(clippy::only_used_in_recursion)]

pub mod agent;
pub mod conversation;
pub mod swarming_architectures;
pub mod workflow;

mod file_persistence;

pub use rig;

// /// Swarm related traits and implementations
// pub mod swarm_trait {
//     use super::agent::Agent;
//     use super::*;

//     /// Core swarm trait
//     pub trait Swarm {
//         /// Add an agent to the swarm
//         async fn add_agent(&mut self, agent: Box<dyn Agent>) -> Result<(), StructureError>;

//         /// Remove an agent from the swarm
//         async fn remove_agent(&mut self, agent_id: String) -> Result<(), StructureError>;

//         /// Run the swarm
//         async fn run(&self) -> Result<(), StructureError>;

//         /// Broadcast a message to all agents
//         async fn broadcast(&self, message: String) -> Result<(), StructureError>;
//     }
// }

// /// Workflow related traits and implementations
// pub mod workflow_trait {
//     use super::*;

//     /// Core workflow trait
//     pub trait Workflow {
//         /// Run the workflow
//         async fn run(&self) -> Result<()>;

//         /// Add a step to the workflow
//         async fn add_step(&mut self, step: Box<dyn Fn() -> Result<()> + Send + Sync>)
//         -> Result<()>;
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_config_default() {
//         let config = base::Config::default();
//         assert!(config.save_metadata);
//         assert_eq!(config.artifact_path, PathBuf::from("./artifacts"));
//     }
// }
