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
        let instruction_file = spec
            .instruction_files
            .first()
            .map(|file| file.filename.as_str())
            .unwrap_or("CLAUDE.md");
        let instruction_path = format!("/workspace/project/{instruction_file}");
        let command = format!(
            r#"if ! command -v claude >/dev/null 2>&1; then
  echo "LIBRARIAN_DIAGNOSTIC claude_cli_missing: claude is not installed in the agent image" >&2
  exit 127
fi
if [ -n "${{CLAUDE_HOME:-}}" ] && [ ! -d "$CLAUDE_HOME" ]; then
  echo "LIBRARIAN_DIAGNOSTIC claude_home_missing: CLAUDE_HOME=$CLAUDE_HOME is not mounted in the agent container" >&2
  exit 126
fi
if [ ! -f {instruction_path} ]; then
  echo "LIBRARIAN_DIAGNOSTIC claude_instruction_missing: {instruction_path} is not mounted" >&2
  exit 126
fi
claude -p "$(cat /workspace/run/prompt.txt)""#,
            instruction_path = shell_quote(&instruction_path)
        );
        Ok(ProviderCommand {
            program: "sh".to_string(),
            args: vec!["-lc".to_string(), command],
        })
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::domain::{AgentInstructionFile, AgentRunSpec, MountMode, NetworkMode, ProviderKind};

    fn spec(filename: &str) -> AgentRunSpec {
        AgentRunSpec {
            job_id: Uuid::new_v4(),
            project_path: ".".into(),
            provider: ProviderKind::ClaudeCode,
            goal: "Say hello".to_string(),
            prompt: "Say hello".to_string(),
            instruction_files: vec![AgentInstructionFile {
                filename: filename.to_string(),
                content: "You are Claude.".to_string(),
            }],
            mount_mode: MountMode::ReadOnly,
            network_mode: NetworkMode::Provider,
            secret_grant_token: None,
        }
    }

    #[tokio::test]
    async fn claude_command_checks_configured_instruction_file() {
        let command = ClaudeCodeProvider
            .command_for_run(&spec("PROJECT_CLAUDE.md"))
            .await
            .expect("command");
        let script = command.args.join(" ");
        assert!(script.contains("/workspace/project/PROJECT_CLAUDE.md"));
        assert!(script.contains("claude -p"));
    }
}
