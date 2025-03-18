//! Swarms-rs is a Rust implementation of the Swarms framework for building multi-agent systems.
//! This crate provides core abstractions and implementations for agents, workflows and swarms.
pub mod agent;
pub mod auto_swarm;
pub mod concurrent_workflow;
pub mod llm;
pub mod multi_agent_orchestrator;
pub mod swarming_architectures;
pub mod tool;

mod conversation;
mod persistence;
mod system_resource_monitor;

pub use rig;
