use crate::{agent::Agent, concurrent_workflow::ConcurrentWorkflow, swarm::Swarm};

pub struct SwarmRouter {
    name: String,
    description: String,
    swarm_type: SwarmType,
    agents: Vec<Box<dyn Agent>>,
}

impl SwarmRouter {
    fn create_swarm(&self, task: &str) -> Box<dyn Swarm> {
        match self.swarm_type {
            SwarmType::ConcurrentWorkflow => {
                // let workflow = ConcurrentWorkflow::builder()
                //     .name(&self.name)
                //     .description(&self.description)
                //     .agents(&self.agents)
                //     .build();
                todo!()
            }
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
            swarm_type: SwarmType::Auto,
            agents: vec![],
        }
    }
}
