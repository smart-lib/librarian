use anyhow::{bail, Result};
use chrono::{Duration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    db::Database,
    domain::{Job, ProviderKind},
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
    use chrono::Utc;
    use uuid::Uuid;

    fn codex_job() -> Job {
        Job {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            provider: ProviderKind::Codex,
            status: crate::domain::JobStatus::Running,
            goal: "test".to_string(),
            mount_mode: crate::domain::MountMode::ReadWrite,
            network_mode: crate::domain::NetworkMode::Open,
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
    fn detects_codex_cli_missing_preflight() {
        let diagnostic = detect_provider_diagnostic(
            &codex_job(),
            "LIBRARIAN_DIAGNOSTIC codex_cli_missing: codex is not installed",
        )
        .expect("diagnostic");
        assert_eq!(diagnostic["code"], "codex_cli_missing");
    }
}
