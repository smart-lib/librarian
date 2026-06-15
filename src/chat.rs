use std::{
    collections::HashSet,
    process::Stdio,
    time::{Duration, Instant},
};

use anyhow::Result;
use async_trait::async_trait;
use tokio::{io::AsyncWriteExt, process::Command as TokioCommand, time::timeout};
use uuid::Uuid;

use crate::{
    config::Config,
    db::Database,
    domain::{ChatTurn, ContextPack, MemoryHit, Project},
    memory, prompt,
};

pub(crate) struct LibrarianChatResult {
    pub(crate) reply: String,
    pub(crate) iterations: usize,
    pub(crate) memory_hits: Vec<MemoryHit>,
    pub(crate) trace: Vec<serde_json::Value>,
    pub(crate) mode: &'static str,
    pub(crate) ui: Option<serde_json::Value>,
}

#[derive(Debug, serde::Deserialize)]
struct LibrarianChatDirective {
    action: String,
    query: Option<String>,
    answer: Option<String>,
    question: Option<String>,
    reason: Option<String>,
    tool: Option<String>,
    tool_action: Option<String>,
    payload: Option<serde_json::Value>,
}

pub(crate) async fn run_librarian_chat_loop(
    db: &Database,
    config: &Config,
    message: &str,
    project: Option<&Project>,
    recent_turns: &[ChatTurn],
    initial_context_pack: ContextPack,
) -> Result<LibrarianChatResult> {
    let mut runner = CodexLibrarianChatRunner;
    run_librarian_chat_loop_with_runner(
        db,
        config,
        message,
        project,
        recent_turns,
        initial_context_pack,
        &mut runner,
    )
    .await
}

#[async_trait]
trait LibrarianChatRunner: Send {
    async fn run(&mut self, config: &Config, prompt: &str) -> Result<String>;
}

struct CodexLibrarianChatRunner;

#[async_trait]
impl LibrarianChatRunner for CodexLibrarianChatRunner {
    async fn run(&mut self, config: &Config, prompt: &str) -> Result<String> {
        run_librarian_codex_chat(config, prompt).await
    }
}

async fn run_librarian_chat_loop_with_runner(
    db: &Database,
    config: &Config,
    message: &str,
    project: Option<&Project>,
    recent_turns: &[ChatTurn],
    initial_context_pack: ContextPack,
    runner: &mut (dyn LibrarianChatRunner + Send),
) -> Result<LibrarianChatResult> {
    let project_id = project.map(|project| project.id);
    let max_iterations = config.chat.max_iterations.clamp(1, 100);
    let mut context_packs = vec![initial_context_pack];
    let job_snapshot = active_job_snapshot(db, project_id).await?;
    let job_event_snapshot = recent_job_event_snapshot(db, project_id).await?;
    let mut tool_feedback = Vec::new();
    let mut trace = Vec::new();
    let mut last_raw_reply = String::new();

    for iteration in 1..=max_iterations {
        let iteration_started_at = Instant::now();
        let librarian_blocks = db
            .list_prompt_blocks(Some(prompt::TARGET_LIBRARIAN))
            .await?;
        let prompt_version =
            prompt::prompt_block_version(Some(prompt::TARGET_LIBRARIAN), &librarian_blocks);
        let librarian_instruction_blocks = prompt::render_prompt_blocks(&librarian_blocks);
        let prompt = build_librarian_chat_prompt(
            config,
            message,
            project,
            recent_turns,
            &context_packs,
            &librarian_instruction_blocks,
            &job_snapshot,
            &job_event_snapshot,
            &tool_feedback,
            iteration,
            max_iterations,
        );
        let prompt_chars = prompt.chars().count();
        let raw_reply = match runner.run(config, &prompt).await {
            Ok(reply) => reply,
            Err(error) => {
                return Ok(chat_provider_unavailable_result(
                    config,
                    error,
                    iteration,
                    combined_memory_hits(&context_packs),
                    trace,
                ));
            }
        };
        let provider_elapsed_ms = iteration_started_at.elapsed().as_millis();
        last_raw_reply = raw_reply.clone();

        let Some(directive) = parse_librarian_chat_directive(&raw_reply) else {
            trace.push(serde_json::json!({
                "iteration": iteration,
                "action": "plain_answer",
                "prompt_version": prompt_version,
                "prompt_chars": prompt_chars,
                "reply_chars": raw_reply.chars().count(),
                "provider_elapsed_ms": provider_elapsed_ms,
            }));
            return Ok(LibrarianChatResult {
                reply: raw_reply,
                iterations: iteration,
                memory_hits: combined_memory_hits(&context_packs),
                trace,
                mode: "codex-chat",
                ui: None,
            });
        };

        let action = directive.action.trim().to_ascii_lowercase();
        match action.as_str() {
            "answer" => {
                let reply = directive
                    .answer
                    .map(|answer| answer.trim().to_string())
                    .filter(|answer| !answer.is_empty())
                    .unwrap_or(raw_reply);
                trace.push(serde_json::json!({
                    "iteration": iteration,
                    "action": "answer",
                    "reason": directive.reason,
                    "prompt_version": prompt_version,
                    "prompt_chars": prompt_chars,
                    "reply_chars": reply.chars().count(),
                    "provider_elapsed_ms": provider_elapsed_ms,
                }));
                return Ok(LibrarianChatResult {
                    reply,
                    iterations: iteration,
                    memory_hits: combined_memory_hits(&context_packs),
                    trace,
                    mode: "codex-chat",
                    ui: None,
                });
            }
            "clarify" => {
                let reply = directive
                    .question
                    .or(directive.answer)
                    .map(|answer| answer.trim().to_string())
                    .filter(|answer| !answer.is_empty())
                    .unwrap_or_else(|| "Can you clarify what you want me to focus on?".to_string());
                trace.push(serde_json::json!({
                    "iteration": iteration,
                    "action": "clarify",
                    "reason": directive.reason,
                    "prompt_version": prompt_version,
                    "prompt_chars": prompt_chars,
                    "reply_chars": reply.chars().count(),
                    "provider_elapsed_ms": provider_elapsed_ms,
                }));
                return Ok(LibrarianChatResult {
                    reply,
                    iterations: iteration,
                    memory_hits: combined_memory_hits(&context_packs),
                    trace,
                    mode: "codex-chat",
                    ui: None,
                });
            }
            "search_memory" => {
                let query = directive
                    .query
                    .map(|query| query.trim().to_string())
                    .filter(|query| !query.is_empty())
                    .unwrap_or_else(|| message.to_string());
                trace.push(serde_json::json!({
                    "iteration": iteration,
                    "action": "search_memory",
                    "query": query,
                    "reason": directive.reason,
                    "prompt_version": prompt_version,
                    "prompt_chars": prompt_chars,
                    "reply_chars": raw_reply.chars().count(),
                    "provider_elapsed_ms": provider_elapsed_ms,
                }));
                if iteration == max_iterations {
                    return Ok(LibrarianChatResult {
                        reply: format!(
                            "I need a bit more context before I can answer well. The next thing I would look for is: {query}"
                        ),
                        iterations: iteration,
                        memory_hits: combined_memory_hits(&context_packs),
                        trace,
                        mode: "codex-chat",
                        ui: None,
                    });
                }
                let context_pack = memory::retrieve_context_with_config(
                    db,
                    Some(config),
                    memory::RetrievalRequest {
                        query,
                        project_id,
                        activity_id: None,
                        limit: config.chat.memory_hit_limit,
                    },
                )
                .await?;
                context_packs.push(context_pack);
            }
            "propose_tool" => {
                let proposal = validate_tool_proposal(
                    directive.tool.as_deref(),
                    directive.tool_action.as_deref(),
                    directive.payload,
                )
                .and_then(|(tool, tool_action, mut payload)| {
                    if let Some(object) = payload.as_object_mut() {
                        if tool == "agent"
                            && matches!(tool_action.as_str(), "launch" | "queue")
                            && object.get("project").is_none()
                        {
                            let Some(project) = project else {
                                anyhow::bail!(
                                    "Agent launch proposals require a current project context or payload.project"
                                );
                            };
                            object.insert(
                                "project".to_string(),
                                serde_json::json!(project.name.clone()),
                            );
                        }
                        object.insert(
                            "chat_scope".to_string(),
                            serde_json::json!(project.map(|project| project.name.clone())),
                        );
                        object.insert(
                            "user_message".to_string(),
                            serde_json::json!(message.trim()),
                        );
                    }
                    Ok((tool, tool_action, payload))
                });

                let (tool, tool_action, payload) = match proposal {
                    Ok(proposal) => proposal,
                    Err(error) => {
                        let error = error.to_string();
                        trace.push(serde_json::json!({
                            "iteration": iteration,
                            "action": "tool_proposal_feedback",
                            "error": error,
                            "reason": directive.reason,
                            "prompt_version": prompt_version,
                            "prompt_chars": prompt_chars,
                            "reply_chars": raw_reply.chars().count(),
                            "provider_elapsed_ms": provider_elapsed_ms,
                        }));
                        if iteration == max_iterations {
                            return Ok(LibrarianChatResult {
                                reply: format!(
                                    "I could not prepare that action after {iteration} attempt(s): {error}"
                                ),
                                iterations: iteration,
                                memory_hits: combined_memory_hits(&context_packs),
                                trace,
                                mode: "codex-chat",
                                ui: None,
                            });
                        }
                        tool_feedback.push(format!(
                            "Previous tool proposal failed validation: {error}. Correct the JSON and try again, or ask the user one clarifying question. Do not repeat the same invalid proposal."
                        ));
                        continue;
                    }
                };
                let approval = db
                    .create_tool_approval(&tool, &tool_action, payload)
                    .await?;
                db.add_system_event(
                    "tool_approval",
                    serde_json::json!({
                        "action": "propose_from_chat",
                        "approval_id": approval.id,
                        "tool": approval.tool,
                        "tool_action": approval.action,
                        "reason": directive.reason,
                    }),
                )
                .await?;
                trace.push(serde_json::json!({
                    "iteration": iteration,
                    "action": "propose_tool",
                    "approval_id": approval.id,
                    "tool": approval.tool,
                    "tool_action": approval.action,
                    "reason": directive.reason,
                    "prompt_version": prompt_version,
                    "prompt_chars": prompt_chars,
                    "reply_chars": raw_reply.chars().count(),
                    "provider_elapsed_ms": provider_elapsed_ms,
                }));
                return Ok(LibrarianChatResult {
                    reply: format!(
                        "I prepared an action for approval: {} {}.",
                        approval.tool, approval.action
                    ),
                    iterations: iteration,
                    memory_hits: combined_memory_hits(&context_packs),
                    trace,
                    mode: "codex-chat",
                    ui: Some(serde_json::json!({
                        "type": "approval",
                        "approval": approval,
                    })),
                });
            }
            _ => {
                trace.push(serde_json::json!({
                    "iteration": iteration,
                    "action": "unknown_directive",
                    "directive_action": action,
                    "prompt_version": prompt_version,
                    "prompt_chars": prompt_chars,
                    "reply_chars": raw_reply.chars().count(),
                    "provider_elapsed_ms": provider_elapsed_ms,
                }));
                return Ok(LibrarianChatResult {
                    reply: raw_reply,
                    iterations: iteration,
                    memory_hits: combined_memory_hits(&context_packs),
                    trace,
                    mode: "codex-chat",
                    ui: None,
                });
            }
        }
    }

    Ok(LibrarianChatResult {
        reply: last_raw_reply,
        iterations: max_iterations,
        memory_hits: combined_memory_hits(&context_packs),
        trace,
        mode: "codex-chat",
        ui: None,
    })
}

fn validate_tool_proposal(
    tool: Option<&str>,
    action: Option<&str>,
    payload: Option<serde_json::Value>,
) -> Result<(String, String, serde_json::Value)> {
    let tool = normalize_tool_token(tool.unwrap_or_default());
    let action = normalize_tool_action(&tool, action.unwrap_or_default());
    let payload = payload.unwrap_or_else(|| serde_json::json!({}));
    if !payload.is_object() {
        anyhow::bail!("Tool proposal payload must be a JSON object");
    }
    let required = match (tool.as_str(), action.as_str()) {
        ("library", "create_folder" | "create_file" | "delete") => &["path"][..],
        ("library", "write_markdown" | "append_markdown") => &["path", "content"],
        ("library", "move") => &["from", "to"],
        ("library", "replace_lines" | "cut_lines") => &["path", "start_line", "end_line"],
        ("library", "replace_find" | "cut_find") => &["path", "query"],
        ("library", "replace_section" | "cut_section") => &["path", "heading"],
        ("workspace", "create_folder" | "create_file" | "delete") => &["path"][..],
        ("workspace", "move") => &["from", "to"],
        ("project", action) if is_project_creation_tool_action(action) => &["library_path"][..],
        ("memory", "remember" | "add") => &["content"][..],
        ("prompt", "add_block" | "add-block") => &["target", "name", "content"],
        ("agent", "launch" | "queue") => &["goal"][..],
        ("agent", _) => anyhow::bail!(
            "Unsupported agent proposal `{tool}.{action}`. Use tool `agent` with tool_action `launch` and put the requested work in payload.goal."
        ),
        _ => anyhow::bail!("Unsupported tool proposal `{tool}.{action}`"),
    };
    for key in required {
        if payload.get(*key).is_none() {
            anyhow::bail!("Tool proposal `{tool}.{action}` missing `{key}`");
        }
    }
    Ok((tool, action, payload))
}

fn normalize_tool_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

fn normalize_tool_action(tool: &str, value: &str) -> String {
    let action = normalize_tool_token(value).replace('.', "_");
    let prefix = format!("{tool}_");
    action
        .strip_prefix(&prefix)
        .map(ToOwned::to_owned)
        .unwrap_or(action)
}

fn is_project_creation_tool_action(action: &str) -> bool {
    matches!(
        action,
        "create_starting_docs_and_project_folder"
            | "create_starting_docs"
            | "create_library_and_project_folder"
            | "create_site_library_and_project_folder"
            | "create_project_library_and_workspace"
            | "create_library_and_workspace"
    )
}

fn available_tool_actions_prompt() -> &'static str {
    r#"Only these exact tool/tool_action pairs are available. Do not invent action names. In JSON, put the left value in `tool` and the right value in `tool_action`.

- tool=library, tool_action=create_folder: create a folder under Library. Required payload: path.
- tool=library, tool_action=create_file: create a file under Library. Required payload: path.
- tool=library, tool_action=write_markdown: overwrite a Markdown file under Library. Required payload: path, content.
- tool=library, tool_action=append_markdown: append Markdown under Library. Required payload: path, content.
- tool=library, tool_action=move: move a Library item. Required payload: from, to.
- tool=library, tool_action=delete: delete a Library item. Required payload: path.
- tool=library, tool_action=replace_lines: replace an inclusive line range in a Library Markdown file. Required payload: path, start_line, end_line, content.
- tool=library, tool_action=cut_lines: remove an inclusive line range in a Library Markdown file. Required payload: path, start_line, end_line.
- tool=library, tool_action=replace_find: replace text found in a Library Markdown file. Required payload: path, query, content.
- tool=library, tool_action=cut_find: remove text found in a Library Markdown file. Required payload: path, query.
- tool=library, tool_action=replace_section: replace a Markdown heading section under Library. Required payload: path, heading, content.
- tool=library, tool_action=cut_section: remove a Markdown heading section under Library. Required payload: path, heading.
- tool=workspace, tool_action=create_folder: create a folder under Projects. Required payload: path.
- tool=workspace, tool_action=create_file: create a file under Projects. Required payload: path.
- tool=workspace, tool_action=move: move a Projects item. Required payload: from, to.
- tool=workspace, tool_action=delete: delete a Projects item. Required payload: path.
- tool=project, tool_action=create_starting_docs_and_project_folder: create a Library documentation node plus a matching project workspace. Required payload: library_path. Optional payload: name, workspace_path, summary, files.
- tool=memory, tool_action=remember: store durable memory. Required payload: content.
- tool=prompt, tool_action=add_block: add a prompt-builder block. Required payload: target, name, content.
- tool=agent, tool_action=launch: queue background project work for an agent. Required payload: goal. Optional payload: project, provider, read_only, allow_network, secret_grant_token.

For git clone, build, tests, refactors, or other open-ended project work, use tool=agent with tool_action=launch. Put the complete task in payload.goal. Do not create clone/build/test-specific tool_action names.
"#
}

async fn active_job_snapshot(db: &Database, project_id: Option<Uuid>) -> Result<String> {
    let mut jobs = db
        .list_jobs()
        .await?
        .into_iter()
        .filter(|job| {
            matches!(
                job.status,
                crate::domain::JobStatus::Queued
                    | crate::domain::JobStatus::Preparing
                    | crate::domain::JobStatus::Running
                    | crate::domain::JobStatus::HeartbeatMissed
            )
        })
        .filter(|job| project_id.is_none_or(|id| job.project_id == id))
        .collect::<Vec<_>>();
    jobs.sort_by_key(|job| job.created_at);
    let mut lines = Vec::new();
    for job in jobs.iter().take(8) {
        let id = job.id.to_string();
        let short_id = &id[..8];
        let heartbeat = job
            .last_heartbeat_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(|| "-".to_string());
        lines.push(format!(
            "- id={short_id}; status={:?}; provider={:?}; project_id={}; mount={:?}; network={:?}; updated={}; heartbeat={}; goal={}",
            job.status,
            job.provider,
            job.project_id,
            job.mount_mode,
            job.network_mode,
            job.updated_at.to_rfc3339(),
            heartbeat,
            compact_prompt_line(&job.goal, 180),
        ));
    }
    if jobs.len() > lines.len() {
        lines.push(format!(
            "- ... {} more active jobs omitted",
            jobs.len() - lines.len()
        ));
    }
    Ok(lines.join("\n"))
}

async fn recent_job_event_snapshot(db: &Database, project_id: Option<Uuid>) -> Result<String> {
    let mut jobs = db
        .list_jobs()
        .await?
        .into_iter()
        .filter(|job| project_id.is_none_or(|id| job.project_id == id))
        .filter(|job| {
            matches!(
                job.status,
                crate::domain::JobStatus::Failed
                    | crate::domain::JobStatus::Cancelled
                    | crate::domain::JobStatus::Completed
            )
        })
        .collect::<Vec<_>>();
    jobs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    let mut lines = Vec::new();
    for job in jobs.iter().take(5) {
        let id = job.id.to_string();
        let short_id = &id[..8];
        lines.push(format!(
            "- job={short_id}; status={:?}; provider={:?}; updated={}; goal={}",
            job.status,
            job.provider,
            job.updated_at.to_rfc3339(),
            compact_prompt_line(&job.goal, 160)
        ));
        let events = db.list_job_events(job.id).await?;
        let interesting = events
            .iter()
            .rev()
            .filter(|event| {
                matches!(
                    event.kind.as_str(),
                    "stderr"
                        | "stdout"
                        | "failure_category"
                        | "status"
                        | "vault"
                        | "post_run_review_skipped"
                        | "provider_diagnostic"
                )
            })
            .take(6)
            .collect::<Vec<_>>();
        for event in interesting.into_iter().rev() {
            lines.push(format!(
                "  - {} {} {}",
                event.created_at.to_rfc3339(),
                event.kind,
                compact_prompt_line(&job_event_payload_summary(&event.payload), 220)
            ));
        }
    }
    if jobs.len() > 5 {
        lines.push(format!(
            "- ... {} more recent terminal jobs omitted",
            jobs.len() - 5
        ));
    }
    Ok(lines.join("\n"))
}

fn job_event_payload_summary(payload: &serde_json::Value) -> String {
    if let Some(line) = payload.get("line").and_then(|value| value.as_str()) {
        return line.to_string();
    }
    if let Some(category) = payload.get("category") {
        let code = category
            .get("code")
            .and_then(|value| value.as_str())
            .unwrap_or("failure");
        let message = category
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let next_step = category
            .get("next_step")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        return [code, message, next_step]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(": ");
    }
    if let Some(status) = payload.get("status").and_then(|value| value.as_str()) {
        if let Some(exit_code) = payload.get("exit_code").and_then(|value| value.as_i64()) {
            return format!("{status} exit_code={exit_code}");
        }
        return status.to_string();
    }
    if let Some(path) = payload.get("run_summary").and_then(|value| value.as_str()) {
        return format!("run_summary={path}");
    }
    serde_json::to_string(payload).unwrap_or_else(|_| "<unreadable event payload>".to_string())
}

fn compact_prompt_line(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut output = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

fn chat_provider_unavailable_result(
    config: &Config,
    error: anyhow::Error,
    iteration: usize,
    memory_hits: Vec<MemoryHit>,
    mut trace: Vec<serde_json::Value>,
) -> LibrarianChatResult {
    let codex_home = config.home.join(".cfg").join("codex-home");
    trace.push(serde_json::json!({
        "iteration": iteration,
        "action": "provider_unavailable",
        "provider": "codex",
        "error": error.to_string(),
    }));
    LibrarianChatResult {
        reply: format!(
            "Codex chat is not available yet, so I cannot answer with the model in this session.\n\nNext steps:\n1. `CODEX_HOME=\"{}\" codex`\n2. `librarian --home \"{}\" auth codex --enable-container-mount --codex-home \"{}\"`\n3. `librarian --home \"{}\" doctor`\n\nProvider error: {}",
            codex_home.display(),
            config.home.display(),
            codex_home.display(),
            config.home.display(),
            error
        ),
        iterations: iteration,
        memory_hits,
        trace,
        mode: "chat-provider-unavailable",
        ui: None,
    }
}

fn build_librarian_chat_prompt(
    config: &Config,
    message: &str,
    project: Option<&Project>,
    recent_turns: &[ChatTurn],
    context_packs: &[ContextPack],
    instruction_blocks: &str,
    job_snapshot: &str,
    job_event_snapshot: &str,
    tool_feedback: &[String],
    iteration: usize,
    max_iterations: usize,
) -> String {
    let scope = project
        .map(|project| format!("project `{}`", project.name))
        .unwrap_or_else(|| "global conversation".to_string());
    let assistant_name = config.chat.assistant_name.trim();
    let assistant_name = if assistant_name.is_empty() {
        "Librarian"
    } else {
        assistant_name
    };
    let mut prompt = String::new();
    prompt.push_str(&format!(
        "You are {assistant_name}: a calm, practical assistant for organizing ideas, projects, memory, and work.\n"
    ));
    prompt.push_str("You are speaking directly with the user in the admin chat.\n");
    prompt.push_str("You are not a background coding agent in this conversation.\n");
    prompt.push_str("Do not claim to have launched agents, edited files, changed settings, or used tools unless the provided context explicitly says so.\n");
    prompt.push_str("Use the retrieved memory as context, but do not dump it back verbatim. Answer naturally and helpfully.\n");
    prompt.push_str("If the user asks for work that should become an agent task, discuss the plan and say that launching a background agent should be an explicit separate action.\n");
    prompt.push_str("If the user asks about existing agent job status, failures, logs, or what went wrong, answer from Active Agent Jobs and Recent Agent Job Events. Do not launch a new agent merely to inspect Librarian's own job state.\n");
    prompt.push_str("Keep the answer concise unless the user asks for detail.\n\n");
    if !instruction_blocks.trim().is_empty() {
        prompt.push_str("## Librarian Instruction Blocks\n\n");
        prompt.push_str(instruction_blocks.trim());
        prompt.push_str("\n\n");
    }
    prompt.push_str("You may answer directly in plain text. If and only if you need another memory search before answering, reply with a single JSON object and no prose: {\"action\":\"search_memory\",\"query\":\"short search query\",\"reason\":\"why this extra lookup is needed\"}. If you need the user to clarify, reply with {\"action\":\"clarify\",\"question\":\"your question\"}.\n\n");
    prompt.push_str("If the user asks you to perform a concrete tool action that should require approval, do not claim it is done. Reply with one JSON object and no prose: {\"action\":\"propose_tool\",\"tool\":\"library|workspace|project|agent|prompt|memory\",\"tool_action\":\"one exact action from Available Tool Actions\",\"payload\":{\"summary\":\"what would be done\"},\"reason\":\"why approval is needed\"}. The payload must be structured enough for the tool to execute after user approval. If you use JSON, it is an internal control message and will not be shown directly.\n\n");
    prompt.push_str("## Available Tool Actions\n\n");
    prompt.push_str(available_tool_actions_prompt());
    prompt.push_str("\n\n");

    prompt.push_str(&format!("## Current Scope\n\n{scope}\n\n"));
    prompt.push_str("## Active Agent Jobs\n\n");
    if job_snapshot.trim().is_empty() {
        prompt.push_str(
            "No queued, preparing, running, or heartbeat-missed agent jobs in this scope.\n\n",
        );
    } else {
        prompt.push_str(job_snapshot.trim());
        prompt.push_str("\n\n");
    }
    prompt.push_str("## Recent Agent Job Events\n\n");
    if job_event_snapshot.trim().is_empty() {
        prompt.push_str("No recent failed/cancelled/completed agent job events in this scope.\n\n");
    } else {
        prompt.push_str(job_event_snapshot.trim());
        prompt.push_str("\n\n");
    }
    prompt.push_str(&format!(
        "## Loop Budget\n\nIteration {iteration} of {max_iterations}. Stop early and answer when you have enough context.\n\n"
    ));
    if iteration >= max_iterations {
        prompt.push_str("This is the final allowed iteration. Do not request another memory search; answer with the available context or ask one clarifying question.\n\n");
    }
    if !tool_feedback.is_empty() {
        prompt.push_str("## Tool Proposal Feedback\n\n");
        for (index, feedback) in tool_feedback.iter().enumerate() {
            prompt.push_str(&format!("{}. {}\n", index + 1, feedback.trim()));
        }
        prompt.push_str("\n");
    }
    prompt.push_str("## Recent Conversation\n\n");
    let recent_turns = recent_turns
        .iter()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    if recent_turns.is_empty() {
        prompt.push_str("No prior turns in this chat session.\n\n");
    } else {
        for turn in recent_turns {
            prompt.push_str(&format!("{}: ", turn.role));
            prompt.push_str(turn.content.trim());
            prompt.push_str("\n\n");
        }
    }
    prompt.push_str("## Retrieved Memory\n\n");
    let hits = filtered_memory_hits(context_packs);
    if hits.is_empty() {
        prompt.push_str("No relevant durable memory was found.\n\n");
    } else {
        for (index, hit) in hits.iter().enumerate() {
            prompt.push_str(&format!(
                "{}. kind={:?}; score={:.3}; observed={}; id={}\n",
                index + 1,
                hit.item.kind,
                hit.score,
                hit.item.observed_at.to_rfc3339(),
                hit.item.id
            ));
            prompt.push_str(hit.item.content.trim());
            prompt.push_str("\n\n");
        }
    }

    prompt.push_str("## User Message\n\n");
    prompt.push_str(message.trim());
    prompt.push_str("\n\n## Response\n\n");
    prompt
}

fn parse_librarian_chat_directive(raw: &str) -> Option<LibrarianChatDirective> {
    let candidate = raw.trim();
    if candidate.is_empty() {
        return None;
    }
    let candidate = candidate
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .or_else(|| {
            candidate
                .strip_prefix("```")
                .and_then(|value| value.strip_suffix("```"))
        })
        .unwrap_or(candidate)
        .trim();
    if !candidate.starts_with('{') {
        return None;
    }
    serde_json::from_str(candidate).ok()
}

fn filtered_memory_hits(context_packs: &[ContextPack]) -> Vec<&MemoryHit> {
    let mut seen = HashSet::new();
    let mut hits = Vec::new();
    for pack in context_packs {
        for hit in &pack.hits {
            if is_placeholder_memory(&hit.item) || !seen.insert(hit.item.id) {
                continue;
            }
            hits.push(hit);
        }
    }
    hits.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(12);
    hits
}

fn combined_memory_hits(context_packs: &[ContextPack]) -> Vec<MemoryHit> {
    filtered_memory_hits(context_packs)
        .into_iter()
        .cloned()
        .collect()
}

async fn run_librarian_codex_chat(config: &Config, prompt: &str) -> Result<String> {
    let Some(codex_home) = &config.codex.host_home else {
        anyhow::bail!(
            "Codex chat is not configured. Run `CODEX_HOME={} codex`, then `librarian auth codex --codex-home {}`.",
            config.home.join(".cfg").join("codex-home").display(),
            config.home.join(".cfg").join("codex-home").display()
        );
    };
    if !codex_home.exists() {
        anyhow::bail!(
            "Codex profile is missing at {}. Run `CODEX_HOME={} codex` and complete sign-in.",
            codex_home.display(),
            codex_home.display()
        );
    }

    let chat_dir = config.home.join(".app").join("chat");
    std::fs::create_dir_all(&chat_dir)?;
    let output_path = chat_dir.join(format!("{}-last-message.txt", Uuid::new_v4()));
    let work_dir = if config.vault_path.exists() {
        config.vault_path.clone()
    } else {
        config.home.clone()
    };

    let mut child = TokioCommand::new("codex")
        .arg("exec")
        .arg("--json")
        .arg("--skip-git-repo-check")
        .arg("--sandbox")
        .arg("read-only")
        .arg("--cd")
        .arg(&work_dir)
        .arg("--output-last-message")
        .arg(&output_path)
        .arg("-")
        .env("CODEX_HOME", codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| anyhow::anyhow!("Failed to start Codex CLI for chat: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await?;
    }

    let timeout_seconds = config.chat.codex_timeout_seconds.max(1);
    let output = timeout(
        Duration::from_secs(timeout_seconds),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Codex chat timed out after {timeout_seconds} seconds"))??;
    let last_message = std::fs::read_to_string(&output_path).unwrap_or_default();
    let _ = std::fs::remove_file(&output_path);
    if output.status.success() {
        let reply = last_message.trim();
        if !reply.is_empty() {
            return Ok(reply.to_string());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let fallback = stdout.trim();
        if !fallback.is_empty() {
            return Ok(fallback.to_string());
        }
        anyhow::bail!("Codex chat completed but returned an empty response.");
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    anyhow::bail!(
        "Codex chat failed with status {}.\n{}\n{}",
        output.status,
        stderr.trim(),
        stdout.trim()
    );
}

fn is_placeholder_memory(item: &crate::domain::MemoryItem) -> bool {
    item.metadata
        .get("mode")
        .and_then(|value| value.as_str())
        .is_some_and(|mode| mode == "local-memory-responder")
        || item
            .content
            .starts_with("I am here as Librarian, not as a background agent runner.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, domain::MemoryKind};

    #[test]
    fn parses_internal_chat_directive_from_json_fence() {
        let directive = parse_librarian_chat_directive(
            r#"```json
{"action":"search_memory","query":"project map","reason":"need project context"}
```"#,
        )
        .expect("directive");

        assert_eq!(directive.action, "search_memory");
        assert_eq!(directive.query.as_deref(), Some("project map"));
        assert_eq!(directive.reason.as_deref(), Some("need project context"));
    }

    #[test]
    fn parses_tool_proposal_chat_directive() {
        let directive = parse_librarian_chat_directive(
            r#"{"action":"propose_tool","tool":"library","tool_action":"create_folder","payload":{"path":"projects/test"},"reason":"user asked for a project folder"}"#,
        )
        .expect("directive");

        assert_eq!(directive.action, "propose_tool");
        assert_eq!(directive.tool.as_deref(), Some("library"));
        assert_eq!(directive.tool_action.as_deref(), Some("create_folder"));
        assert_eq!(directive.payload.expect("payload")["path"], "projects/test");
    }

    #[test]
    fn validates_tool_proposal_contract() {
        let (tool, action, payload) = validate_tool_proposal(
            Some("library"),
            Some("replace-section"),
            Some(
                serde_json::json!({"path":"note.md","heading":"Plan","content":"## Plan\nDone\n"}),
            ),
        )
        .expect("proposal");
        assert_eq!(tool, "library");
        assert_eq!(action, "replace_section");
        assert_eq!(payload["heading"], "Plan");
        assert!(validate_tool_proposal(
            Some("library"),
            Some("replace-section"),
            Some(serde_json::json!({"path":"note.md"})),
        )
        .is_err());
        assert!(validate_tool_proposal(
            Some("shell"),
            Some("run"),
            Some(serde_json::json!({"command":"rm -rf /"})),
        )
        .is_err());
    }

    #[test]
    fn validates_project_creation_aliases() {
        let (tool, action, payload) = validate_tool_proposal(
            Some("project"),
            Some("create-site-library-and-project-folder"),
            Some(serde_json::json!({
                "summary": "Create site docs and workspace.",
                "name": "nomorecare.gg",
                "library_path": "sites/nomorecare.gg",
                "workspace_path": "sites/nomorecare.gg"
            })),
        )
        .expect("proposal");

        assert_eq!(tool, "project");
        assert_eq!(action, "create_site_library_and_project_folder");
        assert_eq!(payload["library_path"], "sites/nomorecare.gg");
    }

    #[test]
    fn validates_canonical_agent_launch_only() {
        let (tool, action, payload) = validate_tool_proposal(
            Some("agent"),
            Some("launch"),
            Some(serde_json::json!({
                "goal": "Clone git@github.com:no-more-care/nomorecare.gg.git into the empty workspace.",
                "allow_network": true
            })),
        )
        .expect("proposal");

        assert_eq!(tool, "agent");
        assert_eq!(action, "launch");
        assert_eq!(
            payload["goal"],
            "Clone git@github.com:no-more-care/nomorecare.gg.git into the empty workspace."
        );

        let (_, action, _) = validate_tool_proposal(
            Some("agent"),
            Some("agent.launch"),
            Some(serde_json::json!({
                "goal": "Clone git@github.com:no-more-care/nomorecare.gg.git into the empty workspace."
            })),
        )
        .expect("fully qualified action should normalize");
        assert_eq!(action, "launch");

        let error = validate_tool_proposal(
            Some("agent"),
            Some("clone_repository_into_project_folder"),
            Some(serde_json::json!({
                "goal": "Clone git@github.com:no-more-care/nomorecare.gg.git"
            })),
        )
        .expect_err("invented action must fail")
        .to_string();
        assert!(error.contains("tool_action `launch`"));
    }

    #[test]
    fn chat_prompt_lists_canonical_tool_manifest() {
        let config = Config::load_or_default(None).expect("config");
        let prompt =
            build_librarian_chat_prompt(&config, "", None, &[], &[], "", "", "", &[], 1, 5);

        assert!(prompt.contains("## Available Tool Actions"));
        assert!(prompt.contains("tool=agent, tool_action=launch"));
        assert!(prompt.contains("Do not invent action names"));
    }

    #[test]
    fn leaves_plain_chat_reply_as_final_text() {
        assert!(parse_librarian_chat_directive("Yes, I am here and I see the context.").is_none());
    }

    #[tokio::test]
    async fn chat_loop_stops_memory_search_at_iteration_budget() {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-chat-loop-{}", Uuid::new_v4()));

        {
            let mut config = Config::load_or_default(Some(home.clone())).expect("config");
            config.chat.max_iterations = 2;
            config.chat.memory_hit_limit = 3;
            config.ensure_layout().expect("layout");
            let db = Database::connect(&config).await.expect("db");
            db.migrate().await.expect("migrate");
            let memory_item = db
                .add_memory_item(
                    None,
                    None,
                    MemoryKind::Fact,
                    Some("loop-test"),
                    "The project map should be represented as a library tree.",
                    Some("test"),
                    serde_json::json!({}),
                )
                .await
                .expect("memory");
            memory::embed_item(&db, &config, &memory_item)
                .await
                .expect("embedding");
            let initial_context = ContextPack {
                query: "how should project maps look?".to_string(),
                project_id: None,
                activity_id: None,
                generated_at: chrono::Utc::now(),
                hits: Vec::new(),
            };
            let mut runner = MockChatRunner::new(vec![
                r#"{"action":"search_memory","query":"library tree project map","reason":"need durable context"}"#,
                r#"{"action":"search_memory","query":"more project map detail","reason":"still checking"}"#,
            ]);

            let result = run_librarian_chat_loop_with_runner(
                &db,
                &config,
                "how should project maps look?",
                None,
                &[],
                initial_context,
                &mut runner,
            )
            .await
            .expect("chat loop");

            assert_eq!(result.iterations, 2);
            assert_eq!(runner.calls, 2);
            assert!(result
                .reply
                .contains("The next thing I would look for is: more project map detail"));
            assert_eq!(result.trace.len(), 2);
            assert_eq!(result.trace[0]["action"], "search_memory");
            assert_eq!(result.trace[1]["query"], "more project map detail");
            assert!(runner.prompts[0].contains("Iteration 1 of 2"));
            assert!(runner.prompts[1].contains("Iteration 2 of 2"));
            assert!(runner.prompts[1].contains("This is the final allowed iteration"));
            assert!(runner.prompts[1].contains("library tree"));
        }

        std::fs::remove_dir_all(home).ok();
    }

    #[tokio::test]
    async fn chat_loop_returns_actionable_provider_unavailable_fallback() {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-chat-provider-{}", Uuid::new_v4()));

        {
            let config = Config::load_or_default(Some(home.clone())).expect("config");
            config.ensure_layout().expect("layout");
            let db = Database::connect(&config).await.expect("db");
            db.migrate().await.expect("migrate");
            let initial_context = ContextPack {
                query: "hello".to_string(),
                project_id: None,
                activity_id: None,
                generated_at: chrono::Utc::now(),
                hits: Vec::new(),
            };
            let mut runner = FailingChatRunner;

            let result = run_librarian_chat_loop_with_runner(
                &db,
                &config,
                "hello",
                None,
                &[],
                initial_context,
                &mut runner,
            )
            .await
            .expect("fallback result");

            assert_eq!(result.mode, "chat-provider-unavailable");
            assert_eq!(result.iterations, 1);
            assert!(result.reply.contains("Codex chat is not available yet"));
            assert!(result.reply.contains("CODEX_HOME="));
            assert!(result.reply.contains("auth codex --enable-container-mount"));
            assert_eq!(result.trace[0]["action"], "provider_unavailable");
            assert_eq!(result.trace[0]["provider"], "codex");
        }

        std::fs::remove_dir_all(home).ok();
    }

    #[tokio::test]
    async fn chat_loop_returns_approval_ui_metadata() {
        let home = std::env::current_dir().expect("current dir").join(format!(
            ".librarian-test-chat-approval-ui-{}",
            Uuid::new_v4()
        ));

        {
            let config = Config::load_or_default(Some(home.clone())).expect("config");
            config.ensure_layout().expect("layout");
            let db = Database::connect(&config).await.expect("db");
            db.migrate().await.expect("migrate");
            let initial_context = ContextPack {
                query: "create docs".to_string(),
                project_id: None,
                activity_id: None,
                generated_at: chrono::Utc::now(),
                hits: Vec::new(),
            };
            let mut runner = MockChatRunner::new(vec![
                r#"{"action":"propose_tool","tool":"project","tool_action":"create_starting_docs_and_project_folder","payload":{"summary":"Create starter docs.","name":"AdvenTableDays","library_path":"Games/AdvenTableDays","workspace_path":"AdvenTableDays"},"reason":"user asked to create a project"}"#,
            ]);

            let result = run_librarian_chat_loop_with_runner(
                &db,
                &config,
                "Create AdvenTable Days docs.",
                None,
                &[],
                initial_context,
                &mut runner,
            )
            .await
            .expect("chat loop");

            assert!(result.reply.contains("prepared an action"));
            let ui = result.ui.expect("ui metadata");
            assert_eq!(ui["type"], "approval");
            assert_eq!(ui["approval"]["tool"], "project");
            assert_eq!(
                ui["approval"]["payload"]["library_path"],
                "Games/AdvenTableDays"
            );
            assert!(db.list_tool_approvals(10).await.expect("approvals").len() == 1);
        }

        std::fs::remove_dir_all(home).ok();
    }

    #[tokio::test]
    async fn chat_prompt_includes_active_jobs_for_current_project() {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-chat-jobs-{}", Uuid::new_v4()));

        {
            let config = Config::load_or_default(Some(home.clone())).expect("config");
            config.ensure_layout().expect("layout");
            let db = Database::connect(&config).await.expect("db");
            db.migrate().await.expect("migrate");
            let matching_path = config.home.join("Projects").join("matching");
            let other_path = config.home.join("Projects").join("other");
            std::fs::create_dir_all(&matching_path).expect("matching workspace");
            std::fs::create_dir_all(&other_path).expect("other workspace");
            let matching_project = db
                .add_project("Matching", &matching_path)
                .await
                .expect("matching project");
            let other_project = db
                .add_project("Other", &other_path)
                .await
                .expect("other project");
            db.create_job(
                matching_project.id,
                crate::domain::ProviderKind::Codex,
                "Clone the important repository.",
                crate::domain::MountMode::ReadWrite,
                crate::domain::NetworkMode::Open,
                None,
            )
            .await
            .expect("matching job");
            db.create_job(
                other_project.id,
                crate::domain::ProviderKind::Codex,
                "Do not include this unrelated job.",
                crate::domain::MountMode::ReadWrite,
                crate::domain::NetworkMode::Open,
                None,
            )
            .await
            .expect("other job");
            let initial_context = ContextPack {
                query: "status".to_string(),
                project_id: Some(matching_project.id),
                activity_id: None,
                generated_at: chrono::Utc::now(),
                hits: Vec::new(),
            };
            let mut runner = MockChatRunner::new(vec!["Jobs noted."]);

            let result = run_librarian_chat_loop_with_runner(
                &db,
                &config,
                "what is running?",
                Some(&matching_project),
                &[],
                initial_context,
                &mut runner,
            )
            .await
            .expect("chat loop");

            assert_eq!(result.iterations, 1);
            assert!(runner.prompts[0].contains("## Active Agent Jobs"));
            assert!(runner.prompts[0].contains("Clone the important repository."));
            assert!(!runner.prompts[0].contains("Do not include this unrelated job."));
        }

        std::fs::remove_dir_all(home).ok();
    }

    #[tokio::test]
    async fn chat_prompt_includes_recent_job_failure_events_for_current_project() {
        let home = std::env::current_dir().expect("current dir").join(format!(
            ".librarian-test-chat-job-events-{}",
            Uuid::new_v4()
        ));

        {
            let config = Config::load_or_default(Some(home.clone())).expect("config");
            config.ensure_layout().expect("layout");
            let db = Database::connect(&config).await.expect("db");
            db.migrate().await.expect("migrate");
            let workspace_path = config
                .home
                .join("Projects")
                .join("sites")
                .join("nomorecare.gg");
            std::fs::create_dir_all(&workspace_path).expect("workspace");
            let project = db
                .add_project("Nomorecare Gg", &workspace_path)
                .await
                .expect("project");
            let job = db
                .create_job(
                    project.id,
                    crate::domain::ProviderKind::Codex,
                    "Clone git@github.com:no-more-care/nomorecare.gg.git into the workspace.",
                    crate::domain::MountMode::ReadWrite,
                    crate::domain::NetworkMode::Open,
                    None,
                )
                .await
                .expect("job");
            db.add_job_event(
                job.id,
                "stderr",
                serde_json::json!({
                    "line": "permission denied while trying to connect to the docker API at unix:///var/run/docker.sock"
                }),
            )
            .await
            .expect("stderr event");
            db.update_job_status(job.id, crate::domain::JobStatus::Failed)
                .await
                .expect("failed status");
            let initial_context = ContextPack {
                query: "what failed?".to_string(),
                project_id: Some(project.id),
                activity_id: None,
                generated_at: chrono::Utc::now(),
                hits: Vec::new(),
            };
            let mut runner = MockChatRunner::new(vec!["The Docker socket was not accessible."]);

            let result = run_librarian_chat_loop_with_runner(
                &db,
                &config,
                "Посмотри, в чём была проблема",
                Some(&project),
                &[],
                initial_context,
                &mut runner,
            )
            .await
            .expect("chat loop");

            assert_eq!(result.iterations, 1);
            assert!(runner.prompts[0].contains("## Recent Agent Job Events"));
            assert!(runner.prompts[0].contains("docker.sock"));
            assert!(runner.prompts[0]
                .contains("Do not launch a new agent merely to inspect Librarian's own job state"));
        }

        std::fs::remove_dir_all(home).ok();
    }

    #[tokio::test]
    async fn chat_loop_feedback_recovers_invalid_tool_proposal() {
        let home = std::env::current_dir().expect("current dir").join(format!(
            ".librarian-test-chat-tool-feedback-{}",
            Uuid::new_v4()
        ));

        {
            let mut config = Config::load_or_default(Some(home.clone())).expect("config");
            config.chat.max_iterations = 3;
            config.ensure_layout().expect("layout");
            let db = Database::connect(&config).await.expect("db");
            db.migrate().await.expect("migrate");
            let workspace_path = config
                .home
                .join("Projects")
                .join("sites")
                .join("nomorecare.gg");
            std::fs::create_dir_all(&workspace_path).expect("workspace");
            let project = db
                .add_project("Nomorecare Gg", &workspace_path)
                .await
                .expect("project");
            let initial_context = ContextPack {
                query: "clone repo".to_string(),
                project_id: Some(project.id),
                activity_id: None,
                generated_at: chrono::Utc::now(),
                hits: Vec::new(),
            };
            let mut runner = MockChatRunner::new(vec![
                r#"{"action":"propose_tool","tool":"agent","tool_action":"clone_repository_into_project_folder","payload":{"goal":"Clone git@github.com:no-more-care/nomorecare.gg.git into the current empty workspace.","allow_network":true},"reason":"user asked to clone a repo"}"#,
                r#"{"action":"propose_tool","tool":"agent","tool_action":"launch","payload":{"goal":"Clone git@github.com:no-more-care/nomorecare.gg.git into the current empty workspace.","allow_network":true},"reason":"corrected to canonical agent launch"}"#,
            ]);

            let result = run_librarian_chat_loop_with_runner(
                &db,
                &config,
                "Clone the repo into this project.",
                Some(&project),
                &[],
                initial_context,
                &mut runner,
            )
            .await
            .expect("chat loop");

            assert_eq!(result.iterations, 2);
            assert_eq!(runner.calls, 2);
            assert_eq!(result.trace[0]["action"], "tool_proposal_feedback");
            assert!(runner.prompts[1].contains("## Tool Proposal Feedback"));
            assert!(runner.prompts[1].contains("Previous tool proposal failed validation"));
            assert!(result.reply.contains("prepared an action"));
            let ui = result.ui.expect("ui metadata");
            assert_eq!(ui["approval"]["tool"], "agent");
            assert_eq!(ui["approval"]["action"], "launch");
            assert_eq!(ui["approval"]["payload"]["project"], "Nomorecare Gg");
            assert!(db.list_tool_approvals(10).await.expect("approvals").len() == 1);
        }

        std::fs::remove_dir_all(home).ok();
    }

    #[test]
    fn filters_placeholder_chat_memory_from_context_hits() {
        let pack = ContextPack {
            query: "plain memory".to_string(),
            project_id: None,
            activity_id: None,
            generated_at: chrono::Utc::now(),
            hits: vec![
                test_memory_hit(
                    "I am here as Librarian, not as a background agent runner.",
                    serde_json::json!({}),
                    0.9,
                ),
                test_memory_hit(
                    "old local responder echo",
                    serde_json::json!({ "mode": "local-memory-responder" }),
                    0.8,
                ),
                test_memory_hit(
                    "Useful project context that should survive filtering.",
                    serde_json::json!({ "mode": "librarian-chat" }),
                    0.7,
                ),
            ],
        };

        let packs = [pack];
        let hits = filtered_memory_hits(&packs);

        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].item.content,
            "Useful project context that should survive filtering."
        );
    }

    #[test]
    fn chat_prompt_includes_recent_conversation_turns() {
        let mut config = Config::load_or_default(Some(
            std::env::current_dir()
                .expect("current dir")
                .join(format!(".librarian-test-chat-prompt-{}", Uuid::new_v4())),
        ))
        .expect("config");
        config.chat.assistant_name = "Sage".to_string();
        let session_id = Uuid::new_v4();
        let recent_turns = vec![
            test_chat_turn(session_id, 1, "user", "We are designing a library map."),
            test_chat_turn(
                session_id,
                2,
                "assistant",
                "Use shelves for folders and books for notes.",
            ),
        ];
        let prompt = build_librarian_chat_prompt(
            &config,
            "What should the next UI step be?",
            None,
            &recent_turns,
            &[ContextPack {
                query: "library map".to_string(),
                project_id: None,
                activity_id: None,
                generated_at: chrono::Utc::now(),
                hits: Vec::new(),
            }],
            "",
            "",
            "",
            &[],
            1,
            5,
        );

        assert!(prompt.contains("## Recent Conversation"));
        assert!(prompt.contains("You are Sage:"));
        assert!(prompt.contains("user: We are designing a library map."));
        assert!(prompt.contains("assistant: Use shelves for folders and books for notes."));
        assert!(prompt.contains("## User Message"));
        assert!(prompt.contains("What should the next UI step be?"));
    }

    fn test_memory_hit(
        content: &str,
        metadata: serde_json::Value,
        score: f64,
    ) -> crate::domain::MemoryHit {
        let now = chrono::Utc::now();
        crate::domain::MemoryHit {
            item: crate::domain::MemoryItem {
                id: Uuid::new_v4(),
                project_id: None,
                activity_id: None,
                kind: MemoryKind::AssistantMessage,
                topic: Some("test".to_string()),
                content: content.to_string(),
                source: Some("test".to_string()),
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
            },
            score,
            semantic_score: score,
            lexical_score: 0.0,
            recency_score: 0.0,
            scope_score: 0.0,
            reason: "test".to_string(),
        }
    }

    fn test_chat_turn(session_id: Uuid, turn_index: i64, role: &str, content: &str) -> ChatTurn {
        ChatTurn {
            id: Uuid::new_v4(),
            session_id,
            turn_index,
            role: role.to_string(),
            content: content.to_string(),
            memory_id: None,
            metadata: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        }
    }

    struct MockChatRunner {
        replies: std::collections::VecDeque<String>,
        prompts: Vec<String>,
        calls: usize,
    }

    impl MockChatRunner {
        fn new(replies: Vec<&str>) -> Self {
            Self {
                replies: replies.into_iter().map(str::to_string).collect(),
                prompts: Vec::new(),
                calls: 0,
            }
        }
    }

    #[async_trait]
    impl LibrarianChatRunner for MockChatRunner {
        async fn run(&mut self, _config: &Config, prompt: &str) -> Result<String> {
            self.calls += 1;
            self.prompts.push(prompt.to_string());
            self.replies
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("mock chat runner exhausted"))
        }
    }

    struct FailingChatRunner;

    #[async_trait]
    impl LibrarianChatRunner for FailingChatRunner {
        async fn run(&mut self, _config: &Config, _prompt: &str) -> Result<String> {
            anyhow::bail!("codex profile missing for test")
        }
    }
}
