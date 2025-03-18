use std::collections::HashSet;

use chrono::Local;
use dashmap::DashMap;
use futures::{StreamExt, stream};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarms_macro::tool;
use thiserror::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::agent::swarms_agent::SwarmsAgent;
use crate::{self as swarms_rs, llm};
use crate::{
    agent::{Agent, AgentError},
    conversation::{AgentShortMemory, Role},
};

#[derive(Debug, Error)]
pub enum MultiAgentOrchestratorError {
    #[error("Agent's name and description must be set.")]
    NameOrDescriptionNotFound,
    #[error("Agent's name should be unique, duplicate name: {0}")]
    DuplicateAgentName(String),
    #[error("Boss agent return unexpected reply: {0}")]
    WrongBossResponse(String),
    #[error("Agent Error: {0}")]
    AgentError(#[from] AgentError),
    #[error("Failed to parse json: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Can not find the agent returned from boss")]
    AgentNotFound,
}

pub struct MultiAgentOrchestrator<M>
where
    M: llm::Model,
{
    boss: SwarmsAgent<M>,
    agents: Vec<Box<dyn Agent>>,
    router_conversation: AgentShortMemory,
    enable_execute_task: bool,
}

impl<M> MultiAgentOrchestrator<M>
where
    M: llm::Model + Send + Sync,
    M::RawCompletionResponse: Send + Sync,
{
    pub fn new(
        boss: SwarmsAgent<M>,
        agents: Vec<Box<dyn Agent>>,
        enable_execute_task: bool,
    ) -> Result<Self, MultiAgentOrchestratorError> {
        let router_conversation = AgentShortMemory::new();

        Ok(Self {
            boss: boss
                .tool(SelectAgent)
                .system_prompt(create_boss_system_prompt(&agents)?),
            agents,
            router_conversation,
            enable_execute_task,
        })
    }

    pub async fn run(
        &self,
        task: impl Into<String>,
    ) -> Result<MultiAgentOrchestratorResult, MultiAgentOrchestratorError> {
        let total_start = Local::now();

        let task = task.into();
        self.router_conversation.add(
            task.clone(),
            self.boss.name(),
            Role::User("User".to_owned()),
            task.clone(),
        );

        let boss_response_str = self.boss.run(task.clone()).await?;
        let boss_response = serde_json::from_str::<SelectAgentResponse>(boss_response_str.trim())?;

        self.router_conversation.add(
            task.clone(),
            self.boss.name(),
            Role::Assistant(self.boss.name()),
            boss_response_str,
        );

        let selected_agent = match self.find_agent_by_name(&boss_response.selected_agent) {
            Some(agent) => agent,
            None => return Err(MultiAgentOrchestratorError::AgentNotFound),
        };

        let selected_agent_name = selected_agent.name();
        let selected_agent_id = selected_agent.id();

        let final_task = boss_response.modified_task.unwrap_or(task.clone());
        let mut agent_response = None;

        let execution_start = Local::now();
        let mut execution_time = 0;
        if !self.enable_execute_task {
            tracing::info!("Task execution skipped (enable_execute_task=false)")
        } else {
            agent_response = Some(selected_agent.run(final_task.clone()).await?);
            execution_time = Local::now()
                .signed_duration_since(execution_start)
                .num_seconds();
            self.router_conversation.add(
                task.clone(),
                self.boss.name(),
                Role::Assistant(selected_agent_name.clone()),
                agent_response.clone().unwrap(), // Safety: we just make it Some
            );
        }

        let total_time = Local::now()
            .signed_duration_since(total_start)
            .num_seconds();

        Ok(MultiAgentOrchestratorResult {
            id: Uuid::new_v4(),
            timestamp: Local::now().timestamp(),
            task: Task {
                original: task.clone(),
                modified: if task != final_task {
                    Some(final_task)
                } else {
                    None
                },
            },
            boss_decision: BossDecision {
                selected_agent: selected_agent_name.clone(),
                reasoning: boss_response.reasoning,
            },
            execution: Execution {
                agent_id: selected_agent_id,
                agent_name: selected_agent_name,
                was_executed: self.enable_execute_task,
                response: agent_response,
                execution_time: if self.enable_execute_task {
                    Some(execution_time)
                } else {
                    None
                },
            },
            total_time,
        })
    }

    pub async fn run_batch(
        &self,
        tasks: Vec<String>,
    ) -> Result<DashMap<String, MultiAgentOrchestratorResult>, MultiAgentOrchestratorError> {
        let results = DashMap::with_capacity(tasks.len());

        let (tx, mut rx) = mpsc::channel(tasks.len());
        stream::iter(tasks)
            .for_each_concurrent(None, |task| {
                let tx = tx.clone();
                let orchestrator = self;
                async move {
                    let result = orchestrator.run(task.clone()).await;
                    match result {
                        Ok(result) => {
                            tx.send((task, result)).await.unwrap();
                        }
                        Err(e) => {
                            tracing::error!(
                                "| multi agent orchestrator  | Task:  {} | Error: {}",
                                task,
                                e
                            );
                        }
                    }
                }
            })
            .await;
        drop(tx);

        while let Some((task, result)) = rx.recv().await {
            results.insert(task, result);
        }

        Ok(results)
    }

    fn find_agent_by_name(&self, agent_name: &str) -> Option<&dyn Agent> {
        self.agents
            .iter()
            .find(|agent| agent.name() == agent_name)
            .map(|agent| &**agent)
    }
}

fn create_boss_system_prompt(
    agents: &Vec<Box<dyn Agent>>,
) -> Result<String, MultiAgentOrchestratorError> {
    // because we need to route, the description of each agent must be set.
    if agents
        .iter()
        .any(|agent| agent.name().is_empty() || agent.description().is_empty())
    {
        return Err(MultiAgentOrchestratorError::NameOrDescriptionNotFound);
    }

    // If two agents have the same name, return error.
    let mut set = HashSet::with_capacity(agents.len());
    for agent in agents {
        if !set.insert(agent.name()) {
            return Err(MultiAgentOrchestratorError::DuplicateAgentName(
                agent.name(),
            ));
        }
    }

    let agent_descriptions = agents
        .iter()
        .map(|agent| format!("- {}: {}\n", agent.name(), agent.description()))
        .collect::<Vec<String>>()
        .concat();

    Ok(format!(
        "You are a boss agent responsible for routing tasks to the most appropriate specialized agent.
    Available agents:
    {agent_descriptions}

    Your job is to:
    1. Analyze the incoming task
    2. Select the most appropriate agent based on their descriptions
    3. Provide clear reasoning for your selection
    4. Optionally modify the task to better suit the selected agent's capabilities

    Always select exactly one agent that best matches the task requirements.
    "
    ))
}

#[tool(description = "Select the most appropriate agent to execute the task.")]
fn select_agent(
    selected: SelectAgentResponse,
) -> Result<SelectAgentResponse, MultiAgentOrchestratorError> {
    Ok(selected)
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SelectAgentResponse {
    /// Name of the chosen agent (must be one of the available agents)
    selected_agent: String,
    /// Brief explanation of why this agent was selected
    reasoning: String,
    /// (Optional) A modified version of the task if needed
    modified_task: Option<String>,
}

#[derive(Serialize)]
pub struct MultiAgentOrchestratorResult {
    id: Uuid,
    timestamp: i64,
    task: Task,
    boss_decision: BossDecision,
    execution: Execution,
    total_time: i64,
}

#[derive(Serialize)]
pub struct Task {
    original: String,
    modified: Option<String>,
}

#[derive(Serialize)]
pub struct BossDecision {
    selected_agent: String,
    reasoning: String,
}

#[derive(Serialize)]
pub struct Execution {
    agent_id: String,
    agent_name: String,
    was_executed: bool,
    response: Option<String>,
    execution_time: Option<i64>,
}
