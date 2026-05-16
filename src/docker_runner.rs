use anyhow::Result;
use tokio::process::Command;

use crate::{
    config::Config,
    domain::{AgentRunSpec, MountMode, NetworkMode, ProviderKind},
    providers::{
        claude_code::ClaudeCodeProvider, codex::CodexProvider, openrouter::OpenRouterProvider,
        ProviderAdapter, ProviderCommand,
    },
};

#[derive(Clone)]
pub struct DockerRunner {
    config: Config,
}

impl DockerRunner {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn docker_command_parts(&self, spec: &AgentRunSpec) -> Result<Vec<String>> {
        let provider_command = provider_command(spec).await?;
        let mut parts = vec![
            self.config.docker.runtime_command.clone(),
            "run".to_string(),
            "--rm".to_string(),
            "--name".to_string(),
            format!("librarian-{}", spec.job_id),
            "--label".to_string(),
            "librarian.managed=true".to_string(),
            "--label".to_string(),
            format!("librarian.job_id={}", spec.job_id),
            "--workdir".to_string(),
            "/workspace/project".to_string(),
        ];

        match spec.network_mode {
            NetworkMode::None => {
                parts.push("--network".to_string());
                parts.push("none".to_string());
            }
            NetworkMode::Provider | NetworkMode::Open => {}
        }

        if let Some(token) = &spec.secret_grant_token {
            parts.push("--env".to_string());
            parts.push(format!("LIBRARIAN_SECRET_GRANT_TOKEN={token}"));
            parts.push("--env".to_string());
            parts.push(format!(
                "LIBRARIAN_BROKER_URL={}",
                self.config.broker.container_url
            ));
        }

        parts.push("--mount".to_string());
        parts.push(project_mount(spec));
        parts.push(self.config.docker.agent_image.clone());
        parts.push(provider_command.program);
        parts.extend(provider_command.args);
        Ok(parts)
    }

    pub async fn cleanup_stopped_librarian_containers(&self) -> Result<CleanupReport> {
        let output = Command::new(&self.config.docker.runtime_command)
            .args([
                "container",
                "prune",
                "--force",
                "--filter",
                "label=librarian.managed=true",
            ])
            .output()
            .await?;
        Ok(CleanupReport {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct CleanupReport {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

async fn provider_command(spec: &AgentRunSpec) -> Result<ProviderCommand> {
    match spec.provider {
        ProviderKind::Codex => CodexProvider.command_for_run(spec).await,
        ProviderKind::OpenRouter => OpenRouterProvider.command_for_run(spec).await,
        ProviderKind::ClaudeCode => ClaudeCodeProvider.command_for_run(spec).await,
    }
}

fn project_mount(spec: &AgentRunSpec) -> String {
    let readonly = match spec.mount_mode {
        MountMode::ReadOnly => ",readonly",
        MountMode::ReadWrite => "",
    };
    format!(
        "type=bind,source={},target=/workspace/project{}",
        spec.project_path.display(),
        readonly
    )
}
