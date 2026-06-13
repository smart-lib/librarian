use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde_json::json;
use tokio::process::Command as TokioCommand;

use crate::db::Database;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum GitGateActionArg {
    Commit,
    Push,
    Revert,
}

pub async fn review_job_changes(
    db: &Database,
    job_id: uuid::Uuid,
    run_tests: bool,
) -> Result<serde_json::Value> {
    let job = db.get_job(job_id).await?;
    let project = db.get_project_by_id(job.project_id).await?;
    let project_path = project
        .path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", project.path.display()))?;
    let git_status = run_project_command(&project_path, "git", &["status", "--short"]).await?;
    let git_diff_stat = run_project_command(&project_path, "git", &["diff", "--stat"]).await?;
    let git_diff_name_status =
        run_project_command(&project_path, "git", &["diff", "--name-status"]).await?;
    let staged_diff_stat =
        run_project_command(&project_path, "git", &["diff", "--cached", "--stat"]).await?;
    let has_worktree_changes = !git_status.stdout.trim().is_empty();
    let cargo_manifest = project_path.join("Cargo.toml");
    let test_result = if run_tests {
        if cargo_manifest.exists() {
            Some(run_project_command(&project_path, "cargo", &["test", "--quiet"]).await?)
        } else {
            Some(ProjectCommandOutput {
                command: "cargo test --quiet".to_string(),
                status: None,
                success: false,
                stdout: String::new(),
                stderr: format!(
                    "Skipped: {} does not exist",
                    user_facing_path(&cargo_manifest).display()
                ),
            })
        }
    } else {
        None
    };
    let tests_ok = test_result
        .as_ref()
        .map(|result| result.success)
        .unwrap_or(false);
    let recommendation = if run_tests && !tests_ok {
        "tests_failed_or_missing"
    } else if has_worktree_changes {
        "review_diff_before_commit"
    } else {
        "no_worktree_changes"
    };
    let report = json!({
        "job_id": job.id,
        "project": {
            "id": project.id,
            "name": project.name,
            "path": user_facing_path(&project_path),
        },
        "run_tests": run_tests,
        "has_worktree_changes": has_worktree_changes,
        "recommendation": recommendation,
        "git": {
            "status": git_status,
            "diff_stat": git_diff_stat,
            "diff_name_status": git_diff_name_status,
            "staged_diff_stat": staged_diff_stat,
        },
        "tests": test_result,
    });
    db.add_job_event(job.id, "review", report.clone()).await?;
    Ok(report)
}

pub async fn build_job_review_packet(
    db: &Database,
    job_id: uuid::Uuid,
    run_tests: bool,
    revert_commit: Option<&str>,
) -> Result<serde_json::Value> {
    let job = db.get_job(job_id).await?;
    let project = db.get_project_by_id(job.project_id).await?;
    let review = review_job_changes(db, job_id, run_tests).await?;
    let commit_gate = gate_job_git_action(db, job_id, GitGateActionArg::Commit).await?;
    let revert_plan = plan_job_git_revert(db, job_id, revert_commit).await?;
    let push_plan = plan_job_git_push(db, job_id).await?;

    let review_recommendation = review
        .get("recommendation")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let commit_allowed = commit_gate
        .get("allowed")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let revert_allowed = revert_plan
        .get("allowed")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let push_allowed = push_plan
        .get("allowed")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let has_worktree_changes = review
        .get("has_worktree_changes")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let next_step = if has_worktree_changes && commit_allowed {
        "review_diff_then_propose_commit"
    } else if has_worktree_changes {
        "fix_commit_gate_blockers"
    } else if push_allowed {
        "review_push_plan_then_push_manually"
    } else if revert_allowed {
        "revert_available_if_needed"
    } else {
        "inspect_packet_blockers"
    };

    let packet = json!({
        "job_id": job.id,
        "project": {
            "id": project.id,
            "name": project.name,
            "path": user_facing_path(&project.path),
        },
        "run_tests": run_tests,
        "summary": {
            "review_recommendation": review_recommendation,
            "has_worktree_changes": has_worktree_changes,
            "commit_allowed": commit_allowed,
            "revert_allowed": revert_allowed,
            "push_allowed": push_allowed,
            "next_step": next_step,
        },
        "review": review,
        "commit_gate": commit_gate,
        "revert_plan": revert_plan,
        "push_plan": push_plan,
    });
    db.add_job_event(job.id, "review_packet", packet.clone())
        .await?;
    Ok(packet)
}

pub async fn gate_job_git_action(
    db: &Database,
    job_id: uuid::Uuid,
    action: GitGateActionArg,
) -> Result<serde_json::Value> {
    let job = db.get_job(job_id).await?;
    let project = db.get_project_by_id(job.project_id).await?;
    let project_path = project
        .path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", project.path.display()))?;
    let branch = run_project_command(&project_path, "git", &["branch", "--show-current"]).await?;
    let status = run_project_command(&project_path, "git", &["status", "--short"]).await?;
    let upstream = run_project_command(
        &project_path,
        "git",
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .await?;
    let branch_name = branch.stdout.trim().to_string();
    let dirty = !status.stdout.trim().is_empty();
    let protected = project
        .git_policy
        .protected_branches
        .iter()
        .any(|protected| protected == &branch_name);
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    if branch_name.is_empty() {
        blockers.push("detached_or_unknown_branch".to_string());
    }
    match action {
        GitGateActionArg::Commit => {
            if !project.git_policy.allow_commit {
                blockers.push("project_policy_disallows_commit".to_string());
            }
            if !dirty {
                blockers.push("no_worktree_changes_to_commit".to_string());
            }
        }
        GitGateActionArg::Push => {
            if !project.git_policy.allow_push {
                blockers.push("project_policy_disallows_push".to_string());
            }
            if dirty {
                blockers.push("worktree_has_uncommitted_changes".to_string());
            }
            if !upstream.success {
                warnings.push("no_upstream_branch_detected".to_string());
            }
        }
        GitGateActionArg::Revert => {
            if !project.git_policy.allow_commit {
                blockers.push("project_policy_disallows_commit".to_string());
            }
            if dirty {
                blockers.push("worktree_has_uncommitted_changes".to_string());
            }
        }
    }
    if protected {
        blockers.push(format!("protected_branch:{branch_name}"));
    }
    if let Some(pattern) = &project.git_policy.require_branch_pattern {
        if !branch_pattern_matches(pattern, &branch_name) {
            blockers.push(format!("branch_does_not_match_required_pattern:{pattern}"));
        }
    }

    let allowed = blockers.is_empty();
    let report = json!({
        "job_id": job.id,
        "action": match action {
            GitGateActionArg::Commit => "commit",
            GitGateActionArg::Push => "push",
            GitGateActionArg::Revert => "revert",
        },
        "allowed": allowed,
        "blockers": blockers,
        "warnings": warnings,
        "project": {
            "id": project.id,
            "name": project.name,
            "path": user_facing_path(&project_path),
            "git_policy": project.git_policy,
        },
        "git": {
            "branch": branch,
            "status": status,
            "upstream": upstream,
            "dirty": dirty,
            "protected": protected,
        },
    });
    db.add_job_event(job.id, "policy_gate", report.clone())
        .await?;
    Ok(report)
}

pub async fn propose_job_git_action(
    db: &Database,
    job_id: uuid::Uuid,
    action: GitGateActionArg,
    message: Option<&str>,
    commit: Option<&str>,
) -> Result<crate::domain::ToolApproval> {
    let gate = if matches!(action, GitGateActionArg::Revert) {
        plan_job_git_revert(db, job_id, commit).await?
    } else {
        gate_job_git_action(db, job_id, action).await?
    };
    if !gate
        .get("allowed")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        anyhow::bail!(
            "Git action is blocked by policy gate: {}",
            serde_json::to_string(&gate.get("blockers").cloned().unwrap_or_else(|| json!([])))?
        );
    }
    let job = db.get_job(job_id).await?;
    let project = db.get_project_by_id(job.project_id).await?;
    let action_label = match action {
        GitGateActionArg::Commit => "commit",
        GitGateActionArg::Push => "push",
        GitGateActionArg::Revert => "revert",
    };
    let message = match action {
        GitGateActionArg::Commit => Some(
            message
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("--message is required for commit proposals"))?
                .trim()
                .to_string(),
        ),
        GitGateActionArg::Push => message.map(|value| value.trim().to_string()),
        GitGateActionArg::Revert => message.map(|value| value.trim().to_string()),
    };
    let commit = if matches!(action, GitGateActionArg::Revert) {
        Some(
            gate.get("target_commit")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("Revert plan did not include a target commit"))?
                .to_string(),
        )
    } else {
        None
    };
    let approval = db
        .create_tool_approval(
            "git",
            action_label,
            json!({
                "job_id": job.id,
                "project_id": project.id,
                "project": project.name,
                "project_path": user_facing_path(&project.path),
                "message": message,
                "commit": commit,
                "gate": gate,
                "summary": format!("Approve git {action_label} for job {} in project `{}`.", job.id, project.name),
            }),
        )
        .await?;
    db.add_job_event(
        job.id,
        "approval_proposed",
        json!({
            "approval_id": approval.id,
            "tool": approval.tool,
            "action": approval.action,
        }),
    )
    .await?;
    Ok(approval)
}

pub async fn plan_job_git_revert(
    db: &Database,
    job_id: uuid::Uuid,
    commit: Option<&str>,
) -> Result<serde_json::Value> {
    let job = db.get_job(job_id).await?;
    let project = db.get_project_by_id(job.project_id).await?;
    let project_path = project
        .path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", project.path.display()))?;
    let branch = run_project_command(&project_path, "git", &["branch", "--show-current"]).await?;
    let status = run_project_command(&project_path, "git", &["status", "--short"]).await?;
    let upstream = run_project_command(
        &project_path,
        "git",
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .await?;
    let target = if let Some(commit) = commit.map(str::trim).filter(|value| !value.is_empty()) {
        commit.to_string()
    } else {
        let latest =
            run_project_command(&project_path, "git", &["log", "-1", "--format=%H"]).await?;
        latest.stdout.trim().to_string()
    };
    let target_info = if target.is_empty() {
        ProjectCommandOutput {
            command: "git show --quiet --format=%H%n%s <missing>".to_string(),
            status: Some(1),
            success: false,
            stdout: String::new(),
            stderr: "No target commit supplied and no latest commit found".to_string(),
        }
    } else {
        run_project_command(
            &project_path,
            "git",
            &["show", "--quiet", "--format=%H%n%s", &target],
        )
        .await?
    };
    let branch_name = branch.stdout.trim().to_string();
    let dirty = !status.stdout.trim().is_empty();
    let protected = project
        .git_policy
        .protected_branches
        .iter()
        .any(|protected| protected == &branch_name);
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    if !project.git_policy.allow_commit {
        blockers.push("project_policy_disallows_commit".to_string());
    }
    if branch_name.is_empty() {
        blockers.push("detached_or_unknown_branch".to_string());
    }
    if dirty {
        blockers.push("worktree_has_uncommitted_changes".to_string());
    }
    if protected {
        blockers.push(format!("protected_branch:{branch_name}"));
    }
    if let Some(pattern) = &project.git_policy.require_branch_pattern {
        if !branch_pattern_matches(pattern, &branch_name) {
            blockers.push(format!("branch_does_not_match_required_pattern:{pattern}"));
        }
    }
    if !target_info.success {
        blockers.push("target_commit_not_found".to_string());
    }
    if !upstream.success {
        warnings.push("no_upstream_branch_detected".to_string());
    }

    let allowed = blockers.is_empty();
    let subject = target_info
        .stdout
        .lines()
        .nth(1)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-");
    let report = json!({
        "job_id": job.id,
        "action": "revert",
        "allowed": allowed,
        "blockers": blockers,
        "warnings": warnings,
        "target_commit": target,
        "target_subject": subject,
        "project": {
            "id": project.id,
            "name": project.name,
            "path": user_facing_path(&project_path),
            "git_policy": project.git_policy,
        },
        "git": {
            "branch": branch,
            "status": status,
            "upstream": upstream,
            "dirty": dirty,
            "protected": protected,
            "target": target_info,
        },
    });
    db.add_job_event(job.id, "revert_plan", report.clone())
        .await?;
    Ok(report)
}

pub async fn plan_job_git_push(db: &Database, job_id: uuid::Uuid) -> Result<serde_json::Value> {
    let gate = gate_job_git_action(db, job_id, GitGateActionArg::Push).await?;
    let job = db.get_job(job_id).await?;
    let project = db.get_project_by_id(job.project_id).await?;
    let project_path = project
        .path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", project.path.display()))?;
    let branch = run_project_command(&project_path, "git", &["branch", "--show-current"]).await?;
    let status = run_project_command(&project_path, "git", &["status", "--short"]).await?;
    let upstream = run_project_command(
        &project_path,
        "git",
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .await?;
    let remotes = run_project_command(&project_path, "git", &["remote", "-v"]).await?;
    let mut blockers = Vec::new();
    let mut warnings = gate
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(values) = gate.get("blockers").and_then(serde_json::Value::as_array) {
        blockers.extend(
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned),
        );
    }
    let (ahead_count, outgoing_commits, diff_stat) = if upstream.success {
        let range = format!("{}..HEAD", upstream.stdout.trim());
        let ahead =
            run_project_command(&project_path, "git", &["rev-list", "--count", &range]).await?;
        let log = run_project_command(
            &project_path,
            "git",
            &["log", "--oneline", "--decorate", "--max-count=20", &range],
        )
        .await?;
        let diff = run_project_command(&project_path, "git", &["diff", "--stat", &range]).await?;
        let count = ahead.stdout.trim().parse::<u64>().unwrap_or(0);
        if count == 0 {
            blockers.push("no_outgoing_commits_to_push".to_string());
        }
        (count, log, diff)
    } else {
        blockers.push("no_upstream_branch_detected".to_string());
        warnings.push("set_upstream_or_push_manually_with_remote_branch".to_string());
        (
            0,
            ProjectCommandOutput {
                command: "git log --oneline --decorate --max-count=20 @{u}..HEAD".to_string(),
                status: Some(1),
                success: false,
                stdout: String::new(),
                stderr: "No upstream branch detected".to_string(),
            },
            ProjectCommandOutput {
                command: "git diff --stat @{u}..HEAD".to_string(),
                status: Some(1),
                success: false,
                stdout: String::new(),
                stderr: "No upstream branch detected".to_string(),
            },
        )
    };
    blockers.sort();
    blockers.dedup();
    warnings.sort();
    warnings.dedup();
    let allowed = blockers.is_empty();
    let report = json!({
        "job_id": job.id,
        "action": "push",
        "allowed": allowed,
        "blockers": blockers,
        "warnings": warnings,
        "ahead_count": ahead_count,
        "project": {
            "id": project.id,
            "name": project.name,
            "path": user_facing_path(&project_path),
            "git_policy": project.git_policy,
        },
        "git": {
            "branch": branch,
            "status": status,
            "upstream": upstream,
            "remotes": remotes,
            "outgoing_commits": outgoing_commits,
            "diff_stat": diff_stat,
            "policy_gate": gate,
        },
    });
    db.add_job_event(job.id, "push_plan", report.clone())
        .await?;
    Ok(report)
}

fn branch_pattern_matches(pattern: &str, branch: &str) -> bool {
    fn inner(pattern: &[u8], branch: &[u8]) -> bool {
        match pattern.split_first() {
            None => branch.is_empty(),
            Some((&b'*', rest)) => {
                inner(rest, branch)
                    || branch
                        .split_first()
                        .is_some_and(|(_, branch_rest)| inner(pattern, branch_rest))
            }
            Some((&b'?', rest)) => branch
                .split_first()
                .is_some_and(|(_, branch_rest)| inner(rest, branch_rest)),
            Some((&expected, rest)) => {
                branch.split_first().is_some_and(|(&actual, branch_rest)| {
                    expected == actual && inner(rest, branch_rest)
                })
            }
        }
    }
    inner(pattern.as_bytes(), branch.as_bytes())
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ProjectCommandOutput {
    pub command: String,
    pub status: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub async fn run_project_command(
    project_path: &Path,
    command: &str,
    args: &[&str],
) -> Result<ProjectCommandOutput> {
    let output = TokioCommand::new(command)
        .args(args)
        .current_dir(project_path)
        .output()
        .await
        .with_context(|| {
            format!(
                "Failed to run `{}` in {}",
                command,
                user_facing_path(project_path).display()
            )
        })?;
    Ok(ProjectCommandOutput {
        command: std::iter::once(command)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" "),
        status: output.status.code(),
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

fn user_facing_path(path: &Path) -> PathBuf {
    let text = path.display().to_string();
    if let Some(stripped) = text.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}
