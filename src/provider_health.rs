use serde::Serialize;
use tokio::process::Command;

use crate::{
    config::Config,
    domain::{ProviderKind, ProviderState},
    providers::runtime::{runtime_spec, ProviderRuntimeSpec},
};

#[derive(Clone, Debug, Serialize)]
pub struct ProviderDiagnostic {
    pub provider: String,
    pub model: Option<String>,
    pub level: &'static str,
    pub status: String,
    pub detail: String,
    pub next_step: Option<String>,
    pub runtime: ProviderRuntimeSpec,
}

pub async fn collect_provider_diagnostics(
    config: &Config,
    states: &[ProviderState],
) -> Vec<ProviderDiagnostic> {
    let mut diagnostics = Vec::new();
    for provider in [
        ProviderKind::Codex,
        ProviderKind::OpenRouter,
        ProviderKind::ClaudeCode,
    ] {
        diagnostics.push(provider_diagnostic(config, states, provider).await);
    }
    diagnostics
}

async fn provider_diagnostic(
    config: &Config,
    states: &[ProviderState],
    provider: ProviderKind,
) -> ProviderDiagnostic {
    let runtime = runtime_spec(&provider, config);
    let pause = provider_pause_state(states, &runtime);
    if let Some(reason) = pause {
        return ProviderDiagnostic {
            provider: runtime.name.to_string(),
            model: runtime.default_model.map(ToOwned::to_owned),
            level: "warn",
            status: "Paused".to_string(),
            detail: reason,
            next_step: Some("Resume the provider or wait until the pause expires.".to_string()),
            runtime,
        };
    }

    match provider {
        ProviderKind::Codex => codex_diagnostic(config, runtime).await,
        ProviderKind::ClaudeCode => claude_diagnostic(config, runtime).await,
        ProviderKind::OpenRouter => ProviderDiagnostic {
            provider: runtime.name.to_string(),
            model: runtime.default_model.map(ToOwned::to_owned),
            level: "warn",
            status: "Needs secret grant".to_string(),
            detail: "OpenRouter is an API-proxy provider and needs a broker secret grant before real runs.".to_string(),
            next_step: Some("Create an OpenRouter secret grant, then run `librarian smoke mvp --provider open-router --secret-grant-token <token>`.".to_string()),
            runtime,
        },
    }
}

fn provider_pause_state(states: &[ProviderState], runtime: &ProviderRuntimeSpec) -> Option<String> {
    states
        .iter()
        .find(|state| {
            state.provider == runtime.name
                && (state.model.as_deref() == runtime.default_model || state.model.is_none())
                && state.status == "Paused"
        })
        .map(|state| {
            state
                .reason
                .clone()
                .unwrap_or_else(|| "Provider is manually paused.".to_string())
        })
}

async fn codex_diagnostic(config: &Config, runtime: ProviderRuntimeSpec) -> ProviderDiagnostic {
    let command = command_available("codex").await;
    if let Err(error) = command {
        return ProviderDiagnostic {
            provider: runtime.name.to_string(),
            model: runtime.default_model.map(ToOwned::to_owned),
            level: "error",
            status: "CLI missing".to_string(),
            detail: error,
            next_step: Some(
                "Install Codex CLI on the host and sign in with Librarian's portable CODEX_HOME."
                    .to_string(),
            ),
            runtime,
        };
    }

    profile_diagnostic(
        runtime,
        config.codex.host_home.as_deref(),
        config.codex.mount_host_home,
        &["auth.json", "config.toml", "credentials.json"],
        "Run `CODEX_HOME=<profile> codex`, then `librarian auth codex --enable-container-mount --codex-home <profile>`.",
    )
}

async fn claude_diagnostic(config: &Config, runtime: ProviderRuntimeSpec) -> ProviderDiagnostic {
    let command = command_available("claude").await;
    if let Err(error) = command {
        return ProviderDiagnostic {
            provider: runtime.name.to_string(),
            model: runtime.default_model.map(ToOwned::to_owned),
            level: "warn",
            status: "CLI missing".to_string(),
            detail: error,
            next_step: Some("Install Claude Code on the host and sign in before enabling containerized Claude jobs.".to_string()),
            runtime,
        };
    }

    profile_diagnostic(
        runtime,
        config.claude.host_home.as_deref(),
        config.claude.mount_host_home,
        &[
            ".credentials.json",
            "credentials.json",
            "settings.json",
            "config.json",
            "claude.json",
        ],
        "Run `CLAUDE_HOME=<profile> claude`, then `librarian auth claude --enable-container-mount --claude-home <profile>`.",
    )
}

fn profile_diagnostic(
    runtime: ProviderRuntimeSpec,
    host_home: Option<&std::path::Path>,
    mount_enabled: bool,
    auth_files: &[&str],
    next_step: &str,
) -> ProviderDiagnostic {
    let provider = runtime.name.to_string();
    let model = runtime.default_model.map(ToOwned::to_owned);
    let Some(path) = host_home else {
        return ProviderDiagnostic {
            provider,
            model,
            level: "warn",
            status: "Profile not configured".to_string(),
            detail: "No host profile path is configured.".to_string(),
            next_step: Some(next_step.to_string()),
            runtime,
        };
    };
    if !path.exists() {
        return ProviderDiagnostic {
            provider,
            model,
            level: if mount_enabled { "error" } else { "warn" },
            status: "Profile missing".to_string(),
            detail: format!("Configured profile does not exist: {}", path.display()),
            next_step: Some(next_step.to_string()),
            runtime,
        };
    }
    if !has_named_file_within(path, auth_files, 3) {
        return ProviderDiagnostic {
            provider,
            model,
            level: "warn",
            status: "Auth not detected".to_string(),
            detail: format!(
                "Profile exists but common auth/config files were not found: {}",
                path.display()
            ),
            next_step: Some(next_step.to_string()),
            runtime,
        };
    }
    if !mount_enabled {
        return ProviderDiagnostic {
            provider,
            model,
            level: "warn",
            status: "Mount disabled".to_string(),
            detail: format!(
                "Profile exists but container mount is disabled: {}",
                path.display()
            ),
            next_step: Some(next_step.to_string()),
            runtime,
        };
    }
    ProviderDiagnostic {
        provider,
        model,
        level: "ok",
        status: "Ready".to_string(),
        detail: format!("Profile mounted from {}", path.display()),
        next_step: None,
        runtime,
    }
}

async fn command_available(command: &str) -> Result<(), String> {
    let output = if cfg!(windows) {
        Command::new("where.exe").arg(command).output().await
    } else {
        Command::new("sh")
            .args(["-lc", &format!("command -v {}", shell_word(command))])
            .output()
            .await
    }
    .map_err(|error| format!("Command check failed: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = stderr
            .lines()
            .next()
            .or_else(|| stdout.lines().next())
            .unwrap_or("command not found");
        Err(message.to_string())
    }
}

fn has_named_file_within(path: &std::path::Path, names: &[&str], depth: usize) -> bool {
    if depth == 0 {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_file()
            && entry_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| names.iter().any(|candidate| candidate == &name))
        {
            return true;
        }
        if entry_path.is_dir() && has_named_file_within(&entry_path, names, depth - 1) {
            return true;
        }
    }
    false
}

fn shell_word(text: &str) -> String {
    if text
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/'))
    {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}
