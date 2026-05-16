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
    domain::{
        JobStatus, MemoryKind, MountMode, NetworkMode, ProviderKind, ScheduleKind, ScheduleStatus,
    },
    gates, memory, scheduler,
    secrets::SecretVault,
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
    message: Option<String>,
    allow_network: Option<bool>,
    read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateWorkerRequest {
    max_concurrent_jobs: usize,
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
        .route("/api/secrets", get(secrets).post(create_secret))
        .route(
            "/api/secrets/grants",
            get(secret_grants).post(create_secret_grant),
        )
        .route("/api/secrets/audit", get(secret_audit))
        .route("/api/system-events", get(system_events))
        .route("/api/jobs/:id/events", get(job_events))
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
            ProviderKind::Codex,
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
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 6px;
      margin-top: 8px;
    }}
    .actions button {{
      height: 32px;
      font-size: 12px;
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
      <form id="worker-form" class="item">
        <label for="worker_concurrency">Max concurrent jobs</label>
        <div class="grid-2">
          <input id="worker_concurrency" type="number" min="1" value="{worker_concurrency}">
          <button type="submit">Save</button>
        </div>
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
        <label for="goal">Goal</label>
        <textarea id="goal" name="goal"></textarea>
        <div class="row">
          <input id="allow_network" name="allow_network" type="checkbox">
          <label for="allow_network">Allow network for this session</label>
        </div>
        <button type="submit">Queue Codex Job</button>
      </form>
    </section>
  </main>
  <script>
    async function load() {{
      const [health, projects, jobs, schedules, systemEvents] = await Promise.all([
        fetch('/api/health').then(r => r.json()),
        fetch('/api/projects').then(r => r.json()),
        fetch('/api/jobs').then(r => r.json()),
        fetch('/api/schedules').then(r => r.json()),
        fetch('/api/system-events').then(r => r.json())
      ]);
      document.querySelector('#worker').innerHTML = `
        <b>${{health.worker.running_jobs}} / ${{health.worker.max_concurrent_jobs}}</b> slots used<br>
        <span class="muted">Queued: ${{health.worker.queued_jobs}} · Available: ${{health.worker.available_slots}}<br>Runtime: ${{health.container_runtime}}</span>
      `;
      document.querySelector('#memory').innerHTML = `
        <b>${{health.memory.embedded_items}} / ${{health.memory.items}}</b> embedded<br>
        <span class="muted">${{health.memory.embedding_backend}} · ${{health.memory.embedding_model}} · ${{health.memory.embedding_dimensions}}d<br>Missing: ${{health.memory.missing_embeddings}}</span>
      `;
      worker_concurrency.value = health.worker.max_concurrent_jobs;
      document.querySelector('#projects').innerHTML = projects.length
        ? projects.map(p => `<div class="item"><b>${{p.name}}</b><br><span class="muted">${{p.path}}</span></div>`).join('')
        : 'No projects registered.';
      document.querySelector('#jobs').innerHTML = jobs.length
        ? jobs.map(j => `<div class="item"><b>${{j.status}}</b><br>${{j.goal}}<br><span class="muted">Heartbeat: ${{j.last_heartbeat_at || '-'}}</span><div class="actions"><button type="button" class="secondary" onclick="eventsFor('${{j.id}}')">Events</button><button type="button" class="danger" onclick="cancelJob('${{j.id}}')">Cancel</button><button type="button" onclick="retryJob('${{j.id}}')">Retry</button></div></div>`).join('')
        : 'No jobs yet.';
      document.querySelector('#schedules').innerHTML = schedules.length
        ? schedules.map(s => `<div class="item"><b>${{s.name}}</b><br><span class="muted">${{s.kind}} · ${{s.status}} · every ${{s.interval_seconds}}s<br>Next: ${{s.next_run_at}}</span><div class="actions"><button type="button" onclick="runSchedule('${{s.id}}')">Run</button><button type="button" class="secondary" onclick='editSchedule(${{JSON.stringify(s)}})'>Edit</button><button type="button" class="danger" onclick="deleteSchedule('${{s.id}}')">Delete</button><button type="button" class="secondary" onclick="enableSchedule('${{s.id}}')">Enable</button><button type="button" class="danger" onclick="disableSchedule('${{s.id}}')">Disable</button></div></div>`).join('')
        : 'No schedules.';
      document.querySelector('#system-events').innerHTML = systemEvents.length
        ? systemEvents.map(e => `<div class="item action"><b>${{e.kind}}</b><br><span class="muted">${{e.created_at}}</span><br>${{JSON.stringify(e.payload)}}</div>`).join('')
        : 'No actions recorded yet.';
    }}
    async function eventsFor(id) {{
      const data = await fetch(`/api/jobs/${{id}}/events`).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
    }}
    async function cancelJob(id) {{
      const data = await fetch(`/api/jobs/${{id}}/cancel`, {{ method: 'POST' }}).then(r => r.json());
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
    document.querySelector('#chat').addEventListener('submit', async event => {{
      event.preventDefault();
      const body = {{
        project: project.value,
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
