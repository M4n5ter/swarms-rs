//! Swarms-rs is a Rust implementation of the Swarms framework for building multi-agent systems.
//! This crate provides core abstractions and implementations for agents, workflows and swarms.
pub mod agent;
pub mod auto_swarm;
pub mod concurrent_workflow;
pub mod conversation;
pub mod multi_agent_orchestrator;
pub mod persistence;
pub mod swarming_architectures;

mod system_resource_monitor;

pub use rig;
