use serde::Deserialize;

use crate::agent::AgentConfig;

#[derive(Deserialize, Debug)]
pub struct GraphWorkflowConfig {
    pub name: String,
    pub description: String,
    pub default_model: Option<String>,
    pub agents: Vec<AgentConfig>,
    pub connections: Vec<ConnectionConfig>,
}

#[derive(Deserialize, Debug)]
pub struct ConnectionConfig {
    pub from: String,
    pub to: String,
    pub condition: Option<String>,
    pub transform: Option<String>,
}
