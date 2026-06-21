use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use uuid::Uuid;

use crate::{
    config::Config,
    db::Database,
    domain::{Job, MountMode, NetworkMode, Project, SecretRecord},
    secrets,
};

const GIT_PROXY_CAPABILITY: &str = "git-proxy";

#[derive(Clone, Debug, Deserialize)]
pub struct GitProxyRequest {
    pub token: String,
    pub cwd: String,
    pub argv: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GitProxyResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub operation: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitOperation {
    LocalRead,
    NetworkRead,
    LocalWrite,
    DestructiveWrite,
    RemoteWrite,
}

impl GitOperation {
    fn label(self) -> &'static str {
        match self {
            Self::LocalRead => "local_read",
            Self::NetworkRead => "network_read",
            Self::LocalWrite => "local_write",
            Self::DestructiveWrite => "destructive_write",
            Self::RemoteWrite => "remote_write",
        }
    }
}

#[derive(Clone, Debug)]
struct ParsedGitCommand {
    argv: Vec<String>,
    subcommand: String,
    operation: GitOperation,
    explicit_remote: Option<String>,
}

pub async fn handle_git_proxy(
    config: &Config,
    db: &Database,
    request: GitProxyRequest,
) -> Result<GitProxyResponse> {
    let grant_id = secrets::decode_grant_token(&request.token)?;
    let grant = db.consume_secret_grant(grant_id).await?;
    if grant.capability != GIT_PROXY_CAPABILITY {
        bail!("Secret grant does not allow git proxy access");
    }
    let Some(job_id) = grant.job_id else {
        bail!("Git proxy grants must be scoped to a job");
    };
    let job = db.get_job(job_id).await?;
    let project = db.get_project_by_id(job.project_id).await?;
    let secret = db
        .get_secret_by_name_or_id(&grant.secret_id.to_string())
        .await?;
    let parsed = parse_git_command(&request.argv)?;
    let cwd = map_container_cwd(&project, &request.cwd)?;
    ensure_git_policy(&job, &project, &parsed)?;
    let remote = resolve_remote_for_policy(&cwd, &parsed).await?;
    ensure_credential_allows_remote(&secret, remote.as_deref())?;

    let result = execute_git_command(config, &job, &secret, &parsed.argv, &cwd).await;
    let (success, response, error) = match result {
        Ok(response) => (response.exit_code == 0, Some(response), None),
        Err(error) => (false, None, Some(error)),
    };
    db.add_secret_audit_event(
        secret.id,
        Some(grant.id),
        Some(job.id),
        "git_proxy",
        success,
        serde_json::json!({
            "operation": parsed.operation.label(),
            "subcommand": parsed.subcommand,
            "remote": remote,
            "cwd": cwd.display().to_string(),
            "argv": redact_git_args(&parsed.argv),
            "error": error.as_ref().map(ToString::to_string),
        }),
    )
    .await?;
    if let Some(error) = error {
        return Err(error);
    }
    Ok(response.expect("response set when no error"))
}

pub async fn create_git_proxy_grant_for_job(
    config: &Config,
    db: &Database,
    job_id: Uuid,
) -> Result<Option<String>> {
    let git_secrets = db
        .list_secret_records()
        .await?
        .into_iter()
        .filter(is_git_credential)
        .collect::<Vec<_>>();
    if git_secrets.is_empty() {
        return Ok(None);
    }
    if git_secrets.len() > 1 {
        bail!(
            "Multiple git credentials are stored. Select a project default before launching brokered git jobs."
        );
    }
    let vault = secrets::SecretVault::new(config.clone());
    let grant_id = vault
        .grant(
            db,
            &git_secrets[0].id.to_string(),
            Some(job_id),
            Some("git"),
            GIT_PROXY_CAPABILITY,
            24 * 60 * 60,
            1000,
        )
        .await?;
    Ok(Some(secrets::encode_grant_token(grant_id)))
}

fn is_git_credential(secret: &SecretRecord) -> bool {
    secret.provider == "git"
        && matches!(
            secret.kind.as_str(),
            "ssh-private-key" | "https-token" | "github-token"
        )
}

fn parse_git_command(argv: &[String]) -> Result<ParsedGitCommand> {
    if argv.is_empty() {
        bail!("git proxy requires a git subcommand");
    }
    let mut normalized = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "-C" => {
                bail!(
                    "git -C is not supported through the proxy yet; run from the target directory"
                )
            }
            "--git-dir" | "--work-tree" => bail!("git directory override is not allowed"),
            value if value.starts_with("--git-dir=") || value.starts_with("--work-tree=") => {
                bail!("git directory override is not allowed")
            }
            value if value.starts_with('-') => {
                normalized.push(argv[index].clone());
                index += 1;
            }
            _ => break,
        }
    }
    if index >= argv.len() {
        bail!("git proxy could not find a subcommand");
    }
    normalized.extend(argv[index..].iter().cloned());
    let subcommand = normalized[index_of_subcommand(&normalized)?].clone();
    let operation = classify_subcommand(&subcommand, &normalized)?;
    let explicit_remote = explicit_remote_arg(&subcommand, &normalized);
    Ok(ParsedGitCommand {
        argv: normalized,
        subcommand,
        operation,
        explicit_remote,
    })
}

fn index_of_subcommand(argv: &[String]) -> Result<usize> {
    argv.iter()
        .position(|arg| !arg.starts_with('-'))
        .ok_or_else(|| anyhow::anyhow!("git proxy could not find a subcommand"))
}

fn classify_subcommand(subcommand: &str, argv: &[String]) -> Result<GitOperation> {
    match subcommand {
        "status" | "diff" | "log" | "show" | "branch" | "rev-parse" | "remote" | "ls-files" => {
            Ok(GitOperation::LocalRead)
        }
        "clone" | "fetch" | "pull" | "ls-remote" => Ok(GitOperation::NetworkRead),
        "add" | "restore" | "checkout" | "switch" | "commit" | "tag" | "stash" => {
            if subcommand == "checkout" && argv.iter().any(|arg| arg == "--orphan") {
                return Ok(GitOperation::DestructiveWrite);
            }
            Ok(GitOperation::LocalWrite)
        }
        "reset" | "clean" | "rebase" | "merge" | "cherry-pick" => {
            Ok(GitOperation::DestructiveWrite)
        }
        "push" => Ok(GitOperation::RemoteWrite),
        _ => bail!("git subcommand `{subcommand}` is not allowed by the proxy"),
    }
}

fn explicit_remote_arg(subcommand: &str, argv: &[String]) -> Option<String> {
    let subcommand_index = index_of_subcommand(argv).ok()?;
    let args = &argv[subcommand_index + 1..];
    match subcommand {
        "clone" | "ls-remote" => args.iter().find(|arg| !arg.starts_with('-')).cloned(),
        "fetch" | "pull" | "push" => args.iter().find(|arg| !arg.starts_with('-')).cloned(),
        _ => None,
    }
}

fn map_container_cwd(project: &Project, cwd: &str) -> Result<PathBuf> {
    let Some(relative) = cwd.strip_prefix("/workspace/project") else {
        bail!("git proxy cwd must be inside /workspace/project");
    };
    let relative = relative.trim_start_matches('/');
    let candidate = project.path.join(relative);
    let canonical_project = canonical_or_existing_parent(&project.path)?;
    let canonical_candidate = canonical_or_existing_parent(&candidate)?;
    if !canonical_candidate.starts_with(&canonical_project) {
        bail!("git proxy cwd escaped the project boundary");
    }
    Ok(candidate)
}

fn canonical_or_existing_parent(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Path has no parent: {}", path.display()))?;
    Ok(parent.canonicalize()?)
}

fn ensure_git_policy(job: &Job, project: &Project, parsed: &ParsedGitCommand) -> Result<()> {
    match parsed.operation {
        GitOperation::LocalRead => Ok(()),
        GitOperation::NetworkRead => {
            if matches!(job.network_mode, NetworkMode::None) {
                bail!(
                    "git network command `{}` requires network permission",
                    parsed.subcommand
                );
            }
            Ok(())
        }
        GitOperation::LocalWrite => {
            if matches!(job.mount_mode, MountMode::ReadOnly) {
                bail!(
                    "git write command `{}` is blocked for read-only jobs",
                    parsed.subcommand
                );
            }
            if parsed.subcommand == "commit" && !project.git_policy.allow_commit {
                bail!("project git policy blocks commits");
            }
            Ok(())
        }
        GitOperation::DestructiveWrite => {
            bail!(
                "git command `{}` requires a destructive-git policy that is not enabled",
                parsed.subcommand
            )
        }
        GitOperation::RemoteWrite => {
            if matches!(job.mount_mode, MountMode::ReadOnly) {
                bail!("git push is blocked for read-only jobs");
            }
            if !project.git_policy.allow_push {
                bail!("project git policy blocks pushes");
            }
            if matches!(job.network_mode, NetworkMode::None) {
                bail!("git push requires network permission");
            }
            Ok(())
        }
    }
}

async fn resolve_remote_for_policy(
    cwd: &Path,
    parsed: &ParsedGitCommand,
) -> Result<Option<String>> {
    if let Some(remote) = &parsed.explicit_remote {
        if looks_like_remote(remote) {
            return Ok(Some(remote.clone()));
        }
        if matches!(
            parsed.operation,
            GitOperation::NetworkRead | GitOperation::RemoteWrite
        ) {
            return git_remote_url(cwd, remote).await.map(Some);
        }
    }
    if matches!(
        parsed.operation,
        GitOperation::NetworkRead | GitOperation::RemoteWrite
    ) {
        return git_remote_url(cwd, "origin").await.map(Some);
    }
    Ok(None)
}

fn looks_like_remote(value: &str) -> bool {
    value.contains("://") || value.contains('@') || value.starts_with("git:")
}

async fn git_remote_url(cwd: &Path, remote: &str) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["remote", "get-url", remote])
        .output()
        .await?;
    if !output.status.success() {
        bail!("Could not resolve git remote `{remote}`");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ensure_credential_allows_remote(secret: &SecretRecord, remote: Option<&str>) -> Result<()> {
    let Some(remote) = remote else {
        return Ok(());
    };
    let allowed = secret
        .metadata
        .get("allowed_remotes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    if allowed.is_empty()
        || allowed
            .iter()
            .any(|candidate| remote_matches(candidate, remote))
    {
        return Ok(());
    }
    bail!(
        "git credential `{}` is not allowed for remote `{remote}`",
        secret.name
    )
}

fn remote_matches(pattern: &str, remote: &str) -> bool {
    pattern == remote || pattern == "*" || remote.starts_with(pattern.trim_end_matches('*'))
}

async fn execute_git_command(
    config: &Config,
    job: &Job,
    secret: &SecretRecord,
    argv: &[String],
    cwd: &Path,
) -> Result<GitProxyResponse> {
    fs::create_dir_all(cwd).with_context(|| format!("Failed to create {}", cwd.display()))?;
    let vault = secrets::SecretVault::new(config.clone());
    let plaintext = vault.decrypt_record(secret)?;
    let run_dir = config
        .home
        .join(".app")
        .join("runs")
        .join(job.id.to_string())
        .join("git-proxy");
    fs::create_dir_all(&run_dir)?;
    let mut command = Command::new("git");
    command.args(argv).current_dir(cwd);
    command.env("GIT_TERMINAL_PROMPT", "0");
    match secret.kind.as_str() {
        "ssh-private-key" => configure_ssh_key(&mut command, &run_dir, &plaintext)?,
        "https-token" | "github-token" => {
            configure_https_token(&mut command, &run_dir, &plaintext)?
        }
        other => bail!("Unsupported git credential kind `{other}`"),
    }
    let output = command.output().await?;
    Ok(GitProxyResponse {
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: redact_secret_text(&String::from_utf8_lossy(&output.stderr), &plaintext),
        operation: classify_subcommand(&argv[index_of_subcommand(argv)?], argv)?
            .label()
            .to_string(),
    })
}

fn configure_ssh_key(command: &mut Command, run_dir: &Path, key: &str) -> Result<()> {
    let key_path = run_dir.join("key");
    fs::write(&key_path, key).with_context(|| format!("Failed to write {}", key_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))?;
    }
    let known_hosts = run_dir.join("known_hosts");
    if !known_hosts.exists() {
        fs::write(&known_hosts, "")?;
    }
    command.env(
        "GIT_SSH_COMMAND",
        format!(
            "ssh -i {} -o IdentitiesOnly=yes -o UserKnownHostsFile={} -o StrictHostKeyChecking=accept-new",
            shell_path(&key_path),
            shell_path(&known_hosts)
        ),
    );
    Ok(())
}

fn configure_https_token(command: &mut Command, run_dir: &Path, token: &str) -> Result<()> {
    let askpass_path = run_dir.join("askpass.sh");
    fs::write(
        &askpass_path,
        format!(
            "#!/bin/sh\ncase \"$1\" in\n*Username*) printf '%s\\n' 'x-access-token' ;;\n*) printf '%s\\n' '{}' ;;\nesac\n",
            token.replace('\'', "'\\''")
        ),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&askpass_path, fs::Permissions::from_mode(0o700))?;
    }
    command.env("GIT_ASKPASS", &askpass_path);
    command.env("GIT_TERMINAL_PROMPT", "0");
    Ok(())
}

fn shell_path(path: &Path) -> String {
    path.display().to_string()
}

fn redact_git_args(argv: &[String]) -> Vec<String> {
    argv.iter()
        .map(|arg| {
            if arg.contains("://") && arg.contains('@') {
                "<redacted-url>".to_string()
            } else {
                arg.clone()
            }
        })
        .collect()
}

fn redact_secret_text(value: &str, secret: &str) -> String {
    if secret.is_empty() {
        value.to_string()
    } else {
        value.replace(secret, "[REDACTED_SECRET]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AutonomyMode, GitPolicy, JobStatus, ProviderKind};
    use chrono::Utc;

    fn job(mount_mode: MountMode, network_mode: NetworkMode) -> Job {
        Job {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            provider: ProviderKind::Codex,
            status: JobStatus::Running,
            goal: "test".to_string(),
            mount_mode,
            network_mode,
            secret_grant_token: None,
            cancel_requested_at: None,
            last_heartbeat_at: None,
            started_at: None,
            finished_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn project() -> Project {
        Project {
            id: Uuid::new_v4(),
            name: "Test".to_string(),
            library_path: None,
            path: std::env::current_dir().expect("cwd"),
            autonomy_mode: AutonomyMode::ProjectFull,
            git_policy: GitPolicy::default(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn classifies_git_commands() {
        assert_eq!(
            parse_git_command(&["status".to_string()])
                .expect("status")
                .operation,
            GitOperation::LocalRead
        );
        assert_eq!(
            parse_git_command(&["clone".to_string(), "git@github.com:a/b.git".to_string()])
                .expect("clone")
                .operation,
            GitOperation::NetworkRead
        );
        assert_eq!(
            parse_git_command(&["push".to_string()])
                .expect("push")
                .operation,
            GitOperation::RemoteWrite
        );
    }

    #[test]
    fn policy_blocks_writes_for_read_only_jobs() {
        let parsed = parse_git_command(&["commit".to_string(), "-m".to_string(), "x".to_string()])
            .expect("commit");
        assert!(ensure_git_policy(
            &job(MountMode::ReadOnly, NetworkMode::Open),
            &project(),
            &parsed
        )
        .is_err());
    }

    #[test]
    fn remote_allowlist_matches_exact_or_prefix_wildcard() {
        assert!(remote_matches(
            "git@github.com:no-more-care/*",
            "git@github.com:no-more-care/nomorecare.gg.git"
        ));
        assert!(!remote_matches(
            "git@github.com:other/*",
            "git@github.com:no-more-care/nomorecare.gg.git"
        ));
    }
}
