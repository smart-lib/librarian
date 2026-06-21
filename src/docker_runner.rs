use std::{fs, path::PathBuf};

use anyhow::{bail, Context, Result};
use tokio::process::Command;

use crate::{
    config::Config,
    domain::{AgentRunSpec, MountMode, NetworkMode, ProviderKind},
    providers::{
        claude_code::ClaudeCodeProvider, codex::CodexProvider, openrouter::OpenRouterProvider,
        runtime::runtime_spec, ProviderAdapter, ProviderCommand,
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
        let run_dir = self.prepare_run_dir(spec)?;
        let mut parts = runtime_prefix(&self.config);
        parts.extend([
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
        ]);

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

        let runtime = runtime_spec(&spec.provider, &self.config);
        if runtime.mount_host_home {
            let Some(host_home) = runtime.host_home.as_ref() else {
                bail!(
                    "{} profile mount is enabled but host_home is not configured",
                    runtime.name
                );
            };
            let Some(container_home) = runtime.container_home.as_deref() else {
                bail!(
                    "{} profile mount is enabled but container_home is not configured",
                    runtime.name
                );
            };
            if !host_home.exists() {
                bail!(
                    "Configured {} home does not exist: {}",
                    runtime.name,
                    host_home.display()
                );
            }
            if let Some(user) = container_user_for_host_path(host_home)? {
                write_container_identity_files(&run_dir, &user)?;
                parts.push("--user".to_string());
                parts.push(user.spec());
                parts.push("--mount".to_string());
                parts.push(identity_file_mount(
                    &self.config,
                    &run_dir,
                    "passwd",
                    "/etc/passwd",
                ));
                parts.push("--mount".to_string());
                parts.push(identity_file_mount(
                    &self.config,
                    &run_dir,
                    "group",
                    "/etc/group",
                ));
            }
            if let Some(env_name) = runtime.profile_env {
                parts.push("--env".to_string());
                parts.push(format!("{env_name}={container_home}"));
            }
            parts.push("--env".to_string());
            parts.push("HOME=/home/agent".to_string());
            parts.push("--mount".to_string());
            parts.push(provider_home_mount(
                &self.config,
                host_home,
                container_home,
                runtime.mount_read_only,
            ));
        }

        parts.push("--mount".to_string());
        parts.push(run_mount(&self.config, &run_dir));
        parts.push("--mount".to_string());
        parts.push(project_mount(&self.config, spec));
        for file in &spec.instruction_files {
            parts.push("--mount".to_string());
            parts.push(instruction_file_mount(
                &self.config,
                &run_dir,
                &file.filename,
            )?);
        }
        parts.push(self.config.docker.agent_image.clone());
        parts.push(provider_command.program);
        parts.extend(provider_command.args);
        Ok(parts)
    }

    fn prepare_run_dir(&self, spec: &AgentRunSpec) -> Result<std::path::PathBuf> {
        let run_dir = self
            .config
            .home
            .join(".app")
            .join("runs")
            .join(spec.job_id.to_string());
        fs::create_dir_all(&run_dir)
            .with_context(|| format!("Failed to create {}", run_dir.display()))?;
        fs::write(run_dir.join("prompt.txt"), &spec.prompt)
            .with_context(|| format!("Failed to write prompt for job {}", spec.job_id))?;
        for file in &spec.instruction_files {
            validate_instruction_filename(&file.filename)?;
            fs::write(run_dir.join(&file.filename), &file.content).with_context(|| {
                format!(
                    "Failed to write instruction file {} for job {}",
                    file.filename, spec.job_id
                )
            })?;
        }
        Ok(run_dir)
    }

    pub async fn cleanup_stopped_librarian_containers(&self) -> Result<CleanupReport> {
        let mut args = self.config.docker.runtime_args.clone();
        args.extend([
            "container".to_string(),
            "prune".to_string(),
            "--force".to_string(),
            "--filter".to_string(),
            "label=librarian.managed=true".to_string(),
        ]);
        let output = Command::new(&self.config.docker.runtime_command)
            .args(args)
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

fn runtime_prefix(config: &Config) -> Vec<String> {
    let mut parts = vec![config.docker.runtime_command.clone()];
    parts.extend(config.docker.runtime_args.clone());
    parts
}

fn provider_home_mount(
    config: &Config,
    host_home: &std::path::Path,
    container_home: &str,
    read_only: bool,
) -> String {
    let readonly = if read_only { ",readonly" } else { "" };
    format!(
        "type=bind,source={},target={}{}",
        mount_source(config, host_home),
        container_home,
        readonly
    )
}

fn project_mount(config: &Config, spec: &AgentRunSpec) -> String {
    let readonly = match spec.mount_mode {
        MountMode::ReadOnly => ",readonly",
        MountMode::ReadWrite => "",
    };
    format!(
        "type=bind,source={},target=/workspace/project{}",
        mount_source(config, &spec.project_path),
        readonly
    )
}

fn run_mount(config: &Config, run_dir: &std::path::Path) -> String {
    format!(
        "type=bind,source={},target=/workspace/run",
        mount_source(config, run_dir)
    )
}

fn identity_file_mount(
    config: &Config,
    run_dir: &std::path::Path,
    filename: &str,
    target: &str,
) -> String {
    format!(
        "type=bind,source={},target={},readonly",
        mount_source(config, &run_dir.join(filename)),
        target
    )
}

fn instruction_file_mount(
    config: &Config,
    run_dir: &std::path::Path,
    filename: &str,
) -> Result<String> {
    validate_instruction_filename(filename)?;
    let source = run_dir.join(filename);
    Ok(format!(
        "type=bind,source={},target=/workspace/project/{},readonly",
        mount_source(config, &source),
        filename
    ))
}

fn validate_instruction_filename(filename: &str) -> Result<()> {
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename == "."
        || filename == ".."
        || PathBuf::from(filename).components().count() != 1
    {
        bail!("Invalid provider instruction filename `{filename}`");
    }
    Ok(())
}

fn mount_source(config: &Config, path: &std::path::Path) -> String {
    let raw = path.display().to_string();
    let host_path = if let Some(stripped) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{stripped}")
    } else if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        stripped.to_string()
    } else {
        raw
    };

    if config.docker.mount_path_style.eq_ignore_ascii_case("wsl") {
        windows_path_to_wsl(&host_path).unwrap_or(host_path)
    } else {
        host_path
    }
}

#[derive(Clone, Debug)]
struct ContainerUser {
    uid: u32,
    gid: u32,
}

impl ContainerUser {
    fn spec(&self) -> String {
        format!("{}:{}", self.uid, self.gid)
    }
}

fn write_container_identity_files(run_dir: &std::path::Path, user: &ContainerUser) -> Result<()> {
    let passwd = format!(
        "root:x:0:0:root:/root:/bin/sh\nagent:x:{}:{}:Librarian Agent:/home/agent:/bin/sh\n",
        user.uid, user.gid
    );
    let group = format!("root:x:0:\nagent:x:{}:\n", user.gid);
    fs::write(run_dir.join("passwd"), passwd)
        .with_context(|| format!("Failed to write {}", run_dir.join("passwd").display()))?;
    fs::write(run_dir.join("group"), group)
        .with_context(|| format!("Failed to write {}", run_dir.join("group").display()))?;
    Ok(())
}

#[cfg(unix)]
fn container_user_for_host_path(path: &std::path::Path) -> Result<Option<ContainerUser>> {
    use std::os::unix::fs::MetadataExt;

    let metadata =
        std::fs::metadata(path).with_context(|| format!("Failed to inspect {}", path.display()))?;
    Ok(Some(ContainerUser {
        uid: metadata.uid(),
        gid: metadata.gid(),
    }))
}

#[cfg(not(unix))]
fn container_user_for_host_path(_path: &std::path::Path) -> Result<Option<ContainerUser>> {
    Ok(None)
}

fn windows_path_to_wsl(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    if bytes.len() < 3 || bytes[1] != b':' || bytes[2] != b'\\' {
        return None;
    }
    let drive = (bytes[0] as char).to_ascii_lowercase();
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    let rest = path[3..].replace('\\', "/");
    Some(format!("/mnt/{drive}/{rest}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MountMode, NetworkMode, ProviderKind};
    use uuid::Uuid;

    #[tokio::test]
    async fn codex_mount_uses_host_owner_on_unix() {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-docker-runner-{}", Uuid::new_v4()));

        {
            let mut config = Config::load_or_default(Some(home.clone())).expect("config");
            config.ensure_layout().expect("layout");
            let codex_home = home.join(".cfg").join("codex-home");
            std::fs::create_dir_all(&codex_home).expect("codex home");
            let project = home.join("Projects").join("Smoke");
            std::fs::create_dir_all(&project).expect("project");
            config.codex.host_home = Some(codex_home);
            config.codex.mount_host_home = true;
            let job_id = Uuid::new_v4();
            let spec = AgentRunSpec {
                job_id,
                project_path: project,
                provider: ProviderKind::Codex,
                goal: "test".to_string(),
                prompt: "test".to_string(),
                instruction_files: Vec::new(),
                mount_mode: MountMode::ReadOnly,
                network_mode: NetworkMode::None,
                secret_grant_token: None,
            };

            let command = DockerRunner::new(config)
                .docker_command_parts(&spec)
                .await
                .expect("command");

            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;

                let metadata = std::fs::metadata(&codex_home).expect("codex home metadata");
                assert!(command.iter().any(|part| part == "--user"));
                assert!(command
                    .iter()
                    .any(|part| part == &format!("{}:{}", metadata.uid(), metadata.gid())));
                assert!(command
                    .iter()
                    .any(|part| part.contains("target=/etc/passwd")));
                assert!(command
                    .iter()
                    .any(|part| part.contains("target=/etc/group")));

                let run_dir = home.join(".app").join("runs").join(job_id.to_string());
                let passwd = std::fs::read_to_string(run_dir.join("passwd")).expect("passwd");
                let group = std::fs::read_to_string(run_dir.join("group")).expect("group");
                assert!(passwd.contains(&format!(
                    "agent:x:{}:{}:Librarian Agent:/home/agent:/bin/sh",
                    metadata.uid(),
                    metadata.gid()
                )));
                assert!(group.contains(&format!("agent:x:{}:", metadata.gid())));
            }
            #[cfg(not(unix))]
            assert!(!command.iter().any(|part| part == "--user"));
        }

        std::fs::remove_dir_all(home).ok();
    }

    #[tokio::test]
    async fn claude_instruction_file_mounts_into_project_root() {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-claude-runner-{}", Uuid::new_v4()));

        {
            let config = Config::load_or_default(Some(home.clone())).expect("config");
            config.ensure_layout().expect("layout");
            let project = home.join("Projects").join("Smoke");
            std::fs::create_dir_all(&project).expect("project");
            let spec = AgentRunSpec {
                job_id: Uuid::new_v4(),
                project_path: project,
                provider: ProviderKind::ClaudeCode,
                goal: "test".to_string(),
                prompt: "test".to_string(),
                instruction_files: vec![crate::domain::AgentInstructionFile {
                    filename: "CLAUDE.md".to_string(),
                    content: "Use the library context.".to_string(),
                }],
                mount_mode: MountMode::ReadOnly,
                network_mode: NetworkMode::Provider,
                secret_grant_token: None,
            };

            let command = DockerRunner::new(config)
                .docker_command_parts(&spec)
                .await
                .expect("command");
            assert!(command.iter().any(|part| {
                part.contains("target=/workspace/project/CLAUDE.md") && part.contains("readonly")
            }));
        }

        std::fs::remove_dir_all(home).ok();
    }

    #[tokio::test]
    async fn claude_profile_mount_uses_configured_home() {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-claude-home-{}", Uuid::new_v4()));

        {
            let mut config = Config::load_or_default(Some(home.clone())).expect("config");
            config.ensure_layout().expect("layout");
            let claude_home = home.join(".cfg").join("claude-home");
            std::fs::create_dir_all(&claude_home).expect("claude home");
            config.claude.host_home = Some(claude_home);
            config.claude.mount_host_home = true;
            let project = home.join("Projects").join("Smoke");
            std::fs::create_dir_all(&project).expect("project");
            let spec = AgentRunSpec {
                job_id: Uuid::new_v4(),
                project_path: project,
                provider: ProviderKind::ClaudeCode,
                goal: "test".to_string(),
                prompt: "test".to_string(),
                instruction_files: vec![crate::domain::AgentInstructionFile {
                    filename: "CLAUDE.md".to_string(),
                    content: "Use the library context.".to_string(),
                }],
                mount_mode: MountMode::ReadOnly,
                network_mode: NetworkMode::Provider,
                secret_grant_token: None,
            };

            let command = DockerRunner::new(config)
                .docker_command_parts(&spec)
                .await
                .expect("command");
            assert!(command
                .iter()
                .any(|part| part == "CLAUDE_HOME=/home/agent/.claude"));
            assert!(command
                .iter()
                .any(|part| part.contains("target=/home/agent/.claude")));
        }

        std::fs::remove_dir_all(home).ok();
    }
}
