use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::llm::request::ToolDefinition;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Error returned by the tool
    #[error("ToolCallError: {0}")]
    ToolCallError(#[from] Box<dyn core::error::Error + Send + Sync>),

    #[error("JsonError: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub trait Tool: Sized + Send + Sync {
    type Error: core::error::Error + Send + Sync + 'static;
    type Args: for<'a> Deserialize<'a> + Send + Sync;
    type Output: Serialize;

    const NAME: &'static str;

    // Required methods
    fn definition(&self) -> ToolDefinition;
    fn call(
        &self,
        args: Self::Args,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + Sync;

    // Provided method
    fn name(&self) -> String {
        Self::NAME.to_string()
    }
}

pub trait ToolDyn: Send + Sync {
    fn name(&self) -> String;

    fn definition(&self) -> ToolDefinition;

    fn call(
        &self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + Sync + '_>>;
}

impl<T: Tool> ToolDyn for T {
    fn name(&self) -> String {
        self.name()
    }

    fn definition(&self) -> ToolDefinition {
        <Self as Tool>::definition(self)
    }

    fn call(
        &self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + Sync + '_>> {
        Box::pin(async move {
            match serde_json::from_str(&args) {
                Ok(args) => <Self as Tool>::call(self, args)
                    .await
                    .map_err(|e| ToolError::ToolCallError(Box::new(e)))
                    .and_then(|output| {
                        serde_json::to_string(&output).map_err(ToolError::JsonError)
                    }),
                Err(e) => Err(ToolError::JsonError(e)),
            }
        })
    }
}
