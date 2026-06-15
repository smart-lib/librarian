use super::*;

pub(super) async fn execute_approval_slash_command(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            approval_slash_help(),
            serde_json::json!({ "command": "approval" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            approval_slash_help(),
            serde_json::json!({ "tool": "approval", "command": command }),
        ),
        "list" => {
            let limit = args
                .get(1)
                .map(|value| value.parse::<i64>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid limit: {error}"))?
                .unwrap_or(10)
                .clamp(1, 50);
            let approvals = state.db.list_tool_approvals(limit).await?;
            let mut reply = format!("Tool approvals: {} item(s).", approvals.len());
            for approval in &approvals {
                reply.push_str(&format!("\n{}", approval_summary_line(approval)));
            }
            slash_reply(
                &reply,
                serde_json::json!({ "tool": "approval", "command": command, "approvals": approvals }),
            )
        }
        "propose" => {
            if args.len() < 4 {
                anyhow::bail!("Usage: /approval propose <tool> <action> <payload-json>");
            }
            let payload = parse_json_payload(&args[3..].join(" "))?;
            let approval = state
                .db
                .create_tool_approval(&args[1], &args[2], payload)
                .await?;
            state
                .db
                .add_system_event(
                    "tool_approval",
                    serde_json::json!({
                        "action": "propose",
                        "approval_id": approval.id,
                        "tool": approval.tool,
                        "tool_action": approval.action,
                    }),
                )
                .await?;
            slash_reply_with_ui(
                "Review this proposed action.",
                serde_json::json!({ "tool": "approval", "command": command, "approval": approval }),
                serde_json::json!({
                    "type": "approval",
                    "approval": approval,
                }),
            )
        }
        "approve" => {
            let id = slash_approval_id_arg(args, "/approval approve <approval-id>")?;
            let (approval, output) = approve_and_execute_tool_approval(state, config, id).await?;
            slash_reply(
                &format!("Approved and executed tool proposal {}.", approval.id),
                serde_json::json!({
                    "tool": "approval",
                    "command": command,
                    "approval": approval,
                    "output": output,
                }),
            )
        }
        "reject" => {
            let id = slash_approval_id_arg(args, "/approval reject <approval-id>")?;
            let approval = reject_tool_approval_by_id(state, id).await?;
            slash_reply(
                &format!("Rejected tool proposal {}.", approval.id),
                serde_json::json!({ "tool": "approval", "command": command, "approval": approval }),
            )
        }
        "execute" => {
            let id = slash_approval_id_arg(args, "/approval execute <approval-id>")?;
            let approval = state.db.get_tool_approval(id).await?;
            if approval.status != ToolApprovalStatus::Approved {
                anyhow::bail!(
                    "Approval `{}` must be approved before execution; current status is {:?}",
                    approval.id,
                    approval.status
                );
            }
            let output = execute_approved_tool_approval(state, config, &approval).await?;
            let approval = state
                .db
                .update_tool_approval_status(id, ToolApprovalStatus::Executed)
                .await?;
            state
                .db
                .add_system_event(
                    "tool_approval",
                    serde_json::json!({
                        "action": "execute",
                        "approval_id": approval.id,
                        "tool": approval.tool,
                        "tool_action": approval.action,
                        "output": output,
                    }),
                )
                .await?;
            slash_reply(
                &format!("Executed approved tool proposal {}.", approval.id),
                serde_json::json!({
                    "tool": "approval",
                    "command": command,
                    "approval": approval,
                    "output": output,
                }),
            )
        }
        _ => slash_reply(
            "Unknown approval command. Try /approval help.",
            serde_json::json!({ "tool": "approval", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

pub(super) async fn approve_and_execute_tool_approval(
    state: &AppState,
    config: &Config,
    id: Uuid,
) -> Result<(crate::domain::ToolApproval, serde_json::Value)> {
    let approval = state
        .db
        .update_tool_approval_status(id, ToolApprovalStatus::Approved)
        .await?;
    let output = execute_approved_tool_approval(state, config, &approval).await?;
    let approval = state
        .db
        .update_tool_approval_status(id, ToolApprovalStatus::Executed)
        .await?;
    state
        .db
        .add_system_event(
            "tool_approval",
            serde_json::json!({
                "action": "approve_and_execute",
                "approval_id": approval.id,
                "tool": approval.tool,
                "tool_action": approval.action,
                "output": output,
            }),
        )
        .await?;
    Ok((approval, output))
}

pub(super) async fn reject_tool_approval_by_id(
    state: &AppState,
    id: Uuid,
) -> Result<crate::domain::ToolApproval> {
    let approval = state
        .db
        .update_tool_approval_status(id, ToolApprovalStatus::Rejected)
        .await?;
    state
        .db
        .add_system_event(
            "tool_approval",
            serde_json::json!({ "action": "reject", "approval_id": approval.id }),
        )
        .await?;
    Ok(approval)
}

fn slash_approval_id_arg(args: &[String], usage: &str) -> Result<Uuid> {
    args.get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))?
        .parse::<Uuid>()
        .map_err(|error| anyhow::anyhow!("Invalid approval id: {error}"))
}

pub(super) fn parse_json_payload(value: &str) -> Result<serde_json::Value> {
    serde_json::from_str(value).map_err(|error| anyhow::anyhow!("Invalid JSON payload: {error}"))
}

fn approval_summary_line(approval: &crate::domain::ToolApproval) -> String {
    let summary = approval
        .payload
        .get("summary")
        .and_then(|value| value.as_str())
        .or_else(|| {
            approval
                .payload
                .get("path")
                .and_then(|value| value.as_str())
        })
        .or_else(|| {
            approval
                .payload
                .get("library_path")
                .and_then(|value| value.as_str())
        })
        .unwrap_or("No summary");
    format!(
        "{} {:?} {}.{} - {}",
        approval.id, approval.status, approval.tool, approval.action, summary
    )
}

pub(super) async fn execute_approved_tool_approval(
    state: &AppState,
    config: &Config,
    approval: &crate::domain::ToolApproval,
) -> Result<serde_json::Value> {
    let tool = approval.tool.trim().to_ascii_lowercase();
    let action = approval.action.trim().to_ascii_lowercase();
    match (tool.as_str(), action.as_str()) {
        ("library", "create_folder" | "mkdir") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.create",
                config.tool_permissions.library_create,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let tool_path = library_tools::create_folder(config, LibraryRoot::Library, &path)?;
            log_slash_library_event(
                &state.db,
                "create_folder",
                serde_json::json!({ "root": "library", "path": tool_path.path }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path }))
        }
        ("library", "create_file" | "touch") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.create",
                config.tool_permissions.library_create,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let tool_path = library_tools::create_empty_file(config, LibraryRoot::Library, &path)?;
            log_slash_library_event(
                &state.db,
                "create_empty_file",
                serde_json::json!({ "root": "library", "path": tool_path.path }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path }))
        }
        ("library", "write_markdown" | "write") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let content = approval_payload_string(&approval.payload, "content")?;
            let tool_path = library_tools::write_markdown(config, &path, &content)?;
            log_slash_library_event(
                &state.db,
                "write_markdown",
                serde_json::json!({ "root": "library", "path": tool_path.path }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path }))
        }
        ("library", "append_markdown" | "append") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let content = approval_payload_string(&approval.payload, "content")?;
            let tool_path = library_tools::append_markdown(config, &path, &content)?;
            log_slash_library_event(
                &state.db,
                "append_markdown",
                serde_json::json!({ "root": "library", "path": tool_path.path }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path }))
        }
        ("library", "move" | "rename") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.move",
                config.tool_permissions.library_move,
            )
            .await?;
            let from = approval_payload_string_any(&approval.payload, &["from", "from_path"])?;
            let to = approval_payload_string_any(&approval.payload, &["to", "to_path"])?;
            let tool_path = library_tools::move_path(config, LibraryRoot::Library, &from, &to)?;
            log_slash_library_event(
                &state.db,
                "move",
                serde_json::json!({ "root": "library", "from": from, "to": tool_path.path }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path }))
        }
        ("library", "delete") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.delete",
                config.tool_permissions.library_delete,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let recursive = approval_payload_bool(&approval.payload, "recursive").unwrap_or(false);
            let tool_path =
                library_tools::delete_path(config, LibraryRoot::Library, &path, recursive)?;
            log_slash_library_event(
                &state.db,
                "delete",
                serde_json::json!({ "root": "library", "path": tool_path.path, "recursive": recursive }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path, "recursive": recursive }))
        }
        ("library", "replace_lines" | "replace-lines") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let start_line = approval_payload_usize(&approval.payload, "start_line")?;
            let end_line = approval_payload_usize(&approval.payload, "end_line")?;
            let content = approval_payload_string(&approval.payload, "content")?;
            let edit = library_tools::replace_markdown_lines(
                config, &path, start_line, end_line, &content,
            )?;
            log_slash_library_event(
                &state.db,
                "replace_markdown_lines",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            Ok(serde_json::json!({ "edit": edit }))
        }
        ("library", "cut_lines" | "cut-lines") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let start_line = approval_payload_usize(&approval.payload, "start_line")?;
            let end_line = approval_payload_usize(&approval.payload, "end_line")?;
            let edit = library_tools::cut_markdown_lines(config, &path, start_line, end_line)?;
            log_slash_library_event(
                &state.db,
                "cut_markdown_lines",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            Ok(serde_json::json!({ "edit": edit }))
        }
        ("library", "replace_find" | "replace-find") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let query = approval_payload_string(&approval.payload, "query")?;
            let content = approval_payload_string(&approval.payload, "content")?;
            let edit =
                library_tools::replace_first_markdown_match(config, &path, &query, &content)?;
            log_slash_library_event(
                &state.db,
                "replace_markdown_match",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            Ok(serde_json::json!({ "edit": edit }))
        }
        ("library", "cut_find" | "cut-find") => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let query = approval_payload_string(&approval.payload, "query")?;
            let edit = library_tools::cut_first_markdown_match(config, &path, &query)?;
            log_slash_library_event(
                &state.db,
                "cut_markdown_match",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            Ok(serde_json::json!({ "edit": edit }))
        }
        ("workspace", "create_folder" | "mkdir") => {
            ensure_tool_permission(
                &state.db,
                config,
                "workspace.create",
                config.tool_permissions.workspace_create,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let tool_path = library_tools::create_folder(config, LibraryRoot::Projects, &path)?;
            log_workspace_event(
                &state.db,
                "create_folder",
                serde_json::json!({ "root": "projects", "path": tool_path.path }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path }))
        }
        ("workspace", "create_file" | "touch") => {
            ensure_tool_permission(
                &state.db,
                config,
                "workspace.create",
                config.tool_permissions.workspace_create,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let tool_path = library_tools::create_empty_file(config, LibraryRoot::Projects, &path)?;
            log_workspace_event(
                &state.db,
                "create_empty_file",
                serde_json::json!({ "root": "projects", "path": tool_path.path }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path }))
        }
        ("workspace", "move" | "rename") => {
            ensure_tool_permission(
                &state.db,
                config,
                "workspace.move",
                config.tool_permissions.workspace_move,
            )
            .await?;
            let from = approval_payload_string_any(&approval.payload, &["from", "from_path"])?;
            let to = approval_payload_string_any(&approval.payload, &["to", "to_path"])?;
            let tool_path = library_tools::move_path(config, LibraryRoot::Projects, &from, &to)?;
            log_workspace_event(
                &state.db,
                "move",
                serde_json::json!({ "root": "projects", "from": from, "to": tool_path.path }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path }))
        }
        ("workspace", "delete") => {
            ensure_tool_permission(
                &state.db,
                config,
                "workspace.delete",
                config.tool_permissions.workspace_delete,
            )
            .await?;
            let path = approval_payload_string(&approval.payload, "path")?;
            let recursive = approval_payload_bool(&approval.payload, "recursive").unwrap_or(false);
            let tool_path =
                library_tools::delete_path(config, LibraryRoot::Projects, &path, recursive)?;
            log_workspace_event(
                &state.db,
                "delete",
                serde_json::json!({ "root": "projects", "path": tool_path.path, "recursive": recursive }),
            )
            .await?;
            Ok(serde_json::json!({ "path": tool_path, "recursive": recursive }))
        }
        ("project", action) if is_project_creation_action(action) => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.create",
                config.tool_permissions.library_create,
            )
            .await?;
            ensure_tool_permission(
                &state.db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            ensure_tool_permission(
                &state.db,
                config,
                "workspace.create",
                config.tool_permissions.workspace_create,
            )
            .await?;

            let library_path = approval_project_library_path(&approval.payload)?;
            let name = approval_payload_optional_string(&approval.payload, "name")
                .or_else(|| library_path.rsplit('/').next().map(ToOwned::to_owned))
                .unwrap_or_else(|| "Project".to_string());
            let workspace_relative =
                approval_payload_optional_string(&approval.payload, "workspace_path")
                    .map(|path| normalize_project_payload_path(&path))
                    .transpose()?
                    .unwrap_or_else(|| project_workspace_folder_name(&name));

            let library_folder =
                library_tools::create_folder(config, LibraryRoot::Library, &library_path)?;
            let workspace_folder =
                library_tools::create_folder(config, LibraryRoot::Projects, &workspace_relative)?;
            let overview_path = format!("{}/Overview.md", library_path.trim_end_matches('/'));
            let overview_content =
                approval_starting_doc_content(&name, &approval.payload, &workspace_relative);
            let overview_file =
                library_tools::write_markdown(config, &overview_path, &overview_content)?;
            let project = state
                .db
                .add_project(
                    &name,
                    &config.home.join("Projects").join(&workspace_relative),
                )
                .await?;
            let project = state
                .db
                .attach_project_library_path(project.id, PathBuf::from(&library_path).as_path())
                .await?;
            log_project_event(
                &state.db,
                "create_starting_docs_and_project_folder",
                serde_json::json!({
                    "approval_id": approval.id,
                    "project_id": project.id,
                    "name": project.name,
                    "library_path": project.library_path,
                    "workspace_path": project.path,
                    "overview": overview_file.path,
                }),
            )
            .await?;
            Ok(serde_json::json!({
                "project": project,
                "library_folder": library_folder,
                "workspace_folder": workspace_folder,
                "overview": overview_file,
            }))
        }
        ("memory", "remember" | "add") => {
            ensure_tool_permission(
                &state.db,
                config,
                "memory.write",
                config.tool_permissions.memory_write,
            )
            .await?;
            let kind = approval
                .payload
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .map(parse_memory_kind_token)
                .transpose()?
                .unwrap_or(MemoryKind::Fact);
            let content = approval_payload_string(&approval.payload, "content")?;
            let item = state
                .db
                .add_memory_item(
                    None,
                    None,
                    kind.clone(),
                    None,
                    &content,
                    Some("admin:approval-execute"),
                    serde_json::json!({
                        "approval_id": approval.id,
                        "tool": approval.tool,
                        "action": approval.action,
                    }),
                )
                .await?;
            memory::embed_item(&state.db, config, &item).await?;
            Ok(serde_json::json!({ "memory_id": item.id, "kind": item.kind }))
        }
        ("prompt", "add_block" | "add-block" | "add") => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            let target = approval_payload_string(&approval.payload, "target")?;
            let name = approval_payload_string(&approval.payload, "name")?;
            let content = approval_payload_string(&approval.payload, "content")?;
            let markdown = approval
                .payload
                .get("markdown")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let block = state
                .db
                .create_prompt_block(&target, &name, &content, markdown)
                .await?;
            Ok(serde_json::json!({ "block_id": block.id, "target": block.target }))
        }
        ("agent", "launch" | "queue") => {
            ensure_tool_permission(
                &state.db,
                config,
                "agent.launch",
                config.tool_permissions.agent_launch,
            )
            .await?;
            let project =
                approval_payload_string_any(&approval.payload, &["project", "chat_scope"])?;
            let goal = approval_payload_string(&approval.payload, "goal")?;
            let provider = approval_payload_optional_string(&approval.payload, "provider")
                .map(|value| router::parse_provider_kind(&value))
                .transpose()?
                .unwrap_or(crate::domain::ProviderKind::Codex);
            let secret_grant_token =
                approval_payload_optional_string(&approval.payload, "secret_grant_token");
            let request = AgentLaunchRequest {
                project,
                goal,
                provider,
                secret_grant_token,
                allow_network: approval_payload_bool(&approval.payload, "allow_network")
                    .unwrap_or(false),
                read_only: approval_payload_bool(&approval.payload, "read_only").unwrap_or(false),
            };
            let goal = request.goal.clone();
            let (job, project) = queue_agent_launch(state, config, request, "approval").await?;
            Ok(serde_json::json!({
                "job": job,
                "project": project,
                "goal": goal,
            }))
        }
        ("git", "commit") => execute_git_commit_approval(state, approval).await,
        ("git", "revert") => execute_git_revert_approval(state, approval).await,
        ("git", "push") => {
            anyhow::bail!("git.push approvals are not executable yet; run `jobs gate <job-id> --action push` and push manually after review")
        }
        ("context", "switch") => {
            let label = approval_payload_string(&approval.payload, "label")?;
            state
                .db
                .add_system_event(
                    "context_tool",
                    serde_json::json!({
                        "action": "switch",
                        "approval_id": approval.id,
                        "label": label,
                        "scope": approval.payload.get("scope").cloned(),
                        "nodes": approval.payload.get("nodes").cloned(),
                    }),
                )
                .await?;
            Ok(serde_json::json!({
                "context": {
                    "label": label,
                    "scope": approval.payload.get("scope").cloned(),
                    "nodes": approval.payload.get("nodes").cloned().unwrap_or_else(|| serde_json::json!([])),
                }
            }))
        }
        _ => anyhow::bail!(
            "Approval executor does not allow `{}` `{}` yet",
            approval.tool,
            approval.action
        ),
    }
}

fn is_project_creation_action(action: &str) -> bool {
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

pub(super) fn approval_payload_string(payload: &serde_json::Value, key: &str) -> Result<String> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("Approval payload must contain non-empty string `{key}`"))
}

fn approval_payload_string_any(payload: &serde_json::Value, keys: &[&str]) -> Result<String> {
    for key in keys {
        if let Some(value) = approval_payload_optional_string(payload, key) {
            return Ok(value);
        }
    }
    anyhow::bail!("Approval payload must contain one of: {}", keys.join(", "))
}

fn approval_payload_optional_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn approval_payload_bool(payload: &serde_json::Value, key: &str) -> Option<bool> {
    payload.get(key).and_then(serde_json::Value::as_bool)
}

fn approval_payload_usize(payload: &serde_json::Value, key: &str) -> Result<usize> {
    let value = payload
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("Approval payload must contain `{key}`"))?;
    if let Some(number) = value.as_u64() {
        return usize::try_from(number)
            .map_err(|error| anyhow::anyhow!("Invalid `{key}` value: {error}"));
    }
    if let Some(text) = value.as_str() {
        return text
            .trim()
            .parse::<usize>()
            .map_err(|error| anyhow::anyhow!("Invalid `{key}` value: {error}"));
    }
    anyhow::bail!("Approval payload `{key}` must be a positive integer")
}

async fn execute_git_commit_approval(
    state: &AppState,
    approval: &crate::domain::ToolApproval,
) -> Result<serde_json::Value> {
    let job_id = approval
        .payload
        .get("job_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Git approval payload must contain `job_id`"))?
        .parse::<Uuid>()
        .map_err(|error| anyhow::anyhow!("Invalid git approval job_id: {error}"))?;
    let message = approval_payload_string(&approval.payload, "message")?;
    let job = state.db.get_job(job_id).await?;
    let project = state.db.get_project_by_id(job.project_id).await?;
    let project_path = project
        .path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", project.path.display()))?;
    let branch =
        run_approval_project_command(&project_path, "git", &["branch", "--show-current"]).await?;
    let status = run_approval_project_command(&project_path, "git", &["status", "--short"]).await?;
    let branch_name = branch.stdout.trim().to_string();
    let mut blockers = Vec::new();
    if !project.git_policy.allow_commit {
        blockers.push("project_policy_disallows_commit".to_string());
    }
    if branch_name.is_empty() {
        blockers.push("detached_or_unknown_branch".to_string());
    }
    if project
        .git_policy
        .protected_branches
        .iter()
        .any(|protected| protected == &branch_name)
    {
        blockers.push(format!("protected_branch:{branch_name}"));
    }
    if let Some(pattern) = &project.git_policy.require_branch_pattern {
        if !approval_branch_pattern_matches(pattern, &branch_name) {
            blockers.push(format!("branch_does_not_match_required_pattern:{pattern}"));
        }
    }
    if status.stdout.trim().is_empty() {
        blockers.push("no_worktree_changes_to_commit".to_string());
    }
    if !blockers.is_empty() {
        anyhow::bail!("Git commit approval blocked: {}", blockers.join(", "));
    }

    let add = run_approval_project_command(&project_path, "git", &["add", "-A"]).await?;
    if !add.success {
        anyhow::bail!("git add failed: {}", add.stderr);
    }
    let commit =
        run_approval_project_command(&project_path, "git", &["commit", "-m", &message]).await?;
    if !commit.success {
        anyhow::bail!("git commit failed: {}", commit.stderr);
    }
    let after_status =
        run_approval_project_command(&project_path, "git", &["status", "--short"]).await?;
    state
        .db
        .add_job_event(
            job.id,
            "git_commit",
            serde_json::json!({
                "approval_id": approval.id,
                "message": message,
                "branch": branch_name,
                "commit": commit,
                "status_after": after_status,
            }),
        )
        .await?;
    Ok(serde_json::json!({
        "job_id": job.id,
        "project": project.name,
        "branch": branch_name,
        "commit": commit,
        "status_after": after_status,
    }))
}

async fn execute_git_revert_approval(
    state: &AppState,
    approval: &crate::domain::ToolApproval,
) -> Result<serde_json::Value> {
    let job_id = approval
        .payload
        .get("job_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Git approval payload must contain `job_id`"))?
        .parse::<Uuid>()
        .map_err(|error| anyhow::anyhow!("Invalid git approval job_id: {error}"))?;
    let commit = approval_payload_string(&approval.payload, "commit")?;
    let job = state.db.get_job(job_id).await?;
    let project = state.db.get_project_by_id(job.project_id).await?;
    let project_path = project
        .path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", project.path.display()))?;
    let branch =
        run_approval_project_command(&project_path, "git", &["branch", "--show-current"]).await?;
    let status = run_approval_project_command(&project_path, "git", &["status", "--short"]).await?;
    let target = run_approval_project_command(
        &project_path,
        "git",
        &["show", "--quiet", "--format=%H%n%s", &commit],
    )
    .await?;
    let branch_name = branch.stdout.trim().to_string();
    let mut blockers = Vec::new();
    if !project.git_policy.allow_commit {
        blockers.push("project_policy_disallows_commit".to_string());
    }
    if branch_name.is_empty() {
        blockers.push("detached_or_unknown_branch".to_string());
    }
    if project
        .git_policy
        .protected_branches
        .iter()
        .any(|protected| protected == &branch_name)
    {
        blockers.push(format!("protected_branch:{branch_name}"));
    }
    if let Some(pattern) = &project.git_policy.require_branch_pattern {
        if !approval_branch_pattern_matches(pattern, &branch_name) {
            blockers.push(format!("branch_does_not_match_required_pattern:{pattern}"));
        }
    }
    if !status.stdout.trim().is_empty() {
        blockers.push("worktree_has_uncommitted_changes".to_string());
    }
    if !target.success {
        blockers.push("target_commit_not_found".to_string());
    }
    if !blockers.is_empty() {
        anyhow::bail!("Git revert approval blocked: {}", blockers.join(", "));
    }

    let revert =
        run_approval_project_command(&project_path, "git", &["revert", "--no-edit", &commit])
            .await?;
    if !revert.success {
        anyhow::bail!("git revert failed: {}", revert.stderr);
    }
    let after_status =
        run_approval_project_command(&project_path, "git", &["status", "--short"]).await?;
    state
        .db
        .add_job_event(
            job.id,
            "git_revert",
            serde_json::json!({
                "approval_id": approval.id,
                "target_commit": commit,
                "branch": branch_name,
                "revert": revert,
                "status_after": after_status,
            }),
        )
        .await?;
    Ok(serde_json::json!({
        "job_id": job.id,
        "project": project.name,
        "branch": branch_name,
        "target_commit": commit,
        "revert": revert,
        "status_after": after_status,
    }))
}

#[derive(Clone, Debug, serde::Serialize)]
struct ApprovalProjectCommandOutput {
    command: String,
    status: Option<i32>,
    success: bool,
    stdout: String,
    stderr: String,
}

async fn run_approval_project_command(
    project_path: &Path,
    command: &str,
    args: &[&str],
) -> Result<ApprovalProjectCommandOutput> {
    let output = TokioCommand::new(command)
        .args(args)
        .current_dir(project_path)
        .output()
        .await
        .with_context(|| format!("Failed to run `{command}` in {}", project_path.display()))?;
    Ok(ApprovalProjectCommandOutput {
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

fn approval_branch_pattern_matches(pattern: &str, branch: &str) -> bool {
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

pub(super) fn approval_project_library_path(payload: &serde_json::Value) -> Result<String> {
    if let Some(path) = approval_payload_optional_string(payload, "library_path") {
        return normalize_library_payload_path(&path);
    }
    if let Some(message) = approval_payload_optional_string(payload, "user_message")
        .or_else(|| approval_payload_optional_string(payload, "summary"))
    {
        if let Some(path) = extract_librarian_path_hint(&message, "/Library/") {
            return normalize_library_payload_path(&path);
        }
    }
    anyhow::bail!("Approval payload must contain `library_path` for project documentation")
}

fn normalize_library_payload_path(path: &str) -> Result<String> {
    let trimmed = path.trim().trim_matches('`').trim_matches('"').trim();
    let relative = trimmed
        .strip_prefix("/Library/")
        .or_else(|| trimmed.strip_prefix("Library/"))
        .unwrap_or(trimmed);
    library_tools::normalize_tool_relative_path(relative.trim_matches('/'))
}

fn normalize_project_payload_path(path: &str) -> Result<String> {
    let trimmed = path.trim().trim_matches('`').trim_matches('"').trim();
    let relative = trimmed
        .strip_prefix("/Projects/")
        .or_else(|| trimmed.strip_prefix("Projects/"))
        .unwrap_or(trimmed);
    library_tools::normalize_tool_relative_path(relative.trim_matches('/'))
}

fn extract_librarian_path_hint(text: &str, prefix: &str) -> Option<String> {
    let start = text.find(prefix)?;
    let tail = &text[start..];
    let end = tail
        .find(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';' | ')' | ']' | '}'))
        .unwrap_or(tail.len());
    Some(tail[..end].trim_matches(['.', '`', '"', '\'']).to_string())
}

fn approval_starting_doc_content(
    name: &str,
    payload: &serde_json::Value,
    workspace_relative: &str,
) -> String {
    let summary = approval_payload_optional_string(payload, "summary")
        .unwrap_or_else(|| "Starting project documentation.".to_string());
    format!(
        "# {name}\n\n## Summary\n\n{summary}\n\n## Workspace\n\n`Projects/{workspace_relative}`\n\n## Notes\n\n- Initial documentation created from an approved Librarian chat proposal.\n"
    )
}

fn approval_slash_help() -> &'static str {
    "Approval commands live under /approval:\n/approval list [limit]\n/approval propose <tool> <action> <payload-json>\n/approval approve <approval-id>\n/approval reject <approval-id>\n/approval execute <approval-id>\n\n/approval approve approves and executes whitelisted actions. /approval execute is kept for already approved proposals."
}
