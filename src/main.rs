mod admin;
mod admin_models;
mod agent_policy;
mod broker;
mod chat;
mod config;
mod db;
mod docker_runner;
mod domain;
mod gates;
mod job_review;
mod library_tools;
mod memory;
mod memory_policy;
mod prompt;
mod provider_health;
mod providers;
mod router;
mod scheduler;
mod secrets;
mod slash_utils;
mod third_eye;
mod vault;
mod worker;

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use config::Config;
use db::Database;
use docker_runner::DockerRunner;
use domain::{
    JobStatus, MemoryKind, MountMode, Project, ProviderKind, ScheduleKind, ScheduleStatus,
    ToolApprovalStatus,
};
use job_review::GitGateActionArg;
use secrets::SecretVault;
use serde_json::json;
use tokio::process::Command as TokioCommand;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Parser)]
#[command(name = "librarian")]
#[command(version)]
#[command(about = "Local-first harness for containerized coding agents")]
struct Cli {
    #[arg(long, env = "LIBRARIAN_HOME")]
    home: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Setup {
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        yes: bool,
        #[arg(long, value_enum, default_value_t = SetupRuntimeArg::Auto)]
        runtime: SetupRuntimeArg,
        #[arg(long, default_value = "podman-machine-default")]
        wsl_distro: String,
        #[arg(long)]
        build_agent_image: bool,
        #[arg(long)]
        skip_doctor: bool,
    },
    Init,
    Doctor {
        #[arg(long)]
        smoke: bool,
        #[arg(long, value_enum, default_value_t = ProviderArg::Codex)]
        smoke_provider: ProviderArg,
        #[arg(long)]
        smoke_run_agent: bool,
    },
    Upgrade {
        #[arg(long)]
        nightly: bool,
        #[arg(long = "ref")]
        reference: Option<String>,
    },
    Admin {
        #[arg(long)]
        bind: Option<String>,
    },
    Broker {
        #[arg(long)]
        bind: Option<String>,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    Smoke {
        #[command(subcommand)]
        command: SmokeCommand,
    },
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Run {
        #[arg(long)]
        project: String,
        #[arg(long, value_enum, default_value_t = ProviderArg::Codex)]
        provider: ProviderArg,
        #[arg(long)]
        goal: String,
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        secret_grant_token: Option<String>,
    },
    Jobs {
        #[command(subcommand)]
        command: Option<JobsCommand>,
    },
    Context {
        query: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, default_value_t = memory::default_hit_limit())]
        limit: usize,
        #[arg(long)]
        prompt: bool,
    },
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    Schedule {
        #[command(subcommand)]
        command: ScheduleCommand,
    },
    Events {
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Secrets {
        #[command(subcommand)]
        command: SecretsCommand,
    },
    Providers {
        #[command(subcommand)]
        command: ProvidersCommand,
    },
    Usage {
        #[command(subcommand)]
        command: UsageCommand,
    },
    ThirdEye {
        #[command(subcommand)]
        command: ThirdEyeCommand,
    },
    Scheduler {
        #[arg(long)]
        once: bool,
    },
    Worker {
        #[arg(long)]
        once: bool,
        #[arg(long)]
        concurrency: Option<usize>,
    },
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    Codex {
        #[arg(long)]
        enable_container_mount: bool,
        #[arg(long)]
        codex_home: Option<PathBuf>,
        #[arg(long)]
        read_only: bool,
    },
    Claude {
        #[arg(long)]
        enable_container_mount: bool,
        #[arg(long)]
        claude_home: Option<PathBuf>,
        #[arg(long)]
        read_only: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RuntimeCommand {
    UseWslPodman {
        #[arg(long, default_value = "podman-machine-default")]
        distro: String,
    },
    UseHostRuntime {
        #[arg(long, default_value_t = default_runtime_command_arg())]
        command: String,
    },
    BuildAgentImage {
        #[arg(long)]
        no_codex: bool,
        #[arg(long)]
        no_claude: bool,
    },
    SmokePlan {
        #[arg(long, default_value = "LibrarianSmoke")]
        project: String,
    },
}

#[derive(Debug, Subcommand)]
enum SmokeCommand {
    All {
        #[arg(long, value_enum, default_value_t = ProviderArg::Codex)]
        provider: ProviderArg,
        #[arg(long)]
        run_agent: bool,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        secret_grant_token: Option<String>,
        #[arg(long)]
        secret: Option<String>,
        #[arg(long, default_value = "LibrarianSmoke")]
        name: String,
        #[arg(long)]
        require_providers_ready: bool,
    },
    Mvp {
        #[arg(long, value_enum, default_value_t = ProviderArg::Codex)]
        provider: ProviderArg,
        #[arg(long)]
        run_agent: bool,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        secret_grant_token: Option<String>,
        #[arg(long)]
        secret: Option<String>,
        #[arg(long, default_value = "LibrarianSmoke")]
        name: String,
    },
    Context {
        #[arg(long, default_value = "LibrarianContextSmoke")]
        name: String,
    },
    Tools {
        #[arg(long, default_value = "LibrarianToolsSmoke")]
        name: String,
    },
    SelfHost {
        #[arg(long, value_enum, default_value_t = ProviderArg::Codex)]
        provider: ProviderArg,
        #[arg(long)]
        project_path: Option<PathBuf>,
        #[arg(long)]
        run_agent: bool,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        secret_grant_token: Option<String>,
    },
    Providers {
        #[arg(long)]
        require_ready: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    AttachLibrary {
        project: String,
        library_path: String,
    },
    DetachLibrary {
        project: String,
    },
    List,
}

#[derive(Debug, Subcommand)]
enum JobsCommand {
    List,
    Events {
        job_id: uuid::Uuid,
    },
    Preflight {
        job_id: uuid::Uuid,
    },
    Review {
        job_id: uuid::Uuid,
        #[arg(long)]
        run_tests: bool,
    },
    ReviewPacket {
        job_id: uuid::Uuid,
        #[arg(long)]
        run_tests: bool,
        #[arg(long)]
        revert_commit: Option<String>,
    },
    Gate {
        job_id: uuid::Uuid,
        #[arg(long, value_enum)]
        action: GitGateActionArg,
    },
    ProposeGit {
        job_id: uuid::Uuid,
        #[arg(long, value_enum)]
        action: GitGateActionArg,
        #[arg(long)]
        message: Option<String>,
        #[arg(long)]
        commit: Option<String>,
    },
    RevertPlan {
        job_id: uuid::Uuid,
        #[arg(long)]
        commit: Option<String>,
    },
    PushPlan {
        job_id: uuid::Uuid,
    },
    Cancel {
        job_id: uuid::Uuid,
    },
    Retry {
        job_id: uuid::Uuid,
    },
}

#[derive(Debug, Subcommand)]
enum MemoryCommand {
    Add {
        content: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, value_enum, default_value_t = MemoryKindArg::Fact)]
        kind: MemoryKindArg,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        source: Option<String>,
    },
    Recent {
        #[arg(long)]
        project: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: i64,
    },
    Embed {
        #[arg(long, default_value_t = 1000)]
        limit: i64,
    },
    Status,
}

#[derive(Debug, Subcommand)]
enum ScheduleCommand {
    List,
    Add {
        name: String,
        #[arg(long, default_value_t = 3600)]
        every_seconds: i64,
        #[arg(long, value_enum, default_value_t = ScheduleKindArg::Reminder)]
        kind: ScheduleKindArg,
        #[arg(long)]
        payload: Option<String>,
    },
    Update {
        schedule_id: uuid::Uuid,
        #[arg(long)]
        name: String,
        #[arg(long)]
        every_seconds: i64,
        #[arg(long, value_enum)]
        kind: ScheduleKindArg,
        #[arg(long)]
        payload: Option<String>,
    },
    Run {
        schedule_id: uuid::Uuid,
    },
    Enable {
        schedule_id: uuid::Uuid,
    },
    Disable {
        schedule_id: uuid::Uuid,
    },
    Delete {
        schedule_id: uuid::Uuid,
    },
    Tick,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Show,
    SetConcurrency {
        max_concurrent_jobs: usize,
    },
    SetFallbacks {
        enabled: String,
    },
    SetFallbackOrder {
        providers: Vec<String>,
    },
    SetBudget {
        enabled: String,
        #[arg(long)]
        daily_total_usd: Option<f64>,
        #[arg(long)]
        daily_provider_usd: Option<f64>,
        #[arg(long)]
        daily_project_usd: Option<f64>,
    },
    SetCodexHome {
        path: PathBuf,
    },
    SetCodexMount {
        enabled: bool,
        #[arg(long)]
        read_only: Option<bool>,
    },
}

#[derive(Debug, Subcommand)]
enum SecretsCommand {
    Status,
    Set {
        name: String,
        #[arg(long)]
        provider: String,
        #[arg(long, default_value = "api-key")]
        kind: String,
        #[arg(long)]
        value: Option<String>,
    },
    List,
    Grant {
        secret: String,
        #[arg(long)]
        job_id: Option<uuid::Uuid>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long, default_value = "read")]
        capability: String,
        #[arg(long, default_value_t = 900)]
        ttl_seconds: i64,
        #[arg(long, default_value_t = 1)]
        max_uses: i64,
    },
    Resolve {
        token: String,
        #[arg(long, default_value = "read")]
        capability: String,
        #[arg(long)]
        job_id: Option<uuid::Uuid>,
        #[arg(long)]
        reveal: bool,
    },
    Grants {
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    Audit {
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
}

#[derive(Debug, Subcommand)]
enum ProvidersCommand {
    Catalog,
    Status,
    Pause {
        provider: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value_t = 1800)]
        seconds: i64,
        #[arg(long, default_value = "manual pause")]
        reason: String,
    },
    Resume {
        provider: String,
        #[arg(long)]
        model: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum UsageCommand {
    List {
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    AddLimit {
        provider: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        job_id: Option<uuid::Uuid>,
        #[arg(long, default_value = "manual limit observation")]
        reason: String,
    },
}

#[derive(Debug, Subcommand)]
enum ThirdEyeCommand {
    Status,
    Providers,
    Refresh {
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        full: bool,
    },
    DbSummary,
    Configure {
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        db_path: Option<PathBuf>,
        #[arg(long)]
        enabled: Option<bool>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProviderArg {
    Codex,
    OpenRouter,
    ClaudeCode,
}

#[derive(Clone, Debug, ValueEnum)]
enum SetupRuntimeArg {
    Auto,
    Host,
    WslPodman,
    None,
}

#[derive(Clone, Debug, ValueEnum)]
enum MemoryKindArg {
    UserMessage,
    AssistantMessage,
    Decision,
    Instruction,
    Fact,
    Status,
    Summary,
    RunObservation,
}

#[derive(Clone, Debug, ValueEnum)]
enum ScheduleKindArg {
    System,
    Reminder,
    AgentTask,
}

impl From<ProviderArg> for ProviderKind {
    fn from(value: ProviderArg) -> Self {
        match value {
            ProviderArg::Codex => ProviderKind::Codex,
            ProviderArg::OpenRouter => ProviderKind::OpenRouter,
            ProviderArg::ClaudeCode => ProviderKind::ClaudeCode,
        }
    }
}

impl From<MemoryKindArg> for MemoryKind {
    fn from(value: MemoryKindArg) -> Self {
        match value {
            MemoryKindArg::UserMessage => MemoryKind::UserMessage,
            MemoryKindArg::AssistantMessage => MemoryKind::AssistantMessage,
            MemoryKindArg::Decision => MemoryKind::Decision,
            MemoryKindArg::Instruction => MemoryKind::Instruction,
            MemoryKindArg::Fact => MemoryKind::Fact,
            MemoryKindArg::Status => MemoryKind::Status,
            MemoryKindArg::Summary => MemoryKind::Summary,
            MemoryKindArg::RunObservation => MemoryKind::RunObservation,
        }
    }
}

impl From<ScheduleKindArg> for ScheduleKind {
    fn from(value: ScheduleKindArg) -> Self {
        match value {
            ScheduleKindArg::System => ScheduleKind::System,
            ScheduleKindArg::Reminder => ScheduleKind::Reminder,
            ScheduleKindArg::AgentTask => ScheduleKind::AgentTask,
        }
    }
}

fn resolve_cli_home(cli: &Cli) -> Result<Option<PathBuf>> {
    if let Some(home) = &cli.home {
        return Ok(Some(home.clone()));
    }

    let Command::Setup { root, yes, .. } = &cli.command else {
        return Ok(None);
    };

    if let Some(root) = root {
        return Ok(Some(root.clone()));
    }

    let default = config::platform_default_home()?;
    if *yes {
        return Ok(Some(default));
    }

    println!("Librarian needs a root directory for config, SQLite, knowledge base, run artifacts, and portable agent profiles.");
    println!("Default root: {}", default.display());
    print!("Use this path? Press Enter to accept, or type another path: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if input.is_empty() {
        Ok(Some(default))
    } else {
        Ok(Some(PathBuf::from(input)))
    }
}

async fn run_setup(
    mut config: Config,
    runtime: SetupRuntimeArg,
    wsl_distro: &str,
    build_agent_image: bool,
    skip_doctor: bool,
) -> Result<()> {
    config.ensure_layout()?;
    config.save()?;
    let db = Database::connect(&config).await?;
    db.migrate().await?;

    let launch_dir = std::env::current_dir()?;
    println!("Librarian root: {}", config.home.display());
    println!("Launch context: {}", launch_dir.display());
    println!("Admin UI: http://{}", config.admin.bind);

    match runtime {
        SetupRuntimeArg::None => {
            println!("Runtime configuration skipped.");
        }
        SetupRuntimeArg::Host => {
            config.docker.runtime_command = default_runtime_command_arg();
            config.docker.runtime_args.clear();
            config.docker.mount_path_style = "host".to_string();
            config.save()?;
            println!("Runtime set to {}", runtime_display(&config));
        }
        SetupRuntimeArg::WslPodman => {
            set_wsl_podman_runtime(&mut config, wsl_distro)?;
            println!("Runtime set to {}", runtime_display(&config));
        }
        SetupRuntimeArg::Auto => {
            if cfg!(windows) && wsl_podman_available(wsl_distro).await {
                set_wsl_podman_runtime(&mut config, wsl_distro)?;
                println!("Runtime auto-detected: {}", runtime_display(&config));
            } else {
                println!("Runtime left as {}", runtime_display(&config));
            }
        }
    }

    if build_agent_image {
        build_agent_image_with_config(&config, false, false).await?;
    } else {
        println!("Agent image build skipped. Run `librarian runtime build-agent-image` when the runtime is ready.");
    }

    println!(
        "Portable Codex profile: {}",
        optional_path(&config.codex.host_home)
    );
    println!("Next auth step:");
    if let Some(path) = &config.codex.host_home {
        println!("  CODEX_HOME={} codex", shell_path(path));
        println!(
            "  {} auth codex --enable-container-mount --codex-home {}",
            doctor_command_prefix(&config),
            shell_path(path)
        );
    } else {
        println!("  Set CODEX_HOME to the profile path, run `codex`, then run:");
        println!("  librarian auth codex --enable-container-mount --codex-home <profile-path>");
    }

    if !skip_doctor {
        println!();
        run_doctor(&config).await?;
    }

    Ok(())
}

fn set_wsl_podman_runtime(config: &mut Config, distro: &str) -> Result<()> {
    config.docker.runtime_command = "wsl.exe".to_string();
    config.docker.runtime_args = vec![
        "-d".to_string(),
        distro.to_string(),
        "--".to_string(),
        "podman".to_string(),
    ];
    config.docker.mount_path_style = "wsl".to_string();
    config.save()
}

async fn wsl_podman_available(distro: &str) -> bool {
    TokioCommand::new("wsl.exe")
        .args(["-d", distro, "--", "podman", "info"])
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn build_agent_image_with_config(
    config: &Config,
    no_codex: bool,
    no_claude: bool,
) -> Result<()> {
    let install_codex = if no_codex { "false" } else { "true" };
    let install_claude = if no_claude { "false" } else { "true" };
    println!(
        "Building {} from Dockerfile.agent (INSTALL_CODEX={install_codex}, INSTALL_CLAUDE={install_claude})",
        config.docker.agent_image
    );
    let codex_build_arg = format!("INSTALL_CODEX={install_codex}");
    let claude_build_arg = format!("INSTALL_CLAUDE={install_claude}");
    let mut args = config.docker.runtime_args.clone();
    args.extend(
        [
            "build",
            "-t",
            &config.docker.agent_image,
            "--build-arg",
            &codex_build_arg,
            "--build-arg",
            &claude_build_arg,
            "-f",
            "Dockerfile.agent",
            ".",
        ]
        .into_iter()
        .map(str::to_string),
    );
    let status = TokioCommand::new(&config.docker.runtime_command)
        .args(args)
        .status()
        .await?;
    if !status.success() {
        anyhow::bail!("Agent image build failed with status {status}");
    }
    Ok(())
}

async fn run_upgrade(config: &Config, nightly: bool, reference: Option<&str>) -> Result<()> {
    if cfg!(windows) {
        anyhow::bail!(
            "The built-in upgrade command currently supports Ubuntu/Linux installs. Re-run the Windows bootstrap script for now."
        );
    }
    let mut command = format!(
        "LIBRARIAN_ROOT={} wget -qO- https://raw.githubusercontent.com/smart-lib/librarian/main/scripts/install-ubuntu.sh | bash -s -- --dir {}",
        shell_path(&config.home),
        shell_path(&config.home)
    );
    if nightly {
        command.push_str(" --nightly");
    }
    if let Some(reference) = reference {
        command.push_str(" --ref ");
        command.push_str(&shell_word(reference));
    }
    println!("Running Librarian upgrade:");
    println!("  {command}");
    let status = TokioCommand::new("sh")
        .arg("-lc")
        .arg(&command)
        .status()
        .await?;
    if !status.success() {
        anyhow::bail!("Librarian upgrade failed with status {status}");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "librarian=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let home = resolve_cli_home(&cli)?;
    let config = Config::load_or_default(home)?;

    match cli.command {
        Command::Setup {
            root: _,
            yes: _,
            runtime,
            wsl_distro,
            build_agent_image,
            skip_doctor,
        } => {
            run_setup(config, runtime, &wsl_distro, build_agent_image, skip_doctor).await?;
        }
        Command::Init => {
            config.ensure_layout()?;
            if !config.config_path.exists() {
                config.save()?;
            }
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            println!("Initialized Librarian at {}", config.home.display());
            println!("Admin UI: http://{}", config.admin.bind);
        }
        Command::Doctor {
            smoke,
            smoke_provider,
            smoke_run_agent,
        } => {
            run_doctor(&config).await?;
            if smoke {
                println!();
                run_all_smoke(
                    &config,
                    smoke_provider.into(),
                    smoke_run_agent,
                    false,
                    None,
                    None,
                    "LibrarianDoctorSmoke",
                    false,
                )
                .await?;
            }
        }
        Command::Upgrade { nightly, reference } => {
            run_upgrade(&config, nightly, reference.as_deref()).await?;
        }
        Command::Admin { bind } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            let bind = bind.unwrap_or_else(|| config.admin.bind.clone());
            admin::serve(bind, db, config).await?;
        }
        Command::Broker { bind } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            let bind = bind.unwrap_or_else(|| config.broker.bind.clone());
            broker::serve(bind, db, config).await?;
        }
        Command::Runtime { command } => match command {
            RuntimeCommand::UseWslPodman { distro } => {
                let mut config = config;
                set_wsl_podman_runtime(&mut config, &distro)?;
                println!("Runtime set to {}", runtime_display(&config));
            }
            RuntimeCommand::UseHostRuntime { command } => {
                let mut config = config;
                config.docker.runtime_command = command;
                config.docker.runtime_args.clear();
                config.docker.mount_path_style = "host".to_string();
                config.save()?;
                println!("Runtime set to {}", runtime_display(&config));
            }
            RuntimeCommand::BuildAgentImage {
                no_codex,
                no_claude,
            } => {
                build_agent_image_with_config(&config, no_codex, no_claude).await?;
            }
            RuntimeCommand::SmokePlan { project } => {
                print_runtime_smoke_plan(&config, &project);
            }
        },
        Command::Smoke { command } => match command {
            SmokeCommand::All {
                provider,
                run_agent,
                allow_network,
                secret_grant_token,
                secret,
                name,
                require_providers_ready,
            } => {
                let provider = provider.into();
                run_all_smoke(
                    &config,
                    provider,
                    run_agent,
                    allow_network,
                    secret_grant_token.as_deref(),
                    secret.as_deref(),
                    &name,
                    require_providers_ready,
                )
                .await?;
            }
            SmokeCommand::Mvp {
                provider,
                run_agent,
                allow_network,
                secret_grant_token,
                secret,
                name,
            } => {
                let provider = provider.into();
                run_mvp_smoke(
                    &config,
                    provider,
                    run_agent,
                    allow_network,
                    secret_grant_token.as_deref(),
                    secret.as_deref(),
                    &name,
                )
                .await?;
            }
            SmokeCommand::Context { name } => {
                run_context_smoke(&config, &name).await?;
            }
            SmokeCommand::Tools { name } => {
                run_tools_smoke(&config, &name).await?;
            }
            SmokeCommand::SelfHost {
                provider,
                project_path,
                run_agent,
                allow_network,
                secret_grant_token,
            } => {
                run_self_host_smoke(
                    &config,
                    provider.into(),
                    project_path.as_deref(),
                    run_agent,
                    allow_network,
                    secret_grant_token.as_deref(),
                )
                .await?;
            }
            SmokeCommand::Providers { require_ready } => {
                run_provider_smoke(&config, require_ready).await?;
            }
        },
        Command::Auth { command } => match command {
            AuthCommand::Codex {
                enable_container_mount,
                codex_home,
                read_only,
            } => {
                println!("Starting Codex auth bootstrap.");
                if let Some(path) = &config.codex.host_home {
                    println!("Portable Codex profile path: {}", path.display());
                    println!("For a local portable profile, run Codex with CODEX_HOME set to that path before sign-in.");
                }
                println!("Run `codex` in this terminal and complete the OpenAI sign-in flow.");
                println!("Librarian will avoid copying Codex credentials into project files.");
                if enable_container_mount || codex_home.is_some() {
                    let mut config = config;
                    if let Some(codex_home) = codex_home {
                        std::fs::create_dir_all(&codex_home)?;
                        config.codex.host_home = Some(codex_home.canonicalize()?);
                    }
                    config.codex.mount_host_home = enable_container_mount;
                    config.codex.mount_read_only = read_only;
                    config.save()?;
                    println!(
                        "Saved Codex runtime profile. host_home={} mount_host_home={} read_only={}",
                        optional_path(&config.codex.host_home),
                        config.codex.mount_host_home,
                        config.codex.mount_read_only
                    );
                } else {
                    println!("For containerized Codex runs, enable the explicit mount with:");
                    println!("  librarian auth codex --enable-container-mount");
                }
            }
            AuthCommand::Claude {
                enable_container_mount,
                claude_home,
                read_only,
            } => {
                println!("Starting Claude Code auth bootstrap.");
                if let Some(path) = &config.claude.host_home {
                    println!("Portable Claude profile path: {}", path.display());
                    println!("Run Claude Code with CLAUDE_HOME set to that path before sign-in.");
                }
                println!("Run `claude` in this terminal and complete the Anthropic sign-in flow.");
                println!("Librarian will mount the selected Claude profile only when you explicitly enable it.");
                if enable_container_mount || claude_home.is_some() {
                    let mut config = config;
                    if let Some(claude_home) = claude_home {
                        std::fs::create_dir_all(&claude_home)?;
                        config.claude.host_home = Some(claude_home.canonicalize()?);
                    }
                    config.claude.mount_host_home = enable_container_mount;
                    config.claude.mount_read_only = read_only;
                    config.save()?;
                    println!(
                        "Saved Claude runtime profile. host_home={} mount_host_home={} read_only={} instruction_file={}",
                        optional_path(&config.claude.host_home),
                        config.claude.mount_host_home,
                        config.claude.mount_read_only,
                        config.claude.instruction_file
                    );
                } else {
                    println!("For containerized Claude Code runs, enable the explicit mount with:");
                    println!("  librarian auth claude --enable-container-mount");
                }
            }
        },
        Command::Project { command } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            match command {
                ProjectCommand::Add { path, name } => {
                    let path = path.canonicalize()?;
                    let name = name.unwrap_or_else(|| {
                        path.file_name()
                            .and_then(|part| part.to_str())
                            .unwrap_or("project")
                            .to_string()
                    });
                    let project = db.add_project(&name, &path).await?;
                    let note = vault::Vault::new(&config).write_project_note(&project)?;
                    println!("Added project `{}` as {}", project.name, project.id);
                    println!("Project note: {}", note.display());
                }
                ProjectCommand::List => {
                    for project in db.list_projects().await? {
                        let library_path = project
                            .library_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "-".to_string());
                        println!(
                            "{}  {}  library={}  workspace={}",
                            project.id,
                            project.name,
                            library_path,
                            project.path.display()
                        );
                    }
                }
                ProjectCommand::AttachLibrary {
                    project,
                    library_path,
                } => {
                    let project = db.get_project_by_name_or_id(&project).await?;
                    let library_path = library_tools::normalize_tool_relative_path(&library_path)?;
                    let project = db
                        .attach_project_library_path(project.id, Path::new(&library_path))
                        .await?;
                    println!(
                        "Attached library path `{}` to project `{}`",
                        project
                            .library_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "-".to_string()),
                        project.name
                    );
                }
                ProjectCommand::DetachLibrary { project } => {
                    let project = db.get_project_by_name_or_id(&project).await?;
                    let project = db.detach_project_library_path(project.id).await?;
                    println!("Detached library path from project `{}`", project.name);
                }
            }
        }
        Command::Run {
            project,
            provider,
            goal,
            read_only,
            allow_network,
            secret_grant_token,
        } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            let project = db.get_project_by_name_or_id(&project).await?;
            let mount_mode = if read_only {
                MountMode::ReadOnly
            } else {
                MountMode::ReadWrite
            };
            agent_policy::ensure_agent_job_allowed(
                &project,
                mount_mode,
                agent_policy::JobCreationSource::ExplicitUserAction,
            )?;
            let provider_kind: ProviderKind = provider.into();
            let network_mode = router::default_network_mode_for_provider(
                &provider_kind,
                allow_network,
                secret_grant_token.is_some(),
            );
            let gated = gates::process_user_prompt(&db, &config, &goal, "cli-run").await?;
            let goal_memory = db
                .add_memory_item(
                    Some(project.id),
                    None,
                    MemoryKind::UserMessage,
                    Some("agent-run-goal"),
                    &gated.content,
                    Some("cli:run"),
                    json!({ "project": project.name.clone() }),
                )
                .await?;
            memory::embed_item(&db, &config, &goal_memory).await?;
            let context_pack = memory::retrieve_context_with_config(
                &db,
                Some(&config),
                memory::RetrievalRequest {
                    query: gated.content.clone(),
                    project_id: Some(project.id),
                    activity_id: None,
                    limit: memory::default_hit_limit(),
                },
            )
            .await?;
            let job = db
                .create_job(
                    project.id,
                    provider_kind,
                    &gated.content,
                    mount_mode,
                    network_mode,
                    secret_grant_token.as_deref(),
                )
                .await?;
            db.add_job_event(
                job.id,
                "context_pack",
                json!({
                    "query": context_pack.query.clone(),
                    "generated_at": context_pack.generated_at,
                    "hits": context_pack.hits.clone(),
                }),
            )
            .await?;
            if !gated.events.is_empty() {
                db.add_job_event(job.id, "gate_events", json!({ "events": gated.events }))
                    .await?;
            }
            println!("Created job {}", job.id);
            println!("Context hits: {}", context_pack.hits.len());
            let agent_blocks = db.list_prompt_blocks(Some(prompt::TARGET_AGENTS)).await?;
            let agent_instruction_blocks = prompt::render_prompt_blocks(&agent_blocks);
            let instruction_files =
                worker::provider_instruction_files(&db, &config, &job.provider).await?;
            let spec = domain::AgentRunSpec {
                job_id: job.id,
                project_path: project.path.clone(),
                provider: job.provider,
                goal: job.goal,
                prompt: prompt::build_agent_prompt(
                    &project,
                    &gated.content,
                    &context_pack,
                    &agent_instruction_blocks,
                ),
                instruction_files,
                mount_mode: job.mount_mode,
                network_mode: job.network_mode,
                secret_grant_token,
            };
            let runner = DockerRunner::new(config.clone());
            let docker_command = runner.docker_command_parts(&spec).await?;
            println!("Prepared Docker command: {}", docker_command.join(" "));
            println!("Run `librarian worker --once` to execute the next queued job.");
        }
        Command::Jobs { command } => {
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            match command.unwrap_or(JobsCommand::List) {
                JobsCommand::List => {
                    for job in db.list_jobs().await? {
                        println!(
                            "{}  {:?}  {:?}  heartbeat={}  {}",
                            job.id,
                            job.provider,
                            job.status,
                            job.last_heartbeat_at
                                .map(|time| time.to_rfc3339())
                                .unwrap_or_else(|| "-".to_string()),
                            job.goal
                        );
                    }
                }
                JobsCommand::Events { job_id } => {
                    for event in db.list_job_events(job_id).await? {
                        println!(
                            "{}  {}  {}",
                            event.created_at.to_rfc3339(),
                            event.kind,
                            serde_json::to_string(&event.payload)?
                        );
                    }
                }
                JobsCommand::Preflight { job_id } => {
                    let report = worker::preflight_job(config.clone(), db.clone(), job_id).await?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                JobsCommand::Review { job_id, run_tests } => {
                    let report = job_review::review_job_changes(&db, job_id, run_tests).await?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                JobsCommand::ReviewPacket {
                    job_id,
                    run_tests,
                    revert_commit,
                } => {
                    let report = job_review::build_job_review_packet(
                        &db,
                        job_id,
                        run_tests,
                        revert_commit.as_deref(),
                    )
                    .await?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                JobsCommand::Gate { job_id, action } => {
                    let report = job_review::gate_job_git_action(&db, job_id, action).await?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                JobsCommand::ProposeGit {
                    job_id,
                    action,
                    message,
                    commit,
                } => {
                    let approval = job_review::propose_job_git_action(
                        &db,
                        job_id,
                        action,
                        message.as_deref(),
                        commit.as_deref(),
                    )
                    .await?;
                    println!("{}", serde_json::to_string_pretty(&approval)?);
                }
                JobsCommand::RevertPlan { job_id, commit } => {
                    let report =
                        job_review::plan_job_git_revert(&db, job_id, commit.as_deref()).await?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                JobsCommand::PushPlan { job_id } => {
                    let report = job_review::plan_job_git_push(&db, job_id).await?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                JobsCommand::Cancel { job_id } => {
                    db.request_cancel_job(job_id).await?;
                    println!("Cancel requested for job {job_id}");
                }
                JobsCommand::Retry { job_id } => {
                    let retry = db.retry_job(job_id).await?;
                    db.update_job_status(retry.id, JobStatus::Queued).await?;
                    println!("Queued retry job {} for {}", retry.id, job_id);
                }
            }
        }
        Command::Context {
            query,
            project,
            limit,
            prompt: show_prompt,
        } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            let project = if let Some(project) = project {
                Some(db.get_project_by_name_or_id(&project).await?)
            } else {
                None
            };
            let project_id = project.as_ref().map(|project| project.id);
            let pack = memory::retrieve_context_with_config(
                &db,
                Some(&config),
                memory::RetrievalRequest {
                    query: query.clone(),
                    project_id,
                    activity_id: None,
                    limit,
                },
            )
            .await?;
            if show_prompt {
                if let Some(project) = project {
                    let agent_blocks = db.list_prompt_blocks(Some(prompt::TARGET_AGENTS)).await?;
                    let agent_instruction_blocks = prompt::render_prompt_blocks(&agent_blocks);
                    println!(
                        "{}",
                        prompt::build_agent_prompt(
                            &project,
                            &query,
                            &pack,
                            &agent_instruction_blocks
                        )
                    );
                } else {
                    println!("{}", memory::render_context_pack(&pack));
                }
            } else {
                println!("{}", memory::render_context_pack(&pack));
            }
        }
        Command::Memory { command } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            match command {
                MemoryCommand::Add {
                    content,
                    project,
                    kind,
                    topic,
                    source,
                } => {
                    let project_id = if let Some(project) = project {
                        Some(db.get_project_by_name_or_id(&project).await?.id)
                    } else {
                        None
                    };
                    let item = db
                        .add_memory_item(
                            project_id,
                            None,
                            kind.into(),
                            topic.as_deref(),
                            &gates::process_user_prompt(&db, &config, &content, "cli-memory")
                                .await?
                                .content,
                            source.as_deref(),
                            json!({ "ingest": "cli" }),
                        )
                        .await?;
                    memory::embed_item(&db, &config, &item).await?;
                    println!("Added memory {}", item.id);
                }
                MemoryCommand::Recent { project, limit } => {
                    let project_id = if let Some(project) = project {
                        Some(db.get_project_by_name_or_id(&project).await?.id)
                    } else {
                        None
                    };
                    for item in db.recent_memory_for_project(project_id, limit).await? {
                        println!(
                            "{}  {:?}  {}  {}",
                            item.id,
                            item.kind,
                            item.observed_at.to_rfc3339(),
                            item.content
                        );
                    }
                }
                MemoryCommand::Embed { limit } => {
                    let count = memory::backfill_embeddings(&db, &config, limit).await?;
                    println!(
                        "Embedded {count} memory item(s) with {} / {} dimensions",
                        config.memory.embedding_model, config.memory.embedding_dimensions
                    );
                }
                MemoryCommand::Status => {
                    let total = db.count_memory_items().await?;
                    let embedded = db
                        .count_memory_embeddings(&config.memory.embedding_model)
                        .await?;
                    let missing = db
                        .count_memory_missing_embedding(&config.memory.embedding_model)
                        .await?;
                    println!("Memory items: {total}");
                    println!("Embedding backend: {}", config.memory.embedding_backend);
                    println!("Embedding model: {}", config.memory.embedding_model);
                    println!(
                        "Embedding dimensions: {}",
                        config.memory.embedding_dimensions
                    );
                    println!("Embedded items: {embedded}");
                    println!("Missing embeddings: {missing}");
                }
            }
        }
        Command::Schedule { command } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            match command {
                ScheduleCommand::List => {
                    for schedule in db.list_schedules().await? {
                        println!(
                            "{}  {:?}  {:?}  every={}s  next={}  {}",
                            schedule.id,
                            schedule.kind,
                            schedule.status,
                            schedule.interval_seconds,
                            schedule.next_run_at.to_rfc3339(),
                            schedule.name
                        );
                    }
                }
                ScheduleCommand::Add {
                    name,
                    every_seconds,
                    kind,
                    payload,
                } => {
                    let payload = match payload {
                        Some(payload) => serde_json::from_str(&payload)?,
                        None => json!({}),
                    };
                    let schedule = db
                        .add_schedule(&name, kind.into(), every_seconds.max(1), payload)
                        .await?;
                    println!("Added schedule {} `{}`", schedule.id, schedule.name);
                }
                ScheduleCommand::Update {
                    schedule_id,
                    name,
                    every_seconds,
                    kind,
                    payload,
                } => {
                    let payload = match payload {
                        Some(payload) => serde_json::from_str(&payload)?,
                        None => json!({}),
                    };
                    let schedule = db
                        .update_schedule(
                            schedule_id,
                            &name,
                            kind.into(),
                            every_seconds.max(1),
                            payload,
                        )
                        .await?;
                    db.add_system_event(
                        "schedule_updated",
                        json!({ "schedule_id": schedule.id, "name": schedule.name }),
                    )
                    .await?;
                    println!("Updated schedule {} `{}`", schedule.id, schedule.name);
                }
                ScheduleCommand::Run { schedule_id } => {
                    scheduler::run_schedule_now(&db, &config, schedule_id).await?;
                    println!("Ran schedule {schedule_id}");
                }
                ScheduleCommand::Enable { schedule_id } => {
                    let schedule = db
                        .set_schedule_status(schedule_id, ScheduleStatus::Enabled)
                        .await?;
                    db.add_system_event(
                        "schedule_enabled",
                        json!({ "schedule_id": schedule.id, "name": schedule.name }),
                    )
                    .await?;
                    println!("Enabled schedule {schedule_id}");
                }
                ScheduleCommand::Disable { schedule_id } => {
                    let schedule = db
                        .set_schedule_status(schedule_id, ScheduleStatus::Disabled)
                        .await?;
                    db.add_system_event(
                        "schedule_disabled",
                        json!({ "schedule_id": schedule.id, "name": schedule.name }),
                    )
                    .await?;
                    println!("Disabled schedule {schedule_id}");
                }
                ScheduleCommand::Delete { schedule_id } => {
                    let schedule = db.get_schedule(schedule_id).await?;
                    db.delete_schedule(schedule_id).await?;
                    db.add_system_event(
                        "schedule_deleted",
                        json!({ "schedule_id": schedule_id, "name": schedule.name }),
                    )
                    .await?;
                    println!("Deleted schedule {schedule_id}");
                }
                ScheduleCommand::Tick => {
                    let report = scheduler::tick(&db, &config).await?;
                    println!(
                        "Scheduler tick: ran_schedules={}, heartbeat_missed={}",
                        report.ran_schedules, report.heartbeat_missed
                    );
                }
            }
        }
        Command::Config { command } => match command {
            ConfigCommand::Show => {
                println!("{}", toml::to_string_pretty(&config)?);
            }
            ConfigCommand::SetConcurrency {
                max_concurrent_jobs,
            } => {
                let mut config = config;
                config.set_worker_concurrency(max_concurrent_jobs);
                config.save()?;
                println!(
                    "Worker concurrency set to {} in {}",
                    config.worker.max_concurrent_jobs,
                    config.config_path.display()
                );
            }
            ConfigCommand::SetFallbacks { enabled } => {
                let mut config = config;
                config.routing.fallback_enabled = parse_bool_arg(&enabled)?;
                config.save()?;
                println!(
                    "Routing fallbacks enabled={} in {}",
                    config.routing.fallback_enabled,
                    config.config_path.display()
                );
            }
            ConfigCommand::SetFallbackOrder { providers } => {
                if providers.is_empty() {
                    anyhow::bail!("Pass at least one provider name");
                }
                for provider in &providers {
                    router::parse_provider_kind(provider)?;
                }
                let mut config = config;
                config.routing.fallback_order = providers;
                config.save()?;
                println!(
                    "Routing fallback order set to {} in {}",
                    config.routing.fallback_order.join(", "),
                    config.config_path.display()
                );
            }
            ConfigCommand::SetBudget {
                enabled,
                daily_total_usd,
                daily_provider_usd,
                daily_project_usd,
            } => {
                let mut config = config;
                config.budget.enabled = parse_bool_arg(&enabled)?;
                if daily_total_usd.is_some() {
                    config.budget.daily_total_usd = daily_total_usd;
                }
                if daily_provider_usd.is_some() {
                    config.budget.daily_provider_usd = daily_provider_usd;
                }
                if daily_project_usd.is_some() {
                    config.budget.daily_project_usd = daily_project_usd;
                }
                config.save()?;
                println!(
                    "Budget guardrails enabled={} total={:?} provider={:?} project={:?} in {}",
                    config.budget.enabled,
                    config.budget.daily_total_usd,
                    config.budget.daily_provider_usd,
                    config.budget.daily_project_usd,
                    config.config_path.display()
                );
            }
            ConfigCommand::SetCodexHome { path } => {
                let mut config = config;
                std::fs::create_dir_all(&path)?;
                config.codex.host_home = Some(path.canonicalize()?);
                config.save()?;
                println!(
                    "Codex host home set to {}",
                    optional_path(&config.codex.host_home)
                );
            }
            ConfigCommand::SetCodexMount { enabled, read_only } => {
                let mut config = config;
                config.codex.mount_host_home = enabled;
                if let Some(read_only) = read_only {
                    config.codex.mount_read_only = read_only;
                }
                config.save()?;
                println!(
                    "Codex mount enabled={} read_only={}",
                    config.codex.mount_host_home, config.codex.mount_read_only
                );
            }
        },
        Command::Secrets { command } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            let vault = SecretVault::new(config.clone());
            match command {
                SecretsCommand::Status => {
                    let status = vault.encryption_status();
                    println!("Secret encryption: {}", status.scheme);
                    println!("Encrypted at rest: {}", status.encrypted_at_rest);
                    println!("{}", status.note);
                }
                SecretsCommand::Set {
                    name,
                    provider,
                    kind,
                    value,
                } => {
                    let value = match value {
                        Some(value) => value,
                        None => std::env::var("LIBRARIAN_SECRET_VALUE")
                            .or_else(|_| std::env::var(&name))
                            .map_err(|_| {
                                anyhow::anyhow!(
                                    "Pass --value or set LIBRARIAN_SECRET_VALUE for this command"
                                )
                            })?,
                    };
                    let record = vault.store(&db, &name, &provider, &kind, &value).await?;
                    println!(
                        "Stored secret {} `{}` for provider {} using {}",
                        record.id, record.name, record.provider, record.encryption
                    );
                }
                SecretsCommand::List => {
                    for record in db.list_secret_records().await? {
                        println!(
                            "{}  {}  provider={}  kind={}  encryption={}  updated={}",
                            record.id,
                            record.name,
                            record.provider,
                            record.kind,
                            record.encryption,
                            record.updated_at.to_rfc3339()
                        );
                    }
                }
                SecretsCommand::Grant {
                    secret,
                    job_id,
                    provider,
                    capability,
                    ttl_seconds,
                    max_uses,
                } => {
                    let grant_id = vault
                        .grant(
                            &db,
                            &secret,
                            job_id,
                            provider.as_deref(),
                            &capability,
                            ttl_seconds,
                            max_uses,
                        )
                        .await?;
                    println!("Grant: {grant_id}");
                    println!("Token: {}", secrets::encode_grant_token(grant_id));
                }
                SecretsCommand::Resolve {
                    token,
                    capability,
                    job_id,
                    reveal,
                } => {
                    let grant_id = secrets::decode_grant_token(&token)?;
                    let resolved = vault
                        .resolve_with_grant(&db, grant_id, &capability, job_id)
                        .await?;
                    println!(
                        "Resolved secret `{}` provider={} kind={}",
                        resolved.name, resolved.provider, resolved.kind
                    );
                    if reveal {
                        println!("{}", resolved.plaintext);
                    } else {
                        println!("Plaintext hidden. Pass --reveal only for local debugging.");
                    }
                }
                SecretsCommand::Grants { limit } => {
                    for grant in db.list_secret_grants(limit).await? {
                        println!(
                            "{}  secret={}  capability={}  uses={}/{}  expires={}",
                            grant.id,
                            grant.secret_id,
                            grant.capability,
                            grant.uses,
                            grant.max_uses,
                            grant.expires_at.to_rfc3339()
                        );
                    }
                }
                SecretsCommand::Audit { limit } => {
                    for event in db.list_secret_audit_events(limit).await? {
                        println!(
                            "{}  action={}  success={}  secret={}  grant={}  {}",
                            event.created_at.to_rfc3339(),
                            event.action,
                            event.success,
                            event.secret_id,
                            event
                                .grant_id
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            serde_json::to_string(&event.metadata)?
                        );
                    }
                }
            }
        }
        Command::Providers { command } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            match command {
                ProvidersCommand::Catalog => {
                    for model in router::model_catalog() {
                        println!(
                            "{}  model={}  input_cost={:?}  output_cost={:?}  hints={}",
                            model.provider,
                            model.model,
                            model.input_cost_per_million,
                            model.output_cost_per_million,
                            model.task_hints.join(",")
                        );
                    }
                }
                ProvidersCommand::Status => {
                    for state in db.list_provider_states().await? {
                        println!(
                            "{}  model={}  status={}  paused_until={}  reason={}",
                            state.provider,
                            state.model.unwrap_or_else(|| "-".to_string()),
                            state.status,
                            state
                                .paused_until
                                .map(|time| time.to_rfc3339())
                                .unwrap_or_else(|| "-".to_string()),
                            state.reason.unwrap_or_else(|| "-".to_string())
                        );
                    }
                }
                ProvidersCommand::Pause {
                    provider,
                    model,
                    seconds,
                    reason,
                } => {
                    let paused_until =
                        chrono::Utc::now() + chrono::Duration::seconds(seconds.max(1));
                    let state = db
                        .set_provider_pause(&provider, model.as_deref(), paused_until, &reason)
                        .await?;
                    db.add_system_event(
                        "provider_paused",
                        json!({
                            "provider": state.provider,
                            "model": state.model,
                            "paused_until": state.paused_until,
                            "reason": state.reason,
                        }),
                    )
                    .await?;
                    println!(
                        "Paused provider `{provider}` until {}",
                        paused_until.to_rfc3339()
                    );
                }
                ProvidersCommand::Resume { provider, model } => {
                    db.resume_provider(&provider, model.as_deref()).await?;
                    db.add_system_event(
                        "provider_resumed",
                        json!({ "provider": provider, "model": model }),
                    )
                    .await?;
                    println!("Resumed provider `{provider}`");
                }
            }
        }
        Command::Usage { command } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            match command {
                UsageCommand::List { limit } => {
                    for event in db.list_usage_observations(limit).await? {
                        println!(
                            "{}  provider={}  model={}  job={}  input={:?}  output={:?}  cost={:?}  limit={}  {}",
                            event.observed_at.to_rfc3339(),
                            event.provider,
                            event.model.unwrap_or_else(|| "-".to_string()),
                            event
                                .job_id
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            event.input_tokens,
                            event.output_tokens,
                            event.cost_usd,
                            event.limit_event,
                            serde_json::to_string(&event.metadata)?
                        );
                    }
                }
                UsageCommand::AddLimit {
                    provider,
                    model,
                    job_id,
                    reason,
                } => {
                    db.add_usage_observation(
                        &provider,
                        model.as_deref(),
                        job_id,
                        None,
                        None,
                        None,
                        true,
                        json!({ "source": "manual", "reason": reason }),
                    )
                    .await?;
                    let paused_until = chrono::Utc::now() + chrono::Duration::minutes(30);
                    db.set_provider_pause(
                        &provider,
                        model.as_deref(),
                        paused_until,
                        "manual limit observation",
                    )
                    .await?;
                    println!("Recorded limit event and paused `{provider}`");
                }
            }
        }
        Command::ThirdEye { command } => {
            config.ensure_layout()?;
            match command {
                ThirdEyeCommand::Status => {
                    let health = third_eye::health(&config).await?;
                    println!("Third Eye enabled: {}", config.third_eye.enabled);
                    println!("Base URL: {}", config.third_eye.base_url);
                    println!("Reachable: {}", health.reachable);
                    println!("API ok: {}", health.api_ok);
                    println!("API detail: {}", serde_json::to_string(&health.detail)?);
                    if let Some(summary) = third_eye::db_summary(&config).await? {
                        println!(
                            "DB: {} calls={} projects={} cost=${:.4}",
                            summary.db_path,
                            summary.api_calls,
                            summary.projects,
                            summary.total_cost_usd
                        );
                    }
                }
                ThirdEyeCommand::Providers => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&third_eye::providers(&config).await?)?
                    );
                }
                ThirdEyeCommand::Refresh { since, full } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &third_eye::refresh(&config, since.as_deref(), full).await?
                        )?
                    );
                }
                ThirdEyeCommand::DbSummary => match third_eye::db_summary(&config).await? {
                    Some(summary) => println!("{}", serde_json::to_string_pretty(&summary)?),
                    None => println!("third_eye.db_path is not configured"),
                },
                ThirdEyeCommand::Configure {
                    base_url,
                    db_path,
                    enabled,
                } => {
                    let mut config = config;
                    if let Some(base_url) = base_url {
                        config.third_eye.base_url = base_url;
                    }
                    if let Some(db_path) = db_path {
                        config.third_eye.db_path = Some(db_path);
                    }
                    if let Some(enabled) = enabled {
                        config.third_eye.enabled = enabled;
                    }
                    config.save()?;
                    println!("Saved Third Eye config in {}", config.config_path.display());
                }
            }
        }
        Command::Events { limit } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            for event in db.list_system_events(limit.max(1)).await? {
                println!(
                    "{}  {}  {}",
                    event.created_at.to_rfc3339(),
                    event.kind,
                    serde_json::to_string(&event.payload)?
                );
            }
        }
        Command::Scheduler { once } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            if once {
                let report = scheduler::tick(&db, &config).await?;
                println!(
                    "Scheduler tick: ran_schedules={}, heartbeat_missed={}",
                    report.ran_schedules, report.heartbeat_missed
                );
            } else {
                println!("Scheduler started. Press Ctrl+C to stop.");
                loop {
                    let report = scheduler::tick(&db, &config).await?;
                    if report.ran_schedules > 0 || report.heartbeat_missed > 0 {
                        println!(
                            "Scheduler tick: ran_schedules={}, heartbeat_missed={}",
                            report.ran_schedules, report.heartbeat_missed
                        );
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
        Command::Worker { once, concurrency } => {
            config.ensure_layout()?;
            let db = Database::connect(&config).await?;
            db.migrate().await?;
            let concurrency = concurrency
                .unwrap_or(config.worker.max_concurrent_jobs)
                .max(1);
            if once {
                let ran = worker::run_batch(config, db, concurrency).await?;
                if ran == 0 {
                    println!("No queued jobs.");
                } else {
                    println!("Ran {ran} job(s).");
                }
            } else {
                println!("Worker started with concurrency {concurrency}. Press Ctrl+C to stop.");
                loop {
                    let ran = worker::run_batch(config.clone(), db.clone(), concurrency).await?;
                    if ran == 0 {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

fn optional_path(path: &Option<PathBuf>) -> String {
    path.as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn parse_bool_arg(value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" | "enabled" => Ok(true),
        "false" | "no" | "off" | "0" | "disabled" => Ok(false),
        _ => anyhow::bail!("Expected true/false, yes/no, on/off, or 1/0"),
    }
}

fn runtime_display(config: &Config) -> String {
    std::iter::once(config.docker.runtime_command.as_str())
        .chain(config.docker.runtime_args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

fn default_runtime_command_arg() -> String {
    if cfg!(windows) {
        "podman".to_string()
    } else {
        "docker".to_string()
    }
}

fn print_runtime_smoke_plan(config: &Config, project: &str) {
    let binary = "librarian";
    let home = config.home.display();
    let project_slug = project
        .trim()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let project_slug = if project_slug.is_empty() {
        "LibrarianSmoke".to_string()
    } else {
        project_slug
    };
    println!("Librarian runtime smoke plan");
    println!();
    println!("One-command broad smoke:");
    println!("   {binary} --home \"{home}\" smoke all --provider codex");
    println!("   {binary} --home \"{home}\" smoke all --provider codex --run-agent");
    println!();
    println!("Focused MVP smoke:");
    println!("   {binary} --home \"{home}\" smoke mvp --provider codex --run-agent");
    println!();
    println!("Local tool smoke without provider calls:");
    println!("   {binary} --home \"{home}\" smoke tools");
    println!();
    println!("Provider health smoke without container launches:");
    println!("   {binary} --home \"{home}\" smoke providers");
    println!();
    println!("Safe preflight-only variant:");
    println!("   {binary} --home \"{home}\" smoke mvp --provider codex");
    println!();
    println!("Manual equivalent, if you need to inspect every step:");
    println!();
    println!("1. Verify setup:");
    println!("   {binary} --home \"{home}\" doctor");
    println!("   {binary} --home \"{home}\" runtime build-agent-image");
    println!();
    println!("2. Create a disposable project:");
    println!("   mkdir -p \"{home}/Projects/{project_slug}\"");
    println!("   {binary} --home \"{home}\" project add \"{home}/Projects/{project_slug}\" --name \"{project}\"");
    println!("   {binary} --home \"{home}\" project attach-library \"{project}\" \"projects/{project_slug}\"");
    println!();
    println!("3. Queue and run one explicit background agent job:");
    println!("   {binary} --home \"{home}\" run --project \"{project}\" --goal \"Reply with a one paragraph smoke-test summary and do not edit files\" --read-only");
    println!("   Codex jobs now use provider network by default; pass --allow-network only when a job needs broader network access.");
    println!("   {binary} --home \"{home}\" worker --once");
    println!();
    println!("4. Inspect results:");
    println!("   {binary} --home \"{home}\" jobs list");
    println!(
        "   {binary} --home \"{home}\" context \"smoke-test summary\" --project \"{project}\""
    );
    println!();
    println!("Admin UI during the smoke:");
    println!("   {binary} --home \"{home}\" admin --bind 0.0.0.0:17377");
}

async fn run_all_smoke(
    config: &Config,
    provider: ProviderKind,
    run_agent: bool,
    allow_network: bool,
    secret_grant_token: Option<&str>,
    secret_ref: Option<&str>,
    name: &str,
    require_providers_ready: bool,
) -> Result<()> {
    println!("Librarian full smoke");
    println!("  root: {}", config.home.display());
    println!("  provider: {}", router::provider_name(&provider));
    println!(
        "  mode: providers + context + tools + mvp{}",
        if run_agent {
            " + real agent run"
        } else {
            " preflight"
        }
    );
    println!();

    println!("== Provider diagnostics ==");
    run_provider_smoke(config, require_providers_ready).await?;
    println!();

    println!("== Context/tree memory ==");
    run_context_smoke(config, &format!("{name}Context")).await?;
    println!();

    println!("== Tools and approvals ==");
    run_tools_smoke(config, &format!("{name}Tools")).await?;
    println!();

    println!("== MVP provider flow ==");
    run_mvp_smoke(
        config,
        provider,
        run_agent,
        allow_network,
        secret_grant_token,
        secret_ref,
        name,
    )
    .await?;
    println!();
    println!("Full smoke passed.");
    Ok(())
}

async fn run_mvp_smoke(
    config: &Config,
    provider: ProviderKind,
    run_agent: bool,
    allow_network: bool,
    secret_grant_token: Option<&str>,
    secret_ref: Option<&str>,
    name: &str,
) -> Result<()> {
    config.ensure_layout()?;
    let db = Database::connect(config).await?;
    db.migrate().await?;

    let started_at = Utc::now();
    let run_slug = smoke_slug(name, started_at);
    let project_name = format!(
        "{}-{}",
        smoke_display_name(name),
        started_at.format("%Y%m%d%H%M%S")
    );
    let library_dir = format!("smoke/{run_slug}");
    let project_dir = format!("_smoke/{run_slug}");
    let project_path = config.home.join("Projects").join(&project_dir);

    println!("Librarian MVP smoke");
    println!("  root: {}", config.home.display());
    println!("  provider: {}", router::provider_name(&provider));
    println!(
        "  mode: {}",
        if run_agent {
            "preflight + real agent run"
        } else {
            "local checks + preflight"
        }
    );
    println!();

    println!("1. Exercising Library and Projects tool sandboxes...");
    library_tools::create_folder(config, library_tools::LibraryRoot::Library, &library_dir)?;
    library_tools::write_markdown(
        config,
        &format!("{library_dir}/overview.md"),
        "# Smoke project\n\nstatus: created\nline-to-replace: old\nline-to-cut: remove me\n",
    )?;
    library_tools::append_markdown(
        config,
        &format!("{library_dir}/overview.md"),
        "\nappend-check: ok\n",
    )?;
    let _slice =
        library_tools::read_markdown_lines(config, &format!("{library_dir}/overview.md"), 1, 2)?;
    let matches = library_tools::find_markdown(
        config,
        &format!("{library_dir}/overview.md"),
        "line-to-replace",
        5,
    )?;
    if matches.is_empty() {
        anyhow::bail!("Smoke markdown find did not return the expected marker");
    }
    library_tools::replace_first_markdown_match(
        config,
        &format!("{library_dir}/overview.md"),
        "line-to-replace",
        "line-to-replace: new\n",
    )?;
    library_tools::cut_first_markdown_match(
        config,
        &format!("{library_dir}/overview.md"),
        "line-to-cut",
    )?;
    library_tools::create_folder(config, library_tools::LibraryRoot::Projects, &project_dir)?;
    library_tools::create_empty_file(
        config,
        library_tools::LibraryRoot::Projects,
        &format!("{project_dir}/scratch.tmp"),
    )?;
    library_tools::move_path(
        config,
        library_tools::LibraryRoot::Projects,
        &format!("{project_dir}/scratch.tmp"),
        &format!("{project_dir}/scratch-renamed.tmp"),
    )?;
    library_tools::delete_path(
        config,
        library_tools::LibraryRoot::Projects,
        &format!("{project_dir}/scratch-renamed.tmp"),
        false,
    )?;
    println!("   OK: created, read, found, replaced, cut, moved, and deleted test files.");

    println!("2. Registering disposable project...");
    let project_path = project_path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", project_path.display()))?;
    let project = db.add_project(&project_name, &project_path).await?;
    let project = db
        .attach_project_library_path(project.id, Path::new(&library_dir))
        .await?;
    println!("   OK: {} ({})", project.name, project.id);

    println!("3. Adding searchable smoke memory...");
    let memory_item = db
        .add_memory_item(
            Some(project.id),
            None,
            MemoryKind::Fact,
            Some("mvp-smoke"),
            &format!("MVP smoke marker for {project_name}: tool sandbox and provider preflight were prepared."),
            Some("cli:smoke"),
            json!({
                "project": project.name,
                "scope": "smoke",
                "provider": router::provider_name(&provider),
            }),
        )
        .await?;
    memory::embed_item(&db, config, &memory_item).await?;
    println!("   OK: {}", memory_item.id);

    println!("4. Queueing provider job and running preflight...");
    let generated_secret_grant =
        resolve_smoke_secret_grant(config, &db, &provider, secret_grant_token, secret_ref).await?;
    let secret_grant_token = secret_grant_token.or(generated_secret_grant.as_deref());
    if let Some(token) = secret_grant_token {
        println!(
            "   OK: using secret grant {}",
            short_secret_token_for_display(token)
        );
    }
    let network_mode = router::default_network_mode_for_provider(
        &provider,
        allow_network,
        secret_grant_token.is_some(),
    );
    let goal = "Reply with one short MVP smoke-test summary. Do not edit files.";
    let job = db
        .create_job(
            project.id,
            provider.clone(),
            goal,
            MountMode::ReadOnly,
            network_mode,
            secret_grant_token,
        )
        .await?;
    let context_pack = memory::retrieve_context_with_config(
        &db,
        Some(config),
        memory::RetrievalRequest {
            query: goal.to_string(),
            project_id: Some(project.id),
            activity_id: None,
            limit: memory::default_hit_limit(),
        },
    )
    .await?;
    db.add_job_event(
        job.id,
        "context_pack",
        json!({
            "query": context_pack.query,
            "generated_at": context_pack.generated_at,
            "hits": context_pack.hits,
        }),
    )
    .await?;
    let report = worker::preflight_job(config.clone(), db.clone(), job.id).await?;
    println!(
        "   OK: job={} context_hits={} prompt_chars={} instruction_files={}",
        report.job_id,
        report.context_hits,
        report.prompt_chars,
        report.instruction_files.len()
    );

    if !run_agent {
        println!();
        println!("Smoke preflight passed.");
        println!(
            "To run the real provider call: {} smoke mvp --provider {} --run-agent",
            doctor_command_prefix(config),
            provider_arg_name(&provider)
        );
        println!(
            "Inspect events: {} jobs events {}",
            doctor_command_prefix(config),
            job.id
        );
        return Ok(());
    }

    println!("5. Running the selected provider in the agent container...");
    worker::run_job_by_id(config.clone(), db.clone(), job.id).await?;
    let job = db.get_job(job.id).await?;
    let events = db.list_job_events(job.id).await?;
    println!("   status: {:?}", job.status);
    if let Some(event) = events.iter().rev().find(|event| {
        matches!(
            event.kind.as_str(),
            "vault" | "failure_category" | "error" | "stderr" | "stdout"
        )
    }) {
        println!(
            "   last signal: {} {}",
            event.kind,
            serde_json::to_string(&event.payload)?
        );
    }
    println!(
        "   events: {} jobs events {}",
        doctor_command_prefix(config),
        job.id
    );
    if !matches!(job.status, JobStatus::Completed) {
        anyhow::bail!("MVP smoke agent run finished as {:?}", job.status);
    }
    println!();
    println!("Smoke passed.");
    Ok(())
}

async fn run_self_host_smoke(
    config: &Config,
    provider: ProviderKind,
    project_path: Option<&Path>,
    run_agent: bool,
    allow_network: bool,
    secret_grant_token: Option<&str>,
) -> Result<()> {
    config.ensure_layout()?;
    let db = Database::connect(config).await?;
    db.migrate().await?;

    let project_path = match project_path {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir()?,
    };
    let project_path = project_path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", project_path.display()))?;
    let cargo_toml = project_path.join("Cargo.toml");
    let cargo_manifest = fs::read_to_string(&cargo_toml).with_context(|| {
        format!(
            "Self-host smoke expects a Rust project at {}",
            cargo_toml.display()
        )
    })?;
    if !cargo_manifest.contains("name = \"librarian\"") {
        anyhow::bail!(
            "Self-host smoke expects the Librarian repository; {} does not declare package `librarian`",
            cargo_toml.display()
        );
    }
    let display_project_path = user_facing_path(&project_path);

    println!("Librarian self-host smoke");
    println!("  root: {}", config.home.display());
    println!("  project path: {}", display_project_path.display());
    println!("  provider: {}", provider_arg_name(&provider));
    println!(
        "  mode: {}",
        if run_agent {
            "preflight + real read-only agent run"
        } else {
            "preflight only"
        }
    );

    println!();
    println!("1. Registering Librarian as a managed project...");
    let existing = db.list_projects().await?.into_iter().find(|project| {
        project
            .path
            .canonicalize()
            .ok()
            .is_some_and(|path| path == project_path)
    });
    let project = match existing {
        Some(project) => project,
        None => db.add_project("Librarian Self Host", &project_path).await?,
    };
    let library_path = Path::new("projects/librarian-self-host");
    library_tools::create_folder(
        config,
        library_tools::LibraryRoot::Library,
        &library_path.to_string_lossy(),
    )?;
    let project = db
        .attach_project_library_path(project.id, library_path)
        .await?;
    let overview_path = "projects/librarian-self-host/overview.md";
    if library_tools::read_markdown(config, overview_path).is_err() {
        library_tools::write_markdown(
            config,
            overview_path,
            "# Librarian Self Host\n\nThis note anchors the Librarian repository as a managed project for supervised agent work.\n",
        )?;
    }
    println!(
        "   OK: project={} library={}",
        project.id,
        project
            .library_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string())
    );

    println!("2. Adding self-host readiness memory...");
    let marker = format!(
        "self-host smoke marker {}",
        Utc::now().format("%Y%m%d%H%M%S")
    );
    let memory_item = db
        .add_memory_item(
            Some(project.id),
            None,
            MemoryKind::Status,
            Some("self-host-readiness"),
            &format!("{marker}: Librarian repository is registered for supervised self-hosting preflight."),
            Some("cli:smoke-self-host"),
            json!({
                "scope": "self-host",
                "repository": project_path,
                "provider": provider_arg_name(&provider),
            }),
        )
        .await?;
    memory::embed_item(&db, config, &memory_item).await?;
    let context_pack = memory::retrieve_context_with_config(
        &db,
        Some(config),
        memory::RetrievalRequest {
            query: marker.clone(),
            project_id: Some(project.id),
            activity_id: None,
            limit: memory::default_hit_limit(),
        },
    )
    .await?;
    if !context_pack
        .hits
        .iter()
        .any(|hit| hit.item.id == memory_item.id)
    {
        anyhow::bail!("Self-host smoke did not retrieve its readiness memory marker");
    }
    println!(
        "   OK: memory={} context_hits={}",
        memory_item.id,
        context_pack.hits.len()
    );

    println!("3. Queueing read-only self-host agent job and running preflight...");
    let network_mode = router::default_network_mode_for_provider(
        &provider,
        allow_network,
        secret_grant_token.is_some(),
    );
    let goal = "Inspect this Librarian repository and reply with one concise self-hosting readiness summary. Do not edit files.";
    let job = db
        .create_job(
            project.id,
            provider.clone(),
            goal,
            MountMode::ReadOnly,
            network_mode,
            secret_grant_token,
        )
        .await?;
    db.add_job_event(
        job.id,
        "context_pack",
        json!({
            "query": context_pack.query,
            "generated_at": context_pack.generated_at,
            "hits": context_pack.hits,
        }),
    )
    .await?;
    let report = worker::preflight_job(config.clone(), db.clone(), job.id).await?;
    println!(
        "   OK: job={} prompt_chars={} instruction_files={}",
        report.job_id,
        report.prompt_chars,
        report.instruction_files.len()
    );
    println!(
        "   events: {} jobs events {}",
        doctor_command_prefix(config),
        job.id
    );

    println!("4. Recording self-host review snapshot...");
    let review = job_review::review_job_changes(&db, job.id, false).await?;
    println!(
        "   OK: has_changes={} recommendation={}",
        review
            .get("has_worktree_changes")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        review
            .get("recommendation")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
    );

    println!("5. Checking commit policy gate...");
    let gate = job_review::gate_job_git_action(&db, job.id, GitGateActionArg::Commit).await?;
    println!(
        "   OK: allowed={} blockers={}",
        gate.get("allowed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        gate.get("blockers")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            .unwrap_or(0)
    );
    let packet = job_review::build_job_review_packet(&db, job.id, false, None).await?;
    println!(
        "   OK: review packet next_step={}",
        packet
            .get("summary")
            .and_then(|summary| summary.get("next_step"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
    );

    if !run_agent {
        println!();
        println!("Self-host preflight passed.");
        println!(
            "To run the real read-only provider call: {} smoke self-host --provider {} --run-agent --project-path \"{}\"",
            doctor_command_prefix(config),
            provider_arg_name(&provider),
            display_project_path.display()
        );
        return Ok(());
    }

    println!("6. Running the selected provider in the agent container...");
    worker::run_job_by_id(config.clone(), db.clone(), job.id).await?;
    let job = db.get_job(job.id).await?;
    println!("   status: {:?}", job.status);
    if !matches!(job.status, JobStatus::Completed) {
        anyhow::bail!("Self-host agent run did not complete successfully");
    }
    println!("Self-host smoke passed.");
    Ok(())
}

async fn run_context_smoke(config: &Config, name: &str) -> Result<()> {
    config.ensure_layout()?;
    let db = Database::connect(config).await?;
    db.migrate().await?;

    let started_at = Utc::now();
    let slug = smoke_slug(name, started_at);
    let parent_library = format!("context-smoke/{slug}");
    let child_library = format!("{parent_library}/ChildProject");
    let parent_workspace = config.home.join("Projects").join("_smoke").join(&slug);
    let child_workspace = parent_workspace.join("ChildProject");

    println!("Librarian context smoke");
    println!("  root: {}", config.home.display());
    println!("  library context: {parent_library}");

    library_tools::create_folder(config, library_tools::LibraryRoot::Library, &child_library)?;
    fs::create_dir_all(&child_workspace)
        .with_context(|| format!("Failed to create {}", child_workspace.display()))?;
    let parent_workspace = parent_workspace
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", parent_workspace.display()))?;
    let child_workspace = child_workspace
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", child_workspace.display()))?;

    let parent = db
        .add_project(
            &format!("{} Parent", smoke_display_name(name)),
            &parent_workspace,
        )
        .await?;
    let parent = db
        .attach_project_library_path(parent.id, Path::new(&parent_library))
        .await?;
    let child = db
        .add_project(
            &format!("{} Child", smoke_display_name(name)),
            &child_workspace,
        )
        .await?;
    let child = db
        .attach_project_library_path(child.id, Path::new(&child_library))
        .await?;

    let child_marker = format!("context smoke child marker {slug}");
    let parent_marker = format!("context smoke parent marker {slug}");
    let child_memory = db
        .add_memory_item(
            Some(child.id),
            None,
            MemoryKind::Fact,
            Some("context-smoke"),
            &format!("{child_marker}: child memory must be visible from parent subtree context."),
            Some("cli:smoke-context"),
            json!({
                "context_path": child_library,
                "parent_context_path": parent_library,
                "scope": "context-smoke",
            }),
        )
        .await?;
    memory::embed_item(&db, config, &child_memory).await?;
    let parent_memory = db
        .add_memory_item(
            Some(parent.id),
            None,
            MemoryKind::Fact,
            Some("context-smoke"),
            &format!("{parent_marker}: parent memory must be visible from child ancestor context."),
            Some("cli:smoke-context"),
            json!({
                "context_path": parent_library,
                "child_context_path": child_library,
                "scope": "context-smoke",
            }),
        )
        .await?;
    memory::embed_item(&db, config, &parent_memory).await?;

    let descendants = db
        .list_projects()
        .await?
        .into_iter()
        .filter(|project| {
            let parent_path = Path::new(&parent_library);
            project
                .library_path
                .as_ref()
                .is_some_and(|path| path == parent_path || path.starts_with(parent_path))
        })
        .collect::<Vec<_>>();
    if descendants.len() < 2 {
        anyhow::bail!("Context smoke expected parent and child projects in subtree");
    }

    let mut subtree_found_child = false;
    for project in descendants {
        let pack = memory::retrieve_context_with_config(
            &db,
            Some(config),
            memory::RetrievalRequest {
                query: child_marker.clone(),
                project_id: Some(project.id),
                activity_id: None,
                limit: memory::default_hit_limit(),
            },
        )
        .await?;
        if pack.hits.iter().any(|hit| hit.item.id == child_memory.id) {
            subtree_found_child = true;
            break;
        }
    }
    if !subtree_found_child {
        anyhow::bail!("Context smoke did not retrieve child memory from parent subtree scan");
    }

    let parent_node_pack = memory::retrieve_context_with_config(
        &db,
        Some(config),
        memory::RetrievalRequest {
            query: child_marker.clone(),
            project_id: Some(parent.id),
            activity_id: None,
            limit: memory::default_hit_limit(),
        },
    )
    .await?;
    if parent_node_pack
        .hits
        .iter()
        .any(|hit| hit.item.id == child_memory.id)
    {
        anyhow::bail!("Context smoke node-only parent retrieval unexpectedly found child memory");
    }

    let ancestor_ids = [child.id, parent.id];
    let mut ancestor_found_parent = false;
    for project_id in ancestor_ids {
        let pack = memory::retrieve_context_with_config(
            &db,
            Some(config),
            memory::RetrievalRequest {
                query: parent_marker.clone(),
                project_id: Some(project_id),
                activity_id: None,
                limit: memory::default_hit_limit(),
            },
        )
        .await?;
        if pack.hits.iter().any(|hit| hit.item.id == parent_memory.id) {
            ancestor_found_parent = true;
            break;
        }
    }
    if !ancestor_found_parent {
        anyhow::bail!("Context smoke did not retrieve parent memory from child ancestor scan");
    }

    println!("   OK: parent project {} ({})", parent.name, parent.id);
    println!("   OK: child project {} ({})", child.name, child.id);
    println!(
        "   OK: subtree scope found child memory {}",
        child_memory.id
    );
    println!("   OK: node scope excluded child memory from parent-only retrieval");
    println!(
        "   OK: ancestor scope found parent memory {}",
        parent_memory.id
    );
    admin::run_dialogue_context_smoke(config, &slug).await?;
    println!("   OK: dialogue inference suggested and auto-selected a Library node");
    println!("Context smoke passed.");
    Ok(())
}

async fn run_tools_smoke(config: &Config, name: &str) -> Result<()> {
    config.ensure_layout()?;
    let db = Database::connect(config).await?;
    db.migrate().await?;

    let started_at = Utc::now();
    let slug = smoke_slug(name, started_at);
    let display_name = smoke_display_name(name);
    let library_dir = format!("tools-smoke/{slug}");
    let workspace_dir = format!("_smoke/tools/{slug}");
    let workspace_path = config.home.join("Projects").join(&workspace_dir);

    println!("Librarian tools smoke");
    println!("  root: {}", config.home.display());
    println!("  library: {library_dir}");

    println!("1. Exercising library Markdown tools...");
    library_tools::create_folder(config, library_tools::LibraryRoot::Library, &library_dir)?;
    let note_path = format!("{library_dir}/overview.md");
    library_tools::write_markdown(
        config,
        &note_path,
        "# Tools Smoke\n\nIntro line.\n\n## Editable\n\nold value\nremove by section\n\n## Keep\n\nstable marker\n",
    )?;
    let read = library_tools::read_markdown_lines(config, &note_path, 1, 6)?;
    if !read.content.contains("## Editable") {
        anyhow::bail!("Tools smoke could not read the editable section");
    }
    library_tools::replace_markdown_lines(config, &note_path, 3, 3, "Intro line updated.\n")?;
    library_tools::replace_first_markdown_match(config, &note_path, "old value", "new value\n")?;
    library_tools::replace_markdown_section(
        config,
        &note_path,
        "Editable",
        "new value\nsection replace marker\n",
    )?;
    library_tools::append_markdown(config, &note_path, "\n## Temporary\n\ncut me\n")?;
    library_tools::cut_markdown_section(config, &note_path, "Temporary")?;
    let matches = library_tools::find_markdown(config, &note_path, "section replace marker", 3)?;
    if matches.is_empty() {
        anyhow::bail!("Tools smoke did not find replaced section marker");
    }
    println!("   OK: read, line edit, find/replace, section replace, section cut.");

    println!("2. Exercising Projects sandbox tools...");
    library_tools::create_folder(config, library_tools::LibraryRoot::Projects, &workspace_dir)?;
    library_tools::create_empty_file(
        config,
        library_tools::LibraryRoot::Projects,
        &format!("{workspace_dir}/scratch.tmp"),
    )?;
    library_tools::move_path(
        config,
        library_tools::LibraryRoot::Projects,
        &format!("{workspace_dir}/scratch.tmp"),
        &format!("{workspace_dir}/scratch-renamed.tmp"),
    )?;
    library_tools::delete_path(
        config,
        library_tools::LibraryRoot::Projects,
        &format!("{workspace_dir}/scratch-renamed.tmp"),
        false,
    )?;
    println!("   OK: project file create, move, delete.");

    println!("3. Registering project context and memory...");
    let workspace_path = workspace_path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", workspace_path.display()))?;
    let project = db.add_project(&display_name, &workspace_path).await?;
    let project = db
        .attach_project_library_path(project.id, Path::new(&library_dir))
        .await?;
    let marker = format!("tools smoke marker {slug}");
    let memory_item = db
        .add_memory_item(
            Some(project.id),
            None,
            MemoryKind::Fact,
            Some("tools-smoke"),
            &format!("{marker}: library/project tools and approvals are operational."),
            Some("cli:smoke-tools"),
            json!({
                "scope": "tools-smoke",
                "context_path": library_dir,
            }),
        )
        .await?;
    memory::embed_item(&db, config, &memory_item).await?;
    let pack = memory::retrieve_context_with_config(
        &db,
        Some(config),
        memory::RetrievalRequest {
            query: marker.clone(),
            project_id: Some(project.id),
            activity_id: None,
            limit: memory::default_hit_limit(),
        },
    )
    .await?;
    if !pack.hits.iter().any(|hit| hit.item.id == memory_item.id) {
        anyhow::bail!("Tools smoke did not retrieve its memory marker");
    }
    println!("   OK: project={} memory={}", project.id, memory_item.id);

    println!("4. Exercising git review and approval gates...");
    job_review::run_project_command(&workspace_path, "git", &["init", "-b", "feature/smoke"])
        .await?;
    job_review::run_project_command(
        &workspace_path,
        "git",
        &["config", "user.email", "smoke@example.invalid"],
    )
    .await?;
    job_review::run_project_command(
        &workspace_path,
        "git",
        &["config", "user.name", "Librarian Smoke"],
    )
    .await?;
    fs::write(
        workspace_path.join("baseline.md"),
        "# Smoke Baseline\n\npushed baseline\n",
    )?;
    job_review::run_project_command(&workspace_path, "git", &["add", "-A"]).await?;
    let baseline_commit = job_review::run_project_command(
        &workspace_path,
        "git",
        &["commit", "-m", "Smoke baseline"],
    )
    .await?;
    if !baseline_commit.success {
        anyhow::bail!(
            "Tools smoke baseline git commit failed: {}",
            baseline_commit.stderr
        );
    }
    let remote_path = config
        .home
        .join("Projects")
        .join("_smoke")
        .join("remotes")
        .join(format!("{slug}.git"));
    let remote_path = if remote_path.is_absolute() {
        remote_path
    } else {
        std::env::current_dir()?.join(remote_path)
    };
    if let Some(parent) = remote_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let remote_arg = remote_path.to_string_lossy().to_string();
    job_review::run_project_command(&workspace_path, "git", &["init", "--bare", &remote_arg])
        .await?;
    job_review::run_project_command(
        &workspace_path,
        "git",
        &["remote", "add", "origin", &remote_arg],
    )
    .await?;
    let baseline_push = job_review::run_project_command(
        &workspace_path,
        "git",
        &["push", "-u", "origin", "feature/smoke"],
    )
    .await?;
    if !baseline_push.success {
        anyhow::bail!("Tools smoke baseline push failed: {}", baseline_push.stderr);
    }
    fs::write(
        workspace_path.join("git-review.md"),
        "# Git Review Smoke\n\npending change\n",
    )?;
    let git_job = db
        .create_job(
            project.id,
            ProviderKind::Codex,
            "git review and policy gate smoke",
            MountMode::ReadWrite,
            crate::domain::NetworkMode::Provider,
            None,
        )
        .await?;
    let review = job_review::review_job_changes(&db, git_job.id, false).await?;
    if !review
        .get("has_worktree_changes")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        anyhow::bail!("Tools smoke expected git review to detect worktree changes");
    }
    let gate = job_review::gate_job_git_action(&db, git_job.id, GitGateActionArg::Commit).await?;
    if !gate
        .get("allowed")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        anyhow::bail!("Tools smoke expected commit gate to allow feature branch changes");
    }
    let approval = job_review::propose_job_git_action(
        &db,
        git_job.id,
        GitGateActionArg::Commit,
        Some("Smoke git approval proposal"),
        None,
    )
    .await?;
    job_review::run_project_command(&workspace_path, "git", &["add", "-A"]).await?;
    let smoke_commit = job_review::run_project_command(
        &workspace_path,
        "git",
        &["commit", "-m", "Smoke reversible change"],
    )
    .await?;
    if !smoke_commit.success {
        anyhow::bail!("Tools smoke git commit failed: {}", smoke_commit.stderr);
    }
    let push_plan = job_review::plan_job_git_push(&db, git_job.id).await?;
    if !push_plan
        .get("allowed")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        || push_plan
            .get("ahead_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            == 0
    {
        anyhow::bail!("Tools smoke expected push plan to allow one outgoing smoke commit");
    }
    let smoke_sha =
        job_review::run_project_command(&workspace_path, "git", &["log", "-1", "--format=%H"])
            .await?;
    let revert_plan =
        job_review::plan_job_git_revert(&db, git_job.id, Some(&smoke_sha.stdout)).await?;
    if !revert_plan
        .get("allowed")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        anyhow::bail!("Tools smoke expected revert plan to allow clean feature branch revert");
    }
    let review_packet =
        job_review::build_job_review_packet(&db, git_job.id, false, Some(&smoke_sha.stdout))
            .await?;
    if review_packet
        .get("summary")
        .and_then(|summary| summary.get("push_allowed"))
        .and_then(serde_json::Value::as_bool)
        != Some(true)
        || review_packet
            .get("summary")
            .and_then(|summary| summary.get("revert_allowed"))
            .and_then(serde_json::Value::as_bool)
            != Some(true)
    {
        anyhow::bail!("Tools smoke expected review packet to allow push review and revert");
    }
    let revert_approval = job_review::propose_job_git_action(
        &db,
        git_job.id,
        GitGateActionArg::Revert,
        Some("Smoke git revert proposal"),
        Some(&smoke_sha.stdout),
    )
    .await?;
    println!(
        "   OK: review packet, policy gates, push plan, approval={}, and revert approval={} are functional.",
        approval.id, revert_approval.id
    );

    println!("5. Exercising legacy memory cleanup...");
    admin::run_memory_cleanup_smoke(config).await?;
    println!("   OK: legacy local responder cleanup is gated and executable.");

    println!("6. Exercising approval persistence...");
    let approval = db
        .create_tool_approval(
            "library",
            "write_markdown",
            json!({
                "path": format!("{library_dir}/approval.md"),
                "content": "# Approval smoke\n",
                "summary": "Tools smoke approval persistence check",
            }),
        )
        .await?;
    let rejected = db
        .update_tool_approval_status(approval.id, ToolApprovalStatus::Rejected)
        .await?;
    if !matches!(rejected.status, ToolApprovalStatus::Rejected) {
        anyhow::bail!("Tools smoke approval did not persist rejected status");
    }
    println!("   OK: approval={} rejected", rejected.id);
    admin::run_approval_ui_smoke(config).await?;
    println!("   OK: approval proposal returns chat UI card metadata");
    admin::run_agent_action_ui_smoke(config, &slug).await?;
    println!("   OK: agent launch returns chat action card metadata");

    println!("7. Exercising job cancel/retry lifecycle...");
    let lifecycle_job = db
        .create_job(
            project.id,
            ProviderKind::Codex,
            "cancel and retry lifecycle smoke",
            MountMode::ReadOnly,
            crate::domain::NetworkMode::Provider,
            None,
        )
        .await?;
    db.request_cancel_job(lifecycle_job.id).await?;
    let cancelled = db.get_job(lifecycle_job.id).await?;
    if !matches!(cancelled.status, JobStatus::Cancelled) || cancelled.cancel_requested_at.is_none()
    {
        anyhow::bail!("Tools smoke expected queued job cancellation to persist Cancelled status");
    }
    let cancel_events = db.list_job_events(lifecycle_job.id).await?;
    if !cancel_events
        .iter()
        .any(|event| event.kind == "cancel_requested")
    {
        anyhow::bail!("Tools smoke expected cancel_requested job event");
    }
    let retry = db.retry_job(lifecycle_job.id).await?;
    let retry_events = db.list_job_events(retry.id).await?;
    if !matches!(retry.status, JobStatus::Queued)
        || !retry_events.iter().any(|event| event.kind == "retry_of")
    {
        anyhow::bail!("Tools smoke expected retry job to remain queued with retry_of event");
    }
    println!(
        "   OK: cancelled job={} and queued retry={}",
        lifecycle_job.id, retry.id
    );

    println!("8. Exercising launch-context registration hints...");
    let projects = db.list_projects().await?;
    let known_context = launch_context_registration_check_for(config, &projects, &workspace_path);
    if known_context.severity != DoctorSeverity::Ok {
        anyhow::bail!("Tools smoke expected registered workspace launch context to be OK");
    }
    let unknown_context_dir = config
        .home
        .join("Projects")
        .join("_smoke")
        .join("launch-context")
        .join(&slug);
    fs::create_dir_all(&unknown_context_dir)?;
    let unknown_context =
        launch_context_registration_check_for(config, &projects, &unknown_context_dir);
    if unknown_context.severity != DoctorSeverity::Warn
        || unknown_context
            .next_step
            .as_deref()
            .is_none_or(|step| !step.contains("project add"))
    {
        anyhow::bail!(
            "Tools smoke expected unknown launch context to warn with project add next step"
        );
    }
    println!("   OK: known context is accepted; unknown context suggests registration.");

    println!("9. Exercising project slash workflow...");
    admin::run_project_slash_smoke(config, &slug).await?;
    println!("   OK: /project create/status/map/attach paths are functional.");

    println!("10. Exercising prompt default presets...");
    admin::run_prompt_defaults_smoke(config).await?;
    println!("   OK: prompt seed/update/delete/export flows are confirmed and renderable.");

    println!("Tools smoke passed.");
    Ok(())
}

async fn run_provider_smoke(config: &Config, require_ready: bool) -> Result<()> {
    config.ensure_layout()?;
    let db = Database::connect(config).await?;
    db.migrate().await?;
    let states = db.list_provider_states().await?;
    let diagnostics = provider_health::collect_provider_diagnostics(config, &states).await;

    println!("Librarian provider smoke");
    println!("  root: {}", config.home.display());
    println!(
        "  mode: {}",
        if require_ready {
            "require all providers ready"
        } else {
            "report only"
        }
    );
    println!();

    let mut not_ready = Vec::new();
    for diagnostic in &diagnostics {
        println!(
            "[{}] {}: {}",
            diagnostic.level.to_ascii_uppercase(),
            diagnostic.provider,
            diagnostic.status
        );
        println!("     {}", diagnostic.detail);
        if let Some(next_step) = &diagnostic.next_step {
            println!("     next: {next_step}");
        }
        if diagnostic.level != "ok" {
            not_ready.push(diagnostic.provider.clone());
        }
    }

    println!();
    println!("Broker proxy policy:");
    let proxy_routes = broker::provider_proxy_policy_routes();
    for route in &proxy_routes {
        if !broker::provider_proxy_policy_allows(route.provider, route.method, route.path) {
            anyhow::bail!(
                "Provider proxy policy route is listed but not allowed: {} {} /{}",
                route.provider,
                route.method,
                route.path
            );
        }
        println!("[OK] {} {} /{}", route.provider, route.method, route.path);
    }
    for (provider, method, path) in [
        ("openrouter", "GET", "api/v1/chat/completions"),
        ("openrouter", "POST", "api/v1/credits"),
        ("openrouter", "POST", "api/v1/../secrets"),
    ] {
        if broker::provider_proxy_policy_allows(provider, method, path) {
            anyhow::bail!("Provider proxy policy unexpectedly allowed {provider} {method} /{path}");
        }
    }
    println!("[OK] denied unsafe OpenRouter probe paths before grant use");

    if require_ready && !not_ready.is_empty() {
        anyhow::bail!(
            "Provider smoke requires ready providers; not ready: {}",
            not_ready.join(", ")
        );
    }
    println!();
    println!("Provider smoke passed.");
    Ok(())
}

async fn resolve_smoke_secret_grant(
    config: &Config,
    db: &Database,
    provider: &ProviderKind,
    explicit_token: Option<&str>,
    secret_ref: Option<&str>,
) -> Result<Option<String>> {
    if explicit_token.is_some() {
        return Ok(None);
    }
    let provider_name = router::provider_name(provider);
    let secret_ref = match secret_ref {
        Some(secret_ref) => Some(secret_ref.to_string()),
        None if matches!(provider, ProviderKind::OpenRouter) => {
            let records = db
                .list_secret_records()
                .await?
                .into_iter()
                .filter(|record| record.provider == "openrouter")
                .collect::<Vec<_>>();
            if records.len() == 1 {
                Some(records[0].id.to_string())
            } else {
                None
            }
        }
        None => None,
    };
    let Some(secret_ref) = secret_ref else {
        return Ok(None);
    };
    let vault = SecretVault::new(config.clone());
    let grant_id = vault
        .grant(db, &secret_ref, None, Some(provider_name), "read", 900, 1)
        .await?;
    Ok(Some(secrets::encode_grant_token(grant_id)))
}

fn short_secret_token_for_display(token: &str) -> String {
    let chars = token.chars().collect::<Vec<_>>();
    if chars.len() <= 12 {
        return token.to_string();
    }
    format!(
        "{}...{}",
        chars.iter().take(6).collect::<String>(),
        chars
            .iter()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>()
    )
}

fn smoke_display_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "LibrarianSmoke".to_string()
    } else {
        trimmed.to_string()
    }
}

fn smoke_slug(name: &str, timestamp: chrono::DateTime<Utc>) -> String {
    let mut slug = name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        slug = "librarian-smoke".to_string();
    }
    format!("{}-{}", slug, timestamp.format("%Y%m%d%H%M%S"))
}

fn provider_arg_name(provider: &ProviderKind) -> &'static str {
    match provider {
        ProviderKind::Codex => "codex",
        ProviderKind::OpenRouter => "open-router",
        ProviderKind::ClaudeCode => "claude-code",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DoctorSeverity {
    Ok,
    Warn,
    Error,
}

impl DoctorSeverity {
    fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Self::Ok => "32",
            Self::Warn => "33",
            Self::Error => "31",
        }
    }
}

#[derive(Clone, Debug)]
struct DoctorCheck {
    severity: DoctorSeverity,
    label: String,
    detail: String,
    next_step: Option<String>,
}

impl DoctorCheck {
    fn ok(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: DoctorSeverity::Ok,
            label: label.into(),
            detail: detail.into(),
            next_step: None,
        }
    }

    fn warn(
        label: impl Into<String>,
        detail: impl Into<String>,
        next_step: impl Into<String>,
    ) -> Self {
        Self {
            severity: DoctorSeverity::Warn,
            label: label.into(),
            detail: detail.into(),
            next_step: Some(next_step.into()),
        }
    }

    fn error(
        label: impl Into<String>,
        detail: impl Into<String>,
        next_step: impl Into<String>,
    ) -> Self {
        Self {
            severity: DoctorSeverity::Error,
            label: label.into(),
            detail: detail.into(),
            next_step: Some(next_step.into()),
        }
    }
}

impl From<provider_health::ProviderDiagnostic> for DoctorCheck {
    fn from(diagnostic: provider_health::ProviderDiagnostic) -> Self {
        let severity = match diagnostic.level {
            "ok" => DoctorSeverity::Ok,
            "error" => DoctorSeverity::Error,
            _ => DoctorSeverity::Warn,
        };
        Self {
            severity,
            label: format!("provider {}", diagnostic.provider),
            detail: format!("{}: {}", diagnostic.status, diagnostic.detail),
            next_step: diagnostic.next_step,
        }
    }
}

async fn run_doctor(config: &Config) -> Result<()> {
    config.ensure_layout()?;

    let mut checks = vec![
        DoctorCheck::ok("librarian version", version_detail(config)),
        DoctorCheck::ok("librarian root (state)", config.home.display().to_string()),
        DoctorCheck::ok(
            "launch context (cwd)",
            format!(
                "{}; used as the current project hint, not as storage",
                std::env::current_dir()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|error| format!("unavailable: {error}"))
            ),
        ),
        path_check("config file", &config.config_path, "Run `librarian init`."),
        path_check(
            "knowledge base",
            &config.vault_path,
            "Run `librarian init`.",
        ),
        DoctorCheck::ok("runtime command", runtime_display(config)),
        DoctorCheck::ok("agent image setting", config.docker.agent_image.clone()),
        DoctorCheck::ok("mount path style", config.docker.mount_path_style.clone()),
        DoctorCheck::ok(
            "routing fallback",
            format!(
                "enabled={} order={}",
                config.routing.fallback_enabled,
                config.routing.fallback_order.join(", ")
            ),
        ),
        DoctorCheck::ok(
            "budget guardrails",
            format!(
                "enabled={} total={:?} provider={:?} project={:?}",
                config.budget.enabled,
                config.budget.daily_total_usd,
                config.budget.daily_provider_usd,
                config.budget.daily_project_usd
            ),
        ),
    ];

    let mut provider_states = Vec::new();
    let sqlite_check = match Database::connect(config).await {
        Ok(db) => match db.migrate().await {
            Ok(()) => {
                provider_states = db.list_provider_states().await.unwrap_or_default();
                checks.push(launch_context_registration_check(
                    config,
                    &db.list_projects().await?,
                ));
                DoctorCheck::ok(
                    "sqlite",
                    format!("opened and migrated {}", config.database_path.display()),
                )
            }
            Err(error) => DoctorCheck::error(
                "sqlite",
                format!("opened but migration failed: {error}"),
                "Check database file permissions, then rerun `librarian init`.",
            ),
        },
        Err(error) => DoctorCheck::error(
            "sqlite",
            format!("could not open {}: {error}", config.database_path.display()),
            "Check LIBRARIAN_HOME and file permissions, then rerun `librarian init`.",
        ),
    };
    checks.push(sqlite_check);

    checks.push(runtime_check("container runtime", config, &["--version"]).await);
    checks.push(runtime_check("runtime engine", config, &["info"]).await);
    checks.push(
        runtime_check(
            "agent image",
            config,
            &["image", "inspect", &config.docker.agent_image],
        )
        .await,
    );
    checks.extend(
        provider_health::collect_provider_diagnostics(config, &provider_states)
            .await
            .into_iter()
            .map(DoctorCheck::from),
    );

    let overall = if checks
        .iter()
        .any(|check| check.severity == DoctorSeverity::Error)
    {
        "blocked"
    } else if checks
        .iter()
        .any(|check| check.severity == DoctorSeverity::Warn)
    {
        "degraded"
    } else {
        "ready"
    };

    let color_enabled = std::env::var_os("NO_COLOR").is_none();
    let title = format!("Librarian doctor: {}", overall.to_ascii_uppercase());
    println!("{}", border_line(title.len()));
    println!(
        "| {} |",
        color_text(color_enabled, overall_color(overall), &title)
    );
    println!("{}", border_line(title.len()));
    println!();
    for check in &checks {
        let tag = color_text(
            color_enabled,
            check.severity.color(),
            &format!("[{}]", check.severity.label()),
        );
        println!(
            "{} {}: {}",
            tag,
            bold_text(color_enabled, &check.label),
            check.detail
        );
        if let Some(next_step) = &check.next_step {
            println!("       next: {next_step}");
        }
    }
    println!();
    print_doctor_next_steps(color_enabled, &checks, config);

    Ok(())
}

fn launch_context_registration_check(config: &Config, projects: &[Project]) -> DoctorCheck {
    match std::env::current_dir() {
        Ok(path) => launch_context_registration_check_for(config, projects, &path),
        Err(error) => DoctorCheck::warn(
            "launch context registration",
            format!("could not inspect current directory: {error}"),
            "Run Librarian from the directory you want to use as context, or pass an explicit project path.",
        ),
    }
}

fn launch_context_registration_check_for(
    config: &Config,
    projects: &[Project],
    cwd: &Path,
) -> DoctorCheck {
    let cwd = normalized_existing_path(cwd);
    let home = normalized_existing_path(&config.home);
    let app = home.join(".app");
    let cfg = home.join(".cfg");
    let mdb = home.join(".mdb");
    let library = normalized_existing_path(&config.vault_path);
    let projects_root = home.join("Projects");

    if path_eq_or_within(&cwd, &home) && !path_eq_or_within(&cwd, &projects_root)
        || paths_equivalent(&cwd, &projects_root)
    {
        let display_cwd = user_facing_path(&cwd);
        return DoctorCheck::ok(
            "launch context registration",
            format!(
                "{} is inside Librarian root/internal storage",
                display_cwd.display()
            ),
        );
    }
    if path_eq_or_within(&cwd, &app)
        || path_eq_or_within(&cwd, &cfg)
        || path_eq_or_within(&cwd, &mdb)
        || path_eq_or_within(&cwd, &library)
    {
        let display_cwd = user_facing_path(&cwd);
        return DoctorCheck::ok(
            "launch context registration",
            format!(
                "{} is Librarian-managed context, not a new workspace",
                display_cwd.display()
            ),
        );
    }
    if let Some(project) = projects
        .iter()
        .find(|project| path_eq_or_within(&cwd, &normalized_existing_path(&project.path)))
    {
        let display_cwd = user_facing_path(&cwd);
        return DoctorCheck::ok(
            "launch context registration",
            format!(
                "{} is already covered by project `{}`",
                display_cwd.display(),
                project.name
            ),
        );
    }

    let name = cwd
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Imported Project");
    let display_cwd = user_facing_path(&cwd);
    DoctorCheck::warn(
        "launch context registration",
        format!(
            "{} is not registered as a Librarian project",
            display_cwd.display()
        ),
        format!(
            "Register it when you want agents to use this folder: {} project add {} --name {}",
            doctor_command_prefix(config),
            shell_path(&display_cwd),
            shell_word(&humanize_project_name(name))
        ),
    )
}

fn normalized_existing_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn user_facing_path(path: &Path) -> PathBuf {
    let text = path.display().to_string();
    if let Some(stripped) = text.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

fn path_eq_or_within(path: &Path, parent: &Path) -> bool {
    paths_equivalent(path, parent) || path.starts_with(parent)
}

fn humanize_project_name(name: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = true;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if previous_was_separator && !output.is_empty() {
                output.push(' ');
            }
            output.push(character);
            previous_was_separator = false;
        } else {
            previous_was_separator = true;
        }
    }
    if output.trim().is_empty() {
        "Imported Project".to_string()
    } else {
        output
    }
}

fn print_doctor_next_steps(color_enabled: bool, checks: &[DoctorCheck], config: &Config) {
    let command = doctor_command_prefix(config);
    let blockers = checks
        .iter()
        .filter(|check| check.severity == DoctorSeverity::Error)
        .collect::<Vec<_>>();
    let warnings = checks
        .iter()
        .filter(|check| check.severity == DoctorSeverity::Warn)
        .collect::<Vec<_>>();
    if blockers.is_empty() && warnings.is_empty() {
        println!(
            "{}",
            color_text(
                color_enabled,
                "32",
                "Next step: run the one-command smoke test, then open the admin UI."
            )
        );
        println!("  {command} smoke mvp --provider codex --run-agent");
        println!("  {command} smoke context");
        println!("  {command} smoke tools");
        println!("  {command} smoke providers");
        println!("  {command} doctor --smoke");
        println!("  {command} admin");
        println!();
        println!("Upgrade:");
        println!("  {command} upgrade");
        return;
    }

    if blockers.is_empty() {
        println!(
            "{}",
            color_text(color_enabled, "33", "Next important step:")
        );
        if let Some(first) = warnings.first() {
            println!(
                "  Fix `{}`: {}",
                first.label,
                first
                    .next_step
                    .as_deref()
                    .unwrap_or("See the warning above.")
            );
        }
        println!();
        println!("Then:");
        println!("  {command} doctor");
        println!("  {command} admin");
        println!();
        println!("Upgrade:");
        println!("  {command} upgrade");
        return;
    }

    println!(
        "{}",
        color_text(color_enabled, "31", "Next important step:")
    );
    if let Some(first) = blockers.first() {
        println!(
            "  Fix `{}` first: {}",
            first.label,
            first
                .next_step
                .as_deref()
                .unwrap_or("See the failed check above.")
        );
    }
    if blockers.len() > 1 {
        println!();
        println!("Remaining blockers:");
        for blocker in blockers.iter().skip(1) {
            println!("  - {}: {}", blocker.label, blocker.detail);
        }
    }
    println!();
    println!("After blockers are fixed:");
    println!("  {command} doctor");
    println!("  {command} admin");
    println!();
    println!("Upgrade:");
    println!("  {command} upgrade");
}

fn doctor_command_prefix(config: &Config) -> String {
    if installed_app_binary(config) {
        return format!("librarian --home {}", shell_path(&config.home));
    }
    let executable = std::env::current_exe()
        .ok()
        .map(|path| shell_path(&path))
        .unwrap_or_else(|| "librarian".to_string());
    format!("{executable} --home {}", shell_path(&config.home))
}

fn installed_app_binary(config: &Config) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let expected = config.home.join(".app").join("bin").join(if cfg!(windows) {
        "librarian.exe"
    } else {
        "librarian"
    });
    paths_equivalent(&exe, &expected)
}

fn version_detail(config: &Config) -> String {
    let running = env!("CARGO_PKG_VERSION");
    let metadata_path = config.home.join(".app").join("version.json");
    let Ok(text) = fs::read_to_string(&metadata_path) else {
        return format!(
            "running={running}; install metadata missing at {}",
            metadata_path.display()
        );
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return format!(
            "running={running}; install metadata is not valid JSON at {}",
            metadata_path.display()
        );
    };
    let installed = value
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let git_ref = value
        .get("git_ref")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let commit = value
        .get("git_commit")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let installed_at = value
        .get("installed_at")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    format!("running={running}; installed={installed}; ref={git_ref}; commit={commit}; installed_at={installed_at}")
}

fn paths_equivalent(left: &std::path::Path, right: &std::path::Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn shell_path(path: &std::path::Path) -> String {
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

fn shell_word(text: &str) -> String {
    if text
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/'))
    {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

fn border_line(title_len: usize) -> String {
    format!("+{}+", "-".repeat(title_len + 2))
}

fn color_text(enabled: bool, color: &str, text: &str) -> String {
    if enabled {
        format!("\x1b[{color}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn bold_text(enabled: bool, text: &str) -> String {
    if enabled {
        format!("\x1b[1m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn overall_color(overall: &str) -> &'static str {
    match overall {
        "ready" => "32",
        "degraded" => "33",
        _ => "31",
    }
}

fn path_check(label: &str, path: &std::path::Path, next_step: &str) -> DoctorCheck {
    if path.exists() {
        DoctorCheck::ok(label, path.display().to_string())
    } else {
        DoctorCheck::warn(label, format!("missing {}", path.display()), next_step)
    }
}

async fn runtime_check(label: &str, config: &Config, args: &[&str]) -> DoctorCheck {
    let mut all_args = config.docker.runtime_args.clone();
    all_args.extend(args.iter().map(|arg| arg.to_string()));
    command_check_owned(label, &config.docker.runtime_command, &all_args).await
}

async fn command_check_owned(label: &str, program: &str, args: &[String]) -> DoctorCheck {
    let output = TokioCommand::new(program).args(args).output().await;
    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let first_line = text.lines().next().unwrap_or("ok");
            DoctorCheck::ok(label, first_line.to_string())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let first_line = stderr.lines().next().unwrap_or("command failed");
            let detail = format!("command failed: {first_line}");
            DoctorCheck::error(label, &detail, command_next_step(label, &detail))
        }
        Err(error) => {
            let detail = format!("unavailable: {error}");
            DoctorCheck::error(label, &detail, command_next_step(label, &detail))
        }
    }
}

fn command_next_step(label: &str, detail: &str) -> &'static str {
    let detail = detail.to_ascii_lowercase();
    if detail.contains("cannot connect to podman") || detail.contains("podman machine") {
        return "Start/fix Podman, or run `librarian runtime use-wsl-podman` if the WSL Podman machine is usable.";
    }
    if detail.contains("permission denied") && detail.contains("docker") {
        return "Your user cannot access Docker yet. Open a new Ubuntu shell, or run the command through `sg docker -c 'librarian runtime build-agent-image'`.";
    }
    match label {
        "container runtime" => {
            "Install/start Docker or Podman, or run `librarian runtime use-wsl-podman` on Windows."
        }
        "runtime engine" => {
            "Start the Docker/Podman engine and verify `docker info` or `podman info` works."
        }
        "agent image" => "Run `librarian runtime build-agent-image`.",
        "host codex" => "Install Codex CLI on the host; use Librarian's local CODEX_HOME when signing in for portability.",
        "host claude" => "Install Claude Code on the host and sign in before enabling containerized Claude jobs.",
        _ => "Check that the command is installed and available in PATH.",
    }
}
