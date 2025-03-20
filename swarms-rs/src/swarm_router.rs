use thiserror::Error;

use crate::{
    agent::Agent,
    concurrent_workflow::ConcurrentWorkflow,
    swarm::{Swarm, SwarmError},
};

#[derive(Debug, Error)]
pub enum SwarmRouterError {
    #[error("Swarm Error: {0}")]
    SwarmError(#[from] SwarmError),
}

pub struct SwarmRouter {
    name: String,
    description: String,
    swarm: Option<Box<dyn Swarm>>,
    swarm_type: SwarmType,
    agents: Vec<Box<dyn Agent>>,
}

impl SwarmRouter {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        swarm_type: SwarmType,
        agents: Vec<Box<dyn Agent>>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            swarm: None,
            swarm_type,
            agents,
        }
    }

    pub async fn run(
        &mut self,
        task: impl Into<String>,
    ) -> Result<Box<dyn erased_serde::Serialize>, SwarmRouterError> {
        let task = task.into();
        self.swarm = Some(self.create_swarm(&task));

        let result = self.swarm.as_ref().unwrap().run(task).await?;
        Ok(result)
    }

    fn create_swarm(&self, task: &str) -> Box<dyn Swarm> {
        match self.swarm_type {
            SwarmType::ConcurrentWorkflow => Box::new(
                ConcurrentWorkflow::builder()
                    .name(&self.name)
                    .description(&self.description)
                    .agents(self.agents.clone())
                    .build(),
            ),
            // TODO: Add more swarm types
            _ => unimplemented!(),
        }
    }
}

pub enum SwarmType {
    Auto,
    AgentRearrange,
    HiearchicalSwarm,
    MixtureOfAgents,
    MajorityVoting,
    GroupChat,
    MultiAgentRouter,
    SpreadSheetSwarm,
    SequentialWorkflow,
    ConcurrentWorkflow,
}

impl Default for SwarmRouter {
    fn default() -> Self {
        Self {
            name: "SwarmRouter".to_string(),
            description: "Routes your task to the desired swarm.".to_string(),
            swarm: None,
            swarm_type: SwarmType::Auto,
            agents: Vec::new(),
        }
    }
}
