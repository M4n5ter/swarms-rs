use std::path::Path;

use chrono::{DateTime, Local};
use futures::{StreamExt, stream};
use serde::Serialize;
use thiserror::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    agent::{Agent, AgentError},
    conversation::{AgentConversation, Role},
    file_persistence::{FilePersistence, FilePersistenceError},
};

#[derive(Debug, Error)]
pub enum ConcurrentWorkflowError {
    #[error("Agent error: {0}")]
    AgentError(#[from] AgentError),
    #[error("FilePersistence error: {0}")]
    FilePersistenceError(#[from] FilePersistenceError),
    #[error("Tasks or Agents are empty")]
    EmptyTasksOrAgents,
}

pub struct ConcurrentWorkflow {
    name: String,
    metadata_output_dir: String,
    description: String,
    agents: Vec<Box<dyn Agent>>,
    metadata: MetadataSchema,
    tasks: Vec<String>,
    conversation: AgentConversation,
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
            metadata: MetadataSchema::default(),
            tasks: Vec::new(),
            conversation: AgentConversation::new("Workflow".to_owned()),
        }
    }

    pub async fn run(
        &mut self,
        task: impl Into<String>,
    ) -> Result<AgentConversation, ConcurrentWorkflowError> {
        let task = task.into();

        if task.is_empty() || self.agents.is_empty() {
            return Err(ConcurrentWorkflowError::EmptyTasksOrAgents);
        }

        self.tasks.push(task.clone());
        self.conversation
            .add(Role::User("".to_owned()), task.clone())
            .await;

        let (tx, mut rx) = mpsc::channel(self.agents.len());
        let agents = &mut self.agents;
        stream::iter(agents)
            .for_each_concurrent(None, |agent| {
                let tx = tx.clone();
                let task = task.clone();
                async move {
                    let output = match Self::run_agent(agent, task.clone()).await {
                        Ok(output) => output,
                        Err(e) => {
                            tracing::error!(
                                "| workflow | Agent {} | Task {} | Error: {}",
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
                    Role::Assistant(output_schema.agent_name.clone()),
                    output_schema.output.clone(),
                )
                .await;
            agents_output_schema.push(output_schema);
        }

        let metadata = MetadataSchema {
            swarm_id: Uuid::new_v4(),
            task,
            description: self.description.clone(),
            agents_output_schema,
            timestamp: Local::now(),
        };

        self.metadata = metadata.clone();

        self.save_metadata(metadata).await?;

        Ok(self.conversation.clone())
    }

    /// Runs the workflow for a batch of tasks, executes agents concurrently for each task.
    pub async fn run_batch(&mut self, tasks: Vec<String>) -> Result<(), ConcurrentWorkflowError> {
        // TODO: `run` method of an agent needs &mut self, so an agent can not be run concurrently. upstream(rig-core) Agent doesn't implement `Clone`
        Ok(())
    }

    async fn run_agent(
        agent: &mut Box<dyn Agent>,
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

impl FilePersistence for ConcurrentWorkflow {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn metadata_dir(&self) -> Option<impl AsRef<Path>> {
        self.metadata_output_dir.clone().into()
    }

    fn artifact_dir(&self) -> Option<impl AsRef<Path>> {
        self.metadata_dir().and_then(|metadata_dir| {
            metadata_dir
                .as_ref()
                .parent()
                .map(|parent| parent.join("artifact"))
        })
    }
}
