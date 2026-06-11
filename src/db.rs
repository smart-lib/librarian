use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use uuid::Uuid;

use crate::{
    config::Config,
    domain::{
        AutonomyMode, ChatSession, ChatTurn, GitPolicy, Job, JobEvent, JobStatus, MemoryEmbedding,
        MemoryItem, MemoryKind, MountMode, NetworkMode, Project, PromptBlock, ProviderKind,
        ProviderState, Schedule, ScheduleKind, ScheduleStatus, SecretAuditEvent, SecretGrant,
        SecretRecord, SystemEvent, ToolApproval, ToolApprovalStatus, UsageObservation,
    },
};

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn connect(config: &Config) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(&config.database_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                library_path TEXT,
                path TEXT NOT NULL UNIQUE,
                autonomy_mode TEXT NOT NULL DEFAULT 'ProjectFull',
                git_policy TEXT NOT NULL DEFAULT '{"allow_commit":true,"allow_push":true,"protected_branches":["main","master"],"require_branch_pattern":null}',
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                status TEXT NOT NULL,
                goal TEXT NOT NULL,
                mount_mode TEXT NOT NULL,
                network_mode TEXT NOT NULL,
                secret_grant_token TEXT,
                cancel_requested_at TEXT,
                last_heartbeat_at TEXT,
                started_at TEXT,
                finished_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(project_id) REFERENCES projects(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS job_events (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(job_id) REFERENCES jobs(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS system_events (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tool_approvals (
                id TEXT PRIMARY KEY,
                tool TEXT NOT NULL,
                action TEXT NOT NULL,
                payload TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS prompt_blocks (
                id TEXT PRIMARY KEY,
                target TEXT NOT NULL,
                name TEXT NOT NULL,
                content TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                position INTEGER NOT NULL,
                markdown INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS chat_sessions (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(project_id) REFERENCES projects(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS chat_turns (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                turn_index INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                memory_id TEXT,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES chat_sessions(id),
                FOREIGN KEY(memory_id) REFERENCES memory_items(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS secret_records (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                provider TEXT NOT NULL,
                kind TEXT NOT NULL,
                ciphertext BLOB NOT NULL,
                encryption TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS secret_grants (
                id TEXT PRIMARY KEY,
                secret_id TEXT NOT NULL,
                job_id TEXT,
                provider TEXT,
                capability TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                max_uses INTEGER NOT NULL,
                uses INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                FOREIGN KEY(secret_id) REFERENCES secret_records(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS secret_audit_events (
                id TEXT PRIMARY KEY,
                secret_id TEXT NOT NULL,
                grant_id TEXT,
                job_id TEXT,
                action TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                FOREIGN KEY(secret_id) REFERENCES secret_records(id),
                FOREIGN KEY(grant_id) REFERENCES secret_grants(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS provider_states (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                model TEXT,
                status TEXT NOT NULL,
                paused_until TEXT,
                reason TEXT,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS usage_observations (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                model TEXT,
                job_id TEXT,
                input_tokens INTEGER,
                output_tokens INTEGER,
                cost_usd REAL,
                limit_event INTEGER NOT NULL DEFAULT 0,
                metadata TEXT NOT NULL DEFAULT '{}',
                observed_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schedules (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                interval_seconds INTEGER NOT NULL,
                next_run_at TEXT NOT NULL,
                last_run_at TEXT,
                payload TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memory_items (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                activity_id TEXT,
                kind TEXT NOT NULL,
                topic TEXT,
                content TEXT NOT NULL,
                source TEXT,
                source_uri TEXT,
                observed_at TEXT NOT NULL,
                valid_from TEXT,
                valid_until TEXT,
                confidence REAL NOT NULL DEFAULT 1.0,
                salience REAL NOT NULL DEFAULT 1.0,
                supersedes_id TEXT,
                contradicts_id TEXT,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(project_id) REFERENCES projects(id),
                FOREIGN KEY(supersedes_id) REFERENCES memory_items(id),
                FOREIGN KEY(contradicts_id) REFERENCES memory_items(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memory_embeddings (
                id TEXT PRIMARY KEY,
                memory_id TEXT NOT NULL,
                model TEXT NOT NULL,
                dimensions INTEGER NOT NULL,
                vector BLOB NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(memory_id) REFERENCES memory_items(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                memory_id UNINDEXED,
                content,
                topic,
                tokenize = 'unicode61'
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memory_project_observed ON memory_items(project_id, observed_at DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memory_activity_observed ON memory_items(activity_id, observed_at DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memory_kind_observed ON memory_items(kind, observed_at DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_embedding_model ON memory_embeddings(memory_id, model)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_secret_grants_lookup ON secret_grants(secret_id, job_id, provider, expires_at)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_secret_audit_created ON secret_audit_events(created_at DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_states_scope ON provider_states(provider, COALESCE(model, ''))",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_usage_provider_observed ON usage_observations(provider, observed_at DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_tool_approvals_status_created ON tool_approvals(status, created_at DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_prompt_blocks_target_position ON prompt_blocks(target, position)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_chat_sessions_project_updated ON chat_sessions(project_id, updated_at DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_chat_turns_session_index ON chat_turns(session_id, turn_index)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "ALTER TABLE projects ADD COLUMN autonomy_mode TEXT NOT NULL DEFAULT 'ProjectFull'",
        )
        .execute(&self.pool)
        .await
        .ok();
        sqlx::query("ALTER TABLE projects ADD COLUMN library_path TEXT")
            .execute(&self.pool)
            .await
            .ok();
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_projects_library_path ON projects(library_path) WHERE library_path IS NOT NULL",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"ALTER TABLE projects ADD COLUMN git_policy TEXT NOT NULL DEFAULT '{"allow_commit":true,"allow_push":true,"protected_branches":["main","master"],"require_branch_pattern":null}'"#,
        )
        .execute(&self.pool)
        .await
        .ok();
        for statement in [
            "ALTER TABLE jobs ADD COLUMN cancel_requested_at TEXT",
            "ALTER TABLE jobs ADD COLUMN last_heartbeat_at TEXT",
            "ALTER TABLE jobs ADD COLUMN started_at TEXT",
            "ALTER TABLE jobs ADD COLUMN finished_at TEXT",
            "ALTER TABLE jobs ADD COLUMN secret_grant_token TEXT",
            "ALTER TABLE schedules ADD COLUMN payload TEXT NOT NULL DEFAULT '{}'",
            "ALTER TABLE memory_items ADD COLUMN project_id TEXT",
            "ALTER TABLE memory_items ADD COLUMN activity_id TEXT",
            "ALTER TABLE memory_items ADD COLUMN topic TEXT",
            "ALTER TABLE memory_items ADD COLUMN source_uri TEXT",
            "ALTER TABLE memory_items ADD COLUMN observed_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00+00:00'",
            "ALTER TABLE memory_items ADD COLUMN valid_from TEXT",
            "ALTER TABLE memory_items ADD COLUMN valid_until TEXT",
            "ALTER TABLE memory_items ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0",
            "ALTER TABLE memory_items ADD COLUMN salience REAL NOT NULL DEFAULT 1.0",
            "ALTER TABLE memory_items ADD COLUMN supersedes_id TEXT",
            "ALTER TABLE memory_items ADD COLUMN contradicts_id TEXT",
            "ALTER TABLE memory_items ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}'",
        ] {
            sqlx::query(statement).execute(&self.pool).await.ok();
        }

        self.ensure_default_schedules().await?;
        Ok(())
    }

    async fn ensure_default_schedules(&self) -> Result<()> {
        for (name, interval_seconds, payload) in [
            (
                "system.heartbeat-recovery",
                30_i64,
                serde_json::json!({ "task": "heartbeat_recovery" }),
            ),
            (
                "system.memory-compaction-candidates",
                3600_i64,
                serde_json::json!({ "task": "memory_compaction_candidates", "older_than_days": 14 }),
            ),
            (
                "system.container-cleanup",
                1800_i64,
                serde_json::json!({ "task": "container_cleanup" }),
            ),
        ] {
            let existing = sqlx::query("SELECT id FROM schedules WHERE name = ? LIMIT 1")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
            if existing.is_some() {
                continue;
            }

            let now = Utc::now();
            sqlx::query(
                r#"
                INSERT INTO schedules
                    (id, name, kind, status, interval_seconds, next_run_at, last_run_at, payload, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(name)
            .bind("System")
            .bind("Enabled")
            .bind(interval_seconds)
            .bind((now + Duration::seconds(interval_seconds)).to_rfc3339())
            .bind(Option::<String>::None)
            .bind(payload.to_string())
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn add_project(&self, name: &str, path: &Path) -> Result<Project> {
        let project = Project {
            id: Uuid::new_v4(),
            name: name.to_string(),
            library_path: None,
            path: path.to_path_buf(),
            autonomy_mode: AutonomyMode::ProjectFull,
            git_policy: GitPolicy::default(),
            created_at: Utc::now(),
        };

        sqlx::query("INSERT INTO projects (id, name, library_path, path, autonomy_mode, git_policy, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(project.id.to_string())
            .bind(&project.name)
            .bind(project.library_path.as_ref().map(|path| path.to_string_lossy().to_string()))
            .bind(project.path.to_string_lossy().to_string())
            .bind(format!("{:?}", project.autonomy_mode))
            .bind(serde_json::to_string(&project.git_policy)?)
            .bind(project.created_at.to_rfc3339())
            .execute(&self.pool)
            .await?;

        Ok(project)
    }

    pub async fn create_tool_approval(
        &self,
        tool: &str,
        action: &str,
        payload: serde_json::Value,
    ) -> Result<ToolApproval> {
        let now = Utc::now();
        let approval = ToolApproval {
            id: Uuid::new_v4(),
            tool: tool.to_string(),
            action: action.to_string(),
            payload,
            status: ToolApprovalStatus::Pending,
            created_at: now,
            updated_at: now,
        };
        sqlx::query(
            r#"
            INSERT INTO tool_approvals
                (id, tool, action, payload, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(approval.id.to_string())
        .bind(&approval.tool)
        .bind(&approval.action)
        .bind(approval.payload.to_string())
        .bind(format!("{:?}", approval.status))
        .bind(approval.created_at.to_rfc3339())
        .bind(approval.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(approval)
    }

    pub async fn list_tool_approvals(&self, limit: i64) -> Result<Vec<ToolApproval>> {
        let rows = sqlx::query(
            r#"
            SELECT id, tool, action, payload, status, created_at, updated_at
            FROM tool_approvals
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_tool_approval).collect()
    }

    pub async fn get_tool_approval(&self, id: Uuid) -> Result<ToolApproval> {
        let row = sqlx::query(
            r#"
            SELECT id, tool, action, payload, status, created_at, updated_at
            FROM tool_approvals
            WHERE id = ?
            LIMIT 1
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => row_to_tool_approval(row),
            None => bail!("Tool approval `{id}` was not found"),
        }
    }

    pub async fn update_tool_approval_status(
        &self,
        id: Uuid,
        status: ToolApprovalStatus,
    ) -> Result<ToolApproval> {
        sqlx::query("UPDATE tool_approvals SET status = ?, updated_at = ? WHERE id = ?")
            .bind(format!("{:?}", status))
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        self.get_tool_approval(id).await
    }

    pub async fn create_prompt_block(
        &self,
        target: &str,
        name: &str,
        content: &str,
        markdown: bool,
    ) -> Result<PromptBlock> {
        let now = Utc::now();
        let next_position: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM prompt_blocks WHERE target = ?",
        )
        .bind(target)
        .fetch_one(&self.pool)
        .await?;
        let block = PromptBlock {
            id: Uuid::new_v4(),
            target: target.to_string(),
            name: name.to_string(),
            content: content.to_string(),
            enabled: true,
            position: next_position,
            markdown,
            created_at: now,
            updated_at: now,
        };
        sqlx::query(
            r#"
            INSERT INTO prompt_blocks
                (id, target, name, content, enabled, position, markdown, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(block.id.to_string())
        .bind(&block.target)
        .bind(&block.name)
        .bind(&block.content)
        .bind(block.enabled)
        .bind(block.position)
        .bind(block.markdown)
        .bind(block.created_at.to_rfc3339())
        .bind(block.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(block)
    }

    pub async fn list_prompt_blocks(&self, target: Option<&str>) -> Result<Vec<PromptBlock>> {
        let rows = if let Some(target) = target {
            sqlx::query(
                r#"
                SELECT id, target, name, content, enabled, position, markdown, created_at, updated_at
                FROM prompt_blocks
                WHERE target = ?
                ORDER BY position, created_at
                "#,
            )
            .bind(target)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, target, name, content, enabled, position, markdown, created_at, updated_at
                FROM prompt_blocks
                ORDER BY target, position, created_at
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter().map(row_to_prompt_block).collect()
    }

    pub async fn set_prompt_block_enabled(&self, id: Uuid, enabled: bool) -> Result<PromptBlock> {
        sqlx::query("UPDATE prompt_blocks SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled)
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        self.get_prompt_block(id).await
    }

    pub async fn update_prompt_block(
        &self,
        id: Uuid,
        name: Option<&str>,
        content: Option<&str>,
        enabled: Option<bool>,
        position: Option<i64>,
        markdown: Option<bool>,
    ) -> Result<PromptBlock> {
        let current = self.get_prompt_block(id).await?;
        sqlx::query(
            r#"
            UPDATE prompt_blocks
            SET name = ?, content = ?, enabled = ?, position = ?, markdown = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(name.unwrap_or(&current.name))
        .bind(content.unwrap_or(&current.content))
        .bind(enabled.unwrap_or(current.enabled))
        .bind(position.unwrap_or(current.position))
        .bind(markdown.unwrap_or(current.markdown))
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        self.get_prompt_block(id).await
    }

    pub async fn delete_prompt_block(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM prompt_blocks WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_chat_session(
        &self,
        project_id: Option<Uuid>,
        title: &str,
    ) -> Result<ChatSession> {
        let now = Utc::now();
        let session = ChatSession {
            id: Uuid::new_v4(),
            project_id,
            title: title.trim().chars().take(80).collect::<String>(),
            created_at: now,
            updated_at: now,
        };
        let title = if session.title.is_empty() {
            "New chat".to_string()
        } else {
            session.title.clone()
        };
        sqlx::query(
            r#"
            INSERT INTO chat_sessions (id, project_id, title, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(session.id.to_string())
        .bind(session.project_id.map(|id| id.to_string()))
        .bind(&title)
        .bind(session.created_at.to_rfc3339())
        .bind(session.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(ChatSession { title, ..session })
    }

    pub async fn get_chat_session(&self, id: Uuid) -> Result<ChatSession> {
        let row = sqlx::query(
            r#"
            SELECT id, project_id, title, created_at, updated_at
            FROM chat_sessions
            WHERE id = ?
            LIMIT 1
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => row_to_chat_session(row),
            None => bail!("Chat session `{id}` was not found"),
        }
    }

    pub async fn list_chat_sessions(&self, limit: i64) -> Result<Vec<ChatSession>> {
        let rows = sqlx::query(
            r#"
            SELECT id, project_id, title, created_at, updated_at
            FROM chat_sessions
            ORDER BY updated_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_chat_session).collect()
    }

    pub async fn add_chat_turn(
        &self,
        session_id: Uuid,
        role: &str,
        content: &str,
        memory_id: Option<Uuid>,
        metadata: serde_json::Value,
    ) -> Result<ChatTurn> {
        let now = Utc::now();
        let row = sqlx::query(
            "SELECT COALESCE(MAX(turn_index), 0) + 1 AS next_index FROM chat_turns WHERE session_id = ?",
        )
        .bind(session_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        let turn = ChatTurn {
            id: Uuid::new_v4(),
            session_id,
            turn_index: row.get("next_index"),
            role: role.to_string(),
            content: content.to_string(),
            memory_id,
            metadata,
            created_at: now,
        };
        sqlx::query(
            r#"
            INSERT INTO chat_turns
                (id, session_id, turn_index, role, content, memory_id, metadata, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(turn.id.to_string())
        .bind(turn.session_id.to_string())
        .bind(turn.turn_index)
        .bind(&turn.role)
        .bind(&turn.content)
        .bind(turn.memory_id.map(|id| id.to_string()))
        .bind(turn.metadata.to_string())
        .bind(turn.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        sqlx::query("UPDATE chat_sessions SET updated_at = ? WHERE id = ?")
            .bind(now.to_rfc3339())
            .bind(session_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(turn)
    }

    pub async fn list_chat_turns(&self, session_id: Uuid) -> Result<Vec<ChatTurn>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, turn_index, role, content, memory_id, metadata, created_at
            FROM chat_turns
            WHERE session_id = ?
            ORDER BY turn_index ASC
            "#,
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_chat_turn).collect()
    }

    pub async fn get_prompt_block(&self, id: Uuid) -> Result<PromptBlock> {
        let row = sqlx::query(
            r#"
            SELECT id, target, name, content, enabled, position, markdown, created_at, updated_at
            FROM prompt_blocks
            WHERE id = ?
            LIMIT 1
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => row_to_prompt_block(row),
            None => bail!("Prompt block `{id}` was not found"),
        }
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let rows = sqlx::query("SELECT id, name, library_path, path, autonomy_mode, git_policy, created_at FROM projects ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(row_to_project).collect()
    }

    pub async fn get_project_by_name_or_id(&self, value: &str) -> Result<Project> {
        let row = sqlx::query(
            "SELECT id, name, library_path, path, autonomy_mode, git_policy, created_at FROM projects WHERE id = ? OR name = ? OR library_path = ? LIMIT 1",
        )
        .bind(value)
        .bind(value)
        .bind(value)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => row_to_project(row),
            None => bail!("Project `{value}` was not found"),
        }
    }

    pub async fn get_project_by_id(&self, id: Uuid) -> Result<Project> {
        let row = sqlx::query(
            "SELECT id, name, library_path, path, autonomy_mode, git_policy, created_at FROM projects WHERE id = ? LIMIT 1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => row_to_project(row),
            None => bail!("Project `{id}` was not found"),
        }
    }

    pub async fn attach_project_library_path(
        &self,
        project_id: Uuid,
        library_path: &Path,
    ) -> Result<Project> {
        sqlx::query("UPDATE projects SET library_path = ? WHERE id = ?")
            .bind(library_path.to_string_lossy().to_string())
            .bind(project_id.to_string())
            .execute(&self.pool)
            .await?;
        self.get_project_by_id(project_id).await
    }

    pub async fn detach_project_library_path(&self, project_id: Uuid) -> Result<Project> {
        sqlx::query("UPDATE projects SET library_path = NULL WHERE id = ?")
            .bind(project_id.to_string())
            .execute(&self.pool)
            .await?;
        self.get_project_by_id(project_id).await
    }

    pub async fn update_project_workspace_path(
        &self,
        project_id: Uuid,
        workspace_path: &Path,
    ) -> Result<Project> {
        sqlx::query("UPDATE projects SET path = ? WHERE id = ?")
            .bind(workspace_path.to_string_lossy().to_string())
            .bind(project_id.to_string())
            .execute(&self.pool)
            .await?;
        self.get_project_by_id(project_id).await
    }

    pub async fn create_job(
        &self,
        project_id: Uuid,
        provider: ProviderKind,
        goal: &str,
        mount_mode: MountMode,
        network_mode: NetworkMode,
        secret_grant_token: Option<&str>,
    ) -> Result<Job> {
        let now = Utc::now();
        let job = Job {
            id: Uuid::new_v4(),
            project_id,
            provider,
            status: JobStatus::Queued,
            goal: goal.to_string(),
            mount_mode,
            network_mode,
            secret_grant_token: secret_grant_token.map(ToOwned::to_owned),
            cancel_requested_at: None,
            last_heartbeat_at: None,
            started_at: None,
            finished_at: None,
            created_at: now,
            updated_at: now,
        };

        sqlx::query(
            r#"
            INSERT INTO jobs
                (id, project_id, provider, status, goal, mount_mode, network_mode, secret_grant_token, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(job.id.to_string())
        .bind(job.project_id.to_string())
        .bind(format!("{:?}", job.provider))
        .bind(format!("{:?}", job.status))
        .bind(&job.goal)
        .bind(format!("{:?}", job.mount_mode))
        .bind(format!("{:?}", job.network_mode))
        .bind(&job.secret_grant_token)
        .bind(job.created_at.to_rfc3339())
        .bind(job.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(job)
    }

    pub async fn list_jobs(&self) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            r#"
            SELECT id, project_id, provider, status, goal, mount_mode, network_mode,
                   secret_grant_token, cancel_requested_at, last_heartbeat_at, started_at, finished_at,
                   created_at, updated_at
            FROM jobs
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_job).collect()
    }

    pub async fn running_jobs_missing_heartbeat(&self, cutoff: DateTime<Utc>) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            r#"
            SELECT id, project_id, provider, status, goal, mount_mode, network_mode,
                   secret_grant_token, cancel_requested_at, last_heartbeat_at, started_at, finished_at,
                   created_at, updated_at
            FROM jobs
            WHERE status IN ('Preparing', 'Running')
              AND COALESCE(last_heartbeat_at, updated_at) < ?
            ORDER BY updated_at ASC
            "#,
        )
        .bind(cutoff.to_rfc3339())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_job).collect()
    }

    pub async fn next_queued_job(&self) -> Result<Option<Job>> {
        let row = sqlx::query(
            r#"
            SELECT id, project_id, provider, status, goal, mount_mode, network_mode,
                   secret_grant_token, cancel_requested_at, last_heartbeat_at, started_at, finished_at,
                   created_at, updated_at
            FROM jobs
            WHERE status = 'Queued'
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_job).transpose()
    }

    pub async fn claim_next_queued_job(&self) -> Result<Option<Job>> {
        loop {
            let Some(job) = self.next_queued_job().await? else {
                return Ok(None);
            };
            let now = Utc::now().to_rfc3339();
            let result = sqlx::query(
                "UPDATE jobs SET status = 'Preparing', updated_at = ? WHERE id = ? AND status = 'Queued'",
            )
            .bind(&now)
            .bind(job.id.to_string())
            .execute(&self.pool)
            .await?;

            if result.rows_affected() == 0 {
                continue;
            }

            self.add_job_event(job.id, "claimed", serde_json::json!({ "at": now }))
                .await?;
            return self.get_job(job.id).await.map(Some);
        }
    }

    pub async fn get_job(&self, job_id: Uuid) -> Result<Job> {
        let row = sqlx::query(
            r#"
            SELECT id, project_id, provider, status, goal, mount_mode, network_mode,
                   secret_grant_token, cancel_requested_at, last_heartbeat_at, started_at, finished_at,
                   created_at, updated_at
            FROM jobs
            WHERE id = ?
            LIMIT 1
            "#,
        )
        .bind(job_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => row_to_job(row),
            None => bail!("Job `{job_id}` was not found"),
        }
    }

    pub async fn update_job_status(&self, job_id: Uuid, status: JobStatus) -> Result<()> {
        sqlx::query("UPDATE jobs SET status = ?, updated_at = ? WHERE id = ?")
            .bind(format!("{:?}", status))
            .bind(Utc::now().to_rfc3339())
            .bind(job_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_job_provider(&self, job_id: Uuid, provider: ProviderKind) -> Result<()> {
        sqlx::query("UPDATE jobs SET provider = ?, updated_at = ? WHERE id = ?")
            .bind(format!("{:?}", provider))
            .bind(Utc::now().to_rfc3339())
            .bind(job_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_job_started(&self, job_id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE jobs SET started_at = COALESCE(started_at, ?), last_heartbeat_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(job_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn heartbeat_job(&self, job_id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE jobs SET last_heartbeat_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(job_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_job_finished(&self, job_id: Uuid, status: JobStatus) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE jobs SET status = ?, finished_at = ?, updated_at = ? WHERE id = ?")
            .bind(format!("{:?}", status))
            .bind(&now)
            .bind(&now)
            .bind(job_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn request_cancel_job(&self, job_id: Uuid) -> Result<()> {
        let job = self.get_job(job_id).await?;
        let now = Utc::now().to_rfc3339();
        if matches!(job.status, JobStatus::Queued) {
            sqlx::query(
                "UPDATE jobs SET status = 'Cancelled', cancel_requested_at = ?, finished_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&now)
            .bind(&now)
            .bind(&now)
            .bind(job_id.to_string())
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query("UPDATE jobs SET cancel_requested_at = ?, updated_at = ? WHERE id = ?")
                .bind(&now)
                .bind(&now)
                .bind(job_id.to_string())
                .execute(&self.pool)
                .await?;
        }
        self.add_job_event(job_id, "cancel_requested", serde_json::json!({ "at": now }))
            .await?;
        Ok(())
    }

    pub async fn retry_job(&self, job_id: Uuid) -> Result<Job> {
        let job = self.get_job(job_id).await?;
        let retry = self
            .create_job(
                job.project_id,
                job.provider,
                &job.goal,
                job.mount_mode,
                job.network_mode,
                job.secret_grant_token.as_deref(),
            )
            .await?;
        self.add_job_event(
            retry.id,
            "retry_of",
            serde_json::json!({ "source_job_id": job_id }),
        )
        .await?;
        Ok(retry)
    }

    pub async fn add_job_event(
        &self,
        job_id: Uuid,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<JobEvent> {
        let event = JobEvent {
            id: Uuid::new_v4(),
            job_id,
            kind: kind.to_string(),
            payload,
            created_at: Utc::now(),
        };

        sqlx::query(
            "INSERT INTO job_events (id, job_id, kind, payload, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(event.id.to_string())
        .bind(event.job_id.to_string())
        .bind(&event.kind)
        .bind(serde_json::to_string(&event.payload)?)
        .bind(event.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(event)
    }

    pub async fn add_system_event(
        &self,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<SystemEvent> {
        let event = SystemEvent {
            id: Uuid::new_v4(),
            kind: kind.to_string(),
            payload,
            created_at: Utc::now(),
        };

        sqlx::query(
            "INSERT INTO system_events (id, kind, payload, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(event.id.to_string())
        .bind(&event.kind)
        .bind(event.payload.to_string())
        .bind(event.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(event)
    }

    pub async fn upsert_secret_record(
        &self,
        name: &str,
        provider: &str,
        kind: &str,
        ciphertext: Vec<u8>,
        encryption: &str,
        metadata: serde_json::Value,
    ) -> Result<SecretRecord> {
        let now = Utc::now();
        let existing = sqlx::query("SELECT id, created_at FROM secret_records WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        let id = existing
            .as_ref()
            .map(|row| row.get::<String, _>("id"))
            .map(|id| Uuid::parse_str(&id))
            .transpose()?
            .unwrap_or_else(Uuid::new_v4);
        let created_at = existing
            .and_then(|row| row.get::<Option<String>, _>("created_at"))
            .map(|time| parse_time(&time))
            .transpose()?
            .unwrap_or(now);

        sqlx::query(
            r#"
            INSERT INTO secret_records
                (id, name, provider, kind, ciphertext, encryption, metadata, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET
                provider = excluded.provider,
                kind = excluded.kind,
                ciphertext = excluded.ciphertext,
                encryption = excluded.encryption,
                metadata = excluded.metadata,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(id.to_string())
        .bind(name)
        .bind(provider)
        .bind(kind)
        .bind(&ciphertext)
        .bind(encryption)
        .bind(metadata.to_string())
        .bind(created_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(SecretRecord {
            id,
            name: name.to_string(),
            provider: provider.to_string(),
            kind: kind.to_string(),
            ciphertext,
            encryption: encryption.to_string(),
            metadata,
            created_at,
            updated_at: now,
        })
    }

    pub async fn list_secret_records(&self) -> Result<Vec<SecretRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, provider, kind, ciphertext, encryption, metadata, created_at, updated_at
            FROM secret_records
            ORDER BY provider, name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_secret_record).collect()
    }

    pub async fn get_secret_by_name_or_id(&self, value: &str) -> Result<SecretRecord> {
        let row = sqlx::query(
            r#"
            SELECT id, name, provider, kind, ciphertext, encryption, metadata, created_at, updated_at
            FROM secret_records
            WHERE id = ? OR name = ?
            LIMIT 1
            "#,
        )
        .bind(value)
        .bind(value)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => row_to_secret_record(row),
            None => bail!("Secret `{value}` was not found"),
        }
    }

    pub async fn create_secret_grant(
        &self,
        secret_id: Uuid,
        job_id: Option<Uuid>,
        provider: Option<&str>,
        capability: &str,
        expires_at: DateTime<Utc>,
        max_uses: i64,
    ) -> Result<SecretGrant> {
        let grant = SecretGrant {
            id: Uuid::new_v4(),
            secret_id,
            job_id,
            provider: provider.map(ToOwned::to_owned),
            capability: capability.to_string(),
            expires_at,
            max_uses: max_uses.max(1),
            uses: 0,
            created_at: Utc::now(),
        };
        sqlx::query(
            r#"
            INSERT INTO secret_grants
                (id, secret_id, job_id, provider, capability, expires_at, max_uses, uses, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(grant.id.to_string())
        .bind(grant.secret_id.to_string())
        .bind(grant.job_id.map(|id| id.to_string()))
        .bind(&grant.provider)
        .bind(&grant.capability)
        .bind(grant.expires_at.to_rfc3339())
        .bind(grant.max_uses)
        .bind(grant.uses)
        .bind(grant.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(grant)
    }

    pub async fn get_secret_grant(&self, grant_id: Uuid) -> Result<SecretGrant> {
        let row = sqlx::query(
            r#"
            SELECT id, secret_id, job_id, provider, capability, expires_at, max_uses, uses, created_at
            FROM secret_grants
            WHERE id = ?
            LIMIT 1
            "#,
        )
        .bind(grant_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => row_to_secret_grant(row),
            None => bail!("Secret grant `{grant_id}` was not found"),
        }
    }

    pub async fn consume_secret_grant(&self, grant_id: Uuid) -> Result<SecretGrant> {
        let grant = self.get_secret_grant(grant_id).await?;
        if grant.expires_at <= Utc::now() {
            bail!("Secret grant `{grant_id}` is expired");
        }
        if grant.uses >= grant.max_uses {
            bail!("Secret grant `{grant_id}` has no remaining uses");
        }
        let result = sqlx::query(
            "UPDATE secret_grants SET uses = uses + 1 WHERE id = ? AND uses < max_uses AND expires_at > ?",
        )
        .bind(grant_id.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            bail!("Secret grant `{grant_id}` could not be consumed");
        }
        self.get_secret_grant(grant_id).await
    }

    pub async fn list_secret_grants(&self, limit: i64) -> Result<Vec<SecretGrant>> {
        let rows = sqlx::query(
            r#"
            SELECT id, secret_id, job_id, provider, capability, expires_at, max_uses, uses, created_at
            FROM secret_grants
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_secret_grant).collect()
    }

    pub async fn add_secret_audit_event(
        &self,
        secret_id: Uuid,
        grant_id: Option<Uuid>,
        job_id: Option<Uuid>,
        action: &str,
        success: bool,
        metadata: serde_json::Value,
    ) -> Result<SecretAuditEvent> {
        let event = SecretAuditEvent {
            id: Uuid::new_v4(),
            secret_id,
            grant_id,
            job_id,
            action: action.to_string(),
            success,
            metadata,
            created_at: Utc::now(),
        };
        sqlx::query(
            r#"
            INSERT INTO secret_audit_events
                (id, secret_id, grant_id, job_id, action, success, metadata, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.id.to_string())
        .bind(event.secret_id.to_string())
        .bind(event.grant_id.map(|id| id.to_string()))
        .bind(event.job_id.map(|id| id.to_string()))
        .bind(&event.action)
        .bind(if event.success { 1_i64 } else { 0_i64 })
        .bind(event.metadata.to_string())
        .bind(event.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(event)
    }

    pub async fn list_secret_audit_events(&self, limit: i64) -> Result<Vec<SecretAuditEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT id, secret_id, grant_id, job_id, action, success, metadata, created_at
            FROM secret_audit_events
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_secret_audit_event).collect()
    }

    pub async fn set_provider_pause(
        &self,
        provider: &str,
        model: Option<&str>,
        paused_until: DateTime<Utc>,
        reason: &str,
    ) -> Result<ProviderState> {
        self.upsert_provider_state(
            provider,
            model,
            "Paused",
            Some(paused_until),
            Some(reason),
            serde_json::json!({}),
        )
        .await
    }

    pub async fn resume_provider(
        &self,
        provider: &str,
        model: Option<&str>,
    ) -> Result<ProviderState> {
        self.upsert_provider_state(
            provider,
            model,
            "Available",
            None,
            None,
            serde_json::json!({}),
        )
        .await
    }

    pub async fn upsert_provider_state(
        &self,
        provider: &str,
        model: Option<&str>,
        status: &str,
        paused_until: Option<DateTime<Utc>>,
        reason: Option<&str>,
        metadata: serde_json::Value,
    ) -> Result<ProviderState> {
        let now = Utc::now();
        let existing = sqlx::query(
            "SELECT id, created_at FROM provider_states WHERE provider = ? AND COALESCE(model, '') = COALESCE(?, '') LIMIT 1",
        )
        .bind(provider)
        .bind(model)
        .fetch_optional(&self.pool)
        .await?;
        let id = existing
            .as_ref()
            .map(|row| row.get::<String, _>("id"))
            .map(|id| Uuid::parse_str(&id))
            .transpose()?
            .unwrap_or_else(Uuid::new_v4);
        let created_at = existing
            .and_then(|row| row.get::<Option<String>, _>("created_at"))
            .map(|time| parse_time(&time))
            .transpose()?
            .unwrap_or(now);
        sqlx::query(
            r#"
            INSERT INTO provider_states
                (id, provider, model, status, paused_until, reason, metadata, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(provider, COALESCE(model, '')) DO UPDATE SET
                status = excluded.status,
                paused_until = excluded.paused_until,
                reason = excluded.reason,
                metadata = excluded.metadata,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(id.to_string())
        .bind(provider)
        .bind(model)
        .bind(status)
        .bind(paused_until.map(|time| time.to_rfc3339()))
        .bind(reason)
        .bind(metadata.to_string())
        .bind(created_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(ProviderState {
            id,
            provider: provider.to_string(),
            model: model.map(ToOwned::to_owned),
            status: status.to_string(),
            paused_until,
            reason: reason.map(ToOwned::to_owned),
            metadata,
            created_at,
            updated_at: now,
        })
    }

    pub async fn get_provider_state(
        &self,
        provider: &str,
        model: Option<&str>,
    ) -> Result<Option<ProviderState>> {
        let row = sqlx::query(
            r#"
            SELECT id, provider, model, status, paused_until, reason, metadata, created_at, updated_at
            FROM provider_states
            WHERE provider = ? AND COALESCE(model, '') = COALESCE(?, '')
            LIMIT 1
            "#,
        )
        .bind(provider)
        .bind(model)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_provider_state).transpose()
    }

    pub async fn list_provider_states(&self) -> Result<Vec<ProviderState>> {
        let rows = sqlx::query(
            r#"
            SELECT id, provider, model, status, paused_until, reason, metadata, created_at, updated_at
            FROM provider_states
            ORDER BY provider, model
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_provider_state).collect()
    }

    pub async fn add_usage_observation(
        &self,
        provider: &str,
        model: Option<&str>,
        job_id: Option<Uuid>,
        input_tokens: Option<i64>,
        output_tokens: Option<i64>,
        cost_usd: Option<f64>,
        limit_event: bool,
        metadata: serde_json::Value,
    ) -> Result<UsageObservation> {
        let event = UsageObservation {
            id: Uuid::new_v4(),
            provider: provider.to_string(),
            model: model.map(ToOwned::to_owned),
            job_id,
            input_tokens,
            output_tokens,
            cost_usd,
            limit_event,
            metadata,
            observed_at: Utc::now(),
        };
        sqlx::query(
            r#"
            INSERT INTO usage_observations
                (id, provider, model, job_id, input_tokens, output_tokens, cost_usd, limit_event, metadata, observed_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.id.to_string())
        .bind(&event.provider)
        .bind(&event.model)
        .bind(event.job_id.map(|id| id.to_string()))
        .bind(event.input_tokens)
        .bind(event.output_tokens)
        .bind(event.cost_usd)
        .bind(if event.limit_event { 1_i64 } else { 0_i64 })
        .bind(event.metadata.to_string())
        .bind(event.observed_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(event)
    }

    pub async fn list_usage_observations(&self, limit: i64) -> Result<Vec<UsageObservation>> {
        let rows = sqlx::query(
            r#"
            SELECT id, provider, model, job_id, input_tokens, output_tokens, cost_usd, limit_event, metadata, observed_at
            FROM usage_observations
            ORDER BY observed_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_usage_observation).collect()
    }

    pub async fn usage_cost_since(
        &self,
        since: DateTime<Utc>,
        provider: Option<&str>,
        project_id: Option<Uuid>,
    ) -> Result<f64> {
        let row = match (provider, project_id) {
            (Some(provider), Some(project_id)) => {
                sqlx::query(
                    r#"
                    SELECT COALESCE(SUM(u.cost_usd), 0.0) AS total
                    FROM usage_observations u
                    LEFT JOIN jobs j ON j.id = u.job_id
                    WHERE u.observed_at >= ?
                      AND u.provider = ?
                      AND j.project_id = ?
                    "#,
                )
                .bind(since.to_rfc3339())
                .bind(provider)
                .bind(project_id.to_string())
                .fetch_one(&self.pool)
                .await?
            }
            (Some(provider), None) => {
                sqlx::query(
                    r#"
                    SELECT COALESCE(SUM(cost_usd), 0.0) AS total
                    FROM usage_observations
                    WHERE observed_at >= ?
                      AND provider = ?
                    "#,
                )
                .bind(since.to_rfc3339())
                .bind(provider)
                .fetch_one(&self.pool)
                .await?
            }
            (None, Some(project_id)) => {
                sqlx::query(
                    r#"
                    SELECT COALESCE(SUM(u.cost_usd), 0.0) AS total
                    FROM usage_observations u
                    LEFT JOIN jobs j ON j.id = u.job_id
                    WHERE u.observed_at >= ?
                      AND j.project_id = ?
                    "#,
                )
                .bind(since.to_rfc3339())
                .bind(project_id.to_string())
                .fetch_one(&self.pool)
                .await?
            }
            (None, None) => {
                sqlx::query(
                    r#"
                    SELECT COALESCE(SUM(cost_usd), 0.0) AS total
                    FROM usage_observations
                    WHERE observed_at >= ?
                    "#,
                )
                .bind(since.to_rfc3339())
                .fetch_one(&self.pool)
                .await?
            }
        };
        Ok(row.get::<f64, _>("total"))
    }

    pub async fn list_system_events(&self, limit: i64) -> Result<Vec<SystemEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT id, kind, payload, created_at
            FROM system_events
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_system_event).collect()
    }

    pub async fn count_memory_compaction_candidates(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS count
            FROM memory_items
            WHERE supersedes_id IS NULL
              AND observed_at < ?
              AND kind IN ('UserMessage', 'AssistantMessage', 'Status', 'RunObservation')
            "#,
        )
        .bind(older_than.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("count"))
    }

    pub async fn list_schedules(&self) -> Result<Vec<Schedule>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, kind, status, interval_seconds, next_run_at, last_run_at,
                   payload, created_at, updated_at
            FROM schedules
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_schedule).collect()
    }

    pub async fn due_schedules(&self) -> Result<Vec<Schedule>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, kind, status, interval_seconds, next_run_at, last_run_at,
                   payload, created_at, updated_at
            FROM schedules
            WHERE status = 'Enabled' AND next_run_at <= ?
            ORDER BY next_run_at ASC
            "#,
        )
        .bind(Utc::now().to_rfc3339())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_schedule).collect()
    }

    pub async fn add_schedule(
        &self,
        name: &str,
        kind: ScheduleKind,
        interval_seconds: i64,
        payload: serde_json::Value,
    ) -> Result<Schedule> {
        let now = Utc::now();
        let schedule = Schedule {
            id: Uuid::new_v4(),
            name: name.to_string(),
            kind,
            status: ScheduleStatus::Enabled,
            interval_seconds,
            next_run_at: now + Duration::seconds(interval_seconds),
            last_run_at: None,
            payload,
            created_at: now,
            updated_at: now,
        };

        sqlx::query(
            r#"
            INSERT INTO schedules
                (id, name, kind, status, interval_seconds, next_run_at, last_run_at, payload, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(schedule.id.to_string())
        .bind(&schedule.name)
        .bind(format!("{:?}", schedule.kind))
        .bind(format!("{:?}", schedule.status))
        .bind(schedule.interval_seconds)
        .bind(schedule.next_run_at.to_rfc3339())
        .bind(Option::<String>::None)
        .bind(schedule.payload.to_string())
        .bind(schedule.created_at.to_rfc3339())
        .bind(schedule.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(schedule)
    }

    pub async fn update_schedule(
        &self,
        schedule_id: Uuid,
        name: &str,
        kind: ScheduleKind,
        interval_seconds: i64,
        payload: serde_json::Value,
    ) -> Result<Schedule> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE schedules
            SET name = ?, kind = ?, interval_seconds = ?, payload = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(name)
        .bind(format!("{:?}", kind))
        .bind(interval_seconds.max(1))
        .bind(payload.to_string())
        .bind(now)
        .bind(schedule_id.to_string())
        .execute(&self.pool)
        .await?;
        self.get_schedule(schedule_id).await
    }

    pub async fn delete_schedule(&self, schedule_id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM schedules WHERE id = ?")
            .bind(schedule_id.to_string())
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            bail!("Schedule `{schedule_id}` was not found");
        }
        Ok(())
    }

    pub async fn mark_schedule_ran(&self, schedule_id: Uuid) -> Result<()> {
        let schedule = self.get_schedule(schedule_id).await?;
        let now = Utc::now();
        let next_run_at = now + Duration::seconds(schedule.interval_seconds);
        sqlx::query(
            "UPDATE schedules SET last_run_at = ?, next_run_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(now.to_rfc3339())
        .bind(next_run_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(schedule_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_schedule_status(
        &self,
        schedule_id: Uuid,
        status: ScheduleStatus,
    ) -> Result<Schedule> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE schedules SET status = ?, updated_at = ? WHERE id = ?")
            .bind(format!("{:?}", status))
            .bind(now)
            .bind(schedule_id.to_string())
            .execute(&self.pool)
            .await?;
        self.get_schedule(schedule_id).await
    }

    pub async fn get_schedule(&self, schedule_id: Uuid) -> Result<Schedule> {
        let row = sqlx::query(
            r#"
            SELECT id, name, kind, status, interval_seconds, next_run_at, last_run_at,
                   payload, created_at, updated_at
            FROM schedules
            WHERE id = ?
            LIMIT 1
            "#,
        )
        .bind(schedule_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => row_to_schedule(row),
            None => bail!("Schedule `{schedule_id}` was not found"),
        }
    }

    pub async fn list_job_events(&self, job_id: Uuid) -> Result<Vec<JobEvent>> {
        let rows = sqlx::query(
            "SELECT id, job_id, kind, payload, created_at FROM job_events WHERE job_id = ? ORDER BY created_at ASC",
        )
        .bind(job_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_job_event).collect()
    }

    pub async fn add_memory_item(
        &self,
        project_id: Option<Uuid>,
        activity_id: Option<Uuid>,
        kind: MemoryKind,
        topic: Option<&str>,
        content: &str,
        source: Option<&str>,
        metadata: serde_json::Value,
    ) -> Result<MemoryItem> {
        let now = Utc::now();
        let item = MemoryItem {
            id: Uuid::new_v4(),
            project_id,
            activity_id,
            kind,
            topic: topic.map(ToOwned::to_owned),
            content: content.to_string(),
            source: source.map(ToOwned::to_owned),
            observed_at: now,
            valid_from: None,
            valid_until: None,
            confidence: 1.0,
            salience: 1.0,
            supersedes_id: None,
            contradicts_id: None,
            metadata,
            created_at: now,
            updated_at: now,
        };

        sqlx::query(
            r#"
            INSERT INTO memory_items
                (id, project_id, activity_id, kind, topic, content, source, observed_at, valid_from,
                 valid_until, confidence, salience, supersedes_id, contradicts_id, metadata,
                 created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(item.id.to_string())
        .bind(item.project_id.map(|id| id.to_string()))
        .bind(item.activity_id.map(|id| id.to_string()))
        .bind(format!("{:?}", item.kind))
        .bind(&item.topic)
        .bind(&item.content)
        .bind(&item.source)
        .bind(item.observed_at.to_rfc3339())
        .bind(item.valid_from.map(|time| time.to_rfc3339()))
        .bind(item.valid_until.map(|time| time.to_rfc3339()))
        .bind(item.confidence)
        .bind(item.salience)
        .bind(item.supersedes_id.map(|id| id.to_string()))
        .bind(item.contradicts_id.map(|id| id.to_string()))
        .bind(serde_json::to_string(&item.metadata)?)
        .bind(item.created_at.to_rfc3339())
        .bind(item.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        sqlx::query("INSERT INTO memory_fts (memory_id, content, topic) VALUES (?, ?, ?)")
            .bind(item.id.to_string())
            .bind(&item.content)
            .bind(&item.topic)
            .execute(&self.pool)
            .await?;

        Ok(item)
    }

    pub async fn add_linked_memory_item(
        &self,
        project_id: Option<Uuid>,
        activity_id: Option<Uuid>,
        kind: MemoryKind,
        topic: Option<&str>,
        content: &str,
        source: Option<&str>,
        metadata: serde_json::Value,
        supersedes_id: Option<Uuid>,
        contradicts_id: Option<Uuid>,
    ) -> Result<MemoryItem> {
        let now = Utc::now();
        let item = MemoryItem {
            id: Uuid::new_v4(),
            project_id,
            activity_id,
            kind,
            topic: topic.map(ToOwned::to_owned),
            content: content.to_string(),
            source: source.map(ToOwned::to_owned),
            observed_at: now,
            valid_from: None,
            valid_until: None,
            confidence: 1.0,
            salience: 1.0,
            supersedes_id,
            contradicts_id,
            metadata,
            created_at: now,
            updated_at: now,
        };

        sqlx::query(
            r#"
            INSERT INTO memory_items
                (id, project_id, activity_id, kind, topic, content, source, observed_at, valid_from,
                 valid_until, confidence, salience, supersedes_id, contradicts_id, metadata,
                 created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(item.id.to_string())
        .bind(item.project_id.map(|id| id.to_string()))
        .bind(item.activity_id.map(|id| id.to_string()))
        .bind(format!("{:?}", item.kind))
        .bind(&item.topic)
        .bind(&item.content)
        .bind(&item.source)
        .bind(item.observed_at.to_rfc3339())
        .bind(item.valid_from.map(|time| time.to_rfc3339()))
        .bind(item.valid_until.map(|time| time.to_rfc3339()))
        .bind(item.confidence)
        .bind(item.salience)
        .bind(item.supersedes_id.map(|id| id.to_string()))
        .bind(item.contradicts_id.map(|id| id.to_string()))
        .bind(serde_json::to_string(&item.metadata)?)
        .bind(item.created_at.to_rfc3339())
        .bind(item.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        sqlx::query("INSERT INTO memory_fts (memory_id, content, topic) VALUES (?, ?, ?)")
            .bind(item.id.to_string())
            .bind(&item.content)
            .bind(&item.topic)
            .execute(&self.pool)
            .await?;

        Ok(item)
    }

    pub async fn get_memory_item(&self, id: Uuid) -> Result<MemoryItem> {
        let row = sqlx::query(
            r#"
            SELECT id, project_id, activity_id, kind, topic, content, source, observed_at,
                   valid_from, valid_until, confidence, salience, supersedes_id,
                   contradicts_id, metadata, created_at, updated_at
            FROM memory_items
            WHERE id = ?
            LIMIT 1
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => row_to_memory_item(row),
            None => bail!("Memory item `{id}` was not found"),
        }
    }

    pub async fn legacy_local_memory_responder_items(&self, limit: i64) -> Result<Vec<MemoryItem>> {
        let rows = sqlx::query(
            r#"
            SELECT id, project_id, activity_id, kind, topic, content, source, observed_at,
                   valid_from, valid_until, confidence, salience, supersedes_id,
                   contradicts_id, metadata, created_at, updated_at
            FROM memory_items
            WHERE kind = 'AssistantMessage'
              AND (
                metadata LIKE '%local-memory-responder%'
                OR content LIKE 'I am here as Librarian, not as a background agent runner.%'
              )
            ORDER BY observed_at ASC
            LIMIT ?
            "#,
        )
        .bind(limit.clamp(1, 10_000))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_memory_item).collect()
    }

    pub async fn count_legacy_local_memory_responder_items(&self) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS count
            FROM memory_items
            WHERE kind = 'AssistantMessage'
              AND (
                metadata LIKE '%local-memory-responder%'
                OR content LIKE 'I am here as Librarian, not as a background agent runner.%'
              )
            "#,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("count"))
    }

    pub async fn delete_memory_items(&self, ids: &[Uuid]) -> Result<usize> {
        let mut deleted = 0usize;
        for id in ids {
            sqlx::query("DELETE FROM memory_embeddings WHERE memory_id = ?")
                .bind(id.to_string())
                .execute(&self.pool)
                .await?;
            sqlx::query("DELETE FROM memory_fts WHERE memory_id = ?")
                .bind(id.to_string())
                .execute(&self.pool)
                .await?;
            let result = sqlx::query("DELETE FROM memory_items WHERE id = ?")
                .bind(id.to_string())
                .execute(&self.pool)
                .await?;
            deleted += result.rows_affected() as usize;
        }
        Ok(deleted)
    }

    pub async fn upsert_memory_embedding(
        &self,
        memory_id: Uuid,
        model: &str,
        dimensions: i64,
        vector: Vec<u8>,
    ) -> Result<MemoryEmbedding> {
        let now = Utc::now();
        let existing = sqlx::query(
            "SELECT id FROM memory_embeddings WHERE memory_id = ? AND model = ? LIMIT 1",
        )
        .bind(memory_id.to_string())
        .bind(model)
        .fetch_optional(&self.pool)
        .await?;
        let id = existing
            .as_ref()
            .map(|row| row.get::<String, _>("id"))
            .map(|id| Uuid::parse_str(&id))
            .transpose()?
            .unwrap_or_else(Uuid::new_v4);

        sqlx::query(
            r#"
            INSERT INTO memory_embeddings (id, memory_id, model, dimensions, vector, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(memory_id, model) DO UPDATE SET
                dimensions = excluded.dimensions,
                vector = excluded.vector,
                created_at = excluded.created_at
            "#,
        )
        .bind(id.to_string())
        .bind(memory_id.to_string())
        .bind(model)
        .bind(dimensions)
        .bind(&vector)
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(MemoryEmbedding {
            id,
            memory_id,
            model: model.to_string(),
            dimensions,
            vector,
            created_at: now,
        })
    }

    pub async fn get_memory_embedding(
        &self,
        memory_id: Uuid,
        model: &str,
    ) -> Result<Option<MemoryEmbedding>> {
        let row = sqlx::query(
            r#"
            SELECT id, memory_id, model, dimensions, vector, created_at
            FROM memory_embeddings
            WHERE memory_id = ? AND model = ?
            LIMIT 1
            "#,
        )
        .bind(memory_id.to_string())
        .bind(model)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_memory_embedding).transpose()
    }

    pub async fn memory_items_missing_embedding(
        &self,
        model: &str,
        limit: i64,
    ) -> Result<Vec<MemoryItem>> {
        let rows = sqlx::query(
            r#"
            SELECT m.id, m.project_id, m.activity_id, m.kind, m.topic, m.content, m.source,
                   m.observed_at, m.valid_from, m.valid_until, m.confidence, m.salience,
                   m.supersedes_id, m.contradicts_id, m.metadata, m.created_at, m.updated_at
            FROM memory_items m
            LEFT JOIN memory_embeddings e ON e.memory_id = m.id AND e.model = ?
            WHERE e.id IS NULL
            ORDER BY m.observed_at DESC
            LIMIT ?
            "#,
        )
        .bind(model)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_memory_item).collect()
    }

    pub async fn count_memory_items(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS count FROM memory_items")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get("count"))
    }

    pub async fn count_memory_embeddings(&self, model: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS count FROM memory_embeddings WHERE model = ?")
            .bind(model)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get("count"))
    }

    pub async fn count_memory_missing_embedding(&self, model: &str) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS count
            FROM memory_items m
            LEFT JOIN memory_embeddings e ON e.memory_id = m.id AND e.model = ?
            WHERE e.id IS NULL
            "#,
        )
        .bind(model)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("count"))
    }

    pub async fn recent_memory_for_project(
        &self,
        project_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<MemoryItem>> {
        let rows = if let Some(project_id) = project_id {
            sqlx::query(
                r#"
                SELECT id, project_id, activity_id, kind, topic, content, source, observed_at,
                       valid_from, valid_until, confidence, salience, supersedes_id,
                       contradicts_id, metadata, created_at, updated_at
                FROM memory_items
                WHERE project_id = ? AND supersedes_id IS NULL
                ORDER BY observed_at DESC
                LIMIT ?
                "#,
            )
            .bind(project_id.to_string())
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, project_id, activity_id, kind, topic, content, source, observed_at,
                       valid_from, valid_until, confidence, salience, supersedes_id,
                       contradicts_id, metadata, created_at, updated_at
                FROM memory_items
                WHERE project_id IS NULL AND supersedes_id IS NULL
                ORDER BY observed_at DESC
                LIMIT ?
                "#,
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter().map(row_to_memory_item).collect()
    }

    pub async fn memory_candidates(
        &self,
        project_id: Option<Uuid>,
        activity_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<MemoryItem>> {
        let rows = match (project_id, activity_id) {
            (Some(project_id), Some(activity_id)) => {
                sqlx::query(
                    r#"
                    SELECT id, project_id, activity_id, kind, topic, content, source, observed_at,
                           valid_from, valid_until, confidence, salience, supersedes_id,
                           contradicts_id, metadata, created_at, updated_at
                    FROM memory_items
                    WHERE supersedes_id IS NULL
                      AND (project_id IS NULL OR project_id = ?)
                      AND (activity_id IS NULL OR activity_id = ?)
                    ORDER BY observed_at DESC
                    LIMIT ?
                    "#,
                )
                .bind(project_id.to_string())
                .bind(activity_id.to_string())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(project_id), None) => {
                sqlx::query(
                    r#"
                    SELECT id, project_id, activity_id, kind, topic, content, source, observed_at,
                           valid_from, valid_until, confidence, salience, supersedes_id,
                           contradicts_id, metadata, created_at, updated_at
                    FROM memory_items
                    WHERE supersedes_id IS NULL
                      AND (project_id IS NULL OR project_id = ?)
                    ORDER BY observed_at DESC
                    LIMIT ?
                    "#,
                )
                .bind(project_id.to_string())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, Some(activity_id)) => {
                sqlx::query(
                    r#"
                    SELECT id, project_id, activity_id, kind, topic, content, source, observed_at,
                           valid_from, valid_until, confidence, salience, supersedes_id,
                           contradicts_id, metadata, created_at, updated_at
                    FROM memory_items
                    WHERE supersedes_id IS NULL
                      AND (activity_id IS NULL OR activity_id = ?)
                    ORDER BY observed_at DESC
                    LIMIT ?
                    "#,
                )
                .bind(activity_id.to_string())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, None) => {
                sqlx::query(
                    r#"
                    SELECT id, project_id, activity_id, kind, topic, content, source, observed_at,
                           valid_from, valid_until, confidence, salience, supersedes_id,
                           contradicts_id, metadata, created_at, updated_at
                    FROM memory_items
                    WHERE supersedes_id IS NULL
                    ORDER BY observed_at DESC
                    LIMIT ?
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(row_to_memory_item).collect()
    }
}

fn row_to_project(row: sqlx::sqlite::SqliteRow) -> Result<Project> {
    Ok(Project {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        name: row.get("name"),
        library_path: row
            .get::<Option<String>, _>("library_path")
            .map(PathBuf::from),
        path: PathBuf::from(row.get::<String, _>("path")),
        autonomy_mode: parse_autonomy_mode(row.get::<String, _>("autonomy_mode").as_str())?,
        git_policy: serde_json::from_str(row.get::<String, _>("git_policy").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
    })
}

fn row_to_job(row: sqlx::sqlite::SqliteRow) -> Result<Job> {
    Ok(Job {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        project_id: Uuid::parse_str(row.get::<String, _>("project_id").as_str())?,
        provider: parse_provider(row.get::<String, _>("provider").as_str())?,
        status: parse_status(row.get::<String, _>("status").as_str())?,
        goal: row.get("goal"),
        mount_mode: parse_mount_mode(row.get::<String, _>("mount_mode").as_str())?,
        network_mode: parse_network_mode(row.get::<String, _>("network_mode").as_str())?,
        secret_grant_token: row.get("secret_grant_token"),
        cancel_requested_at: parse_optional_time(
            row.get::<Option<String>, _>("cancel_requested_at"),
        )?,
        last_heartbeat_at: parse_optional_time(row.get::<Option<String>, _>("last_heartbeat_at"))?,
        started_at: parse_optional_time(row.get::<Option<String>, _>("started_at"))?,
        finished_at: parse_optional_time(row.get::<Option<String>, _>("finished_at"))?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
        updated_at: parse_time(row.get::<String, _>("updated_at").as_str())?,
    })
}

fn row_to_tool_approval(row: sqlx::sqlite::SqliteRow) -> Result<ToolApproval> {
    Ok(ToolApproval {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        tool: row.get("tool"),
        action: row.get("action"),
        payload: serde_json::from_str(row.get::<String, _>("payload").as_str())?,
        status: parse_tool_approval_status(row.get::<String, _>("status").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
        updated_at: parse_time(row.get::<String, _>("updated_at").as_str())?,
    })
}

fn row_to_prompt_block(row: sqlx::sqlite::SqliteRow) -> Result<PromptBlock> {
    Ok(PromptBlock {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        target: row.get("target"),
        name: row.get("name"),
        content: row.get("content"),
        enabled: row.get("enabled"),
        position: row.get("position"),
        markdown: row.get("markdown"),
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
        updated_at: parse_time(row.get::<String, _>("updated_at").as_str())?,
    })
}

fn row_to_chat_session(row: sqlx::sqlite::SqliteRow) -> Result<ChatSession> {
    Ok(ChatSession {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        project_id: parse_optional_uuid(row.get::<Option<String>, _>("project_id"))?,
        title: row.get("title"),
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
        updated_at: parse_time(row.get::<String, _>("updated_at").as_str())?,
    })
}

fn row_to_chat_turn(row: sqlx::sqlite::SqliteRow) -> Result<ChatTurn> {
    Ok(ChatTurn {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        session_id: Uuid::parse_str(row.get::<String, _>("session_id").as_str())?,
        turn_index: row.get("turn_index"),
        role: row.get("role"),
        content: row.get("content"),
        memory_id: parse_optional_uuid(row.get::<Option<String>, _>("memory_id"))?,
        metadata: serde_json::from_str(row.get::<String, _>("metadata").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
    })
}

fn row_to_job_event(row: sqlx::sqlite::SqliteRow) -> Result<JobEvent> {
    Ok(JobEvent {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        job_id: Uuid::parse_str(row.get::<String, _>("job_id").as_str())?,
        kind: row.get("kind"),
        payload: serde_json::from_str(row.get::<String, _>("payload").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
    })
}

fn row_to_system_event(row: sqlx::sqlite::SqliteRow) -> Result<SystemEvent> {
    Ok(SystemEvent {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        kind: row.get("kind"),
        payload: serde_json::from_str(row.get::<String, _>("payload").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
    })
}

fn row_to_secret_record(row: sqlx::sqlite::SqliteRow) -> Result<SecretRecord> {
    Ok(SecretRecord {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        name: row.get("name"),
        provider: row.get("provider"),
        kind: row.get("kind"),
        ciphertext: row.get("ciphertext"),
        encryption: row.get("encryption"),
        metadata: serde_json::from_str(row.get::<String, _>("metadata").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
        updated_at: parse_time(row.get::<String, _>("updated_at").as_str())?,
    })
}

fn row_to_secret_grant(row: sqlx::sqlite::SqliteRow) -> Result<SecretGrant> {
    Ok(SecretGrant {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        secret_id: Uuid::parse_str(row.get::<String, _>("secret_id").as_str())?,
        job_id: parse_optional_uuid(row.get::<Option<String>, _>("job_id"))?,
        provider: row.get("provider"),
        capability: row.get("capability"),
        expires_at: parse_time(row.get::<String, _>("expires_at").as_str())?,
        max_uses: row.get("max_uses"),
        uses: row.get("uses"),
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
    })
}

fn row_to_secret_audit_event(row: sqlx::sqlite::SqliteRow) -> Result<SecretAuditEvent> {
    Ok(SecretAuditEvent {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        secret_id: Uuid::parse_str(row.get::<String, _>("secret_id").as_str())?,
        grant_id: parse_optional_uuid(row.get::<Option<String>, _>("grant_id"))?,
        job_id: parse_optional_uuid(row.get::<Option<String>, _>("job_id"))?,
        action: row.get("action"),
        success: row.get::<i64, _>("success") != 0,
        metadata: serde_json::from_str(row.get::<String, _>("metadata").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
    })
}

fn row_to_provider_state(row: sqlx::sqlite::SqliteRow) -> Result<ProviderState> {
    Ok(ProviderState {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        provider: row.get("provider"),
        model: row.get("model"),
        status: row.get("status"),
        paused_until: parse_optional_time(row.get::<Option<String>, _>("paused_until"))?,
        reason: row.get("reason"),
        metadata: serde_json::from_str(row.get::<String, _>("metadata").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
        updated_at: parse_time(row.get::<String, _>("updated_at").as_str())?,
    })
}

fn row_to_usage_observation(row: sqlx::sqlite::SqliteRow) -> Result<UsageObservation> {
    Ok(UsageObservation {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        provider: row.get("provider"),
        model: row.get("model"),
        job_id: parse_optional_uuid(row.get::<Option<String>, _>("job_id"))?,
        input_tokens: row.get("input_tokens"),
        output_tokens: row.get("output_tokens"),
        cost_usd: row.get("cost_usd"),
        limit_event: row.get::<i64, _>("limit_event") != 0,
        metadata: serde_json::from_str(row.get::<String, _>("metadata").as_str())?,
        observed_at: parse_time(row.get::<String, _>("observed_at").as_str())?,
    })
}

fn row_to_memory_item(row: sqlx::sqlite::SqliteRow) -> Result<MemoryItem> {
    Ok(MemoryItem {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        project_id: parse_optional_uuid(row.get::<Option<String>, _>("project_id"))?,
        activity_id: parse_optional_uuid(row.get::<Option<String>, _>("activity_id"))?,
        kind: parse_memory_kind(row.get::<String, _>("kind").as_str())?,
        topic: row.get("topic"),
        content: row.get("content"),
        source: row.get("source"),
        observed_at: parse_time(row.get::<String, _>("observed_at").as_str())?,
        valid_from: parse_optional_time(row.get::<Option<String>, _>("valid_from"))?,
        valid_until: parse_optional_time(row.get::<Option<String>, _>("valid_until"))?,
        confidence: row.get("confidence"),
        salience: row.get("salience"),
        supersedes_id: parse_optional_uuid(row.get::<Option<String>, _>("supersedes_id"))?,
        contradicts_id: parse_optional_uuid(row.get::<Option<String>, _>("contradicts_id"))?,
        metadata: serde_json::from_str(row.get::<String, _>("metadata").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
        updated_at: parse_time(row.get::<String, _>("updated_at").as_str())?,
    })
}

fn row_to_memory_embedding(row: sqlx::sqlite::SqliteRow) -> Result<MemoryEmbedding> {
    Ok(MemoryEmbedding {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        memory_id: Uuid::parse_str(row.get::<String, _>("memory_id").as_str())?,
        model: row.get("model"),
        dimensions: row.get("dimensions"),
        vector: row.get("vector"),
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
    })
}

fn row_to_schedule(row: sqlx::sqlite::SqliteRow) -> Result<Schedule> {
    Ok(Schedule {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        name: row.get("name"),
        kind: parse_schedule_kind(row.get::<String, _>("kind").as_str())?,
        status: parse_schedule_status(row.get::<String, _>("status").as_str())?,
        interval_seconds: row.get("interval_seconds"),
        next_run_at: parse_time(row.get::<String, _>("next_run_at").as_str())?,
        last_run_at: parse_optional_time(row.get::<Option<String>, _>("last_run_at"))?,
        payload: serde_json::from_str(row.get::<String, _>("payload").as_str())?,
        created_at: parse_time(row.get::<String, _>("created_at").as_str())?,
        updated_at: parse_time(row.get::<String, _>("updated_at").as_str())?,
    })
}

fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn parse_optional_time(value: Option<String>) -> Result<Option<DateTime<Utc>>> {
    value.map(|time| parse_time(&time)).transpose()
}

fn parse_optional_uuid(value: Option<String>) -> Result<Option<Uuid>> {
    value
        .map(|id| Uuid::parse_str(&id).map_err(Into::into))
        .transpose()
}

fn parse_provider(value: &str) -> Result<ProviderKind> {
    match value {
        "Codex" => Ok(ProviderKind::Codex),
        "OpenRouter" => Ok(ProviderKind::OpenRouter),
        "ClaudeCode" => Ok(ProviderKind::ClaudeCode),
        _ => bail!("Unknown provider `{value}`"),
    }
}

fn parse_status(value: &str) -> Result<JobStatus> {
    match value {
        "Queued" => Ok(JobStatus::Queued),
        "Preparing" => Ok(JobStatus::Preparing),
        "Running" => Ok(JobStatus::Running),
        "HeartbeatMissed" => Ok(JobStatus::HeartbeatMissed),
        "Recovering" => Ok(JobStatus::Recovering),
        "Completed" => Ok(JobStatus::Completed),
        "Failed" => Ok(JobStatus::Failed),
        "Cancelled" => Ok(JobStatus::Cancelled),
        _ => bail!("Unknown job status `{value}`"),
    }
}

fn parse_tool_approval_status(value: &str) -> Result<ToolApprovalStatus> {
    match value {
        "Pending" => Ok(ToolApprovalStatus::Pending),
        "Approved" => Ok(ToolApprovalStatus::Approved),
        "Rejected" => Ok(ToolApprovalStatus::Rejected),
        "Executed" => Ok(ToolApprovalStatus::Executed),
        _ => bail!("Unknown tool approval status `{value}`"),
    }
}

fn parse_mount_mode(value: &str) -> Result<MountMode> {
    match value {
        "ReadOnly" => Ok(MountMode::ReadOnly),
        "ReadWrite" => Ok(MountMode::ReadWrite),
        _ => bail!("Unknown mount mode `{value}`"),
    }
}

fn parse_network_mode(value: &str) -> Result<NetworkMode> {
    match value {
        "None" => Ok(NetworkMode::None),
        "Provider" => Ok(NetworkMode::Provider),
        "Open" => Ok(NetworkMode::Open),
        _ => bail!("Unknown network mode `{value}`"),
    }
}

fn parse_autonomy_mode(value: &str) -> Result<AutonomyMode> {
    match value {
        "ProjectFull" => Ok(AutonomyMode::ProjectFull),
        "ProjectGuarded" => Ok(AutonomyMode::ProjectGuarded),
        "ReadOnlyReview" => Ok(AutonomyMode::ReadOnlyReview),
        _ => bail!("Unknown autonomy mode `{value}`"),
    }
}

fn parse_memory_kind(value: &str) -> Result<MemoryKind> {
    match value {
        "UserMessage" => Ok(MemoryKind::UserMessage),
        "AssistantMessage" => Ok(MemoryKind::AssistantMessage),
        "Decision" => Ok(MemoryKind::Decision),
        "Instruction" => Ok(MemoryKind::Instruction),
        "Fact" => Ok(MemoryKind::Fact),
        "Status" => Ok(MemoryKind::Status),
        "Summary" => Ok(MemoryKind::Summary),
        "RunObservation" => Ok(MemoryKind::RunObservation),
        _ => bail!("Unknown memory kind `{value}`"),
    }
}

fn parse_schedule_kind(value: &str) -> Result<ScheduleKind> {
    match value {
        "System" => Ok(ScheduleKind::System),
        "Reminder" => Ok(ScheduleKind::Reminder),
        "AgentTask" => Ok(ScheduleKind::AgentTask),
        _ => bail!("Unknown schedule kind `{value}`"),
    }
}

fn parse_schedule_status(value: &str) -> Result<ScheduleStatus> {
    match value {
        "Enabled" => Ok(ScheduleStatus::Enabled),
        "Disabled" => Ok(ScheduleStatus::Disabled),
        _ => bail!("Unknown schedule status `{value}`"),
    }
}
