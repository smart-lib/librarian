use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub home: PathBuf,
    pub config_path: PathBuf,
    pub database_path: PathBuf,
    pub vault_path: PathBuf,
    pub admin: AdminConfig,
    pub docker: DockerConfig,
    pub worker: WorkerConfig,
    pub chat: ChatConfig,
    pub tool_permissions: ToolPermissionsConfig,
    pub memory: MemoryConfig,
    pub routing: RoutingConfig,
    pub budget: BudgetConfig,
    pub broker: BrokerConfig,
    pub codex: CodexRuntimeConfig,
    pub claude: ClaudeRuntimeConfig,
    pub third_eye: ThirdEyeConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdminConfig {
    pub bind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DockerConfig {
    pub agent_image: String,
    pub runtime_command: String,
    #[serde(default)]
    pub runtime_args: Vec<String>,
    #[serde(default = "default_mount_path_style")]
    pub mount_path_style: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerConfig {
    pub max_concurrent_jobs: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatConfig {
    #[serde(default = "default_assistant_name")]
    pub assistant_name: String,
    pub codex_timeout_seconds: u64,
    pub memory_hit_limit: usize,
    pub max_iterations: usize,
}

fn default_assistant_name() -> String {
    "Librarian".to_string()
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermissionPolicy {
    Auto,
    Ask,
    Deny,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolPermissionsConfig {
    #[serde(default)]
    pub preset: ToolPermissionPreset,
    pub library_read: ToolPermissionPolicy,
    pub library_create: ToolPermissionPolicy,
    pub library_edit_markdown: ToolPermissionPolicy,
    pub library_move: ToolPermissionPolicy,
    pub library_delete: ToolPermissionPolicy,
    pub workspace_create: ToolPermissionPolicy,
    pub workspace_move: ToolPermissionPolicy,
    pub workspace_delete: ToolPermissionPolicy,
    pub memory_write: ToolPermissionPolicy,
    pub settings_change: ToolPermissionPolicy,
    pub agent_launch: ToolPermissionPolicy,
    #[serde(default = "default_context_switch_policy")]
    pub context_switch: ToolPermissionPolicy,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermissionPreset {
    Balanced,
    Autopilot,
    Confirm,
    LockedDown,
    Custom,
}

impl Default for ToolPermissionPreset {
    fn default() -> Self {
        Self::Balanced
    }
}

fn default_context_switch_policy() -> ToolPermissionPolicy {
    ToolPermissionPolicy::Ask
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MemoryConfig {
    pub embedding_backend: String,
    pub embedding_model: String,
    pub embedding_dimensions: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RoutingConfig {
    pub fallback_enabled: bool,
    pub fallback_order: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BudgetConfig {
    pub enabled: bool,
    pub daily_total_usd: Option<f64>,
    pub daily_provider_usd: Option<f64>,
    pub daily_project_usd: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BrokerConfig {
    pub bind: String,
    pub container_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CodexRuntimeConfig {
    pub host_home: Option<PathBuf>,
    pub mount_host_home: bool,
    pub mount_read_only: bool,
    pub container_home: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClaudeRuntimeConfig {
    pub host_home: Option<PathBuf>,
    pub mount_host_home: bool,
    pub mount_read_only: bool,
    pub container_home: String,
    pub instruction_file: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ThirdEyeConfig {
    pub enabled: bool,
    pub base_url: String,
    pub db_path: Option<PathBuf>,
    pub project_export_dir: PathBuf,
}

impl Default for CodexRuntimeConfig {
    fn default() -> Self {
        Self {
            host_home: None,
            mount_host_home: false,
            mount_read_only: false,
            container_home: "/home/agent/.codex".to_string(),
        }
    }
}

impl Default for ClaudeRuntimeConfig {
    fn default() -> Self {
        Self {
            host_home: None,
            mount_host_home: false,
            mount_read_only: false,
            container_home: "/home/agent/.claude".to_string(),
            instruction_file: "CLAUDE.md".to_string(),
        }
    }
}

impl Default for ThirdEyeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://127.0.0.1:4317".to_string(),
            db_path: None,
            project_export_dir: PathBuf::from(".mdb/third-eye-export"),
        }
    }
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:17379".to_string(),
            container_url: "http://host.containers.internal:17379".to_string(),
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            embedding_backend: "local-hash".to_string(),
            embedding_model: "local-hash-v1".to_string(),
            embedding_dimensions: 384,
        }
    }
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            assistant_name: default_assistant_name(),
            codex_timeout_seconds: 180,
            memory_hit_limit: 12,
            max_iterations: 6,
        }
    }
}

impl Default for ToolPermissionsConfig {
    fn default() -> Self {
        Self {
            preset: ToolPermissionPreset::Balanced,
            library_read: ToolPermissionPolicy::Auto,
            library_create: ToolPermissionPolicy::Auto,
            library_edit_markdown: ToolPermissionPolicy::Ask,
            library_move: ToolPermissionPolicy::Ask,
            library_delete: ToolPermissionPolicy::Ask,
            workspace_create: ToolPermissionPolicy::Auto,
            workspace_move: ToolPermissionPolicy::Ask,
            workspace_delete: ToolPermissionPolicy::Ask,
            memory_write: ToolPermissionPolicy::Auto,
            settings_change: ToolPermissionPolicy::Ask,
            agent_launch: ToolPermissionPolicy::Ask,
            context_switch: ToolPermissionPolicy::Ask,
        }
    }
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            fallback_enabled: false,
            fallback_order: vec![
                "codex".to_string(),
                "openrouter".to_string(),
                "claude-code".to_string(),
            ],
        }
    }
}

impl Config {
    pub fn load_or_default(home: Option<PathBuf>) -> Result<Self> {
        let home = match home {
            Some(home) => home,
            None => default_home()?,
        };
        let config_path = default_config_path(&home);
        let stored_config_path = if config_path.exists() {
            config_path.clone()
        } else {
            let legacy_config_path = home.join("config.toml");
            if legacy_config_path.exists() {
                legacy_config_path
            } else {
                config_path.clone()
            }
        };

        let mut config = Self {
            config_path,
            database_path: default_database_path(&home),
            vault_path: default_vault_path(&home),
            admin: AdminConfig {
                bind: "127.0.0.1:17377".to_string(),
            },
            docker: DockerConfig {
                agent_image: "librarian-agent:latest".to_string(),
                runtime_command: default_runtime_command(),
                runtime_args: Vec::new(),
                mount_path_style: "host".to_string(),
            },
            worker: WorkerConfig {
                max_concurrent_jobs: std::env::var("LIBRARIAN_WORKER_CONCURRENCY")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1),
            },
            chat: ChatConfig::default(),
            tool_permissions: ToolPermissionsConfig::default(),
            memory: MemoryConfig::default(),
            routing: RoutingConfig::default(),
            budget: BudgetConfig::default(),
            broker: BrokerConfig::default(),
            codex: CodexRuntimeConfig::default(),
            claude: ClaudeRuntimeConfig::default(),
            third_eye: ThirdEyeConfig::default(),
            home,
        };
        if config.codex.host_home.is_none() {
            config.codex.host_home = Some(default_codex_home(&config.home));
        }
        if config.claude.host_home.is_none() {
            config.claude.host_home = default_claude_home(&config.home);
        }
        config.third_eye.project_export_dir =
            stored_path(&config.home, config.third_eye.project_export_dir.clone());

        if stored_config_path.exists() {
            let stored: StoredConfig = toml::from_str(&fs::read_to_string(&stored_config_path)?)?;
            config.apply_stored(stored);
        }

        if let Some(value) = std::env::var("LIBRARIAN_WORKER_CONCURRENCY")
            .ok()
            .and_then(|value| value.parse().ok())
        {
            config.worker.max_concurrent_jobs = value;
        }

        Ok(config)
    }

    pub fn ensure_layout(&self) -> Result<()> {
        ensure_dir(&self.home)?;
        ensure_dir(&app_dir(&self.home))?;
        ensure_dir(&app_dir(&self.home).join("bin"))?;
        ensure_dir(&app_dir(&self.home).join("runs"))?;
        ensure_dir(&cfg_dir(&self.home))?;
        ensure_dir(&mdb_dir(&self.home))?;
        ensure_dir(&projects_dir(&self.home))?;
        ensure_dir(&self.vault_path)?;
        ensure_dir(&self.vault_path.join("projects"))?;
        ensure_dir(&self.vault_path.join("runs"))?;
        ensure_dir(&self.vault_path.join("decisions"))?;
        ensure_dir(&self.third_eye.project_export_dir)?;
        if let Some(path) = &self.codex.host_home {
            ensure_dir(path)?;
        }
        if let Some(path) = &self.claude.host_home {
            ensure_dir(path)?;
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        self.ensure_layout()?;
        let stored = StoredConfig {
            admin: self.admin.clone(),
            docker: self.docker.clone(),
            worker: self.worker.clone(),
            chat: self.chat.clone(),
            tool_permissions: self.tool_permissions.clone(),
            memory: self.memory.clone(),
            routing: self.routing.clone(),
            budget: self.budget.clone(),
            broker: self.broker.clone(),
            codex: stored_codex_config(&self.home, &self.codex),
            claude: stored_claude_config(&self.home, &self.claude),
            third_eye: stored_third_eye_config(&self.home, &self.third_eye),
            database_path: path_to_stored(&self.home, &self.database_path),
            vault_path: path_to_stored(&self.home, &self.vault_path),
        };
        fs::write(&self.config_path, toml::to_string_pretty(&stored)?)?;
        Ok(())
    }

    pub fn set_worker_concurrency(&mut self, value: usize) {
        self.worker.max_concurrent_jobs = value.max(1);
    }

    fn apply_stored(&mut self, stored: StoredConfig) {
        self.admin = stored.admin;
        self.docker = stored.docker;
        self.worker = stored.worker;
        self.chat = stored.chat;
        self.tool_permissions = stored.tool_permissions;
        self.memory = stored.memory;
        self.routing = stored.routing;
        self.budget = stored.budget;
        self.broker = stored.broker;
        self.codex = stored.codex;
        if let Some(path) = self.codex.host_home.clone() {
            self.codex.host_home = Some(stored_path(&self.home, path));
        }
        self.claude = stored.claude;
        if let Some(path) = self.claude.host_home.clone() {
            self.claude.host_home = Some(stored_path(&self.home, path));
        }
        self.third_eye = stored.third_eye;
        self.third_eye.project_export_dir =
            stored_path(&self.home, self.third_eye.project_export_dir.clone());
        if let Some(database_path) = stored.database_path {
            self.database_path = stored_path(&self.home, database_path);
        }
        if let Some(vault_path) = stored.vault_path {
            self.vault_path = stored_path(&self.home, vault_path);
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredConfig {
    admin: AdminConfig,
    docker: DockerConfig,
    worker: WorkerConfig,
    #[serde(default)]
    chat: ChatConfig,
    #[serde(default)]
    tool_permissions: ToolPermissionsConfig,
    #[serde(default)]
    memory: MemoryConfig,
    #[serde(default)]
    routing: RoutingConfig,
    #[serde(default)]
    budget: BudgetConfig,
    #[serde(default)]
    broker: BrokerConfig,
    #[serde(default)]
    codex: CodexRuntimeConfig,
    #[serde(default)]
    claude: ClaudeRuntimeConfig,
    #[serde(default)]
    third_eye: ThirdEyeConfig,
    database_path: Option<PathBuf>,
    vault_path: Option<PathBuf>,
}

fn default_home() -> Result<PathBuf> {
    platform_default_home()
}

fn app_dir(home: &Path) -> PathBuf {
    home.join(".app")
}

fn cfg_dir(home: &Path) -> PathBuf {
    home.join(".cfg")
}

fn mdb_dir(home: &Path) -> PathBuf {
    home.join(".mdb")
}

fn projects_dir(home: &Path) -> PathBuf {
    home.join("Projects")
}

fn default_config_path(home: &Path) -> PathBuf {
    cfg_dir(home).join("config.toml")
}

fn default_database_path(home: &Path) -> PathBuf {
    mdb_dir(home).join("librarian.db")
}

fn default_vault_path(home: &Path) -> PathBuf {
    home.join("Library")
}

pub fn platform_default_home() -> Result<PathBuf> {
    if cfg!(windows) {
        let appdata = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .context("APPDATA is not set; pass --home or set LIBRARIAN_HOME")?;
        return Ok(appdata.join("Librarian"));
    }

    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set; pass --home or set LIBRARIAN_HOME")?;
        return Ok(home
            .join("Library")
            .join("Application Support")
            .join("Librarian"));
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set; pass --home or set LIBRARIAN_HOME")?;
    Ok(home.join("Librarian"))
}

fn default_codex_home(home: &Path) -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| cfg_dir(home).join("codex-home"))
}

fn default_claude_home(home: &Path) -> Option<PathBuf> {
    std::env::var_os("CLAUDE_HOME")
        .map(PathBuf::from)
        .or_else(|| Some(cfg_dir(home).join("claude-home")))
}

fn default_mount_path_style() -> String {
    "host".to_string()
}

fn stored_path(home: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        home.join(path)
    }
}

fn path_to_stored(home: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(home)
        .ok()
        .map(Path::to_path_buf)
        .or_else(|| Some(path.to_path_buf()))
}

fn stored_codex_config(home: &Path, codex: &CodexRuntimeConfig) -> CodexRuntimeConfig {
    let mut stored = codex.clone();
    stored.host_home = codex
        .host_home
        .as_ref()
        .and_then(|path| path_to_stored(home, path));
    stored
}

fn stored_claude_config(home: &Path, claude: &ClaudeRuntimeConfig) -> ClaudeRuntimeConfig {
    let mut stored = claude.clone();
    stored.host_home = claude
        .host_home
        .as_ref()
        .and_then(|path| path_to_stored(home, path));
    stored
}

fn stored_third_eye_config(home: &Path, third_eye: &ThirdEyeConfig) -> ThirdEyeConfig {
    let mut stored = third_eye.clone();
    stored.project_export_dir = path_to_stored(home, &third_eye.project_export_dir)
        .unwrap_or_else(|| third_eye.project_export_dir.clone());
    stored.db_path = third_eye
        .db_path
        .as_ref()
        .and_then(|path| path_to_stored(home, path));
    stored
}

fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("Failed to create {}", path.display()))
}

fn default_runtime_command() -> String {
    if cfg!(windows) {
        "podman".to_string()
    } else {
        "docker".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn persists_routing_and_budget_config_portably() {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-config-{}", Uuid::new_v4()));
        let mut config = Config::load_or_default(Some(home.clone())).expect("config");
        config.routing.fallback_enabled = true;
        config.routing.fallback_order = vec!["openrouter".to_string(), "codex".to_string()];
        config.budget.enabled = true;
        config.budget.daily_total_usd = Some(5.0);
        config.budget.daily_provider_usd = Some(3.0);
        config.budget.daily_project_usd = Some(2.0);
        config.chat.assistant_name = "Sage".to_string();
        config.chat.codex_timeout_seconds = 42;
        config.chat.memory_hit_limit = 7;
        config.chat.max_iterations = 5;
        config.tool_permissions.library_delete = ToolPermissionPolicy::Deny;
        config.save().expect("save");

        let stored =
            std::fs::read_to_string(home.join(".cfg").join("config.toml")).expect("stored config");
        assert!(stored.contains("database_path = "));
        assert!(stored.contains(".mdb"));
        assert!(stored.contains("librarian.db"));
        assert!(stored.contains("vault_path = \"Library\""));
        assert!(stored.contains("host_home = "));
        assert!(stored.contains(".cfg"));
        assert!(stored.contains("codex-home"));
        assert!(stored.contains("[claude]"));
        assert!(stored.contains("claude-home"));
        assert!(stored.contains("instruction_file = \"CLAUDE.md\""));
        assert!(stored.contains("[chat]"));
        assert!(stored.contains("assistant_name = \"Sage\""));
        assert!(stored.contains("codex_timeout_seconds = 42"));
        assert!(stored.contains("[tool_permissions]"));
        assert!(stored.contains("preset = \"balanced\""));
        assert!(stored.contains("library_delete = \"deny\""));
        assert!(stored.contains("context_switch = \"ask\""));

        let reloaded = Config::load_or_default(Some(home.clone())).expect("reload");
        assert!(reloaded.routing.fallback_enabled);
        assert_eq!(
            reloaded.routing.fallback_order,
            vec!["openrouter".to_string(), "codex".to_string()]
        );
        assert!(reloaded.budget.enabled);
        assert_eq!(reloaded.budget.daily_total_usd, Some(5.0));
        assert_eq!(reloaded.budget.daily_provider_usd, Some(3.0));
        assert_eq!(reloaded.budget.daily_project_usd, Some(2.0));
        assert_eq!(reloaded.chat.codex_timeout_seconds, 42);
        assert_eq!(reloaded.chat.assistant_name, "Sage");
        assert_eq!(reloaded.chat.memory_hit_limit, 7);
        assert_eq!(reloaded.chat.max_iterations, 5);
        assert_eq!(reloaded.claude.instruction_file, "CLAUDE.md");
        assert_eq!(
            reloaded.claude.host_home,
            Some(reloaded.home.join(".cfg").join("claude-home"))
        );
        assert_eq!(
            reloaded.tool_permissions.library_delete,
            ToolPermissionPolicy::Deny
        );
        assert_eq!(
            reloaded.tool_permissions.preset,
            ToolPermissionPreset::Balanced
        );
        assert_eq!(
            reloaded.tool_permissions.context_switch,
            ToolPermissionPolicy::Ask
        );
        std::fs::remove_dir_all(home).ok();
    }

    #[test]
    fn creates_single_root_portable_layout() {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-layout-{}", Uuid::new_v4()));
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        config.save().expect("save");

        for relative in [
            ".app",
            ".app/bin",
            ".cfg",
            ".cfg/codex-home",
            ".mdb",
            "Library",
            "Library/projects",
            "Library/runs",
            "Library/decisions",
            "Projects",
        ] {
            assert!(
                home.join(relative).is_dir(),
                "expected {} to exist",
                relative
            );
        }
        assert!(home.join(".cfg").join("config.toml").is_file());

        let stored =
            std::fs::read_to_string(home.join(".cfg").join("config.toml")).expect("stored config");
        let stored_value: toml::Value = toml::from_str(&stored).expect("stored config toml");
        let stored_database_path = stored_value
            .get("database_path")
            .and_then(toml::Value::as_str)
            .expect("stored database_path");
        assert!(PathBuf::from(stored_database_path).is_relative());
        assert!(stored_database_path.contains(".mdb"));
        assert!(stored_database_path.contains("librarian.db"));
        assert_eq!(
            stored_value.get("vault_path").and_then(toml::Value::as_str),
            Some("Library")
        );
        let stored_codex_home = stored_value
            .get("codex")
            .and_then(|codex| codex.get("host_home"))
            .and_then(toml::Value::as_str)
            .expect("stored codex host_home");
        assert!(PathBuf::from(stored_codex_home).is_relative());
        assert!(stored_codex_home.contains(".cfg"));
        assert!(stored_codex_home.contains("codex-home"));

        let reloaded = Config::load_or_default(Some(home.clone())).expect("reload");
        assert_eq!(reloaded.home, home);
        assert_eq!(
            reloaded.database_path,
            reloaded.home.join(".mdb").join("librarian.db")
        );
        assert_eq!(reloaded.vault_path, reloaded.home.join("Library"));
        assert_eq!(
            reloaded.codex.host_home,
            Some(reloaded.home.join(".cfg").join("codex-home"))
        );

        std::fs::remove_dir_all(home).ok();
    }
}
