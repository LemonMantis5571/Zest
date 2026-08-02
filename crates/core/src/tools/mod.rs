pub mod delegate;
pub mod read_file;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::anthropic::types::ToolDef;

/// A client-side tool.
///
/// `run` returns `Result<String, String>` rather than a harness error type on
/// purpose: a tool failing is a normal conversational event, not a harness
/// failure. The `Err` string goes back to the model as a `tool_result` with
/// `is_error: true` so it can adapt, rather than aborting the turn.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    async fn run(&self, input: Value) -> std::result::Result<String, String>;
}

/// Cloning shares the tools themselves — cheap, and how a delegated worker gets
/// its own registry without rebuilding anything.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.push(tool);
    }

    /// Stable order — the tool list renders at the very front of the prompt, so
    /// reordering it invalidates the entire prompt cache.
    pub fn definitions(&self) -> Vec<ToolDef> {
        self.tools
            .iter()
            .map(|t| ToolDef {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.input_schema(),
            })
            .collect()
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.iter().map(|t| t.name()).collect()
    }

    pub async fn run(&self, name: &str, input: Value) -> std::result::Result<String, String> {
        match self.tools.iter().find(|t| t.name() == name) {
            Some(tool) => tool.run(input).await,
            None => Err(format!("unknown tool: {name}")),
        }
    }
}
