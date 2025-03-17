use std::ops::Deref;

use serde::Serialize;

use crate::{
    conversation::{AgentShortMemory, Role},
    llm::{self, request::CompletionRequest},
};

use super::{Agent, AgentConfig, AgentError};

#[derive(Serialize)]
pub struct SwarmsAgent<M>
where
    M: crate::llm::Model,
{
    model: M,
    config: AgentConfig,
    system_prompt: Option<String>,
    short_memory: AgentShortMemory,
}

impl<M> SwarmsAgent<M>
where
    M: crate::llm::Model,
{
    pub fn new(model: M, system_prompt: impl Into<Option<String>>) -> Self {
        Self {
            model,
            system_prompt: system_prompt.into(),
            config: AgentConfig::default(),
            short_memory: AgentShortMemory::new(),
        }
    }

    pub async fn chat(
        &self,
        prompt: impl Into<String>,
        chat_history: impl Into<Vec<llm::completion::Message>>,
    ) -> Result<String, AgentError> {
        let request = CompletionRequest {
            prompt: llm::completion::Message::user(prompt),
            system_prompt: self.system_prompt.clone(),
            chat_history: chat_history.into(),
            tools: vec![],
            temperature: Some(self.config.temperature),
            max_tokens: Some(self.config.max_tokens),
        };

        let response = self.model.completion(request).await?;
        let choice = response.choice.first().ok_or(AgentError::NoChoiceFound)?;
        match ToOwned::to_owned(choice) {
            llm::completion::AssistantContent::Text(text) => Ok(text.text),
            llm::completion::AssistantContent::ToolCall(tool_call) => {
                let tool_call_id = tool_call.id;
                let tool_call = tool_call.function;

                unimplemented!("Tool call: {tool_call_id} {:?}", tool_call)
            }
        }
    }
}

impl<M> Agent for SwarmsAgent<M>
where
    M: crate::llm::Model + Send + Sync,
{
    fn run(
        &self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<String, AgentError>> + Send + '_>> {
        Box::pin(async move {
            self.short_memory
                .add(
                    &task,
                    &self.config.name,
                    Role::User(self.config.user_name.clone()),
                    &task,
                )
                .await;

            let history = self.short_memory.0.get(&task).unwrap();
            let response = self.chat(&task, history.deref()).await?;

            Ok(response)
        })
    }

    fn run_multiple_tasks(
        &mut self,
        tasks: Vec<String>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<String>, AgentError>> + Send + '_>> {
        todo!()
    }

    fn plan(
        &self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>> {
        todo!()
    }

    fn query_long_term_memory(
        &self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>> {
        todo!()
    }

    fn save_task_state(
        &self,
        task: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + '_>> {
        todo!()
    }

    fn is_response_complete(&self, response: String) -> bool {
        todo!()
    }

    fn id(&self) -> String {
        todo!()
    }

    fn name(&self) -> String {
        todo!()
    }

    fn description(&self) -> String {
        todo!()
    }
}
