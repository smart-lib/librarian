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
        let sandbox = match spec.mount_mode {
            crate::domain::MountMode::ReadOnly => "read-only",
            crate::domain::MountMode::ReadWrite => "workspace-write",
        };
        let command = format!(
            "codex exec --json --skip-git-repo-check --sandbox {sandbox} --cd /workspace/project --output-last-message /workspace/run/last-message.txt - < /workspace/run/prompt.txt"
        );
        Ok(ProviderCommand {
            program: "sh".to_string(),
            args: vec!["-lc".to_string(), command],
        })
    }
}
