mod admin;
mod broker;
mod config;
mod db;
mod docker_runner;
mod domain;
mod gates;
mod memory;
mod prompt;
mod providers;
mod router;
mod scheduler;
mod secrets;
mod third_eye;
mod vault;
mod worker;

use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use config::Config;
use db::Database;
use docker_runner::DockerRunner;
use domain::{
    JobStatus, MemoryKind, MountMode, NetworkMode, ProviderKind, ScheduleKind, ScheduleStatus,
};
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
    Doctor,
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
    },
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    List,
}

#[derive(Debug, Subcommand)]
enum JobsCommand {
    List,
    Events { job_id: uuid::Uuid },
    Preflight { job_id: uuid::Uuid },
    Cancel { job_id: uuid::Uuid },
    Retry { job_id: uuid::Uuid },
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

#[derive(Clone, Debug, ValueEnum)]
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

    println!("Librarian needs a root directory for config, SQLite, vault, run artifacts, and portable agent profiles.");
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
        build_agent_image_with_config(&config, false).await?;
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

async fn build_agent_image_with_config(config: &Config, no_codex: bool) -> Result<()> {
    let install_codex = if no_codex { "false" } else { "true" };
    println!(
        "Building {} from Dockerfile.agent (INSTALL_CODEX={install_codex})",
        config.docker.agent_image
    );
    let build_arg = format!("INSTALL_CODEX={install_codex}");
    let mut args = config.docker.runtime_args.clone();
    args.extend(
        [
            "build",
            "-t",
            &config.docker.agent_image,
            "--build-arg",
            &build_arg,
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
        Command::Doctor => {
            run_doctor(&config).await?;
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
            RuntimeCommand::BuildAgentImage { no_codex } => {
                build_agent_image_with_config(&config, no_codex).await?;
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
                        println!(
                            "{}  {}  {}",
                            project.id,
                            project.name,
                            project.path.display()
                        );
                    }
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
            let network_mode = if allow_network || secret_grant_token.is_some() {
                NetworkMode::Open
            } else {
                NetworkMode::None
            };
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
                    provider.into(),
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
            let spec = domain::AgentRunSpec {
                job_id: job.id,
                project_path: project.path.clone(),
                provider: job.provider,
                goal: job.goal,
                prompt: prompt::build_agent_prompt(&project, &gated.content, &context_pack),
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
                    println!("{}", prompt::build_agent_prompt(&project, &query, &pack));
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
        path_check("vault", &config.vault_path, "Run `librarian init`."),
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

    checks.push(match Database::connect(config).await {
        Ok(db) => match db.migrate().await {
            Ok(()) => DoctorCheck::ok(
                "sqlite",
                format!("opened and migrated {}", config.database_path.display()),
            ),
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
    });

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
    checks.push(host_codex_check().await);
    checks.push(codex_profile_check(config));

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
                "Next step: start the admin UI, add a project, then queue a smoke job."
            )
        );
        println!("  {command} admin");
        println!("  {command} project add <path>");
        println!("  {command} worker --once");
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

fn codex_profile_check(config: &Config) -> DoctorCheck {
    let Some(path) = &config.codex.host_home else {
        return DoctorCheck::warn(
            "codex profile",
            "not configured",
            "Run Codex with CODEX_HOME set to Librarian's codex-home, then enable the container mount.",
        );
    };
    if !path.exists() {
        return DoctorCheck::error(
            "codex profile",
            format!("missing {}", path.display()),
            "Run `CODEX_HOME=<this path> codex`, complete sign-in, then enable the container mount.",
        );
    }
    if !config.codex.mount_host_home {
        return DoctorCheck::warn(
            "codex profile",
            format!("present at {}, container mount disabled", path.display()),
            "Run `librarian auth codex --enable-container-mount --codex-home <this path>` before containerized Codex runs.",
        );
    }
    if codex_profile_has_auth_artifacts(path) {
        return DoctorCheck::ok(
            "codex profile",
            format!("present at {}, mount enabled", path.display()),
        );
    }
    DoctorCheck::warn(
        "codex profile",
        format!(
            "present at {}, but no common auth/config file found at top level",
            path.display()
        ),
        "Run `codex` with CODEX_HOME set to this path, complete sign-in, then rerun `librarian doctor`.",
    )
}

fn codex_profile_has_auth_artifacts(path: &std::path::Path) -> bool {
    let names = ["auth.json", "config.toml", "credentials.json"];
    if names.iter().any(|name| path.join(name).exists()) {
        return true;
    }
    has_named_file_within(path, &names, 3)
}

fn has_named_file_within(path: &std::path::Path, names: &[&str], depth: usize) -> bool {
    if depth == 0 {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_file()
            && entry_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| names.iter().any(|candidate| candidate == &name))
        {
            return true;
        }
        if entry_path.is_dir() && has_named_file_within(&entry_path, names, depth - 1) {
            return true;
        }
    }
    false
}

async fn host_codex_check() -> DoctorCheck {
    if cfg!(windows) {
        command_check("host codex", "where.exe", &["codex"]).await
    } else {
        command_check("host codex", "sh", &["-lc", "command -v codex"]).await
    }
}

async fn runtime_check(label: &str, config: &Config, args: &[&str]) -> DoctorCheck {
    let mut all_args = config.docker.runtime_args.clone();
    all_args.extend(args.iter().map(|arg| arg.to_string()));
    command_check_owned(label, &config.docker.runtime_command, &all_args).await
}

async fn command_check(label: &str, program: &str, args: &[&str]) -> DoctorCheck {
    command_check_owned(
        label,
        program,
        &args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>(),
    )
    .await
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
        _ => "Check that the command is installed and available in PATH.",
    }
}
