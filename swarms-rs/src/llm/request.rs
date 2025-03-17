use serde::{Deserialize, Serialize};

use super::completion::{AssistantContent, Message};

#[derive(Debug)]
pub struct CompletionRequest {
    pub prompt: Message,
    pub system_prompt: Option<String>,
    pub chat_history: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug)]
pub struct CompletionResponse<T> {
    pub choice: Vec<AssistantContent>,
    pub raw_response: T,
}
