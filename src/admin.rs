use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Path as AxumPath, State},
    response::{Html, IntoResponse},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

use crate::{
    config::Config,
    db::Database,
    domain::{JobStatus, MemoryKind, MountMode, NetworkMode, ScheduleKind, ScheduleStatus},
    gates, memory, router, scheduler,
    secrets::SecretVault,
    third_eye, worker,
};

#[derive(Clone)]
struct AppState {
    db: Database,
    config: Arc<RwLock<Config>>,
}

#[derive(Debug, Deserialize)]
struct CreateJobRequest {
    project: String,
    goal: String,
    provider: Option<String>,
    allow_network: Option<bool>,
    read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreateScheduleRequest {
    name: String,
    kind: String,
    every_seconds: i64,
    project: Option<String>,
    goal: Option<String>,
    provider: Option<String>,
    message: Option<String>,
    allow_network: Option<bool>,
    read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateWorkerRequest {
    max_concurrent_jobs: usize,
}

#[derive(Debug, Deserialize)]
struct UpdateRoutingRequest {
    fallback_enabled: bool,
    fallback_order: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateBudgetRequest {
    enabled: bool,
    daily_total_usd: Option<f64>,
    daily_provider_usd: Option<f64>,
    daily_project_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CreateSecretRequest {
    name: String,
    provider: String,
    kind: Option<String>,
    value: String,
}

#[derive(Debug, Deserialize)]
struct CreateSecretGrantRequest {
    secret: String,
    provider: Option<String>,
    capability: Option<String>,
    ttl_seconds: Option<i64>,
    max_uses: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ProviderControlRequest {
    provider: String,
    model: Option<String>,
    seconds: Option<i64>,
    reason: Option<String>,
}

pub async fn serve(bind: String, db: Database, config: Config) -> Result<()> {
    let state = AppState {
        db,
        config: Arc::new(RwLock::new(config)),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/projects", get(projects))
        .route("/api/jobs", get(jobs).post(create_job))
        .route("/api/schedules", get(schedules).post(create_schedule))
        .route("/api/settings/worker", post(update_worker_settings))
        .route("/api/settings/routing", post(update_routing_settings))
        .route("/api/settings/budget", post(update_budget_settings))
        .route("/api/secrets", get(secrets).post(create_secret))
        .route(
            "/api/secrets/grants",
            get(secret_grants).post(create_secret_grant),
        )
        .route("/api/secrets/audit", get(secret_audit))
        .route("/api/system-events", get(system_events))
        .route("/api/providers", get(providers_status))
        .route("/api/providers/pause", post(pause_provider))
        .route("/api/providers/resume", post(resume_provider))
        .route("/api/usage", get(usage_observations))
        .route("/api/third-eye", get(third_eye_status))
        .route("/api/jobs/:id", get(job))
        .route("/api/jobs/:id/events", get(job_events))
        .route("/api/jobs/:id/preflight", post(preflight_job))
        .route("/api/jobs/:id/cancel", post(cancel_job))
        .route("/api/jobs/:id/retry", post(retry_job))
        .route("/api/schedules/:id/enable", post(enable_schedule))
        .route("/api/schedules/:id/disable", post(disable_schedule))
        .route("/api/schedules/:id/run", post(run_schedule))
        .route(
            "/api/schedules/:id",
            patch(update_schedule).delete(delete_schedule),
        )
        .route("/api/chat", post(create_job))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("Librarian admin UI listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    Html(index_html(
        &config.admin.bind,
        config.worker.max_concurrent_jobs,
    ))
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let jobs = state.db.list_jobs().await.unwrap_or_default();
    let config = state.config.read().await;
    let memory_items = state.db.count_memory_items().await.unwrap_or_default();
    let memory_embeddings = state
        .db
        .count_memory_embeddings(&config.memory.embedding_model)
        .await
        .unwrap_or_default();
    let missing_embeddings = state
        .db
        .count_memory_missing_embedding(&config.memory.embedding_model)
        .await
        .unwrap_or_default();
    let provider_states = state.db.list_provider_states().await.unwrap_or_default();
    let running_jobs = jobs
        .iter()
        .filter(|job| matches!(job.status, JobStatus::Preparing | JobStatus::Running))
        .count();
    let queued_jobs = jobs
        .iter()
        .filter(|job| matches!(job.status, JobStatus::Queued))
        .count();
    let max_concurrent_jobs = config.worker.max_concurrent_jobs;
    let available_slots = max_concurrent_jobs.saturating_sub(running_jobs);
    Json(serde_json::json!({
        "ok": true,
        "worker": {
            "max_concurrent_jobs": max_concurrent_jobs,
            "running_jobs": running_jobs,
            "queued_jobs": queued_jobs,
            "available_slots": available_slots,
        },
        "routing": {
            "fallback_enabled": config.routing.fallback_enabled,
            "fallback_order": config.routing.fallback_order,
        },
        "budget": {
            "enabled": config.budget.enabled,
            "daily_total_usd": config.budget.daily_total_usd,
            "daily_provider_usd": config.budget.daily_provider_usd,
            "daily_project_usd": config.budget.daily_project_usd,
        },
        "memory": {
            "embedding_backend": config.memory.embedding_backend,
            "embedding_model": config.memory.embedding_model,
            "embedding_dimensions": config.memory.embedding_dimensions,
            "items": memory_items,
            "embedded_items": memory_embeddings,
            "missing_embeddings": missing_embeddings,
        },
        "secrets": SecretVault::new(config.clone()).encryption_status(),
        "providers": provider_states,
        "container_runtime": config.docker.runtime_command,
    }))
}

async fn projects(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_projects().await?))
}

async fn jobs(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_jobs().await?))
}

async fn job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.get_job(id).await?))
}

async fn schedules(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_schedules().await?))
}

async fn create_schedule(
    State(state): State<AppState>,
    Json(input): Json<CreateScheduleRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let kind = parse_schedule_kind(&input.kind)?;
    let payload = schedule_payload(&kind, &input);
    let schedule = state
        .db
        .add_schedule(&input.name, kind, input.every_seconds.max(1), payload)
        .await?;
    state
        .db
        .add_system_event(
            "schedule_created",
            serde_json::json!({
                "schedule_id": schedule.id,
                "name": schedule.name,
                "kind": schedule.kind,
            }),
        )
        .await?;
    Ok(Json(schedule))
}

async fn update_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(input): Json<CreateScheduleRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let kind = parse_schedule_kind(&input.kind)?;
    let payload = schedule_payload(&kind, &input);
    let schedule = state
        .db
        .update_schedule(id, &input.name, kind, input.every_seconds.max(1), payload)
        .await?;
    state
        .db
        .add_system_event(
            "schedule_updated",
            serde_json::json!({ "schedule_id": schedule.id, "name": schedule.name }),
        )
        .await?;
    Ok(Json(schedule))
}

async fn delete_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let schedule = state.db.get_schedule(id).await?;
    state.db.delete_schedule(id).await?;
    state
        .db
        .add_system_event(
            "schedule_deleted",
            serde_json::json!({ "schedule_id": id, "name": schedule.name }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "schedule_id": id })))
}

async fn system_events(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_system_events(50).await?))
}

async fn providers_status(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let states = state.db.list_provider_states().await?;
    let catalog = router::model_catalog();
    Ok(Json(serde_json::json!({
        "catalog": catalog,
        "states": states,
    })))
}

async fn usage_observations(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_usage_observations(50).await?))
}

async fn third_eye_status(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let health = third_eye::health(&config).await?;
    let db_summary = third_eye::db_summary(&config).await?;
    Ok(Json(serde_json::json!({
        "enabled": config.third_eye.enabled,
        "base_url": config.third_eye.base_url,
        "db_path": config.third_eye.db_path,
        "project_export_dir": config.third_eye.project_export_dir,
        "health": health,
        "db_summary": db_summary,
    })))
}

async fn pause_provider(
    State(state): State<AppState>,
    Json(input): Json<ProviderControlRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let seconds = input.seconds.unwrap_or(1800).max(1);
    let reason = input
        .reason
        .unwrap_or_else(|| "manual admin pause".to_string());
    let paused_until = chrono::Utc::now() + chrono::Duration::seconds(seconds);
    let provider = state
        .db
        .set_provider_pause(
            &input.provider,
            input.model.as_deref(),
            paused_until,
            &reason,
        )
        .await?;
    state
        .db
        .add_system_event(
            "provider_paused",
            serde_json::json!({
                "provider": provider.provider,
                "model": provider.model,
                "paused_until": provider.paused_until,
                "reason": provider.reason,
            }),
        )
        .await?;
    Ok(Json(provider))
}

async fn resume_provider(
    State(state): State<AppState>,
    Json(input): Json<ProviderControlRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let provider = state
        .db
        .resume_provider(&input.provider, input.model.as_deref())
        .await?;
    state
        .db
        .add_system_event(
            "provider_resumed",
            serde_json::json!({
                "provider": provider.provider,
                "model": provider.model,
            }),
        )
        .await?;
    Ok(Json(provider))
}

async fn secrets(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let records = state.db.list_secret_records().await?;
    let redacted = records
        .into_iter()
        .map(|record| {
            serde_json::json!({
                "id": record.id,
                "name": record.name,
                "provider": record.provider,
                "kind": record.kind,
                "encryption": record.encryption,
                "created_at": record.created_at,
                "updated_at": record.updated_at,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(redacted))
}

async fn create_secret(
    State(state): State<AppState>,
    Json(input): Json<CreateSecretRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let vault = SecretVault::new(config);
    let record = vault
        .store(
            &state.db,
            &input.name,
            &input.provider,
            input.kind.as_deref().unwrap_or("api-key"),
            &input.value,
        )
        .await?;
    Ok(Json(serde_json::json!({
        "id": record.id,
        "name": record.name,
        "provider": record.provider,
        "kind": record.kind,
        "encryption": record.encryption,
    })))
}

async fn create_secret_grant(
    State(state): State<AppState>,
    Json(input): Json<CreateSecretGrantRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let vault = SecretVault::new(config);
    let grant_id = vault
        .grant(
            &state.db,
            &input.secret,
            None,
            input.provider.as_deref(),
            input.capability.as_deref().unwrap_or("read"),
            input.ttl_seconds.unwrap_or(900),
            input.max_uses.unwrap_or(1),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "grant_id": grant_id,
        "token": crate::secrets::encode_grant_token(grant_id),
    })))
}

async fn secret_grants(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_secret_grants(50).await?))
}

async fn secret_audit(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_secret_audit_events(50).await?))
}

async fn job_events(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_job_events(id).await?))
}

async fn preflight_job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    Ok(Json(
        worker::preflight_job(config, state.db.clone(), id).await?,
    ))
}

async fn cancel_job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.db.request_cancel_job(id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "job_id": id })))
}

async fn retry_job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let retry = state.db.retry_job(id).await?;
    Ok(Json(retry))
}

async fn enable_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let schedule = state
        .db
        .set_schedule_status(id, ScheduleStatus::Enabled)
        .await?;
    state
        .db
        .add_system_event(
            "schedule_enabled",
            serde_json::json!({ "schedule_id": schedule.id, "name": schedule.name }),
        )
        .await?;
    Ok(Json(schedule))
}

async fn disable_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let schedule = state
        .db
        .set_schedule_status(id, ScheduleStatus::Disabled)
        .await?;
    state
        .db
        .add_system_event(
            "schedule_disabled",
            serde_json::json!({ "schedule_id": schedule.id, "name": schedule.name }),
        )
        .await?;
    Ok(Json(schedule))
}

async fn run_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    scheduler::run_schedule_now(&state.db, &config, id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "schedule_id": id })))
}

async fn update_worker_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateWorkerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (max_concurrent_jobs, config_path) = {
        let mut config = state.config.write().await;
        config.set_worker_concurrency(input.max_concurrent_jobs);
        config.save()?;
        (
            config.worker.max_concurrent_jobs,
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "worker_settings_updated",
            serde_json::json!({
                "max_concurrent_jobs": max_concurrent_jobs,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "worker": {
            "max_concurrent_jobs": max_concurrent_jobs,
        },
    })))
}

async fn update_routing_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateRoutingRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if input.fallback_order.is_empty() {
        return Err(anyhow::anyhow!("fallback_order must include at least one provider").into());
    }
    for provider in &input.fallback_order {
        router::parse_provider_kind(provider)?;
    }
    let (fallback_enabled, fallback_order, config_path) = {
        let mut config = state.config.write().await;
        config.routing.fallback_enabled = input.fallback_enabled;
        config.routing.fallback_order = input.fallback_order;
        config.save()?;
        (
            config.routing.fallback_enabled,
            config.routing.fallback_order.clone(),
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "routing_settings_updated",
            serde_json::json!({
                "fallback_enabled": fallback_enabled,
                "fallback_order": fallback_order,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "routing": {
            "fallback_enabled": fallback_enabled,
            "fallback_order": fallback_order,
        },
    })))
}

async fn update_budget_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateBudgetRequest>,
) -> Result<impl IntoResponse, ApiError> {
    for (label, value) in [
        ("daily_total_usd", input.daily_total_usd),
        ("daily_provider_usd", input.daily_provider_usd),
        ("daily_project_usd", input.daily_project_usd),
    ] {
        if let Some(value) = value {
            if value < 0.0 {
                return Err(anyhow::anyhow!("{label} must be non-negative").into());
            }
        }
    }

    let (enabled, daily_total_usd, daily_provider_usd, daily_project_usd, config_path) = {
        let mut config = state.config.write().await;
        config.budget.enabled = input.enabled;
        config.budget.daily_total_usd = input.daily_total_usd;
        config.budget.daily_provider_usd = input.daily_provider_usd;
        config.budget.daily_project_usd = input.daily_project_usd;
        config.save()?;
        (
            config.budget.enabled,
            config.budget.daily_total_usd,
            config.budget.daily_provider_usd,
            config.budget.daily_project_usd,
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "budget_settings_updated",
            serde_json::json!({
                "enabled": enabled,
                "daily_total_usd": daily_total_usd,
                "daily_provider_usd": daily_provider_usd,
                "daily_project_usd": daily_project_usd,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "budget": {
            "enabled": enabled,
            "daily_total_usd": daily_total_usd,
            "daily_provider_usd": daily_provider_usd,
            "daily_project_usd": daily_project_usd,
        },
    })))
}

async fn create_job(
    State(state): State<AppState>,
    Json(input): Json<CreateJobRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let project = state.db.get_project_by_name_or_id(&input.project).await?;
    let mount_mode = if input.read_only.unwrap_or(false) {
        MountMode::ReadOnly
    } else {
        MountMode::ReadWrite
    };
    let network_mode = if input.allow_network.unwrap_or(false) {
        NetworkMode::Open
    } else {
        NetworkMode::None
    };
    let config = state.config.read().await.clone();
    let gated = gates::process_user_prompt(&state.db, &config, &input.goal, "admin-chat").await?;
    let user_memory = state
        .db
        .add_memory_item(
            Some(project.id),
            None,
            MemoryKind::UserMessage,
            Some("admin-chat"),
            &gated.content,
            Some("admin:chat"),
            serde_json::json!({ "project": project.name.clone() }),
        )
        .await?;
    memory::embed_item(&state.db, &config, &user_memory).await?;
    let context_pack = memory::retrieve_context_with_config(
        &state.db,
        Some(&config),
        memory::RetrievalRequest {
            query: gated.content.clone(),
            project_id: Some(project.id),
            activity_id: None,
            limit: memory::default_hit_limit(),
        },
    )
    .await?;
    let job = state
        .db
        .create_job(
            project.id,
            router::parse_provider_kind(input.provider.as_deref().unwrap_or("codex"))?,
            &gated.content,
            mount_mode,
            network_mode,
        )
        .await?;
    state
        .db
        .add_job_event(
            job.id,
            "context_pack",
            serde_json::json!({
                "query": context_pack.query,
                "generated_at": context_pack.generated_at,
                "hits": context_pack.hits,
            }),
        )
        .await?;
    if !gated.events.is_empty() {
        state
            .db
            .add_job_event(
                job.id,
                "gate_events",
                serde_json::json!({ "events": gated.events }),
            )
            .await?;
    }
    Ok(Json(job))
}

fn parse_schedule_kind(value: &str) -> Result<ScheduleKind> {
    match value {
        "System" | "system" => Ok(ScheduleKind::System),
        "Reminder" | "reminder" => Ok(ScheduleKind::Reminder),
        "AgentTask" | "agent-task" | "agent_task" => Ok(ScheduleKind::AgentTask),
        _ => anyhow::bail!("Unknown schedule kind `{value}`"),
    }
}

fn schedule_payload(kind: &ScheduleKind, input: &CreateScheduleRequest) -> serde_json::Value {
    match kind {
        ScheduleKind::System => serde_json::json!({
            "task": input.message.clone().unwrap_or_else(|| "custom_system_task".to_string()),
        }),
        ScheduleKind::Reminder => serde_json::json!({
            "message": input.message.clone().unwrap_or_default(),
        }),
        ScheduleKind::AgentTask => serde_json::json!({
            "project": input.project.clone().unwrap_or_default(),
            "goal": input.goal.clone().unwrap_or_default(),
            "provider": input.provider.clone().unwrap_or_else(|| "codex".to_string()),
            "allow_network": input.allow_network.unwrap_or(false),
            "read_only": input.read_only.unwrap_or(false),
        }),
    }
}

#[derive(Debug)]
struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(value: E) -> Self {
        Self(value.into())
    }
}

fn index_html(bind: &str, worker_concurrency: usize) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Librarian</title>
  <style>
    :root {{
      color-scheme: light dark;
      --bg: #101214;
      --panel: #181c20;
      --text: #edf1f5;
      --muted: #9aa8b6;
      --line: #303841;
      --accent: #58c4a5;
      --warn: #f0b35a;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: var(--bg);
      color: var(--text);
    }}
    header {{
      height: 56px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 20px;
      border-bottom: 1px solid var(--line);
      background: #15191d;
    }}
    main {{
      display: grid;
      grid-template-columns: minmax(280px, 360px) minmax(0, 1fr);
      min-height: calc(100vh - 56px);
    }}
    aside {{
      border-right: 1px solid var(--line);
      padding: 16px;
      overflow: auto;
    }}
    section {{
      padding: 18px;
      display: grid;
      grid-template-rows: 1fr auto;
      gap: 14px;
      min-width: 0;
    }}
    h1 {{ font-size: 18px; margin: 0; }}
    h2 {{ font-size: 13px; color: var(--muted); text-transform: uppercase; margin: 18px 0 8px; }}
    label {{ display: block; font-size: 13px; color: var(--muted); margin-bottom: 6px; }}
    input, textarea, select, button {{
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: var(--panel);
      color: var(--text);
      font: inherit;
    }}
    input {{ height: 38px; padding: 0 10px; }}
    select {{ height: 38px; padding: 0 10px; }}
    textarea {{ min-height: 104px; resize: vertical; padding: 10px; }}
    button {{
      height: 38px;
      cursor: pointer;
      background: var(--accent);
      color: #06100d;
      border-color: transparent;
      font-weight: 650;
    }}
    .row {{ display: flex; gap: 8px; align-items: center; margin: 10px 0; }}
    .row input[type="checkbox"] {{ width: 18px; height: 18px; }}
    .grid-2 {{ display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }}
    .log {{
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
      padding: 14px;
      overflow: auto;
      min-height: 260px;
      white-space: pre-wrap;
    }}
    .item {{
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 10px;
      margin-bottom: 8px;
      background: var(--panel);
    }}
    .action {{
      border-left: 3px solid var(--accent);
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
      line-height: 1.45;
    }}
    .actions {{
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 6px;
      margin-top: 8px;
    }}
    .actions button {{
      height: 32px;
      font-size: 12px;
    }}
    .mini {{
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 6px;
      margin-top: 8px;
    }}
    .pill {{
      display: inline-block;
      border: 1px solid var(--line);
      border-radius: 999px;
      padding: 2px 8px;
      margin: 2px 4px 2px 0;
      color: var(--muted);
      font-size: 12px;
    }}
    details {{
      margin-top: 8px;
    }}
    summary {{
      cursor: pointer;
      color: var(--accent);
    }}
    pre {{
      overflow: auto;
      white-space: pre-wrap;
      margin: 8px 0 0;
      color: var(--muted);
    }}
    .secondary {{
      background: #27313a;
      color: var(--text);
      border-color: var(--line);
    }}
    .danger {{
      background: #8e4f4f;
      color: #fff;
    }}
    .muted {{ color: var(--muted); }}
    @media (max-width: 820px) {{
      main {{ grid-template-columns: 1fr; }}
      aside {{ border-right: 0; border-bottom: 1px solid var(--line); }}
    }}
  </style>
</head>
<body>
  <header>
    <h1>Librarian</h1>
    <span class="muted">localhost {bind}</span>
  </header>
  <main>
    <aside>
      <h2>Worker</h2>
      <div id="worker" class="item">Loading...</div>
      <h2>Memory</h2>
      <div id="memory" class="item">Loading...</div>
      <h2>Providers</h2>
      <div id="providers" class="muted">Loading...</div>
      <h2>Usage</h2>
      <div id="usage" class="muted">Loading...</div>
      <h2>Third Eye</h2>
      <div id="third-eye" class="item">Loading...</div>
      <h2>Secrets</h2>
      <div id="secrets" class="muted">Loading...</div>
      <div id="secret-grants" class="muted">Loading...</div>
      <form id="secret-form" class="item">
        <label for="secret_name">Secret name</label>
        <input id="secret_name" autocomplete="off" placeholder="openrouter.default">
        <label for="secret_provider">Provider</label>
        <input id="secret_provider" autocomplete="off" placeholder="openrouter">
        <label for="secret_kind">Kind</label>
        <input id="secret_kind" autocomplete="off" value="api-key">
        <label for="secret_value">Value</label>
        <input id="secret_value" type="password" autocomplete="off">
        <button type="submit">Store Secret</button>
      </form>
      <form id="grant-form" class="item">
        <label for="grant_secret">Secret name or id</label>
        <input id="grant_secret" autocomplete="off" placeholder="openrouter.default">
        <label for="grant_provider">Provider</label>
        <input id="grant_provider" autocomplete="off" placeholder="openrouter">
        <div class="grid-2">
          <div>
            <label for="grant_capability">Capability</label>
            <input id="grant_capability" autocomplete="off" value="provider-proxy">
          </div>
          <div>
            <label for="grant_ttl">TTL seconds</label>
            <input id="grant_ttl" type="number" min="1" value="900">
          </div>
        </div>
        <label for="grant_max_uses">Max uses</label>
        <input id="grant_max_uses" type="number" min="1" value="1">
        <button type="submit">Create Grant</button>
      </form>
      <form id="worker-form" class="item">
        <label for="worker_concurrency">Max concurrent jobs</label>
        <div class="grid-2">
          <input id="worker_concurrency" type="number" min="1" value="{worker_concurrency}">
          <button type="submit">Save</button>
        </div>
      </form>
      <form id="routing-form" class="item">
        <div class="row">
          <input id="fallback_enabled" type="checkbox">
          <label for="fallback_enabled">Use fallback provider when paused</label>
        </div>
        <label for="fallback_order">Fallback order</label>
        <input id="fallback_order" autocomplete="off" value="codex, openrouter, claude-code">
        <button type="submit">Save Routing</button>
      </form>
      <form id="budget-form" class="item">
        <div class="row">
          <input id="budget_enabled" type="checkbox">
          <label for="budget_enabled">Enforce daily budget guardrails</label>
        </div>
        <div class="grid-2">
          <div>
            <label for="budget_total">Total USD/day</label>
            <input id="budget_total" type="number" min="0" step="0.01">
          </div>
          <div>
            <label for="budget_provider">Provider USD/day</label>
            <input id="budget_provider" type="number" min="0" step="0.01">
          </div>
        </div>
        <label for="budget_project">Project USD/day</label>
        <input id="budget_project" type="number" min="0" step="0.01">
        <button type="submit">Save Budget</button>
      </form>
      <h2>Projects</h2>
      <div id="projects" class="muted">Loading...</div>
      <h2>Jobs</h2>
      <div id="jobs" class="muted">Loading...</div>
      <h2>Schedules</h2>
      <div id="schedules" class="muted">Loading...</div>
      <form id="schedule-form" class="item">
        <label for="schedule_name">Schedule name</label>
        <input id="schedule_name" autocomplete="off" placeholder="daily.status">
        <div class="grid-2">
          <div>
            <label for="schedule_kind">Kind</label>
            <select id="schedule_kind">
              <option value="reminder">Reminder</option>
              <option value="agent-task">Agent task</option>
            </select>
          </div>
          <div>
            <label for="schedule_every">Every seconds</label>
            <input id="schedule_every" type="number" min="1" value="3600">
          </div>
        </div>
        <label for="schedule_message">Message</label>
        <input id="schedule_message" autocomplete="off">
        <label for="schedule_project">Project</label>
        <input id="schedule_project" autocomplete="off">
        <label for="schedule_provider">Provider</label>
        <select id="schedule_provider">
          <option value="codex">Codex</option>
          <option value="openrouter">OpenRouter</option>
          <option value="claude-code">Claude Code</option>
        </select>
        <label for="schedule_goal">Agent goal</label>
        <textarea id="schedule_goal"></textarea>
        <div class="row">
          <input id="schedule_network" type="checkbox">
          <label for="schedule_network">Allow network</label>
        </div>
        <div class="grid-2">
          <button type="submit">Save Schedule</button>
          <button type="button" class="secondary" onclick="resetScheduleForm()">Clear</button>
        </div>
      </form>
      <h2>Recent Actions</h2>
      <div id="system-events" class="muted">Loading...</div>
      <h2>Settings</h2>
      <div class="item">
        <div>Provider: Codex</div>
        <div class="muted">Network is disabled by default.</div>
        <div class="muted">Worker concurrency: {worker_concurrency}</div>
        <div class="muted">Auth bootstrap: run <code>librarian auth codex</code>.</div>
      </div>
    </aside>
    <section>
      <div class="log" id="output">Ready.</div>
      <form id="chat">
        <label for="project">Project name or id</label>
        <input id="project" name="project" autocomplete="off">
        <label for="provider">Provider</label>
        <select id="provider" name="provider">
          <option value="codex">Codex</option>
          <option value="openrouter">OpenRouter</option>
          <option value="claude-code">Claude Code</option>
        </select>
        <label for="goal">Goal</label>
        <textarea id="goal" name="goal"></textarea>
        <div class="row">
          <input id="allow_network" name="allow_network" type="checkbox">
          <label for="allow_network">Allow network for this session</label>
        </div>
        <button type="submit">Queue Agent Job</button>
      </form>
    </section>
  </main>
  <script>
    function escapeHtml(value) {{
      return String(value ?? '').replace(/[&<>"']/g, character => ({{
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;'
      }}[character]));
    }}
    function asJson(value) {{
      return escapeHtml(JSON.stringify(value, null, 2));
    }}
    function shortId(value) {{
      return String(value || '').slice(0, 8);
    }}
    async function load() {{
      const [health, projects, jobs, schedules, systemEvents, providers, usage, thirdEye, secrets, grants] = await Promise.all([
        fetch('/api/health').then(r => r.json()),
        fetch('/api/projects').then(r => r.json()),
        fetch('/api/jobs').then(r => r.json()),
        fetch('/api/schedules').then(r => r.json()),
        fetch('/api/system-events').then(r => r.json()),
        fetch('/api/providers').then(r => r.json()),
        fetch('/api/usage').then(r => r.json()),
        fetch('/api/third-eye').then(r => r.json()),
        fetch('/api/secrets').then(r => r.json()),
        fetch('/api/secrets/grants').then(r => r.json())
      ]);
      document.querySelector('#worker').innerHTML = `
        <b>${{health.worker.running_jobs}} / ${{health.worker.max_concurrent_jobs}}</b> slots used<br>
        <span class="muted">Queued: ${{health.worker.queued_jobs}} · Available: ${{health.worker.available_slots}}<br>Runtime: ${{health.container_runtime}}</span>
      `;
      document.querySelector('#memory').innerHTML = `
        <b>${{health.memory.embedded_items}} / ${{health.memory.items}}</b> embedded<br>
        <span class="muted">${{escapeHtml(health.memory.embedding_backend)}} &middot; ${{escapeHtml(health.memory.embedding_model)}} &middot; ${{health.memory.embedding_dimensions}}d<br>Missing: ${{health.memory.missing_embeddings}}</span>
      `;
      const stateByKey = new Map((providers.states || []).map(state => [`${{state.provider}}:${{state.model || ''}}`, state]));
      document.querySelector('#providers').innerHTML = providers.catalog.length
        ? providers.catalog.map(model => {{
            const state = stateByKey.get(`${{model.provider}}:${{model.model}}`) || stateByKey.get(`${{model.provider}}:`) || {{}};
            const paused = state.status === 'Paused';
            return `<div class="item">
              <b>${{escapeHtml(model.provider)}}</b><br>
              <span class="muted">${{escapeHtml(model.model)}} &middot; ${{escapeHtml(state.status || 'Ready')}}</span><br>
              ${{(model.task_hints || []).map(hint => `<span class="pill">${{escapeHtml(hint)}}</span>`).join('')}}
              ${{paused ? `<br><span class="muted">Paused until ${{escapeHtml(state.paused_until || '-')}}<br>${{escapeHtml(state.reason || '')}}</span>` : ''}}
              <div class="mini">
                <button type="button" class="secondary" onclick="pauseProvider('${{escapeHtml(model.provider)}}', '${{escapeHtml(model.model)}}')">Pause 30m</button>
                <button type="button" onclick="resumeProvider('${{escapeHtml(model.provider)}}', '${{escapeHtml(model.model)}}')">Resume</button>
              </div>
            </div>`;
          }}).join('')
        : 'No providers.';
      document.querySelector('#usage').innerHTML = usage.length
        ? usage.slice(0, 8).map(event => `<div class="item action">
            <b>${{escapeHtml(event.provider)}}</b> <span class="muted">${{escapeHtml(event.model || '-')}}</span><br>
            <span class="muted">${{escapeHtml(event.observed_at)}} &middot; job ${{escapeHtml(shortId(event.job_id) || '-')}}</span><br>
            input=${{event.input_tokens ?? '-'}} output=${{event.output_tokens ?? '-'}} cost=${{event.cost_usd ?? '-'}} limit=${{event.limit_event}}
          </div>`).join('')
        : 'No usage observations.';
      document.querySelector('#third-eye').innerHTML = `
        <b>${{thirdEye.enabled ? 'Enabled' : 'Disabled'}}</b><br>
        <span class="muted">${{escapeHtml(thirdEye.base_url)}}<br>
        API: ${{thirdEye.health.reachable ? 'reachable' : 'offline'}} / ${{thirdEye.health.api_ok ? 'ok' : 'not ok'}}<br>
        DB: ${{thirdEye.db_summary ? `${{thirdEye.db_summary.api_calls}} calls, $${{Number(thirdEye.db_summary.total_cost_usd || 0).toFixed(4)}}` : 'not configured'}}</span>
      `;
      document.querySelector('#secrets').innerHTML = secrets.length
        ? secrets.slice(0, 8).map(secret => `<div class="item">
            <b>${{escapeHtml(secret.name)}}</b><br>
            <span class="muted">${{escapeHtml(secret.provider)}} &middot; ${{escapeHtml(secret.kind)}} &middot; ${{escapeHtml(secret.encryption)}}<br>${{escapeHtml(secret.updated_at)}}</span>
          </div>`).join('')
        : 'No secrets stored.';
      document.querySelector('#secret-grants').innerHTML = grants.length
        ? grants.slice(0, 6).map(grant => `<div class="item">
            <b>${{escapeHtml(shortId(grant.id))}}</b> <span class="muted">${{escapeHtml(grant.provider || '-')}}</span><br>
            <span class="muted">capability=${{escapeHtml(grant.capability)}} uses=${{grant.uses}}/${{grant.max_uses}} expires=${{escapeHtml(grant.expires_at)}}</span>
          </div>`).join('')
        : 'No active grants listed.';
      worker_concurrency.value = health.worker.max_concurrent_jobs;
      fallback_enabled.checked = Boolean(health.routing.fallback_enabled);
      fallback_order.value = (health.routing.fallback_order || []).join(', ');
      budget_enabled.checked = Boolean(health.budget.enabled);
      budget_total.value = health.budget.daily_total_usd ?? '';
      budget_provider.value = health.budget.daily_provider_usd ?? '';
      budget_project.value = health.budget.daily_project_usd ?? '';
      document.querySelector('#projects').innerHTML = projects.length
        ? projects.map(p => `<div class="item"><b>${{escapeHtml(p.name)}}</b><br><span class="muted">${{escapeHtml(p.path)}}</span></div>`).join('')
        : 'No projects registered.';
      document.querySelector('#jobs').innerHTML = renderJobs(jobs);
      document.querySelector('#schedules').innerHTML = schedules.length
        ? schedules.map(s => `<div class="item"><b>${{s.name}}</b><br><span class="muted">${{s.kind}} · ${{s.status}} · every ${{s.interval_seconds}}s<br>Next: ${{s.next_run_at}}</span><div class="actions"><button type="button" onclick="runSchedule('${{s.id}}')">Run</button><button type="button" class="secondary" onclick='editSchedule(${{JSON.stringify(s)}})'>Edit</button><button type="button" class="danger" onclick="deleteSchedule('${{s.id}}')">Delete</button><button type="button" class="secondary" onclick="enableSchedule('${{s.id}}')">Enable</button><button type="button" class="danger" onclick="disableSchedule('${{s.id}}')">Disable</button></div></div>`).join('')
        : 'No schedules.';
      document.querySelector('#system-events').innerHTML = systemEvents.length
        ? systemEvents.map(e => `<div class="item action"><b>${{escapeHtml(e.kind)}}</b><br><span class="muted">${{escapeHtml(e.created_at)}}</span><br><pre>${{asJson(e.payload)}}</pre></div>`).join('')
        : 'No actions recorded yet.';
    }}
    async function detailsFor(id) {{
      const [job, events] = await Promise.all([
        fetch(`/api/jobs/${{id}}`).then(r => r.json()),
        fetch(`/api/jobs/${{id}}/events`).then(r => r.json())
      ]);
      output.innerHTML = renderJobDetail(job, events);
    }}
    function renderJobDetail(job, events) {{
      return `<div class="item">
        <b>${{escapeHtml(job.status)}}</b> <span class="muted">${{escapeHtml(job.provider)}} &middot; ${{escapeHtml(job.id)}}</span><br>
        <div>${{escapeHtml(job.goal)}}</div>
        <div class="mini">
          <div><span class="muted">Created</span><br>${{escapeHtml(job.created_at)}}</div>
          <div><span class="muted">Started</span><br>${{escapeHtml(job.started_at || '-')}}</div>
          <div><span class="muted">Heartbeat</span><br>${{escapeHtml(job.last_heartbeat_at || '-')}}</div>
          <div><span class="muted">Finished</span><br>${{escapeHtml(job.finished_at || '-')}}</div>
        </div>
        <div class="actions">
          <button type="button" onclick="preflightJob('${{job.id}}')">Preflight</button>
          <button type="button" class="danger" onclick="cancelJob('${{job.id}}')">Cancel</button>
          <button type="button" onclick="retryJob('${{job.id}}')">Retry</button>
        </div>
      </div>${{renderJobEvents(events)}}`;
    }}
    function renderJobs(jobs) {{
      if (!jobs.length) {{
        return 'No jobs yet.';
      }}
      const groups = [
        ['Active', job => ['Preparing', 'Running', 'HeartbeatMissed', 'Recovering'].includes(job.status)],
        ['Queued', job => job.status === 'Queued'],
        ['Failed / Cancelled', job => ['Failed', 'Cancelled'].includes(job.status)],
        ['Completed', job => job.status === 'Completed']
      ];
      return groups.map(([label, predicate]) => {{
        const groupJobs = jobs.filter(predicate);
        if (!groupJobs.length) {{
          return '';
        }}
        return `<details open><summary>${{label}} (${{groupJobs.length}})</summary>` +
          groupJobs.map(renderJobCard).join('') +
          `</details>`;
      }}).join('') || 'No jobs yet.';
    }}
    function renderJobCard(j) {{
      return `<div class="item">
        <b>${{escapeHtml(j.status)}}</b> <span class="muted">${{escapeHtml(j.provider)}} &middot; ${{escapeHtml(shortId(j.id))}}</span><br>
        ${{escapeHtml(j.goal)}}<br>
        <span class="muted">Created: ${{escapeHtml(j.created_at)}}<br>Started: ${{escapeHtml(j.started_at || '-')}}<br>Heartbeat: ${{escapeHtml(j.last_heartbeat_at || '-')}}<br>Finished: ${{escapeHtml(j.finished_at || '-')}}</span>
        <div class="actions">
          <button type="button" class="secondary" onclick="detailsFor('${{j.id}}')">Details</button>
          <button type="button" onclick="preflightJob('${{j.id}}')">Preflight</button>
          <button type="button" class="danger" onclick="cancelJob('${{j.id}}')">Cancel</button>
          <button type="button" onclick="retryJob('${{j.id}}')">Retry</button>
        </div>
      </div>`;
    }}
    function renderJobEvents(events) {{
      if (!events.length) {{
        return 'No events for this job.';
      }}
      return events.map(event => {{
        const payload = event.payload || {{}};
        let body = '';
        if (event.kind === 'context_pack') {{
          const hits = payload.hits || [];
          body = `<div class="muted">Query: ${{escapeHtml(payload.query || '-')}}<br>Hits: ${{hits.length}}</div>` +
            hits.slice(0, 5).map(hit => `<details><summary>${{escapeHtml(hit.reason || 'memory hit')}} score=${{Number(hit.score || 0).toFixed(3)}}</summary><pre>${{asJson(hit.item || hit)}}</pre></details>`).join('');
        }} else if (event.kind === 'prepared') {{
          body = `<div class="muted">Context hits=${{payload.context_hits ?? 0}} &middot; prompt chars=${{payload.prompt_chars ?? 0}}</div>
            <details><summary>Prepared command</summary><pre>${{asJson(payload.command || [])}}</pre></details>
            <details><summary>Project note</summary><pre>${{escapeHtml(payload.project_note || '-')}}</pre></details>`;
        }} else if (event.kind === 'gate_events') {{
          body = (payload.events || []).map(gate => `<div><span class="pill">${{escapeHtml(gate.kind || gate.action || 'gate')}}</span><pre>${{asJson(gate)}}</pre></div>`).join('') || '<span class="muted">No gate changes.</span>';
        }} else if (event.kind === 'provider_fallback_selected') {{
          body = `<div><span class="pill">fallback</span> ${{escapeHtml(payload.from || '-')}} -> ${{escapeHtml(payload.to || '-')}}</div>
            <div class="muted">${{escapeHtml(payload.reason || '')}}</div>`;
        }} else if (event.kind === 'budget_checked') {{
          body = `<div><span class="pill">budget</span> checked</div><pre>${{asJson(payload.checks || [])}}</pre>`;
        }} else if (event.kind === 'budget_blocked' || event.kind === 'provider_paused') {{
          const category = payload.category || {{}};
          body = `<div><span class="pill">${{escapeHtml(category.severity || 'warn')}}</span> ${{escapeHtml(category.code || event.kind)}}</div>
            <div>${{escapeHtml(category.message || payload.error || '')}}</div>
            <div class="muted">${{escapeHtml(category.next_step || '')}}</div>`;
        }} else if (event.kind === 'provider_diagnostic') {{
          const diagnostic = payload.diagnostic || {{}};
          body = `<div><span class="pill">${{escapeHtml(diagnostic.severity || 'info')}}</span> ${{escapeHtml(diagnostic.code || 'provider_diagnostic')}}</div>
            <div>${{escapeHtml(diagnostic.message || '')}}</div>
            <div class="muted">${{escapeHtml(diagnostic.next_step || '')}}</div>
            <details><summary>Raw line</summary><pre>${{escapeHtml(payload.line || '')}}</pre></details>`;
        }} else if (event.kind === 'preflight') {{
          body = `<div><span class="pill">${{escapeHtml(payload.selected_provider || 'provider')}}</span> launched=${{Boolean(payload.launched)}}</div>
            <div class="muted">${{escapeHtml(payload.project_name || '-')}} &middot; context hits=${{payload.context_hits ?? 0}} &middot; prompt chars=${{payload.prompt_chars ?? 0}}</div>
            ${{payload.fallback_from ? `<div class="muted">Fallback: ${{escapeHtml(payload.fallback_from)}} -> ${{escapeHtml(payload.selected_provider)}}<br>${{escapeHtml(payload.fallback_reason || '')}}</div>` : ''}}
            <details><summary>Prepared command</summary><pre>${{asJson(payload.command || [])}}</pre></details>
            <details><summary>Budget checks</summary><pre>${{asJson(payload.budget_checks || [])}}</pre></details>`;
        }} else if (event.kind === 'failure_category') {{
          const category = payload.category || {{}};
          body = `<div><span class="pill">${{escapeHtml(category.severity || 'error')}}</span> ${{escapeHtml(category.code || 'unknown_failure')}}</div>
            <div>${{escapeHtml(category.message || '')}}</div>
            <div class="muted">${{escapeHtml(category.next_step || '')}}</div>
            ${{payload.exit_code !== undefined ? `<div class="muted">Exit code: ${{payload.exit_code}}</div>` : ''}}
            ${{payload.line ? `<details><summary>Matched line</summary><pre>${{escapeHtml(payload.line)}}</pre></details>` : ''}}`;
        }} else if (event.kind === 'vault') {{
          body = `<div><span class="pill">vault</span> run summary</div><pre>${{escapeHtml(payload.run_summary || '-')}}</pre>`;
        }} else if (event.kind === 'stdout' || event.kind === 'stderr') {{
          body = `<pre>${{escapeHtml(payload.line || '')}}</pre>`;
        }} else {{
          body = `<pre>${{asJson(payload)}}</pre>`;
        }}
        return `<div class="item action"><b>${{escapeHtml(event.kind)}}</b><br><span class="muted">${{escapeHtml(event.created_at)}}</span>${{body}}</div>`;
      }}).join('');
    }}
    async function pauseProvider(provider, model) {{
      const data = await fetch('/api/providers/pause', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{ provider, model, seconds: 1800, reason: 'manual admin pause' }})
      }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function resumeProvider(provider, model) {{
      const data = await fetch('/api/providers/resume', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{ provider, model }})
      }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function cancelJob(id) {{
      const data = await fetch(`/api/jobs/${{id}}/cancel`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function preflightJob(id) {{
      const data = await fetch(`/api/jobs/${{id}}/preflight`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function retryJob(id) {{
      const data = await fetch(`/api/jobs/${{id}}/retry`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function runSchedule(id) {{
      const data = await fetch(`/api/schedules/${{id}}/run`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function enableSchedule(id) {{
      const data = await fetch(`/api/schedules/${{id}}/enable`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function disableSchedule(id) {{
      const data = await fetch(`/api/schedules/${{id}}/disable`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function deleteSchedule(id) {{
      const data = await fetch(`/api/schedules/${{id}}`, {{ method: 'DELETE' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    function editSchedule(schedule) {{
      schedule_name.value = schedule.name;
      schedule_kind.value = schedule.kind === 'AgentTask' ? 'agent-task' : schedule.kind.toLowerCase();
      schedule_every.value = schedule.interval_seconds;
      schedule_message.value = schedule.payload.message || schedule.payload.task || '';
      schedule_project.value = schedule.payload.project || '';
      schedule_provider.value = schedule.payload.provider || 'codex';
      schedule_goal.value = schedule.payload.goal || '';
      schedule_network.checked = Boolean(schedule.payload.allow_network);
      schedule_form.dataset.scheduleId = schedule.id;
    }}
    function resetScheduleForm() {{
      schedule_form.reset();
      schedule_every.value = 3600;
      delete schedule_form.dataset.scheduleId;
    }}
    document.querySelector('#schedule-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const body = {{
        name: schedule_name.value,
        kind: schedule_kind.value,
        every_seconds: Number(schedule_every.value || 1),
        message: schedule_message.value,
        project: schedule_project.value,
        provider: schedule_provider.value,
        goal: schedule_goal.value,
        allow_network: schedule_network.checked
      }};
      const id = schedule_form.dataset.scheduleId;
      const response = await fetch(id ? `/api/schedules/${{id}}` : '/api/schedules', {{
        method: id ? 'PATCH' : 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify(body)
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      resetScheduleForm();
      await load();
    }});
    document.querySelector('#worker-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/settings/worker', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{ max_concurrent_jobs: Number(worker_concurrency.value || 1) }})
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    document.querySelector('#routing-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/settings/routing', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{
          fallback_enabled: fallback_enabled.checked,
          fallback_order: fallback_order.value.split(',').map(value => value.trim()).filter(Boolean)
        }})
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    function optionalNumber(value) {{
      return value === '' ? null : Number(value);
    }}
    document.querySelector('#budget-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/settings/budget', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{
          enabled: budget_enabled.checked,
          daily_total_usd: optionalNumber(budget_total.value),
          daily_provider_usd: optionalNumber(budget_provider.value),
          daily_project_usd: optionalNumber(budget_project.value)
        }})
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    document.querySelector('#secret-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/secrets', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{
          name: secret_name.value,
          provider: secret_provider.value,
          kind: secret_kind.value || 'api-key',
          value: secret_value.value
        }})
      }});
      const data = await response.json();
      secret_value.value = '';
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    document.querySelector('#grant-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/secrets/grants', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{
          secret: grant_secret.value,
          provider: grant_provider.value || null,
          capability: grant_capability.value || 'provider-proxy',
          ttl_seconds: Number(grant_ttl.value || 900),
          max_uses: Number(grant_max_uses.value || 1)
        }})
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    document.querySelector('#chat').addEventListener('submit', async event => {{
      event.preventDefault();
      const body = {{
        project: project.value,
        provider: provider.value,
        goal: goal.value,
        allow_network: allow_network.checked
      }};
      const response = await fetch('/api/chat', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify(body)
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    load();
  </script>
</body>
</html>"#
    )
}
