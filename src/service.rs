use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use tokio::process::Command as TokioCommand;

use crate::config::Config;

pub const SERVICE_NAME: &str = "librarian.service";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceStatus {
    pub supported: bool,
    pub installed: bool,
    pub active: bool,
    pub enabled: bool,
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

pub fn render_systemd_user_unit(binary: &Path, home: &Path) -> String {
    format!(
        r#"[Unit]
Description=Librarian autonomous daemon
After=default.target

[Service]
Type=simple
ExecStart={} --home {} daemon
WorkingDirectory={}
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=librarian=info,tower_http=info

[Install]
WantedBy=default.target
"#,
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
    fs::write(&unit_path, render_systemd_user_unit(&binary, &config.home))?;
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

pub async fn start() -> Result<()> {
    ensure_systemctl().await?;
    if !unit_path()?.exists() {
        bail!(
            "Librarian service is not installed. Run `librarian service install --enable` first."
        );
    }
    systemctl_user(&["start", SERVICE_NAME]).await
}

pub async fn stop() -> Result<()> {
    ensure_systemctl().await?;
    systemctl_user(&["stop", SERVICE_NAME]).await
}

pub async fn restart() -> Result<()> {
    ensure_systemctl().await?;
    if !unit_path()?.exists() {
        bail!(
            "Librarian service is not installed. Run `librarian service install --enable` first."
        );
    }
    systemctl_user(&["restart", SERVICE_NAME]).await
}

pub async fn status() -> ServiceStatus {
    if ensure_systemctl().await.is_err() {
        return ServiceStatus {
            supported: false,
            installed: false,
            active: false,
            enabled: false,
            detail: "systemctl --user is not available".to_string(),
        };
    }
    let installed = unit_path().map(|path| path.exists()).unwrap_or(false);
    let active = systemctl_user_success(&["is-active", "--quiet", SERVICE_NAME]).await;
    let enabled = systemctl_user_success(&["is-enabled", "--quiet", SERVICE_NAME]).await;
    let detail = if installed {
        format!(
            "{} installed; active={}; enabled={}",
            SERVICE_NAME, active, enabled
        )
    } else {
        format!("{SERVICE_NAME} is not installed")
    };
    ServiceStatus {
        supported: true,
        installed,
        active,
        enabled,
        detail,
    }
}

pub async fn print_status() -> Result<()> {
    let status = status().await;
    println!("Librarian service");
    println!("  supported: {}", status.supported);
    println!("  installed: {}", status.installed);
    println!("  active: {}", status.active);
    println!("  enabled: {}", status.enabled);
    println!("  detail: {}", status.detail);
    if status.supported && status.installed {
        let _ = systemctl_user_passthrough(&["status", SERVICE_NAME, "--no-pager"]).await;
    }
    Ok(())
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
        );
        assert!(unit.contains(
            "ExecStart=\"/home/user/My Apps/librarian\" --home \"/home/user/My Librarian\" daemon"
        ));
    }
}
