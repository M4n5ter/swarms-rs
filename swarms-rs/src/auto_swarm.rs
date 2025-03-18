use dashmap::DashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarms_macro::tool;
use thiserror::Error;

use crate::{
    self as swarms_rs,
    agent::{
        Agent,
        swarms_agent::{SwarmsAgent, SwarmsAgentBuilder},
    },
    llm,
    swarm::Swarm,
};

pub struct AutoSwarm<M>
where
    M: llm::Model + Clone + Send + Sync + 'static,
    M::RawCompletionResponse: Clone + Send + Sync,
{
    name: String,
    boss: SwarmsAgent<M>,
    swarms: DashMap<String, Box<dyn Swarm>>,
}

impl<M> AutoSwarm<M>
where
    M: llm::Model + Clone + Send + Sync + 'static,
    M::RawCompletionResponse: Clone + Send + Sync,
{
    pub fn new(boss: SwarmsAgent<M>, swarms: Vec<Box<dyn Swarm>>) -> Self {
        let boss = boss.system_prompt(BOSS_PROMPT).tool(CreateAgents);
        let swarms = swarms
            .into_iter()
            .map(|swarm| (swarm.name().to_owned(), swarm))
            .collect::<DashMap<_, _>>();
        Self {
            name: "AutoSwarm".to_owned(),
            boss,
            swarms,
        }
    }

    pub fn run(&self, task: impl Into<String>) -> Result<(), AutoSwarmError> {
        let task: String = task.into();

        if task.is_empty() {
            return Err(AutoSwarmError::EmptyTask);
        }

        // let agents = Self::create_agents(&task);

        Ok(())
    }

    fn create_agents(request: &str, model: M) -> Option<Vec<Box<dyn Agent>>> {
        let request = match serde_json::from_str::<CreateAgentsRequest>(request) {
            Ok(req) => req,
            Err(_) => {
                return None;
            }
        };

        let agents = request
            .agents
            .into_iter()
            .map(|agent| {
                SwarmsAgentBuilder::new_with_model(model.clone())
                    .agent_name(agent.agent_name)
                    .description(agent.agent_description)
                    .system_prompt(agent.agent_system_prompt)
                    .build()
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|a| Box::new(a) as _)
            .collect::<Vec<_>>();

        Some(agents)
    }
}

#[derive(Debug, Error)]
pub enum AutoSwarmError {
    #[error("Empty task")]
    EmptyTask,
    #[error("JSON parsing error: {0}")]
    JsonParseError(#[from] serde_json::Error),
}

#[tool]
fn create_agents(create_agents_request: CreateAgentsRequest) -> Result<String, AutoSwarmError> {
    Ok(serde_json::to_string(&create_agents_request)?)
}

/// The request to create new agents.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateAgentsRequest {
    agents: Vec<AgentToCreate>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentToCreate {
    /// The name of the agent to create.
    agent_name: String,
    /// The description of the agent to create.
    agent_description: String,
    /// The system prompt of the agent to create.
    agent_system_prompt: String,
}

const BOSS_PROMPT: &str = r#"
Manage a swarm of worker agents to efficiently serve the user by deciding whether to create new agents or delegate tasks. Ensure operations are efficient and effective.

### Instructions:

1. **Task Assignment**:
   - Analyze available worker agents when a task is presented.
   - Delegate tasks to existing agents with clear, direct, and actionable instructions if an appropriate agent is available.
   - If no suitable agent exists, create a new agent with a fitting system prompt to handle the task.

2. **Agent Creation**:
   - Name agents according to the task they are intended to perform (e.g., "Twitter Marketing Agent").
   - Provide each new agent with a concise and clear system prompt that includes its role, objectives, and any tools it can utilize.

3. **Efficiency**:
   - Minimize redundancy and maximize task completion speed.
   - Avoid unnecessary agent creation if an existing agent can fulfill the task.

4. **Communication**:
   - Be explicit in task delegation instructions to avoid ambiguity and ensure effective task execution.
   - Require agents to report back on task completion or encountered issues.

5. **Reasoning and Decisions**:
   - Offer brief reasoning when selecting or creating agents to maintain transparency.
   - Avoid using an agent if unnecessary, with a clear explanation if no agents are suitable for a task.

# Output Format

Present your plan in clear, bullet-point format or short concise paragraphs, outlining task assignment, agent creation, efficiency strategies, and communication protocols.

# Notes

- Preserve transparency by always providing reasoning for task-agent assignments and creation.
- Ensure instructions to agents are unambiguous to minimize error.

"#;
