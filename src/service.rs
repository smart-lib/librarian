use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use tokio::{process::Command as TokioCommand, time::timeout};

use crate::config::Config;

pub const SERVICE_NAME: &str = "librarian.service";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceStatus {
    pub supported: bool,
    pub installed: bool,
    pub active: bool,
    pub enabled: bool,
    pub detail: String,
    pub runtime_probe: Option<ServiceRuntimeProbe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceRuntimeProbe {
    pub ok: bool,
    pub detail: String,
}

pub fn unit_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is required for systemd user services")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("systemd")
        .join("user")
        .join(SERVICE_NAME))
}

pub fn render_systemd_user_unit(
    binary: &Path,
    home: &Path,
    supplementary_groups: &[String],
) -> String {
    let supplementary_groups = if supplementary_groups.is_empty() {
        String::new()
    } else {
        format!("SupplementaryGroups={}\n", supplementary_groups.join(" "))
    };
    format!(
        r#"[Unit]
Description=Librarian autonomous daemon
After=default.target

[Service]
Type=simple
{}ExecStart={} --home {} daemon
WorkingDirectory={}
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=librarian=info,tower_http=info

[Install]
WantedBy=default.target
"#,
        supplementary_groups,
        systemd_quote(binary),
        systemd_quote(home),
        systemd_quote(home),
    )
}

pub async fn install(config: &Config, enable: bool, start: bool) -> Result<()> {
    ensure_systemctl().await?;
    let unit_path = unit_path()?;
    if let Some(parent) = unit_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let binary = std::env::current_exe().context("resolve current executable")?;
    fs::write(
        &unit_path,
        render_systemd_user_unit(&binary, &config.home, &service_supplementary_groups()),
    )?;
    systemctl_user(&["daemon-reload"]).await?;
    if enable {
        systemctl_user(&["enable", SERVICE_NAME]).await?;
    }
    if start {
        systemctl_user(&["restart", SERVICE_NAME]).await?;
    }
    Ok(())
}

pub async fn uninstall() -> Result<()> {
    ensure_systemctl().await?;
    let _ = systemctl_user(&["stop", SERVICE_NAME]).await;
    let _ = systemctl_user(&["disable", SERVICE_NAME]).await;
    let unit_path = unit_path()?;
    if unit_path.exists() {
        fs::remove_file(unit_path)?;
    }
    systemctl_user(&["daemon-reload"]).await?;
    Ok(())
}

pub async fn start(config: &Config) -> Result<()> {
    ensure_systemctl().await?;
    if !unit_path()?.exists() {
        bail!(
            "Librarian service is not installed. Run `librarian service install --enable` first."
        );
    }
    install(config, false, false).await?;
    systemctl_user(&["start", SERVICE_NAME]).await
}

pub async fn stop() -> Result<()> {
    ensure_systemctl().await?;
    systemctl_user(&["stop", SERVICE_NAME]).await
}

pub async fn restart(config: &Config) -> Result<()> {
    ensure_systemctl().await?;
    if !unit_path()?.exists() {
        bail!(
            "Librarian service is not installed. Run `librarian service install --enable` first."
        );
    }
    let enabled = systemctl_user_success(&["is-enabled", "--quiet", SERVICE_NAME]).await;
    install(config, enabled, false).await?;
    systemctl_user(&["restart", SERVICE_NAME]).await
}

pub async fn status(config: Option<&Config>) -> ServiceStatus {
    if ensure_systemctl().await.is_err() {
        return ServiceStatus {
            supported: false,
            installed: false,
            active: false,
            enabled: false,
            detail: "systemctl --user is not available".to_string(),
            runtime_probe: None,
        };
    }
    let installed = unit_path().map(|path| path.exists()).unwrap_or(false);
    let active = systemctl_user_success(&["is-active", "--quiet", SERVICE_NAME]).await;
    let enabled = systemctl_user_success(&["is-enabled", "--quiet", SERVICE_NAME]).await;
    let runtime_probe = if installed && active {
        if let Some(config) = config {
            Some(service_runtime_probe(config).await)
        } else {
            None
        }
    } else {
        None
    };
    let detail = if installed {
        let mut detail = format!(
            "{} installed; active={}; enabled={}",
            SERVICE_NAME, active, enabled
        );
        if let Some(probe) = runtime_probe.as_ref() {
            detail.push_str(&format!(
                "; service runtime probe={}",
                if probe.ok { "ok" } else { "failed" }
            ));
            if !probe.ok {
                detail.push_str(&format!(" ({})", probe.detail));
            }
        }
        detail
    } else {
        format!("{SERVICE_NAME} is not installed")
    };
    ServiceStatus {
        supported: true,
        installed,
        active,
        enabled,
        detail,
        runtime_probe,
    }
}

pub async fn print_status(config: &Config) -> Result<()> {
    let status = status(Some(config)).await;
    println!("Librarian service");
    println!("  supported: {}", status.supported);
    println!("  installed: {}", status.installed);
    println!("  active: {}", status.active);
    println!("  enabled: {}", status.enabled);
    println!("  detail: {}", status.detail);
    if let Some(probe) = status.runtime_probe {
        println!("  runtime probe ok: {}", probe.ok);
        println!("  runtime probe detail: {}", probe.detail);
    }
    if status.supported && status.installed {
        let _ = systemctl_user_passthrough(&["status", SERVICE_NAME, "--no-pager"]).await;
    }
    Ok(())
}

async fn service_runtime_probe(config: &Config) -> ServiceRuntimeProbe {
    let mut command = TokioCommand::new("systemd-run");
    command
        .arg("--user")
        .arg("--collect")
        .arg("--wait")
        .arg("--pipe")
        .arg("--quiet")
        .arg("--property=Type=oneshot")
        .arg(&config.docker.runtime_command)
        .args(&config.docker.runtime_args)
        .arg("info");
    match timeout(Duration::from_secs(20), command.output()).await {
        Ok(Ok(output)) if output.status.success() => ServiceRuntimeProbe {
            ok: true,
            detail: "runtime is reachable from the user service manager context".to_string(),
        },
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let detail = format!("{}{}", stderr.trim(), stdout.trim());
            ServiceRuntimeProbe {
                ok: false,
                detail: compact_detail(&detail),
            }
        }
        Ok(Err(error)) => ServiceRuntimeProbe {
            ok: false,
            detail: format!("could not run systemd service-context probe: {error}"),
        },
        Err(_) => ServiceRuntimeProbe {
            ok: false,
            detail: "service-context runtime probe timed out".to_string(),
        },
    }
}

fn service_supplementary_groups() -> Vec<String> {
    if group_exists("docker") && current_user_groups().iter().any(|group| group == "docker") {
        vec!["docker".to_string()]
    } else {
        Vec::new()
    }
}

fn group_exists(group: &str) -> bool {
    fs::read_to_string("/etc/group")
        .map(|content| {
            content
                .lines()
                .any(|line| line.split(':').next() == Some(group))
        })
        .unwrap_or(false)
}

fn current_user_groups() -> Vec<String> {
    Command::new("id")
        .arg("-nG")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .map(|group| group.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn compact_detail(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= 240 {
        normalized
    } else {
        let mut output = normalized.chars().take(239).collect::<String>();
        output.push('…');
        output
    }
}

async fn ensure_systemctl() -> Result<()> {
    let status = TokioCommand::new("systemctl")
        .arg("--user")
        .arg("--version")
        .status()
        .await
        .context("run systemctl --user --version")?;
    if !status.success() {
        bail!("systemctl --user is not available in this session");
    }
    Ok(())
}

async fn systemctl_user(args: &[&str]) -> Result<()> {
    let output = TokioCommand::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .await
        .with_context(|| format!("run systemctl --user {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "systemctl --user {} failed: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }
    Ok(())
}

async fn systemctl_user_success(args: &[&str]) -> bool {
    TokioCommand::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn systemctl_user_passthrough(args: &[&str]) -> Result<()> {
    TokioCommand::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .await
        .with_context(|| format!("run systemctl --user {}", args.join(" ")))?;
    Ok(())
}

fn systemd_quote(path: &Path) -> String {
    let value = path.display().to_string();
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':'))
    {
        return value;
    }
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unit_with_daemon_exec_start() {
        let unit = render_systemd_user_unit(
            Path::new("/home/user/Librarian/.app/bin/librarian"),
            Path::new("/home/user/Librarian"),
            &[],
        );
        assert!(unit.contains(
            "ExecStart=/home/user/Librarian/.app/bin/librarian --home /home/user/Librarian daemon"
        ));
        assert!(unit.contains("Restart=on-failure"));
        assert!(!unit.contains(" admin"));
    }

    #[test]
    fn quotes_paths_with_spaces() {
        let unit = render_systemd_user_unit(
            Path::new("/home/user/My Apps/librarian"),
            Path::new("/home/user/My Librarian"),
            &[],
        );
        assert!(unit.contains(
            "ExecStart=\"/home/user/My Apps/librarian\" --home \"/home/user/My Librarian\" daemon"
        ));
    }

    #[test]
    fn renders_unit_with_docker_supplementary_group() {
        let unit = render_systemd_user_unit(
            Path::new("/home/user/Librarian/.app/bin/librarian"),
            Path::new("/home/user/Librarian"),
            &["docker".to_string()],
        );
        assert!(unit.contains("SupplementaryGroups=docker"));
        assert!(unit.contains("ExecStart=/home/user/Librarian/.app/bin/librarian"));
    }
}
