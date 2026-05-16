use anyhow::Result;
use async_trait::async_trait;

use crate::{
    domain::AgentRunSpec,
    providers::{ProviderAdapter, ProviderCommand},
};

#[derive(Clone, Debug, Default)]
pub struct CodexProvider;

#[async_trait]
impl ProviderAdapter for CodexProvider {
    async fn command_for_run(&self, spec: &AgentRunSpec) -> Result<ProviderCommand> {
        Ok(ProviderCommand {
            program: "codex".to_string(),
            args: vec![
                "exec".to_string(),
                "--json".to_string(),
                "--skip-git-repo-check".to_string(),
                "--sandbox".to_string(),
                "workspace-write".to_string(),
                "--cd".to_string(),
                "/workspace/project".to_string(),
                spec.prompt.clone(),
            ],
        })
    }
}
