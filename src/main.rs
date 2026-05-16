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

use std::path::PathBuf;

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
#[command(about = "Local-first harness for containerized coding agents")]
struct Cli {
    #[arg(long, env = "LIBRARIAN_HOME")]
    home: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init,
    Doctor,
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
    let config = Config::load_or_default(cli.home)?;

    match cli.command {
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
            config.ensure_layout()?;
            println!("Librarian home: {}", config.home.display());
            println!("Runtime command: {}", runtime_display(&config));
            println!("Agent image: {}", config.docker.agent_image);
            println!("Mount path style: {}", config.docker.mount_path_style);
            println!(
                "Codex host home: {}",
                optional_path(&config.codex.host_home)
            );
            println!("Codex mount enabled: {}", config.codex.mount_host_home);
            println!("Codex mount read-only: {}", config.codex.mount_read_only);
            println!("Codex container home: {}", config.codex.container_home);
            print_runtime_check("container runtime", &config, &["--version"]).await;
            print_runtime_check(
                "agent image",
                &config,
                &["image", "inspect", &config.docker.agent_image],
            )
            .await;
            print_command_check("host codex", "codex", &["--version"]).await;
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
                config.docker.runtime_command = "wsl.exe".to_string();
                config.docker.runtime_args = vec![
                    "-d".to_string(),
                    distro,
                    "--".to_string(),
                    "podman".to_string(),
                ];
                config.docker.mount_path_style = "wsl".to_string();
                config.save()?;
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
            }
        },
        Command::Auth { command } => match command {
            AuthCommand::Codex {
                enable_container_mount,
                codex_home,
                read_only,
            } => {
                println!("Starting Codex auth bootstrap.");
                println!("Run `codex` in this terminal and complete the OpenAI sign-in flow.");
                println!("Librarian will avoid copying Codex credentials into project files.");
                if enable_container_mount || codex_home.is_some() {
                    let mut config = config;
                    if let Some(codex_home) = codex_home {
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
            ConfigCommand::SetCodexHome { path } => {
                let mut config = config;
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

async fn print_runtime_check(label: &str, config: &Config, args: &[&str]) {
    let mut all_args = config.docker.runtime_args.clone();
    all_args.extend(args.iter().map(|arg| arg.to_string()));
    print_command_check_owned(label, &config.docker.runtime_command, &all_args).await;
}

async fn print_command_check(label: &str, program: &str, args: &[&str]) {
    print_command_check_owned(
        label,
        program,
        &args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>(),
    )
    .await;
}

async fn print_command_check_owned(label: &str, program: &str, args: &[String]) {
    let output = TokioCommand::new(program).args(args).output().await;
    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let first_line = text.lines().next().unwrap_or("ok");
            println!("{label}: ok ({first_line})");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let first_line = stderr.lines().next().unwrap_or("command failed");
            println!("{label}: failed ({first_line})");
        }
        Err(error) => {
            println!("{label}: unavailable ({error})");
        }
    }
}
