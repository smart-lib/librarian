use anyhow::{bail, Result};
use chrono::{Duration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    db::Database,
    domain::{Job, NetworkMode, ProviderKind},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelMetadata {
    pub provider: String,
    pub model: String,
    pub input_cost_per_million: Option<f64>,
    pub output_cost_per_million: Option<f64>,
    pub task_hints: Vec<String>,
}

pub fn provider_name(provider: &ProviderKind) -> &'static str {
    match provider {
        ProviderKind::Codex => "codex",
        ProviderKind::OpenRouter => "openrouter",
        ProviderKind::ClaudeCode => "claude-code",
    }
}

pub fn default_model(provider: &ProviderKind) -> Option<&'static str> {
    match provider {
        ProviderKind::Codex => Some("codex-cli-default"),
        ProviderKind::OpenRouter => Some("openrouter-default"),
        ProviderKind::ClaudeCode => Some("claude-code-default"),
    }
}

pub fn parse_provider_kind(value: &str) -> Result<ProviderKind> {
    match value {
        "Codex" | "codex" => Ok(ProviderKind::Codex),
        "OpenRouter" | "openrouter" => Ok(ProviderKind::OpenRouter),
        "ClaudeCode" | "claude-code" | "claude_code" => Ok(ProviderKind::ClaudeCode),
        _ => bail!("Unknown provider `{value}`"),
    }
}

pub fn default_network_mode_for_provider(
    provider: &ProviderKind,
    allow_open_network: bool,
    has_secret_grant: bool,
) -> NetworkMode {
    if allow_open_network || has_secret_grant {
        NetworkMode::Open
    } else if provider_requires_provider_network(provider) {
        NetworkMode::Provider
    } else {
        NetworkMode::None
    }
}

pub fn provider_requires_provider_network(provider: &ProviderKind) -> bool {
    matches!(
        provider,
        ProviderKind::Codex | ProviderKind::OpenRouter | ProviderKind::ClaudeCode
    )
}

pub fn model_catalog() -> Vec<ModelMetadata> {
    vec![
        ModelMetadata {
            provider: "codex".to_string(),
            model: "codex-cli-default".to_string(),
            input_cost_per_million: None,
            output_cost_per_million: None,
            task_hints: vec!["coding".to_string(), "repo-edit".to_string()],
        },
        ModelMetadata {
            provider: "openrouter".to_string(),
            model: "openrouter-default".to_string(),
            input_cost_per_million: None,
            output_cost_per_million: None,
            task_hints: vec!["api".to_string(), "fallback".to_string()],
        },
        ModelMetadata {
            provider: "claude-code".to_string(),
            model: "claude-code-default".to_string(),
            input_cost_per_million: None,
            output_cost_per_million: None,
            task_hints: vec!["coding".to_string(), "cli".to_string()],
        },
    ]
}

pub async fn ensure_provider_available(db: &Database, job: &Job) -> Result<()> {
    ensure_provider_kind_available(db, &job.provider).await
}

pub async fn ensure_provider_kind_available(
    db: &Database,
    provider_kind: &ProviderKind,
) -> Result<()> {
    let provider = provider_name(provider_kind);
    let model = default_model(provider_kind);
    if let Some(state) = db.get_provider_state(provider, model).await? {
        if state.status == "Paused" {
            if let Some(paused_until) = state.paused_until {
                if paused_until > Utc::now() {
                    bail!(
                        "Provider `{provider}` model `{}` is paused until {}: {}",
                        model.unwrap_or("-"),
                        paused_until.to_rfc3339(),
                        state
                            .reason
                            .unwrap_or_else(|| "no reason recorded".to_string())
                    );
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ProviderSelection {
    pub provider: ProviderKind,
    pub fallback_from: Option<ProviderKind>,
    pub reason: Option<String>,
}

pub async fn select_provider_for_job(
    config: &Config,
    db: &Database,
    job: &Job,
) -> Result<ProviderSelection> {
    match ensure_provider_available(db, job).await {
        Ok(()) => {
            return Ok(ProviderSelection {
                provider: job.provider.clone(),
                fallback_from: None,
                reason: None,
            });
        }
        Err(error) if !config.routing.fallback_enabled => return Err(error),
        Err(error) => {
            let original_error = error.to_string();
            for candidate in &config.routing.fallback_order {
                let candidate = parse_provider_kind(candidate)?;
                if candidate == job.provider {
                    continue;
                }
                if ensure_provider_kind_available(db, &candidate).await.is_ok() {
                    return Ok(ProviderSelection {
                        provider: candidate,
                        fallback_from: Some(job.provider.clone()),
                        reason: Some(original_error),
                    });
                }
            }
            bail!(
                "Provider `{}` is unavailable and no configured fallback provider is available: {original_error}",
                provider_name(&job.provider)
            );
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BudgetCheck {
    pub scope: &'static str,
    pub limit_usd: f64,
    pub spent_usd: f64,
}

pub async fn ensure_budget_available(
    config: &Config,
    db: &Database,
    job: &Job,
) -> Result<Vec<BudgetCheck>> {
    if !config.budget.enabled {
        return Ok(Vec::new());
    }

    let provider = provider_name(&job.provider);
    let since = Utc::now().date_naive().and_time(NaiveTime::MIN).and_utc();
    let mut checks = Vec::new();

    if let Some(limit) = config.budget.daily_total_usd {
        let spent = db.usage_cost_since(since, None, None).await?;
        checks.push(BudgetCheck {
            scope: "daily_total",
            limit_usd: limit,
            spent_usd: spent,
        });
    }
    if let Some(limit) = config.budget.daily_provider_usd {
        let spent = db.usage_cost_since(since, Some(provider), None).await?;
        checks.push(BudgetCheck {
            scope: "daily_provider",
            limit_usd: limit,
            spent_usd: spent,
        });
    }
    if let Some(limit) = config.budget.daily_project_usd {
        let spent = db
            .usage_cost_since(since, None, Some(job.project_id))
            .await?;
        checks.push(BudgetCheck {
            scope: "daily_project",
            limit_usd: limit,
            spent_usd: spent,
        });
    }

    for check in &checks {
        if check.spent_usd >= check.limit_usd {
            bail!(
                "Budget `{}` is exhausted: spent ${:.4} of ${:.4}",
                check.scope,
                check.spent_usd,
                check.limit_usd
            );
        }
    }

    Ok(checks)
}

pub fn detect_limit_event(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    for needle in [
        "rate limit",
        "rate_limit",
        "quota exceeded",
        "insufficient_quota",
        "too many requests",
        "429",
        "usage limit",
        "credit balance",
    ] {
        if lower.contains(needle) {
            return Some("limit_detected");
        }
    }
    None
}

pub fn detect_provider_diagnostic(job: &Job, text: &str) -> Option<serde_json::Value> {
    let provider = provider_name(&job.provider);
    let model = default_model(&job.provider);
    let lower = text.to_lowercase();

    if matches!(job.provider, ProviderKind::Codex) {
        if lower.contains("librarian_diagnostic codex_cli_missing") {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "codex_cli_missing",
                "severity": "error",
                "message": "Codex CLI is not installed in the agent image.",
                "next_step": "Run `librarian runtime build-agent-image`, or rebuild with Codex installation enabled.",
            }));
        }
        if lower.contains("librarian_diagnostic codex_home_missing") {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "codex_home_missing",
                "severity": "error",
                "message": "The configured Codex profile directory was not mounted into the agent container.",
                "next_step": "Run `librarian auth codex --enable-container-mount`, then `librarian doctor`.",
            }));
        }
        if (lower.contains("failed to read config file") || lower.contains("config.toml"))
            && lower.contains("permission denied")
        {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "codex_profile_permission_denied",
                "severity": "error",
                "message": "Codex profile files are mounted, but the container user cannot read them.",
                "next_step": "Upgrade Librarian and retry. The Docker runner should launch Codex with the host profile owner UID/GID on Unix hosts.",
            }));
        }
        if lower.contains("failed to lookup address information")
            || lower.contains("failed to connect to websocket")
            || lower.contains("stream disconnected before completion")
                && lower.contains("chatgpt.com")
            || lower.contains("error sending request for url") && lower.contains("chatgpt.com")
        {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "codex_provider_network_unavailable",
                "severity": "error",
                "message": "Codex started, but the agent container could not reach ChatGPT/OpenAI endpoints.",
                "next_step": "Upgrade Librarian and retry. Codex-backed jobs should use provider network by default; use `--allow-network` only for broader job network access.",
            }));
        }
        if lower.contains("401 missing bearer")
            || lower.contains("missing bearer")
            || lower.contains("no bearer token")
            || (lower.contains("unauthorized") && lower.contains("bearer"))
        {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "codex_auth_missing_bearer",
                "severity": "error",
                "message": "Codex started, but no usable OpenAI bearer token was available inside the run.",
                "next_step": "Authenticate Codex on the host, then enable the explicit Codex profile mount with `librarian auth codex --enable-container-mount`.",
            }));
        }
        if lower.contains("not logged in")
            || lower.contains("please log in")
            || lower.contains("run codex login")
        {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "codex_login_required",
                "severity": "error",
                "message": "Codex reports that the current profile is not authenticated.",
                "next_step": "Run `codex` on the host and complete sign-in, then retry the Librarian job.",
            }));
        }
    }
    if matches!(job.provider, ProviderKind::ClaudeCode) {
        if lower.contains("librarian_diagnostic claude_cli_missing") {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "claude_cli_missing",
                "severity": "error",
                "message": "Claude Code CLI is not installed in the agent image.",
                "next_step": "Rebuild the Librarian agent image with Claude Code installed, then rerun `librarian doctor`.",
            }));
        }
        if lower.contains("librarian_diagnostic claude_home_missing") {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "claude_home_missing",
                "severity": "error",
                "message": "The configured Claude profile directory was not mounted into the agent container.",
                "next_step": "Set `[claude].host_home` to the signed-in profile and enable `[claude].mount_host_home`, then rerun `librarian doctor`.",
            }));
        }
        if lower.contains("librarian_diagnostic claude_instruction_missing") {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "claude_instruction_missing",
                "severity": "error",
                "message": "Claude started without the expected project-local CLAUDE.md instruction file.",
                "next_step": "Upgrade Librarian and retry; Claude jobs should mount CLAUDE.md into the project root during job preparation.",
            }));
        }
        if lower.contains("permission denied")
            && (lower.contains("claude")
                || lower.contains("credentials")
                || lower.contains("config"))
        {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "claude_profile_permission_denied",
                "severity": "error",
                "message": "Claude profile files are mounted, but the container user cannot read them.",
                "next_step": "Retry with the current Librarian Docker runner, which should launch Claude with the host profile owner UID/GID on Unix hosts.",
            }));
        }
        if lower.contains("not logged in")
            || lower.contains("please log in")
            || lower.contains("login required")
            || lower.contains("authentication required")
        {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "claude_login_required",
                "severity": "error",
                "message": "Claude Code reports that the mounted profile is not authenticated.",
                "next_step": "Sign in to Claude Code on the host, point `[claude].host_home` at that profile, enable the container mount, and retry.",
            }));
        }
        if lower.contains("failed to lookup address information")
            || lower.contains("network")
                && (lower.contains("unreachable") || lower.contains("timed out"))
            || lower.contains("api.anthropic.com")
        {
            return Some(serde_json::json!({
                "provider": provider,
                "model": model,
                "code": "claude_provider_network_unavailable",
                "severity": "error",
                "message": "Claude Code started, but the agent container could not reach Claude/Anthropic endpoints.",
                "next_step": "Confirm the job uses provider network mode and retry. Use open network only when the task itself needs broader network access.",
            }));
        }
    }

    None
}

pub async fn record_limit_event(db: &Database, job: &Job, text: &str) -> Result<()> {
    let provider = provider_name(&job.provider);
    let model = default_model(&job.provider);
    db.add_usage_observation(
        provider,
        model,
        Some(job.id),
        None,
        None,
        None,
        true,
        serde_json::json!({ "source": "worker-log", "sample": truncate(text, 500) }),
    )
    .await?;
    let paused_until = Utc::now() + Duration::minutes(30);
    db.set_provider_pause(
        provider,
        model,
        paused_until,
        "limit detected from worker output",
    )
    .await?;
    db.add_system_event(
        "provider_paused",
        serde_json::json!({
            "provider": provider,
            "model": model,
            "paused_until": paused_until,
            "reason": "limit detected from worker output",
        }),
    )
    .await?;
    Ok(())
}

pub async fn record_job_usage_estimate(
    db: &Database,
    job: &Job,
    prompt_chars: usize,
    exit_code: Option<i32>,
) -> Result<()> {
    let provider = provider_name(&job.provider);
    let model = default_model(&job.provider);
    let input_tokens = (prompt_chars as f64 / 4.0).ceil() as i64;
    db.add_usage_observation(
        provider,
        model,
        Some(job.id),
        Some(input_tokens),
        None,
        None,
        false,
        serde_json::json!({
            "source": "local-estimate",
            "prompt_chars": prompt_chars,
            "exit_code": exit_code,
        }),
    )
    .await?;
    Ok(())
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        db::Database,
        domain::{MountMode, NetworkMode, ScheduleKind},
        scheduler,
    };
    use chrono::Utc;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn codex_job() -> Job {
        provider_job(ProviderKind::Codex)
    }

    fn claude_job() -> Job {
        provider_job(ProviderKind::ClaudeCode)
    }

    fn provider_job(provider: ProviderKind) -> Job {
        Job {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            provider,
            status: crate::domain::JobStatus::Running,
            goal: "test".to_string(),
            mount_mode: crate::domain::MountMode::ReadWrite,
            network_mode: crate::domain::NetworkMode::Open,
            secret_grant_token: None,
            cancel_requested_at: None,
            last_heartbeat_at: None,
            started_at: None,
            finished_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn detects_codex_missing_bearer() {
        let diagnostic =
            detect_provider_diagnostic(&codex_job(), "provider error: 401 Missing bearer")
                .expect("diagnostic");
        assert_eq!(diagnostic["code"], "codex_auth_missing_bearer");
    }

    #[test]
    fn detects_codex_profile_permission_denied() {
        let diagnostic = detect_provider_diagnostic(
            &codex_job(),
            "Error loading config.toml: Failed to read config file /home/agent/.codex/config.toml: Permission denied (os error 13)",
        )
        .expect("diagnostic");
        assert_eq!(diagnostic["code"], "codex_profile_permission_denied");
    }

    #[test]
    fn codex_defaults_to_provider_network() {
        assert!(matches!(
            default_network_mode_for_provider(&ProviderKind::Codex, false, false),
            NetworkMode::Provider
        ));
        assert!(matches!(
            default_network_mode_for_provider(&ProviderKind::Codex, true, false),
            NetworkMode::Open
        ));
    }

    #[test]
    fn detects_codex_provider_network_unavailable() {
        let diagnostic = detect_provider_diagnostic(
            &codex_job(),
            "failed to connect to websocket: failed to lookup address information, url: wss://chatgpt.com/backend-api/codex/responses",
        )
        .expect("diagnostic");
        assert_eq!(diagnostic["code"], "codex_provider_network_unavailable");
    }

    #[test]
    fn detects_codex_cli_missing_preflight() {
        let diagnostic = detect_provider_diagnostic(
            &codex_job(),
            "LIBRARIAN_DIAGNOSTIC codex_cli_missing: codex is not installed",
        )
        .expect("diagnostic");
        assert_eq!(diagnostic["code"], "codex_cli_missing");
    }

    #[test]
    fn detects_claude_cli_missing_preflight() {
        let diagnostic = detect_provider_diagnostic(
            &claude_job(),
            "LIBRARIAN_DIAGNOSTIC claude_cli_missing: claude is not installed",
        )
        .expect("diagnostic");
        assert_eq!(diagnostic["code"], "claude_cli_missing");
    }

    #[test]
    fn detects_claude_instruction_missing() {
        let diagnostic = detect_provider_diagnostic(
            &claude_job(),
            "LIBRARIAN_DIAGNOSTIC claude_instruction_missing: /workspace/project/CLAUDE.md is not mounted",
        )
        .expect("diagnostic");
        assert_eq!(diagnostic["code"], "claude_instruction_missing");
    }

    #[test]
    fn detects_claude_login_required() {
        let diagnostic = detect_provider_diagnostic(
            &claude_job(),
            "Authentication required: please log in to Claude Code",
        )
        .expect("diagnostic");
        assert_eq!(diagnostic["code"], "claude_login_required");
    }

    async fn test_config_and_db(name: &str) -> (Config, Database, PathBuf) {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-{}-{name}", Uuid::new_v4()));
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        (config, db, home)
    }

    #[tokio::test]
    async fn selects_configured_fallback_when_provider_paused() {
        let (mut config, db, home) = test_config_and_db("fallback").await;
        config.routing.fallback_enabled = true;
        config.routing.fallback_order = vec![
            "codex".to_string(),
            "openrouter".to_string(),
            "claude-code".to_string(),
        ];
        db.set_provider_pause(
            "codex",
            Some("codex-cli-default"),
            Utc::now() + chrono::Duration::minutes(30),
            "test pause",
        )
        .await
        .expect("pause");

        let job = codex_job();
        let selection = select_provider_for_job(&config, &db, &job)
            .await
            .expect("selection");
        assert_eq!(selection.provider, ProviderKind::OpenRouter);
        std::fs::remove_dir_all(home).ok();
    }

    #[tokio::test]
    async fn blocks_dispatch_when_provider_budget_is_spent() {
        let (mut config, db, home) = test_config_and_db("budget").await;
        config.budget.enabled = true;
        config.budget.daily_provider_usd = Some(1.0);
        let project_dir = home.join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let project = db
            .add_project("budget-project", &project_dir)
            .await
            .expect("project");
        let job = db
            .create_job(
                project.id,
                ProviderKind::Codex,
                "test",
                MountMode::ReadWrite,
                NetworkMode::None,
                None,
            )
            .await
            .expect("job");
        db.add_usage_observation(
            "codex",
            Some("codex-cli-default"),
            Some(job.id),
            None,
            None,
            Some(2.0),
            false,
            serde_json::json!({ "source": "test" }),
        )
        .await
        .expect("usage");

        let error = ensure_budget_available(&config, &db, &job)
            .await
            .expect_err("budget should block");
        assert!(error.to_string().contains("daily_provider"));
        std::fs::remove_dir_all(home).ok();
    }

    #[tokio::test]
    async fn scheduled_agent_task_preserves_provider_selection() {
        let (config, db, home) = test_config_and_db("schedule-provider").await;
        let project_dir = home.join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        db.add_project("scheduled-project", &project_dir)
            .await
            .expect("project");
        let schedule = db
            .add_schedule(
                "test.agent",
                ScheduleKind::AgentTask,
                3600,
                serde_json::json!({
                    "project": "scheduled-project",
                    "goal": "test scheduled provider",
                    "provider": "claude-code",
                    "secret_grant_token": "test-token",
                }),
            )
            .await
            .expect("schedule");

        scheduler::run_schedule_now(&db, &config, schedule.id)
            .await
            .expect("run schedule");
        let jobs = db.list_jobs().await.expect("jobs");
        assert!(jobs
            .iter()
            .any(|job| matches!(job.provider, ProviderKind::ClaudeCode)));
        let job = jobs
            .iter()
            .find(|job| matches!(job.provider, ProviderKind::ClaudeCode))
            .expect("scheduled job");
        assert_eq!(job.secret_grant_token.as_deref(), Some("test-token"));
        assert!(matches!(job.network_mode, NetworkMode::Open));
        std::fs::remove_dir_all(home).ok();
    }
}
