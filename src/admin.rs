use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse},
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::{process::Command as TokioCommand, sync::RwLock};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

use crate::{
    admin_models::*,
    admin_ui,
    agent_policy::{self, JobCreationSource},
    chat::{self, LibrarianChatResult},
    config::{Config, ToolPermissionPolicy, ToolPermissionPreset, ToolPermissionsConfig},
    db::Database,
    domain::{
        JobStatus, MemoryKind, MountMode, Project, ScheduleKind, ScheduleStatus, ToolApprovalStatus,
    },
    gates, job_review, library_tools,
    library_tools::LibraryRoot,
    memory,
    memory_policy::{durable_memory_priority, durable_memory_type, is_visible_durable_memory_item},
    prompt, provider_health, router, scheduler,
    secrets::SecretVault,
    slash_utils::split_slash_args,
    third_eye, worker,
};

#[derive(Clone)]
struct AppState {
    db: Database,
    config: Arc<RwLock<Config>>,
}

#[derive(Clone, Debug)]
struct ChatProjectContext {
    nodes: Vec<ChatLibraryContextNode>,
    suggested_nodes: Vec<ChatLibraryContextNode>,
    scope: ContextScope,
    source: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextScope {
    Node,
    Subtree,
    Ancestors,
    NodeAndAncestors,
    ContextSet,
}

impl ContextScope {
    fn label(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Subtree => "subtree",
            Self::Ancestors => "ancestors",
            Self::NodeAndAncestors => "node+ancestors",
            Self::ContextSet => "context-set",
        }
    }
}

#[derive(Clone, Debug)]
struct ChatLibraryContextNode {
    library_path: Option<PathBuf>,
    project: Option<Project>,
}

impl ChatProjectContext {
    fn primary_project(&self) -> Option<&Project> {
        self.nodes.iter().find_map(|node| node.project.as_ref())
    }

    fn primary_project_id(&self) -> Option<Uuid> {
        self.primary_project().map(|project| project.id)
    }

    fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn label(&self) -> String {
        context_label_for_nodes(&self.nodes)
    }

    fn suggested_label(&self) -> String {
        context_label_for_nodes(&self.suggested_nodes)
    }

    fn has_suggestion(&self) -> bool {
        !self.suggested_nodes.is_empty()
    }

    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "source": self.source,
            "label": self.label(),
            "scope": self.scope.label(),
            "nodes": self.nodes.iter().map(library_context_metadata).collect::<Vec<_>>(),
            "suggested_nodes": self.suggested_nodes.iter().map(library_context_metadata).collect::<Vec<_>>(),
            "projects": self.nodes.iter().filter_map(|node| node.project.as_ref()).map(project_context_metadata).collect::<Vec<_>>(),
        })
    }
}

pub async fn serve(bind: String, db: Database, config: Config) -> Result<()> {
    validate_admin_auth_for_bind(&bind, &config)?;
    let state = AppState {
        db,
        config: Arc::new(RwLock::new(config)),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/projects", get(projects).post(create_project))
        .route("/api/project-map", get(project_map))
        .route(
            "/api/prompt-blocks",
            get(prompt_blocks).post(create_prompt_block),
        )
        .route("/api/prompt-blocks/render", get(render_prompt_target))
        .route(
            "/api/prompt-blocks/export-proposal",
            post(propose_prompt_export),
        )
        .route(
            "/api/prompt-blocks/:id",
            patch(update_prompt_block).delete(delete_prompt_block),
        )
        .route("/api/prompt-blocks/:id/enable", post(enable_prompt_block))
        .route("/api/prompt-blocks/:id/disable", post(disable_prompt_block))
        .route(
            "/api/projects/:id/attach-library",
            post(attach_project_library),
        )
        .route(
            "/api/projects/:id/attach-workspace",
            post(attach_project_workspace),
        )
        .route("/api/jobs", get(jobs).post(create_job))
        .route("/api/schedules", get(schedules).post(create_schedule))
        .route("/api/settings/worker", post(update_worker_settings))
        .route("/api/settings/chat", post(update_chat_settings))
        .route(
            "/api/settings/tool-permissions",
            post(update_tool_permissions_settings),
        )
        .route("/api/settings/codex", post(update_codex_runtime_settings))
        .route("/api/settings/claude", post(update_claude_runtime_settings))
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
        .route("/api/providers/:provider/smoke", post(provider_smoke))
        .route("/api/providers/pause", post(pause_provider))
        .route("/api/providers/resume", post(resume_provider))
        .route("/api/usage", get(usage_observations))
        .route("/api/third-eye", get(third_eye_status))
        .route("/api/library/tree", get(library_tree))
        .route("/api/library/folders", post(library_create_folder))
        .route("/api/library/files", post(library_create_file))
        .route(
            "/api/library/markdown",
            get(library_read_markdown).post(library_write_markdown),
        )
        .route("/api/library/move", post(library_move))
        .route("/api/library/delete", post(library_delete))
        .route("/api/jobs/:id", get(job))
        .route("/api/jobs/:id/events", get(job_events))
        .route("/api/jobs/:id/preflight", post(preflight_job))
        .route("/api/jobs/:id/review-packet", post(review_packet_job))
        .route(
            "/api/jobs/:id/git-action-proposal",
            post(propose_job_git_action_api),
        )
        .route("/api/jobs/:id/cancel", post(cancel_job))
        .route("/api/jobs/:id/retry", post(retry_job))
        .route("/api/schedules/:id/enable", post(enable_schedule))
        .route("/api/schedules/:id/disable", post(disable_schedule))
        .route("/api/schedules/:id/run", post(run_schedule))
        .route(
            "/api/schedules/:id",
            patch(update_schedule).delete(delete_schedule),
        )
        .route("/api/chat/sessions", get(chat_sessions))
        .route("/api/chat/sessions/:id/turns", get(chat_session_turns))
        .route("/api/chat", post(librarian_chat))
        .route("/api/slash-commands", get(slash_commands))
        .route("/api/approvals/:id/approve", post(approve_tool_approval))
        .route("/api/approvals/:id/reject", post(reject_tool_approval))
        .route("/api/agent-jobs", post(create_job))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_admin_auth,
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("Librarian admin UI listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn validate_admin_auth_for_bind(bind: &str, config: &Config) -> Result<()> {
    if !is_external_admin_bind(bind) {
        return Ok(());
    }
    if config.admin.auth_enabled
        && config
            .admin
            .auth_token
            .as_deref()
            .is_some_and(|v| !v.is_empty())
    {
        return Ok(());
    }
    anyhow::bail!(
        "Admin bind `{bind}` is externally reachable. Set `[admin].auth_enabled = true` and `[admin].auth_token`, or set LIBRARIAN_ADMIN_TOKEN before binding outside localhost."
    )
}

fn is_external_admin_bind(bind: &str) -> bool {
    let host = bind
        .rsplit_once(':')
        .map(|(host, _)| host.trim_matches(['[', ']']))
        .unwrap_or(bind)
        .trim();
    if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
        return false;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => !ip.is_loopback(),
        Err(_) => true,
    }
}

async fn require_admin_auth(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let config = state.config.read().await;
    if !config.admin.auth_enabled {
        return next.run(request).await;
    }
    let Some(expected) = config
        .admin
        .auth_token
        .as_deref()
        .filter(|token| !token.is_empty())
    else {
        return (
            StatusCode::UNAUTHORIZED,
            "Admin auth is enabled but no admin token is configured.",
        )
            .into_response();
    };
    let header_ok = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);
    let query_ok = request.uri().query().is_some_and(|query| {
        query.split('&').any(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            key == "admin_token" && value == expected
        })
    });
    if header_ok || query_ok {
        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, "Admin authentication required.").into_response()
    }
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    Html(admin_ui::chat_first_app_html(
        &config.admin.bind,
        config.worker.max_concurrent_jobs,
    ))
}

async fn slash_commands(State(state): State<AppState>) -> impl IntoResponse {
    let mut commands = vec![
        serde_json::json!({"command": "/help", "description": "Show available command groups", "group": "general"}),
        serde_json::json!({"command": "/lib help", "description": "Knowledge base files and Markdown tools", "group": "library"}),
        serde_json::json!({"command": "/lib tree", "description": "Show the Library tree", "group": "library"}),
        serde_json::json!({"command": "/lib read ", "description": "Read a Markdown note", "group": "library"}),
        serde_json::json!({"command": "/lib append ", "description": "Append to a Markdown note", "group": "library"}),
        serde_json::json!({"command": "/lib replace-lines ", "description": "Replace a line range in a note", "group": "library"}),
        serde_json::json!({"command": "/lib replace-find ", "description": "Replace the first search match in a note", "group": "library"}),
        serde_json::json!({"command": "/work help", "description": "Project workspace folder tools", "group": "workspace"}),
        serde_json::json!({"command": "/work mkdir ", "description": "Create a workspace folder", "group": "workspace"}),
        serde_json::json!({"command": "/work touch ", "description": "Create an empty workspace file", "group": "workspace"}),
        serde_json::json!({"command": "/project help", "description": "Project records and attachments", "group": "project"}),
        serde_json::json!({"command": "/project list", "description": "List registered projects", "group": "project"}),
        serde_json::json!({"command": "/project create ", "description": "Create a library project", "group": "project"}),
        serde_json::json!({"command": "/project attach-workspace ", "description": "Attach an existing workspace directory", "group": "project"}),
        serde_json::json!({"command": "/mem help", "description": "Durable memory tools", "group": "memory"}),
        serde_json::json!({"command": "/remember ", "description": "Remember a durable fact", "group": "memory"}),
        serde_json::json!({"command": "/mem recent", "description": "Show recent durable memory", "group": "memory"}),
        serde_json::json!({"command": "/mem cleanup-legacy-local-responder", "description": "Clean old placeholder memory replies", "group": "memory"}),
        serde_json::json!({"command": "/approval list", "description": "Review pending approvals", "group": "approval"}),
        serde_json::json!({"command": "/prompt blocks", "description": "List prompt blocks", "group": "prompt"}),
        serde_json::json!({"command": "/prompt export-presets ", "description": "Export prompt blocks as portable JSON", "group": "prompt"}),
        serde_json::json!({"command": "/prompt import-presets ", "description": "Import portable prompt preset JSON", "group": "prompt"}),
        serde_json::json!({"command": "/settings tool-permissions", "description": "Show tool permission policy", "group": "settings"}),
        serde_json::json!({"command": "/agent list", "description": "List background agent jobs", "group": "agent"}),
        serde_json::json!({"command": "/agent preflight ", "description": "Prepare a job command without running it", "group": "agent"}),
        serde_json::json!({"command": "/agent review-packet ", "description": "Build a patch review packet for a job", "group": "agent"}),
        serde_json::json!({"command": "/agent launch ", "description": "Queue an explicit background agent job", "group": "agent"}),
    ];
    if let Ok(projects) = state.db.list_projects().await {
        for project in projects.into_iter().take(20) {
            commands.push(serde_json::json!({
                "command": format!("/project status {}", project.name),
                "description": "Show project library/workspace status",
                "group": "project",
            }));
            commands.push(serde_json::json!({
                "command": format!("/agent launch --project \"{}\" --goal ", project.name),
                "description": "Queue an explicit agent job for this project",
                "group": "agent",
            }));
        }
    }
    Json(commands)
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
        "admin": {
            "bind": config.admin.bind,
            "auth_enabled": config.admin.auth_enabled,
            "auth_configured": config.admin.auth_token.as_deref().is_some_and(|token| !token.is_empty()),
        },
            "worker": {
                "max_concurrent_jobs": max_concurrent_jobs,
                "running_jobs": running_jobs,
                "queued_jobs": queued_jobs,
                "available_slots": available_slots,
            },
            "chat": {
                "assistant_name": config.chat.assistant_name,
                "codex_timeout_seconds": config.chat.codex_timeout_seconds,
                "memory_hit_limit": config.chat.memory_hit_limit,
                "max_iterations": config.chat.max_iterations,
        },
        "tool_permissions": config.tool_permissions,
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
    let projects = state.db.list_projects().await?;
    Ok(Json(
        projects.iter().map(project_api_json).collect::<Vec<_>>(),
    ))
}

fn project_api_json(project: &Project) -> serde_json::Value {
    let context_path = project
        .library_path
        .as_ref()
        .map(|path| path.to_string_lossy().replace('\\', "/"));
    let workspace_path = project.path.to_string_lossy().to_string();
    serde_json::json!({
        "id": project.id,
        "name": project.name,
        "library_path": context_path.clone(),
        "context_path": context_path,
        "workspace_path": workspace_path,
        "path": workspace_path,
        "autonomy_mode": project.autonomy_mode,
        "git_policy": project.git_policy,
        "created_at": project.created_at,
    })
}

async fn create_project(
    State(state): State<AppState>,
    Json(input): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "library.create",
        config.tool_permissions.library_create,
    )
    .await?;
    ensure_tool_permission(
        &state.db,
        &config,
        "workspace.create",
        config.tool_permissions.workspace_create,
    )
    .await?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(anyhow::anyhow!("Project name must not be empty").into());
    }
    let library_path = input
        .library_path
        .unwrap_or_else(|| format!("projects/{}", project_folder_name(name)));
    let library_path = library_tools::normalize_tool_relative_path(&library_path)?;
    library_tools::create_folder(&config, LibraryRoot::Library, &library_path)?;
    let workspace_path = if let Some(path) = input.workspace_path {
        canonical_existing_dir(&path)?
    } else {
        let relative = project_folder_name(name);
        library_tools::create_folder(&config, LibraryRoot::Projects, &relative)?;
        config.home.join("Projects").join(relative).canonicalize()?
    };
    let project = state.db.add_project(name, &workspace_path).await?;
    let project = state
        .db
        .attach_project_library_path(project.id, PathBuf::from(&library_path).as_path())
        .await?;
    log_project_event(
        &state.db,
        "create",
        serde_json::json!({
            "project_id": project.id,
            "name": project.name.clone(),
            "library_path": project.library_path.clone(),
            "workspace_path": project.path.clone(),
            "source": "admin-api",
        }),
    )
    .await?;
    Ok(Json(project_api_json(&project)))
}

async fn attach_project_library(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(input): Json<AttachLibraryRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "library.move",
        config.tool_permissions.library_move,
    )
    .await?;
    let library_path = library_tools::normalize_tool_relative_path(&input.library_path)?;
    let project = state
        .db
        .attach_project_library_path(id, PathBuf::from(&library_path).as_path())
        .await?;
    log_project_event(
        &state.db,
        "attach_library",
        serde_json::json!({
            "project_id": project.id,
            "library_path": project.library_path.clone(),
            "source": "admin-api",
        }),
    )
    .await?;
    Ok(Json(project_api_json(&project)))
}

async fn attach_project_workspace(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(input): Json<AttachWorkspaceRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "workspace.move",
        config.tool_permissions.workspace_move,
    )
    .await?;
    let workspace_path = canonical_existing_dir(&input.workspace_path)?;
    let project = state
        .db
        .update_project_workspace_path(id, &workspace_path)
        .await?;
    log_project_event(
        &state.db,
        "attach_workspace",
        serde_json::json!({
            "project_id": project.id,
            "workspace_path": project.path.clone(),
            "source": "admin-api",
        }),
    )
    .await?;
    Ok(Json(project_api_json(&project)))
}

async fn project_map(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let projects = state.db.list_projects().await?;
    Ok(Json(build_project_map(&config, projects)?))
}

async fn prompt_blocks(
    State(state): State<AppState>,
    Query(query): Query<PromptBlocksQuery>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state.db.list_prompt_blocks(query.target.as_deref()).await?,
    ))
}

async fn create_prompt_block(
    State(state): State<AppState>,
    Json(input): Json<CreatePromptBlockRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "settings.change",
        config.tool_permissions.settings_change,
    )
    .await?;
    let block = state
        .db
        .create_prompt_block(
            &input.target,
            &input.name,
            &input.content,
            input.markdown.unwrap_or(true),
        )
        .await?;
    state
        .db
        .add_system_event(
            "prompt_tool",
            serde_json::json!({
                "action": "add_block",
                "source": "admin-api",
                "block_id": block.id,
                "target": block.target,
            }),
        )
        .await?;
    Ok(Json(block))
}

async fn update_prompt_block(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(input): Json<UpdatePromptBlockRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "settings.change",
        config.tool_permissions.settings_change,
    )
    .await?;
    let block = state
        .db
        .update_prompt_block(
            id,
            input.name.as_deref(),
            input.content.as_deref(),
            input.enabled,
            input.position,
            input.markdown,
        )
        .await?;
    state
        .db
        .add_system_event(
            "prompt_tool",
            serde_json::json!({
                "action": "update_block",
                "source": "admin-api",
                "block_id": block.id,
                "target": block.target,
            }),
        )
        .await?;
    Ok(Json(block))
}

async fn delete_prompt_block(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "settings.change",
        config.tool_permissions.settings_change,
    )
    .await?;
    state.db.delete_prompt_block(id).await?;
    state
        .db
        .add_system_event(
            "prompt_tool",
            serde_json::json!({
                "action": "delete_block",
                "source": "admin-api",
                "block_id": id,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "block_id": id })))
}

async fn propose_prompt_export(
    State(state): State<AppState>,
    Json(input): Json<ExportPromptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let blocks = state.db.list_prompt_blocks(Some(&input.target)).await?;
    let rendered = render_prompt_blocks(&blocks);
    let approval = state
        .db
        .create_tool_approval(
            "library",
            "write_markdown",
            serde_json::json!({
                "path": input.path,
                "content": rendered,
                "target": input.target,
            }),
        )
        .await?;
    state
        .db
        .add_system_event(
            "tool_approval",
            serde_json::json!({
                "action": "propose_prompt_export",
                "approval_id": approval.id,
                "target": input.target,
            }),
        )
        .await?;
    Ok(Json(approval))
}

async fn enable_prompt_block(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    set_prompt_block_enabled_api(state, id, true).await
}

async fn disable_prompt_block(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    set_prompt_block_enabled_api(state, id, false).await
}

async fn set_prompt_block_enabled_api(
    state: AppState,
    id: Uuid,
    enabled: bool,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "settings.change",
        config.tool_permissions.settings_change,
    )
    .await?;
    let block = state.db.set_prompt_block_enabled(id, enabled).await?;
    state
        .db
        .add_system_event(
            "prompt_tool",
            serde_json::json!({
                "action": if enabled { "enable_block" } else { "disable_block" },
                "source": "admin-api",
                "block_id": block.id,
                "target": block.target,
            }),
        )
        .await?;
    Ok(Json(block))
}

async fn render_prompt_target(
    State(state): State<AppState>,
    Query(query): Query<PromptBlocksQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(target) = query.target.as_deref() else {
        return Err(anyhow::anyhow!("target is required").into());
    };
    let blocks = state.db.list_prompt_blocks(Some(target)).await?;
    let rendered = render_prompt_blocks(&blocks);
    let version = prompt::prompt_block_version(Some(target), &blocks);
    Ok(Json(serde_json::json!({
        "target": target,
        "rendered": rendered,
        "version": version,
        "blocks": blocks,
    })))
}

async fn library_tree(
    State(state): State<AppState>,
    Query(query): Query<LibraryTreeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let max_depth = query.max_depth.unwrap_or(6);
    if let Some(root) = query.root {
        ensure_library_api_root(root)?;
    }
    Ok(Json(serde_json::json!({
        "roots": [library_tools::tree(&config, LibraryRoot::Library, max_depth)?],
    })))
}

async fn library_create_folder(
    State(state): State<AppState>,
    Json(input): Json<LibraryPathRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_library_api_root(input.root)?;
    let config = state.config.read().await.clone();
    let path = library_tools::create_folder(&config, input.root, &input.path)?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "create_folder",
                "root": input.root,
                "path": path.path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

async fn library_create_file(
    State(state): State<AppState>,
    Json(input): Json<LibraryPathRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_library_api_root(input.root)?;
    let config = state.config.read().await.clone();
    let path = library_tools::create_empty_file(&config, input.root, &input.path)?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "create_empty_file",
                "root": input.root,
                "path": path.path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

async fn library_read_markdown(
    State(state): State<AppState>,
    Query(input): Query<LibraryMarkdownRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let content = library_tools::read_markdown(&config, &input.path)?;
    Ok(Json(serde_json::json!({
        "root": "library",
        "path": input.path,
        "content": content,
    })))
}

async fn library_write_markdown(
    State(state): State<AppState>,
    Json(input): Json<LibraryMarkdownRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(content) = input.content.as_deref() else {
        return Err(anyhow::anyhow!("content is required").into());
    };
    let config = state.config.read().await.clone();
    let path = library_tools::write_markdown(&config, &input.path, content)?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "write_markdown",
                "root": "library",
                "path": path.path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

async fn library_move(
    State(state): State<AppState>,
    Json(input): Json<LibraryMoveRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_library_api_root(input.root)?;
    let config = state.config.read().await.clone();
    let path = library_tools::move_path(&config, input.root, &input.from, &input.to)?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "move",
                "root": input.root,
                "from": input.from,
                "to": path.path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

async fn library_delete(
    State(state): State<AppState>,
    Json(input): Json<LibraryDeleteRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_library_api_root(input.root)?;
    let config = state.config.read().await.clone();
    let path = library_tools::delete_path(
        &config,
        input.root,
        &input.path,
        input.recursive.unwrap_or(false),
    )?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "delete",
                "root": input.root,
                "path": path.path,
                "recursive": input.recursive.unwrap_or(false),
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

fn ensure_library_api_root(root: LibraryRoot) -> Result<()> {
    if root != LibraryRoot::Library {
        anyhow::bail!(
            "Library API only accepts root=library; use workspace/project tools for Projects"
        );
    }
    Ok(())
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
    let config = state.config.read().await;
    let diagnostics = provider_health::collect_provider_diagnostics(&config, &states).await;
    let command_prefix = format!("librarian --home {}", admin_shell_path(&config.home));
    let default_codex_home = config.home.join(".cfg").join("codex-home");
    let codex_home = config
        .codex
        .host_home
        .as_ref()
        .unwrap_or(&default_codex_home);
    let default_claude_home = config.home.join(".cfg").join("claude-home");
    let claude_home = config
        .claude
        .host_home
        .as_ref()
        .unwrap_or(&default_claude_home);
    Ok(Json(serde_json::json!({
        "catalog": catalog,
        "states": states,
        "commands": {
            "codex_auth": format!(
                "CODEX_HOME={} codex\n{} auth codex --enable-container-mount --codex-home {}",
                admin_shell_path(codex_home),
                command_prefix,
                admin_shell_path(codex_home),
            ),
            "claude_auth": format!(
                "CLAUDE_HOME={} claude\n{} auth claude --enable-container-mount --claude-home {}",
                admin_shell_path(claude_home),
                command_prefix,
                admin_shell_path(claude_home),
            ),
            "build_agent_image": format!("{command_prefix} runtime build-agent-image"),
            "smoke_codex": format!("{command_prefix} smoke mvp --provider codex --run-agent"),
            "smoke_claude": format!("{command_prefix} smoke mvp --provider claude-code --run-agent"),
            "smoke_openrouter": format!("{command_prefix} smoke mvp --provider open-router --secret <secret-name-or-id> --run-agent"),
        },
        "diagnostics": diagnostics,
        "runtime": {
            "codex": {
                "host_home": config.codex.host_home.as_ref().map(|path| path.display().to_string()),
                "host_home_exists": config.codex.host_home.as_ref().map(|path| path.exists()),
                "mount_host_home": config.codex.mount_host_home,
                "mount_read_only": config.codex.mount_read_only,
                "container_home": config.codex.container_home,
            },
            "claude-code": {
                "host_home": config.claude.host_home.as_ref().map(|path| path.display().to_string()),
                "host_home_exists": config.claude.host_home.as_ref().map(|path| path.exists()),
                "mount_host_home": config.claude.mount_host_home,
                "mount_read_only": config.claude.mount_read_only,
                "container_home": config.claude.container_home,
                "instruction_file": config.claude.instruction_file,
            },
        },
    })))
}

async fn provider_smoke(
    State(state): State<AppState>,
    AxumPath(provider): AxumPath<String>,
    Query(query): Query<ProviderSmokeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let provider_arg = match provider.trim().to_ascii_lowercase().as_str() {
        "codex" => "codex",
        "claude" | "claude-code" => "claude-code",
        "openrouter" | "open-router" => "open-router",
        other => return Err(anyhow::anyhow!("Unknown provider `{other}`").into()),
    };
    let config = state.config.read().await.clone();
    let exe = std::env::current_exe().context("Failed to resolve current Librarian executable")?;
    let command = vec![
        admin_shell_path(&exe),
        "--home".to_string(),
        admin_shell_path(&config.home),
        "smoke".to_string(),
        "mvp".to_string(),
        "--provider".to_string(),
        provider_arg.to_string(),
    ];
    if query.dry_run.unwrap_or(false) {
        return Ok(Json(serde_json::json!({
            "provider": provider_arg,
            "dry_run": true,
            "command": command,
        })));
    }
    let output = TokioCommand::new(&exe)
        .arg("--home")
        .arg(&config.home)
        .arg("smoke")
        .arg("mvp")
        .arg("--provider")
        .arg(provider_arg)
        .output()
        .await?;
    Ok(Json(serde_json::json!({
        "provider": provider_arg,
        "command": command,
        "success": output.status.success(),
        "status": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
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
    let grants = state
        .db
        .list_secret_grants(50)
        .await?
        .into_iter()
        .map(|grant| {
            serde_json::json!({
                "id": grant.id,
                "token": crate::secrets::encode_grant_token(grant.id),
                "secret_id": grant.secret_id,
                "job_id": grant.job_id,
                "provider": grant.provider,
                "capability": grant.capability,
                "expires_at": grant.expires_at,
                "max_uses": grant.max_uses,
                "uses": grant.uses,
                "created_at": grant.created_at,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(grants))
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

async fn review_packet_job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        job_review::build_job_review_packet(&state.db, id, false, None).await?,
    ))
}

async fn propose_job_git_action_api(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(input): Json<JobGitActionProposalRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let action = match input.action.trim().to_ascii_lowercase().as_str() {
        "commit" => job_review::GitGateActionArg::Commit,
        "push" => job_review::GitGateActionArg::Push,
        "revert" => job_review::GitGateActionArg::Revert,
        other => return Err(anyhow::anyhow!("Unknown git action proposal `{other}`").into()),
    };
    let approval = job_review::propose_job_git_action(
        &state.db,
        id,
        action,
        input.message.as_deref(),
        input.commit.as_deref(),
    )
    .await?;
    Ok(Json(serde_json::json!({ "approval": approval })))
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

async fn update_chat_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateChatSettingsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (assistant_name, codex_timeout_seconds, memory_hit_limit, max_iterations, config_path) = {
        let mut config = state.config.write().await;
        if let Some(name) = input.assistant_name {
            let name = name.trim();
            config.chat.assistant_name = if name.is_empty() {
                "Librarian".to_string()
            } else {
                name.to_string()
            };
        }
        if let Some(timeout) = input.codex_timeout_seconds {
            config.chat.codex_timeout_seconds = timeout.max(1);
        }
        if let Some(limit) = input.memory_hit_limit {
            config.chat.memory_hit_limit = limit.max(1);
        }
        if let Some(iterations) = input.max_iterations {
            config.chat.max_iterations = iterations.clamp(1, 100);
        }
        config.save()?;
        (
            config.chat.assistant_name.clone(),
            config.chat.codex_timeout_seconds,
            config.chat.memory_hit_limit,
            config.chat.max_iterations,
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "chat_settings_updated",
            serde_json::json!({
                "assistant_name": assistant_name,
                "codex_timeout_seconds": codex_timeout_seconds,
                "memory_hit_limit": memory_hit_limit,
                "max_iterations": max_iterations,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "chat": {
            "assistant_name": assistant_name,
            "codex_timeout_seconds": codex_timeout_seconds,
            "memory_hit_limit": memory_hit_limit,
            "max_iterations": max_iterations,
        },
    })))
}

async fn update_tool_permissions_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateToolPermissionsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let current_config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &current_config,
        "settings.change",
        current_config.tool_permissions.settings_change,
    )
    .await?;

    let permissions = {
        let mut config = state.config.write().await;
        let preset_choice = input
            .preset
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(parse_tool_permission_preset)
            .transpose()?;
        if let Some(preset) = preset_choice {
            if preset != ToolPermissionPreset::Custom {
                apply_tool_permission_preset(&mut config.tool_permissions, preset);
                config.save()?;
                config.tool_permissions.clone()
            } else {
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "library_read",
                    input.library_read,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "library_create",
                    input.library_create,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "library_edit_markdown",
                    input.library_edit_markdown,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "library_move",
                    input.library_move,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "library_delete",
                    input.library_delete,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "workspace_create",
                    input.workspace_create,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "workspace_move",
                    input.workspace_move,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "workspace_delete",
                    input.workspace_delete,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "memory_write",
                    input.memory_write,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "settings_change",
                    input.settings_change,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "agent_launch",
                    input.agent_launch,
                )?;
                apply_optional_tool_permission(
                    &mut config.tool_permissions,
                    "context_switch",
                    input.context_switch,
                )?;
                config.save()?;
                config.tool_permissions.clone()
            }
        } else {
            config.tool_permissions.clone()
        }
    };
    state
        .db
        .add_system_event(
            "settings_tool",
            serde_json::json!({
                "action": "tool_permissions_updated",
                "source": "admin-api",
                "tool_permissions": permissions,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "tool_permissions": permissions,
    })))
}

fn apply_optional_tool_permission(
    permissions: &mut ToolPermissionsConfig,
    key: &str,
    value: Option<String>,
) -> Result<()> {
    if let Some(value) = value.as_deref().filter(|value| !value.trim().is_empty()) {
        set_tool_permission(permissions, key, parse_tool_permission_policy(value)?)?;
    }
    Ok(())
}

async fn update_codex_runtime_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateCodexRuntimeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (host_home, mount_host_home, mount_read_only, config_path) = {
        let mut config = state.config.write().await;
        if let Some(path) = input.host_home {
            let path = path.trim();
            config.codex.host_home = if path.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(path))
            };
        }
        if let Some(enabled) = input.mount_host_home {
            config.codex.mount_host_home = enabled;
        }
        if let Some(read_only) = input.mount_read_only {
            config.codex.mount_read_only = read_only;
        }
        config.save()?;
        (
            config
                .codex
                .host_home
                .as_ref()
                .map(|path| path.display().to_string()),
            config.codex.mount_host_home,
            config.codex.mount_read_only,
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "codex_runtime_updated",
            serde_json::json!({
                "host_home": host_home,
                "mount_host_home": mount_host_home,
                "mount_read_only": mount_read_only,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "codex": {
            "host_home": host_home,
            "mount_host_home": mount_host_home,
            "mount_read_only": mount_read_only,
        },
    })))
}

async fn update_claude_runtime_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateClaudeRuntimeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (host_home, mount_host_home, mount_read_only, instruction_file, config_path) = {
        let mut config = state.config.write().await;
        if let Some(path) = input.host_home {
            let path = path.trim();
            config.claude.host_home = if path.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(path))
            };
        }
        if let Some(enabled) = input.mount_host_home {
            config.claude.mount_host_home = enabled;
        }
        if let Some(read_only) = input.mount_read_only {
            config.claude.mount_read_only = read_only;
        }
        if let Some(file) = input.instruction_file {
            let file = file.trim();
            if file.is_empty() || file.contains('/') || file.contains('\\') {
                return Err(anyhow::anyhow!(
                    "Claude instruction file must be a filename like CLAUDE.md"
                )
                .into());
            }
            config.claude.instruction_file = file.to_string();
        }
        config.save()?;
        (
            config
                .claude
                .host_home
                .as_ref()
                .map(|path| path.display().to_string()),
            config.claude.mount_host_home,
            config.claude.mount_read_only,
            config.claude.instruction_file.clone(),
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "claude_runtime_updated",
            serde_json::json!({
                "host_home": host_home,
                "mount_host_home": mount_host_home,
                "mount_read_only": mount_read_only,
                "instruction_file": instruction_file,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "claude": {
            "host_home": host_home,
            "mount_host_home": mount_host_home,
            "mount_read_only": mount_read_only,
            "instruction_file": instruction_file,
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

async fn chat_sessions(
    State(state): State<AppState>,
    Query(query): Query<ChatSessionsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let sessions = state
        .db
        .list_chat_sessions(query.limit.unwrap_or(20))
        .await?;
    let mut output = Vec::new();
    for session in sessions {
        let turn_count = state.db.list_chat_turns(session.id).await?.len();
        output.push(serde_json::json!({
            "id": session.id,
            "project_id": session.project_id,
            "title": session.title,
            "created_at": session.created_at,
            "updated_at": session.updated_at,
            "turn_count": turn_count,
        }));
    }
    Ok(Json(output))
}

async fn chat_session_turns(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let session = state.db.get_chat_session(id).await?;
    let turns = state.db.list_chat_turns(id).await?;
    Ok(Json(serde_json::json!({
        "session": session,
        "turns": turns,
    })))
}

async fn approve_tool_approval(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let (approval, output) = approve_and_execute_tool_approval(&state, &config, id).await?;
    Ok(Json(serde_json::json!({
        "approval": approval,
        "output": output,
    })))
}

async fn reject_tool_approval(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let approval = reject_tool_approval_by_id(&state, id).await?;
    Ok(Json(serde_json::json!({ "approval": approval })))
}

async fn resolve_chat_project_context(
    state: &AppState,
    config: &Config,
    input: &LibrarianChatRequest,
    message: &str,
) -> Result<ChatProjectContext> {
    let known_projects = state.db.list_projects().await?;
    let scope = input
        .project_context_scope
        .as_deref()
        .map(parse_context_scope)
        .transpose()?
        .unwrap_or(ContextScope::Subtree);
    let mut requested = Vec::new();
    if let Some(values) = &input.project_context {
        requested.extend(values.iter().map(String::as_str));
    }
    if let Some(value) = input.project.as_deref() {
        requested.push(value);
    }

    let mut nodes = Vec::new();
    for value in requested {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let node = resolve_library_context_node(config, &known_projects, value)?;
        if !nodes
            .iter()
            .any(|existing| same_context_node(existing, &node))
        {
            nodes.push(node);
        }
    }
    if !nodes.is_empty() {
        return Ok(ChatProjectContext {
            nodes,
            suggested_nodes: Vec::new(),
            scope,
            source: "explicit",
        });
    }

    if message.trim_start().starts_with('/') {
        return Ok(ChatProjectContext {
            nodes: Vec::new(),
            suggested_nodes: Vec::new(),
            scope,
            source: "global",
        });
    }

    let inferred_nodes = infer_context_nodes(config, &known_projects, message)?;

    match config.tool_permissions.context_switch {
        ToolPermissionPolicy::Auto if !inferred_nodes.is_empty() => Ok(ChatProjectContext {
            nodes: inferred_nodes,
            suggested_nodes: Vec::new(),
            scope,
            source: "auto",
        }),
        ToolPermissionPolicy::Ask if !inferred_nodes.is_empty() => Ok(ChatProjectContext {
            nodes: Vec::new(),
            suggested_nodes: inferred_nodes,
            scope,
            source: "suggested",
        }),
        _ => Ok(ChatProjectContext {
            nodes: Vec::new(),
            suggested_nodes: Vec::new(),
            scope,
            source: "global",
        }),
    }
}

fn infer_context_nodes(
    config: &Config,
    known_projects: &[Project],
    message: &str,
) -> Result<Vec<ChatLibraryContextNode>> {
    let message_key = normalized_project_lookup_key(message);
    if message_key.is_empty() {
        return Ok(Vec::new());
    }
    let mut matches = known_projects
        .iter()
        .filter(|project| {
            let name_key = normalized_project_lookup_key(&project.name);
            let path_key = project
                .library_path
                .as_ref()
                .map(|path| normalized_project_lookup_key(&path.to_string_lossy()))
                .unwrap_or_default();
            !name_key.is_empty()
                && (message_key.contains(&name_key)
                    || (!path_key.is_empty() && message_key.contains(&path_key)))
        })
        .cloned()
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.name.cmp(&right.name));
    let mut nodes = matches
        .into_iter()
        .map(|project| ChatLibraryContextNode {
            library_path: project.library_path.clone(),
            project: Some(project),
        })
        .collect::<Vec<_>>();

    let library_tree = library_tools::tree(config, LibraryRoot::Library, 8)?;
    collect_inferred_library_nodes(&library_tree, known_projects, &message_key, &mut nodes);
    nodes.sort_by(|left, right| {
        library_context_display_label(left).cmp(&library_context_display_label(right))
    });
    nodes.dedup_by(|left, right| same_context_node(left, right));
    if let Some(max_depth) = nodes.iter().map(context_node_depth).max() {
        nodes.retain(|node| context_node_depth(node) == max_depth);
    }
    if nodes.len() == 1 {
        Ok(nodes)
    } else {
        Ok(Vec::new())
    }
}

fn context_node_depth(node: &ChatLibraryContextNode) -> usize {
    node.library_path
        .as_ref()
        .map(|path| path.components().count())
        .unwrap_or(0)
}

fn collect_inferred_library_nodes(
    entry: &library_tools::LibraryEntry,
    known_projects: &[Project],
    message_key: &str,
    output: &mut Vec<ChatLibraryContextNode>,
) {
    if !entry.path.is_empty() {
        let path_key = normalized_project_lookup_key(&entry.path);
        let name_key = normalized_project_lookup_key(&entry.name);
        let label_key = normalized_project_lookup_key(&humanize_project_name(&entry.name));
        if (!path_key.is_empty() && message_key.contains(&path_key))
            || (!name_key.is_empty() && message_key.contains(&name_key))
            || (!label_key.is_empty() && message_key.contains(&label_key))
        {
            let library_path = PathBuf::from(&entry.path);
            let project = known_projects
                .iter()
                .find(|project| project.library_path.as_ref() == Some(&library_path))
                .cloned();
            output.push(ChatLibraryContextNode {
                library_path: Some(library_path),
                project,
            });
        }
    }
    for child in &entry.children {
        collect_inferred_library_nodes(child, known_projects, message_key, output);
    }
}

#[path = "admin/smoke.rs"]
mod smoke;

pub use smoke::{
    run_agent_action_ui_smoke, run_agent_review_packet_ui_smoke, run_approval_ui_smoke,
    run_dialogue_context_smoke, run_memory_cleanup_smoke, run_project_slash_smoke,
    run_prompt_defaults_smoke,
};

fn parse_context_scope(value: &str) -> Result<ContextScope> {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "node" | "current" => Ok(ContextScope::Node),
        "subtree" | "descendants" | "children" => Ok(ContextScope::Subtree),
        "ancestors" | "parents" => Ok(ContextScope::Ancestors),
        "node+ancestors" | "node-and-ancestors" | "current+parents" => {
            Ok(ContextScope::NodeAndAncestors)
        }
        "context-set" | "set" | "selected" => Ok(ContextScope::ContextSet),
        _ => anyhow::bail!(
            "Context scope must be node, subtree, ancestors, node+ancestors, or context-set"
        ),
    }
}

fn resolve_library_context_node(
    config: &Config,
    projects: &[Project],
    value: &str,
) -> Result<ChatLibraryContextNode> {
    if let Some(project) = find_project_context_ref(projects, value) {
        return Ok(ChatLibraryContextNode {
            library_path: project.library_path.clone(),
            project: Some(project.clone()),
        });
    }
    let library_path = normalize_library_context_path(config, value)?;
    let project = projects
        .iter()
        .find(|project| project.library_path.as_ref() == Some(&library_path))
        .cloned();
    Ok(ChatLibraryContextNode {
        library_path: Some(library_path),
        project,
    })
}

fn find_project_context_ref<'a>(projects: &'a [Project], value: &str) -> Option<&'a Project> {
    let normalized_value = value.trim().trim_start_matches('/').replace('\\', "/");
    projects.iter().find(|project| {
        project.id.to_string() == value
            || project.name == value
            || project
                .library_path
                .as_ref()
                .map(|path| path.to_string_lossy().replace('\\', "/") == normalized_value)
                .unwrap_or(false)
    })
}

fn normalize_library_context_path(config: &Config, value: &str) -> Result<PathBuf> {
    let trimmed = value
        .trim()
        .trim_start_matches("Library/")
        .trim_start_matches("Library\\")
        .trim_start_matches('/');
    let normalized = library_tools::normalize_tool_relative_path(trimmed)?;
    let relative = PathBuf::from(normalized);
    let absolute = config.vault_path.join(&relative);
    if !absolute.exists() {
        anyhow::bail!("Library context `{}` was not found", relative.display());
    }
    Ok(relative)
}

fn same_context_node(left: &ChatLibraryContextNode, right: &ChatLibraryContextNode) -> bool {
    if let (Some(left_project), Some(right_project)) = (&left.project, &right.project) {
        return left_project.id == right_project.id;
    }
    left.library_path == right.library_path
}

fn context_label_for_nodes(nodes: &[ChatLibraryContextNode]) -> String {
    if nodes.is_empty() {
        "Global conversation".to_string()
    } else {
        nodes
            .iter()
            .map(library_context_display_label)
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

fn library_context_metadata(node: &ChatLibraryContextNode) -> serde_json::Value {
    serde_json::json!({
        "kind": if node.library_path.is_some() { "library_node" } else { "project" },
        "label": library_context_display_label(node),
        "library_path": node.library_path.as_ref().map(|path| path.to_string_lossy().replace('\\', "/")),
        "project": node.project.as_ref().map(project_context_metadata),
    })
}

fn project_context_metadata(project: &Project) -> serde_json::Value {
    let context_path = project
        .library_path
        .as_ref()
        .map(|path| path.to_string_lossy().replace('\\', "/"));
    serde_json::json!({
        "id": project.id,
        "name": project.name,
        "display_name": project_display_label(project),
        "library_path": context_path.clone(),
        "context_path": context_path,
        "workspace_path": project.path.to_string_lossy().to_string(),
    })
}

fn library_context_display_label(node: &ChatLibraryContextNode) -> String {
    node.library_path
        .as_ref()
        .map(|path| {
            let value = path.to_string_lossy().replace('\\', "/");
            humanize_project_name(value.split('/').next_back().unwrap_or(&value))
        })
        .or_else(|| node.project.as_ref().map(project_display_label))
        .unwrap_or_else(|| "Global conversation".to_string())
}

fn project_display_label(project: &Project) -> String {
    project
        .library_path
        .as_ref()
        .and_then(|path| path.file_stem().or_else(|| path.file_name()))
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| project.name.clone())
        .trim_end_matches(".md")
        .split(['/', '\\'])
        .next_back()
        .map(humanize_project_name)
        .unwrap_or_else(|| humanize_project_name(&project.name))
}

fn humanize_project_name(value: &str) -> String {
    let mut out = String::new();
    let normalized = value
        .trim_end_matches(".md")
        .trim_end_matches(".MD")
        .replace(['_', '-', '/', '\\'], " ");
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut previous_lowercase_or_digit = false;
    let mut previous_uppercase = false;
    let mut previous_alpha = false;
    let mut previous_digit = false;
    for (index, character) in chars.iter().copied().enumerate() {
        let next_lowercase = chars
            .get(index + 1)
            .copied()
            .map(|next| next.is_ascii_lowercase())
            .unwrap_or(false);
        if character.is_ascii_uppercase()
            && (previous_lowercase_or_digit || (previous_uppercase && next_lowercase))
        {
            out.push(' ');
        } else if character.is_ascii_digit() && previous_alpha && !previous_digit {
            out.push(' ');
        } else if character.is_ascii_alphabetic() && previous_digit {
            out.push(' ');
        }
        if character == '.' {
            out.push(' ');
            previous_lowercase_or_digit = false;
            previous_uppercase = false;
            previous_alpha = false;
            previous_digit = false;
            continue;
        }
        previous_lowercase_or_digit = character.is_ascii_lowercase() || character.is_ascii_digit();
        previous_uppercase = character.is_ascii_uppercase();
        previous_alpha = character.is_ascii_alphabetic();
        previous_digit = character.is_ascii_digit();
        out.push(character);
    }
    out.split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!(
                    "{}{}",
                    first.to_uppercase().collect::<String>(),
                    chars.collect::<String>()
                ),
                None => String::new(),
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalized_project_lookup_key(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

async fn retrieve_chat_context_pack(
    db: &Database,
    config: &Config,
    query: &str,
    chat_context: &ChatProjectContext,
) -> Result<crate::domain::ContextPack> {
    let project_ids = context_project_ids_for_retrieval(db, chat_context).await?;
    if project_ids.is_empty() {
        return memory::retrieve_context_with_config(
            db,
            Some(config),
            memory::RetrievalRequest {
                query: query.to_string(),
                project_id: None,
                activity_id: None,
                limit: config.chat.memory_hit_limit,
            },
        )
        .await;
    }

    let mut packs = Vec::new();
    for project_id in &project_ids {
        packs.push(
            memory::retrieve_context_with_config(
                db,
                Some(config),
                memory::RetrievalRequest {
                    query: query.to_string(),
                    project_id: Some(*project_id),
                    activity_id: None,
                    limit: config.chat.memory_hit_limit,
                },
            )
            .await?,
        );
    }

    let mut hits = Vec::new();
    for pack in &packs {
        hits.extend(pack.hits.clone());
    }
    hits.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen = std::collections::HashSet::new();
    hits.retain(|hit| seen.insert(hit.item.id));
    hits.truncate(config.chat.memory_hit_limit.max(1));

    Ok(crate::domain::ContextPack {
        query: query.to_string(),
        project_id: project_ids.first().copied(),
        activity_id: None,
        generated_at: chrono::Utc::now(),
        hits,
    })
}

async fn context_project_ids_for_retrieval(
    db: &Database,
    chat_context: &ChatProjectContext,
) -> Result<Vec<Uuid>> {
    let all_projects = db.list_projects().await?;
    let mut ids = Vec::new();
    for node in &chat_context.nodes {
        if matches!(
            chat_context.scope,
            ContextScope::Node | ContextScope::NodeAndAncestors | ContextScope::ContextSet
        ) {
            if let Some(project) = &node.project {
                ids.push(project.id);
            }
        }
        if chat_context.scope == ContextScope::ContextSet {
            continue;
        }
        if chat_context.scope == ContextScope::Ancestors {
            // Ancestor expansion below includes exact parent records only; skip exact project here.
        } else if let Some(project) = &node.project {
            ids.push(project.id);
        }
        if let Some(library_path) = &node.library_path {
            for project in &all_projects {
                let Some(project_path) = project.library_path.as_ref() else {
                    continue;
                };
                let include = match chat_context.scope {
                    ContextScope::Node => project_path == library_path,
                    ContextScope::Subtree => library_path_contains(library_path, project_path),
                    ContextScope::Ancestors => {
                        project_path != library_path
                            && library_path_contains(project_path, library_path)
                    }
                    ContextScope::NodeAndAncestors => {
                        project_path == library_path
                            || library_path_contains(project_path, library_path)
                    }
                    ContextScope::ContextSet => project_path == library_path,
                };
                if include {
                    ids.push(project.id);
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn library_path_contains(parent: &Path, candidate: &Path) -> bool {
    candidate == parent || candidate.starts_with(parent)
}

async fn librarian_chat(
    State(state): State<AppState>,
    Json(input): Json<LibrarianChatRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let started_at = Instant::now();
    let message = input.message.trim();
    if message.is_empty() {
        return Err(anyhow::anyhow!("message must not be empty").into());
    }

    let config = state.config.read().await.clone();
    let chat_context = resolve_chat_project_context(&state, &config, &input, message).await?;
    let project = chat_context.primary_project();
    let project_id = chat_context.primary_project_id();
    let context_metadata = chat_context.metadata();
    let context_label = chat_context.label();
    let gated = gates::process_user_prompt(&state.db, &config, message, "librarian-chat").await?;
    let chat_session = match input.session_id {
        Some(session_id) => state.db.get_chat_session(session_id).await?,
        None => {
            state
                .db
                .create_chat_session(project_id, chat_session_title(&gated.content))
                .await?
        }
    };
    let previous_turns = state.db.list_chat_turns(chat_session.id).await?;

    let user_memory = state
        .db
        .add_memory_item(
            project_id,
            None,
            MemoryKind::UserMessage,
            Some("librarian-chat"),
            &gated.content,
            Some("admin:librarian-chat"),
            serde_json::json!({
                "project": project.map(|project| project.name.clone()),
                "scope": if chat_context.is_empty() { "global" } else { "project_context" },
                "context": context_metadata.clone(),
                "chat_session_id": chat_session.id,
                "memory_role": "raw_chat_turn",
                "durability": "transcript",
            }),
        )
        .await?;
    memory::embed_item(&state.db, &config, &user_memory).await?;
    state
        .db
        .add_chat_turn(
            chat_session.id,
            "user",
            &gated.content,
            Some(user_memory.id),
            serde_json::json!({
                "project": project.map(|project| project.name.clone()),
                "scope": if chat_context.is_empty() { "global" } else { "project_context" },
                "context": context_metadata.clone(),
            }),
        )
        .await?;

    let chat_result = if chat_context.has_suggestion() {
        let suggested_label = chat_context.suggested_label();
        let approval = state
            .db
            .create_tool_approval(
                "context",
                "switch",
                serde_json::json!({
                    "summary": format!("Switch chat context to {suggested_label}"),
                    "label": suggested_label,
                    "scope": chat_context.scope.label(),
                    "nodes": chat_context.suggested_nodes.iter().map(library_context_metadata).collect::<Vec<_>>(),
                    "user_message": gated.content.trim(),
                }),
            )
            .await?;
        LibrarianChatResult {
            reply: format!(
                "Похоже, этот диалог относится к контексту `{suggested_label}`. Переключить текущий контекст?"
            ),
            memory_hits: Vec::new(),
            mode: "context-switch-proposal",
            iterations: 0,
            trace: Vec::new(),
            ui: Some(serde_json::json!({
                "type": "context_switch",
                "label": suggested_label,
                "approval": approval,
                "context": {
                    "source": "suggested",
                    "label": suggested_label,
                "scope": chat_context.scope.label(),
                "nodes": chat_context.suggested_nodes.iter().map(library_context_metadata).collect::<Vec<_>>(),
                }
            })),
        }
    } else if let Some(result) =
        execute_slash_command(&state, &config, &chat_context, project, &gated.content).await?
    {
        result
    } else {
        let initial_context_pack =
            retrieve_chat_context_pack(&state.db, &config, &gated.content, &chat_context).await?;
        chat::run_librarian_chat_loop(
            &state.db,
            &config,
            &gated.content,
            project,
            &previous_turns,
            initial_context_pack,
        )
        .await?
    };
    let reply = chat_result.reply;
    let assistant_memory = state
        .db
        .add_memory_item(
            project_id,
            None,
            MemoryKind::AssistantMessage,
            Some("librarian-chat"),
            &reply,
            Some("admin:librarian-chat"),
            serde_json::json!({
                "project": project.map(|project| project.name.clone()),
                "scope": if chat_context.is_empty() { "global" } else { "project_context" },
                "context": context_metadata.clone(),
                "chat_session_id": chat_session.id,
                "memory_role": "raw_chat_turn",
                "durability": "transcript",
                "mode": chat_result.mode,
                "iterations": chat_result.iterations,
                "trace": chat_result.trace.clone(),
                "ui": chat_result.ui.clone(),
            }),
        )
        .await?;
    memory::embed_item(&state.db, &config, &assistant_memory).await?;
    state
        .db
        .add_chat_turn(
            chat_session.id,
            "assistant",
            &reply,
            Some(assistant_memory.id),
            serde_json::json!({
                "project": project.map(|project| project.name.clone()),
                "scope": if chat_context.is_empty() { "global" } else { "project_context" },
                "context": context_metadata.clone(),
                "mode": chat_result.mode,
                "iterations": chat_result.iterations,
                "ui": chat_result.ui.clone(),
            }),
        )
        .await?;

    let elapsed_ms = started_at.elapsed().as_millis();
    state
        .db
        .add_system_event(
            "chat_request",
            serde_json::json!({
                "session_id": chat_session.id,
                "project": project.map(|project| project.name.clone()),
                "context": context_metadata.clone(),
                "mode": chat_result.mode,
                "iterations": chat_result.iterations,
                "memory_hits": chat_result.memory_hits.len(),
                "elapsed_ms": elapsed_ms,
                "message_chars": gated.content.chars().count(),
                "reply_chars": reply.chars().count(),
            }),
        )
        .await?;

    Ok(Json(serde_json::json!({
        "session_id": chat_session.id,
        "reply": reply,
        "project": project.map(|project| project.name.clone()),
        "context": context_metadata,
        "context_label": context_label,
        "memory_hits": chat_result.memory_hits.clone(),
        "mode": chat_result.mode,
        "iterations": chat_result.iterations,
        "ui": chat_result.ui.clone(),
    })))
}

fn chat_session_title(message: &str) -> &str {
    message.lines().next().unwrap_or("New chat").trim()
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
    let provider = router::parse_provider_kind(input.provider.as_deref().unwrap_or("codex"))?;
    agent_policy::ensure_agent_job_allowed(
        &project,
        mount_mode,
        JobCreationSource::ExplicitUserAction,
    )?;
    let network_mode = router::default_network_mode_for_provider(
        &provider,
        input.allow_network.unwrap_or(false),
        input.secret_grant_token.is_some(),
    );
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
            provider,
            &gated.content,
            mount_mode,
            network_mode,
            input.secret_grant_token.as_deref(),
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

async fn execute_slash_command(
    state: &AppState,
    config: &Config,
    chat_context: &ChatProjectContext,
    project: Option<&Project>,
    message: &str,
) -> Result<Option<LibrarianChatResult>> {
    let Some(command_line) = message.trim().strip_prefix('/') else {
        return Ok(None);
    };
    let args = split_slash_args(command_line)?;
    if args.is_empty() {
        return Ok(Some(slash_reply(
            "Available commands: /context, /lib, /work, /mem, /settings, /remember, /help",
            serde_json::json!({ "command": "empty" }),
        )));
    }

    let command = args[0].to_ascii_lowercase();
    let result = if command == "lib" {
        execute_library_slash_command(&state.db, config, &args[1..]).await?
    } else if matches!(command.as_str(), "work" | "workspace") {
        execute_workspace_slash_command(&state.db, config, &args[1..]).await?
    } else if matches!(command.as_str(), "mem" | "memory") {
        execute_memory_slash_command(&state.db, config, project, &args[1..]).await?
    } else if matches!(command.as_str(), "settings" | "config") {
        execute_settings_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "context" | "ctx") {
        execute_context_slash_command(state, config, chat_context, &args[1..]).await?
    } else if matches!(command.as_str(), "agent" | "agents" | "job" | "jobs") {
        execute_agent_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "project" | "projects") {
        execute_project_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "approval" | "approvals") {
        execute_approval_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "prompt" | "prompts") {
        execute_prompt_slash_command(state, config, &args[1..]).await?
    } else if command == "remember" {
        let mut memory_args = vec!["remember".to_string(), "fact".to_string()];
        memory_args.extend(args.iter().skip(1).cloned());
        execute_memory_slash_command(&state.db, config, project, &memory_args).await?
    } else {
        match command.as_str() {
            "help" => slash_reply(
                slash_help(),
                serde_json::json!({ "command": command }),
            ),
            "library" => {
                execute_library_slash_command(&state.db, config, &["tree".to_string()]).await?
            }
            _ => slash_reply(
                "Unknown slash command. Try /help. Context commands live under /context; library commands live under /lib; working-folder commands live under /work; memory commands live under /mem; project commands live under /project; approvals live under /approval; prompt blocks live under /prompt; settings commands live under /settings; background agent jobs live under /agent.",
                serde_json::json!({ "command": command, "status": "unknown" }),
            ),
        }
    };

    Ok(Some(result))
}

async fn execute_library_slash_command(
    db: &Database,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            library_slash_help(),
            serde_json::json!({ "command": "lib" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            library_slash_help(),
            serde_json::json!({ "tool": "library", "command": command }),
        ),
        "tree" => {
            ensure_tool_permission(
                db,
                config,
                "library.read",
                config.tool_permissions.library_read,
            )
            .await?;
            let depth = args
                .get(1)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid depth: {error}"))?
                .unwrap_or(4);
            let roots = vec![library_tools::tree(config, LibraryRoot::Library, depth)?];
            slash_reply(
                &format!("Library tree loaded: {} root(s).", roots.len()),
                serde_json::json!({ "tool": "library", "command": command, "roots": roots }),
            )
        }
        "mkdir" => {
            let path = slash_single_path_arg(&args, "/lib mkdir <path>")?;
            let root = LibraryRoot::Library;
            ensure_tool_permission(
                db,
                config,
                "library.create",
                config.tool_permissions.library_create,
            )
            .await?;
            let tool_path = library_tools::create_folder(config, root, path)?;
            log_slash_library_event(
                db,
                "create_folder",
                serde_json::json!({ "root": root, "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Created folder in {:?}: {}", root, tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": root, "path": tool_path.path }),
            )
        }
        "touch" => {
            let path = slash_single_path_arg(&args, "/lib touch <path>")?;
            let root = LibraryRoot::Library;
            ensure_tool_permission(
                db,
                config,
                "library.create",
                config.tool_permissions.library_create,
            )
            .await?;
            let tool_path = library_tools::create_empty_file(config, root, path)?;
            log_slash_library_event(
                db,
                "create_empty_file",
                serde_json::json!({ "root": root, "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Created empty file in {:?}: {}", root, tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": root, "path": tool_path.path }),
            )
        }
        "read" => {
            ensure_tool_permission(
                db,
                config,
                "library.read",
                config.tool_permissions.library_read,
            )
            .await?;
            let path = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: /read <library-md-path>"))?;
            let content = if args.len() >= 4 {
                let start = parse_line_number(&args[2])?;
                let end = parse_line_number(&args[3])?;
                library_tools::read_markdown_lines(config, path, start, end)?.content
            } else {
                library_tools::read_markdown(config, path)?
            };
            slash_reply(
                &format!("Read `{path}`:\n\n{content}"),
                serde_json::json!({ "tool": "library", "command": command, "root": "library", "path": path }),
            )
        }
        "write" | "write-overwrite" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("Usage: /lib write-overwrite <library-md-path> <content>")
            })?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib write-overwrite <library-md-path> <content>");
            }
            let content = args[2..].join(" ");
            let tool_path = library_tools::write_markdown(config, path, &content)?;
            log_slash_library_event(
                db,
                "write_markdown",
                serde_json::json!({ "root": "library", "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Overwrote Markdown note: {}", tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": "library", "path": tool_path.path }),
            )
        }
        "append" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: /lib append <library-md-path> <content>"))?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib append <library-md-path> <content>");
            }
            let content = args[2..].join(" ");
            let tool_path = library_tools::append_markdown(config, path, &content)?;
            log_slash_library_event(
                db,
                "append_markdown",
                serde_json::json!({ "root": "library", "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Appended to Markdown note: {}", tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": "library", "path": tool_path.path }),
            )
        }
        "read-lines" => {
            ensure_tool_permission(
                db,
                config,
                "library.read",
                config.tool_permissions.library_read,
            )
            .await?;
            let path = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("Usage: /lib read-lines <library-md-path> <start> <end>")
            })?;
            if args.len() < 4 {
                anyhow::bail!("Usage: /lib read-lines <library-md-path> <start> <end>");
            }
            let slice = library_tools::read_markdown_lines(
                config,
                path,
                parse_line_number(&args[2])?,
                parse_line_number(&args[3])?,
            )?;
            slash_reply(
                &format!(
                    "Read `{}` lines {}-{} of {}:\n\n{}",
                    slice.path, slice.start_line, slice.end_line, slice.total_lines, slice.content
                ),
                serde_json::json!({ "tool": "library", "command": command, "slice": slice }),
            )
        }
        "cut-lines" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let edit = slash_line_edit(
                config,
                &args,
                None,
                "/lib cut-lines <library-md-path> <start> <end>",
            )?;
            log_slash_library_event(
                db,
                "cut_markdown_lines",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Cut `{}` lines {}-{}:\n\n{}",
                    edit.path, edit.start_line, edit.end_line, edit.removed
                ),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "replace-lines" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            if args.len() < 5 {
                anyhow::bail!(
                    "Usage: /lib replace-lines <library-md-path> <start> <end> <content>"
                );
            }
            let replacement = args[4..].join(" ");
            let edit = slash_line_edit(
                config,
                &args[..4],
                Some(&replacement),
                "/lib replace-lines <library-md-path> <start> <end> <content>",
            )?;
            log_slash_library_event(
                db,
                "replace_markdown_lines",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Replaced `{}` lines {}-{}.",
                    edit.path, edit.start_line, edit.end_line
                ),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "find" => {
            ensure_tool_permission(
                db,
                config,
                "library.read",
                config.tool_permissions.library_read,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib find <library-md-path> <query> [limit]");
            }
            let limit = args
                .get(3)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid limit: {error}"))?
                .unwrap_or(10);
            let matches = library_tools::find_markdown(config, &args[1], &args[2], limit)?;
            let mut reply = format!("Found {} match(es) in `{}`.", matches.len(), args[1]);
            for item in &matches {
                reply.push_str(&format!("\n{}: {}", item.line_number, item.line));
            }
            slash_reply(
                &reply,
                serde_json::json!({ "tool": "library", "command": command, "matches": matches }),
            )
        }
        "cut-find" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib cut-find <library-md-path> <query>");
            }
            let edit = library_tools::cut_first_markdown_match(config, &args[1], &args[2])?;
            log_slash_library_event(
                db,
                "cut_markdown_match",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Cut first match in `{}` at line {}:\n\n{}",
                    edit.path, edit.start_line, edit.removed
                ),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "replace-find" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            if args.len() < 4 {
                anyhow::bail!("Usage: /lib replace-find <library-md-path> <query> <content>");
            }
            let replacement = args[3..].join(" ");
            let edit = library_tools::replace_first_markdown_match(
                config,
                &args[1],
                &args[2],
                &replacement,
            )?;
            log_slash_library_event(
                db,
                "replace_markdown_match",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Replaced first match in `{}` at line {}.",
                    edit.path, edit.start_line
                ),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "cut-section" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib cut-section <library-md-path> <heading>");
            }
            let edit = library_tools::cut_markdown_section(config, &args[1], &args[2])?;
            log_slash_library_event(
                db,
                "cut_markdown_section",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line, "heading": args[2] }),
            )
            .await?;
            slash_reply(
                &format!("Cut section `{}` in `{}`.", args[2], edit.path),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "replace-section" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            if args.len() < 4 {
                anyhow::bail!("Usage: /lib replace-section <library-md-path> <heading> <content>");
            }
            let replacement = args[3..].join(" ");
            let edit =
                library_tools::replace_markdown_section(config, &args[1], &args[2], &replacement)?;
            log_slash_library_event(
                db,
                "replace_markdown_section",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line, "heading": args[2] }),
            )
            .await?;
            slash_reply(
                &format!("Replaced section `{}` in `{}`.", args[2], edit.path),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "move" | "rename" => {
            ensure_tool_permission(
                db,
                config,
                "library.move",
                config.tool_permissions.library_move,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib move <from> <to>");
            }
            let root = LibraryRoot::Library;
            let tool_path = library_tools::move_path(config, root, &args[1], &args[2])?;
            log_slash_library_event(
                db,
                "move",
                serde_json::json!({ "root": root, "from": args[1], "to": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Moved {:?} item to: {}", root, tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": root, "from": args[1], "to": tool_path.path }),
            )
        }
        "delete" => {
            ensure_tool_permission(
                db,
                config,
                "library.delete",
                config.tool_permissions.library_delete,
            )
            .await?;
            if args.len() < 3 || !args.iter().any(|arg| arg == "--yes") {
                return Ok(slash_reply(
                    "Delete is destructive. Use: /lib delete <path> --yes [--recursive]",
                    serde_json::json!({ "tool": "library", "command": command, "status": "needs_explicit_confirmation" }),
                ));
            }
            let root = LibraryRoot::Library;
            let recursive = args.iter().any(|arg| arg == "--recursive");
            let tool_path = library_tools::delete_path(config, root, &args[1], recursive)?;
            log_slash_library_event(
                db,
                "delete",
                serde_json::json!({ "root": root, "path": tool_path.path, "recursive": recursive }),
            )
            .await?;
            slash_reply(
                &format!("Deleted {:?} item: {}", root, tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": root, "path": tool_path.path, "recursive": recursive }),
            )
        }
        _ => slash_reply(
            "Unknown library command. Try /lib help.",
            serde_json::json!({ "tool": "library", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

fn slash_reply(reply: &str, trace: serde_json::Value) -> LibrarianChatResult {
    LibrarianChatResult {
        reply: reply.to_string(),
        iterations: 0,
        memory_hits: Vec::new(),
        trace: vec![trace],
        mode: "slash-command",
        ui: None,
    }
}

fn slash_reply_with_ui(
    reply: &str,
    trace: serde_json::Value,
    ui: serde_json::Value,
) -> LibrarianChatResult {
    LibrarianChatResult {
        reply: reply.to_string(),
        iterations: 0,
        memory_hits: Vec::new(),
        trace: vec![trace],
        mode: "slash-command",
        ui: Some(ui),
    }
}

fn agent_action_ui(command: &str, trace: &serde_json::Value) -> serde_json::Value {
    if trace
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|kind| kind != "agent_action")
    {
        let mut ui = trace.clone();
        ui["command"] = serde_json::Value::String(command.to_string());
        return ui;
    }

    let mut ui = serde_json::json!({
        "type": "agent_action",
        "command": command,
    });
    if let Some(value) = trace.get("status") {
        ui["status"] = value.clone();
    }
    if let Some(value) = trace.get("job") {
        ui["job"] = value.clone();
    }
    if let Some(value) = trace.get("jobs") {
        ui["jobs"] = value.clone();
    }
    if let Some(value) = trace.get("job_id") {
        ui["job_id"] = value.clone();
    }
    if let Some(value) = trace.get("source_job_id") {
        ui["source_job_id"] = value.clone();
    }
    if let Some(value) = trace.get("project") {
        ui["project"] = value.clone();
    }
    if let Some(value) = trace.get("report") {
        ui["report"] = value.clone();
    }
    if let Some(value) = trace.get("events") {
        ui["events"] = value.clone();
    }
    ui
}

fn agent_slash_reply(reply: &str, command: &str, trace: serde_json::Value) -> LibrarianChatResult {
    let ui = agent_action_ui(command, &trace);
    slash_reply_with_ui(reply, trace, ui)
}

fn slash_help() -> &'static str {
    "Available command groups:\n/context help - show or change the active chat context\n/lib help - Markdown library and library hierarchy tools\n/work help - default working-folder tools under Projects\n/project help - library project and workspace attachment tools\n/mem help - durable memory tools\n/approval help - pending tool approval proposals\n/prompt help - prompt builder block presets\n/settings help - inspect and change guarded settings\n/agent help - explicit background agent jobs\n\nLibrary projects live in /lib. Implementation/product working folders live in /work or attached external project records."
}

async fn execute_context_slash_command(
    state: &AppState,
    config: &Config,
    chat_context: &ChatProjectContext,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            context_slash_help(),
            serde_json::json!({ "command": "context" }),
        ));
    }
    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            context_slash_help(),
            serde_json::json!({ "tool": "context", "command": command }),
        ),
        "show" | "status" => slash_reply(
            &format!(
                "Current context: {} ({})",
                chat_context.label(),
                chat_context.scope.label()
            ),
            serde_json::json!({
                "tool": "context",
                "command": command,
                "context": chat_context.metadata(),
            }),
        ),
        "clear" => context_update_reply(
            "Context cleared. Future messages will use the global conversation until you select another context.",
            Vec::new(),
            chat_context.scope,
            "clear",
        ),
        "scope" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: /context scope <node|subtree|ancestors|node+ancestors|context-set>");
            }
            let scope = parse_context_scope(&args[1])?;
            context_update_reply(
                &format!(
                    "Context scope set to {} for {}.",
                    scope.label(),
                    chat_context.label()
                ),
                chat_context.nodes.clone(),
                scope,
                "scope",
            )
        }
        "set" | "use" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: /context set <library-path|project-name|project-id>");
            }
            let nodes = resolve_context_nodes_from_args(state, config, &args[1..]).await?;
            context_update_reply(
                &format!("Context set to {}.", context_label_for_nodes(&nodes)),
                nodes,
                chat_context.scope,
                "set",
            )
        }
        "add" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: /context add <library-path|project-name|project-id>");
            }
            let mut nodes = chat_context.nodes.clone();
            for node in resolve_context_nodes_from_args(state, config, &args[1..]).await? {
                if !nodes.iter().any(|existing| same_context_node(existing, &node)) {
                    nodes.push(node);
                }
            }
            context_update_reply(
                &format!("Context set to {}.", context_label_for_nodes(&nodes)),
                nodes,
                chat_context.scope,
                "add",
            )
        }
        "remove" | "rm" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: /context remove <library-path|project-name|project-id>");
            }
            let remove_nodes = resolve_context_nodes_from_args(state, config, &args[1..]).await?;
            let mut nodes = chat_context.nodes.clone();
            nodes.retain(|node| {
                !remove_nodes
                    .iter()
                    .any(|remove| same_context_node(node, remove))
            });
            context_update_reply(
                &format!("Context set to {}.", context_label_for_nodes(&nodes)),
                nodes,
                chat_context.scope,
                "remove",
            )
        }
        _ => slash_reply(
            "Unknown context command. Try /context help.",
            serde_json::json!({ "tool": "context", "command": command, "status": "unknown" }),
        ),
    };
    Ok(result)
}

async fn resolve_context_nodes_from_args(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<Vec<ChatLibraryContextNode>> {
    let projects = state.db.list_projects().await?;
    let mut nodes = Vec::new();
    for value in args {
        let node = resolve_library_context_node(config, &projects, value)?;
        if !nodes
            .iter()
            .any(|existing| same_context_node(existing, &node))
        {
            nodes.push(node);
        }
    }
    Ok(nodes)
}

fn context_update_reply(
    reply: &str,
    nodes: Vec<ChatLibraryContextNode>,
    scope: ContextScope,
    action: &str,
) -> LibrarianChatResult {
    LibrarianChatResult {
        reply: reply.to_string(),
        iterations: 0,
        memory_hits: Vec::new(),
        trace: Vec::new(),
        mode: "slash-command",
        ui: Some(serde_json::json!({
            "type": "context_update",
            "action": action,
            "context": {
                "source": "slash-command",
                "label": context_label_for_nodes(&nodes),
                "scope": scope.label(),
                "nodes": nodes.iter().map(library_context_metadata).collect::<Vec<_>>(),
            }
        })),
    }
}

fn context_slash_help() -> &'static str {
    "Context commands live under /context:\n/context show - show the current chat context\n/context scope <node|subtree|ancestors|node+ancestors|context-set> - change memory scope; ancestors excludes the current node, node+ancestors includes it\n/context set <library-path|project-name|project-id> - replace the context\n/context add <library-path|project-name|project-id> - add a context node\n/context remove <library-path|project-name|project-id> - remove a context node\n/context clear - return to global conversation"
}

fn library_slash_help() -> &'static str {
    "Library commands live under /lib:\n/lib tree [depth]\n/lib mkdir <path>\n/lib touch <path>\n/lib read <library-md-path> [start] [end]\n/lib read-lines <library-md-path> <start> <end>\n/lib write-overwrite <library-md-path> <content>\n/lib append <library-md-path> <content>\n/lib cut-lines <library-md-path> <start> <end>\n/lib replace-lines <library-md-path> <start> <end> <content>\n/lib find <library-md-path> <query> [limit]\n/lib cut-find <library-md-path> <query>\n/lib replace-find <library-md-path> <query> <content>\n/lib cut-section <library-md-path> <heading>\n/lib replace-section <library-md-path> <heading> <content>\n/lib move <from> <to>\n/lib delete <path> --yes [--recursive]"
}

async fn execute_workspace_slash_command(
    db: &Database,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            workspace_slash_help(),
            serde_json::json!({ "command": "work" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let root = LibraryRoot::Projects;
    let result = match command.as_str() {
        "help" => slash_reply(
            workspace_slash_help(),
            serde_json::json!({ "tool": "workspace", "command": command }),
        ),
        "tree" => {
            ensure_tool_permission(db, config, "workspace.read", ToolPermissionPolicy::Auto)
                .await?;
            let depth = args
                .get(1)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid depth: {error}"))?
                .unwrap_or(4);
            let tree = library_tools::tree(config, root, depth)?;
            slash_reply(
                "Workspace tree loaded.",
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "tree": tree }),
            )
        }
        "mkdir" => {
            let path = slash_single_path_arg(args, "/work mkdir <path>")?;
            ensure_tool_permission(
                db,
                config,
                "workspace.create",
                config.tool_permissions.workspace_create,
            )
            .await?;
            let tool_path = library_tools::create_folder(config, root, path)?;
            log_workspace_event(
                db,
                "create_folder",
                serde_json::json!({ "root": root, "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Created workspace folder: {}", tool_path.path),
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "path": tool_path.path }),
            )
        }
        "touch" => {
            let path = slash_single_path_arg(args, "/work touch <path>")?;
            ensure_tool_permission(
                db,
                config,
                "workspace.create",
                config.tool_permissions.workspace_create,
            )
            .await?;
            let tool_path = library_tools::create_empty_file(config, root, path)?;
            log_workspace_event(
                db,
                "create_empty_file",
                serde_json::json!({ "root": root, "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Created workspace file: {}", tool_path.path),
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "path": tool_path.path }),
            )
        }
        "move" | "rename" => {
            ensure_tool_permission(
                db,
                config,
                "workspace.move",
                config.tool_permissions.workspace_move,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /work move <from> <to>");
            }
            let tool_path = library_tools::move_path(config, root, &args[1], &args[2])?;
            log_workspace_event(
                db,
                "move",
                serde_json::json!({ "root": root, "from": args[1], "to": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Moved workspace item to: {}", tool_path.path),
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "from": args[1], "to": tool_path.path }),
            )
        }
        "delete" => {
            ensure_tool_permission(
                db,
                config,
                "workspace.delete",
                config.tool_permissions.workspace_delete,
            )
            .await?;
            if args.len() < 3 || !args.iter().any(|arg| arg == "--yes") {
                return Ok(slash_reply(
                    "Delete is destructive. Use: /work delete <path> --yes [--recursive]",
                    serde_json::json!({ "tool": "workspace", "command": command, "status": "needs_explicit_confirmation" }),
                ));
            }
            let recursive = args.iter().any(|arg| arg == "--recursive");
            let tool_path = library_tools::delete_path(config, root, &args[1], recursive)?;
            log_workspace_event(
                db,
                "delete",
                serde_json::json!({ "root": root, "path": tool_path.path, "recursive": recursive }),
            )
            .await?;
            slash_reply(
                &format!("Deleted workspace item: {}", tool_path.path),
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "path": tool_path.path, "recursive": recursive }),
            )
        }
        _ => slash_reply(
            "Unknown workspace command. Try /work help.",
            serde_json::json!({ "tool": "workspace", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

fn workspace_slash_help() -> &'static str {
    "Workspace commands live under /work and operate only inside Librarian/Projects:\n/work tree [depth]\n/work mkdir <path>\n/work touch <path>\n/work move <from> <to>\n/work delete <path> --yes [--recursive]\n\nUse this for default implementation/product folders, not for library knowledge."
}

async fn execute_project_slash_command(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            project_slash_help(),
            serde_json::json!({ "command": "project" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            project_slash_help(),
            serde_json::json!({ "tool": "project", "command": command }),
        ),
        "list" => {
            let projects = state.db.list_projects().await?;
            let mut reply = format!("Projects: {} registered.", projects.len());
            for project in &projects {
                reply.push_str(&format!("\n{}", format_project_summary(project)));
            }
            slash_reply(
                &reply,
                serde_json::json!({ "tool": "project", "command": command, "projects": projects }),
            )
        }
        "status" => {
            let project = slash_project_arg(args, "/project status <project>")?;
            let project = state.db.get_project_by_name_or_id(project).await?;
            slash_reply(
                &format_project_summary(&project),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        "map" => {
            let projects = state.db.list_projects().await?;
            let map = build_project_map(config, projects)?;
            slash_reply(
                &format!(
                    "Project map loaded: {} linked project(s).",
                    map["linked_project_count"].as_u64().unwrap_or(0)
                ),
                serde_json::json!({ "tool": "project", "command": command, "map": map }),
            )
        }
        "create" => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.create",
                config.tool_permissions.library_create,
            )
            .await?;
            ensure_tool_permission(
                &state.db,
                config,
                "workspace.create",
                config.tool_permissions.workspace_create,
            )
            .await?;
            let request = parse_project_create_args(&args[1..])?;
            let library_path = request
                .library_path
                .unwrap_or_else(|| format!("projects/{}", project_folder_name(&request.name)));
            let library_path = library_tools::normalize_tool_relative_path(&library_path)?;
            library_tools::create_folder(config, LibraryRoot::Library, &library_path)?;

            let workspace_path = if let Some(path) = request.workspace_path {
                canonical_existing_dir(&path)?
            } else {
                let relative = project_folder_name(&request.name);
                library_tools::create_folder(config, LibraryRoot::Projects, &relative)?;
                config.home.join("Projects").join(relative).canonicalize()?
            };
            let project = state.db.add_project(&request.name, &workspace_path).await?;
            let project = state
                .db
                .attach_project_library_path(project.id, PathBuf::from(&library_path).as_path())
                .await?;
            log_project_event(
                &state.db,
                "create",
                serde_json::json!({
                    "project_id": project.id,
                    "name": project.name.clone(),
                    "library_path": project.library_path.clone(),
                    "workspace_path": project.path.clone(),
                }),
            )
            .await?;
            slash_reply(
                &format!("Created project.\n{}", format_project_summary(&project)),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        "attach-library" => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.move",
                config.tool_permissions.library_move,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /project attach-library <project> <library-path>");
            }
            let project = state.db.get_project_by_name_or_id(&args[1]).await?;
            let library_path = library_tools::normalize_tool_relative_path(&args[2])?;
            let project = state
                .db
                .attach_project_library_path(project.id, PathBuf::from(&library_path).as_path())
                .await?;
            log_project_event(
                &state.db,
                "attach_library",
                serde_json::json!({ "project_id": project.id, "library_path": project.library_path.clone() }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Attached library path.\n{}",
                    format_project_summary(&project)
                ),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        "detach-library" => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.move",
                config.tool_permissions.library_move,
            )
            .await?;
            let project = slash_project_arg(args, "/project detach-library <project>")?;
            let project = state.db.get_project_by_name_or_id(project).await?;
            let project = state.db.detach_project_library_path(project.id).await?;
            log_project_event(
                &state.db,
                "detach_library",
                serde_json::json!({ "project_id": project.id }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Detached library path.\n{}",
                    format_project_summary(&project)
                ),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        "attach-workspace" => {
            ensure_tool_permission(
                &state.db,
                config,
                "workspace.move",
                config.tool_permissions.workspace_move,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /project attach-workspace <project> <existing-directory>");
            }
            let project = state.db.get_project_by_name_or_id(&args[1]).await?;
            let workspace_path = canonical_existing_dir(&args[2])?;
            let project = state
                .db
                .update_project_workspace_path(project.id, &workspace_path)
                .await?;
            log_project_event(
                &state.db,
                "attach_workspace",
                serde_json::json!({ "project_id": project.id, "workspace_path": project.path.clone() }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Attached workspace path.\n{}",
                    format_project_summary(&project)
                ),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        _ => slash_reply(
            "Unknown project command. Try /project help.",
            serde_json::json!({ "tool": "project", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

struct ProjectCreateSlashRequest {
    name: String,
    library_path: Option<String>,
    workspace_path: Option<String>,
}

fn parse_project_create_args(args: &[String]) -> Result<ProjectCreateSlashRequest> {
    let name = args
        .first()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Usage: /project create <name> [--library path] [--workspace existing-directory]"
            )
        })?
        .clone();
    let mut library_path = None;
    let mut workspace_path = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--library" | "--library-path" => {
                index += 1;
                library_path = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow::anyhow!("--library requires a value"))?
                        .clone(),
                );
            }
            "--workspace" | "--workspace-path" => {
                index += 1;
                workspace_path = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow::anyhow!("--workspace requires a value"))?
                        .clone(),
                );
            }
            value => anyhow::bail!("Unknown /project create flag `{value}`"),
        }
        index += 1;
    }
    Ok(ProjectCreateSlashRequest {
        name,
        library_path,
        workspace_path,
    })
}

fn slash_project_arg<'a>(args: &'a [String], usage: &str) -> Result<&'a str> {
    args.get(1)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))
}

fn canonical_existing_dir(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if !path.is_dir() {
        anyhow::bail!("Workspace path must be an existing directory");
    }
    path.canonicalize()
        .map_err(|error| anyhow::anyhow!("Failed to resolve workspace path: {error}"))
}

fn admin_shell_path(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    if text
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '(' | ')' | '&' | ';'))
    {
        format!("\"{}\"", text.replace('"', "\\\""))
    } else {
        text
    }
}

fn project_folder_name(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if matches!(ch, '-' | '_' | '.') {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "project".to_string()
    } else {
        trimmed
    }
}

fn project_workspace_folder_name(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "Project".to_string()
    } else {
        trimmed
    }
}

fn format_project_summary(project: &Project) -> String {
    format!(
        "{} `{}` library={} workspace={}",
        project.id,
        project.name,
        project
            .library_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string()),
        project.path.display()
    )
}

async fn log_project_event(db: &Database, action: &str, payload: serde_json::Value) -> Result<()> {
    db.add_system_event(
        "project_tool",
        serde_json::json!({
            "action": action,
            "source": "slash-command",
            "payload": payload,
        }),
    )
    .await?;
    Ok(())
}

fn build_project_map(config: &Config, projects: Vec<Project>) -> Result<serde_json::Value> {
    let mut by_library_path: HashMap<String, Vec<Project>> = HashMap::new();
    let mut detached = Vec::new();
    for project in projects {
        match &project.library_path {
            Some(path) => by_library_path
                .entry(path.to_string_lossy().replace('\\', "/"))
                .or_default()
                .push(project),
            None => detached.push(project),
        }
    }
    let linked_project_count = by_library_path.values().map(Vec::len).sum::<usize>();
    let root = library_tools::tree(config, LibraryRoot::Library, 12)?;
    let tree = project_map_node(&root, &by_library_path);
    Ok(serde_json::json!({
        "root": tree,
        "linked_project_count": linked_project_count,
        "detached_projects": detached,
        "metaphor": {
            "folder_with_folders": "rack_or_row",
            "folder_with_files": "shelf",
            "markdown_file": "book",
            "file": "artifact"
        }
    }))
}

fn project_map_node(
    entry: &library_tools::LibraryEntry,
    projects: &HashMap<String, Vec<Project>>,
) -> serde_json::Value {
    let child_nodes = entry
        .children
        .iter()
        .map(|child| project_map_node(child, projects))
        .collect::<Vec<_>>();
    let linked_projects = projects.get(&entry.path).cloned().unwrap_or_default();
    serde_json::json!({
        "name": entry.name,
        "path": entry.path,
        "kind": entry.kind,
        "visual_kind": project_visual_kind(entry),
        "projects": linked_projects,
        "children": child_nodes,
    })
}

fn project_visual_kind(entry: &library_tools::LibraryEntry) -> &'static str {
    match entry.kind {
        library_tools::LibraryEntryKind::Markdown => "book",
        library_tools::LibraryEntryKind::File => "artifact",
        library_tools::LibraryEntryKind::Folder => {
            if entry
                .children
                .iter()
                .any(|child| child.kind == library_tools::LibraryEntryKind::Folder)
            {
                "rack"
            } else {
                "shelf"
            }
        }
    }
}

fn project_slash_help() -> &'static str {
    "Project commands live under /project:\n/project list\n/project map\n/project status <project>\n/project create <name> [--library path] [--workspace existing-directory]\n/project attach-library <project> <library-path>\n/project detach-library <project>\n/project attach-workspace <project> <existing-directory>\n\nA project can have a Library documentation path and one implementation/workspace directory. Default create makes Library/projects/{name} and Projects/{name}."
}

#[path = "admin/approvals.rs"]
mod approvals;
use approvals::{
    approval_payload_string, approval_project_library_path, approve_and_execute_tool_approval,
    execute_approval_slash_command, execute_approved_tool_approval, parse_json_payload,
    reject_tool_approval_by_id,
};

#[path = "admin/prompt_commands.rs"]
mod prompt_commands;
use prompt_commands::{
    execute_prompt_slash_command, parse_prompt_add_block_args, render_prompt_blocks,
};

async fn execute_memory_slash_command(
    db: &Database,
    config: &Config,
    project: Option<&Project>,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            memory_slash_help(),
            serde_json::json!({ "command": "mem" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            memory_slash_help(),
            serde_json::json!({ "tool": "memory", "command": command }),
        ),
        "remember" | "add" => {
            ensure_tool_permission(
                db,
                config,
                "memory.write",
                config.tool_permissions.memory_write,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!(
                    "Usage: /mem remember <fact|decision|instruction|status|summary> <content>"
                );
            }
            let kind = parse_memory_kind_token(&args[1])?;
            let content = args[2..].join(" ");
            let item = db
                .add_memory_item(
                    project.map(|project| project.id),
                    None,
                    kind.clone(),
                    None,
                    &content,
                    Some("admin:slash-memory"),
                    serde_json::json!({
                        "tool": "memory",
                        "command": command,
                        "memory_role": "durable_memory",
                        "memory_type": durable_memory_type(&kind),
                        "retrieval_priority": durable_memory_priority(&kind),
                        "durability": "durable",
                        "scope": if project.is_some() { "project" } else { "global" },
                        "project": project.map(|project| project.name.clone()),
                    }),
                )
                .await?;
            memory::embed_item(db, config, &item).await?;
            db.add_system_event(
                "memory_tool",
                serde_json::json!({
                    "action": "remember",
                    "source": "slash-command",
                    "memory_id": item.id,
                    "kind": item.kind,
                    "scope": if project.is_some() { "project" } else { "global" },
                    "project": project.map(|project| project.name.clone()),
                }),
            )
            .await?;
            slash_reply(
                &format!("Remembered {:?}: {}", item.kind, item.content),
                serde_json::json!({
                    "tool": "memory",
                    "command": command,
                    "memory_id": item.id,
                    "kind": item.kind,
                    "scope": if project.is_some() { "project" } else { "global" },
                }),
            )
        }
        "supersede" | "contradict" => {
            ensure_tool_permission(
                db,
                config,
                "memory.write",
                config.tool_permissions.memory_write,
            )
            .await?;
            if args.len() < 4 {
                anyhow::bail!(
                    "Usage: /mem {command} <old-memory-id> <fact|decision|instruction|status|summary> <content>"
                );
            }
            let old_id = args[1]
                .parse::<Uuid>()
                .map_err(|error| anyhow::anyhow!("Invalid memory id: {error}"))?;
            let old = db.get_memory_item(old_id).await?;
            let kind = parse_memory_kind_token(&args[2])?;
            let content = args[3..].join(" ");
            let supersedes_id = (command == "supersede").then_some(old.id);
            let contradicts_id = (command == "contradict").then_some(old.id);
            let item = db
                .add_linked_memory_item(
                    project.map(|project| project.id),
                    None,
                    kind.clone(),
                    old.topic.as_deref(),
                    &content,
                    Some("admin:slash-memory"),
                    serde_json::json!({
                        "tool": "memory",
                        "command": command,
                        "memory_role": "durable_memory",
                        "memory_type": durable_memory_type(&kind),
                        "retrieval_priority": durable_memory_priority(&kind),
                        "durability": "durable",
                        "scope": if project.is_some() { "project" } else { "global" },
                        "project": project.map(|project| project.name.clone()),
                        "linked_memory_id": old.id,
                    }),
                    supersedes_id,
                    contradicts_id,
                )
                .await?;
            memory::embed_item(db, config, &item).await?;
            db.add_system_event(
                "memory_tool",
                serde_json::json!({
                    "action": command,
                    "source": "slash-command",
                    "memory_id": item.id,
                    "linked_memory_id": old.id,
                    "kind": item.kind,
                    "scope": if project.is_some() { "project" } else { "global" },
                    "project": project.map(|project| project.name.clone()),
                }),
            )
            .await?;
            slash_reply(
                &format!(
                    "{command} memory `{}` with {:?}: {}",
                    old.id, item.kind, item.content
                ),
                serde_json::json!({
                    "tool": "memory",
                    "command": command,
                    "memory_id": item.id,
                    "linked_memory_id": old.id,
                    "kind": item.kind,
                    "scope": if project.is_some() { "project" } else { "global" },
                }),
            )
        }
        "cleanup-legacy-local-responder" | "cleanup-local-responder" | "cleanup-legacy" => {
            ensure_tool_permission(
                db,
                config,
                "memory.write",
                config.tool_permissions.memory_write,
            )
            .await?;
            let limit = args
                .iter()
                .position(|arg| arg == "--limit")
                .and_then(|index| args.get(index + 1))
                .map(|value| value.parse::<i64>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid --limit value: {error}"))?
                .unwrap_or(100)
                .clamp(1, 10_000);
            let total = db.count_legacy_local_memory_responder_items().await?;
            let candidates = db.legacy_local_memory_responder_items(limit).await?;
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                let mut reply = format!(
                    "Legacy local responder cleanup found {total} candidate(s); showing {}.",
                    candidates.len()
                );
                for item in candidates.iter().take(5) {
                    reply.push_str(&format!(
                        "\n{} {} {:?}: {}",
                        item.observed_at.format("%Y-%m-%d %H:%M"),
                        item.id,
                        item.kind,
                        item.content.chars().take(120).collect::<String>()
                    ));
                }
                reply.push_str("\nRun `/mem cleanup-legacy-local-responder --yes` to delete them.");
                return Ok(slash_reply(
                    &reply,
                    serde_json::json!({
                        "tool": "memory",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                        "candidate_count": total,
                        "shown_count": candidates.len(),
                    }),
                ));
            }
            let ids = candidates.iter().map(|item| item.id).collect::<Vec<_>>();
            let deleted = db.delete_memory_items(&ids).await?;
            db.add_system_event(
                "memory_tool",
                serde_json::json!({
                    "action": "cleanup_legacy_local_responder",
                    "source": "slash-command",
                    "deleted": deleted,
                    "candidate_count": total,
                }),
            )
            .await?;
            slash_reply(
                &format!("Deleted {deleted} legacy local responder memory item(s)."),
                serde_json::json!({
                    "tool": "memory",
                    "command": command,
                    "deleted": deleted,
                    "candidate_count": total,
                }),
            )
        }
        "recent" => {
            let limit = args
                .get(1)
                .map(|value| value.parse::<i64>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid limit: {error}"))?
                .unwrap_or(10)
                .clamp(1, 50);
            let items = db
                .recent_memory_for_project(project.map(|project| project.id), limit)
                .await?;
            let items = items
                .into_iter()
                .filter(|item| is_visible_durable_memory_item(item))
                .collect::<Vec<_>>();
            let mut reply = format!("Recent memory: {} item(s).", items.len());
            for item in &items {
                reply.push_str(&format!(
                    "\n{} {} {:?}: {}",
                    item.observed_at.format("%Y-%m-%d %H:%M"),
                    item.id,
                    item.kind,
                    item.content
                ));
            }
            slash_reply(
                &reply,
                serde_json::json!({
                    "tool": "memory",
                    "command": command,
                    "items": items,
                    "scope": if project.is_some() { "project" } else { "global" },
                }),
            )
        }
        _ => slash_reply(
            "Unknown memory command. Try /mem help.",
            serde_json::json!({ "tool": "memory", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

fn memory_slash_help() -> &'static str {
    "Memory commands live under /mem:\n/mem remember <fact|decision|instruction|status|summary> <content>\n/mem supersede <old-memory-id> <kind> <content>\n/mem contradict <old-memory-id> <kind> <content>\n/remember <content> - shortcut for /mem remember fact <content>\n/mem recent [limit]\n/mem cleanup-legacy-local-responder [--limit n] --yes\n\nMemory is stored in the current chat scope: selected project when present, otherwise global."
}

async fn execute_settings_slash_command(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            settings_slash_help(),
            serde_json::json!({ "command": "settings" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            settings_slash_help(),
            serde_json::json!({ "tool": "settings", "command": command }),
        ),
        "tool-permissions" | "permissions" => slash_reply(
            &format_tool_permissions(&config.tool_permissions),
            serde_json::json!({
                "tool": "settings",
                "command": command,
                "tool_permissions": config.tool_permissions,
            }),
        ),
        "set-permission-preset" | "permission-preset" | "preset" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            if args.len() < 2 {
                anyhow::bail!(
                    "Usage: /settings permission-preset <balanced|autopilot|confirm|locked_down> --yes"
                );
            }
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(slash_reply(
                    "Settings changes require explicit confirmation. Use: /settings permission-preset <balanced|autopilot|confirm|locked_down> --yes",
                    serde_json::json!({
                        "tool": "settings",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                    }),
                ));
            }
            let preset = parse_tool_permission_preset(&args[1])?;
            let config_path = {
                let mut writable_config = state.config.write().await;
                apply_tool_permission_preset(&mut writable_config.tool_permissions, preset);
                writable_config.save()?;
                writable_config.config_path.clone()
            };
            state
                .db
                .add_system_event(
                    "settings_tool",
                    serde_json::json!({
                        "action": "set_tool_permission_preset",
                        "source": "slash-command",
                        "preset": preset,
                        "config_path": config_path,
                    }),
                )
                .await?;
            slash_reply(
                &format!(
                    "Updated tool permissions preset to `{}`.",
                    preset_label(preset)
                ),
                serde_json::json!({
                    "tool": "settings",
                    "command": command,
                    "preset": preset,
                }),
            )
        }
        "set-tool-permission" | "set-permission" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            if args.len() < 4 {
                anyhow::bail!("Usage: /settings set-tool-permission <key> <auto|ask|deny> --yes");
            }
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(slash_reply(
                    "Settings changes require explicit confirmation. Use: /settings set-tool-permission <key> <auto|ask|deny> --yes",
                    serde_json::json!({
                        "tool": "settings",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                    }),
                ));
            }
            let key = args[1].as_str();
            let policy = parse_tool_permission_policy(&args[2])?;
            let config_path = {
                let mut writable_config = state.config.write().await;
                set_tool_permission(&mut writable_config.tool_permissions, key, policy)?;
                writable_config.save()?;
                writable_config.config_path.clone()
            };
            state
                .db
                .add_system_event(
                    "settings_tool",
                    serde_json::json!({
                        "action": "set_tool_permission",
                        "source": "slash-command",
                        "key": key,
                        "policy": policy,
                        "config_path": config_path,
                    }),
                )
                .await?;
            slash_reply(
                &format!(
                    "Updated tool permission `{key}` to `{}`.",
                    policy_label(policy)
                ),
                serde_json::json!({
                    "tool": "settings",
                    "command": command,
                    "key": key,
                    "policy": policy,
                }),
            )
        }
        _ => slash_reply(
            "Unknown settings command. Try /settings help.",
            serde_json::json!({ "tool": "settings", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

fn settings_slash_help() -> &'static str {
    "Settings commands live under /settings:\n/settings tool-permissions - show current tool permission policies\n/settings permission-preset <balanced|autopilot|confirm|locked_down> --yes - apply a whole permission package\n/settings set-tool-permission <key> <auto|ask|deny> --yes - update one permission and mark the package custom\n\nPermission keys: library_read, library_create, library_edit_markdown, library_move, library_delete, workspace_create, workspace_move, workspace_delete, memory_write, settings_change, agent_launch, context_switch."
}

fn format_tool_permissions(permissions: &ToolPermissionsConfig) -> String {
    format!(
        "Tool permissions:\n\
preset = {}\n\
library_read = {}\n\
library_create = {}\n\
library_edit_markdown = {}\n\
library_move = {}\n\
library_delete = {}\n\
workspace_create = {}\n\
workspace_move = {}\n\
workspace_delete = {}\n\
memory_write = {}\n\
settings_change = {}\n\
agent_launch = {}\n\
context_switch = {}",
        preset_label(permissions.preset),
        policy_label(permissions.library_read),
        policy_label(permissions.library_create),
        policy_label(permissions.library_edit_markdown),
        policy_label(permissions.library_move),
        policy_label(permissions.library_delete),
        policy_label(permissions.workspace_create),
        policy_label(permissions.workspace_move),
        policy_label(permissions.workspace_delete),
        policy_label(permissions.memory_write),
        policy_label(permissions.settings_change),
        policy_label(permissions.agent_launch),
        policy_label(permissions.context_switch),
    )
}

fn parse_tool_permission_preset(value: &str) -> Result<ToolPermissionPreset> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "balanced" => Ok(ToolPermissionPreset::Balanced),
        "autopilot" | "auto" => Ok(ToolPermissionPreset::Autopilot),
        "confirm" | "ask" => Ok(ToolPermissionPreset::Confirm),
        "locked_down" | "lockeddown" | "locked" | "deny" => Ok(ToolPermissionPreset::LockedDown),
        "custom" => Ok(ToolPermissionPreset::Custom),
        _ => anyhow::bail!(
            "Tool permission preset must be balanced, autopilot, confirm, or locked_down"
        ),
    }
}

fn parse_tool_permission_policy(value: &str) -> Result<ToolPermissionPolicy> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(ToolPermissionPolicy::Auto),
        "ask" => Ok(ToolPermissionPolicy::Ask),
        "deny" => Ok(ToolPermissionPolicy::Deny),
        _ => anyhow::bail!("Tool permission policy must be auto, ask, or deny"),
    }
}

fn set_tool_permission(
    permissions: &mut ToolPermissionsConfig,
    key: &str,
    policy: ToolPermissionPolicy,
) -> Result<()> {
    match key.trim().to_ascii_lowercase().as_str() {
        "library_read" => permissions.library_read = policy,
        "library_create" => permissions.library_create = policy,
        "library_edit_markdown" => permissions.library_edit_markdown = policy,
        "library_move" => permissions.library_move = policy,
        "library_delete" => permissions.library_delete = policy,
        "workspace_create" => permissions.workspace_create = policy,
        "workspace_move" => permissions.workspace_move = policy,
        "workspace_delete" => permissions.workspace_delete = policy,
        "memory_write" => permissions.memory_write = policy,
        "settings_change" => permissions.settings_change = policy,
        "agent_launch" => permissions.agent_launch = policy,
        "context_switch" => permissions.context_switch = policy,
        _ => anyhow::bail!("Unknown tool permission key `{key}`. Try /settings tool-permissions."),
    }
    permissions.preset = ToolPermissionPreset::Custom;
    Ok(())
}

fn apply_tool_permission_preset(
    permissions: &mut ToolPermissionsConfig,
    preset: ToolPermissionPreset,
) {
    *permissions = match preset {
        ToolPermissionPreset::Balanced => ToolPermissionsConfig::default(),
        ToolPermissionPreset::Autopilot => ToolPermissionsConfig {
            preset,
            library_read: ToolPermissionPolicy::Auto,
            library_create: ToolPermissionPolicy::Auto,
            library_edit_markdown: ToolPermissionPolicy::Auto,
            library_move: ToolPermissionPolicy::Auto,
            library_delete: ToolPermissionPolicy::Ask,
            workspace_create: ToolPermissionPolicy::Auto,
            workspace_move: ToolPermissionPolicy::Auto,
            workspace_delete: ToolPermissionPolicy::Ask,
            memory_write: ToolPermissionPolicy::Auto,
            settings_change: ToolPermissionPolicy::Ask,
            agent_launch: ToolPermissionPolicy::Auto,
            context_switch: ToolPermissionPolicy::Auto,
        },
        ToolPermissionPreset::Confirm => ToolPermissionsConfig {
            preset,
            library_read: ToolPermissionPolicy::Auto,
            library_create: ToolPermissionPolicy::Ask,
            library_edit_markdown: ToolPermissionPolicy::Ask,
            library_move: ToolPermissionPolicy::Ask,
            library_delete: ToolPermissionPolicy::Ask,
            workspace_create: ToolPermissionPolicy::Ask,
            workspace_move: ToolPermissionPolicy::Ask,
            workspace_delete: ToolPermissionPolicy::Ask,
            memory_write: ToolPermissionPolicy::Ask,
            settings_change: ToolPermissionPolicy::Ask,
            agent_launch: ToolPermissionPolicy::Ask,
            context_switch: ToolPermissionPolicy::Ask,
        },
        ToolPermissionPreset::LockedDown => ToolPermissionsConfig {
            preset,
            library_read: ToolPermissionPolicy::Auto,
            library_create: ToolPermissionPolicy::Deny,
            library_edit_markdown: ToolPermissionPolicy::Deny,
            library_move: ToolPermissionPolicy::Deny,
            library_delete: ToolPermissionPolicy::Deny,
            workspace_create: ToolPermissionPolicy::Deny,
            workspace_move: ToolPermissionPolicy::Deny,
            workspace_delete: ToolPermissionPolicy::Deny,
            memory_write: ToolPermissionPolicy::Ask,
            settings_change: ToolPermissionPolicy::Ask,
            agent_launch: ToolPermissionPolicy::Deny,
            context_switch: ToolPermissionPolicy::Deny,
        },
        ToolPermissionPreset::Custom => {
            let mut custom = permissions.clone();
            custom.preset = ToolPermissionPreset::Custom;
            custom
        }
    };
}

fn preset_label(preset: ToolPermissionPreset) -> &'static str {
    match preset {
        ToolPermissionPreset::Balanced => "balanced",
        ToolPermissionPreset::Autopilot => "autopilot",
        ToolPermissionPreset::Confirm => "confirm",
        ToolPermissionPreset::LockedDown => "locked_down",
        ToolPermissionPreset::Custom => "custom",
    }
}

fn policy_label(policy: ToolPermissionPolicy) -> &'static str {
    match policy {
        ToolPermissionPolicy::Auto => "auto",
        ToolPermissionPolicy::Ask => "ask",
        ToolPermissionPolicy::Deny => "deny",
    }
}

async fn execute_agent_slash_command(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            agent_slash_help(),
            serde_json::json!({ "command": "agent" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            agent_slash_help(),
            serde_json::json!({ "tool": "agent", "command": command }),
        ),
        "list" => {
            let limit = args
                .get(1)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid limit: {error}"))?
                .unwrap_or(10)
                .clamp(1, 50);
            let jobs = state.db.list_jobs().await?;
            let mut reply = format!(
                "Agent jobs: showing {} of {}.",
                jobs.len().min(limit),
                jobs.len()
            );
            for job in jobs.iter().take(limit) {
                reply.push_str(&format!("\n{}", format_job_summary(job)));
            }
            agent_slash_reply(
                &reply,
                &command,
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "jobs": jobs.into_iter().take(limit).collect::<Vec<_>>(),
                }),
            )
        }
        "status" => {
            let job_id = slash_job_id_arg(args, "/agent status <job-id>")?;
            let job = state.db.get_job(job_id).await?;
            agent_slash_reply(
                &format_job_summary(&job),
                &command,
                serde_json::json!({ "tool": "agent", "command": command, "job": job }),
            )
        }
        "events" => {
            let job_id = slash_job_id_arg(args, "/agent events <job-id>")?;
            let events = state.db.list_job_events(job_id).await?;
            let mut reply = format!("Job events: {} event(s).", events.len());
            for event in events.iter().take(30) {
                reply.push_str(&format!(
                    "\n{} {}: {}",
                    event.created_at.format("%Y-%m-%d %H:%M:%S"),
                    event.kind,
                    event.payload
                ));
            }
            agent_slash_reply(
                &reply,
                &command,
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "job_id": job_id,
                    "events": events,
                }),
            )
        }
        "preflight" => {
            let job_id = slash_job_id_arg(args, "/agent preflight <job-id>")?;
            let report = worker::preflight_job(config.clone(), state.db.clone(), job_id).await?;
            agent_slash_reply(
                &format!(
                    "Preflight for job {job_id}:\n\n{}",
                    serde_json::to_string_pretty(&report)?
                ),
                &command,
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "job_id": job_id,
                    "report": report,
                }),
            )
        }
        "review-packet" | "review" => {
            let job_id = slash_job_id_arg(args, "/agent review-packet <job-id>")?;
            let packet =
                job_review::build_job_review_packet(&state.db, job_id, false, None).await?;
            let summary = packet
                .get("summary")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let next_step = summary
                .get("next_step")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("inspect_packet");
            let project = packet
                .get("project")
                .and_then(|value| value.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("project");
            agent_slash_reply(
                &format!("Review packet for job {job_id} is ready. Next step: {next_step}."),
                &command,
                serde_json::json!({
                    "type": "job_review",
                    "tool": "agent",
                    "command": command,
                    "status": "ready",
                    "job_id": job_id,
                    "project": project,
                    "packet": packet,
                }),
            )
        }
        "launch" | "queue" => {
            ensure_tool_permission(
                &state.db,
                config,
                "agent.launch",
                config.tool_permissions.agent_launch,
            )
            .await?;
            let request = parse_agent_launch_args(&args[1..])?;
            if !request.confirmed {
                return Ok(agent_slash_reply(
                    "Agent launch requires explicit confirmation. Use: /agent launch <project> <goal> --yes",
                    &command,
                    serde_json::json!({
                        "tool": "agent",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                    }),
                ));
            }
            let project = state.db.get_project_by_name_or_id(&request.project).await?;
            let network_mode = router::default_network_mode_for_provider(
                &request.provider,
                request.allow_network,
                request.secret_grant_token.is_some(),
            );
            let mount_mode = if request.read_only {
                MountMode::ReadOnly
            } else {
                MountMode::ReadWrite
            };
            agent_policy::ensure_agent_job_allowed(
                &project,
                mount_mode,
                JobCreationSource::ExplicitUserAction,
            )?;
            let job = state
                .db
                .create_job(
                    project.id,
                    request.provider,
                    &request.goal,
                    mount_mode,
                    network_mode,
                    request.secret_grant_token.as_deref(),
                )
                .await?;
            let context_pack = memory::retrieve_context_with_config(
                &state.db,
                Some(config),
                memory::RetrievalRequest {
                    query: request.goal.clone(),
                    project_id: Some(project.id),
                    activity_id: None,
                    limit: config.chat.memory_hit_limit,
                },
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
            state
                .db
                .add_job_event(
                    job.id,
                    "queued_from_chat",
                    serde_json::json!({
                        "source": "slash-command",
                        "project": project.name,
                    }),
                )
                .await?;
            agent_slash_reply(
                &format!(
                    "Queued background agent job.\n{}\n\nRun `librarian worker --once` or keep a worker running to execute it.",
                    format_job_summary(&job)
                ),
                &command,
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "job": job,
                    "project": project.name,
                    "goal": request.goal,
                }),
            )
        }
        "cancel" => {
            ensure_tool_permission(
                &state.db,
                config,
                "agent.cancel",
                config.tool_permissions.agent_launch,
            )
            .await?;
            let job_id = slash_job_id_arg(args, "/agent cancel <job-id> --yes")?;
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(agent_slash_reply(
                    "Cancel changes job state. Use: /agent cancel <job-id> --yes",
                    &command,
                    serde_json::json!({
                        "tool": "agent",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                        "job_id": job_id,
                    }),
                ));
            }
            state.db.request_cancel_job(job_id).await?;
            agent_slash_reply(
                &format!("Cancel requested for job {job_id}."),
                &command,
                serde_json::json!({ "tool": "agent", "command": command, "job_id": job_id }),
            )
        }
        "retry" => {
            ensure_tool_permission(
                &state.db,
                config,
                "agent.retry",
                config.tool_permissions.agent_launch,
            )
            .await?;
            let job_id = slash_job_id_arg(args, "/agent retry <job-id> --yes")?;
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(agent_slash_reply(
                    "Retry creates a new queued job. Use: /agent retry <job-id> --yes",
                    &command,
                    serde_json::json!({
                        "tool": "agent",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                        "job_id": job_id,
                    }),
                ));
            }
            let retry = state.db.retry_job(job_id).await?;
            agent_slash_reply(
                &format!("Queued retry job.\n{}", format_job_summary(&retry)),
                &command,
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "source_job_id": job_id,
                    "job": retry,
                }),
            )
        }
        _ => slash_reply(
            "Unknown agent command. Try /agent help.",
            serde_json::json!({ "tool": "agent", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

struct AgentLaunchSlashRequest {
    project: String,
    goal: String,
    provider: crate::domain::ProviderKind,
    secret_grant_token: Option<String>,
    allow_network: bool,
    read_only: bool,
    confirmed: bool,
}

fn parse_agent_launch_args(args: &[String]) -> Result<AgentLaunchSlashRequest> {
    let project = args
        .first()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Usage: /agent launch <project> <goal> [--provider codex] [--read-only] [--allow-network] [--secret-grant-token token] --yes"))?
        .clone();
    let mut provider = crate::domain::ProviderKind::Codex;
    let mut secret_grant_token = None;
    let mut allow_network = false;
    let mut read_only = false;
    let mut confirmed = false;
    let mut goal_parts = Vec::new();
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--provider" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?;
                provider = router::parse_provider_kind(value)?;
            }
            "--secret-grant-token" | "--secret" => {
                index += 1;
                secret_grant_token = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow::anyhow!("--secret-grant-token requires a value"))?
                        .clone(),
                );
            }
            "--allow-network" | "--network" => allow_network = true,
            "--read-only" => read_only = true,
            "--yes" | "--approve" => confirmed = true,
            value if value.starts_with("--") => {
                anyhow::bail!("Unknown /agent launch flag `{value}`")
            }
            value => goal_parts.push(value.to_string()),
        }
        index += 1;
    }

    let goal = goal_parts.join(" ").trim().to_string();
    if goal.is_empty() {
        anyhow::bail!("Usage: /agent launch <project> <goal> [--provider codex] [--read-only] [--allow-network] [--secret-grant-token token] --yes");
    }

    Ok(AgentLaunchSlashRequest {
        project,
        goal,
        provider,
        secret_grant_token,
        allow_network,
        read_only,
        confirmed,
    })
}

fn slash_job_id_arg(args: &[String], usage: &str) -> Result<Uuid> {
    args.get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))?
        .parse::<Uuid>()
        .map_err(|error| anyhow::anyhow!("Invalid job id: {error}"))
}

fn format_job_summary(job: &crate::domain::Job) -> String {
    format!(
        "{} {:?} {:?} provider={} project={} goal={}",
        job.id,
        job.status,
        job.mount_mode,
        router::provider_name(&job.provider),
        job.project_id,
        job.goal
    )
}

fn agent_slash_help() -> &'static str {
    "Agent commands live under /agent and only run when called explicitly:\n/agent list [limit]\n/agent status <job-id>\n/agent events <job-id>\n/agent preflight <job-id>\n/agent review-packet <job-id>\n/agent launch <project> <goal> [--provider codex|openrouter|claude-code] [--read-only] [--allow-network] [--secret-grant-token token] --yes\n/agent cancel <job-id> --yes\n/agent retry <job-id> --yes\n\nUse /agent launch for background work. Normal chat never creates jobs."
}

fn parse_memory_kind_token(value: &str) -> Result<MemoryKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fact" => Ok(MemoryKind::Fact),
        "decision" => Ok(MemoryKind::Decision),
        "instruction" => Ok(MemoryKind::Instruction),
        "status" => Ok(MemoryKind::Status),
        "summary" => Ok(MemoryKind::Summary),
        "observation" | "run-observation" | "run_observation" => Ok(MemoryKind::RunObservation),
        _ => anyhow::bail!(
            "Memory kind must be fact, decision, instruction, status, summary, or observation"
        ),
    }
}

async fn ensure_tool_permission(
    db: &Database,
    config: &Config,
    action: &str,
    policy: ToolPermissionPolicy,
) -> Result<()> {
    let decision = match policy {
        ToolPermissionPolicy::Auto => "allowed_auto",
        ToolPermissionPolicy::Ask => "allowed_user_slash",
        ToolPermissionPolicy::Deny => "denied",
    };
    db.add_system_event(
        "tool_permission",
        serde_json::json!({
            "action": action,
            "policy": policy,
            "decision": decision,
            "source": "slash-command",
            "config_path": config.config_path,
        }),
    )
    .await?;
    if policy == ToolPermissionPolicy::Deny {
        anyhow::bail!("Tool action `{action}` is denied by tool permissions");
    }
    Ok(())
}

fn slash_single_path_arg<'a>(args: &'a [String], usage: &str) -> Result<&'a str> {
    args.get(1)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))
}

fn parse_line_number(value: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .map_err(|error| anyhow::anyhow!("Invalid line number `{value}`: {error}"))
}

fn slash_line_edit(
    config: &Config,
    args: &[String],
    replacement: Option<&str>,
    usage: &str,
) -> Result<library_tools::MarkdownEdit> {
    if args.len() < 4 {
        anyhow::bail!("Usage: {usage}");
    }
    let path = &args[1];
    let start = parse_line_number(&args[2])?;
    let end = parse_line_number(&args[3])?;
    match replacement {
        Some(replacement) => {
            library_tools::replace_markdown_lines(config, path, start, end, replacement)
        }
        None => library_tools::cut_markdown_lines(config, path, start, end),
    }
}

async fn log_slash_library_event(
    db: &Database,
    action: &str,
    payload: serde_json::Value,
) -> Result<()> {
    db.add_system_event(
        "library_tool",
        serde_json::json!({
            "action": action,
            "source": "slash-command",
            "payload": payload,
        }),
    )
    .await?;
    Ok(())
}

async fn log_workspace_event(
    db: &Database,
    action: &str,
    payload: serde_json::Value,
) -> Result<()> {
    db.add_system_event(
        "workspace_tool",
        serde_json::json!({
            "action": action,
            "source": "slash-command",
            "payload": payload,
        }),
    )
    .await?;
    Ok(())
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
            "secret_grant_token": input.secret_grant_token.clone(),
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

#[cfg(test)]
#[path = "admin/tests.rs"]
mod tests;
