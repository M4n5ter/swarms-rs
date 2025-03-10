use thiserror::Error;

use crate::{
    agent::{Agent, AgentError},
    conversation::SwarmConversation,
};

#[derive(Debug, Error)]
pub enum SwarmError {
    #[error("Empty tasks lists or agents lists")]
    EmptyTasksOrAgents,
    #[error("Agent error: {0}")]
    AgentError(#[from] AgentError),
    #[error("Agents can't form a perfect square grid")]
    CanNotFormAPerfectSquareGrid,
}

pub enum SwarmResult {
    Responses(Vec<String>),
    FullHistory(SwarmConversation),
}

pub async fn circular_swarm(
    agents: Vec<Vec<Box<dyn Agent>>>,
    tasks: Vec<String>,
    return_full_history: bool,
) -> Result<SwarmResult, SwarmError> {
    let mut flat_agents = agents.into_iter().flatten().collect::<Vec<_>>();
    if flat_agents.is_empty() || tasks.is_empty() {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut conversation = SwarmConversation::new();
    let mut responses = Vec::new();
    for task in &tasks {
        for agent in &mut flat_agents {
            let response = agent.run(task.to_owned()).await?;
            conversation.add_log(agent.name(), task.to_owned(), response.clone());
            responses.push(response);
        }
    }

    if return_full_history {
        Ok(SwarmResult::FullHistory(conversation))
    } else {
        Ok(SwarmResult::Responses(responses))
    }
}

pub async fn grid_swarm(
    mut agents: Vec<Box<dyn Agent>>,
    mut tasks: Vec<String>,
) -> Result<SwarmConversation, SwarmError> {
    let mut conversation = SwarmConversation::new();

    let grid_size = (agents.len() as f64).sqrt();
    if grid_size.fract() == 0.0 {
        let grid_size = grid_size as u64;
        for i in 0..grid_size {
            for j in 0..grid_size {
                if let Some(task) = tasks.pop() {
                    let index = i * grid_size + j;
                    if let Some(agent) = agents.get_mut(index as usize) {
                        let response = agent.run(task.clone()).await.unwrap();
                        conversation.add_log(agent.name(), task, response);
                    }
                }
            }
        }
    } else {
        return Err(SwarmError::CanNotFormAPerfectSquareGrid);
    }

    Ok(conversation)
}
