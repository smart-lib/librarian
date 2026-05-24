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
    async fn command_for_run(&self, _spec: &AgentRunSpec) -> Result<ProviderCommand> {
        let command = format!(
            r#"if ! command -v claude >/dev/null 2>&1; then
  echo "LIBRARIAN_DIAGNOSTIC claude_cli_missing: claude is not installed in the agent image" >&2
  exit 127
fi
if [ -n "${{CLAUDE_HOME:-}}" ] && [ ! -d "$CLAUDE_HOME" ]; then
  echo "LIBRARIAN_DIAGNOSTIC claude_home_missing: CLAUDE_HOME=$CLAUDE_HOME is not mounted in the agent container" >&2
  exit 126
fi
if [ ! -f /workspace/project/CLAUDE.md ]; then
  echo "LIBRARIAN_DIAGNOSTIC claude_instruction_missing: /workspace/project/CLAUDE.md is not mounted" >&2
  exit 126
fi
claude -p "$(cat /workspace/run/prompt.txt)""#
        );
        Ok(ProviderCommand {
            program: "sh".to_string(),
            args: vec!["-lc".to_string(), command],
        })
    }
}
