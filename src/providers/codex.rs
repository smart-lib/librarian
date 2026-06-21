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
    async fn command_for_run(&self, _spec: &AgentRunSpec) -> Result<ProviderCommand> {
        let command = format!(
            r#"if ! command -v codex >/dev/null 2>&1; then
  echo "LIBRARIAN_DIAGNOSTIC codex_cli_missing: codex is not installed in the agent image" >&2
  exit 127
fi
if [ -n "${{CODEX_HOME:-}}" ] && [ ! -d "$CODEX_HOME" ]; then
  echo "LIBRARIAN_DIAGNOSTIC codex_home_missing: CODEX_HOME=$CODEX_HOME is not mounted in the agent container" >&2
  exit 126
fi
codex exec --json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox --cd /workspace/project --output-last-message /workspace/run/last-message.txt - < /workspace/run/prompt.txt"#
        );
        Ok(ProviderCommand {
            program: "sh".to_string(),
            args: vec!["-lc".to_string(), command],
        })
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::domain::{AgentRunSpec, MountMode, NetworkMode, ProviderKind};

    fn spec(mount_mode: MountMode) -> AgentRunSpec {
        AgentRunSpec {
            job_id: Uuid::new_v4(),
            project_path: ".".into(),
            provider: ProviderKind::Codex,
            goal: "Say hello".to_string(),
            prompt: "Say hello".to_string(),
            instruction_files: Vec::new(),
            mount_mode,
            network_mode: NetworkMode::None,
            secret_grant_token: None,
            git_grant_token: None,
        }
    }

    #[tokio::test]
    async fn codex_command_bypasses_nested_sandbox_inside_docker_boundary() {
        for mount_mode in [MountMode::ReadOnly, MountMode::ReadWrite] {
            let command = CodexProvider
                .command_for_run(&spec(mount_mode))
                .await
                .expect("command");
            let script = command.args.join(" ");

            assert!(script.contains("--dangerously-bypass-approvals-and-sandbox"));
            assert!(!script.contains("--sandbox danger-full-access"));
            assert!(!script.contains("--sandbox read-only"));
            assert!(!script.contains("--sandbox workspace-write"));
        }
    }
}
