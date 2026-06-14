use serde::Deserialize;

use crate::library_tools::LibraryRoot;

#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    pub project: String,
    pub goal: String,
    pub provider: Option<String>,
    pub secret_grant_token: Option<String>,
    pub allow_network: Option<bool>,
    pub read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct LibrarianChatRequest {
    pub message: String,
    pub project: Option<String>,
    #[serde(default)]
    pub project_context: Option<Vec<String>>,
    #[serde(default)]
    pub project_context_scope: Option<String>,
    pub session_id: Option<uuid::Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ChatSessionsQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub library_path: Option<String>,
    pub workspace_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AttachLibraryRequest {
    pub library_path: String,
}

#[derive(Debug, Deserialize)]
pub struct AttachWorkspaceRequest {
    pub workspace_path: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateToolPermissionsRequest {
    pub preset: Option<String>,
    pub library_read: Option<String>,
    pub library_create: Option<String>,
    pub library_edit_markdown: Option<String>,
    pub library_move: Option<String>,
    pub library_delete: Option<String>,
    pub workspace_create: Option<String>,
    pub workspace_move: Option<String>,
    pub workspace_delete: Option<String>,
    pub memory_write: Option<String>,
    pub settings_change: Option<String>,
    pub agent_launch: Option<String>,
    pub context_switch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PromptBlocksQuery {
    pub target: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePromptBlockRequest {
    pub target: String,
    pub name: String,
    pub content: String,
    pub markdown: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePromptBlockRequest {
    pub name: Option<String>,
    pub content: Option<String>,
    pub enabled: Option<bool>,
    pub position: Option<i64>,
    pub markdown: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ExportPromptRequest {
    pub target: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct JobGitActionProposalRequest {
    pub action: String,
    pub message: Option<String>,
    pub commit: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LibraryTreeQuery {
    pub root: Option<LibraryRoot>,
    pub max_depth: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct LibraryPathRequest {
    pub root: LibraryRoot,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct LibraryMoveRequest {
    pub root: LibraryRoot,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct LibraryDeleteRequest {
    pub root: LibraryRoot,
    pub path: String,
    pub recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct LibraryMarkdownRequest {
    pub path: String,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    pub kind: String,
    pub every_seconds: i64,
    pub project: Option<String>,
    pub goal: Option<String>,
    pub provider: Option<String>,
    pub secret_grant_token: Option<String>,
    pub message: Option<String>,
    pub allow_network: Option<bool>,
    pub read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkerRequest {
    pub max_concurrent_jobs: usize,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChatSettingsRequest {
    pub assistant_name: Option<String>,
    pub codex_timeout_seconds: Option<u64>,
    pub memory_hit_limit: Option<usize>,
    pub max_iterations: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCodexRuntimeRequest {
    pub host_home: Option<String>,
    pub mount_host_home: Option<bool>,
    pub mount_read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateClaudeRuntimeRequest {
    pub host_home: Option<String>,
    pub mount_host_home: Option<bool>,
    pub mount_read_only: Option<bool>,
    pub instruction_file: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoutingRequest {
    pub fallback_enabled: bool,
    pub fallback_order: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBudgetRequest {
    pub enabled: bool,
    pub daily_total_usd: Option<f64>,
    pub daily_provider_usd: Option<f64>,
    pub daily_project_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSecretRequest {
    pub name: String,
    pub provider: String,
    pub kind: Option<String>,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSecretGrantRequest {
    pub secret: String,
    pub provider: Option<String>,
    pub capability: Option<String>,
    pub ttl_seconds: Option<i64>,
    pub max_uses: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderControlRequest {
    pub provider: String,
    pub model: Option<String>,
    pub seconds: Option<i64>,
    pub reason: Option<String>,
}
