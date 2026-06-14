use super::*;
use axum::{body, http::StatusCode};

#[tokio::test]
async fn chat_endpoint_handles_slash_command_without_creating_jobs() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-chat-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let state = AppState {
            db: db.clone(),
            config: Arc::new(RwLock::new(config)),
        };

        let response = librarian_chat(
            State(state),
            Json(LibrarianChatRequest {
                message: "/help".to_string(),
                project: None,
                project_context: None,
                project_context_scope: None,
                session_id: None,
            }),
        )
        .await
        .expect("chat response")
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload["mode"], "slash-command");
        assert_eq!(payload["iterations"], 0);
        let session_id = Uuid::parse_str(payload["session_id"].as_str().expect("session id"))
            .expect("session uuid");
        assert!(payload["reply"]
            .as_str()
            .expect("reply")
            .contains("Available command groups"));

        assert!(db.list_jobs().await.expect("jobs").is_empty());
        assert_eq!(db.count_memory_items().await.expect("memory count"), 2);
        let recent_memory = db
            .recent_memory_for_project(None, 10)
            .await
            .expect("recent memory");
        assert!(recent_memory
            .iter()
            .any(|item| matches!(item.kind, MemoryKind::UserMessage) && item.content == "/help"));
        assert!(recent_memory.iter().any(|item| {
            matches!(item.kind, MemoryKind::AssistantMessage)
                && item
                    .metadata
                    .get("mode")
                    .and_then(serde_json::Value::as_str)
                    == Some("slash-command")
        }));
        let session_id_text = session_id.to_string();
        assert!(recent_memory.iter().all(|item| {
            item.metadata
                .get("chat_session_id")
                .and_then(serde_json::Value::as_str)
                == Some(session_id_text.as_str())
                && item
                    .metadata
                    .get("durability")
                    .and_then(serde_json::Value::as_str)
                    == Some("transcript")
        }));
        let turns = db.list_chat_turns(session_id).await.expect("chat turns");
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].turn_index, 1);
        assert_eq!(turns[0].role, "user");
        assert_eq!(turns[0].content, "/help");
        assert_eq!(turns[1].turn_index, 2);
        assert_eq!(turns[1].role, "assistant");
        assert!(turns[0].memory_id.is_some());
        assert!(turns[1].memory_id.is_some());

        let sessions = chat_sessions(
            State(AppState {
                db: db.clone(),
                config: Arc::new(RwLock::new(
                    Config::load_or_default(Some(home.clone())).expect("config reload"),
                )),
            }),
            Query(ChatSessionsQuery { limit: Some(5) }),
        )
        .await
        .expect("sessions")
        .into_response();
        assert_eq!(sessions.status(), StatusCode::OK);

        let response = chat_session_turns(
            State(AppState {
                db: db.clone(),
                config: Arc::new(RwLock::new(
                    Config::load_or_default(Some(home.clone())).expect("config reload"),
                )),
            }),
            AxumPath(session_id),
        )
        .await
        .expect("turn response")
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("turn body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("turn json");
        assert_eq!(payload["session"]["id"], session_id.to_string());
        assert_eq!(payload["turns"].as_array().expect("turns").len(), 2);
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn context_slash_command_returns_ui_context_update() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-context-chat-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        library_tools::create_folder(&config, LibraryRoot::Library, "Games")
            .expect("library folder");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let state = AppState {
            db: db.clone(),
            config: Arc::new(RwLock::new(config)),
        };

        let response = librarian_chat(
            State(state),
            Json(LibrarianChatRequest {
                message: "/context set Games".to_string(),
                project: None,
                project_context: None,
                project_context_scope: None,
                session_id: None,
            }),
        )
        .await
        .expect("chat response")
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload["mode"], "slash-command");
        assert_eq!(payload["ui"]["type"], "context_update");
        assert_eq!(payload["ui"]["action"], "set");
        assert_eq!(payload["ui"]["context"]["scope"], "subtree");
        assert_eq!(
            payload["ui"]["context"]["nodes"][0]["library_path"],
            "Games"
        );
        assert!(payload["reply"]
            .as_str()
            .expect("reply")
            .contains("Context set to Games"));
        assert!(db.list_jobs().await.expect("jobs").is_empty());

        let session_id = Uuid::parse_str(payload["session_id"].as_str().expect("session id"))
            .expect("session uuid");
        let turns = db.list_chat_turns(session_id).await.expect("chat turns");
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[1].metadata["ui"]["type"], "context_update");
        assert_eq!(
            turns[1].metadata["ui"]["context"]["nodes"][0]["library_path"],
            "Games"
        );
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn context_retrieval_scope_selects_expected_project_ids() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-context-scope-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let parent_workspace = config.home.join("Projects").join("Games");
        let child_workspace = parent_workspace.join("AdvenTableDays");
        let sibling_workspace = config.home.join("Projects").join("Tools");
        std::fs::create_dir_all(&child_workspace).expect("child workspace");
        std::fs::create_dir_all(&sibling_workspace).expect("sibling workspace");
        let parent = db
            .add_project("Games", &parent_workspace)
            .await
            .expect("parent");
        let parent = db
            .attach_project_library_path(parent.id, Path::new("Games"))
            .await
            .expect("parent library");
        let child = db
            .add_project("AdvenTableDays", &child_workspace)
            .await
            .expect("child");
        let child = db
            .attach_project_library_path(child.id, Path::new("Games/AdvenTableDays"))
            .await
            .expect("child library");
        let sibling = db
            .add_project("Tools", &sibling_workspace)
            .await
            .expect("sibling");
        let _sibling = db
            .attach_project_library_path(sibling.id, Path::new("Tools"))
            .await
            .expect("sibling library");
        let node = ChatLibraryContextNode {
            library_path: Some(PathBuf::from("Games")),
            project: Some(parent.clone()),
        };
        let child_node = ChatLibraryContextNode {
            library_path: Some(PathBuf::from("Games/AdvenTableDays")),
            project: Some(child.clone()),
        };

        let ids_for = |scope, nodes: Vec<ChatLibraryContextNode>| {
            let db = db.clone();
            async move {
                context_project_ids_for_retrieval(
                    &db,
                    &ChatProjectContext {
                        nodes,
                        suggested_nodes: Vec::new(),
                        scope,
                        source: "test",
                    },
                )
                .await
                .expect("ids")
            }
        };

        fn assert_ids(mut actual: Vec<Uuid>, mut expected: Vec<Uuid>) {
            actual.sort();
            expected.sort();
            assert_eq!(actual, expected);
        }

        assert_ids(
            ids_for(ContextScope::Node, vec![node.clone()]).await,
            vec![parent.id],
        );
        assert_ids(
            ids_for(ContextScope::Subtree, vec![node.clone()]).await,
            vec![parent.id, child.id],
        );
        assert_ids(
            ids_for(ContextScope::Ancestors, vec![child_node.clone()]).await,
            vec![parent.id],
        );
        assert_ids(
            ids_for(ContextScope::NodeAndAncestors, vec![child_node.clone()]).await,
            vec![parent.id, child.id],
        );
        assert_ids(
            ids_for(
                ContextScope::ContextSet,
                vec![node.clone(), child_node.clone()],
            )
            .await,
            vec![parent.id, child.id],
        );
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn dialogue_context_inference_suggests_library_node_without_project() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-context-infer-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        library_tools::create_folder(&config, LibraryRoot::Library, "Games/AdvenTableDays")
            .expect("library folder");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let state = AppState {
            db,
            config: Arc::new(RwLock::new(config.clone())),
        };
        let context = resolve_chat_project_context(
            &state,
            &config,
            &LibrarianChatRequest {
                message: "Что дальше по AdvenTableDays?".to_string(),
                project: None,
                project_context: None,
                project_context_scope: None,
                session_id: None,
            },
            "Что дальше по AdvenTableDays?",
        )
        .await
        .expect("context");

        assert!(context.nodes.is_empty());
        assert_eq!(context.suggested_nodes.len(), 1);
        assert_eq!(
            context.suggested_nodes[0].library_path.as_deref(),
            Some(Path::new("Games/AdvenTableDays"))
        );
        assert_eq!(context.source, "suggested");
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn dialogue_context_inference_auto_selects_library_node() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-context-auto-{}", Uuid::new_v4()));

    {
        let mut config = Config::load_or_default(Some(home.clone())).expect("config");
        config.tool_permissions.context_switch = ToolPermissionPolicy::Auto;
        config.ensure_layout().expect("layout");
        library_tools::create_folder(&config, LibraryRoot::Library, "Games/AdvenTableDays")
            .expect("library folder");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let state = AppState {
            db,
            config: Arc::new(RwLock::new(config.clone())),
        };
        let context = resolve_chat_project_context(
            &state,
            &config,
            &LibrarianChatRequest {
                message: "Открой контекст Games/AdvenTableDays".to_string(),
                project: None,
                project_context: None,
                project_context_scope: None,
                session_id: None,
            },
            "Открой контекст Games/AdvenTableDays",
        )
        .await
        .expect("context");

        assert!(context.suggested_nodes.is_empty());
        assert_eq!(context.nodes.len(), 1);
        assert_eq!(
            context.nodes[0].library_path.as_deref(),
            Some(Path::new("Games/AdvenTableDays"))
        );
        assert_eq!(context.source, "auto");
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn memory_recent_hides_raw_transcript_turns() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-memory-recent-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        db.add_memory_item(
            None,
            None,
            MemoryKind::UserMessage,
            Some("chat"),
            "raw chat should stay out of /mem recent",
            Some("test"),
            serde_json::json!({
                "memory_role": "raw_chat_turn",
                "durability": "transcript",
            }),
        )
        .await
        .expect("raw memory");
        db.add_memory_item(
            None,
            None,
            MemoryKind::AssistantMessage,
            Some("legacy-chat"),
            "legacy assistant chat should stay out of /mem recent",
            Some("admin:librarian-chat"),
            serde_json::json!({}),
        )
        .await
        .expect("legacy assistant memory");
        db.add_memory_item(
            None,
            None,
            MemoryKind::AssistantMessage,
            Some("agent-note"),
            "unclassified assistant note should remain visible",
            Some("test"),
            serde_json::json!({}),
        )
        .await
        .expect("assistant note memory");

        execute_memory_slash_command(
            &db,
            &config,
            None,
            &[
                "remember".to_string(),
                "fact".to_string(),
                "durable memory should remain visible".to_string(),
            ],
        )
        .await
        .expect("remember");
        let result = execute_memory_slash_command(
            &db,
            &config,
            None,
            &["recent".to_string(), "10".to_string()],
        )
        .await
        .expect("recent");

        assert!(result
            .reply
            .contains("durable memory should remain visible"));
        assert!(result.reply.contains("Fact"));
        assert!(result
            .reply
            .contains("unclassified assistant note should remain visible"));
        assert!(!result.reply.contains("raw chat should stay out"));
        assert!(!result.reply.contains("legacy assistant chat"));
        assert_eq!(result.mode, "slash-command");
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn memory_supersede_links_new_durable_item() {
    let home = std::env::current_dir().expect("current dir").join(format!(
        ".librarian-test-memory-supersede-{}",
        Uuid::new_v4()
    ));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let old = db
            .add_memory_item(
                None,
                None,
                MemoryKind::Fact,
                Some("test"),
                "Old durable fact",
                Some("test"),
                serde_json::json!({ "durability": "durable" }),
            )
            .await
            .expect("old memory");

        let result = execute_memory_slash_command(
            &db,
            &config,
            None,
            &[
                "supersede".to_string(),
                old.id.to_string(),
                "fact".to_string(),
                "New durable fact".to_string(),
            ],
        )
        .await
        .expect("supersede");

        let memory_id = Uuid::parse_str(
            result.trace[0]["memory_id"]
                .as_str()
                .expect("memory id in trace"),
        )
        .expect("memory uuid");
        let new = db.get_memory_item(memory_id).await.expect("new memory");
        assert_eq!(new.supersedes_id, Some(old.id));
        assert_eq!(new.contradicts_id, None);
        assert_eq!(
            new.metadata
                .get("durability")
                .and_then(serde_json::Value::as_str),
            Some("durable")
        );
        assert!(result.reply.contains("supersede memory"));
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn memory_contradict_suppresses_old_item_from_retrieval() {
    let home = std::env::current_dir().expect("current dir").join(format!(
        ".librarian-test-memory-contradict-{}",
        Uuid::new_v4()
    ));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let old = db
            .add_memory_item(
                None,
                None,
                MemoryKind::Fact,
                Some("atlas"),
                "Atlas color is red",
                Some("test"),
                serde_json::json!({ "durability": "durable" }),
            )
            .await
            .expect("old memory");
        memory::embed_item(&db, &config, &old)
            .await
            .expect("old embed");

        execute_memory_slash_command(
            &db,
            &config,
            None,
            &[
                "contradict".to_string(),
                old.id.to_string(),
                "fact".to_string(),
                "Atlas color is green".to_string(),
            ],
        )
        .await
        .expect("contradict");
        let pack = memory::retrieve_context_with_config(
            &db,
            Some(&config),
            memory::RetrievalRequest {
                query: "atlas color".to_string(),
                project_id: None,
                activity_id: None,
                limit: 10,
            },
        )
        .await
        .expect("context");

        assert!(!pack.hits.iter().any(|hit| hit.item.id == old.id));
        assert!(pack
            .hits
            .iter()
            .any(|hit| hit.item.contradicts_id == Some(old.id)));
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn explicit_agent_slash_command_creates_one_queued_job() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-agent-chat-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let workspace_path = config.home.join("Projects").join("launch-test");
        std::fs::create_dir_all(&workspace_path).expect("workspace");
        let project = db
            .add_project("Launch Test", &workspace_path)
            .await
            .expect("project");
        let state = AppState {
            db: db.clone(),
            config: Arc::new(RwLock::new(config)),
        };

        let response = librarian_chat(
                State(state),
                Json(LibrarianChatRequest {
                    message: r#"/agent launch "Launch Test" "summarize state" --provider codex --read-only --yes"#
                        .to_string(),
                    project: None,
                    project_context: None,
                    project_context_scope: None,
                    session_id: None,
                }),
            )
            .await
            .expect("chat response")
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload["mode"], "slash-command");
        assert!(payload["reply"]
            .as_str()
            .expect("reply")
            .contains("Queued background agent job"));
        assert_eq!(payload["ui"]["type"], "agent_action");
        assert_eq!(payload["ui"]["command"], "launch");
        assert_eq!(payload["ui"]["project"], "Launch Test");
        assert_eq!(payload["ui"]["job"]["goal"], "summarize state");

        let jobs = db.list_jobs().await.expect("jobs");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].project_id, project.id);
        assert!(matches!(jobs[0].status, JobStatus::Queued));
        assert!(matches!(jobs[0].mount_mode, MountMode::ReadOnly));
        assert!(matches!(
            jobs[0].network_mode,
            crate::domain::NetworkMode::Provider
        ));
        assert_eq!(jobs[0].goal, "summarize state");
        let events = db.list_job_events(jobs[0].id).await.expect("events");
        assert!(events.iter().any(|event| event.kind == "queued_from_chat"));
    }

    std::fs::remove_dir_all(home).ok();
}

#[test]
fn splits_quoted_slash_command_arguments() {
    let args = split_slash_args(r#"mkdir library "Project Shelf/Book Notes""#).expect("args");
    assert_eq!(
        args,
        vec![
            "mkdir".to_string(),
            "library".to_string(),
            "Project Shelf/Book Notes".to_string(),
        ]
    );
}

#[test]
fn rejects_unclosed_slash_command_quotes() {
    assert!(split_slash_args(r#"read "Project Shelf/Book Notes.md"#).is_err());
}

#[test]
fn parses_tool_permission_policy_tokens() {
    assert_eq!(
        parse_tool_permission_policy("auto").expect("auto"),
        ToolPermissionPolicy::Auto
    );
    assert_eq!(
        parse_tool_permission_policy("ASK").expect("ask"),
        ToolPermissionPolicy::Ask
    );
    assert_eq!(
        parse_tool_permission_policy("deny").expect("deny"),
        ToolPermissionPolicy::Deny
    );
    assert!(parse_tool_permission_policy("maybe").is_err());
}

#[test]
fn parses_tool_permission_preset_tokens() {
    assert_eq!(
        parse_tool_permission_preset("balanced").expect("balanced"),
        ToolPermissionPreset::Balanced
    );
    assert_eq!(
        parse_tool_permission_preset("locked-down").expect("locked"),
        ToolPermissionPreset::LockedDown
    );
    assert!(parse_tool_permission_preset("maybe").is_err());
}

#[test]
fn sets_tool_permission_by_key() {
    let mut permissions = ToolPermissionsConfig::default();
    set_tool_permission(
        &mut permissions,
        "library_edit_markdown",
        ToolPermissionPolicy::Auto,
    )
    .expect("set permission");
    assert_eq!(
        permissions.library_edit_markdown,
        ToolPermissionPolicy::Auto
    );
    assert_eq!(permissions.preset, ToolPermissionPreset::Custom);
    assert!(set_tool_permission(
        &mut permissions,
        "unknown_permission",
        ToolPermissionPolicy::Deny,
    )
    .is_err());
}

#[test]
fn applies_permission_presets() {
    let mut permissions = ToolPermissionsConfig::default();
    set_tool_permission(
        &mut permissions,
        "context_switch",
        ToolPermissionPolicy::Auto,
    )
    .expect("set custom");
    assert_eq!(permissions.preset, ToolPermissionPreset::Custom);

    apply_tool_permission_preset(&mut permissions, ToolPermissionPreset::LockedDown);
    assert_eq!(permissions.preset, ToolPermissionPreset::LockedDown);
    assert_eq!(permissions.context_switch, ToolPermissionPolicy::Deny);
    assert_eq!(permissions.library_create, ToolPermissionPolicy::Deny);

    apply_tool_permission_preset(&mut permissions, ToolPermissionPreset::Balanced);
    assert_eq!(permissions.preset, ToolPermissionPreset::Balanced);
    assert_eq!(permissions.context_switch, ToolPermissionPolicy::Ask);
}

#[test]
fn humanizes_project_names_for_context_labels() {
    assert_eq!(humanize_project_name("AdvenTableDays"), "Adven Table Days");
    assert_eq!(
        humanize_project_name("games/adventable-days/overview.md"),
        "Games Adventable Days Overview"
    );
    assert_eq!(humanize_project_name("AIResearch2026"), "AI Research 2026");
}

#[test]
fn library_context_paths_include_descendants() {
    assert!(library_path_contains(
        Path::new("Games"),
        Path::new("Games/AdvenTableDays")
    ));
    assert!(library_path_contains(
        Path::new("Games"),
        Path::new("Games")
    ));
    assert!(!library_path_contains(
        Path::new("Games"),
        Path::new("GameTools")
    ));
}

#[test]
fn parses_context_scope_tokens() {
    assert_eq!(
        parse_context_scope("node").expect("node"),
        ContextScope::Node
    );
    assert_eq!(
        parse_context_scope("node_and_ancestors").expect("ancestors"),
        ContextScope::NodeAndAncestors
    );
    assert!(parse_context_scope("sideways").is_err());
}

#[test]
fn parses_agent_launch_slash_args() {
    let args = split_slash_args(
            r#"launch "Library Project" "inspect current state" --provider codex --read-only --allow-network --yes"#,
        )
        .expect("split args");
    let request = parse_agent_launch_args(&args[1..]).expect("launch args");

    assert_eq!(request.project, "Library Project");
    assert_eq!(request.goal, "inspect current state");
    assert_eq!(request.provider, crate::domain::ProviderKind::Codex);
    assert!(request.read_only);
    assert!(request.allow_network);
    assert!(request.confirmed);
}

#[test]
fn rejects_agent_launch_without_goal() {
    let args = vec!["Project".to_string(), "--yes".to_string()];
    assert!(parse_agent_launch_args(&args).is_err());
}

#[test]
fn parses_project_create_slash_args() {
    let args = split_slash_args(
            r#"create "Library Project" --library "projects/Library Project" --workspace "C:/work/library""#,
        )
        .expect("split args");
    let request = parse_project_create_args(&args[1..]).expect("project args");

    assert_eq!(request.name, "Library Project");
    assert_eq!(
        request.library_path.as_deref(),
        Some("projects/Library Project")
    );
    assert_eq!(request.workspace_path.as_deref(), Some("C:/work/library"));
}

#[test]
fn creates_stable_project_folder_names() {
    assert_eq!(project_folder_name("My Cool Project"), "my-cool-project");
    assert_eq!(project_folder_name("..."), "...");
    assert_eq!(project_folder_name("  "), "project");
    assert_eq!(
        project_workspace_folder_name("AdvenTableDays"),
        "AdvenTableDays"
    );
    assert_eq!(
        project_workspace_folder_name("My Cool Project"),
        "My-Cool-Project"
    );
}

#[test]
fn parses_approval_json_payload() {
    let payload = parse_json_payload(r#"{"tool":"library","path":"note.md"}"#).expect("json");
    assert_eq!(payload["tool"], "library");
    assert!(parse_json_payload("not-json").is_err());
}

#[test]
fn extracts_required_approval_payload_string() {
    let payload = serde_json::json!({ "path": "notes/test.md" });
    assert_eq!(
        approval_payload_string(&payload, "path").expect("path"),
        "notes/test.md"
    );
    assert!(approval_payload_string(&payload, "content").is_err());
}

#[tokio::test]
async fn approval_executor_handles_context_switch() {
    let home = std::env::current_dir().expect("current dir").join(format!(
        ".librarian-test-context-approval-{}",
        Uuid::new_v4()
    ));
    let config = Config::load_or_default(Some(home.clone())).expect("config");
    config.ensure_layout().expect("layout");
    let db = Database::connect(&config).await.expect("db");
    db.migrate().await.expect("migrate");
    let state = AppState {
        db: db.clone(),
        config: Arc::new(RwLock::new(config.clone())),
    };
    let approval = db
        .create_tool_approval(
            "context",
            "switch",
            serde_json::json!({"label":"Games","scope":"subtree","nodes":[]}),
        )
        .await
        .expect("approval");
    let output = execute_approved_tool_approval(&state, &config, &approval)
        .await
        .expect("execute");
    assert_eq!(output["context"]["label"], "Games");
    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn approval_propose_slash_returns_approval_ui_card() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-approval-ui-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let state = AppState {
            db,
            config: Arc::new(RwLock::new(config.clone())),
        };
        let result = execute_approval_slash_command(
            &state,
            &config,
            &[
                "propose".to_string(),
                "library".to_string(),
                "create_folder".to_string(),
                serde_json::json!({
                    "summary": "Create a project shelf.",
                    "library_path": "Games/NewShelf",
                })
                .to_string(),
            ],
        )
        .await
        .expect("approval propose");

        assert_eq!(result.mode, "slash-command");
        assert_eq!(result.reply, "Review this proposed action.");
        let ui = result.ui.expect("ui");
        assert_eq!(ui["type"], "approval");
        assert_eq!(ui["approval"]["tool"], "library");
        assert_eq!(ui["approval"]["action"], "create_folder");
        assert_eq!(ui["approval"]["payload"]["library_path"], "Games/NewShelf");
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn agent_review_packet_slash_returns_chat_card() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-review-card-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let repo = std::env::current_dir().expect("repo");
        let project = db
            .add_project("LibrarianReviewCard", &repo)
            .await
            .expect("project");
        let job = db
            .create_job(
                project.id,
                crate::domain::ProviderKind::Codex,
                "inspect review card",
                MountMode::ReadOnly,
                crate::domain::NetworkMode::Provider,
                None,
            )
            .await
            .expect("job");
        let state = AppState {
            db: db.clone(),
            config: Arc::new(RwLock::new(config.clone())),
        };

        let result = execute_agent_slash_command(
            &state,
            &config,
            &["review-packet".to_string(), job.id.to_string()],
        )
        .await
        .expect("review packet");

        assert!(result.reply.contains("Review packet"));
        let ui = result.ui.expect("ui");
        assert_eq!(ui["type"], "job_review");
        assert_eq!(ui["job_id"], job.id.to_string());
        assert_eq!(ui["packet"]["project"]["name"], "LibrarianReviewCard");
        assert!(ui["packet"]["summary"]["next_step"].is_string());
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn job_git_action_proposal_api_returns_approval_card_payload() {
    let home = std::env::current_dir().expect("current dir").join(format!(
        ".librarian-test-review-proposal-{}",
        Uuid::new_v4()
    ));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let repo = home.join("repo");
        std::fs::create_dir_all(&repo).expect("repo dir");
        let run_git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .expect("git command");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run_git(&["init"]);
        run_git(&["config", "user.email", "smoke@example.invalid"]);
        run_git(&["config", "user.name", "Librarian Smoke"]);
        std::fs::write(repo.join("README.md"), "# Smoke\n").expect("seed file");
        run_git(&["add", "README.md"]);
        run_git(&["commit", "-m", "seed"]);
        run_git(&["checkout", "-b", "feature/review-proposal"]);
        std::fs::write(repo.join("README.md"), "# Smoke\n\nChanged.\n").expect("dirty file");

        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let project = db
            .add_project("ReviewProposalApi", &repo)
            .await
            .expect("project");
        let job = db
            .create_job(
                project.id,
                crate::domain::ProviderKind::Codex,
                "inspect review proposal",
                MountMode::ReadWrite,
                crate::domain::NetworkMode::Provider,
                None,
            )
            .await
            .expect("job");
        let state = AppState {
            db: db.clone(),
            config: Arc::new(RwLock::new(config.clone())),
        };

        let response = propose_job_git_action_api(
            State(state),
            AxumPath(job.id),
            Json(JobGitActionProposalRequest {
                action: "commit".to_string(),
                message: Some("Review proposal smoke".to_string()),
                commit: None,
            }),
        )
        .await
        .expect("proposal response")
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload["approval"]["tool"], "git");
        assert_eq!(payload["approval"]["action"], "commit");
        assert_eq!(payload["approval"]["payload"]["job_id"], job.id.to_string());
        assert_eq!(
            payload["approval"]["payload"]["message"],
            "Review proposal smoke"
        );
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn claude_runtime_settings_endpoint_persists_instruction_file() {
    let home = std::env::current_dir().expect("current dir").join(format!(
        ".librarian-test-claude-settings-{}",
        Uuid::new_v4()
    ));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let state = AppState {
            db: db.clone(),
            config: Arc::new(RwLock::new(config.clone())),
        };

        let response = update_claude_runtime_settings(
            State(state.clone()),
            Json(UpdateClaudeRuntimeRequest {
                host_home: Some(home.join(".cfg").join("claude-home").display().to_string()),
                mount_host_home: Some(true),
                mount_read_only: Some(true),
                instruction_file: Some("PROJECT_CLAUDE.md".to_string()),
            }),
        )
        .await
        .expect("settings response")
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["claude"]["instruction_file"], "PROJECT_CLAUDE.md");

        let saved = state.config.read().await.clone();
        assert!(saved.claude.mount_host_home);
        assert!(saved.claude.mount_read_only);
        assert_eq!(saved.claude.instruction_file, "PROJECT_CLAUDE.md");
        assert!(db
            .list_system_events(5)
            .await
            .expect("events")
            .iter()
            .any(|event| event.kind == "claude_runtime_updated"));
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn provider_smoke_endpoint_exposes_dry_run_command() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-provider-smoke-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let state = AppState {
            db,
            config: Arc::new(RwLock::new(config)),
        };

        let response = provider_smoke(
            State(state),
            AxumPath("claude-code".to_string()),
            Query(ProviderSmokeQuery {
                dry_run: Some(true),
            }),
        )
        .await
        .expect("smoke response")
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload["provider"], "claude-code");
        assert_eq!(payload["dry_run"], true);
        let command = payload["command"].as_array().expect("command");
        assert!(command.iter().any(|part| part == "smoke"));
        assert!(command.iter().any(|part| part == "mvp"));
        assert!(command.iter().any(|part| part == "claude-code"));
    }

    std::fs::remove_dir_all(home).ok();
}

#[test]
fn admin_external_bind_requires_auth_token() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-admin-auth-{}", Uuid::new_v4()));
    let mut config = Config::load_or_default(Some(home.clone())).expect("config");

    assert!(validate_admin_auth_for_bind("127.0.0.1:17377", &config).is_ok());
    assert!(validate_admin_auth_for_bind("localhost:17377", &config).is_ok());
    assert!(validate_admin_auth_for_bind("0.0.0.0:17377", &config).is_err());

    config.admin.auth_enabled = true;
    config.admin.auth_token = Some("secret-token".to_string());
    assert!(validate_admin_auth_for_bind("0.0.0.0:17377", &config).is_ok());
    assert!(validate_admin_auth_for_bind("[::]:17377", &config).is_ok());

    std::fs::remove_dir_all(home).ok();
}

#[test]
fn normalizes_approval_library_paths_from_user_text() {
    let payload = serde_json::json!({
        "user_message": "Создай стартовую документацию в /Library/Games/AdvenTableDays/ и пустую папку проекта."
    });
    assert_eq!(
        approval_project_library_path(&payload).expect("library path"),
        "Games/AdvenTableDays"
    );
}

#[tokio::test]
async fn approval_approve_executes_project_starting_docs_without_secret_key() {
    let home = std::env::current_dir().expect("current dir").join(format!(
        ".librarian-test-approval-project-{}",
        Uuid::new_v4()
    ));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        let approval = db
                .create_tool_approval(
                    "project",
                    "create_starting_docs_and_project_folder",
                    serde_json::json!({
                        "summary": "Create starter docs for AdvenTable Days.",
                        "user_message": "Создай стартовую документацию в /Library/Games/AdvenTableDays/ и пустую папку проекта."
                    }),
                )
                .await
                .expect("approval");
        let state = AppState {
            db: db.clone(),
            config: Arc::new(RwLock::new(config.clone())),
        };

        let result = execute_approval_slash_command(
            &state,
            &config,
            &["approve".to_string(), approval.id.to_string()],
        )
        .await
        .expect("approve");

        assert!(result.reply.contains("Approved and executed"));
        assert!(home
            .join("Library")
            .join("Games")
            .join("AdvenTableDays")
            .join("Overview.md")
            .is_file());
        assert!(home.join("Projects").join("AdvenTableDays").is_dir());
        let project = db
            .get_project_by_name_or_id("AdvenTableDays")
            .await
            .expect("project");
        assert_eq!(
            project.library_path.as_deref(),
            Some(std::path::Path::new("Games/AdvenTableDays"))
        );
        assert_eq!(
            db.get_tool_approval(approval.id)
                .await
                .expect("approval")
                .status,
            ToolApprovalStatus::Executed
        );
    }

    std::fs::remove_dir_all(home).ok();
}

#[tokio::test]
async fn approval_executor_handles_library_edits_and_workspace_moves() {
    let home = std::env::current_dir()
        .expect("current dir")
        .join(format!(".librarian-test-approval-tools-{}", Uuid::new_v4()));

    {
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");
        let db = Database::connect(&config).await.expect("db");
        db.migrate().await.expect("migrate");
        library_tools::write_markdown(&config, "notes/demo.md", "one\ntwo\nthree\n")
            .expect("seed note");
        library_tools::create_empty_file(&config, LibraryRoot::Projects, "Demo/old.txt")
            .expect("seed workspace file");
        let state = AppState {
            db: db.clone(),
            config: Arc::new(RwLock::new(config.clone())),
        };

        let edit = db
            .create_tool_approval(
                "library",
                "replace_lines",
                serde_json::json!({
                    "path": "notes/demo.md",
                    "start_line": 2,
                    "end_line": 2,
                    "content": "TWO\n"
                }),
            )
            .await
            .expect("edit approval");
        execute_approval_slash_command(
            &state,
            &config,
            &["approve".to_string(), edit.id.to_string()],
        )
        .await
        .expect("approve edit");
        assert_eq!(
            library_tools::read_markdown(&config, "notes/demo.md").expect("read note"),
            "one\nTWO\nthree\n"
        );

        let move_file = db
            .create_tool_approval(
                "workspace",
                "move",
                serde_json::json!({ "from": "Demo/old.txt", "to": "Demo/new.txt" }),
            )
            .await
            .expect("move approval");
        execute_approval_slash_command(
            &state,
            &config,
            &["approve".to_string(), move_file.id.to_string()],
        )
        .await
        .expect("approve move");
        assert!(home.join("Projects").join("Demo").join("new.txt").is_file());
        assert!(!home.join("Projects").join("Demo").join("old.txt").exists());
    }

    std::fs::remove_dir_all(home).ok();
}

#[test]
fn maps_library_entries_to_visual_kinds() {
    let markdown = library_tools::LibraryEntry {
        name: "Book.md".to_string(),
        path: "Book.md".to_string(),
        root: LibraryRoot::Library,
        kind: library_tools::LibraryEntryKind::Markdown,
        children: Vec::new(),
    };
    let shelf = library_tools::LibraryEntry {
        name: "Shelf".to_string(),
        path: "Shelf".to_string(),
        root: LibraryRoot::Library,
        kind: library_tools::LibraryEntryKind::Folder,
        children: vec![markdown.clone()],
    };
    let rack = library_tools::LibraryEntry {
        name: "Rack".to_string(),
        path: "Rack".to_string(),
        root: LibraryRoot::Library,
        kind: library_tools::LibraryEntryKind::Folder,
        children: vec![shelf.clone()],
    };

    assert_eq!(project_visual_kind(&markdown), "book");
    assert_eq!(project_visual_kind(&shelf), "shelf");
    assert_eq!(project_visual_kind(&rack), "rack");
}

#[test]
fn parses_prompt_add_block_args() {
    let args = split_slash_args(r#"add-block agents identity "You are Librarian" --plain"#)
        .expect("split args");
    let request = parse_prompt_add_block_args(&args[1..]).expect("prompt args");

    assert_eq!(request.target, "agents");
    assert_eq!(request.name, "identity");
    assert_eq!(request.content, "You are Librarian");
    assert!(!request.markdown);
}

#[test]
fn renders_enabled_prompt_blocks_only() {
    let now = chrono::Utc::now();
    let blocks = vec![
        crate::domain::PromptBlock {
            id: Uuid::new_v4(),
            target: "agents".to_string(),
            name: "one".to_string(),
            content: "First".to_string(),
            enabled: true,
            position: 1,
            markdown: true,
            created_at: now,
            updated_at: now,
        },
        crate::domain::PromptBlock {
            id: Uuid::new_v4(),
            target: "agents".to_string(),
            name: "two".to_string(),
            content: "Second".to_string(),
            enabled: false,
            position: 2,
            markdown: true,
            created_at: now,
            updated_at: now,
        },
    ];

    assert_eq!(render_prompt_blocks(&blocks), "First");
}
