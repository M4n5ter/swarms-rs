use std::{
    hash::{Hash, Hasher},
    path::Path,
};

use chrono::Local;
use dashmap::{DashMap, DashSet};
use futures::{StreamExt, future::BoxFuture, stream};
use serde::Serialize;
use thiserror::Error;
use tokio::sync::mpsc;
use twox_hash::XxHash3_64;
use uuid::Uuid;

use crate::{
    agent::{Agent, AgentError},
    conversation::{AgentConversation, AgentShortMemory, Role},
    persistence::{self, PersistenceError},
    swarm::{MetadataSchema, Swarm, SwarmError},
    utils::run_agent_with_output_schema,
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

#[derive(Default)]
pub struct ConcurrentWorkflowBuilder {
    name: String,
    description: String,
    metadata_output_dir: String,
    agents: Vec<Box<dyn Agent>>,
}

impl ConcurrentWorkflowBuilder {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn metadata_output_dir(mut self, dir: impl Into<String>) -> Self {
        self.metadata_output_dir = dir.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn add_agent(mut self, agent: Box<dyn Agent>) -> Self {
        self.agents.push(agent);
        self
    }

    pub fn agents(self, agents: Vec<Box<dyn Agent>>) -> Self {
        agents
            .into_iter()
            .fold(self, |builder, agent| builder.add_agent(agent))
    }

    pub fn build(self) -> ConcurrentWorkflow {
        ConcurrentWorkflow {
            name: self.name,
            metadata_output_dir: self.metadata_output_dir,
            description: self.description,
            agents: self.agents,
            ..Default::default()
        }
    }
}

#[derive(Default)]
pub struct ConcurrentWorkflow {
    name: String,
    description: String,
    metadata_map: MetadataSchemaMap,
    metadata_output_dir: String,
    tasks: DashSet<String>,
    agents: Vec<Box<dyn Agent>>,
    conversation: AgentShortMemory,
}

impl ConcurrentWorkflow {
    pub fn builder() -> ConcurrentWorkflowBuilder {
        ConcurrentWorkflowBuilder::default()
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
            .add(&task, &self.name, Role::User("User".to_owned()), &task);

        let (tx, mut rx) = mpsc::channel(self.agents.len());
        let agents = &self.agents;
        stream::iter(agents)
            .for_each_concurrent(None, |agent| {
                let tx = tx.clone();
                let task = task.clone();
                async move {
                    let output =
                        match run_agent_with_output_schema(agent.as_ref(), task.clone()).await {
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
            self.conversation.add(
                &task,
                &self.name,
                Role::Assistant(output_schema.agent_name.clone()),
                &output_schema.output,
            );
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
    ) -> Result<DashMap<String, AgentConversation>, ConcurrentWorkflowError> {
        if tasks.is_empty() || self.agents.is_empty() {
            return Err(ConcurrentWorkflowError::EmptyTasksOrAgents);
        }

        let results = DashMap::with_capacity(tasks.len());
        let (tx, mut rx) = mpsc::channel(tasks.len());
        stream::iter(tasks)
            .for_each_concurrent(None, |task| {
                let tx = tx.clone();
                let workflow = self;
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
                    tracing::error!("| concurrent workflow | Task: {} | Error: {}", task, e);
                }
            }
        }

        Ok(results)
    }
}

#[derive(Clone, Default, Serialize)]
struct MetadataSchemaMap(DashMap<String, MetadataSchema>);

impl MetadataSchemaMap {
    fn add(&self, task: impl Into<String>, metadata: MetadataSchema) {
        self.0.insert(task.into(), metadata);
    }
}

impl Swarm for ConcurrentWorkflow {
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&self, task: String) -> BoxFuture<Result<Box<dyn erased_serde::Serialize>, SwarmError>> {
        Box::pin(async move {
            self.run(task)
                .await
                .map(|output| Box::new(output) as _)
                .map_err(|e| e.into())
        })
    }
}
