use anyhow::Result;
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

use crate::agent_trait::Agent;
use crate::base::{Config, Structure};

/// MultiAgentExecutor configuration
#[derive(Debug)]
pub struct MultiAgentExecutorConfig {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub max_loops: i32,
    pub max_workers: usize,
    pub autosave: bool,
    pub logging: bool,
    pub return_metadata: bool,
    pub metadata_filename: String,
    pub rules: Option<String>,
}

impl Default for MultiAgentExecutorConfig {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: Some("MultiAgentExecutor".to_string()),
            description: None,
            max_loops: 100,
            max_workers: 5,
            autosave: false,
            logging: false,
            return_metadata: false,
            metadata_filename: "multi_agent_exec_metadata.json".to_string(),
            rules: None,
        }
    }
}

/// Execution result data structure
#[derive(Debug, Clone)]
pub struct ExecutionResult {
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

/// MultiAgentExecutor implementation
pub struct MultiAgentExecutor {
    config: MultiAgentExecutorConfig,
    base_config: Config,
    agents: Arc<Mutex<Vec<Box<dyn Agent>>>>,
    results: Arc<Mutex<Vec<ExecutionResult>>>,
    tasks: Arc<Mutex<Vec<String>>>,
}

impl MultiAgentExecutor {
    pub fn new(config: MultiAgentExecutorConfig) -> Self {
        Self {
            config,
            base_config: Config::default(),
            agents: Arc::new(Mutex::new(Vec::new())),
            results: Arc::new(Mutex::new(Vec::new())),
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Add an agent to the executor
    pub async fn add_agent(&mut self, agent: Box<dyn Agent>) -> Result<()> {
        let mut agents = self.agents.lock().await;
        agents.push(agent);
        Ok(())
    }

    /// Add a task to the executor
    pub async fn add_task(&mut self, task: String) -> Result<()> {
        let mut tasks = self.tasks.lock().await;
        tasks.push(task);
        Ok(())
    }

    /// Execute a single agent task with error handling
    async fn execute_agent_task(&self, agent: Box<dyn Agent>, task: String) -> ExecutionResult {
        let start_time = SystemTime::now();
        let task_id = Uuid::new_v4().to_string();
        let agent_id = agent.id().to_owned();
        let agent_name = agent.name();

        if self.config.logging {
            info!("Agent {} starting task {}: {}", agent_id, task_id, task);
        }

        // Send the task to the agent
        match agent.send_message(task.clone()).await {
            Ok(_) => {
                // Run the agent
                match agent.run().await {
                    Ok(_) => {
                        // Get the agent's response
                        let message = agent.receive_message().await;
                        let end_time = SystemTime::now();

                        if self.config.logging {
                            info!("Agent {} completed task {}", agent_id, task_id);
                        }

                        match message {
                            Ok(msg) => ExecutionResult {
                                agent_id,
                                agent_name,
                                task_id,
                                input: task,
                                output: Some(msg),
                                start_time,
                                end_time,
                                status: "success".to_string(),
                                error: None,
                            },
                            Err(e) => ExecutionResult {
                                agent_id,
                                agent_name,
                                task_id,
                                input: task,
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

                        if self.config.logging {
                            error!("Error in agent {} task {}: {}", agent_id, task_id, e);
                        }

                        ExecutionResult {
                            agent_id,
                            agent_name,
                            task_id,
                            input: task,
                            output: None,
                            start_time,
                            end_time,
                            status: "error".to_string(),
                            error: Some(format!("{}", e)),
                        }
                    }
                }
            }
            Err(e) => {
                let end_time = SystemTime::now();

                if self.config.logging {
                    error!(
                        "Error sending task to agent {} task {}: {}",
                        agent_id, task_id, e
                    );
                }

                ExecutionResult {
                    agent_id,
                    agent_name,
                    task_id,
                    input: task,
                    output: None,
                    start_time,
                    end_time,
                    status: "error".to_string(),
                    error: Some(format!("Failed to send message: {}", e)),
                }
            }
        }
    }

    /// Execute a task with all agents
    pub async fn execute_task(&self, task: &str) -> Result<Vec<ExecutionResult>> {
        let agents = self.agents.lock().await;

        if agents.is_empty() {
            return Err(anyhow::anyhow!("No agents provided to the executor"));
        }

        // Create futures for all agents
        let mut execution_results = Vec::new();

        // Process agents in chunks to respect max_workers
        for chunk in agents.chunks(self.config.max_workers) {
            let mut futures = Vec::new();

            for agent in chunk {
                let task_str = task.to_string();

                // Create a future for each agent in this chunk
                futures.push(self.execute_agent_task(agent.clone(), task_str));
            }

            // Execute this chunk of futures concurrently
            let chunk_results = join_all(futures).await;
            execution_results.extend(chunk_results);
        }

        // Store results
        let mut results = self.results.lock().await;
        results.extend(execution_results.clone());

        Ok(execution_results)
    }

    /// Execute all tasks with all agents
    pub async fn execute_all_tasks(&self) -> Result<Vec<ExecutionResult>> {
        let tasks = self.tasks.lock().await;

        if tasks.is_empty() {
            return Err(anyhow::anyhow!("No tasks provided to the executor"));
        }

        let mut all_results = Vec::new();

        for task in tasks.iter() {
            let results = self.execute_task(task).await?;
            all_results.extend(results);
        }

        Ok(all_results)
    }

    /// Save execution results to a file
    pub async fn save_results(&self, results: &[ExecutionResult]) -> Result<()> {
        // Convert results to a serializable format
        let mut serializable_results = Vec::new();
        for result in results {
            let serializable = serde_json::json!({
                "agent_id": result.agent_id,
                "agent_name": result.agent_name,
                "task_id": result.task_id,
                "input": result.input,
                "output": result.output,
                "start_time": result.start_time.duration_since(SystemTime::UNIX_EPOCH)?.as_secs(),
                "end_time": result.end_time.duration_since(SystemTime::UNIX_EPOCH)?.as_secs(),
                "status": result.status,
                "error": result.error
            });
            serializable_results.push(serializable);
        }

        let data = serde_json::to_vec(&serializable_results)?;
        let now = SystemTime::now();
        let since_epoch = now.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();

        let filename = format!("execution_results_{}.json", since_epoch);
        let path = self.base_config.artifact_path.join(filename);

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        self.save_to_file(&data, path).await
    }
}

impl Structure for MultiAgentExecutor {
    async fn run(&self) -> Result<()> {
        // Execute all tasks
        let results = self.execute_all_tasks().await?;

        // Save results if autosave is enabled
        if self.config.autosave {
            self.save_results(&results).await?;
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
        if self.config.logging {
            info!("[EVENT] {}", event);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentConfig;
    use crate::agent::BaseAgent;

    #[test]
    fn test_multi_agent_executor_config_default() {
        let config = MultiAgentExecutorConfig::default();
        assert_eq!(config.max_loops, 100);
        assert_eq!(config.max_workers, 5);
        assert!(!config.autosave);
        assert!(!config.logging);
    }

    #[tokio::test]
    async fn test_multi_agent_executor_add_agent() {
        let mut executor = MultiAgentExecutor::new(MultiAgentExecutorConfig::default());
        let agent = Box::new(BaseAgent::new(AgentConfig::default())) as _;
        executor.add_agent(agent).await.unwrap();

        let agents = executor.agents.lock().await;
        assert_eq!(agents.len(), 1);
    }

    #[tokio::test]
    async fn test_multi_agent_executor_add_task() {
        let mut executor = MultiAgentExecutor::new(MultiAgentExecutorConfig::default());
        executor.add_task("Test task".to_string()).await.unwrap();

        let tasks = executor.tasks.lock().await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0], "Test task");
    }
}
