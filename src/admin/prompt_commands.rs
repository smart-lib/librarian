use super::*;

pub(super) async fn execute_prompt_slash_command(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            prompt_slash_help(),
            serde_json::json!({ "command": "prompt" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            prompt_slash_help(),
            serde_json::json!({ "tool": "prompt", "command": command }),
        ),
        "blocks" | "list" => {
            let target = args.get(1).map(String::as_str);
            let blocks = state.db.list_prompt_blocks(target).await?;
            let version = prompt::prompt_block_version(target, &blocks);
            let mut reply = format!("Prompt blocks: {} item(s).", blocks.len());
            for block in &blocks {
                reply.push_str(&format!(
                    "\n{} [{}] #{} {} enabled={}",
                    block.id, block.target, block.position, block.name, block.enabled
                ));
            }
            slash_reply(
                &reply,
                serde_json::json!({ "tool": "prompt", "command": command, "target": target, "version": version, "blocks": blocks }),
            )
        }
        "seed-defaults" | "seed" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(slash_reply(
                    "Seeding default prompt blocks changes Librarian instructions. Use: /prompt seed-defaults --yes",
                    serde_json::json!({
                        "tool": "prompt",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                    }),
                ));
            }
            let seeded = seed_default_prompt_blocks(&state.db).await?;
            slash_reply(
                &format!("Seeded {} default prompt block(s).", seeded.len()),
                serde_json::json!({
                    "tool": "prompt",
                    "command": command,
                    "seeded": seeded,
                }),
            )
        }
        "add-block" | "add" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            let request = parse_prompt_add_block_args(&args[1..])?;
            let block = state
                .db
                .create_prompt_block(
                    &request.target,
                    &request.name,
                    &request.content,
                    request.markdown,
                )
                .await?;
            state
                .db
                .add_system_event(
                    "prompt_tool",
                    serde_json::json!({
                        "action": "add_block",
                        "source": "slash-command",
                        "block_id": block.id,
                        "target": block.target,
                    }),
                )
                .await?;
            slash_reply(
                &format!(
                    "Added prompt block {} [{}] {}.",
                    block.id, block.target, block.name
                ),
                serde_json::json!({ "tool": "prompt", "command": command, "block": block }),
            )
        }
        "enable" | "disable" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            let id = slash_prompt_block_id_arg(args, "/prompt enable <block-id>")?;
            let enabled = command == "enable";
            let block = state.db.set_prompt_block_enabled(id, enabled).await?;
            state
                .db
                .add_system_event(
                    "prompt_tool",
                    serde_json::json!({
                        "action": if enabled { "enable_block" } else { "disable_block" },
                        "source": "slash-command",
                        "block_id": block.id,
                        "target": block.target,
                    }),
                )
                .await?;
            slash_reply(
                &format!(
                    "{} prompt block {}.",
                    if enabled { "Enabled" } else { "Disabled" },
                    block.id
                ),
                serde_json::json!({ "tool": "prompt", "command": command, "block": block }),
            )
        }
        "update" | "edit" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            let request = parse_prompt_update_args(&args[1..])?;
            if !request.confirmed {
                return Ok(slash_reply(
                    "Updating prompt blocks changes Librarian instructions. Use: /prompt update <block-id> [--name name] [--content content] [--position n] [--markdown|--plain] [--enable|--disable] --yes",
                    serde_json::json!({
                        "tool": "prompt",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                        "block_id": request.id,
                    }),
                ));
            }
            let block = state
                .db
                .update_prompt_block(
                    request.id,
                    request.name.as_deref(),
                    request.content.as_deref(),
                    request.enabled,
                    request.position,
                    request.markdown,
                )
                .await?;
            state
                .db
                .add_system_event(
                    "prompt_tool",
                    serde_json::json!({
                        "action": "update_block",
                        "source": "slash-command",
                        "block_id": block.id,
                        "target": block.target,
                    }),
                )
                .await?;
            slash_reply(
                &format!(
                    "Updated prompt block {} [{}] {}.",
                    block.id, block.target, block.name
                ),
                serde_json::json!({ "tool": "prompt", "command": command, "block": block }),
            )
        }
        "delete" | "remove" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            let id = slash_prompt_block_id_arg(args, "/prompt delete <block-id> --yes")?;
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(slash_reply(
                    "Deleting prompt blocks changes Librarian instructions. Use: /prompt delete <block-id> --yes",
                    serde_json::json!({
                        "tool": "prompt",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                        "block_id": id,
                    }),
                ));
            }
            let block = state.db.get_prompt_block(id).await?;
            state.db.delete_prompt_block(id).await?;
            state
                .db
                .add_system_event(
                    "prompt_tool",
                    serde_json::json!({
                        "action": "delete_block",
                        "source": "slash-command",
                        "block_id": id,
                        "target": block.target,
                    }),
                )
                .await?;
            slash_reply(
                &format!("Deleted prompt block {id}."),
                serde_json::json!({ "tool": "prompt", "command": command, "block_id": id }),
            )
        }
        "export-proposal" | "propose-export" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /prompt export-proposal <target> <library-md-path>");
            }
            let target = args[1].trim();
            let path = library_tools::normalize_tool_relative_path(args[2].trim())?;
            let blocks = state.db.list_prompt_blocks(Some(target)).await?;
            let rendered = render_prompt_blocks(&blocks);
            let approval = state
                .db
                .create_tool_approval(
                    "library",
                    "write_markdown",
                    serde_json::json!({
                        "path": path,
                        "content": rendered,
                        "target": target,
                        "summary": format!("Export prompt target `{target}` to Library markdown."),
                    }),
                )
                .await?;
            state
                .db
                .add_system_event(
                    "tool_approval",
                    serde_json::json!({
                        "action": "propose_prompt_export",
                        "source": "slash-command",
                        "approval_id": approval.id,
                        "target": target,
                    }),
                )
                .await?;
            slash_reply_with_ui(
                "Review this prompt export proposal.",
                serde_json::json!({
                    "tool": "prompt",
                    "command": command,
                    "approval": approval,
                }),
                serde_json::json!({
                    "type": "approval",
                    "approval": approval,
                }),
            )
        }
        "export-presets" | "export-json" => {
            let target = args.get(1).map(String::as_str);
            let blocks = state.db.list_prompt_blocks(target).await?;
            let document = prompt_preset_document(target, &blocks);
            let json = serde_json::to_string_pretty(&document)?;
            slash_reply(
                &format!(
                    "Prompt preset export: {} block(s).\n\n```json\n{json}\n```",
                    document.blocks.len()
                ),
                serde_json::json!({
                    "tool": "prompt",
                    "command": command,
                    "target": target,
                    "preset": document,
                }),
            )
        }
        "import-presets" | "import-json" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            if args.len() < 2 {
                anyhow::bail!("Usage: /prompt import-presets <preset-json> --yes");
            }
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(slash_reply(
                    "Importing prompt presets changes Librarian instructions. Use: /prompt import-presets <preset-json> --yes",
                    serde_json::json!({
                        "tool": "prompt",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                    }),
                ));
            }
            let json = args
                .iter()
                .skip(1)
                .filter(|arg| *arg != "--yes" && *arg != "--approve")
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            let document: PromptPresetDocument = serde_json::from_str(&json)
                .map_err(|error| anyhow::anyhow!("Invalid prompt preset JSON: {error}"))?;
            let imported = import_prompt_preset_document(&state.db, &document).await?;
            state
                .db
                .add_system_event(
                    "prompt_tool",
                    serde_json::json!({
                        "action": "import_presets",
                        "source": "slash-command",
                        "imported": imported.iter().map(|block| serde_json::json!({
                            "id": block.id,
                            "target": block.target,
                            "name": block.name,
                        })).collect::<Vec<_>>(),
                    }),
                )
                .await?;
            slash_reply(
                &format!("Imported {} prompt preset block(s).", imported.len()),
                serde_json::json!({
                    "tool": "prompt",
                    "command": command,
                    "imported": imported,
                }),
            )
        }
        "render" => {
            let target = args
                .get(1)
                .map(String::as_str)
                .ok_or_else(|| anyhow::anyhow!("Usage: /prompt render <target>"))?;
            let blocks = state.db.list_prompt_blocks(Some(target)).await?;
            let rendered = render_prompt_blocks(&blocks);
            let version = prompt::prompt_block_version(Some(target), &blocks);
            slash_reply(
                &format!("Rendered prompt target `{target}`:\n\n{rendered}"),
                serde_json::json!({
                    "tool": "prompt",
                    "command": command,
                    "target": target,
                    "rendered": rendered,
                    "version": version,
                    "blocks": blocks,
                }),
            )
        }
        _ => slash_reply(
            "Unknown prompt command. Try /prompt help.",
            serde_json::json!({ "tool": "prompt", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

pub(super) struct PromptAddBlockSlashRequest {
    pub(super) target: String,
    pub(super) name: String,
    pub(super) content: String,
    pub(super) markdown: bool,
}

struct PromptUpdateSlashRequest {
    id: Uuid,
    name: Option<String>,
    content: Option<String>,
    enabled: Option<bool>,
    position: Option<i64>,
    markdown: Option<bool>,
    confirmed: bool,
}

pub(super) fn parse_prompt_add_block_args(args: &[String]) -> Result<PromptAddBlockSlashRequest> {
    if args.len() < 3 {
        anyhow::bail!("Usage: /prompt add-block <target> <name> <content> [--plain]");
    }
    let target = args[0].clone();
    let name = args[1].clone();
    let mut markdown = true;
    let mut content_parts = Vec::new();
    for arg in &args[2..] {
        if arg == "--plain" {
            markdown = false;
        } else {
            content_parts.push(arg.clone());
        }
    }
    let content = content_parts.join(" ").trim().to_string();
    if content.is_empty() {
        anyhow::bail!("Prompt block content must not be empty");
    }
    Ok(PromptAddBlockSlashRequest {
        target,
        name,
        content,
        markdown,
    })
}

fn parse_prompt_update_args(args: &[String]) -> Result<PromptUpdateSlashRequest> {
    let id = args
        .first()
        .ok_or_else(|| {
            anyhow::anyhow!("Usage: /prompt update <block-id> [--name name] [--content content] [--position n] [--markdown|--plain] [--enable|--disable] --yes")
        })?
        .parse::<Uuid>()
        .map_err(|error| anyhow::anyhow!("Invalid prompt block id: {error}"))?;
    let mut name = None;
    let mut content = None;
    let mut enabled = None;
    let mut position = None;
    let mut markdown = None;
    let mut confirmed = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--name" => {
                index += 1;
                name = Some(
                    args.get(index)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| anyhow::anyhow!("--name requires a value"))?
                        .clone(),
                );
            }
            "--content" => {
                index += 1;
                content = Some(
                    args.get(index)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| anyhow::anyhow!("--content requires a value"))?
                        .clone(),
                );
            }
            "--position" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("--position requires a value"))?;
                position = Some(
                    value
                        .parse::<i64>()
                        .map_err(|error| anyhow::anyhow!("Invalid position `{value}`: {error}"))?,
                );
            }
            "--markdown" => markdown = Some(true),
            "--plain" => markdown = Some(false),
            "--enable" => enabled = Some(true),
            "--disable" => enabled = Some(false),
            "--yes" | "--approve" => confirmed = true,
            value => anyhow::bail!("Unknown /prompt update flag `{value}`"),
        }
        index += 1;
    }
    if name.is_none()
        && content.is_none()
        && enabled.is_none()
        && position.is_none()
        && markdown.is_none()
    {
        anyhow::bail!("Prompt update needs at least one changed field");
    }
    Ok(PromptUpdateSlashRequest {
        id,
        name,
        content,
        enabled,
        position,
        markdown,
        confirmed,
    })
}

async fn seed_default_prompt_blocks(db: &Database) -> Result<Vec<crate::domain::PromptBlock>> {
    let defaults = default_prompt_block_presets();
    let mut seeded = Vec::new();
    for preset in defaults {
        let existing = db
            .list_prompt_blocks(Some(preset.target))
            .await?
            .into_iter()
            .find(|block| block.name == preset.name);
        if let Some(block) = existing {
            seeded.push(block);
        } else {
            seeded.push(
                db.create_prompt_block(preset.target, preset.name, preset.content, true)
                    .await?,
            );
        }
    }
    db.add_system_event(
        "prompt_tool",
        serde_json::json!({
            "action": "seed_defaults",
            "source": "slash-command",
            "blocks": seeded.iter().map(|block| serde_json::json!({
                "id": block.id,
                "target": block.target,
                "name": block.name,
            })).collect::<Vec<_>>(),
        }),
    )
    .await?;
    Ok(seeded)
}

struct PromptBlockPreset {
    target: &'static str,
    name: &'static str,
    content: &'static str,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PromptPresetDocument {
    pub(super) schema: String,
    pub(super) target: Option<String>,
    pub(super) blocks: Vec<PromptPresetBlock>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PromptPresetBlock {
    pub(super) target: String,
    pub(super) name: String,
    pub(super) content: String,
    pub(super) enabled: bool,
    pub(super) position: i64,
    pub(super) markdown: bool,
}

fn default_prompt_block_presets() -> Vec<PromptBlockPreset> {
    debug_assert!(prompt::PROMPT_PROFILE_KINDS.contains(&prompt::PromptProfileKind::Chat));
    let librarian_target = prompt::default_profile_target(prompt::PromptProfileKind::Chat);
    let agent_target = prompt::default_profile_target(prompt::PromptProfileKind::Agent);
    let generic_agent_file_target =
        prompt::default_profile_target(prompt::PromptProfileKind::ProviderInstruction);
    vec![
        PromptBlockPreset {
            target: librarian_target,
            name: "Identity",
            content: "You are Librarian: a calm, practical assistant for organizing ideas, project memory, time, tasks, and supervised agent work. Keep normal chat conversational; launch background agents only after an explicit user action.",
        },
        PromptBlockPreset {
            target: librarian_target,
            name: "Memory policy",
            content: "Use current chat context and durable memory carefully. Treat raw transcript memories as conversation history, and save durable facts, decisions, and instructions only when they are useful beyond the current turn.",
        },
        PromptBlockPreset {
            target: agent_target,
            name: "Agent boundary",
            content: "Work only inside the mounted project boundary. Preserve user work, explain risky operations before doing them, and report concise outcomes back to Librarian.",
        },
        PromptBlockPreset {
            target: agent_target,
            name: "Git policy",
            content: "Inspect repository state before editing. Do not revert unrelated user changes. Commit or push only when the task or project policy explicitly allows it.",
        },
        PromptBlockPreset {
            target: prompt::TARGET_CLAUDE_FILE,
            name: "Claude launch context",
            content: "This project is being opened by Librarian for a focused background task. Read this file as project-local guidance, then complete the prompt from the current working directory.",
        },
        PromptBlockPreset {
            target: generic_agent_file_target,
            name: "Generic agent launch context",
            content: "You are running as a supervised project agent under Librarian. Use the mounted workspace as the project root and keep outputs suitable for a later Librarian summary.",
        },
    ]
}

pub(super) fn prompt_preset_document(
    target: Option<&str>,
    blocks: &[crate::domain::PromptBlock],
) -> PromptPresetDocument {
    PromptPresetDocument {
        schema: "librarian.prompt-presets.v1".to_string(),
        target: target.map(ToOwned::to_owned),
        blocks: blocks
            .iter()
            .map(|block| PromptPresetBlock {
                target: block.target.clone(),
                name: block.name.clone(),
                content: block.content.clone(),
                enabled: block.enabled,
                position: block.position,
                markdown: block.markdown,
            })
            .collect(),
    }
}

pub(super) async fn import_prompt_preset_document(
    db: &Database,
    document: &PromptPresetDocument,
) -> Result<Vec<crate::domain::PromptBlock>> {
    if document.schema != "librarian.prompt-presets.v1" {
        anyhow::bail!("Unsupported prompt preset schema `{}`", document.schema);
    }
    let mut imported = Vec::new();
    for preset in &document.blocks {
        if preset.target.trim().is_empty()
            || preset.name.trim().is_empty()
            || preset.content.trim().is_empty()
        {
            anyhow::bail!("Prompt preset blocks require non-empty target, name, and content");
        }
        let existing = db
            .list_prompt_blocks(Some(&preset.target))
            .await?
            .into_iter()
            .find(|block| block.name == preset.name);
        let block = if let Some(block) = existing {
            db.update_prompt_block(
                block.id,
                Some(&preset.name),
                Some(&preset.content),
                Some(preset.enabled),
                Some(preset.position),
                Some(preset.markdown),
            )
            .await?
        } else {
            let block = db
                .create_prompt_block(
                    &preset.target,
                    &preset.name,
                    &preset.content,
                    preset.markdown,
                )
                .await?;
            db.update_prompt_block(
                block.id,
                None,
                None,
                Some(preset.enabled),
                Some(preset.position),
                Some(preset.markdown),
            )
            .await?
        };
        imported.push(block);
    }
    Ok(imported)
}

fn slash_prompt_block_id_arg(args: &[String], usage: &str) -> Result<Uuid> {
    args.get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))?
        .parse::<Uuid>()
        .map_err(|error| anyhow::anyhow!("Invalid prompt block id: {error}"))
}

pub(super) fn render_prompt_blocks(blocks: &[crate::domain::PromptBlock]) -> String {
    blocks
        .iter()
        .filter(|block| block.enabled)
        .map(|block| block.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn prompt_slash_help() -> &'static str {
    "Prompt builder commands live under /prompt:\n/prompt blocks [target]\n/prompt seed-defaults --yes\n/prompt add-block <target> <name> <content> [--plain]\n/prompt update <block-id> [--name name] [--content content] [--position n] [--markdown|--plain] [--enable|--disable] --yes\n/prompt delete <block-id> --yes\n/prompt export-proposal <target> <library-md-path>\n/prompt export-presets [target]\n/prompt import-presets <preset-json> --yes\n/prompt enable <block-id>\n/prompt disable <block-id>\n/prompt render <target>\n\nTargets are flexible labels such as librarian, agents, codex, claude, or AGENTS.md. This is the data model for the future visual block editor."
}
