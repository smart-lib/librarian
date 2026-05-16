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
    pub memory: MemoryConfig,
    pub broker: BrokerConfig,
    pub codex: CodexRuntimeConfig,
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerConfig {
    pub max_concurrent_jobs: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MemoryConfig {
    pub embedding_backend: String,
    pub embedding_model: String,
    pub embedding_dimensions: i64,
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
pub struct ThirdEyeConfig {
    pub enabled: bool,
    pub base_url: String,
    pub db_path: Option<PathBuf>,
    pub project_export_dir: PathBuf,
}

impl Default for CodexRuntimeConfig {
    fn default() -> Self {
        Self {
            host_home: default_codex_home(),
            mount_host_home: false,
            mount_read_only: false,
            container_home: "/home/agent/.codex".to_string(),
        }
    }
}

impl Default for ThirdEyeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://127.0.0.1:4317".to_string(),
            db_path: None,
            project_export_dir: PathBuf::from("third-eye-export"),
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

impl Config {
    pub fn load_or_default(home: Option<PathBuf>) -> Result<Self> {
        let home = match home {
            Some(home) => home,
            None => default_home()?,
        };
        let config_path = home.join("config.toml");

        let mut config = Self {
            config_path,
            database_path: home.join("librarian.db"),
            vault_path: home.join("vault"),
            admin: AdminConfig {
                bind: "127.0.0.1:17377".to_string(),
            },
            docker: DockerConfig {
                agent_image: "librarian-agent:latest".to_string(),
                runtime_command: default_runtime_command(),
            },
            worker: WorkerConfig {
                max_concurrent_jobs: std::env::var("LIBRARIAN_WORKER_CONCURRENCY")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1),
            },
            memory: MemoryConfig::default(),
            broker: BrokerConfig::default(),
            codex: CodexRuntimeConfig::default(),
            third_eye: ThirdEyeConfig::default(),
            home,
        };

        if config.config_path.exists() {
            let stored: StoredConfig = toml::from_str(&fs::read_to_string(&config.config_path)?)?;
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
        ensure_dir(&self.vault_path)?;
        ensure_dir(&self.vault_path.join("projects"))?;
        ensure_dir(&self.vault_path.join("runs"))?;
        ensure_dir(&self.vault_path.join("decisions"))?;
        ensure_dir(&self.third_eye.project_export_dir)?;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        self.ensure_layout()?;
        let stored = StoredConfig {
            admin: self.admin.clone(),
            docker: self.docker.clone(),
            worker: self.worker.clone(),
            memory: self.memory.clone(),
            broker: self.broker.clone(),
            codex: self.codex.clone(),
            third_eye: self.third_eye.clone(),
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
        self.memory = stored.memory;
        self.broker = stored.broker;
        self.codex = stored.codex;
        if let Some(path) = self.codex.host_home.clone() {
            self.codex.host_home = Some(stored_path(&self.home, path));
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
    memory: MemoryConfig,
    #[serde(default)]
    broker: BrokerConfig,
    #[serde(default)]
    codex: CodexRuntimeConfig,
    #[serde(default)]
    third_eye: ThirdEyeConfig,
    database_path: Option<PathBuf>,
    vault_path: Option<PathBuf>,
}

fn default_home() -> Result<PathBuf> {
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .context("Could not determine a home directory for Librarian")?;
    Ok(base.join("librarian"))
}

fn default_codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
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
