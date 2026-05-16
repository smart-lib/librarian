use anyhow::Result;
use async_trait::async_trait;

use crate::{
    domain::AgentRunSpec,
    providers::{ProviderAdapter, ProviderCommand},
};

#[derive(Clone, Debug, Default)]
pub struct ClaudeCodeProvider;

#[async_trait]
impl ProviderAdapter for ClaudeCodeProvider {
    async fn command_for_run(&self, spec: &AgentRunSpec) -> Result<ProviderCommand> {
        Ok(ProviderCommand {
            program: "claude".to_string(),
            args: vec!["-p".to_string(), provider_shaped_prompt(&spec.prompt)],
        })
    }
}

fn provider_shaped_prompt(prompt: &str) -> String {
    prompt
        .replace("OpenRouter", "the configured API provider")
        .replace("openrouter", "configured API provider")
}
