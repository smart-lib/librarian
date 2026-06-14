use super::*;

pub async fn run_dialogue_context_smoke(config: &Config, name: &str) -> Result<()> {
    let slug = name
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let slug = if slug.is_empty() {
        "dialogue-context-smoke".to_string()
    } else {
        slug
    };
    let library_path = format!("context-smoke/{slug}/DialogueNode");
    library_tools::create_folder(config, LibraryRoot::Library, &library_path)?;
    let db = Database::connect(config).await?;
    db.migrate().await?;

    let state = AppState {
        db,
        config: Arc::new(RwLock::new(config.clone())),
    };
    let ask_context = resolve_chat_project_context(
        &state,
        config,
        &LibrarianChatRequest {
            message: "What is the status of DialogueNode?".to_string(),
            project: None,
            project_context: None,
            project_context_scope: None,
            session_id: None,
        },
        "What is the status of DialogueNode?",
    )
    .await?;
    if !ask_context.nodes.is_empty() || ask_context.suggested_nodes.len() != 1 {
        anyhow::bail!("Dialogue context smoke expected one suggested Library node in ask mode");
    }

    let mut auto_config = config.clone();
    auto_config.tool_permissions.context_switch = ToolPermissionPolicy::Auto;
    let auto_context = resolve_chat_project_context(
        &state,
        &auto_config,
        &LibrarianChatRequest {
            message: format!("Open context {library_path}"),
            project: None,
            project_context: None,
            project_context_scope: None,
            session_id: None,
        },
        &format!("Open context {library_path}"),
    )
    .await?;
    if auto_context.nodes.len() != 1 || !auto_context.suggested_nodes.is_empty() {
        anyhow::bail!("Dialogue context smoke expected one selected Library node in auto mode");
    }
    Ok(())
}

pub async fn run_approval_ui_smoke(config: &Config) -> Result<()> {
    let db = Database::connect(config).await?;
    db.migrate().await?;
    let state = AppState {
        db,
        config: Arc::new(RwLock::new(config.clone())),
    };
    let result = execute_approval_slash_command(
        &state,
        config,
        &[
            "propose".to_string(),
            "library".to_string(),
            "create_folder".to_string(),
            serde_json::json!({
                "summary": "Create smoke approval shelf.",
                "library_path": "approval-smoke/shelf",
            })
            .to_string(),
        ],
    )
    .await?;
    if result
        .ui
        .as_ref()
        .and_then(|ui| ui.get("type"))
        .and_then(serde_json::Value::as_str)
        != Some("approval")
    {
        anyhow::bail!("Approval UI smoke expected an approval card payload");
    }
    Ok(())
}

pub async fn run_agent_action_ui_smoke(config: &Config, name: &str) -> Result<()> {
    config.ensure_layout()?;
    let db = Database::connect(config).await?;
    db.migrate().await?;
    let workspace_dir = config
        .home
        .join("Projects")
        .join(format!("_smoke/agent-action-ui/{name}"));
    std::fs::create_dir_all(&workspace_dir).map_err(|error| {
        anyhow::anyhow!("Failed to create {}: {error}", workspace_dir.display())
    })?;
    let workspace_dir = workspace_dir.canonicalize().map_err(|error| {
        anyhow::anyhow!("Failed to resolve {}: {error}", workspace_dir.display())
    })?;
    let project_name = format!("Agent Action UI {name}");
    let project = db.add_project(&project_name, &workspace_dir).await?;
    let state = AppState {
        db: db.clone(),
        config: Arc::new(RwLock::new(config.clone())),
    };
    let result = execute_agent_slash_command(
        &state,
        config,
        &[
            "launch".to_string(),
            project.name.clone(),
            "summarize smoke action card".to_string(),
            "--provider".to_string(),
            "codex".to_string(),
            "--read-only".to_string(),
            "--yes".to_string(),
        ],
    )
    .await?;
    let ui = result
        .ui
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Agent action UI smoke expected UI metadata"))?;
    if ui.get("type").and_then(serde_json::Value::as_str) != Some("agent_action") {
        anyhow::bail!("Agent action UI smoke expected an agent_action payload");
    }
    if ui.get("job").is_none() {
        anyhow::bail!("Agent action UI smoke expected queued job metadata");
    }
    let jobs = db.list_jobs().await?;
    let matching = jobs
        .iter()
        .filter(|job| job.project_id == project.id && job.goal == "summarize smoke action card")
        .count();
    if matching != 1 {
        anyhow::bail!("Agent action UI smoke expected exactly one queued job, got {matching}");
    }
    Ok(())
}

pub async fn run_agent_review_packet_ui_smoke(config: &Config, job_id: Uuid) -> Result<()> {
    let db = Database::connect(config).await?;
    db.migrate().await?;
    let state = AppState {
        db,
        config: Arc::new(RwLock::new(config.clone())),
    };
    let result = execute_agent_slash_command(
        &state,
        config,
        &["review-packet".to_string(), job_id.to_string()],
    )
    .await?;
    let Some(ui) = result.ui else {
        anyhow::bail!("Agent review packet smoke expected chat UI metadata");
    };
    if ui.get("type").and_then(serde_json::Value::as_str) != Some("job_review") {
        anyhow::bail!("Agent review packet smoke expected `job_review` UI type");
    }
    if ui
        .get("packet")
        .and_then(|packet| packet.get("summary"))
        .and_then(|summary| summary.get("next_step"))
        .and_then(serde_json::Value::as_str)
        .is_none()
    {
        anyhow::bail!("Agent review packet smoke expected packet.summary.next_step");
    }
    Ok(())
}

pub async fn run_project_slash_smoke(config: &Config, name: &str) -> Result<()> {
    config.ensure_layout()?;
    let db = Database::connect(config).await?;
    db.migrate().await?;
    let state = AppState {
        db: db.clone(),
        config: Arc::new(RwLock::new(config.clone())),
    };

    let slug = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let slug = if slug.is_empty() {
        "project-slash-smoke".to_string()
    } else {
        slug
    };
    let project_name = format!("Project Slash Smoke {slug}");
    let initial_library = format!("project-slash-smoke/{slug}/initial");
    let attached_library = format!("project-slash-smoke/{slug}/attached");
    let initial_workspace = config
        .home
        .join("Projects")
        .join("_smoke")
        .join("project-slash")
        .join(&slug)
        .join("initial");
    let attached_workspace = config
        .home
        .join("Projects")
        .join("_smoke")
        .join("project-slash")
        .join(&slug)
        .join("attached");
    std::fs::create_dir_all(&initial_workspace)?;
    std::fs::create_dir_all(&attached_workspace)?;

    let create = execute_project_slash_command(
        &state,
        config,
        &[
            "create".to_string(),
            project_name.clone(),
            "--library".to_string(),
            initial_library.clone(),
            "--workspace".to_string(),
            initial_workspace.display().to_string(),
        ],
    )
    .await?;
    if !create.reply.contains("Created project") {
        anyhow::bail!("Project slash smoke expected create reply");
    }
    let project = db.get_project_by_name_or_id(&project_name).await?;
    if project.library_path.as_deref() != Some(Path::new(&initial_library)) {
        anyhow::bail!("Project slash smoke did not attach initial Library path");
    }

    execute_project_slash_command(
        &state,
        config,
        &[
            "attach-library".to_string(),
            project_name.clone(),
            attached_library.clone(),
        ],
    )
    .await?;
    execute_project_slash_command(
        &state,
        config,
        &[
            "attach-workspace".to_string(),
            project_name.clone(),
            attached_workspace.display().to_string(),
        ],
    )
    .await?;
    let project = db.get_project_by_name_or_id(&project_name).await?;
    if project.library_path.as_deref() != Some(Path::new(&attached_library))
        || project.path != attached_workspace.canonicalize()?
    {
        anyhow::bail!("Project slash smoke did not persist attached paths");
    }

    let status = execute_project_slash_command(
        &state,
        config,
        &["status".to_string(), project_name.clone()],
    )
    .await?;
    if !status.reply.contains(&project_name) {
        anyhow::bail!("Project slash smoke expected status reply to include project name");
    }
    let map = execute_project_slash_command(&state, config, &["map".to_string()]).await?;
    if map
        .trace
        .first()
        .and_then(|trace| trace.get("map"))
        .and_then(|map| map.get("linked_project_count"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
        == 0
    {
        anyhow::bail!("Project slash smoke expected project map to include linked projects");
    }
    Ok(())
}

pub async fn run_prompt_defaults_smoke(config: &Config) -> Result<()> {
    config.ensure_layout()?;
    let db = Database::connect(config).await?;
    db.migrate().await?;
    let state = AppState {
        db: db.clone(),
        config: Arc::new(RwLock::new(config.clone())),
    };
    let needs_confirmation =
        execute_prompt_slash_command(&state, config, &["seed-defaults".to_string()]).await?;
    if needs_confirmation
        .trace
        .first()
        .and_then(|trace| trace.get("status"))
        .and_then(serde_json::Value::as_str)
        != Some("needs_explicit_confirmation")
    {
        anyhow::bail!("Prompt defaults smoke expected explicit confirmation gate");
    }

    let seeded = execute_prompt_slash_command(
        &state,
        config,
        &["seed-defaults".to_string(), "--yes".to_string()],
    )
    .await?;
    let seeded_count = seeded
        .trace
        .first()
        .and_then(|trace| trace.get("seeded"))
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    if seeded_count < 6 {
        anyhow::bail!("Prompt defaults smoke expected all default blocks to be reported");
    }

    let before_count = db.list_prompt_blocks(None).await?.len();
    execute_prompt_slash_command(
        &state,
        config,
        &["seed-defaults".to_string(), "--yes".to_string()],
    )
    .await?;
    let after_count = db.list_prompt_blocks(None).await?.len();
    if before_count != after_count {
        anyhow::bail!("Prompt defaults smoke expected seeding to be idempotent");
    }

    let librarian = execute_prompt_slash_command(
        &state,
        config,
        &["render".to_string(), "librarian".to_string()],
    )
    .await?;
    if !librarian.reply.contains("You are Librarian") {
        anyhow::bail!("Prompt defaults smoke expected Librarian identity in rendered prompt");
    }
    let claude = execute_prompt_slash_command(
        &state,
        config,
        &["render".to_string(), "CLAUDE.md".to_string()],
    )
    .await?;
    if !claude.reply.contains("project-local guidance") {
        anyhow::bail!("Prompt defaults smoke expected Claude instruction preset");
    }

    let scratch = db
        .create_prompt_block("librarian", "Smoke scratch", "temporary", true)
        .await?;
    let update_gate = execute_prompt_slash_command(
        &state,
        config,
        &[
            "update".to_string(),
            scratch.id.to_string(),
            "--content".to_string(),
            "updated scratch".to_string(),
        ],
    )
    .await?;
    if update_gate
        .trace
        .first()
        .and_then(|trace| trace.get("status"))
        .and_then(serde_json::Value::as_str)
        != Some("needs_explicit_confirmation")
    {
        anyhow::bail!("Prompt defaults smoke expected update confirmation gate");
    }
    execute_prompt_slash_command(
        &state,
        config,
        &[
            "update".to_string(),
            scratch.id.to_string(),
            "--name".to_string(),
            "Smoke scratch updated".to_string(),
            "--content".to_string(),
            "updated scratch".to_string(),
            "--position".to_string(),
            "1".to_string(),
            "--plain".to_string(),
            "--yes".to_string(),
        ],
    )
    .await?;
    let updated = db.get_prompt_block(scratch.id).await?;
    if updated.name != "Smoke scratch updated"
        || updated.content != "updated scratch"
        || updated.markdown
        || updated.position != 1
    {
        anyhow::bail!("Prompt defaults smoke expected slash update to persist fields");
    }
    let delete_gate = execute_prompt_slash_command(
        &state,
        config,
        &["delete".to_string(), scratch.id.to_string()],
    )
    .await?;
    if delete_gate
        .trace
        .first()
        .and_then(|trace| trace.get("status"))
        .and_then(serde_json::Value::as_str)
        != Some("needs_explicit_confirmation")
    {
        anyhow::bail!("Prompt defaults smoke expected delete confirmation gate");
    }
    execute_prompt_slash_command(
        &state,
        config,
        &[
            "delete".to_string(),
            scratch.id.to_string(),
            "--yes".to_string(),
        ],
    )
    .await?;
    if db.get_prompt_block(scratch.id).await.is_ok() {
        anyhow::bail!("Prompt defaults smoke expected slash delete to remove the block");
    }

    let export = execute_prompt_slash_command(
        &state,
        config,
        &[
            "export-proposal".to_string(),
            "librarian".to_string(),
            "prompt-smoke/librarian-export.md".to_string(),
        ],
    )
    .await?;
    if export
        .ui
        .as_ref()
        .and_then(|ui| ui.get("type"))
        .and_then(serde_json::Value::as_str)
        != Some("approval")
    {
        anyhow::bail!("Prompt defaults smoke expected export proposal approval UI");
    }
    let approval_id = export
        .trace
        .first()
        .and_then(|trace| trace.get("approval"))
        .and_then(|approval| approval.get("id"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Prompt export smoke did not return approval id"))?
        .to_string();
    execute_approval_slash_command(&state, config, &["approve".to_string(), approval_id]).await?;
    let exported = library_tools::read_markdown(config, "prompt-smoke/librarian-export.md")?;
    if !exported.contains("You are Librarian") {
        anyhow::bail!("Prompt defaults smoke expected approved export to write rendered prompt");
    }

    let preset_export = execute_prompt_slash_command(
        &state,
        config,
        &["export-presets".to_string(), "librarian".to_string()],
    )
    .await?;
    let preset = preset_export
        .trace
        .first()
        .and_then(|trace| trace.get("preset"))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Prompt preset export smoke did not return JSON preset"))?;
    let preset_json = serde_json::to_string(&preset)?;
    let import_gate = execute_prompt_slash_command(
        &state,
        config,
        &["import-presets".to_string(), preset_json.clone()],
    )
    .await?;
    if import_gate
        .trace
        .first()
        .and_then(|trace| trace.get("status"))
        .and_then(serde_json::Value::as_str)
        != Some("needs_explicit_confirmation")
    {
        anyhow::bail!("Prompt defaults smoke expected import confirmation gate");
    }
    let before_import_count = db.list_prompt_blocks(None).await?.len();
    let imported = execute_prompt_slash_command(
        &state,
        config,
        &[
            "import-presets".to_string(),
            preset_json,
            "--yes".to_string(),
        ],
    )
    .await?;
    let imported_count = imported
        .trace
        .first()
        .and_then(|trace| trace.get("imported"))
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    if imported_count == 0 || before_import_count != db.list_prompt_blocks(None).await?.len() {
        anyhow::bail!("Prompt defaults smoke expected preset import to update idempotently");
    }
    Ok(())
}

pub async fn run_memory_cleanup_smoke(config: &Config) -> Result<()> {
    config.ensure_layout()?;
    let db = Database::connect(config).await?;
    db.migrate().await?;
    let legacy = db
        .add_memory_item(
            None,
            None,
            MemoryKind::AssistantMessage,
            Some("librarian-chat"),
            "I am here as Librarian, not as a background agent runner.\n\nLegacy smoke item.",
            Some("admin:librarian-chat"),
            serde_json::json!({
                "mode": "local-memory-responder",
                "scope": "global",
            }),
        )
        .await?;
    let gate = execute_memory_slash_command(
        &db,
        config,
        None,
        &["cleanup-legacy-local-responder".to_string()],
    )
    .await?;
    if gate
        .trace
        .first()
        .and_then(|trace| trace.get("status"))
        .and_then(serde_json::Value::as_str)
        != Some("needs_explicit_confirmation")
    {
        anyhow::bail!("Memory cleanup smoke expected explicit confirmation gate");
    }
    execute_memory_slash_command(
        &db,
        config,
        None,
        &[
            "cleanup-legacy-local-responder".to_string(),
            "--yes".to_string(),
        ],
    )
    .await?;
    if db.get_memory_item(legacy.id).await.is_ok() {
        anyhow::bail!("Memory cleanup smoke expected legacy responder item to be deleted");
    }
    Ok(())
}
