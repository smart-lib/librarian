use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum ProviderKind {
    Codex,
    OpenRouter,
    ClaudeCode,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum JobStatus {
    Queued,
    Preparing,
    Running,
    HeartbeatMissed,
    Recovering,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum ScheduleStatus {
    Enabled,
    Disabled,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum ToolApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Executed,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum ScheduleKind {
    System,
    Reminder,
    AgentTask,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum MountMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum NetworkMode {
    None,
    Provider,
    Open,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum AutonomyMode {
    ProjectFull,
    ProjectGuarded,
    ReadOnlyReview,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GitPolicy {
    pub allow_commit: bool,
    pub allow_push: bool,
    pub protected_branches: Vec<String>,
    pub require_branch_pattern: Option<String>,
}

impl Default for GitPolicy {
    fn default() -> Self {
        Self {
            allow_commit: true,
            allow_push: true,
            protected_branches: vec!["main".to_string(), "master".to_string()],
            require_branch_pattern: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub library_path: Option<PathBuf>,
    pub path: PathBuf,
    pub autonomy_mode: AutonomyMode,
    pub git_policy: GitPolicy,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub project_id: Uuid,
    pub provider: ProviderKind,
    pub status: JobStatus,
    pub goal: String,
    pub mount_mode: MountMode,
    pub network_mode: NetworkMode,
    pub secret_grant_token: Option<String>,
    pub cancel_requested_at: Option<DateTime<Utc>>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JobEvent {
    pub id: Uuid,
    pub job_id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SystemEvent {
    pub id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolApproval {
    pub id: Uuid,
    pub tool: String,
    pub action: String,
    pub payload: serde_json::Value,
    pub status: ToolApprovalStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PromptBlock {
    pub id: Uuid,
    pub target: String,
    pub name: String,
    pub content: String,
    pub enabled: bool,
    pub position: i64,
    pub markdown: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatSession {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatTurn {
    pub id: Uuid,
    pub session_id: Uuid,
    pub turn_index: i64,
    pub role: String,
    pub content: String,
    pub memory_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SecretRecord {
    pub id: Uuid,
    pub name: String,
    pub provider: String,
    pub kind: String,
    pub ciphertext: Vec<u8>,
    pub encryption: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SecretGrant {
    pub id: Uuid,
    pub secret_id: Uuid,
    pub job_id: Option<Uuid>,
    pub provider: Option<String>,
    pub capability: String,
    pub expires_at: DateTime<Utc>,
    pub max_uses: i64,
    pub uses: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SecretAuditEvent {
    pub id: Uuid,
    pub secret_id: Uuid,
    pub grant_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub action: String,
    pub success: bool,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderState {
    pub id: Uuid,
    pub provider: String,
    pub model: Option<String>,
    pub status: String,
    pub paused_until: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageObservation {
    pub id: Uuid,
    pub provider: String,
    pub model: Option<String>,
    pub job_id: Option<Uuid>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub limit_event: bool,
    pub metadata: serde_json::Value,
    pub observed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Schedule {
    pub id: Uuid,
    pub name: String,
    pub kind: ScheduleKind,
    pub status: ScheduleStatus,
    pub interval_seconds: i64,
    pub next_run_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum MemoryKind {
    UserMessage,
    AssistantMessage,
    Decision,
    Instruction,
    Fact,
    Status,
    Summary,
    RunObservation,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MemoryItem {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub activity_id: Option<Uuid>,
    pub kind: MemoryKind,
    pub topic: Option<String>,
    pub content: String,
    pub source: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub confidence: f64,
    pub salience: f64,
    pub supersedes_id: Option<Uuid>,
    pub contradicts_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MemoryEmbedding {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub model: String,
    pub dimensions: i64,
    pub vector: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MemoryHit {
    pub item: MemoryItem,
    pub score: f64,
    pub semantic_score: f64,
    pub lexical_score: f64,
    pub recency_score: f64,
    pub scope_score: f64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContextPack {
    pub query: String,
    pub project_id: Option<Uuid>,
    pub activity_id: Option<Uuid>,
    pub generated_at: DateTime<Utc>,
    pub hits: Vec<MemoryHit>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AgentRunSpec {
    pub job_id: Uuid,
    pub project_path: PathBuf,
    pub provider: ProviderKind,
    pub goal: String,
    pub prompt: String,
    pub mount_mode: MountMode,
    pub network_mode: NetworkMode,
    pub secret_grant_token: Option<String>,
}
