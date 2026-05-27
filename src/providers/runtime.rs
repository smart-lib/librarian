use std::path::PathBuf;

use serde::Serialize;

use crate::{config::Config, domain::ProviderKind, router};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProviderRuntimeSpec {
    pub provider: ProviderKind,
    pub name: &'static str,
    pub default_model: Option<&'static str>,
    pub cli_name: Option<&'static str>,
    pub profile_env: Option<&'static str>,
    pub host_home: Option<PathBuf>,
    pub container_home: Option<String>,
    pub mount_host_home: bool,
    pub mount_read_only: bool,
    pub project_instruction_file: Option<String>,
    pub needs_provider_network: bool,
}

pub fn runtime_spec(provider: &ProviderKind, config: &Config) -> ProviderRuntimeSpec {
    match provider {
        ProviderKind::Codex => ProviderRuntimeSpec {
            provider: provider.clone(),
            name: router::provider_name(provider),
            default_model: router::default_model(provider),
            cli_name: Some("codex"),
            profile_env: Some("CODEX_HOME"),
            host_home: config.codex.host_home.clone(),
            container_home: Some(config.codex.container_home.clone()),
            mount_host_home: config.codex.mount_host_home,
            mount_read_only: config.codex.mount_read_only,
            project_instruction_file: None,
            needs_provider_network: router::provider_requires_provider_network(provider),
        },
        ProviderKind::ClaudeCode => ProviderRuntimeSpec {
            provider: provider.clone(),
            name: router::provider_name(provider),
            default_model: router::default_model(provider),
            cli_name: Some("claude"),
            profile_env: Some("CLAUDE_HOME"),
            host_home: config.claude.host_home.clone(),
            container_home: Some(config.claude.container_home.clone()),
            mount_host_home: config.claude.mount_host_home,
            mount_read_only: config.claude.mount_read_only,
            project_instruction_file: Some(config.claude.instruction_file.clone()),
            needs_provider_network: router::provider_requires_provider_network(provider),
        },
        ProviderKind::OpenRouter => ProviderRuntimeSpec {
            provider: provider.clone(),
            name: router::provider_name(provider),
            default_model: router::default_model(provider),
            cli_name: None,
            profile_env: None,
            host_home: None,
            container_home: None,
            mount_host_home: false,
            mount_read_only: false,
            project_instruction_file: None,
            needs_provider_network: router::provider_requires_provider_network(provider),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::{config::Config, domain::ProviderKind};

    use super::*;

    #[test]
    fn codex_runtime_spec_describes_profile_mount() {
        let mut config = Config::load_or_default(None).expect("config");
        config.codex.mount_host_home = true;
        config.codex.container_home = "/home/agent/.codex".to_string();

        let spec = runtime_spec(&ProviderKind::Codex, &config);

        assert_eq!(spec.name, "codex");
        assert_eq!(spec.cli_name, Some("codex"));
        assert_eq!(spec.profile_env, Some("CODEX_HOME"));
        assert_eq!(spec.container_home.as_deref(), Some("/home/agent/.codex"));
        assert!(spec.mount_host_home);
        assert!(spec.needs_provider_network);
    }

    #[test]
    fn claude_runtime_spec_keeps_project_instruction_file() {
        let config = Config::load_or_default(None).expect("config");

        let spec = runtime_spec(&ProviderKind::ClaudeCode, &config);

        assert_eq!(spec.name, "claude-code");
        assert_eq!(spec.cli_name, Some("claude"));
        assert_eq!(spec.profile_env, Some("CLAUDE_HOME"));
        assert_eq!(spec.project_instruction_file.as_deref(), Some("CLAUDE.md"));
    }
}
