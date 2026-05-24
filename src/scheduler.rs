use anyhow::{bail, Result};
use chrono::{Duration, Utc};
use serde_json::json;

use crate::{
    config::Config,
    db::Database,
    docker_runner::DockerRunner,
    domain::{JobStatus, MountMode, Schedule, ScheduleKind},
    router,
};

pub const DEFAULT_HEARTBEAT_TIMEOUT_SECONDS: i64 = 120;

pub async fn tick(db: &Database, config: &Config) -> Result<SchedulerTickReport> {
    let schedules = db.due_schedules().await?;
    let mut ran_schedules = 0;

    for schedule in schedules {
        run_schedule(db, config, &schedule).await?;
        db.mark_schedule_ran(schedule.id).await?;
        ran_schedules += 1;
    }

    let heartbeat_missed = mark_heartbeat_missed(db, DEFAULT_HEARTBEAT_TIMEOUT_SECONDS).await?;

    Ok(SchedulerTickReport {
        ran_schedules,
        heartbeat_missed,
    })
}

pub async fn run_schedule_now(
    db: &Database,
    config: &Config,
    schedule_id: uuid::Uuid,
) -> Result<()> {
    let schedule = db.get_schedule(schedule_id).await?;
    run_schedule(db, config, &schedule).await?;
    db.mark_schedule_ran(schedule.id).await?;
    db.add_system_event(
        "schedule_manual_run",
        json!({
            "schedule_id": schedule.id,
            "name": schedule.name,
        }),
    )
    .await?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct SchedulerTickReport {
    pub ran_schedules: usize,
    pub heartbeat_missed: usize,
}

async fn run_schedule(db: &Database, config: &Config, schedule: &Schedule) -> Result<()> {
    match schedule.kind {
        ScheduleKind::System => run_system_schedule(db, config, schedule).await?,
        ScheduleKind::Reminder => {
            db.add_system_event(
                "reminder_due",
                json!({
                    "schedule_id": schedule.id,
                    "name": schedule.name,
                    "payload": schedule.payload,
                }),
            )
            .await?;
        }
        ScheduleKind::AgentTask => run_agent_task_schedule(db, schedule).await?,
    }
    Ok(())
}

async fn run_system_schedule(db: &Database, config: &Config, schedule: &Schedule) -> Result<()> {
    let task = schedule
        .payload
        .get("task")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");

    match task {
        "heartbeat_recovery" => {
            db.add_system_event(
                "schedule_tick",
                json!({
                    "schedule_id": schedule.id,
                    "name": schedule.name,
                    "task": task,
                }),
            )
            .await?;
        }
        "memory_compaction_candidates" => {
            let older_than_days = schedule
                .payload
                .get("older_than_days")
                .and_then(|value| value.as_i64())
                .unwrap_or(14)
                .max(1);
            let cutoff = Utc::now() - Duration::days(older_than_days);
            let candidates = db.count_memory_compaction_candidates(cutoff).await?;
            db.add_system_event(
                "memory_compaction_candidates",
                json!({
                    "schedule_id": schedule.id,
                    "name": schedule.name,
                    "older_than_days": older_than_days,
                    "candidate_count": candidates,
                }),
            )
            .await?;
        }
        "container_cleanup" => {
            let report = DockerRunner::new(config.clone())
                .cleanup_stopped_librarian_containers()
                .await?;
            db.add_system_event(
                "container_cleanup",
                json!({
                    "schedule_id": schedule.id,
                    "name": schedule.name,
                    "success": report.success,
                    "stdout": report.stdout,
                    "stderr": report.stderr,
                }),
            )
            .await?;
        }
        other => {
            db.add_system_event(
                "schedule_unknown_system_task",
                json!({
                    "schedule_id": schedule.id,
                    "name": schedule.name,
                    "task": other,
                    "payload": schedule.payload,
                }),
            )
            .await?;
        }
    }
    Ok(())
}

async fn run_agent_task_schedule(db: &Database, schedule: &Schedule) -> Result<()> {
    let project_ref = required_payload_string(schedule, "project")?;
    let goal = required_payload_string(schedule, "goal")?;
    let provider = router::parse_provider_kind(
        schedule
            .payload
            .get("provider")
            .and_then(|value| value.as_str())
            .unwrap_or("codex"),
    )?;
    let project = db.get_project_by_name_or_id(project_ref).await?;
    let mount_mode = if schedule
        .payload
        .get("read_only")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        MountMode::ReadOnly
    } else {
        MountMode::ReadWrite
    };
    let secret_grant_token = schedule
        .payload
        .get("secret_grant_token")
        .and_then(|value| value.as_str());
    let network_mode = router::default_network_mode_for_provider(
        &provider,
        schedule
            .payload
            .get("allow_network")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        secret_grant_token.is_some(),
    );

    let job = db
        .create_job(
            project.id,
            provider,
            goal,
            mount_mode,
            network_mode,
            secret_grant_token,
        )
        .await?;
    db.add_job_event(
        job.id,
        "scheduled",
        json!({
            "schedule_id": schedule.id,
            "schedule_name": schedule.name,
        }),
    )
    .await?;
    db.add_system_event(
        "scheduled_agent_task",
        json!({
            "schedule_id": schedule.id,
            "schedule_name": schedule.name,
            "job_id": job.id,
            "project": project.name,
            "network_mode": network_mode,
        }),
    )
    .await?;
    Ok(())
}

fn required_payload_string<'a>(schedule: &'a Schedule, key: &str) -> Result<&'a str> {
    match schedule.payload.get(key).and_then(|value| value.as_str()) {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => bail!(
            "Schedule `{}` payload must include non-empty `{key}`",
            schedule.name
        ),
    }
}

async fn mark_heartbeat_missed(db: &Database, timeout_seconds: i64) -> Result<usize> {
    let cutoff = Utc::now() - Duration::seconds(timeout_seconds);
    let jobs = db.running_jobs_missing_heartbeat(cutoff).await?;
    let mut count = 0;
    for job in jobs {
        db.update_job_status(job.id, JobStatus::HeartbeatMissed)
            .await?;
        db.add_job_event(
            job.id,
            "heartbeat_missed",
            json!({
                "cutoff": cutoff,
                "last_heartbeat_at": job.last_heartbeat_at,
            }),
        )
        .await?;
        count += 1;
    }
    Ok(count)
}
