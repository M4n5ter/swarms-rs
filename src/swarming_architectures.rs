use futures::{StreamExt, stream};
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

/// Implements a circular swarm where agents pass tasks in a circular manner.
pub async fn circular_swarm(
    agents: Vec<Vec<Box<dyn Agent>>>,
    tasks: Vec<String>,
    return_full_history: bool,
) -> Result<SwarmResult, SwarmError> {
    if agents.is_empty() || tasks.is_empty() || tasks.iter().all(|task| task.is_empty()) {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut flat_agents = agents.into_iter().flatten().collect::<Vec<_>>();
    if flat_agents.is_empty() || tasks.is_empty() {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    // TODO: maybe need concurrency
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
    if agents.is_empty() || tasks.is_empty() || tasks.iter().all(|task| task.is_empty()) {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut conversation = SwarmConversation::new();

    // TODO: maybe need concurrency
    let grid_size = (agents.len() as f64).sqrt();
    if grid_size.fract() == 0.0 {
        let grid_size = grid_size as u64;
        for i in 0..grid_size {
            for j in 0..grid_size {
                if let Some(task) = tasks.pop() {
                    let index = i * grid_size + j;
                    if let Some(agent) = agents.get_mut(index as usize) {
                        let response = agent.run(task.clone()).await?;
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

/// Linear Swarm: Agents process tasks in a sequential linear manner
pub async fn linear_swarm(
    agents: Vec<Box<dyn Agent>>,
    mut tasks: Vec<String>,
    return_full_history: bool,
) -> Result<SwarmResult, SwarmError> {
    if agents.is_empty() || tasks.is_empty() {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut conversation = SwarmConversation::new();
    let mut responses = Vec::new();

    for mut agent in agents {
        if let Some(task) = tasks.pop() {
            let response = agent.run(task.clone()).await?;
            conversation.add_log(agent.name(), task, response.clone());
            responses.push(response);
        };
    }

    if return_full_history {
        Ok(SwarmResult::FullHistory(conversation))
    } else {
        Ok(SwarmResult::Responses(responses))
    }
}

/// Facilitates one-to-one communication between two agents. The sender and receiver agents exchange messages for a specified number of loops.
pub async fn one_to_one(
    mut sender: impl Agent,
    mut receiver: impl Agent,
    task: String,
    max_loops: u32,
) -> Result<SwarmConversation, SwarmError> {
    if task.is_empty() {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut conversation = SwarmConversation::new();
    let mut responses = Vec::new();

    for _ in 0..max_loops {
        let task = task.clone();
        let sender_response = sender.run(task.clone()).await?;
        conversation.add_log(sender.name(), task.clone(), sender_response.clone());
        responses.push(sender_response.clone());

        let receiver_response = receiver.run(sender_response).await?;
        conversation.add_log(receiver.name(), task, receiver_response.clone());
        responses.push(receiver_response);
    }

    Ok(conversation)
}

pub async fn one_to_three(
    mut sender: impl Agent,
    receivers: [Box<dyn Agent>; 3],
    task: String,
) -> Result<SwarmConversation, SwarmError> {
    if task.is_empty() {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut conversation = SwarmConversation::new();
    let sender_message = sender.run(task.clone()).await?;
    conversation.add_log(sender.name(), task, sender_message.clone());
    let results = stream::iter(receivers)
        .then(|mut receiver| {
            let task = sender_message.clone();
            async move {
                receiver
                    .run(task.clone())
                    .await
                    .map(|response| (receiver.name(), task, response))
            }
        })
        .collect::<Vec<_>>()
        .await;

    for result in results {
        match result {
            Ok((agent_name, task, response)) => conversation.add_log(agent_name, task, response),
            Err(e) => tracing::error!("Receiver agent failed in one to three swarm: {}", e),
        }
    }

    Ok(conversation)
}

pub async fn broadcast(
    mut sender: impl Agent,
    receivers: Vec<Box<dyn Agent>>,
    task: String,
) -> Result<SwarmConversation, SwarmError> {
    if receivers.is_empty() || task.is_empty() {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut conversation = SwarmConversation::new();

    // First get the sender's boardcast response
    let broadcast_response = sender.run(task.clone()).await?;
    conversation.add_log(sender.name(), task.clone(), broadcast_response);

    // Then have all agents process it
    let results = stream::iter(receivers)
        .then(|mut receiver| {
            let task = task.clone();
            async move {
                receiver
                    .run(task.clone())
                    .await
                    .map(|response| (receiver.name(), task, response))
            }
        })
        .collect::<Vec<_>>()
        .await;

    for result in results {
        match result {
            Ok((agent_name, task, response)) => conversation.add_log(agent_name, task, response),
            Err(e) => tracing::error!("Receiver agent failed in boardcast swarm: {}", e),
        }
    }

    Ok(conversation)
}
