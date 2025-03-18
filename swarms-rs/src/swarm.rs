use erased_serde::Serialize;
use futures::future::BoxFuture;
use thiserror::Error;

use crate::concurrent_workflow::ConcurrentWorkflowError;

pub trait Swarm {
    fn name(&self) -> &str;

    fn run(&self, task: String) -> BoxFuture<Result<Box<dyn Serialize>, SwarmError>>;
}

#[derive(Debug, Error)]
pub enum SwarmError {
    #[error("ConcurrentWorkflowError: {0}")]
    ConcurrentWorkflowError(#[from] ConcurrentWorkflowError),
}
