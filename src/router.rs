use anyhow::{bail, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
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
    let provider = provider_name(&job.provider);
    let model = default_model(&job.provider);
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
