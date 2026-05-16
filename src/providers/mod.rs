pub mod claude_code;
pub mod codex;
pub mod openrouter;

use anyhow::Result;
use async_trait::async_trait;

use crate::domain::AgentRunSpec;

#[derive(Clone, Debug)]
pub struct ProviderCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    async fn command_for_run(&self, spec: &AgentRunSpec) -> Result<ProviderCommand>;
}
