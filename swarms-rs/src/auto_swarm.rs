use std::fmt::{Display, Formatter};

use dashmap::DashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarms_macro::tool;
use thiserror::Error;

use crate::{
    self as swarms_rs,
    agent::{
        Agent, AgentError,
        swarms_agent::{SwarmsAgent, SwarmsAgentBuilder},
    },
    llm,
    swarm_router::{SwarmRouter, SwarmRouterError, SwarmType},
};

pub struct AutoSwarm<M>
where
    M: llm::Model + Clone + Send + Sync + 'static,
    M::RawCompletionResponse: Clone + Send + Sync,
{
    name: String,
    description: String,
    boss: SwarmsAgent<M>,
    agents_model: M,
    existing_agents: DashMap<String, Box<dyn Agent>>,
    existing_agents_info: Vec<AgentInfo>,
}

impl<M> AutoSwarm<M>
where
    M: llm::Model + Clone + Send + Sync + 'static,
    M::RawCompletionResponse: Clone + Send + Sync,
{
    pub fn new<S: Into<String>>(
        swarm_name: S,
        description: S,
        boss: SwarmsAgent<M>,
        agents_model: M,
    ) -> Self {
        let boss = boss
            .system_prompt(BOSS_PROMPT)
            .tool(SelectAgents)
            .tool(CreateAgents);

        Self {
            name: swarm_name.into(),
            description: description.into(),
            boss,
            agents_model,
            existing_agents: DashMap::new(),
            existing_agents_info: Vec::new(),
        }
    }

    pub async fn run(
        &self,
        task: impl Into<String>,
    ) -> Result<Box<dyn erased_serde::Serialize>, AutoSwarmError> {
        let task: String = task.into();

        if task.is_empty() {
            return Err(AutoSwarmError::EmptyTask);
        }

        let existing_agents = self
            .existing_agents_info
            .iter()
            .fold(String::new(), |s, agent_info| format!("{s}\n{agent_info}"));

        let prompt = format!("### Existing Agents:\n{existing_agents}\n\n### Task:\n{task}");

        let boss_resp = self.boss.run(prompt).await?;
        if let Ok(SelectAgentsRequest { agents }) = serde_json::from_str(&boss_resp) {
            let agents = agents
                .into_iter()
                .filter(|agent| self.existing_agents.contains_key(agent))
                .map(|agent| self.existing_agents.get(&agent).unwrap().clone()) // Safety: We have already checked the agent exists.
                .collect::<Vec<_>>();
            return self.swarm_router(task, agents).await;
        }

        if let Ok(request) = serde_json::from_str(&boss_resp) {
            let agents = self.create_agents(request, self.agents_model.clone())?;
            return self.swarm_router(task, agents).await;
        }

        Err(AutoSwarmError::UnknownBossBehavior(
            "Boss neither creates nor selects Agents.".to_owned(),
        ))
    }

    fn create_agents(
        &self,
        request: CreateAgentsRequest,
        model: M,
    ) -> Result<Vec<Box<dyn Agent>>, AutoSwarmError> {
        if request.agents.is_empty() {
            return Err(AutoSwarmError::UnknownBossBehavior(
                "Boss doesn't provide agents".to_owned(),
            ));
        }

        // Safety: We have already checked the agents is not None and not empty.
        let agents = request
            .agents
            .into_iter()
            .map(|atc| {
                Box::new(
                    SwarmsAgentBuilder::new_with_model(model.clone())
                        .agent_name(atc.agent_name)
                        .description(atc.agent_description)
                        .system_prompt(atc.agent_system_prompt)
                        .build(),
                ) as _
            })
            .collect::<Vec<_>>();

        Ok(agents)
    }

    async fn swarm_router(
        &self,
        task: String,
        agents: Vec<Box<dyn Agent>>,
    ) -> Result<Box<dyn erased_serde::Serialize>, AutoSwarmError> {
        let result = SwarmRouter::new(&self.name, &self.description, SwarmType::Auto, agents)
            .run(task)
            .await?;

        Ok(result)
    }
}

#[derive(Debug, Error)]
pub enum AutoSwarmError {
    #[error("Empty task")]
    EmptyTask,
    #[error("JSON parsing error: {0}")]
    JsonParseError(#[from] serde_json::Error),
    #[error("Boss agent error: {0}")]
    BossAgentError(#[from] AgentError),
    #[error("Swarm Router Error: {0}")]
    SwarmRouterError(#[from] SwarmRouterError),
    #[error("Unknown Boss behavior: {0}")]
    UnknownBossBehavior(String),
}

#[tool(description = "
    Select a group of agents to solve the task.
    All agents will cooperate to solve the task.")]
fn select_agents(
    select_agents_request: SelectAgentsRequest,
) -> Result<SelectAgentsRequest, AutoSwarmError> {
    tracing::info!(
        "AutoSwarm boss selected: {:?}",
        select_agents_request.agents
    );
    Ok(select_agents_request)
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SelectAgentsRequest {
    /// A list of agents names. Names should in existing agents.
    agents: Vec<String>,
}

#[tool(description = "
    Create new agents, the agents created will be used to solve the task.
    If need multiple agents cooperate to solve the problem, create multiple agents.
    Agents with the same name will only keep the last one, which means that if there are already agents that meet the conditions,
    but other agents need to be added, just create a complete number of agents, and the old agent with the same name will be replaced.
    ")]
fn create_agents(
    create_agents_request: CreateAgentsRequest,
) -> Result<CreateAgentsRequest, AutoSwarmError> {
    tracing::info!(
        "AutoSwarm boss created agents: {:?}",
        create_agents_request.agents
    );
    Ok(create_agents_request)
}

/// The request to create new agents.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateAgentsRequest {
    /// A list of agents to create.
    agents: Vec<AgentInfo>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentInfo {
    /// The name of the agent to create.
    agent_name: String,
    /// The description of the agent to create.
    agent_description: String,
    /// The system prompt of the agent to create.
    agent_system_prompt: String,
}

impl Display for AgentInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "[Agent Name: {}] | Description: {} | System Prompt: {}",
            self.agent_name, self.agent_description, self.agent_system_prompt
        )
    }
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
