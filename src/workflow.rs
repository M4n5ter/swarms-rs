use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::Step;
use crate::agent_trait::Agent;
use crate::base::{Config, Structure};
use crate::workflow_trait::Workflow;

/// Workflow configuration
#[derive(Debug, Default)]
pub struct WorkflowConfig {
    pub name: Option<String>,
    pub description: Option<String>,
    pub agents: Vec<Box<dyn Agent>>,
    pub task_pool: Vec<String>,
}

/// Base Workflow implementation
pub struct BaseWorkflow {
    config: WorkflowConfig,
    base_config: Config,
    steps: Arc<Mutex<Vec<Step>>>,
}

impl BaseWorkflow {
    pub fn new(config: WorkflowConfig) -> Self {
        Self {
            config,
            base_config: Config::default(),
            steps: Arc::new(Mutex::new(Vec::new())),
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
}

impl Structure for BaseWorkflow {
    async fn run(&self) -> Result<()> {
        // Default implementation
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

    async fn log_event(&self, _event: String) -> Result<()> {
        // TODO: Implement event logging
        Ok(())
    }
}

impl Workflow for BaseWorkflow {
    async fn run(&self) -> Result<()> {
        // Execute all steps in sequence
        let steps = self.steps.lock().await;
        for step in steps.iter() {
            step()?;
        }
        Ok(())
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

    #[test]
    fn test_workflow_config_default() {
        let config = WorkflowConfig::default();
        assert!(config.agents.is_empty());
        assert!(config.task_pool.is_empty());
    }

    #[tokio::test]
    async fn test_workflow_add_task() {
        let mut workflow = BaseWorkflow::new(WorkflowConfig::default());
        workflow.add_task("Test task".to_string()).await.unwrap();
        assert_eq!(workflow.config.task_pool.len(), 1);
    }
}
