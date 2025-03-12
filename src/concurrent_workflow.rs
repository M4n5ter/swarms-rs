use std::{
    hash::{Hash, Hasher},
    path::Path,
};

use chrono::{DateTime, Local};
use dashmap::{DashMap, DashSet};
use futures::{StreamExt, stream};
use serde::Serialize;
use thiserror::Error;
use tokio::sync::mpsc;
use twox_hash::XxHash3_64;
use uuid::Uuid;

use crate::{
    agent::{Agent, AgentError},
    conversation::{AgentConversation, AgentShortMemory, Role},
    persistence::{self, PersistenceError},
};

#[derive(Debug, Error)]
pub enum ConcurrentWorkflowError {
    #[error("Agent error: {0}")]
    AgentError(#[from] AgentError),
    #[error("FilePersistence error: {0}")]
    FilePersistenceError(#[from] PersistenceError),
    #[error("Tasks or Agents are empty")]
    EmptyTasksOrAgents,
    #[error("Task already exists")]
    TaskAlreadyExists,
    #[error("Json error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub struct ConcurrentWorkflow {
    name: String,
    metadata_output_dir: String,
    description: String,
    agents: Vec<Box<dyn Agent>>,
    metadata_map: MetadataSchemaMap,
    tasks: DashSet<String>,
    conversation: AgentShortMemory,
}

impl ConcurrentWorkflow {
    pub fn new(
        name: impl Into<String>,
        metadata_output_dir: impl Into<String>,
        description: impl Into<String>,
        agents: Vec<Box<dyn Agent>>,
    ) -> Self {
        Self {
            name: name.into(),
            metadata_output_dir: metadata_output_dir.into(),
            description: description.into(),
            agents,
            metadata_map: MetadataSchemaMap::new(),
            tasks: DashSet::new(),
            conversation: AgentShortMemory::new(),
        }
    }

    pub async fn run(
        &self,
        task: impl Into<String>,
    ) -> Result<AgentConversation, ConcurrentWorkflowError> {
        let task = task.into();

        if task.is_empty() || self.agents.is_empty() {
            return Err(ConcurrentWorkflowError::EmptyTasksOrAgents);
        }
        if !self.tasks.insert(task.clone()) {
            return Err(ConcurrentWorkflowError::TaskAlreadyExists);
        };

        self.conversation
            .add(&task, &self.name, Role::User("User".to_owned()), &task)
            .await;

        let (tx, mut rx) = mpsc::channel(self.agents.len());
        let agents = &self.agents;
        stream::iter(agents)
            .for_each_concurrent(None, |agent| {
                let tx = tx.clone();
                let task = task.clone();
                async move {
                    let output = match run_agent(agent.as_ref(), task.clone()).await {
                        Ok(output) => output,
                        Err(e) => {
                            tracing::error!(
                                "| concurrent workflow | Agent: {} | Task: {} | Error: {}",
                                agent.name(),
                                task,
                                e
                            );
                            return;
                        }
                    };
                    tx.send(output).await.unwrap();
                }
            })
            .await;
        drop(tx);

        let mut agents_output_schema = Vec::with_capacity(self.agents.len());
        while let Some(output_schema) = rx.recv().await {
            self.conversation
                .add(
                    &task,
                    &self.name,
                    Role::Assistant(output_schema.agent_name.clone()),
                    &output_schema.output,
                )
                .await;
            agents_output_schema.push(output_schema);
        }

        let metadata = MetadataSchema {
            swarm_id: Uuid::new_v4(),
            task: task.clone(),
            description: self.description.clone(),
            agents_output_schema,
            timestamp: Local::now(),
        };

        self.metadata_map.add(&task, metadata.clone());

        let mut hasher = XxHash3_64::default();
        task.hash(&mut hasher);
        let task_hash = hasher.finish();
        let metadata_path_dir = Path::new(&self.metadata_output_dir);
        let metadata_output_dir = metadata_path_dir
            .join(format!("{:x}", task_hash & 0xFFFFFFFF)) // Lower 32 bits of the hash
            .with_extension("json");
        let metadata_data = serde_json::to_string_pretty(&metadata)?;
        persistence::save_to_file(metadata_data, &metadata_output_dir).await?;

        // Safety: we know that the task exists
        Ok(self.conversation.0.get(&task).unwrap().clone())
    }

    /// Runs the workflow for a batch of tasks, executes agents concurrently for each task.
    pub async fn run_batch(
        &self,
        tasks: Vec<String>,
    ) -> Result<DashMap<Task, AgentConversation>, ConcurrentWorkflowError> {
        if tasks.is_empty() || self.agents.is_empty() {
            return Err(ConcurrentWorkflowError::EmptyTasksOrAgents);
        }

        let results = DashMap::with_capacity(tasks.len());
        let (tx, mut rx) = mpsc::channel(tasks.len());
        stream::iter(tasks)
            .for_each_concurrent(None, |task| {
                let tx = tx.clone();
                let workflow = &self;
                async move {
                    let result = workflow.run(&task).await;
                    tx.send((task, result)).await.unwrap(); // Safety: we know rx is not dropped
                }
            })
            .await;
        drop(tx);

        while let Some((task, result)) = rx.recv().await {
            match result {
                Ok(conversation) => {
                    results.insert(task, conversation);
                }
                Err(e) => {
                    tracing::error!("| workflow | Error: {}", e);
                }
            }
        }

        Ok(results)
    }
}

#[derive(Clone, Default, Serialize)]
struct MetadataSchemaMap(DashMap<Task, MetadataSchema>);
type Task = String;

impl MetadataSchemaMap {
    fn new() -> Self {
        Self(DashMap::new())
    }

    fn add(&self, task: impl Into<String>, metadata: MetadataSchema) {
        self.0.insert(task.into(), metadata);
    }
}

#[derive(Clone, Default, Serialize)]
pub struct MetadataSchema {
    swarm_id: Uuid,
    task: String,
    description: String,
    agents_output_schema: Vec<AgentOutputSchema>,
    timestamp: DateTime<Local>,
}

#[derive(Clone, Serialize)]
pub struct AgentOutputSchema {
    run_id: Uuid,
    agent_name: String,
    task: String,
    output: String,
    start: DateTime<Local>,
    end: DateTime<Local>,
    duration: i64,
}

async fn run_agent(
    agent: &dyn Agent,
    task: String,
) -> Result<AgentOutputSchema, ConcurrentWorkflowError> {
    let start = Local::now();
    let output = agent.run(task.clone()).await?;

    let end = Local::now();
    let duration = end.signed_duration_since(start).num_seconds();

    let agent_output = AgentOutputSchema {
        run_id: Uuid::new_v4(),
        agent_name: agent.name(),
        task,
        output,
        start,
        end,
        duration,
    };

    Ok(agent_output)
}
