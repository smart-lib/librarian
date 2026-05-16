use anyhow::Result;
use async_trait::async_trait;

use crate::{
    domain::AgentRunSpec,
    providers::{ProviderAdapter, ProviderCommand},
};

#[derive(Clone, Debug, Default)]
pub struct OpenRouterProvider;

#[async_trait]
impl ProviderAdapter for OpenRouterProvider {
    async fn command_for_run(&self, spec: &AgentRunSpec) -> Result<ProviderCommand> {
        Ok(ProviderCommand {
            program: "sh".to_string(),
            args: vec![
                "-lc".to_string(),
                format!(
                    "printf '%s\n' {} && test -n \"$LIBRARIAN_SECRET_GRANT_TOKEN\" && curl -sS \"$LIBRARIAN_BROKER_URL/v1/proxy/openrouter/api/v1/chat/completions\" -H \"x-librarian-grant-token: $LIBRARIAN_SECRET_GRANT_TOKEN\" -H 'content-type: application/json' --data @- <<'JSON'\n{{\"model\":\"openrouter-default\",\"messages\":[{{\"role\":\"user\",\"content\":{}}}]}}\nJSON",
                    shell_quote("OpenRouter provider adapter is API-proxy based."),
                    serde_json::to_string(&spec.prompt)?
                ),
            ],
        })
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
