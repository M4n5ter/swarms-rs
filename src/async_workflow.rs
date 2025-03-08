use anyhow::Result;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

use crate::Step;
use crate::agent_trait::Agent;
use crate::base::{Config, Structure};
use crate::workflow_trait::Workflow;

/// AsyncWorkflow configuration
#[derive(Debug)]
pub struct AsyncWorkflowConfig {
    pub name: Option<String>,
    pub description: Option<String>,
    pub agents: Vec<Box<dyn Agent>>,
    pub task_pool: Vec<String>,
    pub max_workers: usize,
    pub autosave: bool,
    pub verbose: bool,
}

impl Default for AsyncWorkflowConfig {
    fn default() -> Self {
        Self {
            name: Some("AsyncWorkflow".to_string()),
            description: None,
            agents: Vec::new(),
            task_pool: Vec::new(),
            max_workers: 5,
            autosave: false,
            verbose: false,
        }
    }
}

/// Agent output data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub agent_id: String,
    pub agent_name: String,
    pub task_id: String,
    pub input: String,
    pub output: Option<String>,
    pub start_time: SystemTime,
    pub end_time: SystemTime,
    pub status: String,
    pub error: Option<String>,
}

/// Workflow output data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOutput {
    pub workflow_id: String,
    pub workflow_name: String,
    pub start_time: SystemTime,
    pub end_time: SystemTime,
    pub total_agents: usize,
    pub successful_tasks: usize,
    pub failed_tasks: usize,
    pub agent_outputs: Vec<AgentOutput>,
    pub metadata: HashMap<String, String>,
}

/// AsyncWorkflow implementation
pub struct AsyncWorkflow {
    config: AsyncWorkflowConfig,
    base_config: Config,
    workflow_id: String,
    steps: Arc<Mutex<Vec<Step>>>,
    results: Arc<Mutex<Vec<AgentOutput>>>,
}

impl AsyncWorkflow {
    pub fn new(config: AsyncWorkflowConfig) -> Self {
        Self {
            config,
            base_config: Config::default(),
            workflow_id: Uuid::new_v4().to_string(),
            steps: Arc::new(Mutex::new(Vec::new())),
            results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn add_task(&mut self, task: String) -> Result<()> {
        self.config.task_pool.push(task);
        Ok(())
    }

    pub async fn add_agent(&mut self, agent: Box<dyn Agent>) -> Result<()> {
        self.config.agents.push(agent);
        Ok(())
    }

    /// Execute a single agent task with error handling
    async fn execute_agent_task(&self, agent: Box<dyn Agent>, task: String) -> AgentOutput {
        let start_time = SystemTime::now();
        let task_id = Uuid::new_v4().to_string();
        let agent_id_str = agent_id(&*agent).await;

        if self.config.verbose {
            info!("Agent {} starting task {}: {}", agent_id_str, task_id, task);
        }

        match agent.run().await {
            Ok(_) => {
                let message = agent.receive_message().await;
                let end_time = SystemTime::now();
                let agent_name_str = agent_name(&*agent).await;

                if self.config.verbose {
                    info!("Agent {} completed task {}", agent_id_str, task_id);
                }

                match message {
                    Ok(msg) => AgentOutput {
                        agent_id: agent_id_str,
                        agent_name: agent_name_str,
                        task_id,
                        input: task.to_string(),
                        output: Some(msg),
                        start_time,
                        end_time,
                        status: "success".to_string(),
                        error: None,
                    },
                    Err(e) => AgentOutput {
                        agent_id: agent_id_str.clone(),
                        agent_name: agent_name_str,
                        task_id,
                        input: task.to_string(),
                        output: None,
                        start_time,
                        end_time,
                        status: "error".to_string(),
                        error: Some(format!("Failed to receive message: {}", e)),
                    },
                }
            }
            Err(e) => {
                let end_time = SystemTime::now();
                let agent_name_str = agent_name(&*agent).await;

                if self.config.verbose {
                    error!("Error in agent {} task {}: {}", agent_id_str, task_id, e);
                }

                AgentOutput {
                    agent_id: agent_id_str,
                    agent_name: agent_name_str,
                    task_id,
                    input: task.to_string(),
                    output: None,
                    start_time,
                    end_time,
                    status: "error".to_string(),
                    error: Some(format!("{}", e)),
                }
            }
        }
    }

    /// Run the workflow with a specific task
    pub async fn run_with_task(&self, task: &str) -> Result<WorkflowOutput> {
        if self.config.agents.is_empty() {
            return Err(anyhow::anyhow!("No agents provided to the workflow"));
        }

        let start_time = SystemTime::now();

        // Send the task to all agents
        for agent in &self.config.agents {
            agent.send_message(task.to_string()).await?;
        }

        // Create futures for all agents
        let mut agent_outputs = Vec::new();

        // Process agents in chunks to respect max_workers
        for chunk in self.config.agents.chunks(self.config.max_workers) {
            let mut futures = Vec::new();

            for agent in chunk {
                let task_str = task.to_string();

                // Create a future for each agent in this chunk
                futures.push(self.execute_agent_task(agent.clone(), task_str));
            }

            // Execute this chunk of futures concurrently
            let chunk_results = join_all(futures).await;
            agent_outputs.extend(chunk_results);
        }

        // Store results
        let mut results = self.results.lock().await;
        results.extend(agent_outputs.clone());

        let end_time = SystemTime::now();

        // Calculate success/failure counts
        let successful_tasks = agent_outputs
            .iter()
            .filter(|output| output.status == "success")
            .count();
        let failed_tasks = agent_outputs.len() - successful_tasks;

        // Create workflow output
        let output = WorkflowOutput {
            workflow_id: self.workflow_id.clone(),
            workflow_name: self
                .config
                .name
                .clone()
                .unwrap_or_else(|| "AsyncWorkflow".to_string()),
            start_time,
            end_time,
            total_agents: self.config.agents.len(),
            successful_tasks,
            failed_tasks,
            agent_outputs,
            metadata: HashMap::new(),
        };

        // Save results if autosave is enabled
        if self.config.autosave {
            self.save_workflow_output(&output).await?;
        }

        Ok(output)
    }

    /// Save workflow output to a file
    async fn save_workflow_output(&self, output: &WorkflowOutput) -> Result<()> {
        let data = serde_json::to_vec(&output)?;
        let now = SystemTime::now();
        let since_epoch = now.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();

        let filename = format!("workflow_{}_{}.json", output.workflow_id, since_epoch);
        let path = self.base_config.artifact_path.join(filename);

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        self.save_to_file(&data, path).await
    }
}

// Helper functions to get agent information
async fn agent_id(agent: &dyn Agent) -> String {
    // Use the id method from the Agent trait
    agent.id()
}

async fn agent_name(agent: &dyn Agent) -> String {
    // Use the name method from the Agent trait
    agent.name()
}

impl Structure for AsyncWorkflow {
    async fn run(&self) -> Result<()> {
        // Default implementation - run all tasks in the task pool
        if !self.config.task_pool.is_empty() {
            for task in &self.config.task_pool {
                self.run_with_task(task).await?;
            }
        }
        Ok(())
    }

    async fn save_to_file(&self, data: &[u8], path: std::path::PathBuf) -> Result<()> {
        tokio::fs::write(path, data).await?;
        Ok(())
    }

    async fn load_from_file(&self, path: std::path::PathBuf) -> Result<Vec<u8>> {
        Ok(tokio::fs::read(path).await?)
    }

    async fn save_metadata(&self, metadata: HashMap<String, String>) -> Result<()> {
        let data = serde_json::to_vec(&metadata)?;
        self.save_to_file(&data, self.base_config.metadata_path.clone())
            .await
    }

    async fn load_metadata(&self) -> Result<HashMap<String, String>> {
        let data = self
            .load_from_file(self.base_config.metadata_path.clone())
            .await?;
        Ok(serde_json::from_slice(&data)?)
    }

    async fn log_error(&self, error: anyhow::Error) -> Result<()> {
        let error_data = format!("{}", error);
        tokio::fs::write(self.base_config.error_path.join("error.log"), error_data).await?;
        Ok(())
    }

    async fn save_artifact(&self, artifact: Vec<u8>) -> Result<()> {
        self.save_to_file(&artifact, self.base_config.artifact_path.clone())
            .await
    }

    async fn load_artifact(&self, path: std::path::PathBuf) -> Result<Vec<u8>> {
        self.load_from_file(path).await
    }

    async fn log_event(&self, event: String) -> Result<()> {
        // TODO: Implement event logging
        if self.config.verbose {
            info!("[EVENT] {}", event);
        }
        Ok(())
    }
}

impl Workflow for AsyncWorkflow {
    async fn run(&self) -> Result<()> {
        // Run the workflow using the Structure implementation
        <Self as Structure>::run(self).await
    }

    async fn add_step(&mut self, step: Box<dyn Fn() -> Result<()> + Send + Sync>) -> Result<()> {
        let mut steps = self.steps.lock().await;
        steps.push(step);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentConfig;
    use crate::agent::BaseAgent;

    #[test]
    fn test_async_workflow_config_default() {
        let config = AsyncWorkflowConfig::default();
        assert_eq!(config.max_workers, 5);
        assert!(!config.autosave);
        assert!(!config.verbose);
        assert!(config.agents.is_empty());
        assert!(config.task_pool.is_empty());
    }

    #[tokio::test]
    async fn test_async_workflow_add_task() {
        let mut workflow = AsyncWorkflow::new(AsyncWorkflowConfig::default());
        workflow.add_task("Test task".to_string()).await.unwrap();
        assert_eq!(workflow.config.task_pool.len(), 1);
    }

    #[tokio::test]
    async fn test_async_workflow_add_agent() {
        let mut workflow = AsyncWorkflow::new(AsyncWorkflowConfig::default());
        let agent = Box::new(BaseAgent::new(AgentConfig::default())) as _;
        workflow.add_agent(agent).await.unwrap();
        assert_eq!(workflow.config.agents.len(), 1);
    }
}
