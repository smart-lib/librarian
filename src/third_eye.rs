use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row,
};

use crate::config::Config;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ThirdEyeHealth {
    pub reachable: bool,
    pub api_ok: bool,
    pub detail: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ThirdEyeDbSummary {
    pub db_path: String,
    pub api_calls: i64,
    pub projects: i64,
    pub tool_events: i64,
    pub agent_sessions: i64,
    pub codex_plan_days: i64,
    pub total_cost_usd: f64,
}

pub async fn health(config: &Config) -> Result<ThirdEyeHealth> {
    let url = format!(
        "{}/api/health",
        config.third_eye.base_url.trim_end_matches('/')
    );
    let response = reqwest::get(url).await;
    match response {
        Ok(response) => {
            let status_ok = response.status().is_success();
            let detail = response
                .json::<serde_json::Value>()
                .await
                .unwrap_or_else(|_| serde_json::json!({}));
            Ok(ThirdEyeHealth {
                reachable: true,
                api_ok: status_ok,
                detail,
            })
        }
        Err(error) => Ok(ThirdEyeHealth {
            reachable: false,
            api_ok: false,
            detail: serde_json::json!({ "error": error.to_string() }),
        }),
    }
}

pub async fn refresh(
    config: &Config,
    since: Option<&str>,
    full: bool,
) -> Result<serde_json::Value> {
    let mut url = reqwest::Url::parse(&format!(
        "{}/api/refresh",
        config.third_eye.base_url.trim_end_matches('/')
    ))?;
    if let Some(since) = since {
        url.query_pairs_mut().append_pair("since", since);
    }
    if full {
        url.query_pairs_mut().append_pair("mode", "full");
    }
    Ok(reqwest::Client::new()
        .post(url)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?)
}

pub async fn providers(config: &Config) -> Result<serde_json::Value> {
    let url = format!(
        "{}/api/providers",
        config.third_eye.base_url.trim_end_matches('/')
    );
    Ok(reqwest::get(url).await?.json::<serde_json::Value>().await?)
}

pub async fn db_summary(config: &Config) -> Result<Option<ThirdEyeDbSummary>> {
    let Some(path) = &config.third_eye.db_path else {
        return Ok(None);
    };
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let api_calls = count_table(&pool, "api_calls").await?;
    let projects = count_table(&pool, "projects").await?;
    let tool_events = count_table(&pool, "tool_events").await?;
    let agent_sessions = count_table(&pool, "agent_sessions").await?;
    let codex_plan_days = count_table(&pool, "codex_plan_daily").await?;
    let total_cost_usd = sqlx::query("SELECT COALESCE(SUM(cost_usd), 0.0) AS cost FROM api_calls")
        .fetch_one(&pool)
        .await?
        .get("cost");
    Ok(Some(ThirdEyeDbSummary {
        db_path: path.display().to_string(),
        api_calls,
        projects,
        tool_events,
        agent_sessions,
        codex_plan_days,
        total_cost_usd,
    }))
}

async fn count_table(pool: &sqlx::SqlitePool, table: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) AS count FROM {table}");
    Ok(sqlx::query(&sql).fetch_one(pool).await?.get("count"))
}
