use std::process::Stdio;

use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
};

use crate::{
    config::Config,
    db::Database,
    docker_runner::DockerRunner,
    domain::{AgentRunSpec, Job, JobStatus, MemoryKind},
    gates,
    memory::{self, RetrievalRequest},
    prompt, router,
    vault::Vault,
};

pub async fn run_once(config: Config, db: Database) -> Result<bool> {
    let Some(job) = db.claim_next_queued_job().await? else {
        return Ok(false);
    };

    run_job(config, db, job).await?;
    Ok(true)
}

pub async fn run_batch(config: Config, db: Database, concurrency: usize) -> Result<usize> {
    let concurrency = concurrency.max(1);
    let mut set = tokio::task::JoinSet::new();

    for _ in 0..concurrency {
        let config = config.clone();
        let db = db.clone();
        set.spawn(async move { run_once(config, db).await });
    }

    let mut ran = 0;
    while let Some(result) = set.join_next().await {
        if result?? {
            ran += 1;
        }
    }
    Ok(ran)
}

#[derive(Clone, Debug, Serialize)]
pub struct JobPreflightReport {
    pub job_id: uuid::Uuid,
    pub selected_provider: String,
    pub fallback_from: Option<String>,
    pub fallback_reason: Option<String>,
    pub project_name: String,
    pub project_path: String,
    pub context_hits: usize,
    pub prompt_chars: usize,
    pub command: Vec<String>,
    pub budget_checks: Vec<router::BudgetCheck>,
}

pub async fn preflight_job(
    config: Config,
    db: Database,
    job_id: uuid::Uuid,
) -> Result<JobPreflightReport> {
    let job = db.get_job(job_id).await?;
    let report = prepare_job(&config, &db, job.clone(), true).await?;
    db.add_job_event(
        job.id,
        "preflight",
        json!({
            "selected_provider": &report.selected_provider,
            "fallback_from": &report.fallback_from,
            "fallback_reason": &report.fallback_reason,
            "project_name": &report.project_name,
            "project_path": &report.project_path,
            "context_hits": report.context_hits,
            "prompt_chars": report.prompt_chars,
            "command": &report.command,
            "budget_checks": &report.budget_checks,
            "launched": false,
        }),
    )
    .await?;
    Ok(report)
}

async fn run_job(config: Config, db: Database, mut job: Job) -> Result<()> {
    if db.get_job(job.id).await?.cancel_requested_at.is_some() {
        db.mark_job_finished(job.id, JobStatus::Cancelled).await?;
        db.add_job_event(
            job.id,
            "status",
            json!({ "status": "Cancelled", "stage": "pre_start" }),
        )
        .await?;
        return Ok(());
    }

    db.add_job_event(job.id, "status", json!({ "status": "Preparing" }))
        .await?;
    match router::select_provider_for_job(&config, &db, &job).await {
        Ok(selection) => {
            if selection.provider != job.provider {
                let fallback_from = selection
                    .fallback_from
                    .clone()
                    .unwrap_or(job.provider.clone());
                db.update_job_provider(job.id, selection.provider.clone())
                    .await?;
                db.add_job_event(
                    job.id,
                    "provider_fallback_selected",
                    json!({
                        "from": router::provider_name(&fallback_from),
                        "to": router::provider_name(&selection.provider),
                        "reason": selection.reason,
                    }),
                )
                .await?;
                job.provider = selection.provider;
            }
        }
        Err(error) => {
            db.update_job_status(job.id, JobStatus::Queued).await?;
            db.add_job_event(
                job.id,
                "provider_paused",
                json!({ "error": error.to_string() }),
            )
            .await?;
            return Ok(());
        }
    }

    match router::ensure_budget_available(&config, &db, &job).await {
        Ok(checks) if !checks.is_empty() => {
            db.add_job_event(job.id, "budget_checked", json!({ "checks": checks }))
                .await?;
        }
        Ok(_) => {}
        Err(error) => {
            db.update_job_status(job.id, JobStatus::Queued).await?;
            db.add_job_event(
                job.id,
                "budget_blocked",
                json!({ "error": error.to_string() }),
            )
            .await?;
            return Ok(());
        }
    }

    let project = db.get_project_by_id(job.project_id).await?;
    let vault = Vault::new(&config);
    let project_note = vault.write_project_note(&project)?;
    let context_pack = memory::retrieve_context_with_config(
        &db,
        Some(&config),
        RetrievalRequest {
            query: job.goal.clone(),
            project_id: Some(project.id),
            activity_id: None,
            limit: memory::default_hit_limit(),
        },
    )
    .await?;
    let enriched_prompt = prompt::build_agent_prompt(&project, &job.goal, &context_pack);

    let spec = AgentRunSpec {
        job_id: job.id,
        project_path: project.path.clone(),
        provider: job.provider.clone(),
        goal: job.goal.clone(),
        prompt: enriched_prompt,
        mount_mode: job.mount_mode,
        network_mode: job.network_mode,
        secret_grant_token: None,
    };
    let prompt_len = spec.prompt.chars().count();

    let runner = DockerRunner::new(config.clone());
    let command_parts = runner.docker_command_parts(&spec).await?;
    db.add_job_event(
        job.id,
        "prepared",
        json!({
            "command": command_parts.clone(),
            "project_note": project_note,
            "context_hits": context_pack.hits.len(),
            "prompt_chars": prompt_len,
        }),
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

    db.update_job_status(job.id, JobStatus::Running).await?;
    db.mark_job_started(job.id).await?;
    db.add_job_event(job.id, "status", json!({ "status": "Running" }))
        .await?;

    let output = execute(command_parts, &db, &config, &job).await;
    match output {
        Ok(code) if code == 0 => {
            db.mark_job_finished(job.id, JobStatus::Completed).await?;
            db.add_job_event(
                job.id,
                "status",
                json!({ "status": "Completed", "exit_code": code }),
            )
            .await?;
            router::record_job_usage_estimate(&db, &job, prompt_len, Some(code)).await?;
            let path =
                vault.write_run_summary(&project, &job, "Completed", "Completed successfully.")?;
            db.add_job_event(job.id, "vault", json!({ "run_summary": path }))
                .await?;
            let memory_item = db
                .add_memory_item(
                    Some(project.id),
                    Some(job.id),
                    MemoryKind::RunObservation,
                    Some("run-outcome"),
                    "Job completed successfully.",
                    Some("worker"),
                    json!({ "job_id": job.id, "exit_code": code }),
                )
                .await?;
            memory::embed_item(&db, &config, &memory_item).await?;
        }
        Ok(code) => {
            let final_status = if db.get_job(job.id).await?.cancel_requested_at.is_some() {
                JobStatus::Cancelled
            } else {
                JobStatus::Failed
            };
            db.mark_job_finished(job.id, final_status.clone()).await?;
            db.add_job_event(
                job.id,
                "status",
                json!({ "status": format!("{:?}", final_status), "exit_code": code }),
            )
            .await?;
            router::record_job_usage_estimate(&db, &job, prompt_len, Some(code)).await?;
            let path = vault.write_run_summary(
                &project,
                &job,
                "Failed",
                &format!("Failed with exit code {code}."),
            )?;
            db.add_job_event(job.id, "vault", json!({ "run_summary": path }))
                .await?;
            let memory_item = db
                .add_memory_item(
                    Some(project.id),
                    Some(job.id),
                    MemoryKind::RunObservation,
                    Some("run-outcome"),
                    &format!("Job failed with exit code {code}."),
                    Some("worker"),
                    json!({ "job_id": job.id, "exit_code": code }),
                )
                .await?;
            memory::embed_item(&db, &config, &memory_item).await?;
        }
        Err(error) => {
            let final_status = if db.get_job(job.id).await?.cancel_requested_at.is_some() {
                JobStatus::Cancelled
            } else {
                JobStatus::Failed
            };
            db.mark_job_finished(job.id, final_status.clone()).await?;
            db.add_job_event(job.id, "error", json!({ "error": error.to_string() }))
                .await?;
            let path = vault.write_run_summary(
                &project,
                &job,
                "Failed",
                &format!("Failed to execute: {error}"),
            )?;
            db.add_job_event(job.id, "vault", json!({ "run_summary": path }))
                .await?;
            let memory_item = db
                .add_memory_item(
                    Some(project.id),
                    Some(job.id),
                    MemoryKind::RunObservation,
                    Some("run-error"),
                    &format!("Job failed to execute: {error}"),
                    Some("worker"),
                    json!({ "job_id": job.id }),
                )
                .await?;
            memory::embed_item(&db, &config, &memory_item).await?;
            return Err(error);
        }
    }

    Ok(())
}

async fn prepare_job(
    config: &Config,
    db: &Database,
    mut job: Job,
    dry_run: bool,
) -> Result<JobPreflightReport> {
    let selection = router::select_provider_for_job(config, db, &job).await?;
    let mut fallback_from = None;
    let mut fallback_reason = None;
    if selection.provider != job.provider {
        fallback_from = selection
            .fallback_from
            .clone()
            .map(|provider| router::provider_name(&provider).to_string());
        fallback_reason = selection.reason.clone();
        if !dry_run {
            db.update_job_provider(job.id, selection.provider.clone())
                .await?;
            db.add_job_event(
                job.id,
                "provider_fallback_selected",
                json!({
                    "from": &fallback_from,
                    "to": router::provider_name(&selection.provider),
                    "reason": &fallback_reason,
                }),
            )
            .await?;
        }
        job.provider = selection.provider;
    }

    let budget_checks = router::ensure_budget_available(config, db, &job).await?;
    if !dry_run && !budget_checks.is_empty() {
        db.add_job_event(
            job.id,
            "budget_checked",
            json!({ "checks": &budget_checks }),
        )
        .await?;
    }

    let project = db.get_project_by_id(job.project_id).await?;
    let context_pack = memory::retrieve_context_with_config(
        db,
        Some(config),
        RetrievalRequest {
            query: job.goal.clone(),
            project_id: Some(project.id),
            activity_id: None,
            limit: memory::default_hit_limit(),
        },
    )
    .await?;
    let enriched_prompt = prompt::build_agent_prompt(&project, &job.goal, &context_pack);
    let prompt_len = enriched_prompt.chars().count();
    let spec = AgentRunSpec {
        job_id: job.id,
        project_path: project.path.clone(),
        provider: job.provider.clone(),
        goal: job.goal.clone(),
        prompt: enriched_prompt,
        mount_mode: job.mount_mode,
        network_mode: job.network_mode,
        secret_grant_token: None,
    };
    let command = DockerRunner::new(config.clone())
        .docker_command_parts(&spec)
        .await?;

    if !dry_run {
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
    }

    Ok(JobPreflightReport {
        job_id: job.id,
        selected_provider: router::provider_name(&job.provider).to_string(),
        fallback_from,
        fallback_reason,
        project_name: project.name,
        project_path: project.path.display().to_string(),
        context_hits: context_pack.hits.len(),
        prompt_chars: prompt_len,
        command,
        budget_checks,
    })
}

async fn execute(
    command_parts: Vec<String>,
    db: &Database,
    config: &Config,
    job: &Job,
) -> Result<i32> {
    let mut command = Command::new(&command_parts[0]);
    command.args(&command_parts[1..]);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn()?;

    let stdout = child.stdout.take().map(BufReader::new);
    let stderr = child.stderr.take().map(BufReader::new);

    let stdout_task = tokio::spawn(stream_lines(
        stdout,
        db.clone(),
        config.clone(),
        job.clone(),
        "stdout",
    ));
    let stderr_task = tokio::spawn(stream_lines(
        stderr,
        db.clone(),
        config.clone(),
        job.clone(),
        "stderr",
    ));

    let status = loop {
        if db.get_job(job.id).await?.cancel_requested_at.is_some() {
            db.add_job_event(job.id, "cancel", json!({ "action": "kill_child" }))
                .await?;
            child.kill().await?;
            break child.wait().await?;
        }

        match child.try_wait()? {
            Some(status) => break status,
            None => {
                db.heartbeat_job(job.id).await?;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    };
    stdout_task.await??;
    stderr_task.await??;

    Ok(status.code().unwrap_or(1))
}

async fn stream_lines<R>(
    reader: Option<BufReader<R>>,
    db: Database,
    config: Config,
    job: Job,
    kind: &'static str,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let Some(reader) = reader else {
        return Ok(());
    };
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        let safe_line = gates::process_output(&db, &config, &line, kind)
            .await?
            .content;
        if let Some(reason) = crate::router::detect_limit_event(&safe_line) {
            db.add_job_event(
                job.id,
                "provider_limit_detected",
                json!({ "reason": reason, "line": safe_line }),
            )
            .await?;
            crate::router::record_limit_event(&db, &job, &safe_line).await?;
        }
        if let Some(diagnostic) = crate::router::detect_provider_diagnostic(&job, &safe_line) {
            db.add_job_event(
                job.id,
                "provider_diagnostic",
                json!({ "diagnostic": diagnostic, "line": safe_line }),
            )
            .await?;
        }
        db.add_job_event(job.id, kind, json!({ "line": safe_line }))
            .await?;
    }
    Ok(())
}
