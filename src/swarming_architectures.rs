use futures::{StreamExt, stream};
use thiserror::Error;
use tokio::sync::mpsc;

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

/// All agents process each task in a circular manner, each agent process each task.
pub async fn circular_swarm(
    mut agents: Vec<Box<dyn Agent>>,
    tasks: Vec<String>,
    return_full_history: bool,
) -> Result<SwarmResult, SwarmError> {
    if agents.is_empty() || tasks.is_empty() || tasks.iter().all(|task| task.is_empty()) {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    // TODO: maybe need concurrency? Now it's sequential, because the `run` method needs a mutable reference to the agent
    let mut conversation = SwarmConversation::new();
    let mut responses = Vec::new();
    for task in &tasks {
        for agent in &mut agents {
            let response = match agent.run(task.to_owned()).await {
                Ok(response) => response,
                Err(e) => {
                    tracing::error!(
                        "| circular swarm | Agent {} | Task {} | Error: {}",
                        agent.name(),
                        task,
                        e
                    );
                    continue;
                }
            };
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

/// Grid Swarm: (Concurrently) Agents are arranged in a grid and process tasks in a grid-like manner, a agent process a task, then the next agent process the next task, and so on.
pub async fn grid_swarm(
    agents: Vec<Box<dyn Agent>>,
    tasks: Vec<String>,
) -> Result<SwarmConversation, SwarmError> {
    if agents.is_empty() || tasks.is_empty() || tasks.iter().all(|task| task.is_empty()) {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut conversation = SwarmConversation::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(agents.len());

    let grid_size = (agents.len() as f64).sqrt() as usize;
    if grid_size * grid_size != agents.len() {
        return Err(SwarmError::CanNotFormAPerfectSquareGrid);
    }

    stream::iter(agents.into_iter().enumerate())
        .for_each_concurrent(None, |(index, agent)| {
            let tx = tx.clone();
            let task = tasks.get(index).cloned();
            async move {
                if let Some(task) = task {
                    let result = agent
                        .run(task.clone())
                        .await
                        .map(|response| (agent.name(), task, response));
                    tx.send(result).await.unwrap(); // Safe: we know the rx is alive
                }
            }
        })
        .await;

    while let Some(result) = rx.recv().await {
        match result {
            Ok((agent_name, task, response)) => {
                conversation.add_log(agent_name, task, response);
            }
            Err(e) => {
                tracing::error!("Agent failed in grid swarm: {}", e);
            }
        }
    }

    Ok(conversation)
}

/// Linear Swarm: Agents process tasks in a sequential linear manner, a agent process a task, then the next agent process the next task, and so on.
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

    for agent in agents {
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
    sender: impl Agent,
    receiver: impl Agent,
    task: impl Into<String>,
    max_loops: u32,
) -> Result<SwarmConversation, SwarmError> {
    let task = task.into();
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

/// (Concurrently) Sender agent processes the task and then sends the result to all receivers agent.
pub async fn one_to_three(
    sender: impl Agent,
    receivers: [Box<dyn Agent>; 3],
    task: impl Into<String>,
) -> Result<SwarmConversation, SwarmError> {
    let task = task.into();
    if task.is_empty() {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut conversation = SwarmConversation::new();
    let sender_message = sender.run(task.clone()).await?;
    conversation.add_log(sender.name(), task, sender_message.clone());

    let (tx, mut rx) = mpsc::channel(3);
    stream::iter(receivers)
        .for_each_concurrent(None, |receiver| {
            let task = sender_message.clone();
            let tx = tx.clone();
            async move {
                let result = receiver
                    .run(task.clone())
                    .await
                    .map(|response| (receiver.name(), task, response));
                tx.send(result).await.unwrap(); // Safe: we know the receiver is alive
            }
        })
        .await;

    while let Some(result) = rx.recv().await {
        match result {
            Ok((agent_name, task, response)) => conversation.add_log(agent_name, task, response),
            Err(e) => tracing::error!("Receiver agent failed in one to three swarm: {}", e),
        }
    }

    Ok(conversation)
}

/// (Concurrently) Sender agent processes the task and then broadcasts the result to all receiver agents.
pub async fn broadcast(
    sender: impl Agent,
    receivers: Vec<Box<dyn Agent>>,
    task: impl Into<String>,
) -> Result<SwarmConversation, SwarmError> {
    let task = task.into();
    if receivers.is_empty() || task.is_empty() {
        return Err(SwarmError::EmptyTasksOrAgents);
    }

    let mut conversation = SwarmConversation::new();

    // First get the sender's boardcast response
    let broadcast_response = sender.run(task.clone()).await?;
    conversation.add_log(sender.name(), task.clone(), broadcast_response);

    // Then have all agents process it
    let (tx, mut rx) = mpsc::channel(receivers.len());

    // TODO: tokio::spawn is needed ?
    // tokio::spawn(async move {
    stream::iter(receivers)
        .for_each_concurrent(None, |receiver| {
            let task = task.clone();
            let tx = tx.clone();
            async move {
                let result = receiver
                    .run(task.clone())
                    .await
                    .map(|response| (receiver.name(), task, response));
                tx.send(result).await.unwrap(); // Safe: we know the receiver is alive
            }
        })
        .await;
    // });

    while let Some(result) = rx.recv().await {
        match result {
            Ok((agent_name, task, response)) => conversation.add_log(agent_name, task, response),
            Err(e) => tracing::error!("Receiver agent failed in boardcast swarm: {}", e),
        }
    }

    Ok(conversation)
}
