//! Swarms-rs is a Rust implementation of the Swarms framework for building multi-agent systems.
//! This crate provides core abstractions and implementations for agents, workflows and swarms.
pub mod agent;
pub mod concurrent_workflow;
pub mod conversation;
pub mod swarming_architectures;

mod file_persistence;

pub use rig;
