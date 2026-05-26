use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use anyhow::Result;
use axum::{
    extract::{Path as AxumPath, Query, State},
    response::{Html, IntoResponse},
    routing::{get, patch, post},
    Json, Router,
};
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

use crate::{
    admin_models::*,
    chat::{self, LibrarianChatResult},
    config::{Config, ToolPermissionPolicy, ToolPermissionPreset, ToolPermissionsConfig},
    db::Database,
    domain::{
        JobStatus, MemoryKind, MountMode, Project, ScheduleKind, ScheduleStatus, ToolApprovalStatus,
    },
    gates, library_tools,
    library_tools::LibraryRoot,
    memory, router, scheduler,
    secrets::SecretVault,
    slash_utils::split_slash_args,
    third_eye, worker,
};

#[derive(Clone)]
struct AppState {
    db: Database,
    config: Arc<RwLock<Config>>,
}

#[derive(Clone, Debug)]
struct ChatProjectContext {
    nodes: Vec<ChatLibraryContextNode>,
    suggested_nodes: Vec<ChatLibraryContextNode>,
    source: &'static str,
}

#[derive(Clone, Debug)]
struct ChatLibraryContextNode {
    library_path: Option<PathBuf>,
    project: Option<Project>,
}

impl ChatProjectContext {
    fn primary_project(&self) -> Option<&Project> {
        self.nodes.iter().find_map(|node| node.project.as_ref())
    }

    fn primary_project_id(&self) -> Option<Uuid> {
        self.primary_project().map(|project| project.id)
    }

    fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn label(&self) -> String {
        context_label_for_nodes(&self.nodes)
    }

    fn suggested_label(&self) -> String {
        context_label_for_nodes(&self.suggested_nodes)
    }

    fn has_suggestion(&self) -> bool {
        !self.suggested_nodes.is_empty()
    }

    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "source": self.source,
            "label": self.label(),
            "nodes": self.nodes.iter().map(library_context_metadata).collect::<Vec<_>>(),
            "suggested_nodes": self.suggested_nodes.iter().map(library_context_metadata).collect::<Vec<_>>(),
            "projects": self.nodes.iter().filter_map(|node| node.project.as_ref()).map(project_context_metadata).collect::<Vec<_>>(),
        })
    }
}

fn chat_first_app_html(bind: &str, worker_concurrency: usize) -> String {
    let html = r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Librarian</title>
  <style>
    :root {
      color-scheme: dark;
      --bg: #101214;
      --panel: #181d21;
      --panel-2: #20272d;
      --text: #edf1f5;
      --muted: #99a6b2;
      --line: #303941;
      --accent: #62c7a8;
      --accent-2: #8fb7ff;
      --chrome: #e4c16f;
      --chrome-hover: #ffd98a;
      --danger: #c76f6f;
      --shadow: 0 18px 60px rgba(0, 0, 0, .38);
      --edge-control-space: 68px;
    }
    * { box-sizing: border-box; }
    html, body {
      width: 100%;
      height: 100%;
      min-width: 860px;
      min-height: 560px;
      overflow: hidden;
    }
    body {
      margin: 0;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: var(--bg);
      color: var(--text);
    }
    button, textarea, input, select { font: inherit; }
    button {
      border: 1px solid transparent;
      border-radius: 6px;
      min-height: 38px;
      padding: 0 13px;
      cursor: pointer;
      background: var(--accent);
      color: #06100d;
      font-weight: 700;
    }
    button.secondary {
      background: var(--panel-2);
      border-color: var(--line);
      color: var(--text);
    }
    button.danger { background: var(--danger); color: #fff; }
    input, select, textarea {
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #11161a;
      color: var(--text);
    }
    input, select { height: 38px; padding: 0 10px; }
    textarea { padding: 12px; resize: none; line-height: 1.45; }
    label {
      display: block;
      margin: 0 0 6px;
      color: var(--muted);
      font-size: 12px;
    }
    h1, h2, h3, p { margin: 0; }
    h2 {
      margin: 0 0 14px;
      font-size: 18px;
      letter-spacing: 0;
    }
    h3 {
      margin: 0 0 8px;
      font-size: 14px;
      letter-spacing: 0;
    }
    .app {
      height: 100dvh;
      min-height: 560px;
      display: grid;
      grid-template-rows: minmax(0, 1fr) auto;
      overflow: hidden;
    }
    .topbar {
      position: fixed;
      top: 0;
      left: 0;
      right: 0;
      z-index: 6;
      height: 64px;
      pointer-events: none;
    }
    .brand {
      position: absolute;
      top: 0;
      left: 50%;
      transform: translateX(-50%);
      min-width: 210px;
      padding: 8px 22px 10px;
      border: 1px solid var(--line);
      border-top: 0;
      border-radius: 0 0 18px 18px;
      background: rgba(18, 22, 25, .96);
      box-shadow: var(--shadow);
      text-align: center;
      line-height: 1.2;
      font-weight: 800;
      pointer-events: auto;
    }
    .brand span {
      display: block;
      margin-top: 2px;
      color: var(--accent);
      font-size: 11px;
      font-weight: 700;
    }
    .brand .context {
      color: var(--muted);
      font-size: 10px;
      font-style: italic;
      font-weight: 500;
    }
    .icon-button {
      position: absolute;
      top: 10px;
      width: 44px;
      height: 44px;
      min-height: 44px;
      padding: 0;
      display: grid;
      place-items: center;
      background: transparent;
      border-color: transparent;
      color: var(--chrome);
      pointer-events: auto;
      transition: color .16s ease, transform .18s cubic-bezier(.2, 1.4, .4, 1);
    }
    #settings-open { left: 12px; }
    #projects-open { right: 12px; }
    #new-chat {
      right: 66px;
      font-size: 22px;
      font-weight: 700;
    }
    .icon-button:hover, .icon-button:focus-visible {
      color: var(--chrome-hover);
      background: transparent;
      border-color: transparent;
      transform: translateY(-2px) scale(1.08);
      outline: none;
    }
    .settings-icon, .map-icon {
      position: relative;
      display: block;
      width: 24px;
      height: 24px;
    }
    .settings-icon::before,
    .settings-icon::after {
      content: "";
      position: absolute;
      left: 3px;
      right: 3px;
      height: 2px;
      background: currentColor;
      border-radius: 2px;
      box-shadow: 0 8px 0 currentColor, 0 16px 0 currentColor;
    }
    .settings-icon::after {
      top: 1px;
      left: 7px;
      right: auto;
      width: 4px;
      height: 4px;
      border-radius: 50%;
      box-shadow: -2px 8px 0 currentColor, 7px 16px 0 currentColor;
      background: currentColor;
    }
    .map-icon::before {
      content: "";
      position: absolute;
      left: 11px;
      top: 4px;
      width: 2px;
      height: 16px;
      background: currentColor;
      box-shadow: -7px 7px 0 -1px currentColor, 7px 7px 0 -1px currentColor;
    }
    .map-icon::after {
      content: "";
      position: absolute;
      left: 8px;
      top: 1px;
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: currentColor;
      box-shadow: -8px 15px 0 -1px currentColor, 8px 15px 0 -1px currentColor;
    }
    .chat-log {
      min-height: 0;
      overflow: auto;
      padding: 86px clamp(12px, var(--edge-control-space), 72px) 28px;
      scroll-behavior: smooth;
    }
    .thread {
      width: min(100%, calc(100vw - (var(--edge-control-space) * 2)));
      margin: 0 auto;
      display: flex;
      flex-direction: column;
      gap: 14px;
    }
    .message {
      max-width: 100%;
      padding: 13px 15px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
      white-space: pre-wrap;
      line-height: 1.45;
    }
    .message.user {
      align-self: flex-end;
      background: #1d2b29;
      border-color: #2f5a50;
    }
    .message.assistant, .message.system { align-self: flex-start; }
    .message.system { color: var(--muted); }
    .message.command {
      border-color: rgba(143, 183, 255, .5);
      background: #171f29;
      color: var(--text);
    }
    .message.command small { color: var(--accent-2); }
    .message.thinking {
      color: var(--muted);
      border-style: dashed;
    }
    .message.approval {
      border-color: rgba(228, 193, 111, .58);
      background: #211f18;
    }
    .approval-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      margin-bottom: 10px;
      font-weight: 800;
    }
    .approval-risk {
      color: var(--chrome);
      font-size: 12px;
      text-transform: uppercase;
    }
    .approval-summary {
      color: var(--text);
      margin-bottom: 10px;
    }
    .approval-paths {
      display: grid;
      gap: 5px;
      margin: 0 0 12px;
      color: var(--muted);
      font-size: 13px;
    }
    .approval-actions {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
      margin-top: 10px;
    }
    .approval-actions button {
      min-height: 34px;
    }
    .approval-actions .reject {
      background: transparent;
      border-color: rgba(199, 111, 111, .7);
      color: #efb1b1;
    }
    .approval-status {
      margin-top: 10px;
      color: var(--muted);
      font-size: 13px;
    }
    .thinking-dots {
      display: inline-flex;
      gap: 4px;
      margin-left: 6px;
      vertical-align: middle;
    }
    .thinking-dots i {
      width: 5px;
      height: 5px;
      border-radius: 50%;
      background: currentColor;
      animation: thinking-pulse 1s infinite ease-in-out;
    }
    .thinking-dots i:nth-child(2) { animation-delay: .15s; }
    .thinking-dots i:nth-child(3) { animation-delay: .3s; }
    @keyframes thinking-pulse {
      0%, 80%, 100% { opacity: .25; transform: translateY(0); }
      40% { opacity: 1; transform: translateY(-3px); }
    }
    .message small {
      display: block;
      margin-top: 8px;
      color: var(--muted);
    }
    .composer {
      border-top: 1px solid var(--line);
      background: rgba(18, 22, 25, .98);
      padding: 12px 14px 14px;
      position: relative;
    }
    .composer-inner {
      width: 100%;
      margin: 0;
      display: block;
    }
    #goal-input {
      height: 112px;
      max-height: 38vh;
      resize: vertical;
    }
    .slash-palette {
      position: absolute;
      left: 14px;
      right: 14px;
      bottom: calc(100% + 8px);
      max-height: 260px;
      overflow: auto;
      display: none;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: rgba(18, 22, 25, .98);
      box-shadow: var(--shadow);
      padding: 8px;
      z-index: 8;
    }
    .slash-palette.open { display: grid; gap: 4px; }
    .slash-option {
      min-height: 36px;
      border-radius: 6px;
      padding: 7px 9px;
      display: grid;
      grid-template-columns: minmax(150px, 240px) minmax(0, 1fr);
      gap: 12px;
      color: var(--muted);
      cursor: pointer;
    }
    .slash-option.active,
    .slash-option:hover {
      background: var(--panel-2);
      color: var(--text);
    }
    .slash-option code {
      color: var(--accent);
      white-space: nowrap;
    }
    .overlay {
      position: fixed;
      inset: 0;
      z-index: 10;
      display: none;
      grid-template-rows: 58px minmax(0, 1fr);
      background: var(--bg);
    }
    .overlay.open { display: grid; }
    .overlay-head {
      display: grid;
      grid-template-columns: 64px minmax(0, 1fr) 64px;
      align-items: center;
      position: relative;
      border-bottom: 1px solid var(--line);
      background: rgba(18, 22, 25, .96);
    }
    .overlay-head .icon-button {
      position: static;
      margin: 0 auto;
    }
    .overlay-head .icon-button:hover, .overlay-head .icon-button:focus-visible {
      transform: none;
    }
    .overlay-title {
      justify-self: center;
      font-weight: 800;
    }
    .overlay-body {
      min-height: 0;
      display: grid;
      grid-template-columns: 220px minmax(0, 1fr);
      overflow: hidden;
    }
    .tabs {
      border-right: 1px solid var(--line);
      padding: 18px 12px;
      background: #12161a;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .tab-button {
      justify-content: flex-start;
      background: transparent;
      border-color: transparent;
      color: var(--muted);
      text-align: left;
    }
    .tab-button.active {
      background: var(--panel-2);
      border-color: var(--line);
      color: var(--text);
    }
    .tab-content {
      min-height: 0;
      overflow: auto;
      padding: 24px clamp(22px, 4vw, 54px);
    }
    .tab-pane { display: none; max-width: 980px; }
    .tab-pane.active { display: block; }
    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      gap: 12px;
    }
    .card {
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
      padding: 14px;
      line-height: 1.45;
    }
    .card.action {
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
      overflow-wrap: anywhere;
    }
    .muted { color: var(--muted); }
    .tiny { font-size: 12px; }
    .stack { display: grid; gap: 12px; }
    .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
    .form-grid {
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 10px;
      align-items: end;
    }
    .form-grid .wide { grid-column: span 2; }
    .prompt-block {
      border-top: 1px solid var(--line);
      padding-top: 12px;
    }
    .prompt-block textarea {
      min-height: 118px;
      resize: vertical;
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 13px;
    }
    .project-stage {
      min-height: 0;
      overflow: auto;
      padding: 28px clamp(20px, 5vw, 70px);
    }
    .project-layout {
      display: grid;
      grid-template-columns: minmax(280px, 380px) minmax(0, 1fr);
      gap: 18px;
      min-height: 100%;
    }
    .project-map {
      min-height: 420px;
      position: relative;
      overflow: auto;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #0f1417;
      padding: 24px;
    }
    .map-legend {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
      margin-bottom: 14px;
    }
    .legend-chip {
      border: 1px solid var(--line);
      border-radius: 999px;
      padding: 4px 9px;
      color: var(--muted);
      font-size: 12px;
    }
    .tree {
      min-width: 640px;
      display: flex;
      align-items: flex-start;
      gap: 28px;
      position: relative;
    }
    .tree::before {
      content: "";
      position: absolute;
      left: 252px;
      top: 24px;
      width: 28px;
      border-top: 1px solid rgba(98, 199, 168, .45);
    }
    .node-column {
      display: flex;
      flex-direction: column;
      gap: 18px;
      align-items: center;
    }
    .node {
      min-width: 210px;
      max-width: 260px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
      box-shadow: var(--shadow);
      padding: 14px;
      text-align: left;
    }
    .node.active { border-color: var(--accent); }
    .node.book { border-color: #8fb7ff; }
    .node.shelf { border-color: #62c7a8; }
    .node.rack { border-color: #e4c16f; }
    .node.artifact { border-color: #99a6b2; }
    .node.root {
      background: #1d2529;
      text-align: center;
    }
    .node .badge {
      display: inline-block;
      margin-bottom: 6px;
      color: var(--muted);
      font-size: 11px;
      text-transform: uppercase;
    }
    .node button { width: 100%; margin-top: 10px; }
    .empty {
      width: min(560px, 100%);
      margin: 12vh auto 0;
      text-align: center;
    }
    .empty .card { text-align: left; }
    @media (max-width: 900px), (max-height: 600px) {
      html, body { min-width: 720px; min-height: 500px; }
      :root { --edge-control-space: 12px; }
      .app { min-height: 500px; }
      .chat-log { padding: 78px 12px 18px; }
      .composer { padding: 10px 12px; }
      .overlay-body { grid-template-columns: 180px minmax(0, 1fr); }
      #goal-input { height: 88px; }
    }
  </style>
</head>
<body>
  <div class="app">
    <header class="topbar">
      <button id="settings-open" class="icon-button" type="button" aria-label="Settings" title="Settings"><span class="settings-icon"></span></button>
      <div class="brand">Librarian<span id="motto-line">Smart. Silent. Steady.</span><span id="context-line" class="context">Context: Global conversation</span></div>
      <button id="new-chat" class="icon-button" type="button" aria-label="New chat" title="New chat">+</button>
      <button id="projects-open" class="icon-button" type="button" aria-label="Projects" title="Projects"><span class="map-icon"></span></button>
    </header>
    <main id="chat-log" class="chat-log">
      <div id="thread" class="thread">
        <article class="message assistant">Ready. Write what you want Librarian to do.</article>
      </div>
    </main>
    <form id="chat-form" class="composer" autocomplete="off">
      <div id="slash-palette" class="slash-palette" role="listbox" aria-label="Slash commands"></div>
      <div class="composer-inner">
        <textarea id="goal-input" name="goal" placeholder="Message Librarian" autocomplete="off" required></textarea>
      </div>
    </form>
  </div>

  <section id="settings-overlay" class="overlay" aria-hidden="true">
    <header class="overlay-head">
      <button class="icon-button" type="button" data-close="settings-overlay" aria-label="Close settings">X</button>
      <div class="overlay-title">Settings</div>
      <span></span>
    </header>
    <div class="overlay-body">
      <nav class="tabs">
        <button class="tab-button active" type="button" data-tab="overview">Overview</button>
        <button class="tab-button" type="button" data-tab="chats">Chats</button>
        <button class="tab-button" type="button" data-tab="providers">Providers</button>
        <button class="tab-button" type="button" data-tab="jobs">Jobs</button>
        <button class="tab-button" type="button" data-tab="prompt">Prompt</button>
        <button class="tab-button" type="button" data-tab="system">System</button>
      </nav>
      <div class="tab-content">
        <section class="tab-pane active" data-pane="overview"><h2>Overview</h2><div id="overview" class="grid"></div></section>
        <section class="tab-pane" data-pane="chats"><h2>Chats</h2><div id="chat-sessions" class="stack"></div></section>
        <section class="tab-pane" data-pane="providers"><h2>Providers</h2><div id="providers" class="grid"></div></section>
        <section class="tab-pane" data-pane="jobs"><h2>Jobs</h2><div id="jobs" class="stack"></div></section>
        <section class="tab-pane" data-pane="prompt"><h2>Prompt Blocks</h2><div id="prompt-builder" class="stack"></div></section>
        <section class="tab-pane" data-pane="system"><h2>System</h2><div id="system-events" class="stack"></div></section>
      </div>
    </div>
  </section>

  <section id="projects-overlay" class="overlay" aria-hidden="true">
    <header class="overlay-head">
      <button class="icon-button" type="button" data-close="projects-overlay" aria-label="Close projects">X</button>
      <div class="overlay-title">Projects</div>
      <span></span>
    </header>
    <div id="project-stage" class="project-stage"></div>
  </section>

  <script>
    (() => {
      const state = {
        projects: [],
        projectMap: null,
        promptBlocks: [],
        jobs: [],
        chatSessions: [],
        providers: { catalog: [], states: [] },
        health: null,
        activeProject: '',
        activeContext: [],
        chatSessionId: null,
        inputHistory: [],
        historyIndex: null,
        draftInput: '',
        slashIndex: 0,
        slashOpen: false,
        slashCommands: []
      };
      const fallbackSlashCommands = [
        ['/help', 'Show available command groups'],
        ['/lib help', 'Knowledge base files and Markdown tools'],
        ['/lib tree', 'Show the Library tree'],
        ['/lib read ', 'Read a Markdown note'],
        ['/lib append ', 'Append to a Markdown note'],
        ['/lib replace-lines ', 'Replace a line range in a note'],
        ['/lib replace-find ', 'Replace the first search match in a note'],
        ['/work help', 'Project workspace folder tools'],
        ['/work mkdir ', 'Create a workspace folder'],
        ['/work touch ', 'Create an empty workspace file'],
        ['/project help', 'Project records and attachments'],
        ['/project list', 'List registered projects'],
        ['/project create ', 'Create a library project'],
        ['/project attach-workspace ', 'Attach an existing workspace directory'],
        ['/mem help', 'Durable memory tools'],
        ['/remember ', 'Remember a durable fact'],
        ['/mem recent', 'Show recent durable memory'],
        ['/approval list', 'Review pending approvals'],
        ['/prompt blocks', 'List prompt blocks'],
        ['/settings tool-permissions', 'Show tool permission policy'],
        ['/agent list', 'List background agent jobs'],
        ['/agent preflight ', 'Prepare a job command without running it'],
        ['/agent launch ', 'Queue an explicit background agent job']
      ];
      const el = id => document.getElementById(id);
      const qsa = selector => Array.from(document.querySelectorAll(selector));
      const htmlEscape = value => String(value ?? '').replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
      const shortId = value => value ? String(value).slice(0, 8) : '';
      function humanProjectName(value) {
        return String(value || '')
          .replace(/\.md$/i, '')
          .replace(/[\\/]/g, ' ')
          .replace(/[_-]+/g, ' ')
          .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
          .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
          .replace(/([A-Za-z])([0-9])/g, '$1 $2')
          .replace(/([0-9])([A-Za-z])/g, '$1 $2')
          .split(/\s+/)
          .filter(Boolean)
          .map(part => part.charAt(0).toUpperCase() + part.slice(1))
          .join(' ');
      }
      function projectDisplayName(project) {
        if (!project) return '';
        const source = project.context_path || project.library_path || project.name || '';
        const last = String(source).split(/[\\/]/).filter(Boolean).pop() || project.name;
        return humanProjectName(last);
      }
      function contextNodeFromMetadata(node) {
        const project = node?.project || {};
        return {
          id: project.id || node?.id || '',
          name: project.name || node?.context_path || node?.library_path || node?.label || '',
          library_path: node?.context_path || node?.library_path || project.context_path || project.library_path || '',
          context_path: node?.context_path || node?.library_path || project.context_path || project.library_path || '',
          path: project.workspace_path || project.path || '',
          label: node?.label || project.display_name || ''
        };
      }
      function contextLabelFromProjects(projects) {
        return Array.isArray(projects) && projects.length
          ? projects.map(projectDisplayName).join(' + ')
          : 'Global conversation';
      }
      function currentContextProjects() {
        if (state.activeContext.length) return state.activeContext;
        const active = state.projects.find(project => project.name === state.activeProject);
        return active ? [active] : [];
      }
      function currentContextLabel() {
        return contextLabelFromProjects(currentContextProjects());
      }
      function turnContextLabel(turn) {
        return turn?.metadata?.context?.label || (turn?.metadata?.project ? humanProjectName(turn.metadata.project) : 'Global conversation');
      }

      function openOverlay(id) {
        el(id).classList.add('open');
        el(id).setAttribute('aria-hidden', 'false');
      }
      function closeOverlay(id) {
        el(id).classList.remove('open');
        el(id).setAttribute('aria-hidden', 'true');
      }
      function setTab(name) {
        qsa('.tab-button').forEach(button => button.classList.toggle('active', button.dataset.tab === name));
        qsa('.tab-pane').forEach(pane => pane.classList.toggle('active', pane.dataset.pane === name));
      }
      function appendMessage(role, text, detail, contextLabel) {
        const article = document.createElement('article');
        article.className = `message ${role}`;
        setMessage(article, text, detail, contextLabel);
        el('thread').appendChild(article);
        el('chat-log').scrollTop = el('chat-log').scrollHeight;
        return article;
      }
      function appendThinkingMessage() {
        const article = document.createElement('article');
        article.className = 'message assistant thinking';
        article.innerHTML = 'Thinking<span class="thinking-dots"><i></i><i></i><i></i></span>';
        el('thread').appendChild(article);
        el('chat-log').scrollTop = el('chat-log').scrollHeight;
        return article;
      }
      function setApprovalCard(article, approval, fallbackText, detail) {
        const payload = approval?.payload || {};
        const paths = [];
        if (payload.library_path) paths.push(`Knowledge base: ${payload.library_path}`);
        if (payload.workspace_path) paths.push(`Project folder: ${payload.workspace_path}`);
        if (Array.isArray(payload.files)) {
          for (const file of payload.files.slice(0, 4)) {
            if (file?.path) paths.push(`File: ${file.path}`);
          }
        }
        const summary = payload.summary || fallbackText || `${approval?.tool || 'tool'} ${approval?.action || 'action'}`;
        const terminal = approval?.status && approval.status !== 'Pending';
        article.className = 'message assistant approval';
        article.innerHTML = `
          <div class="approval-head"><span>Approval needed</span><span class="approval-risk">Review</span></div>
          <div class="approval-summary">${htmlEscape(summary)}</div>
          <div class="approval-paths">${paths.length ? paths.map(path => `<div>${htmlEscape(path)}</div>`).join('') : '<div>No paths declared.</div>'}</div>
          <div class="approval-actions">
            <button type="button" data-approval-decision="approve">Approve</button>
            <button type="button" class="reject" data-approval-decision="reject">Reject</button>
          </div>
          <details><summary>Technical details</summary><pre>${htmlEscape(JSON.stringify({ id: approval?.id, tool: approval?.tool, action: approval?.action, payload }, null, 2))}</pre></details>
          ${terminal ? `<div class="approval-status">${htmlEscape(approval.status)}</div>` : ''}
        `;
        if (detail) {
          const small = document.createElement('small');
          small.textContent = detail;
          article.appendChild(small);
        }
        article.querySelectorAll('[data-approval-decision]').forEach(button => {
          if (terminal) button.disabled = true;
          button.addEventListener('click', () => decideApproval(article, approval?.id, button.dataset.approvalDecision));
        });
        el('chat-log').scrollTop = el('chat-log').scrollHeight;
      }
      function setContextSwitchCard(article, ui, detail) {
        const context = ui?.context || {};
        const nodes = Array.isArray(context.nodes) ? context.nodes.map(contextNodeFromMetadata) : [];
        const label = context.label || ui?.label || contextLabelFromProjects(nodes);
        article.className = 'message assistant approval context-switch';
        article.innerHTML = `
          <div class="approval-head"><span>Context suggestion</span><span class="approval-risk">Review</span></div>
          <div class="approval-summary">Switch this chat to <strong>${htmlEscape(label)}</strong>?</div>
          <div class="approval-paths">${nodes.length ? nodes.map(node => `<div>${htmlEscape(node.library_path || node.name)}</div>`).join('') : '<div>No context nodes declared.</div>'}</div>
          <div class="approval-actions">
            <button type="button" data-context-decision="approve">Switch context</button>
            <button type="button" class="reject" data-context-decision="reject">Keep current</button>
          </div>
        `;
        if (detail) {
          const small = document.createElement('small');
          small.textContent = `${detail} - proposed context: ${label}`;
          article.appendChild(small);
        }
        article.querySelector('[data-context-decision="approve"]').addEventListener('click', () => {
          state.activeContext = nodes;
          state.activeProject = nodes[0]?.name || '';
          renderContext();
          appendMessage('system', `Context switched to ${label}.`, 'Context');
          article.querySelectorAll('[data-context-decision]').forEach(button => button.disabled = true);
        });
        article.querySelector('[data-context-decision="reject"]').addEventListener('click', () => {
          appendMessage('system', 'Context suggestion dismissed.', 'Context');
          article.querySelectorAll('[data-context-decision]').forEach(button => button.disabled = true);
        });
        el('chat-log').scrollTop = el('chat-log').scrollHeight;
      }
      async function decideApproval(article, approvalId, decision) {
        if (!approvalId) return;
        const buttons = Array.from(article.querySelectorAll('[data-approval-decision]'));
        buttons.forEach(button => button.disabled = true);
        let status = article.querySelector('.approval-status');
        if (!status) {
          status = document.createElement('div');
          status.className = 'approval-status';
          article.appendChild(status);
        }
        status.textContent = decision === 'approve' ? 'Approving and running...' : 'Rejecting...';
        try {
          const response = await fetch(`/api/approvals/${encodeURIComponent(approvalId)}/${decision}`, { method: 'POST' });
          const data = await response.json();
          if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
          status.textContent = decision === 'approve' ? 'Approved and executed.' : 'Rejected.';
          await refresh();
        } catch (error) {
          status.textContent = `Could not ${decision}: ${error.message || error}`;
          buttons.forEach(button => button.disabled = false);
        }
      }
      function setMessage(article, text, detail, contextLabel) {
        article.classList.remove('thinking');
        article.textContent = text;
        const context = contextLabel || currentContextLabel();
        if (detail !== undefined && detail !== null && detail !== '') {
          const small = document.createElement('small');
          small.textContent = `${detail} - context: ${context}`;
          article.appendChild(small);
          article.title = `${detail} - context: ${context}`;
        } else if (context) {
          const small = document.createElement('small');
          small.textContent = `context: ${context}`;
          small.style.fontStyle = 'italic';
          article.appendChild(small);
          article.title = `context: ${context}`;
        }
        el('chat-log').scrollTop = el('chat-log').scrollHeight;
      }
      function activeProjectName() {
        return state.activeProject || '';
      }
      async function loadJson(path, fallback) {
        try {
          const response = await fetch(path);
          if (!response.ok) return fallback;
          return await response.json();
        } catch (_) {
          return fallback;
        }
      }
      async function refresh() {
        const [health, projects, projectMap, promptBlocks, jobs, chatSessions, providers, events] = await Promise.all([
          loadJson('/api/health', null),
          loadJson('/api/projects', []),
          loadJson('/api/project-map', null),
          loadJson('/api/prompt-blocks', []),
          loadJson('/api/jobs', []),
          loadJson('/api/chat/sessions?limit=20', []),
          loadJson('/api/providers', { catalog: [], states: [] }),
          loadJson('/api/system-events', [])
        ]);
        state.health = health;
        state.projects = Array.isArray(projects) ? projects : [];
        state.projectMap = projectMap;
        state.promptBlocks = Array.isArray(promptBlocks) ? promptBlocks : [];
        state.jobs = Array.isArray(jobs) ? jobs : [];
        state.chatSessions = Array.isArray(chatSessions) ? chatSessions : [];
        state.providers = providers || { catalog: [], states: [] };
        if (!state.projects.some(project => project.name === state.activeProject)) {
          state.activeProject = '';
        }
        state.activeContext = state.activeContext.filter(active =>
          state.projects.some(project => project.id === active.id || project.name === active.name)
        );
        renderOverview();
        renderChatSessions();
        renderProviders();
        renderJobs();
        renderPromptBuilder();
        renderSystemEvents(events);
        renderProjects();
        renderContext();
      }
      async function restoreLatestChatSession() {
        if (state.chatSessionId) return;
        const sessions = await loadJson('/api/chat/sessions?limit=1', []);
        if (!Array.isArray(sessions) || !sessions.length) return;
        await restoreChatSession(sessions[0].id, false);
      }
      async function restoreChatSession(sessionId, announce) {
        if (!sessionId) return;
        const transcript = await loadJson(`/api/chat/sessions/${encodeURIComponent(sessionId)}/turns`, null);
        if (!transcript || !Array.isArray(transcript.turns)) return;
        state.chatSessionId = transcript.session?.id || sessionId;
        const thread = el('thread');
        thread.innerHTML = '';
        for (const turn of transcript.turns) {
          const contextLabel = turnContextLabel(turn);
          const article = appendMessage(turn.role === 'assistant' ? 'assistant' : 'user', turn.content, turn.role === 'assistant' ? assistantName() : '', contextLabel);
          if (turn.role === 'assistant' && turn.metadata?.ui?.type === 'approval') {
            setApprovalCard(article, turn.metadata.ui.approval, turn.content, assistantName());
          }
        }
        if (!transcript.turns.length && announce) {
          appendMessage('system', `Restored empty chat session ${shortId(state.chatSessionId)}.`);
        } else if (announce) {
          appendMessage('system', `Restored chat session ${shortId(state.chatSessionId)}.`);
        }
      }
      function renderContext() {
        document.querySelector('.brand').firstChild.nodeValue = assistantName();
        el('context-line').textContent = `Context: ${currentContextLabel()}`;
      }
      function assistantName() {
        const name = state.health?.chat?.assistant_name || 'Librarian';
        return String(name).trim() || 'Librarian';
      }
      function renderOverview() {
        const health = state.health || {};
        const worker = health.worker || {};
        const chat = health.chat || {};
        const memory = health.memory || {};
        const secrets = health.secrets || {};
        el('overview').innerHTML = [
          card('Worker', `queued=${worker.queued_jobs ?? 0}<br>running=${worker.running_jobs ?? 0}<br>slots=${worker.available_slots ?? '__WORKER_CONCURRENCY__'}`),
          card('Chat', `name=${htmlEscape(chat.assistant_name || 'Librarian')}<br>timeout=${chat.codex_timeout_seconds ?? 180}s<br>memory hits=${chat.memory_hit_limit ?? 12}<br>max iterations=${chat.max_iterations ?? 6}`),
          `<form id="chat-settings-form" class="card stack">
            <h3>Chat Settings</h3>
            <div class="form-grid">
              <div><label for="chat-assistant-name">Name</label><input id="chat-assistant-name" value="${htmlEscape(chat.assistant_name || 'Librarian')}"></div>
              <div><label for="chat-timeout">Timeout, seconds</label><input id="chat-timeout" type="number" min="1" value="${chat.codex_timeout_seconds ?? 180}"></div>
              <div><label for="chat-memory-hit-limit">Memory hits</label><input id="chat-memory-hit-limit" type="number" min="1" value="${chat.memory_hit_limit ?? 12}"></div>
              <div><label for="chat-max-iterations">Max iterations</label><input id="chat-max-iterations" type="number" min="1" max="100" value="${chat.max_iterations ?? 6}"></div>
              <button type="submit">Save</button>
            </div>
          </form>`,
          card('Memory', `items=${memory.items ?? 0}<br>embedded=${memory.embedded_items ?? 0}<br>missing=${memory.missing_embeddings ?? 0}`),
          card('Knowledge base', `${htmlEscape(health.vault_path || 'Library')}<br><span class="muted">${htmlEscape(health.database_path || '.mdb/librarian.db')}</span>`),
          card('Secrets', `${htmlEscape(secrets.status || 'unknown')}<br><span class="muted">${htmlEscape(secrets.location || '')}</span>`)
        ].join('');
        const chatForm = el('chat-settings-form');
        if (chatForm) chatForm.addEventListener('submit', saveChatSettings);
      }
      async function saveChatSettings(event) {
        event.preventDefault();
        await postJson('/api/settings/chat', {
          assistant_name: el('chat-assistant-name').value,
          codex_timeout_seconds: Number(el('chat-timeout').value || 180),
          memory_hit_limit: Number(el('chat-memory-hit-limit').value || 12),
          max_iterations: Number(el('chat-max-iterations').value || 6)
        });
      }
      function renderChatSessions() {
        el('chat-sessions').innerHTML = state.chatSessions.length ? state.chatSessions.map(session => {
          const active = session.id === state.chatSessionId ? ' active' : '';
          return `<div class="card${active}">
            <h3>${htmlEscape(session.title || 'Chat')} <span class="muted tiny">${shortId(session.id)}</span></h3>
            <div class="muted tiny">turns=${session.turn_count ?? 0} updated=${htmlEscape(session.updated_at || '')}</div>
            <div class="row"><button type="button" data-restore-chat="${htmlEscape(session.id)}">Restore</button></div>
          </div>`;
        }).join('') : '<div class="card muted">No saved chat sessions yet.</div>';
        qsa('[data-restore-chat]').forEach(button => button.addEventListener('click', async () => {
          await restoreChatSession(button.dataset.restoreChat, true);
          closeOverlay('settings-overlay');
        }));
      }
      function renderProviders() {
        const states = new Map((state.providers.states || []).map(item => [`${item.provider}:${item.model || ''}`, item]));
        const models = state.providers.catalog || [];
        const runtime = state.providers.runtime || {};
        const cards = models.length ? models.map(model => {
          const current = states.get(`${model.provider}:${model.model}`) || states.get(`${model.provider}:`) || {};
          const providerRuntime = runtime[model.provider] || {};
          const runtimeLines = Object.keys(providerRuntime).length
            ? `<br><span class="muted tiny">host profile: ${htmlEscape(providerRuntime.host_home || '-')}</span><br><span class="muted tiny">mount: ${providerRuntime.mount_host_home ? 'enabled' : 'disabled'}${providerRuntime.host_home_exists === false ? ' · missing profile' : ''}</span>`
            : '';
          return card(htmlEscape(model.provider), `${htmlEscape(model.model || 'default')}<br><span class="muted">${htmlEscape(current.status || 'Not paused')}</span>${runtimeLines}`);
        }).join('') : '<div class="card muted">No providers reported.</div>';
        const commands = state.providers.commands || {};
        const codex = runtime['codex'] || {};
        const claude = runtime['claude-code'] || {};
        const providerTools = `<div class="card">
          <h3>Provider Setup</h3>
          <div class="muted tiny">Auth still opens in the host shell, but these commands match the current Librarian root.</div>
          <div class="row">
            <button type="button" class="secondary" data-provider-command="codex">Codex auth command</button>
            <button type="button" class="secondary" data-provider-command="claude">Claude auth command</button>
            <button type="button" class="secondary" data-provider-command="image">Build image</button>
            <button type="button" class="secondary" data-provider-command="smoke-codex">Codex smoke</button>
            <button type="button" class="secondary" data-provider-command="smoke-claude">Claude smoke</button>
          </div>
          <div class="muted tiny">Claude instruction file: ${htmlEscape(claude.instruction_file || 'CLAUDE.md')}</div>
        </div>`;
        const codexForm = `<form id="codex-runtime-form" class="card stack">
          <h3>Codex Runtime</h3>
          <div class="form-grid">
            <div class="wide"><label for="codex-host-home">Host profile path</label><input id="codex-host-home" value="${htmlEscape(codex.host_home || '')}" placeholder="/home/user/Librarian/.cfg/codex-home"></div>
            <button type="submit">Save</button>
          </div>
          <div class="row">
            <label><input id="codex-mount-home" type="checkbox" ${codex.mount_host_home ? 'checked' : ''}> mount profile</label>
            <label><input id="codex-mount-readonly" type="checkbox" ${codex.mount_read_only ? 'checked' : ''}> read-only</label>
          </div>
        </form>`;
        const claudeForm = `<form id="claude-runtime-form" class="card stack">
          <h3>Claude Runtime</h3>
          <div class="form-grid">
            <div class="wide"><label for="claude-host-home">Host profile path</label><input id="claude-host-home" value="${htmlEscape(claude.host_home || '')}" placeholder="/home/user/.claude"></div>
            <div><label for="claude-instruction-file">Instruction file</label><input id="claude-instruction-file" value="${htmlEscape(claude.instruction_file || 'CLAUDE.md')}"></div>
            <button type="submit">Save</button>
          </div>
          <div class="row">
            <label><input id="claude-mount-home" type="checkbox" ${claude.mount_host_home ? 'checked' : ''}> mount profile</label>
            <label><input id="claude-mount-readonly" type="checkbox" ${claude.mount_read_only ? 'checked' : ''}> read-only</label>
          </div>
        </form>`;
        el('providers').innerHTML = providerTools + codexForm + claudeForm + cards;
        const codexRuntimeForm = el('codex-runtime-form');
        if (codexRuntimeForm) codexRuntimeForm.addEventListener('submit', saveCodexRuntime);
        const form = el('claude-runtime-form');
        if (form) form.addEventListener('submit', saveClaudeRuntime);
        qsa('[data-provider-command]').forEach(button => button.addEventListener('click', () => {
          const key = button.dataset.providerCommand;
          const command = {
            codex: commands.codex_auth,
            claude: commands.claude_auth,
            image: commands.build_agent_image,
            'smoke-codex': commands.smoke_codex,
            'smoke-claude': commands.smoke_claude
          }[key] || 'Command is not available yet.';
          appendMessage('system', command, 'Provider command');
        }));
      }
      async function saveCodexRuntime(event) {
        event.preventDefault();
        await postJson('/api/settings/codex', {
          host_home: el('codex-host-home').value || null,
          mount_host_home: el('codex-mount-home').checked,
          mount_read_only: el('codex-mount-readonly').checked
        });
      }
      async function saveClaudeRuntime(event) {
        event.preventDefault();
        await postJson('/api/settings/claude', {
          host_home: el('claude-host-home').value || null,
          mount_host_home: el('claude-mount-home').checked,
          mount_read_only: el('claude-mount-readonly').checked,
          instruction_file: el('claude-instruction-file').value || 'CLAUDE.md'
        });
      }
      function renderJobs() {
        el('jobs').innerHTML = state.jobs.length ? state.jobs.slice(0, 12).map(job => {
          return `<div class="card">
            <h3>${htmlEscape(job.status)} <span class="muted tiny">${htmlEscape(job.provider)} ${shortId(job.id)}</span></h3>
            <div>${htmlEscape(job.goal)}</div>
            <div class="muted tiny">${htmlEscape(job.created_at || '')}</div>
            <div class="row">
              <button type="button" class="secondary" data-job-events="${htmlEscape(job.id)}">Events</button>
              <button type="button" data-job-preflight="${htmlEscape(job.id)}">Preflight</button>
              <button type="button" class="secondary" data-job-retry="${htmlEscape(job.id)}">Retry</button>
              <button type="button" class="danger" data-job-cancel="${htmlEscape(job.id)}">Cancel</button>
            </div>
            <div id="job-events-${htmlEscape(job.id)}" class="stack"></div>
          </div>`;
        }).join('') : '<div class="card muted">No jobs yet.</div>';
        qsa('[data-job-events]').forEach(button => button.addEventListener('click', () => showJobEvents(button.dataset.jobEvents)));
        qsa('[data-job-preflight]').forEach(button => button.addEventListener('click', () => runJobAction(button.dataset.jobPreflight, 'preflight')));
        qsa('[data-job-retry]').forEach(button => button.addEventListener('click', () => runJobAction(button.dataset.jobRetry, 'retry')));
        qsa('[data-job-cancel]').forEach(button => button.addEventListener('click', () => runJobAction(button.dataset.jobCancel, 'cancel')));
      }
      async function showJobEvents(id) {
        const target = el(`job-events-${id}`);
        if (!target) return;
        target.innerHTML = '<div class="muted tiny">Loading events...</div>';
        const events = await loadJson(`/api/jobs/${encodeURIComponent(id)}/events`, []);
        target.innerHTML = Array.isArray(events) && events.length ? events.slice(-8).reverse().map(event => {
          const payload = event.payload || {};
          const summary = event.kind === 'stdout' || event.kind === 'stderr'
            ? payload.line || ''
            : event.kind === 'failure_category'
              ? `${payload.category?.code || 'failure'}: ${payload.category?.message || ''}`
              : JSON.stringify(payload);
          return `<div class="card action"><b>${htmlEscape(event.kind)}</b> <span class="muted tiny">${htmlEscape(event.created_at || '')}</span><br>${htmlEscape(summary)}</div>`;
        }).join('') : '<div class="muted tiny">No events for this job.</div>';
      }
      async function runJobAction(id, action) {
        const response = await fetch(`/api/jobs/${encodeURIComponent(id)}/${action}`, { method: 'POST' });
        const data = await response.json().catch(() => ({}));
        if (!response.ok) {
          appendMessage('system', data.error || `${action} failed: ${response.status}`);
          return;
        }
        appendMessage('system', `${action} queued for job ${shortId(id)}.`);
        await refresh();
        if (action === 'preflight') await showJobEvents(id);
      }
      function renderPromptBuilder() {
        const blocks = state.promptBlocks;
        const targets = Array.from(new Set(['librarian', 'agents', 'AGENTS.md', 'CLAUDE.md', ...blocks.map(block => block.target)])).sort();
        const targetOptions = targets.map(target => `<option value="${htmlEscape(target)}">${htmlEscape(target)}</option>`).join('');
        const form = `<form id="prompt-block-form" class="card stack">
          <h3>Add block</h3>
          <div class="form-grid">
            <div><label for="prompt-target">Target</label><input id="prompt-target" list="prompt-targets" value="librarian"><datalist id="prompt-targets">${targetOptions}</datalist></div>
            <div><label for="prompt-name">Name</label><input id="prompt-name" required placeholder="identity"></div>
            <label><input id="prompt-markdown" type="checkbox" checked> markdown</label>
            <div class="wide"><label for="prompt-content">Content</label><textarea id="prompt-content" required rows="6" placeholder="You are Librarian..."></textarea></div>
            <button type="submit">Add</button>
          </div>
        </form>`;
        const byTarget = targets
          .map(target => [target, blocks.filter(block => block.target === target).sort((a, b) => a.position - b.position)])
          .filter(([, items]) => items.length);
        const list = byTarget.length ? byTarget.map(([target, items]) => `<section class="card stack">
          <h3>${htmlEscape(target)} <span class="muted tiny">${items.filter(item => item.enabled).length}/${items.length} enabled</span></h3>
          <div class="row">
            <button type="button" class="secondary" data-render-prompt="${htmlEscape(target)}">Preview target</button>
            <button type="button" class="secondary" data-export-prompt="${htmlEscape(target)}">Export proposal</button>
          </div>
          ${items.map(block => `<div class="prompt-block ${block.enabled ? '' : 'muted'}">
            <div class="form-grid">
              <div><label>Name</label><input id="prompt-name-${htmlEscape(block.id)}" value="${htmlEscape(block.name)}"></div>
              <div><label>Position</label><input id="prompt-position-${htmlEscape(block.id)}" type="number" value="${block.position}"></div>
              <label><input id="prompt-markdown-${htmlEscape(block.id)}" type="checkbox" ${block.markdown ? 'checked' : ''}> markdown</label>
              <div class="wide"><label>Content</label><textarea id="prompt-content-${htmlEscape(block.id)}" rows="7">${htmlEscape(block.content)}</textarea></div>
            </div>
            <div class="row">
              <button type="button" data-save-prompt="${htmlEscape(block.id)}">Save</button>
              <button type="button" data-toggle-prompt="${htmlEscape(block.id)}" data-enabled="${block.enabled ? 'false' : 'true'}">${block.enabled ? 'Disable' : 'Enable'}</button>
              <button type="button" class="secondary" data-move-prompt="${htmlEscape(block.id)}" data-position="${block.position - 1}">Up</button>
              <button type="button" class="secondary" data-move-prompt="${htmlEscape(block.id)}" data-position="${block.position + 1}">Down</button>
              <button type="button" class="danger" data-delete-prompt="${htmlEscape(block.id)}">Delete</button>
            </div>
          </div>`).join('')}
        </section>`).join('') : '<div class="card muted">No prompt blocks yet.</div>';
        el('prompt-builder').innerHTML = `${form}<div id="prompt-preview" class="card muted">Choose preview target from any block.</div>${list}`;
        el('prompt-block-form').addEventListener('submit', createPromptBlockFromUi);
        qsa('[data-toggle-prompt]').forEach(button => button.addEventListener('click', async () => {
          await fetch(`/api/prompt-blocks/${button.dataset.togglePrompt}/${button.dataset.enabled === 'true' ? 'enable' : 'disable'}`, { method: 'POST' });
          await refresh();
        }));
        qsa('[data-render-prompt]').forEach(button => button.addEventListener('click', () => renderPromptPreview(button.dataset.renderPrompt)));
        qsa('[data-save-prompt]').forEach(button => button.addEventListener('click', () => savePromptBlock(button.dataset.savePrompt)));
        qsa('[data-move-prompt]').forEach(button => button.addEventListener('click', () => updatePromptBlock(button.dataset.movePrompt, { position: Number(button.dataset.position) })));
        qsa('[data-delete-prompt]').forEach(button => button.addEventListener('click', () => deletePromptBlock(button.dataset.deletePrompt)));
        qsa('[data-export-prompt]').forEach(button => button.addEventListener('click', () => proposePromptExport(button.dataset.exportPrompt)));
      }
      async function createPromptBlockFromUi(event) {
        event.preventDefault();
        await postJson('/api/prompt-blocks', {
          target: el('prompt-target').value,
          name: el('prompt-name').value,
          content: el('prompt-content').value,
          markdown: el('prompt-markdown').checked
        });
      }
      async function renderPromptPreview(target) {
        const data = await loadJson(`/api/prompt-blocks/render?target=${encodeURIComponent(target)}`, null);
        el('prompt-preview').innerHTML = data ? `<h3>${htmlEscape(target)}</h3><pre>${htmlEscape(data.rendered || '')}</pre>` : 'Could not render prompt.';
      }
      async function savePromptBlock(id) {
        await updatePromptBlock(id, {
          name: el(`prompt-name-${id}`).value,
          content: el(`prompt-content-${id}`).value,
          position: Number(el(`prompt-position-${id}`).value || 0),
          markdown: el(`prompt-markdown-${id}`).checked
        });
      }
      async function updatePromptBlock(id, body) {
        const response = await fetch(`/api/prompt-blocks/${id}`, { method: 'PATCH', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
        if (!response.ok) appendMessage('system', `Prompt update failed: ${response.status}`);
        await refresh();
      }
      async function deletePromptBlock(id) {
        if (!confirm('Delete this prompt block?')) return;
        const response = await fetch(`/api/prompt-blocks/${id}`, { method: 'DELETE' });
        if (!response.ok) appendMessage('system', `Prompt delete failed: ${response.status}`);
        await refresh();
      }
      async function proposePromptExport(target) {
        const path = prompt('Library Markdown export path', `prompts/${target}.md`);
        if (!path) return;
        await postJson('/api/prompt-blocks/export-proposal', { target, path });
        appendMessage('system', `Created export approval proposal for ${target}.`);
      }
      function renderSystemEvents(events) {
        el('system-events').innerHTML = Array.isArray(events) && events.length ? events.slice(0, 20).map(event => {
          return `<div class="card"><b>${htmlEscape(event.kind)}</b><br><span class="muted tiny">${htmlEscape(event.created_at || '')}</span></div>`;
        }).join('') : '<div class="card muted">No system events.</div>';
      }
      function renderProjects() {
        const createForm = `<form id="project-create-form" class="card stack">
          <h3>Create project</h3>
          <div class="form-grid">
            <div><label for="project-name">Name</label><input id="project-name" required placeholder="My Project"></div>
            <div><label for="project-library-path">Knowledge path</label><input id="project-library-path" placeholder="projects/my-project"></div>
            <div class="wide"><label for="project-workspace-path">Existing workspace path</label><input id="project-workspace-path" placeholder="optional external directory"></div>
            <button type="submit">Create</button>
          </div>
        </form>`;
        const agentForm = state.projects.length ? `<form id="agent-launch-form" class="card stack">
          <h3>Launch agent</h3>
          <div class="form-grid">
            <div><label for="agent-project">Project</label><select id="agent-project">${state.projects.map(project => `<option value="${htmlEscape(project.name)}" ${project.name === state.activeProject ? 'selected' : ''}>${htmlEscape(project.name)}</option>`).join('')}</select></div>
            <div><label for="agent-provider">Provider</label><select id="agent-provider"><option value="codex">codex</option><option value="openrouter">openrouter</option><option value="claude-code">claude-code</option></select></div>
            <div class="wide"><label for="agent-goal">Goal</label><input id="agent-goal" required placeholder="Inspect the project and summarize next steps"></div>
            <button type="submit">Queue</button>
          </div>
          <div class="row"><label><input id="agent-read-only" type="checkbox" checked> read-only</label><label><input id="agent-network" type="checkbox"> network</label></div>
        </form>` : '';
        if (!state.projects.length) {
          el('project-stage').innerHTML = `<div class="project-layout"><div class="stack">${createForm}<div class="card muted">No projects yet. Create one here or use <code>/project create</code> in chat.</div></div><div class="project-map">${renderProjectMapSurface()}</div></div>`;
          wireProjectForms();
          return;
        }
        const cards = state.projects.map(project => {
          const active = project.name === state.activeProject ? ' active' : '';
          return `<div class="card${active}">
            <h3>${htmlEscape(project.name)}</h3>
            <div class="muted tiny">Knowledge: ${htmlEscape(project.library_path || '-')}</div>
            <div class="muted tiny">Workspace: ${htmlEscape(project.path)}</div>
            <div class="row">
              <button type="button" data-project="${htmlEscape(project.name)}">Use</button>
              <button class="secondary" type="button" data-attach-library="${htmlEscape(project.id)}">Knowledge</button>
              <button class="secondary" type="button" data-attach-workspace="${htmlEscape(project.id)}">Workspace</button>
            </div>
          </div>`;
        }).join('');
        el('project-stage').innerHTML = `<div class="project-layout"><div class="stack">${createForm}${agentForm}${cards}</div><div class="project-map">${renderProjectMapSurface()}</div></div>`;
        wireProjectForms();
        qsa('[data-project]').forEach(button => button.addEventListener('click', () => {
          state.activeProject = button.dataset.project || '';
          const project = state.projects.find(project => project.name === state.activeProject);
          state.activeContext = project ? [project] : [];
          state.chatSessionId = null;
          renderProjects();
          renderContext();
          closeOverlay('projects-overlay');
        }));
      }
      function renderProjectMapTree(node) {
        if (!node) return '<div class="node root"><h3>Librarian</h3><div class="muted tiny">Knowledge base is empty.</div></div>';
        const projects = Array.isArray(node.projects) && node.projects.length
          ? `<div class="muted tiny">${node.projects.map(project => htmlEscape(project.name)).join(', ')}</div>`
          : '';
        const children = Array.isArray(node.children) && node.children.length
          ? `<div class="node-column">${node.children.map(renderProjectMapTree).join('')}</div>`
          : '';
        return `<div class="tree"><div class="node ${htmlEscape(node.visual_kind || '')}"><span class="badge">${htmlEscape(node.visual_kind || 'node')}</span><h3>${htmlEscape(node.name || 'Knowledge base')}</h3><div class="muted tiny">${htmlEscape(node.path || '.')}</div>${projects}</div>${children}</div>`;
      }
      function renderProjectMapSurface() {
        const count = state.projectMap?.linked_project_count ?? 0;
        const detached = Array.isArray(state.projectMap?.detached_projects) ? state.projectMap.detached_projects.length : 0;
        return `<div class="map-legend">
          <span class="legend-chip">Books: Markdown notes</span>
          <span class="legend-chip">Shelves: folders with files</span>
          <span class="legend-chip">Racks: nested folders</span>
          <span class="legend-chip">${count} linked · ${detached} detached</span>
        </div>${renderProjectMapTree(state.projectMap?.root)}`;
      }
      function wireProjectForms() {
        const form = el('project-create-form');
        if (form) form.addEventListener('submit', createProjectFromUi);
        const agentForm = el('agent-launch-form');
        if (agentForm) agentForm.addEventListener('submit', launchAgentFromUi);
        qsa('[data-attach-library]').forEach(button => button.addEventListener('click', async () => {
          const value = prompt('Knowledge base path inside Librarian/Library');
          if (!value) return;
          await postJson(`/api/projects/${button.dataset.attachLibrary}/attach-library`, { library_path: value });
        }));
        qsa('[data-attach-workspace]').forEach(button => button.addEventListener('click', async () => {
          const value = prompt('Existing workspace directory path');
          if (!value) return;
          await postJson(`/api/projects/${button.dataset.attachWorkspace}/attach-workspace`, { workspace_path: value });
        }));
      }
      async function createProjectFromUi(event) {
        event.preventDefault();
        await postJson('/api/projects', {
          name: el('project-name').value,
          library_path: el('project-library-path').value || null,
          workspace_path: el('project-workspace-path').value || null
        });
      }
      async function launchAgentFromUi(event) {
        event.preventDefault();
        const body = {
          project: el('agent-project').value,
          provider: el('agent-provider').value,
          goal: el('agent-goal').value,
          read_only: el('agent-read-only').checked,
          allow_network: el('agent-network').checked
        };
        const response = await fetch('/api/jobs', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
        const data = await response.json();
        if (!response.ok) {
          appendMessage('system', data.error || `Agent launch failed: ${response.status}`);
          return;
        }
        appendMessage('system', `Queued agent job ${shortId(data.id)} for ${body.project}.`);
        await refresh();
      }
      async function postJson(url, body) {
        const response = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
        const data = await response.json();
        if (!response.ok) {
          appendMessage('system', data.error || `Request failed: ${response.status}`);
          return;
        }
        await refresh();
      }
      function card(title, body) {
        return `<div class="card"><h3>${htmlEscape(title)}</h3><div>${body}</div></div>`;
      }
      async function submitChat(event) {
        event.preventDefault();
        const input = el('goal-input');
        const goal = input.value.trim();
        if (!goal) return;
        rememberInput(goal);
        closeSlashPalette();
        const contextProjects = currentContextProjects();
        const contextLabel = contextLabelFromProjects(contextProjects);
        appendMessage('user', goal, '', contextLabel);
        input.value = '';
        input.disabled = true;
        const startedAt = performance.now();
        const pending = appendThinkingMessage();
        const project = activeProjectName();
        try {
          const response = await fetch('/api/chat', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              message: goal,
              project: project || null,
              project_context: contextProjects.map(project => project.name || project.library_path).filter(Boolean),
              session_id: state.chatSessionId
            })
          });
          const data = await response.json();
          if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
          if (data.session_id) state.chatSessionId = data.session_id;
          if (data.context?.nodes?.length) {
            state.activeContext = data.context.nodes.map(contextNodeFromMetadata);
            state.activeProject = state.activeContext[0]?.name || '';
            renderContext();
          }
          const elapsed = Math.max(1, Math.round(performance.now() - startedAt));
          const memoryCount = Array.isArray(data.memory_hits) ? data.memory_hits.length : 0;
          const iterationCount = data.iterations ?? 1;
          const detail = data.mode === 'slash-command'
            ? 'Command result'
            : `${assistantName()} - ${(elapsed / 1000).toFixed(1)}s - ${iterationCount} it - ${memoryCount} memory`;
          if (data.ui?.type === 'context_switch') {
            setContextSwitchCard(pending, data.ui, detail);
          } else if (data.ui?.type === 'context_update') {
            const nodes = Array.isArray(data.ui.context?.nodes) ? data.ui.context.nodes.map(contextNodeFromMetadata) : [];
            state.activeContext = nodes;
            state.activeProject = nodes[0]?.name || '';
            renderContext();
            if (data.mode === 'slash-command') pending.className = 'message system command';
            setMessage(pending, data.reply || 'Context updated.', detail, data.ui.context?.label || contextLabelFromProjects(nodes));
          } else if (data.ui?.type === 'approval') {
            setApprovalCard(pending, data.ui.approval, data.reply || 'Approval requested.', detail);
          } else {
            if (data.mode === 'slash-command') pending.className = 'message system command';
            setMessage(pending, data.reply || 'I am here.', detail, data.context_label || contextLabel);
          }
          await refresh();
        } catch (error) {
          setMessage(pending, `Could not answer: ${error.message || error}`, 'System');
        } finally {
          input.disabled = false;
          input.focus();
        }
      }

      el('settings-open').addEventListener('click', () => openOverlay('settings-overlay'));
      el('projects-open').addEventListener('click', () => openOverlay('projects-overlay'));
      el('new-chat').addEventListener('click', () => {
        state.chatSessionId = null;
        el('thread').innerHTML = '';
        appendMessage('assistant', 'New chat started.', assistantName());
        el('goal-input').focus();
      });
      qsa('[data-close]').forEach(button => button.addEventListener('click', () => closeOverlay(button.dataset.close)));
      qsa('.tab-button').forEach(button => button.addEventListener('click', () => setTab(button.dataset.tab)));
      el('chat-form').addEventListener('submit', submitChat);
      el('goal-input').addEventListener('keydown', event => {
        if (handleSlashKeys(event)) return;
        if (handleHistoryKeys(event)) return;
        if (event.key === 'Enter' && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey) {
          event.preventDefault();
          el('chat-form').requestSubmit();
        }
      });
      el('goal-input').addEventListener('input', () => {
        state.historyIndex = null;
        updateSlashPalette();
      });
      el('goal-input').addEventListener('blur', () => {
        window.setTimeout(closeSlashPalette, 120);
      });
      function rememberInput(value) {
        if (!value) return;
        if (state.inputHistory[state.inputHistory.length - 1] !== value) {
          state.inputHistory.push(value);
          if (state.inputHistory.length > 100) state.inputHistory.shift();
        }
        state.historyIndex = null;
        state.draftInput = '';
      }
      function handleHistoryKeys(event) {
        const input = el('goal-input');
        if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return false;
        if (state.slashOpen) return false;
        const atStart = input.selectionStart === 0 && input.selectionEnd === 0;
        const atEnd = input.selectionStart === input.value.length && input.selectionEnd === input.value.length;
        const empty = input.value.length === 0;
        if (event.key === 'ArrowUp' && !(empty || atStart)) return false;
        if (event.key === 'ArrowDown' && !(empty || atEnd || state.historyIndex !== null)) return false;
        if (!state.inputHistory.length) return false;
        event.preventDefault();
        if (state.historyIndex === null) {
          state.draftInput = input.value;
          state.historyIndex = state.inputHistory.length;
        }
        if (event.key === 'ArrowUp') {
          state.historyIndex = Math.max(0, state.historyIndex - 1);
          input.value = state.inputHistory[state.historyIndex] || '';
        } else {
          state.historyIndex = Math.min(state.inputHistory.length, state.historyIndex + 1);
          input.value = state.historyIndex === state.inputHistory.length ? state.draftInput : state.inputHistory[state.historyIndex];
        }
        window.requestAnimationFrame(() => input.setSelectionRange(input.value.length, input.value.length));
        return true;
      }
      function matchingSlashCommands() {
        const value = el('goal-input').value;
        if (!value.startsWith('/')) return [];
        const query = value.toLowerCase();
        const commands = state.slashCommands.length ? state.slashCommands : fallbackSlashCommands.map(([command, description]) => ({ command, description }));
        return commands
          .map(item => [item.command, item.description || ''])
          .filter(([command]) => command.toLowerCase().startsWith(query) || command.toLowerCase().includes(query))
          .slice(0, 8);
      }
      function updateSlashPalette() {
        const palette = el('slash-palette');
        const matches = matchingSlashCommands();
        state.slashOpen = matches.length > 0;
        state.slashIndex = Math.min(state.slashIndex, Math.max(0, matches.length - 1));
        palette.classList.toggle('open', state.slashOpen);
        palette.innerHTML = matches.map(([command, description], index) => `
          <div class="slash-option ${index === state.slashIndex ? 'active' : ''}" role="option" data-slash-index="${index}">
            <code>${htmlEscape(command)}</code><span>${htmlEscape(description)}</span>
          </div>
        `).join('');
        Array.from(palette.querySelectorAll('[data-slash-index]')).forEach(option => {
          option.addEventListener('mousedown', event => {
            event.preventDefault();
            applySlashCommand(matches[Number(option.dataset.slashIndex)]?.[0]);
          });
        });
      }
      function closeSlashPalette() {
        state.slashOpen = false;
        el('slash-palette').classList.remove('open');
      }
      function applySlashCommand(command) {
        if (!command) return;
        const input = el('goal-input');
        input.value = command;
        input.focus();
        input.setSelectionRange(input.value.length, input.value.length);
        closeSlashPalette();
      }
      function handleSlashKeys(event) {
        updateSlashPalette();
        if (!state.slashOpen) return false;
        const matches = matchingSlashCommands();
        if (!matches.length) return false;
        if (event.key === 'ArrowDown') {
          event.preventDefault();
          state.slashIndex = (state.slashIndex + 1) % matches.length;
          updateSlashPalette();
          return true;
        }
        if (event.key === 'ArrowUp') {
          event.preventDefault();
          state.slashIndex = (state.slashIndex + matches.length - 1) % matches.length;
          updateSlashPalette();
          return true;
        }
        if (event.key === 'Tab') {
          event.preventDefault();
          applySlashCommand(matches[state.slashIndex]?.[0]);
          return true;
        }
        if (event.key === 'Escape') {
          event.preventDefault();
          closeSlashPalette();
          return true;
        }
        return false;
      }
      async function loadSlashCommands() {
        const commands = await loadJson('/api/slash-commands', []);
        if (Array.isArray(commands)) state.slashCommands = commands;
      }
      refresh()
        .then(loadSlashCommands)
        .then(restoreLatestChatSession)
        .catch(error => appendMessage('system', `Admin data failed to load: ${error.message || error}`));
    })();
  </script>
</body>
</html>"##;
    html.replace("__BIND__", bind)
        .replace("__WORKER_CONCURRENCY__", &worker_concurrency.to_string())
}

#[allow(dead_code)]
fn app_html(bind: &str, worker_concurrency: usize) -> String {
    let html = r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Librarian</title>
  <style>
    :root {
      color-scheme: dark;
      --bg: #0f1214;
      --panel: #171b1f;
      --panel-2: #1f262c;
      --text: #edf1f5;
      --muted: #9aa8b6;
      --line: #303841;
      --accent: #58c4a5;
      --accent-2: #8bb8ff;
      --danger: #b96666;
      --rail: 72px;
      --sidebar: clamp(260px, 27vw, 360px);
    }
    * { box-sizing: border-box; }
    html, body {
      width: 100%;
      height: 100%;
      min-width: 960px;
      min-height: 620px;
      overflow: hidden;
    }
    body {
      margin: 0;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: var(--bg);
      color: var(--text);
    }
    h1, h2, h3, p { margin: 0; }
    h1 { font-size: 18px; letter-spacing: 0; }
    h2 {
      margin: 18px 0 8px;
      color: var(--muted);
      font-size: 13px;
      text-transform: uppercase;
      letter-spacing: 0;
    }
    h3 { font-size: 15px; }
    label {
      display: block;
      margin: 0 0 6px;
      color: var(--muted);
      font-size: 12px;
    }
    input, textarea, select, button { font: inherit; }
    input, textarea, select {
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #11161a;
      color: var(--text);
    }
    input, select { height: 38px; padding: 0 10px; }
    textarea {
      min-height: 112px;
      padding: 10px;
      resize: none;
      line-height: 1.45;
    }
    button {
      min-height: 38px;
      border: 1px solid transparent;
      border-radius: 6px;
      padding: 0 12px;
      cursor: pointer;
      background: var(--accent);
      color: #06100d;
      font-weight: 700;
    }
    button.secondary {
      background: var(--panel-2);
      color: var(--text);
      border-color: var(--line);
    }
    button.danger { background: var(--danger); color: #fff; }
    .app {
      height: 100dvh;
      min-height: 620px;
      display: grid;
      grid-template-columns: var(--rail) var(--sidebar) minmax(0, 1fr);
      overflow: hidden;
    }
    .rail {
      border-right: 1px solid var(--line);
      background: #12161a;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: space-between;
      padding: 14px 10px;
    }
    .rail-group { display: flex; flex-direction: column; gap: 10px; align-items: center; }
    .icon-btn {
      width: 44px;
      height: 44px;
      min-height: 44px;
      display: grid;
      place-items: center;
      padding: 0;
      background: transparent;
      border-color: transparent;
      color: var(--muted);
      font-size: 23px;
    }
    .icon-btn:hover, .icon-btn.active {
      color: var(--text);
      background: var(--panel-2);
      border-color: var(--line);
    }
    .sidebar {
      min-width: 0;
      border-right: 1px solid var(--line);
      background: #14181c;
      display: grid;
      grid-template-rows: auto 1fr;
      overflow: hidden;
    }
    .sidebar-head {
      padding: 18px 18px 12px;
      border-bottom: 1px solid var(--line);
    }
    .sidebar-scroll { overflow: auto; padding: 12px 14px 18px; }
    .chat-shell {
      min-width: 0;
      min-height: 0;
      display: grid;
      grid-template-rows: auto minmax(0, 1fr) auto;
      overflow: hidden;
    }
    .chat-top {
      height: 64px;
      padding: 0 22px;
      border-bottom: 1px solid var(--line);
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
    }
    .chat-log {
      min-height: 0;
      overflow: auto;
      padding: 22px;
      display: flex;
      flex-direction: column;
      gap: 12px;
    }
    .composer {
      border-top: 1px solid var(--line);
      background: #12161a;
      padding: 14px 18px 18px;
      display: grid;
      grid-template-columns: minmax(180px, 260px) minmax(150px, 190px) minmax(0, 1fr) auto;
      gap: 10px;
      align-items: end;
    }
    .composer textarea { min-height: 74px; max-height: 160px; }
    .composer button[type="submit"] { min-width: 128px; height: 74px; }
    .advanced-row { grid-column: 1 / -1; }
    .advanced-grid {
      margin-top: 10px;
      display: grid;
      grid-template-columns: 260px auto;
      gap: 12px;
      align-items: end;
    }
    .card, .item {
      border: 1px solid var(--line);
      border-radius: 6px;
      background: var(--panel);
      padding: 10px;
      margin-bottom: 8px;
    }
    .message {
      max-width: 920px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
      padding: 14px;
      white-space: pre-wrap;
      line-height: 1.5;
    }
    .message.system { border-left: 3px solid var(--accent); }
    .message.job { border-left: 3px solid var(--accent-2); }
    .muted { color: var(--muted); }
    .tiny { font-size: 12px; }
    .row { display: flex; gap: 8px; align-items: center; margin: 10px 0; }
    .row input[type="checkbox"] { width: 18px; height: 18px; }
    .grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
    .status-grid { display: grid; grid-template-columns: 1fr; gap: 8px; }
    .pill {
      display: inline-block;
      border: 1px solid var(--line);
      border-radius: 999px;
      padding: 2px 8px;
      margin: 2px 4px 2px 0;
      color: var(--muted);
      font-size: 12px;
    }
    .actions { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 6px; margin-top: 8px; }
    .actions button { min-height: 32px; font-size: 12px; }
    .mini { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px; margin-top: 8px; }
    .action {
      border-left: 3px solid var(--accent);
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
      line-height: 1.45;
    }
    details { margin-top: 8px; }
    summary { cursor: pointer; color: var(--accent); }
    pre { overflow: auto; white-space: pre-wrap; margin: 8px 0 0; color: var(--muted); }
    .overlay {
      position: fixed;
      inset: 0;
      z-index: 20;
      display: none;
      padding: 18px;
      background: rgba(8, 10, 12, 0.82);
    }
    .overlay.open { display: grid; }
    .overlay-panel {
      min-width: 0;
      min-height: 0;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #12161a;
      display: grid;
      grid-template-rows: auto minmax(0, 1fr);
      overflow: hidden;
    }
    .overlay-head {
      height: 64px;
      padding: 0 18px;
      border-bottom: 1px solid var(--line);
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
    }
    .overlay-body {
      min-height: 0;
      display: grid;
      grid-template-columns: 220px minmax(0, 1fr);
      overflow: hidden;
    }
    .tabs { border-right: 1px solid var(--line); padding: 12px; overflow: auto; }
    .tab-btn {
      width: 100%;
      background: transparent;
      color: var(--muted);
      border-color: transparent;
      text-align: left;
      padding: 0 10px;
      margin-bottom: 4px;
    }
    .tab-btn.active { color: var(--text); background: var(--panel-2); border-color: var(--line); }
    .tab-content { min-height: 0; overflow: auto; padding: 18px; }
    .tab-pane { display: none; max-width: 1080px; }
    .tab-pane.active { display: block; }
    .map-body { min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) 320px; overflow: hidden; }
    .map-canvas {
      position: relative;
      overflow: hidden;
      background:
        linear-gradient(rgba(255,255,255,0.025) 1px, transparent 1px),
        linear-gradient(90deg, rgba(255,255,255,0.025) 1px, transparent 1px);
      background-size: 32px 32px;
    }
    .tree-svg { position: absolute; inset: 0; width: 100%; height: 100%; }
    .node {
      position: absolute;
      width: 210px;
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 12px;
      background: #182027;
      box-shadow: 0 10px 28px rgba(0,0,0,0.24);
    }
    .node.root { border-color: var(--accent); background: #15231f; }
    .map-side { border-left: 1px solid var(--line); padding: 16px; overflow: auto; background: #14181c; }
    @media (max-width: 1100px) {
      html, body { min-width: 860px; }
      .app { grid-template-columns: 64px 280px minmax(0, 1fr); }
      .composer { grid-template-columns: minmax(160px, 220px) minmax(0, 1fr) auto; }
      .composer .provider-field { display: none; }
    }
  </style>
</head>
<body>
  <div class="app">
    <nav class="rail" aria-label="Primary">
      <div class="rail-group">
        <button class="icon-btn active" title="Chat" aria-label="Chat">L</button>
        <button class="icon-btn" title="Settings" aria-label="Settings" onclick="openOverlay('settings-overlay')">&#9881;</button>
      </div>
      <div class="rail-group">
        <button class="icon-btn" title="Project map" aria-label="Project map" onclick="openProjectMap()">&#9711;</button>
      </div>
    </nav>

    <aside class="sidebar">
      <div class="sidebar-head">
        <h1>Librarian</h1>
        <div class="muted tiny">localhost __BIND__</div>
      </div>
      <div class="sidebar-scroll">
        <h2>Status</h2>
        <div class="status-grid">
          <div id="worker" class="card">Loading...</div>
          <div id="memory" class="card">Loading...</div>
          <div id="providers-summary" class="card">Loading...</div>
        </div>
        <h2>Projects</h2>
        <div id="projects-summary" class="muted">Loading...</div>
        <h2>Recent Jobs</h2>
        <div id="jobs-summary" class="muted">Loading...</div>
      </div>
    </aside>

    <main class="chat-shell">
      <header class="chat-top">
        <div>
          <h1>Chat</h1>
          <div class="muted tiny">Ask Librarian to organize work, queue agent runs, or inspect project context.</div>
        </div>
        <div class="row">
          <button type="button" class="secondary" onclick="openProjectMap()">Project map</button>
          <button type="button" class="secondary" onclick="openOverlay('settings-overlay')">Settings</button>
        </div>
      </header>
      <div class="chat-log" id="output">
        <div class="message system">Ready. Choose a project, describe the outcome you want, and send it to an agent. Technical routing and tokens live under Advanced.</div>
      </div>
      <form id="chat" class="composer">
        <div>
          <label for="project">Project</label>
          <select id="project" name="project"></select>
        </div>
        <div class="provider-field">
          <label for="provider">Agent</label>
          <select id="provider" name="provider">
            <option value="codex">Codex</option>
            <option value="openrouter">OpenRouter</option>
            <option value="claude-code">Claude Code</option>
          </select>
        </div>
        <div>
          <label for="goal">What should happen?</label>
          <textarea id="goal" name="goal" placeholder="Example: inspect this project and suggest the next implementation step"></textarea>
        </div>
        <button type="submit">Send</button>
        <div class="advanced-row">
          <details>
            <summary>Advanced run options</summary>
            <div class="advanced-grid">
              <div>
                <label for="secret_grant_token">Secret access grant</label>
                <input id="secret_grant_token" name="secret_grant_token" autocomplete="off" placeholder="optional">
              </div>
              <div class="row">
                <input id="allow_network" name="allow_network" type="checkbox">
                <label for="allow_network">Allow network for this session</label>
              </div>
            </div>
          </details>
        </div>
      </form>
    </main>
  </div>

  <div class="overlay" id="settings-overlay" role="dialog" aria-modal="true" aria-label="Settings">
    <div class="overlay-panel">
      <div class="overlay-head">
        <div>
          <h1>Settings</h1>
          <div class="muted tiny">Operational controls, providers, schedules, secrets, and diagnostics.</div>
        </div>
        <button type="button" class="secondary" onclick="closeOverlay('settings-overlay')">Close</button>
      </div>
      <div class="overlay-body">
        <nav class="tabs" aria-label="Settings sections">
          <button class="tab-btn active" type="button" onclick="setTab('overview')">Overview</button>
          <button class="tab-btn" type="button" onclick="setTab('providers')">Providers</button>
          <button class="tab-btn" type="button" onclick="setTab('jobs')">Jobs</button>
          <button class="tab-btn" type="button" onclick="setTab('schedules')">Schedules</button>
          <button class="tab-btn" type="button" onclick="setTab('secrets')">Secrets</button>
          <button class="tab-btn" type="button" onclick="setTab('limits')">Limits</button>
          <button class="tab-btn" type="button" onclick="setTab('events')">Events</button>
        </nav>
        <div class="tab-content">
          <section class="tab-pane active" data-tab="overview"><h2>System</h2><div id="overview-panel">Loading...</div></section>
          <section class="tab-pane" data-tab="providers"><h2>Providers</h2><div id="providers">Loading...</div><h2>Usage</h2><div id="usage">Loading...</div><h2>Third Eye</h2><div id="third-eye" class="card">Loading...</div></section>
          <section class="tab-pane" data-tab="jobs"><h2>Jobs</h2><div id="jobs">Loading...</div></section>
          <section class="tab-pane" data-tab="schedules">
            <h2>Schedules</h2><div id="schedules">Loading...</div>
            <form id="schedule-form" class="card">
              <label for="schedule_name">Schedule name</label><input id="schedule_name" autocomplete="off" placeholder="daily.status">
              <div class="grid-2"><div><label for="schedule_kind">Kind</label><select id="schedule_kind"><option value="reminder">Reminder</option><option value="agent-task">Agent task</option></select></div><div><label for="schedule_every">Every seconds</label><input id="schedule_every" type="number" min="1" value="3600"></div></div>
              <label for="schedule_message">Message</label><input id="schedule_message" autocomplete="off">
              <label for="schedule_project">Project</label><input id="schedule_project" autocomplete="off">
              <label for="schedule_provider">Provider</label><select id="schedule_provider"><option value="codex">Codex</option><option value="openrouter">OpenRouter</option><option value="claude-code">Claude Code</option></select>
              <details><summary>Advanced schedule options</summary><label for="schedule_secret_grant_token">Secret grant token</label><input id="schedule_secret_grant_token" autocomplete="off"><div class="row"><input id="schedule_network" type="checkbox"><label for="schedule_network">Allow network</label></div></details>
              <label for="schedule_goal">Agent goal</label><textarea id="schedule_goal"></textarea>
              <div class="grid-2"><button type="submit">Save Schedule</button><button type="button" class="secondary" onclick="resetScheduleForm()">Clear</button></div>
            </form>
          </section>
          <section class="tab-pane" data-tab="secrets">
            <h2>Secrets</h2><div id="secrets">Loading...</div><div id="secret-grants">Loading...</div>
            <div class="grid-2">
              <form id="secret-form" class="card"><h3>Store Secret</h3><label for="secret_name">Secret name</label><input id="secret_name" autocomplete="off" placeholder="openrouter.default"><label for="secret_provider">Provider</label><input id="secret_provider" autocomplete="off" placeholder="openrouter"><label for="secret_kind">Kind</label><input id="secret_kind" autocomplete="off" value="api-key"><label for="secret_value">Value</label><input id="secret_value" type="password" autocomplete="off"><button type="submit">Store Secret</button></form>
              <form id="grant-form" class="card"><h3>Create Grant</h3><label for="grant_secret">Secret name or id</label><input id="grant_secret" autocomplete="off" placeholder="openrouter.default"><label for="grant_provider">Provider</label><input id="grant_provider" autocomplete="off" placeholder="openrouter"><div class="grid-2"><div><label for="grant_capability">Capability</label><input id="grant_capability" autocomplete="off" value="provider-proxy"></div><div><label for="grant_ttl">TTL seconds</label><input id="grant_ttl" type="number" min="1" value="900"></div></div><label for="grant_max_uses">Max uses</label><input id="grant_max_uses" type="number" min="1" value="1"><button type="submit">Create Grant</button></form>
            </div>
          </section>
          <section class="tab-pane" data-tab="limits">
            <h2>Worker</h2><form id="worker-form" class="card"><label for="worker_concurrency">Max concurrent jobs</label><div class="grid-2"><input id="worker_concurrency" type="number" min="1" value="__WORKER_CONCURRENCY__"><button type="submit">Save</button></div></form>
            <h2>Routing</h2><form id="routing-form" class="card"><div class="row"><input id="fallback_enabled" type="checkbox"><label for="fallback_enabled">Use fallback provider when paused</label></div><label for="fallback_order">Fallback order</label><input id="fallback_order" autocomplete="off" value="codex, openrouter, claude-code"><button type="submit">Save Routing</button></form>
            <h2>Budget</h2><form id="budget-form" class="card"><div class="row"><input id="budget_enabled" type="checkbox"><label for="budget_enabled">Enforce daily budget guardrails</label></div><div class="grid-2"><div><label for="budget_total">Total USD/day</label><input id="budget_total" type="number" min="0" step="0.01"></div><div><label for="budget_provider">Provider USD/day</label><input id="budget_provider" type="number" min="0" step="0.01"></div></div><label for="budget_project">Project USD/day</label><input id="budget_project" type="number" min="0" step="0.01"><button type="submit">Save Budget</button></form>
          </section>
          <section class="tab-pane" data-tab="events"><h2>Recent Actions</h2><div id="system-events">Loading...</div></section>
        </div>
      </div>
    </div>
  </div>

  <div class="overlay" id="map-overlay" role="dialog" aria-modal="true" aria-label="Project map">
    <div class="overlay-panel">
      <div class="overlay-head"><div><h1>Project Map</h1><div class="muted tiny">A visual project tree from Librarian's project registry. Knowledge-base project folders come next.</div></div><button type="button" class="secondary" onclick="closeOverlay('map-overlay')">Close</button></div>
      <div class="map-body">
        <div class="map-canvas" id="project-map"></div>
        <aside class="map-side"><h2>Project Model</h2><p class="muted">Librarian will keep long-lived project memory in Markdown project folders, and optionally attach each project to a real working directory for agent runs.</p><div class="card"><b>Default workspace</b><div class="muted">~/Librarian/Projects/{ProjectName}</div></div><div class="card"><b>Pattern to capture</b><div class="muted">When launched inside an unknown folder, Librarian should ask whether to register it as a working project and create/link a project memory folder.</div></div><div id="project-map-list"></div></aside>
      </div>
    </div>
  </div>

  <script>
    let cachedProjects = [];
    const $ = selector => document.querySelector(selector);
    function escapeHtml(value) { return String(value ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])); }
    function asJson(value) { return escapeHtml(JSON.stringify(value, null, 2)); }
    function shortId(value) { return String(value || '').slice(0, 8); }
    function shortToken(value) { const token = String(value || ''); return token.length > 18 ? `${token.slice(0, 10)}...${token.slice(-6)}` : token; }
    function openOverlay(id) { document.getElementById(id).classList.add('open'); }
    function closeOverlay(id) { document.getElementById(id).classList.remove('open'); }
    function setTab(name) {
      document.querySelectorAll('.tab-btn').forEach(button => button.classList.toggle('active', button.textContent.trim().toLowerCase() === name));
      document.querySelectorAll('.tab-pane').forEach(pane => pane.classList.toggle('active', pane.dataset.tab === name));
    }
    function openProjectMap() { renderProjectMap(cachedProjects); openOverlay('map-overlay'); }
    async function load() {
      const [health, projects, jobs, schedules, systemEvents, providers, usage, thirdEye, secrets, grants] = await Promise.all([
        fetch('/api/health').then(r => r.json()), fetch('/api/projects').then(r => r.json()), fetch('/api/jobs').then(r => r.json()), fetch('/api/schedules').then(r => r.json()), fetch('/api/system-events').then(r => r.json()), fetch('/api/providers').then(r => r.json()), fetch('/api/usage').then(r => r.json()), fetch('/api/third-eye').then(r => r.json()), fetch('/api/secrets').then(r => r.json()), fetch('/api/secrets/grants').then(r => r.json())
      ]);
      cachedProjects = projects;
      renderStatus(health, providers, thirdEye);
      renderProjectSelectors(projects);
      renderSettings(health, projects, jobs, schedules, systemEvents, providers, usage, thirdEye, secrets, grants);
    }
    function renderStatus(health, providers, thirdEye) {
      worker.innerHTML = `<b>${health.worker.running_jobs} / ${health.worker.max_concurrent_jobs}</b> slots used<br><span class="muted">Queued: ${health.worker.queued_jobs} · Available: ${health.worker.available_slots}<br>Runtime: ${escapeHtml(health.container_runtime)}</span>`;
      memory.innerHTML = `<b>${health.memory.embedded_items} / ${health.memory.items}</b> memories embedded<br><span class="muted">${escapeHtml(health.memory.embedding_backend)} · ${escapeHtml(health.memory.embedding_model)} · ${health.memory.embedding_dimensions}d<br>Missing: ${health.memory.missing_embeddings}</span>`;
      const known = (providers.catalog || []).length;
      const paused = (providers.states || []).filter(state => state.status === 'Paused').length;
      $('#providers-summary').innerHTML = `<b>${known}</b> providers known<br><span class="muted">${paused} paused · Third Eye ${thirdEye.enabled ? 'enabled' : 'disabled'}</span>`;
      $('#overview-panel').innerHTML = `<div class="grid-2"><div class="card">${worker.innerHTML}</div><div class="card">${memory.innerHTML}</div><div class="card"><b>Secrets</b><br><span class="muted">${escapeHtml(health.secrets.status)} · ${escapeHtml(health.secrets.mode)}</span></div><div class="card"><b>Admin</b><br><span class="muted">Network disabled by default · Worker concurrency __WORKER_CONCURRENCY__</span></div></div>`;
    }
    function renderProjectSelectors(projects) {
      if (!projects.length) {
        project.innerHTML = '<option value="">No projects yet</option>';
        $('#projects-summary').innerHTML = '<div class="card muted">No projects registered yet. Add one from the CLI for now: librarian project add &lt;path&gt;</div>';
        return;
      }
      project.innerHTML = projects.map(p => `<option value="${escapeHtml(p.name)}">${escapeHtml(p.name)}</option>`).join('');
      $('#projects-summary').innerHTML = projects.slice(0, 6).map(p => `<div class="card"><b>${escapeHtml(p.name)}</b><br><span class="muted">${escapeHtml(p.path)}</span></div>`).join('');
    }
    function renderSettings(health, projects, jobs, schedules, systemEvents, providers, usage, thirdEye, secrets, grants) {
      const stateByKey = new Map((providers.states || []).map(state => [`${state.provider}:${state.model || ''}`, state]));
      $('#providers').innerHTML = providers.catalog.length ? providers.catalog.map(model => {
        const state = stateByKey.get(`${model.provider}:${model.model}`) || stateByKey.get(`${model.provider}:`) || {};
        const paused = state.status === 'Paused';
        return `<div class="card"><b>${escapeHtml(model.provider)}</b><br><span class="muted">${escapeHtml(model.model)} · ${escapeHtml(state.status || 'Ready')}</span><br>${(model.task_hints || []).map(hint => `<span class="pill">${escapeHtml(hint)}</span>`).join('')}${paused ? `<br><span class="muted">Paused until ${escapeHtml(state.paused_until || '-')}<br>${escapeHtml(state.reason || '')}</span>` : ''}<div class="mini"><button type="button" class="secondary" onclick="pauseProvider('${escapeHtml(model.provider)}', '${escapeHtml(model.model)}')">Pause 30m</button><button type="button" onclick="resumeProvider('${escapeHtml(model.provider)}', '${escapeHtml(model.model)}')">Resume</button></div></div>`;
      }).join('') : 'No providers.';
      $('#usage').innerHTML = usage.length ? usage.slice(0, 8).map(event => `<div class="card action"><b>${escapeHtml(event.provider)}</b> <span class="muted">${escapeHtml(event.model || '-')}</span><br><span class="muted">${escapeHtml(event.observed_at)} · job ${escapeHtml(shortId(event.job_id) || '-')}</span><br>input=${event.input_tokens ?? '-'} output=${event.output_tokens ?? '-'} cost=${event.cost_usd ?? '-'} limit=${event.limit_event}</div>`).join('') : 'No usage observations.';
      $('#third-eye').innerHTML = `<b>${thirdEye.enabled ? 'Enabled' : 'Disabled'}</b><br><span class="muted">${escapeHtml(thirdEye.base_url)}<br>API: ${thirdEye.health.reachable ? 'reachable' : 'offline'} / ${thirdEye.health.api_ok ? 'ok' : 'not ok'}<br>DB: ${thirdEye.db_summary ? `${thirdEye.db_summary.api_calls} calls, $${Number(thirdEye.db_summary.total_cost_usd || 0).toFixed(4)}` : 'not configured'}</span>`;
      $('#secrets').innerHTML = secrets.length ? secrets.slice(0, 8).map(secret => `<div class="card"><b>${escapeHtml(secret.name)}</b><br><span class="muted">${escapeHtml(secret.provider)} · ${escapeHtml(secret.kind)} · ${escapeHtml(secret.encryption)}<br>${escapeHtml(secret.updated_at)}</span></div>`).join('') : 'No secrets stored.';
      $('#secret-grants').innerHTML = grants.length ? grants.slice(0, 6).map(grant => `<div class="card"><b>${escapeHtml(shortId(grant.id))}</b> <span class="muted">${escapeHtml(grant.provider || '-')}</span><br><span class="muted">capability=${escapeHtml(grant.capability)} uses=${grant.uses}/${grant.max_uses} expires=${escapeHtml(grant.expires_at)}</span></div>`).join('') : 'No active grants listed.';
      worker_concurrency.value = health.worker.max_concurrent_jobs;
      fallback_enabled.checked = Boolean(health.routing.fallback_enabled);
      fallback_order.value = (health.routing.fallback_order || []).join(', ');
      budget_enabled.checked = Boolean(health.budget.enabled);
      budget_total.value = health.budget.daily_total_usd ?? '';
      budget_provider.value = health.budget.daily_provider_usd ?? '';
      budget_project.value = health.budget.daily_project_usd ?? '';
      wireGrantTokenHints(grants);
      $('#jobs-summary').innerHTML = jobs.length ? jobs.slice(0, 5).map(renderJobCard).join('') : '<div class="card muted">No jobs yet.</div>';
      $('#jobs').innerHTML = renderJobs(jobs);
      $('#schedules').innerHTML = schedules.length ? schedules.map(s => `<div class="card"><b>${escapeHtml(s.name)}</b><br><span class="muted">${escapeHtml(s.kind)} · ${escapeHtml(s.status)} · every ${s.interval_seconds}s<br>Next: ${escapeHtml(s.next_run_at)}</span><div class="actions"><button type="button" onclick="runSchedule('${s.id}')">Run</button><button type="button" class="secondary" onclick='editSchedule(${JSON.stringify(s)})'>Edit</button><button type="button" class="danger" onclick="deleteSchedule('${s.id}')">Delete</button><button type="button" class="secondary" onclick="enableSchedule('${s.id}')">Enable</button><button type="button" class="danger" onclick="disableSchedule('${s.id}')">Disable</button></div></div>`).join('') : 'No schedules.';
      $('#system-events').innerHTML = systemEvents.length ? systemEvents.map(e => `<div class="card action"><b>${escapeHtml(e.kind)}</b><br><span class="muted">${escapeHtml(e.created_at)}</span><br><pre>${asJson(e.payload)}</pre></div>`).join('') : 'No actions recorded yet.';
    }
    function renderProjectMap(projects) {
      const canvas = $('#project-map');
      const width = canvas.clientWidth || 900;
      const height = canvas.clientHeight || 600;
      const centerY = Math.max(120, height / 2 - 42);
      const rootX = 70;
      const childX = Math.min(width - 280, 390);
      const gap = Math.max(96, Math.min(140, (height - 140) / Math.max(1, projects.length)));
      let html = `<svg class="tree-svg" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none">`;
      projects.forEach((p, index) => {
        const y = 80 + index * gap;
        html += `<path d="M ${rootX + 210} ${centerY + 42} C ${rootX + 300} ${centerY + 42}, ${childX - 80} ${y + 42}, ${childX} ${y + 42}" stroke="#58c4a5" stroke-width="2" fill="none" opacity="0.65"/>`;
      });
      html += '</svg>';
      html += `<div class="node root" style="left:${rootX}px;top:${centerY}px"><b>Librarian</b><br><span class="muted">Knowledge projects</span></div>`;
      projects.forEach((p, index) => {
        const y = 80 + index * gap;
        html += `<div class="node" style="left:${childX}px;top:${y}px"><b>${escapeHtml(p.name)}</b><br><span class="muted">${escapeHtml(p.path)}</span></div>`;
      });
      if (!projects.length) html += `<div class="node" style="left:${childX}px;top:${centerY}px"><b>No projects yet</b><br><span class="muted">Add a project to grow the tree.</span></div>`;
      canvas.innerHTML = html;
      $('#project-map-list').innerHTML = projects.length ? projects.map(p => `<div class="card"><b>${escapeHtml(p.name)}</b><br><span class="muted">${escapeHtml(p.path)}</span></div>`).join('') : '<div class="card muted">No registered projects.</div>';
    }
    function renderJobs(jobs) {
      if (!jobs.length) return 'No jobs yet.';
      const groups = [['Active', job => ['Preparing', 'Running', 'HeartbeatMissed', 'Recovering'].includes(job.status)], ['Queued', job => job.status === 'Queued'], ['Failed / Cancelled', job => ['Failed', 'Cancelled'].includes(job.status)], ['Completed', job => job.status === 'Completed']];
      return groups.map(([label, predicate]) => {
        const groupJobs = jobs.filter(predicate);
        if (!groupJobs.length) return '';
        return `<details open><summary>${label} (${groupJobs.length})</summary>${groupJobs.map(renderJobCard).join('')}</details>`;
      }).join('') || 'No jobs yet.';
    }
    function renderJobCard(j) {
      return `<div class="card"><b>${escapeHtml(j.status)}</b> <span class="muted">${escapeHtml(j.provider)} · ${escapeHtml(shortId(j.id))}</span><br>${escapeHtml(j.goal)}<br><span class="muted">Created: ${escapeHtml(j.created_at)}</span><div class="actions"><button type="button" class="secondary" onclick="detailsFor('${j.id}')">Details</button><button type="button" onclick="preflightJob('${j.id}')">Preflight</button><button type="button" class="danger" onclick="cancelJob('${j.id}')">Cancel</button><button type="button" onclick="retryJob('${j.id}')">Retry</button></div></div>`;
    }
    async function detailsFor(id) {
      const [job, events] = await Promise.all([fetch(`/api/jobs/${id}`).then(r => r.json()), fetch(`/api/jobs/${id}/events`).then(r => r.json())]);
      output.innerHTML = renderJobDetail(job, events);
    }
    function renderJobDetail(job, events) {
      return `<div class="message job"><b>${escapeHtml(job.status)}</b> <span class="muted">${escapeHtml(job.provider)} · ${escapeHtml(job.id)}</span><br><div>${escapeHtml(job.goal)}</div><div class="actions"><button type="button" onclick="preflightJob('${job.id}')">Preflight</button><button type="button" class="danger" onclick="cancelJob('${job.id}')">Cancel</button><button type="button" onclick="retryJob('${job.id}')">Retry</button></div></div>${renderJobEvents(events)}`;
    }
    function renderJobEvents(events) {
      if (!events.length) return '<div class="message system">No events for this job.</div>';
      return events.map(event => {
        const payload = event.payload || {};
        let body = '';
        if (event.kind === 'stdout' || event.kind === 'stderr') body = `<pre>${escapeHtml(payload.line || '')}</pre>`;
        else if (event.kind === 'preflight' || event.kind === 'prepared') body = `<div class="muted">Project: ${escapeHtml(payload.project_name || '-')} · context hits=${payload.context_hits ?? 0} · prompt chars=${payload.prompt_chars ?? 0}</div><details><summary>Prepared command</summary><pre>${asJson(payload.command || [])}</pre></details>`;
        else if (event.kind === 'failure_category') { const category = payload.category || {}; body = `<div><span class="pill">${escapeHtml(category.severity || 'error')}</span> ${escapeHtml(category.code || 'unknown_failure')}</div><div>${escapeHtml(category.message || '')}</div><div class="muted">${escapeHtml(category.next_step || '')}</div>`; }
        else body = `<pre>${asJson(payload)}</pre>`;
        return `<div class="message job action"><b>${escapeHtml(event.kind)}</b><br><span class="muted">${escapeHtml(event.created_at)}</span>${body}</div>`;
      }).join('');
    }
    function wireGrantTokenHints(grants) {
      const value = grants.filter(grant => grant.token).map(grant => grant.token)[0] || '';
      secret_grant_token.placeholder = value || 'optional';
      schedule_secret_grant_token.placeholder = value || 'optional';
    }
    async function postAction(url, options = {}) {
      const data = await fetch(url, options).then(r => r.json());
      output.innerHTML = `<div class="message system">${escapeHtml(JSON.stringify(data, null, 2))}</div>`;
      await load();
    }
    async function pauseProvider(provider, model) { await postAction('/api/providers/pause', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ provider, model, seconds: 1800, reason: 'manual admin pause' }) }); }
    async function resumeProvider(provider, model) { await postAction('/api/providers/resume', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ provider, model }) }); }
    async function cancelJob(id) { await postAction(`/api/jobs/${id}/cancel`, { method: 'POST' }); }
    async function preflightJob(id) { await postAction(`/api/jobs/${id}/preflight`, { method: 'POST' }); }
    async function retryJob(id) { await postAction(`/api/jobs/${id}/retry`, { method: 'POST' }); }
    async function runSchedule(id) { await postAction(`/api/schedules/${id}/run`, { method: 'POST' }); }
    async function enableSchedule(id) { await postAction(`/api/schedules/${id}/enable`, { method: 'POST' }); }
    async function disableSchedule(id) { await postAction(`/api/schedules/${id}/disable`, { method: 'POST' }); }
    async function deleteSchedule(id) { await postAction(`/api/schedules/${id}`, { method: 'DELETE' }); }
    function editSchedule(schedule) {
      schedule_name.value = schedule.name;
      schedule_kind.value = schedule.kind === 'AgentTask' ? 'agent-task' : schedule.kind.toLowerCase();
      schedule_every.value = schedule.interval_seconds;
      schedule_message.value = schedule.payload.message || schedule.payload.task || '';
      schedule_project.value = schedule.payload.project || '';
      schedule_provider.value = schedule.payload.provider || 'codex';
      schedule_secret_grant_token.value = schedule.payload.secret_grant_token || '';
      schedule_goal.value = schedule.payload.goal || '';
      schedule_network.checked = Boolean(schedule.payload.allow_network);
      schedule_form.dataset.scheduleId = schedule.id;
      setTab('schedules');
      openOverlay('settings-overlay');
    }
    function resetScheduleForm() { schedule_form.reset(); schedule_every.value = 3600; delete schedule_form.dataset.scheduleId; }
    schedule_form.addEventListener('submit', async event => {
      event.preventDefault();
      const body = { name: schedule_name.value, kind: schedule_kind.value, every_seconds: Number(schedule_every.value || 1), message: schedule_message.value, project: schedule_project.value, provider: schedule_provider.value, secret_grant_token: schedule_secret_grant_token.value || null, goal: schedule_goal.value, allow_network: schedule_network.checked };
      const id = schedule_form.dataset.scheduleId;
      await postAction(id ? `/api/schedules/${id}` : '/api/schedules', { method: id ? 'PATCH' : 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
      resetScheduleForm();
    });
    const worker_form = document.querySelector('#worker-form');
    worker_form.addEventListener('submit', async event => { event.preventDefault(); await postAction('/api/settings/worker', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ max_concurrent_jobs: Number(worker_concurrency.value || 1) }) }); });
    const routing_form = document.querySelector('#routing-form');
    routing_form.addEventListener('submit', async event => { event.preventDefault(); await postAction('/api/settings/routing', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ fallback_enabled: fallback_enabled.checked, fallback_order: fallback_order.value.split(',').map(value => value.trim()).filter(Boolean) }) }); });
    function optionalNumber(value) { return value === '' ? null : Number(value); }
    const budget_form = document.querySelector('#budget-form');
    budget_form.addEventListener('submit', async event => { event.preventDefault(); await postAction('/api/settings/budget', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ enabled: budget_enabled.checked, daily_total_usd: optionalNumber(budget_total.value), daily_provider_usd: optionalNumber(budget_provider.value), daily_project_usd: optionalNumber(budget_project.value) }) }); });
    const secret_form = document.querySelector('#secret-form');
    secret_form.addEventListener('submit', async event => { event.preventDefault(); await postAction('/api/secrets', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ name: secret_name.value, provider: secret_provider.value, kind: secret_kind.value || 'api-key', value: secret_value.value }) }); secret_value.value = ''; });
    const grant_form = document.querySelector('#grant-form');
    grant_form.addEventListener('submit', async event => { event.preventDefault(); await postAction('/api/secrets/grants', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ secret: grant_secret.value, provider: grant_provider.value || null, capability: grant_capability.value || 'provider-proxy', ttl_seconds: Number(grant_ttl.value || 900), max_uses: Number(grant_max_uses.value || 1) }) }); });
    chat.addEventListener('submit', async event => {
      event.preventDefault();
      if (!project.value) {
        output.innerHTML = '<div class="message system">Add or select a project first. Project creation from the web UI is the next workflow to land.</div>';
        return;
      }
      const body = { project: project.value, provider: provider.value, secret_grant_token: secret_grant_token.value || null, goal: goal.value, allow_network: allow_network.checked };
      const data = await fetch('/api/chat', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }).then(r => r.json());
      output.innerHTML = `<div class="message system">Queued agent work.</div><div class="message job"><pre>${escapeHtml(JSON.stringify(data, null, 2))}</pre></div>`;
      goal.value = '';
      await load();
    });
    window.addEventListener('resize', () => { if (document.getElementById('map-overlay').classList.contains('open')) renderProjectMap(cachedProjects); });
    load();
  </script>
</body>
</html>"##;
    html.replace("__BIND__", bind)
        .replace("__WORKER_CONCURRENCY__", &worker_concurrency.to_string())
}

pub async fn serve(bind: String, db: Database, config: Config) -> Result<()> {
    let state = AppState {
        db,
        config: Arc::new(RwLock::new(config)),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/projects", get(projects).post(create_project))
        .route("/api/project-map", get(project_map))
        .route(
            "/api/prompt-blocks",
            get(prompt_blocks).post(create_prompt_block),
        )
        .route("/api/prompt-blocks/render", get(render_prompt_target))
        .route(
            "/api/prompt-blocks/export-proposal",
            post(propose_prompt_export),
        )
        .route(
            "/api/prompt-blocks/:id",
            patch(update_prompt_block).delete(delete_prompt_block),
        )
        .route("/api/prompt-blocks/:id/enable", post(enable_prompt_block))
        .route("/api/prompt-blocks/:id/disable", post(disable_prompt_block))
        .route(
            "/api/projects/:id/attach-library",
            post(attach_project_library),
        )
        .route(
            "/api/projects/:id/attach-workspace",
            post(attach_project_workspace),
        )
        .route("/api/jobs", get(jobs).post(create_job))
        .route("/api/schedules", get(schedules).post(create_schedule))
        .route("/api/settings/worker", post(update_worker_settings))
        .route("/api/settings/chat", post(update_chat_settings))
        .route("/api/settings/codex", post(update_codex_runtime_settings))
        .route("/api/settings/claude", post(update_claude_runtime_settings))
        .route("/api/settings/routing", post(update_routing_settings))
        .route("/api/settings/budget", post(update_budget_settings))
        .route("/api/secrets", get(secrets).post(create_secret))
        .route(
            "/api/secrets/grants",
            get(secret_grants).post(create_secret_grant),
        )
        .route("/api/secrets/audit", get(secret_audit))
        .route("/api/system-events", get(system_events))
        .route("/api/providers", get(providers_status))
        .route("/api/providers/pause", post(pause_provider))
        .route("/api/providers/resume", post(resume_provider))
        .route("/api/usage", get(usage_observations))
        .route("/api/third-eye", get(third_eye_status))
        .route("/api/library/tree", get(library_tree))
        .route("/api/library/folders", post(library_create_folder))
        .route("/api/library/files", post(library_create_file))
        .route(
            "/api/library/markdown",
            get(library_read_markdown).post(library_write_markdown),
        )
        .route("/api/library/move", post(library_move))
        .route("/api/library/delete", post(library_delete))
        .route("/api/jobs/:id", get(job))
        .route("/api/jobs/:id/events", get(job_events))
        .route("/api/jobs/:id/preflight", post(preflight_job))
        .route("/api/jobs/:id/cancel", post(cancel_job))
        .route("/api/jobs/:id/retry", post(retry_job))
        .route("/api/schedules/:id/enable", post(enable_schedule))
        .route("/api/schedules/:id/disable", post(disable_schedule))
        .route("/api/schedules/:id/run", post(run_schedule))
        .route(
            "/api/schedules/:id",
            patch(update_schedule).delete(delete_schedule),
        )
        .route("/api/chat/sessions", get(chat_sessions))
        .route("/api/chat/sessions/:id/turns", get(chat_session_turns))
        .route("/api/chat", post(librarian_chat))
        .route("/api/slash-commands", get(slash_commands))
        .route("/api/approvals/:id/approve", post(approve_tool_approval))
        .route("/api/approvals/:id/reject", post(reject_tool_approval))
        .route("/api/agent-jobs", post(create_job))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("Librarian admin UI listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    Html(chat_first_app_html(
        &config.admin.bind,
        config.worker.max_concurrent_jobs,
    ))
}

async fn slash_commands(State(state): State<AppState>) -> impl IntoResponse {
    let mut commands = vec![
        serde_json::json!({"command": "/help", "description": "Show available command groups", "group": "general"}),
        serde_json::json!({"command": "/lib help", "description": "Knowledge base files and Markdown tools", "group": "library"}),
        serde_json::json!({"command": "/lib tree", "description": "Show the Library tree", "group": "library"}),
        serde_json::json!({"command": "/lib read ", "description": "Read a Markdown note", "group": "library"}),
        serde_json::json!({"command": "/lib append ", "description": "Append to a Markdown note", "group": "library"}),
        serde_json::json!({"command": "/lib replace-lines ", "description": "Replace a line range in a note", "group": "library"}),
        serde_json::json!({"command": "/lib replace-find ", "description": "Replace the first search match in a note", "group": "library"}),
        serde_json::json!({"command": "/work help", "description": "Project workspace folder tools", "group": "workspace"}),
        serde_json::json!({"command": "/work mkdir ", "description": "Create a workspace folder", "group": "workspace"}),
        serde_json::json!({"command": "/work touch ", "description": "Create an empty workspace file", "group": "workspace"}),
        serde_json::json!({"command": "/project help", "description": "Project records and attachments", "group": "project"}),
        serde_json::json!({"command": "/project list", "description": "List registered projects", "group": "project"}),
        serde_json::json!({"command": "/project create ", "description": "Create a library project", "group": "project"}),
        serde_json::json!({"command": "/project attach-workspace ", "description": "Attach an existing workspace directory", "group": "project"}),
        serde_json::json!({"command": "/mem help", "description": "Durable memory tools", "group": "memory"}),
        serde_json::json!({"command": "/remember ", "description": "Remember a durable fact", "group": "memory"}),
        serde_json::json!({"command": "/mem recent", "description": "Show recent durable memory", "group": "memory"}),
        serde_json::json!({"command": "/approval list", "description": "Review pending approvals", "group": "approval"}),
        serde_json::json!({"command": "/prompt blocks", "description": "List prompt blocks", "group": "prompt"}),
        serde_json::json!({"command": "/settings tool-permissions", "description": "Show tool permission policy", "group": "settings"}),
        serde_json::json!({"command": "/agent list", "description": "List background agent jobs", "group": "agent"}),
        serde_json::json!({"command": "/agent preflight ", "description": "Prepare a job command without running it", "group": "agent"}),
        serde_json::json!({"command": "/agent launch ", "description": "Queue an explicit background agent job", "group": "agent"}),
    ];
    if let Ok(projects) = state.db.list_projects().await {
        for project in projects.into_iter().take(20) {
            commands.push(serde_json::json!({
                "command": format!("/project status {}", project.name),
                "description": "Show project library/workspace status",
                "group": "project",
            }));
            commands.push(serde_json::json!({
                "command": format!("/agent launch --project \"{}\" --goal ", project.name),
                "description": "Queue an explicit agent job for this project",
                "group": "agent",
            }));
        }
    }
    Json(commands)
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let jobs = state.db.list_jobs().await.unwrap_or_default();
    let config = state.config.read().await;
    let memory_items = state.db.count_memory_items().await.unwrap_or_default();
    let memory_embeddings = state
        .db
        .count_memory_embeddings(&config.memory.embedding_model)
        .await
        .unwrap_or_default();
    let missing_embeddings = state
        .db
        .count_memory_missing_embedding(&config.memory.embedding_model)
        .await
        .unwrap_or_default();
    let provider_states = state.db.list_provider_states().await.unwrap_or_default();
    let running_jobs = jobs
        .iter()
        .filter(|job| matches!(job.status, JobStatus::Preparing | JobStatus::Running))
        .count();
    let queued_jobs = jobs
        .iter()
        .filter(|job| matches!(job.status, JobStatus::Queued))
        .count();
    let max_concurrent_jobs = config.worker.max_concurrent_jobs;
    let available_slots = max_concurrent_jobs.saturating_sub(running_jobs);
    Json(serde_json::json!({
        "ok": true,
            "worker": {
                "max_concurrent_jobs": max_concurrent_jobs,
                "running_jobs": running_jobs,
                "queued_jobs": queued_jobs,
                "available_slots": available_slots,
            },
            "chat": {
                "assistant_name": config.chat.assistant_name,
                "codex_timeout_seconds": config.chat.codex_timeout_seconds,
                "memory_hit_limit": config.chat.memory_hit_limit,
                "max_iterations": config.chat.max_iterations,
        },
        "tool_permissions": config.tool_permissions,
        "routing": {
            "fallback_enabled": config.routing.fallback_enabled,
            "fallback_order": config.routing.fallback_order,
        },
        "budget": {
            "enabled": config.budget.enabled,
            "daily_total_usd": config.budget.daily_total_usd,
            "daily_provider_usd": config.budget.daily_provider_usd,
            "daily_project_usd": config.budget.daily_project_usd,
        },
        "memory": {
            "embedding_backend": config.memory.embedding_backend,
            "embedding_model": config.memory.embedding_model,
            "embedding_dimensions": config.memory.embedding_dimensions,
            "items": memory_items,
            "embedded_items": memory_embeddings,
            "missing_embeddings": missing_embeddings,
        },
        "secrets": SecretVault::new(config.clone()).encryption_status(),
        "providers": provider_states,
        "container_runtime": config.docker.runtime_command,
    }))
}

async fn projects(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let projects = state.db.list_projects().await?;
    Ok(Json(
        projects.iter().map(project_api_json).collect::<Vec<_>>(),
    ))
}

fn project_api_json(project: &Project) -> serde_json::Value {
    let context_path = project
        .library_path
        .as_ref()
        .map(|path| path.to_string_lossy().replace('\\', "/"));
    let workspace_path = project.path.to_string_lossy().to_string();
    serde_json::json!({
        "id": project.id,
        "name": project.name,
        "library_path": context_path.clone(),
        "context_path": context_path,
        "workspace_path": workspace_path,
        "path": workspace_path,
        "autonomy_mode": project.autonomy_mode,
        "git_policy": project.git_policy,
        "created_at": project.created_at,
    })
}

async fn create_project(
    State(state): State<AppState>,
    Json(input): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "library.create",
        config.tool_permissions.library_create,
    )
    .await?;
    ensure_tool_permission(
        &state.db,
        &config,
        "workspace.create",
        config.tool_permissions.workspace_create,
    )
    .await?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(anyhow::anyhow!("Project name must not be empty").into());
    }
    let library_path = input
        .library_path
        .unwrap_or_else(|| format!("projects/{}", project_folder_name(name)));
    let library_path = library_tools::normalize_tool_relative_path(&library_path)?;
    library_tools::create_folder(&config, LibraryRoot::Library, &library_path)?;
    let workspace_path = if let Some(path) = input.workspace_path {
        canonical_existing_dir(&path)?
    } else {
        let relative = project_folder_name(name);
        library_tools::create_folder(&config, LibraryRoot::Projects, &relative)?;
        config.home.join("Projects").join(relative).canonicalize()?
    };
    let project = state.db.add_project(name, &workspace_path).await?;
    let project = state
        .db
        .attach_project_library_path(project.id, PathBuf::from(&library_path).as_path())
        .await?;
    log_project_event(
        &state.db,
        "create",
        serde_json::json!({
            "project_id": project.id,
            "name": project.name.clone(),
            "library_path": project.library_path.clone(),
            "workspace_path": project.path.clone(),
            "source": "admin-api",
        }),
    )
    .await?;
    Ok(Json(project_api_json(&project)))
}

async fn attach_project_library(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(input): Json<AttachLibraryRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "library.move",
        config.tool_permissions.library_move,
    )
    .await?;
    let library_path = library_tools::normalize_tool_relative_path(&input.library_path)?;
    let project = state
        .db
        .attach_project_library_path(id, PathBuf::from(&library_path).as_path())
        .await?;
    log_project_event(
        &state.db,
        "attach_library",
        serde_json::json!({
            "project_id": project.id,
            "library_path": project.library_path.clone(),
            "source": "admin-api",
        }),
    )
    .await?;
    Ok(Json(project_api_json(&project)))
}

async fn attach_project_workspace(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(input): Json<AttachWorkspaceRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "workspace.move",
        config.tool_permissions.workspace_move,
    )
    .await?;
    let workspace_path = canonical_existing_dir(&input.workspace_path)?;
    let project = state
        .db
        .update_project_workspace_path(id, &workspace_path)
        .await?;
    log_project_event(
        &state.db,
        "attach_workspace",
        serde_json::json!({
            "project_id": project.id,
            "workspace_path": project.path.clone(),
            "source": "admin-api",
        }),
    )
    .await?;
    Ok(Json(project_api_json(&project)))
}

async fn project_map(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let projects = state.db.list_projects().await?;
    Ok(Json(build_project_map(&config, projects)?))
}

async fn prompt_blocks(
    State(state): State<AppState>,
    Query(query): Query<PromptBlocksQuery>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state.db.list_prompt_blocks(query.target.as_deref()).await?,
    ))
}

async fn create_prompt_block(
    State(state): State<AppState>,
    Json(input): Json<CreatePromptBlockRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "settings.change",
        config.tool_permissions.settings_change,
    )
    .await?;
    let block = state
        .db
        .create_prompt_block(
            &input.target,
            &input.name,
            &input.content,
            input.markdown.unwrap_or(true),
        )
        .await?;
    state
        .db
        .add_system_event(
            "prompt_tool",
            serde_json::json!({
                "action": "add_block",
                "source": "admin-api",
                "block_id": block.id,
                "target": block.target,
            }),
        )
        .await?;
    Ok(Json(block))
}

async fn update_prompt_block(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(input): Json<UpdatePromptBlockRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "settings.change",
        config.tool_permissions.settings_change,
    )
    .await?;
    let block = state
        .db
        .update_prompt_block(
            id,
            input.name.as_deref(),
            input.content.as_deref(),
            input.enabled,
            input.position,
            input.markdown,
        )
        .await?;
    state
        .db
        .add_system_event(
            "prompt_tool",
            serde_json::json!({
                "action": "update_block",
                "source": "admin-api",
                "block_id": block.id,
                "target": block.target,
            }),
        )
        .await?;
    Ok(Json(block))
}

async fn delete_prompt_block(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "settings.change",
        config.tool_permissions.settings_change,
    )
    .await?;
    state.db.delete_prompt_block(id).await?;
    state
        .db
        .add_system_event(
            "prompt_tool",
            serde_json::json!({
                "action": "delete_block",
                "source": "admin-api",
                "block_id": id,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "block_id": id })))
}

async fn propose_prompt_export(
    State(state): State<AppState>,
    Json(input): Json<ExportPromptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let blocks = state.db.list_prompt_blocks(Some(&input.target)).await?;
    let rendered = render_prompt_blocks(&blocks);
    let approval = state
        .db
        .create_tool_approval(
            "library",
            "write_markdown",
            serde_json::json!({
                "path": input.path,
                "content": rendered,
                "target": input.target,
            }),
        )
        .await?;
    state
        .db
        .add_system_event(
            "tool_approval",
            serde_json::json!({
                "action": "propose_prompt_export",
                "approval_id": approval.id,
                "target": input.target,
            }),
        )
        .await?;
    Ok(Json(approval))
}

async fn enable_prompt_block(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    set_prompt_block_enabled_api(state, id, true).await
}

async fn disable_prompt_block(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    set_prompt_block_enabled_api(state, id, false).await
}

async fn set_prompt_block_enabled_api(
    state: AppState,
    id: Uuid,
    enabled: bool,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    ensure_tool_permission(
        &state.db,
        &config,
        "settings.change",
        config.tool_permissions.settings_change,
    )
    .await?;
    let block = state.db.set_prompt_block_enabled(id, enabled).await?;
    state
        .db
        .add_system_event(
            "prompt_tool",
            serde_json::json!({
                "action": if enabled { "enable_block" } else { "disable_block" },
                "source": "admin-api",
                "block_id": block.id,
                "target": block.target,
            }),
        )
        .await?;
    Ok(Json(block))
}

async fn render_prompt_target(
    State(state): State<AppState>,
    Query(query): Query<PromptBlocksQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(target) = query.target.as_deref() else {
        return Err(anyhow::anyhow!("target is required").into());
    };
    let blocks = state.db.list_prompt_blocks(Some(target)).await?;
    let rendered = render_prompt_blocks(&blocks);
    Ok(Json(serde_json::json!({
        "target": target,
        "rendered": rendered,
        "blocks": blocks,
    })))
}

async fn library_tree(
    State(state): State<AppState>,
    Query(query): Query<LibraryTreeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let max_depth = query.max_depth.unwrap_or(6);
    if let Some(root) = query.root {
        ensure_library_api_root(root)?;
    }
    Ok(Json(serde_json::json!({
        "roots": [library_tools::tree(&config, LibraryRoot::Library, max_depth)?],
    })))
}

async fn library_create_folder(
    State(state): State<AppState>,
    Json(input): Json<LibraryPathRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_library_api_root(input.root)?;
    let config = state.config.read().await.clone();
    let path = library_tools::create_folder(&config, input.root, &input.path)?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "create_folder",
                "root": input.root,
                "path": path.path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

async fn library_create_file(
    State(state): State<AppState>,
    Json(input): Json<LibraryPathRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_library_api_root(input.root)?;
    let config = state.config.read().await.clone();
    let path = library_tools::create_empty_file(&config, input.root, &input.path)?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "create_empty_file",
                "root": input.root,
                "path": path.path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

async fn library_read_markdown(
    State(state): State<AppState>,
    Query(input): Query<LibraryMarkdownRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let content = library_tools::read_markdown(&config, &input.path)?;
    Ok(Json(serde_json::json!({
        "root": "library",
        "path": input.path,
        "content": content,
    })))
}

async fn library_write_markdown(
    State(state): State<AppState>,
    Json(input): Json<LibraryMarkdownRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(content) = input.content.as_deref() else {
        return Err(anyhow::anyhow!("content is required").into());
    };
    let config = state.config.read().await.clone();
    let path = library_tools::write_markdown(&config, &input.path, content)?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "write_markdown",
                "root": "library",
                "path": path.path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

async fn library_move(
    State(state): State<AppState>,
    Json(input): Json<LibraryMoveRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_library_api_root(input.root)?;
    let config = state.config.read().await.clone();
    let path = library_tools::move_path(&config, input.root, &input.from, &input.to)?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "move",
                "root": input.root,
                "from": input.from,
                "to": path.path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

async fn library_delete(
    State(state): State<AppState>,
    Json(input): Json<LibraryDeleteRequest>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_library_api_root(input.root)?;
    let config = state.config.read().await.clone();
    let path = library_tools::delete_path(
        &config,
        input.root,
        &input.path,
        input.recursive.unwrap_or(false),
    )?;
    state
        .db
        .add_system_event(
            "library_tool",
            serde_json::json!({
                "action": "delete",
                "root": input.root,
                "path": path.path,
                "recursive": input.recursive.unwrap_or(false),
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": path })))
}

fn ensure_library_api_root(root: LibraryRoot) -> Result<()> {
    if root != LibraryRoot::Library {
        anyhow::bail!(
            "Library API only accepts root=library; use workspace/project tools for Projects"
        );
    }
    Ok(())
}

async fn jobs(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_jobs().await?))
}

async fn job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.get_job(id).await?))
}

async fn schedules(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_schedules().await?))
}

async fn create_schedule(
    State(state): State<AppState>,
    Json(input): Json<CreateScheduleRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let kind = parse_schedule_kind(&input.kind)?;
    let payload = schedule_payload(&kind, &input);
    let schedule = state
        .db
        .add_schedule(&input.name, kind, input.every_seconds.max(1), payload)
        .await?;
    state
        .db
        .add_system_event(
            "schedule_created",
            serde_json::json!({
                "schedule_id": schedule.id,
                "name": schedule.name,
                "kind": schedule.kind,
            }),
        )
        .await?;
    Ok(Json(schedule))
}

async fn update_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(input): Json<CreateScheduleRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let kind = parse_schedule_kind(&input.kind)?;
    let payload = schedule_payload(&kind, &input);
    let schedule = state
        .db
        .update_schedule(id, &input.name, kind, input.every_seconds.max(1), payload)
        .await?;
    state
        .db
        .add_system_event(
            "schedule_updated",
            serde_json::json!({ "schedule_id": schedule.id, "name": schedule.name }),
        )
        .await?;
    Ok(Json(schedule))
}

async fn delete_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let schedule = state.db.get_schedule(id).await?;
    state.db.delete_schedule(id).await?;
    state
        .db
        .add_system_event(
            "schedule_deleted",
            serde_json::json!({ "schedule_id": id, "name": schedule.name }),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "schedule_id": id })))
}

async fn system_events(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_system_events(50).await?))
}

async fn providers_status(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let states = state.db.list_provider_states().await?;
    let catalog = router::model_catalog();
    let config = state.config.read().await;
    let command_prefix = format!("librarian --home {}", admin_shell_path(&config.home));
    let default_codex_home = config.home.join(".cfg").join("codex-home");
    let codex_home = config
        .codex
        .host_home
        .as_ref()
        .unwrap_or(&default_codex_home);
    let default_claude_home = config.home.join(".cfg").join("claude-home");
    let claude_home = config
        .claude
        .host_home
        .as_ref()
        .unwrap_or(&default_claude_home);
    Ok(Json(serde_json::json!({
        "catalog": catalog,
        "states": states,
        "commands": {
            "codex_auth": format!(
                "CODEX_HOME={} codex\n{} auth codex --enable-container-mount --codex-home {}",
                admin_shell_path(codex_home),
                command_prefix,
                admin_shell_path(codex_home),
            ),
            "claude_auth": format!(
                "CLAUDE_HOME={} claude\n# Then save this profile path and enable the Claude mount in Settings -> Providers.",
                admin_shell_path(claude_home),
            ),
            "build_agent_image": format!("{command_prefix} runtime build-agent-image"),
            "smoke_codex": format!("{command_prefix} smoke mvp --provider codex --run-agent"),
            "smoke_claude": format!("{command_prefix} smoke mvp --provider claude-code --run-agent"),
        },
        "runtime": {
            "codex": {
                "host_home": config.codex.host_home.as_ref().map(|path| path.display().to_string()),
                "host_home_exists": config.codex.host_home.as_ref().map(|path| path.exists()),
                "mount_host_home": config.codex.mount_host_home,
                "mount_read_only": config.codex.mount_read_only,
                "container_home": config.codex.container_home,
            },
            "claude-code": {
                "host_home": config.claude.host_home.as_ref().map(|path| path.display().to_string()),
                "host_home_exists": config.claude.host_home.as_ref().map(|path| path.exists()),
                "mount_host_home": config.claude.mount_host_home,
                "mount_read_only": config.claude.mount_read_only,
                "container_home": config.claude.container_home,
                "instruction_file": config.claude.instruction_file,
            },
        },
    })))
}

async fn usage_observations(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_usage_observations(50).await?))
}

async fn third_eye_status(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let health = third_eye::health(&config).await?;
    let db_summary = third_eye::db_summary(&config).await?;
    Ok(Json(serde_json::json!({
        "enabled": config.third_eye.enabled,
        "base_url": config.third_eye.base_url,
        "db_path": config.third_eye.db_path,
        "project_export_dir": config.third_eye.project_export_dir,
        "health": health,
        "db_summary": db_summary,
    })))
}

async fn pause_provider(
    State(state): State<AppState>,
    Json(input): Json<ProviderControlRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let seconds = input.seconds.unwrap_or(1800).max(1);
    let reason = input
        .reason
        .unwrap_or_else(|| "manual admin pause".to_string());
    let paused_until = chrono::Utc::now() + chrono::Duration::seconds(seconds);
    let provider = state
        .db
        .set_provider_pause(
            &input.provider,
            input.model.as_deref(),
            paused_until,
            &reason,
        )
        .await?;
    state
        .db
        .add_system_event(
            "provider_paused",
            serde_json::json!({
                "provider": provider.provider,
                "model": provider.model,
                "paused_until": provider.paused_until,
                "reason": provider.reason,
            }),
        )
        .await?;
    Ok(Json(provider))
}

async fn resume_provider(
    State(state): State<AppState>,
    Json(input): Json<ProviderControlRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let provider = state
        .db
        .resume_provider(&input.provider, input.model.as_deref())
        .await?;
    state
        .db
        .add_system_event(
            "provider_resumed",
            serde_json::json!({
                "provider": provider.provider,
                "model": provider.model,
            }),
        )
        .await?;
    Ok(Json(provider))
}

async fn secrets(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let records = state.db.list_secret_records().await?;
    let redacted = records
        .into_iter()
        .map(|record| {
            serde_json::json!({
                "id": record.id,
                "name": record.name,
                "provider": record.provider,
                "kind": record.kind,
                "encryption": record.encryption,
                "created_at": record.created_at,
                "updated_at": record.updated_at,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(redacted))
}

async fn create_secret(
    State(state): State<AppState>,
    Json(input): Json<CreateSecretRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let vault = SecretVault::new(config);
    let record = vault
        .store(
            &state.db,
            &input.name,
            &input.provider,
            input.kind.as_deref().unwrap_or("api-key"),
            &input.value,
        )
        .await?;
    Ok(Json(serde_json::json!({
        "id": record.id,
        "name": record.name,
        "provider": record.provider,
        "kind": record.kind,
        "encryption": record.encryption,
    })))
}

async fn create_secret_grant(
    State(state): State<AppState>,
    Json(input): Json<CreateSecretGrantRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let vault = SecretVault::new(config);
    let grant_id = vault
        .grant(
            &state.db,
            &input.secret,
            None,
            input.provider.as_deref(),
            input.capability.as_deref().unwrap_or("read"),
            input.ttl_seconds.unwrap_or(900),
            input.max_uses.unwrap_or(1),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "grant_id": grant_id,
        "token": crate::secrets::encode_grant_token(grant_id),
    })))
}

async fn secret_grants(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let grants = state
        .db
        .list_secret_grants(50)
        .await?
        .into_iter()
        .map(|grant| {
            serde_json::json!({
                "id": grant.id,
                "token": crate::secrets::encode_grant_token(grant.id),
                "secret_id": grant.secret_id,
                "job_id": grant.job_id,
                "provider": grant.provider,
                "capability": grant.capability,
                "expires_at": grant.expires_at,
                "max_uses": grant.max_uses,
                "uses": grant.uses,
                "created_at": grant.created_at,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(grants))
}

async fn secret_audit(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_secret_audit_events(50).await?))
}

async fn job_events(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.db.list_job_events(id).await?))
}

async fn preflight_job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    Ok(Json(
        worker::preflight_job(config, state.db.clone(), id).await?,
    ))
}

async fn cancel_job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.db.request_cancel_job(id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "job_id": id })))
}

async fn retry_job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let retry = state.db.retry_job(id).await?;
    Ok(Json(retry))
}

async fn enable_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let schedule = state
        .db
        .set_schedule_status(id, ScheduleStatus::Enabled)
        .await?;
    state
        .db
        .add_system_event(
            "schedule_enabled",
            serde_json::json!({ "schedule_id": schedule.id, "name": schedule.name }),
        )
        .await?;
    Ok(Json(schedule))
}

async fn disable_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let schedule = state
        .db
        .set_schedule_status(id, ScheduleStatus::Disabled)
        .await?;
    state
        .db
        .add_system_event(
            "schedule_disabled",
            serde_json::json!({ "schedule_id": schedule.id, "name": schedule.name }),
        )
        .await?;
    Ok(Json(schedule))
}

async fn run_schedule(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    scheduler::run_schedule_now(&state.db, &config, id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "schedule_id": id })))
}

async fn update_worker_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateWorkerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (max_concurrent_jobs, config_path) = {
        let mut config = state.config.write().await;
        config.set_worker_concurrency(input.max_concurrent_jobs);
        config.save()?;
        (
            config.worker.max_concurrent_jobs,
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "worker_settings_updated",
            serde_json::json!({
                "max_concurrent_jobs": max_concurrent_jobs,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "worker": {
            "max_concurrent_jobs": max_concurrent_jobs,
        },
    })))
}

async fn update_chat_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateChatSettingsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (assistant_name, codex_timeout_seconds, memory_hit_limit, max_iterations, config_path) = {
        let mut config = state.config.write().await;
        if let Some(name) = input.assistant_name {
            let name = name.trim();
            config.chat.assistant_name = if name.is_empty() {
                "Librarian".to_string()
            } else {
                name.to_string()
            };
        }
        if let Some(timeout) = input.codex_timeout_seconds {
            config.chat.codex_timeout_seconds = timeout.max(1);
        }
        if let Some(limit) = input.memory_hit_limit {
            config.chat.memory_hit_limit = limit.max(1);
        }
        if let Some(iterations) = input.max_iterations {
            config.chat.max_iterations = iterations.clamp(1, 100);
        }
        config.save()?;
        (
            config.chat.assistant_name.clone(),
            config.chat.codex_timeout_seconds,
            config.chat.memory_hit_limit,
            config.chat.max_iterations,
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "chat_settings_updated",
            serde_json::json!({
                "assistant_name": assistant_name,
                "codex_timeout_seconds": codex_timeout_seconds,
                "memory_hit_limit": memory_hit_limit,
                "max_iterations": max_iterations,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "chat": {
            "assistant_name": assistant_name,
            "codex_timeout_seconds": codex_timeout_seconds,
            "memory_hit_limit": memory_hit_limit,
            "max_iterations": max_iterations,
        },
    })))
}

async fn update_codex_runtime_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateCodexRuntimeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (host_home, mount_host_home, mount_read_only, config_path) = {
        let mut config = state.config.write().await;
        if let Some(path) = input.host_home {
            let path = path.trim();
            config.codex.host_home = if path.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(path))
            };
        }
        if let Some(enabled) = input.mount_host_home {
            config.codex.mount_host_home = enabled;
        }
        if let Some(read_only) = input.mount_read_only {
            config.codex.mount_read_only = read_only;
        }
        config.save()?;
        (
            config
                .codex
                .host_home
                .as_ref()
                .map(|path| path.display().to_string()),
            config.codex.mount_host_home,
            config.codex.mount_read_only,
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "codex_runtime_updated",
            serde_json::json!({
                "host_home": host_home,
                "mount_host_home": mount_host_home,
                "mount_read_only": mount_read_only,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "codex": {
            "host_home": host_home,
            "mount_host_home": mount_host_home,
            "mount_read_only": mount_read_only,
        },
    })))
}

async fn update_claude_runtime_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateClaudeRuntimeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (host_home, mount_host_home, mount_read_only, instruction_file, config_path) = {
        let mut config = state.config.write().await;
        if let Some(path) = input.host_home {
            let path = path.trim();
            config.claude.host_home = if path.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(path))
            };
        }
        if let Some(enabled) = input.mount_host_home {
            config.claude.mount_host_home = enabled;
        }
        if let Some(read_only) = input.mount_read_only {
            config.claude.mount_read_only = read_only;
        }
        if let Some(file) = input.instruction_file {
            let file = file.trim();
            if file.is_empty() || file.contains('/') || file.contains('\\') {
                return Err(anyhow::anyhow!(
                    "Claude instruction file must be a filename like CLAUDE.md"
                )
                .into());
            }
            config.claude.instruction_file = file.to_string();
        }
        config.save()?;
        (
            config
                .claude
                .host_home
                .as_ref()
                .map(|path| path.display().to_string()),
            config.claude.mount_host_home,
            config.claude.mount_read_only,
            config.claude.instruction_file.clone(),
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "claude_runtime_updated",
            serde_json::json!({
                "host_home": host_home,
                "mount_host_home": mount_host_home,
                "mount_read_only": mount_read_only,
                "instruction_file": instruction_file,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "claude": {
            "host_home": host_home,
            "mount_host_home": mount_host_home,
            "mount_read_only": mount_read_only,
            "instruction_file": instruction_file,
        },
    })))
}

async fn update_routing_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateRoutingRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if input.fallback_order.is_empty() {
        return Err(anyhow::anyhow!("fallback_order must include at least one provider").into());
    }
    for provider in &input.fallback_order {
        router::parse_provider_kind(provider)?;
    }
    let (fallback_enabled, fallback_order, config_path) = {
        let mut config = state.config.write().await;
        config.routing.fallback_enabled = input.fallback_enabled;
        config.routing.fallback_order = input.fallback_order;
        config.save()?;
        (
            config.routing.fallback_enabled,
            config.routing.fallback_order.clone(),
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "routing_settings_updated",
            serde_json::json!({
                "fallback_enabled": fallback_enabled,
                "fallback_order": fallback_order,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "routing": {
            "fallback_enabled": fallback_enabled,
            "fallback_order": fallback_order,
        },
    })))
}

async fn update_budget_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateBudgetRequest>,
) -> Result<impl IntoResponse, ApiError> {
    for (label, value) in [
        ("daily_total_usd", input.daily_total_usd),
        ("daily_provider_usd", input.daily_provider_usd),
        ("daily_project_usd", input.daily_project_usd),
    ] {
        if let Some(value) = value {
            if value < 0.0 {
                return Err(anyhow::anyhow!("{label} must be non-negative").into());
            }
        }
    }

    let (enabled, daily_total_usd, daily_provider_usd, daily_project_usd, config_path) = {
        let mut config = state.config.write().await;
        config.budget.enabled = input.enabled;
        config.budget.daily_total_usd = input.daily_total_usd;
        config.budget.daily_provider_usd = input.daily_provider_usd;
        config.budget.daily_project_usd = input.daily_project_usd;
        config.save()?;
        (
            config.budget.enabled,
            config.budget.daily_total_usd,
            config.budget.daily_provider_usd,
            config.budget.daily_project_usd,
            config.config_path.clone(),
        )
    };
    state
        .db
        .add_system_event(
            "budget_settings_updated",
            serde_json::json!({
                "enabled": enabled,
                "daily_total_usd": daily_total_usd,
                "daily_provider_usd": daily_provider_usd,
                "daily_project_usd": daily_project_usd,
                "config_path": config_path,
            }),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "budget": {
            "enabled": enabled,
            "daily_total_usd": daily_total_usd,
            "daily_provider_usd": daily_provider_usd,
            "daily_project_usd": daily_project_usd,
        },
    })))
}

async fn chat_sessions(
    State(state): State<AppState>,
    Query(query): Query<ChatSessionsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let sessions = state
        .db
        .list_chat_sessions(query.limit.unwrap_or(20))
        .await?;
    let mut output = Vec::new();
    for session in sessions {
        let turn_count = state.db.list_chat_turns(session.id).await?.len();
        output.push(serde_json::json!({
            "id": session.id,
            "project_id": session.project_id,
            "title": session.title,
            "created_at": session.created_at,
            "updated_at": session.updated_at,
            "turn_count": turn_count,
        }));
    }
    Ok(Json(output))
}

async fn chat_session_turns(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let session = state.db.get_chat_session(id).await?;
    let turns = state.db.list_chat_turns(id).await?;
    Ok(Json(serde_json::json!({
        "session": session,
        "turns": turns,
    })))
}

async fn approve_tool_approval(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let (approval, output) = approve_and_execute_tool_approval(&state, &config, id).await?;
    Ok(Json(serde_json::json!({
        "approval": approval,
        "output": output,
    })))
}

async fn reject_tool_approval(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let approval = reject_tool_approval_by_id(&state, id).await?;
    Ok(Json(serde_json::json!({ "approval": approval })))
}

async fn resolve_chat_project_context(
    state: &AppState,
    config: &Config,
    input: &LibrarianChatRequest,
    message: &str,
) -> Result<ChatProjectContext> {
    let known_projects = state.db.list_projects().await?;
    let mut requested = Vec::new();
    if let Some(values) = &input.project_context {
        requested.extend(values.iter().map(String::as_str));
    }
    if let Some(value) = input.project.as_deref() {
        requested.push(value);
    }

    let mut nodes = Vec::new();
    for value in requested {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let node = resolve_library_context_node(config, &known_projects, value)?;
        if !nodes
            .iter()
            .any(|existing| same_context_node(existing, &node))
        {
            nodes.push(node);
        }
    }
    if !nodes.is_empty() {
        return Ok(ChatProjectContext {
            nodes,
            suggested_nodes: Vec::new(),
            source: "explicit",
        });
    }

    if message.trim_start().starts_with('/') {
        return Ok(ChatProjectContext {
            nodes: Vec::new(),
            suggested_nodes: Vec::new(),
            source: "global",
        });
    }

    let message_key = normalized_project_lookup_key(message);
    let mut matches = known_projects
        .iter()
        .filter(|project| {
            let name_key = normalized_project_lookup_key(&project.name);
            let path_key = project
                .library_path
                .as_ref()
                .map(|path| normalized_project_lookup_key(&path.to_string_lossy()))
                .unwrap_or_default();
            !name_key.is_empty()
                && (message_key.contains(&name_key)
                    || (!path_key.is_empty() && message_key.contains(&path_key)))
        })
        .cloned()
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.name.cmp(&right.name));
    let inferred_nodes = if matches.len() == 1 {
        matches
            .into_iter()
            .map(|project| ChatLibraryContextNode {
                library_path: project.library_path.clone(),
                project: Some(project),
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    match config.tool_permissions.context_switch {
        ToolPermissionPolicy::Auto if !inferred_nodes.is_empty() => Ok(ChatProjectContext {
            nodes: inferred_nodes,
            suggested_nodes: Vec::new(),
            source: "auto",
        }),
        ToolPermissionPolicy::Ask if !inferred_nodes.is_empty() => Ok(ChatProjectContext {
            nodes: Vec::new(),
            suggested_nodes: inferred_nodes,
            source: "suggested",
        }),
        _ => Ok(ChatProjectContext {
            nodes: Vec::new(),
            suggested_nodes: Vec::new(),
            source: "global",
        }),
    }
}

fn resolve_library_context_node(
    config: &Config,
    projects: &[Project],
    value: &str,
) -> Result<ChatLibraryContextNode> {
    if let Some(project) = find_project_context_ref(projects, value) {
        return Ok(ChatLibraryContextNode {
            library_path: project.library_path.clone(),
            project: Some(project.clone()),
        });
    }
    let library_path = normalize_library_context_path(config, value)?;
    let project = projects
        .iter()
        .find(|project| project.library_path.as_ref() == Some(&library_path))
        .cloned();
    Ok(ChatLibraryContextNode {
        library_path: Some(library_path),
        project,
    })
}

fn find_project_context_ref<'a>(projects: &'a [Project], value: &str) -> Option<&'a Project> {
    let normalized_value = value.trim().trim_start_matches('/').replace('\\', "/");
    projects.iter().find(|project| {
        project.id.to_string() == value
            || project.name == value
            || project
                .library_path
                .as_ref()
                .map(|path| path.to_string_lossy().replace('\\', "/") == normalized_value)
                .unwrap_or(false)
    })
}

fn normalize_library_context_path(config: &Config, value: &str) -> Result<PathBuf> {
    let trimmed = value
        .trim()
        .trim_start_matches("Library/")
        .trim_start_matches("Library\\")
        .trim_start_matches('/');
    let normalized = library_tools::normalize_tool_relative_path(trimmed)?;
    let relative = PathBuf::from(normalized);
    let absolute = config.vault_path.join(&relative);
    if !absolute.exists() {
        anyhow::bail!("Library context `{}` was not found", relative.display());
    }
    Ok(relative)
}

fn same_context_node(left: &ChatLibraryContextNode, right: &ChatLibraryContextNode) -> bool {
    if let (Some(left_project), Some(right_project)) = (&left.project, &right.project) {
        return left_project.id == right_project.id;
    }
    left.library_path == right.library_path
}

fn context_label_for_nodes(nodes: &[ChatLibraryContextNode]) -> String {
    if nodes.is_empty() {
        "Global conversation".to_string()
    } else {
        nodes
            .iter()
            .map(library_context_display_label)
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

fn library_context_metadata(node: &ChatLibraryContextNode) -> serde_json::Value {
    serde_json::json!({
        "kind": if node.library_path.is_some() { "library_node" } else { "project" },
        "label": library_context_display_label(node),
        "library_path": node.library_path.as_ref().map(|path| path.to_string_lossy().replace('\\', "/")),
        "project": node.project.as_ref().map(project_context_metadata),
    })
}

fn project_context_metadata(project: &Project) -> serde_json::Value {
    let context_path = project
        .library_path
        .as_ref()
        .map(|path| path.to_string_lossy().replace('\\', "/"));
    serde_json::json!({
        "id": project.id,
        "name": project.name,
        "display_name": project_display_label(project),
        "library_path": context_path.clone(),
        "context_path": context_path,
        "workspace_path": project.path.to_string_lossy().to_string(),
    })
}

fn library_context_display_label(node: &ChatLibraryContextNode) -> String {
    node.library_path
        .as_ref()
        .map(|path| {
            let value = path.to_string_lossy().replace('\\', "/");
            humanize_project_name(value.split('/').next_back().unwrap_or(&value))
        })
        .or_else(|| node.project.as_ref().map(project_display_label))
        .unwrap_or_else(|| "Global conversation".to_string())
}

fn project_display_label(project: &Project) -> String {
    project
        .library_path
        .as_ref()
        .and_then(|path| path.file_stem().or_else(|| path.file_name()))
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| project.name.clone())
        .trim_end_matches(".md")
        .split(['/', '\\'])
        .next_back()
        .map(humanize_project_name)
        .unwrap_or_else(|| humanize_project_name(&project.name))
}

fn humanize_project_name(value: &str) -> String {
    let mut out = String::new();
    let normalized = value
        .trim_end_matches(".md")
        .trim_end_matches(".MD")
        .replace(['_', '-', '/', '\\'], " ");
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut previous_lowercase_or_digit = false;
    let mut previous_uppercase = false;
    let mut previous_alpha = false;
    let mut previous_digit = false;
    for (index, character) in chars.iter().copied().enumerate() {
        let next_lowercase = chars
            .get(index + 1)
            .copied()
            .map(|next| next.is_ascii_lowercase())
            .unwrap_or(false);
        if character.is_ascii_uppercase()
            && (previous_lowercase_or_digit || (previous_uppercase && next_lowercase))
        {
            out.push(' ');
        } else if character.is_ascii_digit() && previous_alpha && !previous_digit {
            out.push(' ');
        } else if character.is_ascii_alphabetic() && previous_digit {
            out.push(' ');
        }
        if character == '.' {
            out.push(' ');
            previous_lowercase_or_digit = false;
            previous_uppercase = false;
            previous_alpha = false;
            previous_digit = false;
            continue;
        }
        previous_lowercase_or_digit = character.is_ascii_lowercase() || character.is_ascii_digit();
        previous_uppercase = character.is_ascii_uppercase();
        previous_alpha = character.is_ascii_alphabetic();
        previous_digit = character.is_ascii_digit();
        out.push(character);
    }
    out.split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!(
                    "{}{}",
                    first.to_uppercase().collect::<String>(),
                    chars.collect::<String>()
                ),
                None => String::new(),
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalized_project_lookup_key(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

async fn retrieve_chat_context_pack(
    db: &Database,
    config: &Config,
    query: &str,
    chat_context: &ChatProjectContext,
) -> Result<crate::domain::ContextPack> {
    let project_ids = context_project_ids_for_retrieval(db, chat_context).await?;
    if project_ids.is_empty() {
        return memory::retrieve_context_with_config(
            db,
            Some(config),
            memory::RetrievalRequest {
                query: query.to_string(),
                project_id: None,
                activity_id: None,
                limit: config.chat.memory_hit_limit,
            },
        )
        .await;
    }

    let mut packs = Vec::new();
    for project_id in &project_ids {
        packs.push(
            memory::retrieve_context_with_config(
                db,
                Some(config),
                memory::RetrievalRequest {
                    query: query.to_string(),
                    project_id: Some(*project_id),
                    activity_id: None,
                    limit: config.chat.memory_hit_limit,
                },
            )
            .await?,
        );
    }

    let mut hits = Vec::new();
    for pack in &packs {
        hits.extend(pack.hits.clone());
    }
    hits.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen = std::collections::HashSet::new();
    hits.retain(|hit| seen.insert(hit.item.id));
    hits.truncate(config.chat.memory_hit_limit.max(1));

    Ok(crate::domain::ContextPack {
        query: query.to_string(),
        project_id: project_ids.first().copied(),
        activity_id: None,
        generated_at: chrono::Utc::now(),
        hits,
    })
}

async fn context_project_ids_for_retrieval(
    db: &Database,
    chat_context: &ChatProjectContext,
) -> Result<Vec<Uuid>> {
    let all_projects = db.list_projects().await?;
    let mut ids = Vec::new();
    for node in &chat_context.nodes {
        if let Some(project) = &node.project {
            ids.push(project.id);
        }
        if let Some(library_path) = &node.library_path {
            for project in &all_projects {
                if project
                    .library_path
                    .as_ref()
                    .is_some_and(|project_path| library_path_contains(library_path, project_path))
                {
                    ids.push(project.id);
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn library_path_contains(parent: &Path, candidate: &Path) -> bool {
    candidate == parent || candidate.starts_with(parent)
}

async fn librarian_chat(
    State(state): State<AppState>,
    Json(input): Json<LibrarianChatRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let started_at = Instant::now();
    let message = input.message.trim();
    if message.is_empty() {
        return Err(anyhow::anyhow!("message must not be empty").into());
    }

    let config = state.config.read().await.clone();
    let chat_context = resolve_chat_project_context(&state, &config, &input, message).await?;
    let project = chat_context.primary_project();
    let project_id = chat_context.primary_project_id();
    let context_metadata = chat_context.metadata();
    let context_label = chat_context.label();
    let gated = gates::process_user_prompt(&state.db, &config, message, "librarian-chat").await?;
    let chat_session = match input.session_id {
        Some(session_id) => state.db.get_chat_session(session_id).await?,
        None => {
            state
                .db
                .create_chat_session(project_id, chat_session_title(&gated.content))
                .await?
        }
    };
    let previous_turns = state.db.list_chat_turns(chat_session.id).await?;

    let user_memory = state
        .db
        .add_memory_item(
            project_id,
            None,
            MemoryKind::UserMessage,
            Some("librarian-chat"),
            &gated.content,
            Some("admin:librarian-chat"),
            serde_json::json!({
                "project": project.map(|project| project.name.clone()),
                "scope": if chat_context.is_empty() { "global" } else { "project_context" },
                "context": context_metadata.clone(),
                "chat_session_id": chat_session.id,
                "memory_role": "raw_chat_turn",
                "durability": "transcript",
            }),
        )
        .await?;
    memory::embed_item(&state.db, &config, &user_memory).await?;
    state
        .db
        .add_chat_turn(
            chat_session.id,
            "user",
            &gated.content,
            Some(user_memory.id),
            serde_json::json!({
                "project": project.map(|project| project.name.clone()),
                "scope": if chat_context.is_empty() { "global" } else { "project_context" },
                "context": context_metadata.clone(),
            }),
        )
        .await?;

    let chat_result = if chat_context.has_suggestion() {
        let suggested_label = chat_context.suggested_label();
        LibrarianChatResult {
            reply: format!(
                "Похоже, этот диалог относится к контексту `{suggested_label}`. Переключить текущий контекст?"
            ),
            memory_hits: Vec::new(),
            mode: "context-switch-proposal",
            iterations: 0,
            trace: Vec::new(),
            ui: Some(serde_json::json!({
                "type": "context_switch",
                "label": suggested_label,
                "context": {
                    "source": "suggested",
                    "label": suggested_label,
                    "nodes": chat_context.suggested_nodes.iter().map(library_context_metadata).collect::<Vec<_>>(),
                }
            })),
        }
    } else if let Some(result) =
        execute_slash_command(&state, &config, &chat_context, project, &gated.content).await?
    {
        result
    } else {
        let initial_context_pack =
            retrieve_chat_context_pack(&state.db, &config, &gated.content, &chat_context).await?;
        chat::run_librarian_chat_loop(
            &state.db,
            &config,
            &gated.content,
            project,
            &previous_turns,
            initial_context_pack,
        )
        .await?
    };
    let reply = chat_result.reply;
    let assistant_memory = state
        .db
        .add_memory_item(
            project_id,
            None,
            MemoryKind::AssistantMessage,
            Some("librarian-chat"),
            &reply,
            Some("admin:librarian-chat"),
            serde_json::json!({
                "project": project.map(|project| project.name.clone()),
                "scope": if chat_context.is_empty() { "global" } else { "project_context" },
                "context": context_metadata.clone(),
                "chat_session_id": chat_session.id,
                "memory_role": "raw_chat_turn",
                "durability": "transcript",
                "mode": chat_result.mode,
                "iterations": chat_result.iterations,
                "trace": chat_result.trace.clone(),
                "ui": chat_result.ui.clone(),
            }),
        )
        .await?;
    memory::embed_item(&state.db, &config, &assistant_memory).await?;
    state
        .db
        .add_chat_turn(
            chat_session.id,
            "assistant",
            &reply,
            Some(assistant_memory.id),
            serde_json::json!({
                "project": project.map(|project| project.name.clone()),
                "scope": if chat_context.is_empty() { "global" } else { "project_context" },
                "context": context_metadata.clone(),
                "mode": chat_result.mode,
                "iterations": chat_result.iterations,
                "ui": chat_result.ui.clone(),
            }),
        )
        .await?;

    let elapsed_ms = started_at.elapsed().as_millis();
    state
        .db
        .add_system_event(
            "chat_request",
            serde_json::json!({
                "session_id": chat_session.id,
                "project": project.map(|project| project.name.clone()),
                "context": context_metadata.clone(),
                "mode": chat_result.mode,
                "iterations": chat_result.iterations,
                "memory_hits": chat_result.memory_hits.len(),
                "elapsed_ms": elapsed_ms,
                "message_chars": gated.content.chars().count(),
                "reply_chars": reply.chars().count(),
            }),
        )
        .await?;

    Ok(Json(serde_json::json!({
        "session_id": chat_session.id,
        "reply": reply,
        "project": project.map(|project| project.name.clone()),
        "context": context_metadata,
        "context_label": context_label,
        "memory_hits": chat_result.memory_hits.clone(),
        "mode": chat_result.mode,
        "iterations": chat_result.iterations,
        "ui": chat_result.ui.clone(),
    })))
}

fn chat_session_title(message: &str) -> &str {
    message.lines().next().unwrap_or("New chat").trim()
}

async fn create_job(
    State(state): State<AppState>,
    Json(input): Json<CreateJobRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let project = state.db.get_project_by_name_or_id(&input.project).await?;
    let mount_mode = if input.read_only.unwrap_or(false) {
        MountMode::ReadOnly
    } else {
        MountMode::ReadWrite
    };
    let provider = router::parse_provider_kind(input.provider.as_deref().unwrap_or("codex"))?;
    let network_mode = router::default_network_mode_for_provider(
        &provider,
        input.allow_network.unwrap_or(false),
        input.secret_grant_token.is_some(),
    );
    let config = state.config.read().await.clone();
    let gated = gates::process_user_prompt(&state.db, &config, &input.goal, "admin-chat").await?;
    let user_memory = state
        .db
        .add_memory_item(
            Some(project.id),
            None,
            MemoryKind::UserMessage,
            Some("admin-chat"),
            &gated.content,
            Some("admin:chat"),
            serde_json::json!({ "project": project.name.clone() }),
        )
        .await?;
    memory::embed_item(&state.db, &config, &user_memory).await?;
    let context_pack = memory::retrieve_context_with_config(
        &state.db,
        Some(&config),
        memory::RetrievalRequest {
            query: gated.content.clone(),
            project_id: Some(project.id),
            activity_id: None,
            limit: memory::default_hit_limit(),
        },
    )
    .await?;
    let job = state
        .db
        .create_job(
            project.id,
            provider,
            &gated.content,
            mount_mode,
            network_mode,
            input.secret_grant_token.as_deref(),
        )
        .await?;
    state
        .db
        .add_job_event(
            job.id,
            "context_pack",
            serde_json::json!({
                "query": context_pack.query,
                "generated_at": context_pack.generated_at,
                "hits": context_pack.hits,
            }),
        )
        .await?;
    if !gated.events.is_empty() {
        state
            .db
            .add_job_event(
                job.id,
                "gate_events",
                serde_json::json!({ "events": gated.events }),
            )
            .await?;
    }
    Ok(Json(job))
}

async fn execute_slash_command(
    state: &AppState,
    config: &Config,
    chat_context: &ChatProjectContext,
    project: Option<&Project>,
    message: &str,
) -> Result<Option<LibrarianChatResult>> {
    let Some(command_line) = message.trim().strip_prefix('/') else {
        return Ok(None);
    };
    let args = split_slash_args(command_line)?;
    if args.is_empty() {
        return Ok(Some(slash_reply(
            "Available commands: /context, /lib, /work, /mem, /settings, /remember, /help",
            serde_json::json!({ "command": "empty" }),
        )));
    }

    let command = args[0].to_ascii_lowercase();
    let result = if command == "lib" {
        execute_library_slash_command(&state.db, config, &args[1..]).await?
    } else if matches!(command.as_str(), "work" | "workspace") {
        execute_workspace_slash_command(&state.db, config, &args[1..]).await?
    } else if matches!(command.as_str(), "mem" | "memory") {
        execute_memory_slash_command(&state.db, config, project, &args[1..]).await?
    } else if matches!(command.as_str(), "settings" | "config") {
        execute_settings_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "context" | "ctx") {
        execute_context_slash_command(state, config, chat_context, &args[1..]).await?
    } else if matches!(command.as_str(), "agent" | "agents" | "job" | "jobs") {
        execute_agent_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "project" | "projects") {
        execute_project_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "approval" | "approvals") {
        execute_approval_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "prompt" | "prompts") {
        execute_prompt_slash_command(state, config, &args[1..]).await?
    } else if command == "remember" {
        let mut memory_args = vec!["remember".to_string(), "fact".to_string()];
        memory_args.extend(args.iter().skip(1).cloned());
        execute_memory_slash_command(&state.db, config, project, &memory_args).await?
    } else {
        match command.as_str() {
            "help" => slash_reply(
                slash_help(),
                serde_json::json!({ "command": command }),
            ),
            "library" => {
                execute_library_slash_command(&state.db, config, &["tree".to_string()]).await?
            }
            _ => slash_reply(
                "Unknown slash command. Try /help. Context commands live under /context; library commands live under /lib; working-folder commands live under /work; memory commands live under /mem; project commands live under /project; approvals live under /approval; prompt blocks live under /prompt; settings commands live under /settings; background agent jobs live under /agent.",
                serde_json::json!({ "command": command, "status": "unknown" }),
            ),
        }
    };

    Ok(Some(result))
}

async fn execute_library_slash_command(
    db: &Database,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            library_slash_help(),
            serde_json::json!({ "command": "lib" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            library_slash_help(),
            serde_json::json!({ "tool": "library", "command": command }),
        ),
        "tree" => {
            ensure_tool_permission(
                db,
                config,
                "library.read",
                config.tool_permissions.library_read,
            )
            .await?;
            let depth = args
                .get(1)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid depth: {error}"))?
                .unwrap_or(4);
            let roots = vec![library_tools::tree(config, LibraryRoot::Library, depth)?];
            slash_reply(
                &format!("Library tree loaded: {} root(s).", roots.len()),
                serde_json::json!({ "tool": "library", "command": command, "roots": roots }),
            )
        }
        "mkdir" => {
            let path = slash_single_path_arg(&args, "/lib mkdir <path>")?;
            let root = LibraryRoot::Library;
            ensure_tool_permission(
                db,
                config,
                "library.create",
                config.tool_permissions.library_create,
            )
            .await?;
            let tool_path = library_tools::create_folder(config, root, path)?;
            log_slash_library_event(
                db,
                "create_folder",
                serde_json::json!({ "root": root, "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Created folder in {:?}: {}", root, tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": root, "path": tool_path.path }),
            )
        }
        "touch" => {
            let path = slash_single_path_arg(&args, "/lib touch <path>")?;
            let root = LibraryRoot::Library;
            ensure_tool_permission(
                db,
                config,
                "library.create",
                config.tool_permissions.library_create,
            )
            .await?;
            let tool_path = library_tools::create_empty_file(config, root, path)?;
            log_slash_library_event(
                db,
                "create_empty_file",
                serde_json::json!({ "root": root, "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Created empty file in {:?}: {}", root, tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": root, "path": tool_path.path }),
            )
        }
        "read" => {
            ensure_tool_permission(
                db,
                config,
                "library.read",
                config.tool_permissions.library_read,
            )
            .await?;
            let path = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: /read <library-md-path>"))?;
            let content = if args.len() >= 4 {
                let start = parse_line_number(&args[2])?;
                let end = parse_line_number(&args[3])?;
                library_tools::read_markdown_lines(config, path, start, end)?.content
            } else {
                library_tools::read_markdown(config, path)?
            };
            slash_reply(
                &format!("Read `{path}`:\n\n{content}"),
                serde_json::json!({ "tool": "library", "command": command, "root": "library", "path": path }),
            )
        }
        "write" | "write-overwrite" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("Usage: /lib write-overwrite <library-md-path> <content>")
            })?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib write-overwrite <library-md-path> <content>");
            }
            let content = args[2..].join(" ");
            let tool_path = library_tools::write_markdown(config, path, &content)?;
            log_slash_library_event(
                db,
                "write_markdown",
                serde_json::json!({ "root": "library", "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Overwrote Markdown note: {}", tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": "library", "path": tool_path.path }),
            )
        }
        "append" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let path = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: /lib append <library-md-path> <content>"))?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib append <library-md-path> <content>");
            }
            let content = args[2..].join(" ");
            let tool_path = library_tools::append_markdown(config, path, &content)?;
            log_slash_library_event(
                db,
                "append_markdown",
                serde_json::json!({ "root": "library", "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Appended to Markdown note: {}", tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": "library", "path": tool_path.path }),
            )
        }
        "read-lines" => {
            ensure_tool_permission(
                db,
                config,
                "library.read",
                config.tool_permissions.library_read,
            )
            .await?;
            let path = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("Usage: /lib read-lines <library-md-path> <start> <end>")
            })?;
            if args.len() < 4 {
                anyhow::bail!("Usage: /lib read-lines <library-md-path> <start> <end>");
            }
            let slice = library_tools::read_markdown_lines(
                config,
                path,
                parse_line_number(&args[2])?,
                parse_line_number(&args[3])?,
            )?;
            slash_reply(
                &format!(
                    "Read `{}` lines {}-{} of {}:\n\n{}",
                    slice.path, slice.start_line, slice.end_line, slice.total_lines, slice.content
                ),
                serde_json::json!({ "tool": "library", "command": command, "slice": slice }),
            )
        }
        "cut-lines" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            let edit = slash_line_edit(
                config,
                &args,
                None,
                "/lib cut-lines <library-md-path> <start> <end>",
            )?;
            log_slash_library_event(
                db,
                "cut_markdown_lines",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Cut `{}` lines {}-{}:\n\n{}",
                    edit.path, edit.start_line, edit.end_line, edit.removed
                ),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "replace-lines" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            if args.len() < 5 {
                anyhow::bail!(
                    "Usage: /lib replace-lines <library-md-path> <start> <end> <content>"
                );
            }
            let replacement = args[4..].join(" ");
            let edit = slash_line_edit(
                config,
                &args[..4],
                Some(&replacement),
                "/lib replace-lines <library-md-path> <start> <end> <content>",
            )?;
            log_slash_library_event(
                db,
                "replace_markdown_lines",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Replaced `{}` lines {}-{}.",
                    edit.path, edit.start_line, edit.end_line
                ),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "find" => {
            ensure_tool_permission(
                db,
                config,
                "library.read",
                config.tool_permissions.library_read,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib find <library-md-path> <query> [limit]");
            }
            let limit = args
                .get(3)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid limit: {error}"))?
                .unwrap_or(10);
            let matches = library_tools::find_markdown(config, &args[1], &args[2], limit)?;
            let mut reply = format!("Found {} match(es) in `{}`.", matches.len(), args[1]);
            for item in &matches {
                reply.push_str(&format!("\n{}: {}", item.line_number, item.line));
            }
            slash_reply(
                &reply,
                serde_json::json!({ "tool": "library", "command": command, "matches": matches }),
            )
        }
        "cut-find" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib cut-find <library-md-path> <query>");
            }
            let edit = library_tools::cut_first_markdown_match(config, &args[1], &args[2])?;
            log_slash_library_event(
                db,
                "cut_markdown_match",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Cut first match in `{}` at line {}:\n\n{}",
                    edit.path, edit.start_line, edit.removed
                ),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "replace-find" => {
            ensure_tool_permission(
                db,
                config,
                "library.edit_markdown",
                config.tool_permissions.library_edit_markdown,
            )
            .await?;
            if args.len() < 4 {
                anyhow::bail!("Usage: /lib replace-find <library-md-path> <query> <content>");
            }
            let replacement = args[3..].join(" ");
            let edit = library_tools::replace_first_markdown_match(
                config,
                &args[1],
                &args[2],
                &replacement,
            )?;
            log_slash_library_event(
                db,
                "replace_markdown_match",
                serde_json::json!({ "root": "library", "path": edit.path, "start_line": edit.start_line, "end_line": edit.end_line }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Replaced first match in `{}` at line {}.",
                    edit.path, edit.start_line
                ),
                serde_json::json!({ "tool": "library", "command": command, "edit": edit }),
            )
        }
        "move" | "rename" => {
            ensure_tool_permission(
                db,
                config,
                "library.move",
                config.tool_permissions.library_move,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /lib move <from> <to>");
            }
            let root = LibraryRoot::Library;
            let tool_path = library_tools::move_path(config, root, &args[1], &args[2])?;
            log_slash_library_event(
                db,
                "move",
                serde_json::json!({ "root": root, "from": args[1], "to": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Moved {:?} item to: {}", root, tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": root, "from": args[1], "to": tool_path.path }),
            )
        }
        "delete" => {
            ensure_tool_permission(
                db,
                config,
                "library.delete",
                config.tool_permissions.library_delete,
            )
            .await?;
            if args.len() < 3 || !args.iter().any(|arg| arg == "--yes") {
                return Ok(slash_reply(
                    "Delete is destructive. Use: /lib delete <path> --yes [--recursive]",
                    serde_json::json!({ "tool": "library", "command": command, "status": "needs_explicit_confirmation" }),
                ));
            }
            let root = LibraryRoot::Library;
            let recursive = args.iter().any(|arg| arg == "--recursive");
            let tool_path = library_tools::delete_path(config, root, &args[1], recursive)?;
            log_slash_library_event(
                db,
                "delete",
                serde_json::json!({ "root": root, "path": tool_path.path, "recursive": recursive }),
            )
            .await?;
            slash_reply(
                &format!("Deleted {:?} item: {}", root, tool_path.path),
                serde_json::json!({ "tool": "library", "command": command, "root": root, "path": tool_path.path, "recursive": recursive }),
            )
        }
        _ => slash_reply(
            "Unknown library command. Try /lib help.",
            serde_json::json!({ "tool": "library", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

fn slash_reply(reply: &str, trace: serde_json::Value) -> LibrarianChatResult {
    LibrarianChatResult {
        reply: reply.to_string(),
        iterations: 0,
        memory_hits: Vec::new(),
        trace: vec![trace],
        mode: "slash-command",
        ui: None,
    }
}

fn slash_help() -> &'static str {
    "Available command groups:\n/context help - show or change the active chat context\n/lib help - Markdown library and library hierarchy tools\n/work help - default working-folder tools under Projects\n/project help - library project and workspace attachment tools\n/mem help - durable memory tools\n/approval help - pending tool approval proposals\n/prompt help - prompt builder block presets\n/settings help - inspect and change guarded settings\n/agent help - explicit background agent jobs\n\nLibrary projects live in /lib. Implementation/product working folders live in /work or attached external project records."
}

async fn execute_context_slash_command(
    state: &AppState,
    config: &Config,
    chat_context: &ChatProjectContext,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            context_slash_help(),
            serde_json::json!({ "command": "context" }),
        ));
    }
    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            context_slash_help(),
            serde_json::json!({ "tool": "context", "command": command }),
        ),
        "show" | "status" => slash_reply(
            &format!("Current context: {}", chat_context.label()),
            serde_json::json!({
                "tool": "context",
                "command": command,
                "context": chat_context.metadata(),
            }),
        ),
        "clear" => context_update_reply(
            "Context cleared. Future messages will use the global conversation until you select another context.",
            Vec::new(),
            "clear",
        ),
        "set" | "use" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: /context set <library-path|project-name|project-id>");
            }
            let nodes = resolve_context_nodes_from_args(state, config, &args[1..]).await?;
            context_update_reply(
                &format!("Context set to {}.", context_label_for_nodes(&nodes)),
                nodes,
                "set",
            )
        }
        "add" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: /context add <library-path|project-name|project-id>");
            }
            let mut nodes = chat_context.nodes.clone();
            for node in resolve_context_nodes_from_args(state, config, &args[1..]).await? {
                if !nodes.iter().any(|existing| same_context_node(existing, &node)) {
                    nodes.push(node);
                }
            }
            context_update_reply(
                &format!("Context set to {}.", context_label_for_nodes(&nodes)),
                nodes,
                "add",
            )
        }
        "remove" | "rm" => {
            if args.len() < 2 {
                anyhow::bail!("Usage: /context remove <library-path|project-name|project-id>");
            }
            let remove_nodes = resolve_context_nodes_from_args(state, config, &args[1..]).await?;
            let mut nodes = chat_context.nodes.clone();
            nodes.retain(|node| {
                !remove_nodes
                    .iter()
                    .any(|remove| same_context_node(node, remove))
            });
            context_update_reply(
                &format!("Context set to {}.", context_label_for_nodes(&nodes)),
                nodes,
                "remove",
            )
        }
        _ => slash_reply(
            "Unknown context command. Try /context help.",
            serde_json::json!({ "tool": "context", "command": command, "status": "unknown" }),
        ),
    };
    Ok(result)
}

async fn resolve_context_nodes_from_args(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<Vec<ChatLibraryContextNode>> {
    let projects = state.db.list_projects().await?;
    let mut nodes = Vec::new();
    for value in args {
        let node = resolve_library_context_node(config, &projects, value)?;
        if !nodes
            .iter()
            .any(|existing| same_context_node(existing, &node))
        {
            nodes.push(node);
        }
    }
    Ok(nodes)
}

fn context_update_reply(
    reply: &str,
    nodes: Vec<ChatLibraryContextNode>,
    action: &str,
) -> LibrarianChatResult {
    LibrarianChatResult {
        reply: reply.to_string(),
        iterations: 0,
        memory_hits: Vec::new(),
        trace: Vec::new(),
        mode: "slash-command",
        ui: Some(serde_json::json!({
            "type": "context_update",
            "action": action,
            "context": {
                "source": "slash-command",
                "label": context_label_for_nodes(&nodes),
                "nodes": nodes.iter().map(library_context_metadata).collect::<Vec<_>>(),
            }
        })),
    }
}

fn context_slash_help() -> &'static str {
    "Context commands live under /context:\n/context show - show the current chat context\n/context set <library-path|project-name|project-id> - replace the context\n/context add <library-path|project-name|project-id> - add a context node\n/context remove <library-path|project-name|project-id> - remove a context node\n/context clear - return to global conversation"
}

fn library_slash_help() -> &'static str {
    "Library commands live under /lib:\n/lib tree [depth]\n/lib mkdir <path>\n/lib touch <path>\n/lib read <library-md-path> [start] [end]\n/lib read-lines <library-md-path> <start> <end>\n/lib write-overwrite <library-md-path> <content>\n/lib append <library-md-path> <content>\n/lib cut-lines <library-md-path> <start> <end>\n/lib replace-lines <library-md-path> <start> <end> <content>\n/lib find <library-md-path> <query> [limit]\n/lib cut-find <library-md-path> <query>\n/lib replace-find <library-md-path> <query> <content>\n/lib move <from> <to>\n/lib delete <path> --yes [--recursive]"
}

async fn execute_workspace_slash_command(
    db: &Database,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            workspace_slash_help(),
            serde_json::json!({ "command": "work" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let root = LibraryRoot::Projects;
    let result = match command.as_str() {
        "help" => slash_reply(
            workspace_slash_help(),
            serde_json::json!({ "tool": "workspace", "command": command }),
        ),
        "tree" => {
            ensure_tool_permission(db, config, "workspace.read", ToolPermissionPolicy::Auto)
                .await?;
            let depth = args
                .get(1)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid depth: {error}"))?
                .unwrap_or(4);
            let tree = library_tools::tree(config, root, depth)?;
            slash_reply(
                "Workspace tree loaded.",
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "tree": tree }),
            )
        }
        "mkdir" => {
            let path = slash_single_path_arg(args, "/work mkdir <path>")?;
            ensure_tool_permission(
                db,
                config,
                "workspace.create",
                config.tool_permissions.workspace_create,
            )
            .await?;
            let tool_path = library_tools::create_folder(config, root, path)?;
            log_workspace_event(
                db,
                "create_folder",
                serde_json::json!({ "root": root, "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Created workspace folder: {}", tool_path.path),
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "path": tool_path.path }),
            )
        }
        "touch" => {
            let path = slash_single_path_arg(args, "/work touch <path>")?;
            ensure_tool_permission(
                db,
                config,
                "workspace.create",
                config.tool_permissions.workspace_create,
            )
            .await?;
            let tool_path = library_tools::create_empty_file(config, root, path)?;
            log_workspace_event(
                db,
                "create_empty_file",
                serde_json::json!({ "root": root, "path": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Created workspace file: {}", tool_path.path),
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "path": tool_path.path }),
            )
        }
        "move" | "rename" => {
            ensure_tool_permission(
                db,
                config,
                "workspace.move",
                config.tool_permissions.workspace_move,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /work move <from> <to>");
            }
            let tool_path = library_tools::move_path(config, root, &args[1], &args[2])?;
            log_workspace_event(
                db,
                "move",
                serde_json::json!({ "root": root, "from": args[1], "to": tool_path.path }),
            )
            .await?;
            slash_reply(
                &format!("Moved workspace item to: {}", tool_path.path),
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "from": args[1], "to": tool_path.path }),
            )
        }
        "delete" => {
            ensure_tool_permission(
                db,
                config,
                "workspace.delete",
                config.tool_permissions.workspace_delete,
            )
            .await?;
            if args.len() < 3 || !args.iter().any(|arg| arg == "--yes") {
                return Ok(slash_reply(
                    "Delete is destructive. Use: /work delete <path> --yes [--recursive]",
                    serde_json::json!({ "tool": "workspace", "command": command, "status": "needs_explicit_confirmation" }),
                ));
            }
            let recursive = args.iter().any(|arg| arg == "--recursive");
            let tool_path = library_tools::delete_path(config, root, &args[1], recursive)?;
            log_workspace_event(
                db,
                "delete",
                serde_json::json!({ "root": root, "path": tool_path.path, "recursive": recursive }),
            )
            .await?;
            slash_reply(
                &format!("Deleted workspace item: {}", tool_path.path),
                serde_json::json!({ "tool": "workspace", "command": command, "root": root, "path": tool_path.path, "recursive": recursive }),
            )
        }
        _ => slash_reply(
            "Unknown workspace command. Try /work help.",
            serde_json::json!({ "tool": "workspace", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

fn workspace_slash_help() -> &'static str {
    "Workspace commands live under /work and operate only inside Librarian/Projects:\n/work tree [depth]\n/work mkdir <path>\n/work touch <path>\n/work move <from> <to>\n/work delete <path> --yes [--recursive]\n\nUse this for default implementation/product folders, not for library knowledge."
}

async fn execute_project_slash_command(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            project_slash_help(),
            serde_json::json!({ "command": "project" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            project_slash_help(),
            serde_json::json!({ "tool": "project", "command": command }),
        ),
        "list" => {
            let projects = state.db.list_projects().await?;
            let mut reply = format!("Projects: {} registered.", projects.len());
            for project in &projects {
                reply.push_str(&format!("\n{}", format_project_summary(project)));
            }
            slash_reply(
                &reply,
                serde_json::json!({ "tool": "project", "command": command, "projects": projects }),
            )
        }
        "status" => {
            let project = slash_project_arg(args, "/project status <project>")?;
            let project = state.db.get_project_by_name_or_id(project).await?;
            slash_reply(
                &format_project_summary(&project),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        "map" => {
            let projects = state.db.list_projects().await?;
            let map = build_project_map(config, projects)?;
            slash_reply(
                &format!(
                    "Project map loaded: {} linked project(s).",
                    map["linked_project_count"].as_u64().unwrap_or(0)
                ),
                serde_json::json!({ "tool": "project", "command": command, "map": map }),
            )
        }
        "create" => {
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
                "workspace.create",
                config.tool_permissions.workspace_create,
            )
            .await?;
            let request = parse_project_create_args(&args[1..])?;
            let library_path = request
                .library_path
                .unwrap_or_else(|| format!("projects/{}", project_folder_name(&request.name)));
            let library_path = library_tools::normalize_tool_relative_path(&library_path)?;
            library_tools::create_folder(config, LibraryRoot::Library, &library_path)?;

            let workspace_path = if let Some(path) = request.workspace_path {
                canonical_existing_dir(&path)?
            } else {
                let relative = project_folder_name(&request.name);
                library_tools::create_folder(config, LibraryRoot::Projects, &relative)?;
                config.home.join("Projects").join(relative).canonicalize()?
            };
            let project = state.db.add_project(&request.name, &workspace_path).await?;
            let project = state
                .db
                .attach_project_library_path(project.id, PathBuf::from(&library_path).as_path())
                .await?;
            log_project_event(
                &state.db,
                "create",
                serde_json::json!({
                    "project_id": project.id,
                    "name": project.name.clone(),
                    "library_path": project.library_path.clone(),
                    "workspace_path": project.path.clone(),
                }),
            )
            .await?;
            slash_reply(
                &format!("Created project.\n{}", format_project_summary(&project)),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        "attach-library" => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.move",
                config.tool_permissions.library_move,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /project attach-library <project> <library-path>");
            }
            let project = state.db.get_project_by_name_or_id(&args[1]).await?;
            let library_path = library_tools::normalize_tool_relative_path(&args[2])?;
            let project = state
                .db
                .attach_project_library_path(project.id, PathBuf::from(&library_path).as_path())
                .await?;
            log_project_event(
                &state.db,
                "attach_library",
                serde_json::json!({ "project_id": project.id, "library_path": project.library_path.clone() }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Attached library path.\n{}",
                    format_project_summary(&project)
                ),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        "detach-library" => {
            ensure_tool_permission(
                &state.db,
                config,
                "library.move",
                config.tool_permissions.library_move,
            )
            .await?;
            let project = slash_project_arg(args, "/project detach-library <project>")?;
            let project = state.db.get_project_by_name_or_id(project).await?;
            let project = state.db.detach_project_library_path(project.id).await?;
            log_project_event(
                &state.db,
                "detach_library",
                serde_json::json!({ "project_id": project.id }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Detached library path.\n{}",
                    format_project_summary(&project)
                ),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        "attach-workspace" => {
            ensure_tool_permission(
                &state.db,
                config,
                "workspace.move",
                config.tool_permissions.workspace_move,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!("Usage: /project attach-workspace <project> <existing-directory>");
            }
            let project = state.db.get_project_by_name_or_id(&args[1]).await?;
            let workspace_path = canonical_existing_dir(&args[2])?;
            let project = state
                .db
                .update_project_workspace_path(project.id, &workspace_path)
                .await?;
            log_project_event(
                &state.db,
                "attach_workspace",
                serde_json::json!({ "project_id": project.id, "workspace_path": project.path.clone() }),
            )
            .await?;
            slash_reply(
                &format!(
                    "Attached workspace path.\n{}",
                    format_project_summary(&project)
                ),
                serde_json::json!({ "tool": "project", "command": command, "project": project }),
            )
        }
        _ => slash_reply(
            "Unknown project command. Try /project help.",
            serde_json::json!({ "tool": "project", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

struct ProjectCreateSlashRequest {
    name: String,
    library_path: Option<String>,
    workspace_path: Option<String>,
}

fn parse_project_create_args(args: &[String]) -> Result<ProjectCreateSlashRequest> {
    let name = args
        .first()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Usage: /project create <name> [--library path] [--workspace existing-directory]"
            )
        })?
        .clone();
    let mut library_path = None;
    let mut workspace_path = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--library" | "--library-path" => {
                index += 1;
                library_path = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow::anyhow!("--library requires a value"))?
                        .clone(),
                );
            }
            "--workspace" | "--workspace-path" => {
                index += 1;
                workspace_path = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow::anyhow!("--workspace requires a value"))?
                        .clone(),
                );
            }
            value => anyhow::bail!("Unknown /project create flag `{value}`"),
        }
        index += 1;
    }
    Ok(ProjectCreateSlashRequest {
        name,
        library_path,
        workspace_path,
    })
}

fn slash_project_arg<'a>(args: &'a [String], usage: &str) -> Result<&'a str> {
    args.get(1)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))
}

fn canonical_existing_dir(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if !path.is_dir() {
        anyhow::bail!("Workspace path must be an existing directory");
    }
    path.canonicalize()
        .map_err(|error| anyhow::anyhow!("Failed to resolve workspace path: {error}"))
}

fn admin_shell_path(path: &std::path::Path) -> String {
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

fn project_folder_name(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if matches!(ch, '-' | '_' | '.') {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "project".to_string()
    } else {
        trimmed
    }
}

fn project_workspace_folder_name(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "Project".to_string()
    } else {
        trimmed
    }
}

fn format_project_summary(project: &Project) -> String {
    format!(
        "{} `{}` library={} workspace={}",
        project.id,
        project.name,
        project
            .library_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string()),
        project.path.display()
    )
}

async fn log_project_event(db: &Database, action: &str, payload: serde_json::Value) -> Result<()> {
    db.add_system_event(
        "project_tool",
        serde_json::json!({
            "action": action,
            "source": "slash-command",
            "payload": payload,
        }),
    )
    .await?;
    Ok(())
}

fn build_project_map(config: &Config, projects: Vec<Project>) -> Result<serde_json::Value> {
    let mut by_library_path: HashMap<String, Vec<Project>> = HashMap::new();
    let mut detached = Vec::new();
    for project in projects {
        match &project.library_path {
            Some(path) => by_library_path
                .entry(path.to_string_lossy().replace('\\', "/"))
                .or_default()
                .push(project),
            None => detached.push(project),
        }
    }
    let linked_project_count = by_library_path.values().map(Vec::len).sum::<usize>();
    let root = library_tools::tree(config, LibraryRoot::Library, 12)?;
    let tree = project_map_node(&root, &by_library_path);
    Ok(serde_json::json!({
        "root": tree,
        "linked_project_count": linked_project_count,
        "detached_projects": detached,
        "metaphor": {
            "folder_with_folders": "rack_or_row",
            "folder_with_files": "shelf",
            "markdown_file": "book",
            "file": "artifact"
        }
    }))
}

fn project_map_node(
    entry: &library_tools::LibraryEntry,
    projects: &HashMap<String, Vec<Project>>,
) -> serde_json::Value {
    let child_nodes = entry
        .children
        .iter()
        .map(|child| project_map_node(child, projects))
        .collect::<Vec<_>>();
    let linked_projects = projects.get(&entry.path).cloned().unwrap_or_default();
    serde_json::json!({
        "name": entry.name,
        "path": entry.path,
        "kind": entry.kind,
        "visual_kind": project_visual_kind(entry),
        "projects": linked_projects,
        "children": child_nodes,
    })
}

fn project_visual_kind(entry: &library_tools::LibraryEntry) -> &'static str {
    match entry.kind {
        library_tools::LibraryEntryKind::Markdown => "book",
        library_tools::LibraryEntryKind::File => "artifact",
        library_tools::LibraryEntryKind::Folder => {
            if entry
                .children
                .iter()
                .any(|child| child.kind == library_tools::LibraryEntryKind::Folder)
            {
                "rack"
            } else {
                "shelf"
            }
        }
    }
}

fn project_slash_help() -> &'static str {
    "Project commands live under /project:\n/project list\n/project map\n/project status <project>\n/project create <name> [--library path] [--workspace existing-directory]\n/project attach-library <project> <library-path>\n/project detach-library <project>\n/project attach-workspace <project> <existing-directory>\n\nA project can have a Library documentation path and one implementation/workspace directory. Default create makes Library/projects/{name} and Projects/{name}."
}

async fn execute_approval_slash_command(
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
            slash_reply(
                &format!(
                    "Created pending approval {} for {}.{}.",
                    approval.id, approval.tool, approval.action
                ),
                serde_json::json!({ "tool": "approval", "command": command, "approval": approval }),
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

async fn approve_and_execute_tool_approval(
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

async fn reject_tool_approval_by_id(
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

fn parse_json_payload(value: &str) -> Result<serde_json::Value> {
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

async fn execute_approved_tool_approval(
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
        ("project", "create_starting_docs_and_project_folder" | "create_starting_docs") => {
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
                    kind,
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
        _ => anyhow::bail!(
            "Approval executor does not allow `{}` `{}` yet",
            approval.tool,
            approval.action
        ),
    }
}

fn approval_payload_string(payload: &serde_json::Value, key: &str) -> Result<String> {
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

fn approval_project_library_path(payload: &serde_json::Value) -> Result<String> {
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

async fn execute_prompt_slash_command(
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
            let mut reply = format!("Prompt blocks: {} item(s).", blocks.len());
            for block in &blocks {
                reply.push_str(&format!(
                    "\n{} [{}] #{} {} enabled={}",
                    block.id, block.target, block.position, block.name, block.enabled
                ));
            }
            slash_reply(
                &reply,
                serde_json::json!({ "tool": "prompt", "command": command, "target": target, "blocks": blocks }),
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
        "render" => {
            let target = args
                .get(1)
                .map(String::as_str)
                .ok_or_else(|| anyhow::anyhow!("Usage: /prompt render <target>"))?;
            let blocks = state.db.list_prompt_blocks(Some(target)).await?;
            let rendered = render_prompt_blocks(&blocks);
            slash_reply(
                &format!("Rendered prompt target `{target}`:\n\n{rendered}"),
                serde_json::json!({
                    "tool": "prompt",
                    "command": command,
                    "target": target,
                    "rendered": rendered,
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

struct PromptAddBlockSlashRequest {
    target: String,
    name: String,
    content: String,
    markdown: bool,
}

fn parse_prompt_add_block_args(args: &[String]) -> Result<PromptAddBlockSlashRequest> {
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

fn slash_prompt_block_id_arg(args: &[String], usage: &str) -> Result<Uuid> {
    args.get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))?
        .parse::<Uuid>()
        .map_err(|error| anyhow::anyhow!("Invalid prompt block id: {error}"))
}

fn render_prompt_blocks(blocks: &[crate::domain::PromptBlock]) -> String {
    blocks
        .iter()
        .filter(|block| block.enabled)
        .map(|block| block.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn prompt_slash_help() -> &'static str {
    "Prompt builder commands live under /prompt:\n/prompt blocks [target]\n/prompt add-block <target> <name> <content> [--plain]\n/prompt enable <block-id>\n/prompt disable <block-id>\n/prompt render <target>\n\nTargets are flexible labels such as librarian, agents, codex, claude, or AGENTS.md. This is the data model for the future visual block editor."
}

async fn execute_memory_slash_command(
    db: &Database,
    config: &Config,
    project: Option<&Project>,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            memory_slash_help(),
            serde_json::json!({ "command": "mem" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            memory_slash_help(),
            serde_json::json!({ "tool": "memory", "command": command }),
        ),
        "remember" | "add" => {
            ensure_tool_permission(
                db,
                config,
                "memory.write",
                config.tool_permissions.memory_write,
            )
            .await?;
            if args.len() < 3 {
                anyhow::bail!(
                    "Usage: /mem remember <fact|decision|instruction|status|summary> <content>"
                );
            }
            let kind = parse_memory_kind_token(&args[1])?;
            let content = args[2..].join(" ");
            let item = db
                .add_memory_item(
                    project.map(|project| project.id),
                    None,
                    kind,
                    None,
                    &content,
                    Some("admin:slash-memory"),
                    serde_json::json!({
                        "tool": "memory",
                        "command": command,
                        "memory_role": "durable_memory",
                        "durability": "durable",
                        "scope": if project.is_some() { "project" } else { "global" },
                        "project": project.map(|project| project.name.clone()),
                    }),
                )
                .await?;
            memory::embed_item(db, config, &item).await?;
            db.add_system_event(
                "memory_tool",
                serde_json::json!({
                    "action": "remember",
                    "source": "slash-command",
                    "memory_id": item.id,
                    "kind": item.kind,
                    "scope": if project.is_some() { "project" } else { "global" },
                    "project": project.map(|project| project.name.clone()),
                }),
            )
            .await?;
            slash_reply(
                &format!("Remembered {:?}: {}", item.kind, item.content),
                serde_json::json!({
                    "tool": "memory",
                    "command": command,
                    "memory_id": item.id,
                    "kind": item.kind,
                    "scope": if project.is_some() { "project" } else { "global" },
                }),
            )
        }
        "supersede" | "contradict" => {
            ensure_tool_permission(
                db,
                config,
                "memory.write",
                config.tool_permissions.memory_write,
            )
            .await?;
            if args.len() < 4 {
                anyhow::bail!(
                    "Usage: /mem {command} <old-memory-id> <fact|decision|instruction|status|summary> <content>"
                );
            }
            let old_id = args[1]
                .parse::<Uuid>()
                .map_err(|error| anyhow::anyhow!("Invalid memory id: {error}"))?;
            let old = db.get_memory_item(old_id).await?;
            let kind = parse_memory_kind_token(&args[2])?;
            let content = args[3..].join(" ");
            let supersedes_id = (command == "supersede").then_some(old.id);
            let contradicts_id = (command == "contradict").then_some(old.id);
            let item = db
                .add_linked_memory_item(
                    project.map(|project| project.id),
                    None,
                    kind,
                    old.topic.as_deref(),
                    &content,
                    Some("admin:slash-memory"),
                    serde_json::json!({
                        "tool": "memory",
                        "command": command,
                        "memory_role": "durable_memory",
                        "durability": "durable",
                        "scope": if project.is_some() { "project" } else { "global" },
                        "project": project.map(|project| project.name.clone()),
                        "linked_memory_id": old.id,
                    }),
                    supersedes_id,
                    contradicts_id,
                )
                .await?;
            memory::embed_item(db, config, &item).await?;
            db.add_system_event(
                "memory_tool",
                serde_json::json!({
                    "action": command,
                    "source": "slash-command",
                    "memory_id": item.id,
                    "linked_memory_id": old.id,
                    "kind": item.kind,
                    "scope": if project.is_some() { "project" } else { "global" },
                    "project": project.map(|project| project.name.clone()),
                }),
            )
            .await?;
            slash_reply(
                &format!(
                    "{command} memory `{}` with {:?}: {}",
                    old.id, item.kind, item.content
                ),
                serde_json::json!({
                    "tool": "memory",
                    "command": command,
                    "memory_id": item.id,
                    "linked_memory_id": old.id,
                    "kind": item.kind,
                    "scope": if project.is_some() { "project" } else { "global" },
                }),
            )
        }
        "recent" => {
            let limit = args
                .get(1)
                .map(|value| value.parse::<i64>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid limit: {error}"))?
                .unwrap_or(10)
                .clamp(1, 50);
            let items = db
                .recent_memory_for_project(project.map(|project| project.id), limit)
                .await?;
            let items = items
                .into_iter()
                .filter(|item| is_visible_durable_memory_item(item))
                .collect::<Vec<_>>();
            let mut reply = format!("Recent memory: {} item(s).", items.len());
            for item in &items {
                reply.push_str(&format!(
                    "\n{} {} {:?}: {}",
                    item.observed_at.format("%Y-%m-%d %H:%M"),
                    item.id,
                    item.kind,
                    item.content
                ));
            }
            slash_reply(
                &reply,
                serde_json::json!({
                    "tool": "memory",
                    "command": command,
                    "items": items,
                    "scope": if project.is_some() { "project" } else { "global" },
                }),
            )
        }
        _ => slash_reply(
            "Unknown memory command. Try /mem help.",
            serde_json::json!({ "tool": "memory", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

fn is_raw_transcript_memory_item(item: &crate::domain::MemoryItem) -> bool {
    item.metadata
        .get("durability")
        .and_then(serde_json::Value::as_str)
        == Some("transcript")
        || item
            .metadata
            .get("memory_role")
            .and_then(serde_json::Value::as_str)
            == Some("raw_chat_turn")
}

fn is_visible_durable_memory_item(item: &crate::domain::MemoryItem) -> bool {
    if is_raw_transcript_memory_item(item) {
        return false;
    }
    if matches!(
        item.kind,
        MemoryKind::UserMessage | MemoryKind::AssistantMessage
    ) {
        if item
            .metadata
            .get("memory_role")
            .and_then(serde_json::Value::as_str)
            == Some("durable_memory")
        {
            return true;
        }
        if item.source.as_deref() == Some("admin:librarian-chat")
            || item.topic.as_deref() == Some("librarian-chat")
        {
            return false;
        }
    }
    true
}

fn memory_slash_help() -> &'static str {
    "Memory commands live under /mem:\n/mem remember <fact|decision|instruction|status|summary> <content>\n/mem supersede <old-memory-id> <kind> <content>\n/mem contradict <old-memory-id> <kind> <content>\n/remember <content> - shortcut for /mem remember fact <content>\n/mem recent [limit]\n\nMemory is stored in the current chat scope: selected project when present, otherwise global."
}

async fn execute_settings_slash_command(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            settings_slash_help(),
            serde_json::json!({ "command": "settings" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            settings_slash_help(),
            serde_json::json!({ "tool": "settings", "command": command }),
        ),
        "tool-permissions" | "permissions" => slash_reply(
            &format_tool_permissions(&config.tool_permissions),
            serde_json::json!({
                "tool": "settings",
                "command": command,
                "tool_permissions": config.tool_permissions,
            }),
        ),
        "set-permission-preset" | "permission-preset" | "preset" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            if args.len() < 2 {
                anyhow::bail!(
                    "Usage: /settings permission-preset <balanced|autopilot|confirm|locked_down> --yes"
                );
            }
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(slash_reply(
                    "Settings changes require explicit confirmation. Use: /settings permission-preset <balanced|autopilot|confirm|locked_down> --yes",
                    serde_json::json!({
                        "tool": "settings",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                    }),
                ));
            }
            let preset = parse_tool_permission_preset(&args[1])?;
            let config_path = {
                let mut writable_config = state.config.write().await;
                apply_tool_permission_preset(&mut writable_config.tool_permissions, preset);
                writable_config.save()?;
                writable_config.config_path.clone()
            };
            state
                .db
                .add_system_event(
                    "settings_tool",
                    serde_json::json!({
                        "action": "set_tool_permission_preset",
                        "source": "slash-command",
                        "preset": preset,
                        "config_path": config_path,
                    }),
                )
                .await?;
            slash_reply(
                &format!(
                    "Updated tool permissions preset to `{}`.",
                    preset_label(preset)
                ),
                serde_json::json!({
                    "tool": "settings",
                    "command": command,
                    "preset": preset,
                }),
            )
        }
        "set-tool-permission" | "set-permission" => {
            ensure_tool_permission(
                &state.db,
                config,
                "settings.change",
                config.tool_permissions.settings_change,
            )
            .await?;
            if args.len() < 4 {
                anyhow::bail!("Usage: /settings set-tool-permission <key> <auto|ask|deny> --yes");
            }
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(slash_reply(
                    "Settings changes require explicit confirmation. Use: /settings set-tool-permission <key> <auto|ask|deny> --yes",
                    serde_json::json!({
                        "tool": "settings",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                    }),
                ));
            }
            let key = args[1].as_str();
            let policy = parse_tool_permission_policy(&args[2])?;
            let config_path = {
                let mut writable_config = state.config.write().await;
                set_tool_permission(&mut writable_config.tool_permissions, key, policy)?;
                writable_config.save()?;
                writable_config.config_path.clone()
            };
            state
                .db
                .add_system_event(
                    "settings_tool",
                    serde_json::json!({
                        "action": "set_tool_permission",
                        "source": "slash-command",
                        "key": key,
                        "policy": policy,
                        "config_path": config_path,
                    }),
                )
                .await?;
            slash_reply(
                &format!(
                    "Updated tool permission `{key}` to `{}`.",
                    policy_label(policy)
                ),
                serde_json::json!({
                    "tool": "settings",
                    "command": command,
                    "key": key,
                    "policy": policy,
                }),
            )
        }
        _ => slash_reply(
            "Unknown settings command. Try /settings help.",
            serde_json::json!({ "tool": "settings", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

fn settings_slash_help() -> &'static str {
    "Settings commands live under /settings:\n/settings tool-permissions - show current tool permission policies\n/settings permission-preset <balanced|autopilot|confirm|locked_down> --yes - apply a whole permission package\n/settings set-tool-permission <key> <auto|ask|deny> --yes - update one permission and mark the package custom\n\nPermission keys: library_read, library_create, library_edit_markdown, library_move, library_delete, workspace_create, workspace_move, workspace_delete, memory_write, settings_change, agent_launch, context_switch."
}

fn format_tool_permissions(permissions: &ToolPermissionsConfig) -> String {
    format!(
        "Tool permissions:\n\
preset = {}\n\
library_read = {}\n\
library_create = {}\n\
library_edit_markdown = {}\n\
library_move = {}\n\
library_delete = {}\n\
workspace_create = {}\n\
workspace_move = {}\n\
workspace_delete = {}\n\
memory_write = {}\n\
settings_change = {}\n\
agent_launch = {}\n\
context_switch = {}",
        preset_label(permissions.preset),
        policy_label(permissions.library_read),
        policy_label(permissions.library_create),
        policy_label(permissions.library_edit_markdown),
        policy_label(permissions.library_move),
        policy_label(permissions.library_delete),
        policy_label(permissions.workspace_create),
        policy_label(permissions.workspace_move),
        policy_label(permissions.workspace_delete),
        policy_label(permissions.memory_write),
        policy_label(permissions.settings_change),
        policy_label(permissions.agent_launch),
        policy_label(permissions.context_switch),
    )
}

fn parse_tool_permission_preset(value: &str) -> Result<ToolPermissionPreset> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "balanced" => Ok(ToolPermissionPreset::Balanced),
        "autopilot" | "auto" => Ok(ToolPermissionPreset::Autopilot),
        "confirm" | "ask" => Ok(ToolPermissionPreset::Confirm),
        "locked_down" | "lockeddown" | "locked" | "deny" => Ok(ToolPermissionPreset::LockedDown),
        "custom" => Ok(ToolPermissionPreset::Custom),
        _ => anyhow::bail!(
            "Tool permission preset must be balanced, autopilot, confirm, or locked_down"
        ),
    }
}

fn parse_tool_permission_policy(value: &str) -> Result<ToolPermissionPolicy> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(ToolPermissionPolicy::Auto),
        "ask" => Ok(ToolPermissionPolicy::Ask),
        "deny" => Ok(ToolPermissionPolicy::Deny),
        _ => anyhow::bail!("Tool permission policy must be auto, ask, or deny"),
    }
}

fn set_tool_permission(
    permissions: &mut ToolPermissionsConfig,
    key: &str,
    policy: ToolPermissionPolicy,
) -> Result<()> {
    match key.trim().to_ascii_lowercase().as_str() {
        "library_read" => permissions.library_read = policy,
        "library_create" => permissions.library_create = policy,
        "library_edit_markdown" => permissions.library_edit_markdown = policy,
        "library_move" => permissions.library_move = policy,
        "library_delete" => permissions.library_delete = policy,
        "workspace_create" => permissions.workspace_create = policy,
        "workspace_move" => permissions.workspace_move = policy,
        "workspace_delete" => permissions.workspace_delete = policy,
        "memory_write" => permissions.memory_write = policy,
        "settings_change" => permissions.settings_change = policy,
        "agent_launch" => permissions.agent_launch = policy,
        "context_switch" => permissions.context_switch = policy,
        _ => anyhow::bail!("Unknown tool permission key `{key}`. Try /settings tool-permissions."),
    }
    permissions.preset = ToolPermissionPreset::Custom;
    Ok(())
}

fn apply_tool_permission_preset(
    permissions: &mut ToolPermissionsConfig,
    preset: ToolPermissionPreset,
) {
    *permissions = match preset {
        ToolPermissionPreset::Balanced => ToolPermissionsConfig::default(),
        ToolPermissionPreset::Autopilot => ToolPermissionsConfig {
            preset,
            library_read: ToolPermissionPolicy::Auto,
            library_create: ToolPermissionPolicy::Auto,
            library_edit_markdown: ToolPermissionPolicy::Auto,
            library_move: ToolPermissionPolicy::Auto,
            library_delete: ToolPermissionPolicy::Ask,
            workspace_create: ToolPermissionPolicy::Auto,
            workspace_move: ToolPermissionPolicy::Auto,
            workspace_delete: ToolPermissionPolicy::Ask,
            memory_write: ToolPermissionPolicy::Auto,
            settings_change: ToolPermissionPolicy::Ask,
            agent_launch: ToolPermissionPolicy::Auto,
            context_switch: ToolPermissionPolicy::Auto,
        },
        ToolPermissionPreset::Confirm => ToolPermissionsConfig {
            preset,
            library_read: ToolPermissionPolicy::Auto,
            library_create: ToolPermissionPolicy::Ask,
            library_edit_markdown: ToolPermissionPolicy::Ask,
            library_move: ToolPermissionPolicy::Ask,
            library_delete: ToolPermissionPolicy::Ask,
            workspace_create: ToolPermissionPolicy::Ask,
            workspace_move: ToolPermissionPolicy::Ask,
            workspace_delete: ToolPermissionPolicy::Ask,
            memory_write: ToolPermissionPolicy::Ask,
            settings_change: ToolPermissionPolicy::Ask,
            agent_launch: ToolPermissionPolicy::Ask,
            context_switch: ToolPermissionPolicy::Ask,
        },
        ToolPermissionPreset::LockedDown => ToolPermissionsConfig {
            preset,
            library_read: ToolPermissionPolicy::Auto,
            library_create: ToolPermissionPolicy::Deny,
            library_edit_markdown: ToolPermissionPolicy::Deny,
            library_move: ToolPermissionPolicy::Deny,
            library_delete: ToolPermissionPolicy::Deny,
            workspace_create: ToolPermissionPolicy::Deny,
            workspace_move: ToolPermissionPolicy::Deny,
            workspace_delete: ToolPermissionPolicy::Deny,
            memory_write: ToolPermissionPolicy::Ask,
            settings_change: ToolPermissionPolicy::Ask,
            agent_launch: ToolPermissionPolicy::Deny,
            context_switch: ToolPermissionPolicy::Deny,
        },
        ToolPermissionPreset::Custom => {
            let mut custom = permissions.clone();
            custom.preset = ToolPermissionPreset::Custom;
            custom
        }
    };
}

fn preset_label(preset: ToolPermissionPreset) -> &'static str {
    match preset {
        ToolPermissionPreset::Balanced => "balanced",
        ToolPermissionPreset::Autopilot => "autopilot",
        ToolPermissionPreset::Confirm => "confirm",
        ToolPermissionPreset::LockedDown => "locked_down",
        ToolPermissionPreset::Custom => "custom",
    }
}

fn policy_label(policy: ToolPermissionPolicy) -> &'static str {
    match policy {
        ToolPermissionPolicy::Auto => "auto",
        ToolPermissionPolicy::Ask => "ask",
        ToolPermissionPolicy::Deny => "deny",
    }
}

async fn execute_agent_slash_command(
    state: &AppState,
    config: &Config,
    args: &[String],
) -> Result<LibrarianChatResult> {
    if args.is_empty() {
        return Ok(slash_reply(
            agent_slash_help(),
            serde_json::json!({ "command": "agent" }),
        ));
    }

    let command = args[0].to_ascii_lowercase();
    let result = match command.as_str() {
        "help" => slash_reply(
            agent_slash_help(),
            serde_json::json!({ "tool": "agent", "command": command }),
        ),
        "list" => {
            let limit = args
                .get(1)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|error| anyhow::anyhow!("Invalid limit: {error}"))?
                .unwrap_or(10)
                .clamp(1, 50);
            let jobs = state.db.list_jobs().await?;
            let mut reply = format!(
                "Agent jobs: showing {} of {}.",
                jobs.len().min(limit),
                jobs.len()
            );
            for job in jobs.iter().take(limit) {
                reply.push_str(&format!("\n{}", format_job_summary(job)));
            }
            slash_reply(
                &reply,
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "jobs": jobs.into_iter().take(limit).collect::<Vec<_>>(),
                }),
            )
        }
        "status" => {
            let job_id = slash_job_id_arg(args, "/agent status <job-id>")?;
            let job = state.db.get_job(job_id).await?;
            slash_reply(
                &format_job_summary(&job),
                serde_json::json!({ "tool": "agent", "command": command, "job": job }),
            )
        }
        "events" => {
            let job_id = slash_job_id_arg(args, "/agent events <job-id>")?;
            let events = state.db.list_job_events(job_id).await?;
            let mut reply = format!("Job events: {} event(s).", events.len());
            for event in events.iter().take(30) {
                reply.push_str(&format!(
                    "\n{} {}: {}",
                    event.created_at.format("%Y-%m-%d %H:%M:%S"),
                    event.kind,
                    event.payload
                ));
            }
            slash_reply(
                &reply,
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "job_id": job_id,
                    "events": events,
                }),
            )
        }
        "preflight" => {
            let job_id = slash_job_id_arg(args, "/agent preflight <job-id>")?;
            let report = worker::preflight_job(config.clone(), state.db.clone(), job_id).await?;
            slash_reply(
                &format!(
                    "Preflight for job {job_id}:\n\n{}",
                    serde_json::to_string_pretty(&report)?
                ),
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "job_id": job_id,
                    "report": report,
                }),
            )
        }
        "launch" | "queue" => {
            ensure_tool_permission(
                &state.db,
                config,
                "agent.launch",
                config.tool_permissions.agent_launch,
            )
            .await?;
            let request = parse_agent_launch_args(&args[1..])?;
            if !request.confirmed {
                return Ok(slash_reply(
                    "Agent launch requires explicit confirmation. Use: /agent launch <project> <goal> --yes",
                    serde_json::json!({
                        "tool": "agent",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                    }),
                ));
            }
            let project = state.db.get_project_by_name_or_id(&request.project).await?;
            let network_mode = router::default_network_mode_for_provider(
                &request.provider,
                request.allow_network,
                request.secret_grant_token.is_some(),
            );
            let mount_mode = if request.read_only {
                MountMode::ReadOnly
            } else {
                MountMode::ReadWrite
            };
            let job = state
                .db
                .create_job(
                    project.id,
                    request.provider,
                    &request.goal,
                    mount_mode,
                    network_mode,
                    request.secret_grant_token.as_deref(),
                )
                .await?;
            let context_pack = memory::retrieve_context_with_config(
                &state.db,
                Some(config),
                memory::RetrievalRequest {
                    query: request.goal.clone(),
                    project_id: Some(project.id),
                    activity_id: None,
                    limit: config.chat.memory_hit_limit,
                },
            )
            .await?;
            state
                .db
                .add_job_event(
                    job.id,
                    "context_pack",
                    serde_json::json!({
                        "query": context_pack.query,
                        "generated_at": context_pack.generated_at,
                        "hits": context_pack.hits,
                    }),
                )
                .await?;
            state
                .db
                .add_job_event(
                    job.id,
                    "queued_from_chat",
                    serde_json::json!({
                        "source": "slash-command",
                        "project": project.name,
                    }),
                )
                .await?;
            slash_reply(
                &format!(
                    "Queued background agent job.\n{}\n\nRun `librarian worker --once` or keep a worker running to execute it.",
                    format_job_summary(&job)
                ),
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "job": job,
                    "project": project.name,
                }),
            )
        }
        "cancel" => {
            ensure_tool_permission(
                &state.db,
                config,
                "agent.cancel",
                config.tool_permissions.agent_launch,
            )
            .await?;
            let job_id = slash_job_id_arg(args, "/agent cancel <job-id> --yes")?;
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(slash_reply(
                    "Cancel changes job state. Use: /agent cancel <job-id> --yes",
                    serde_json::json!({
                        "tool": "agent",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                        "job_id": job_id,
                    }),
                ));
            }
            state.db.request_cancel_job(job_id).await?;
            slash_reply(
                &format!("Cancel requested for job {job_id}."),
                serde_json::json!({ "tool": "agent", "command": command, "job_id": job_id }),
            )
        }
        "retry" => {
            ensure_tool_permission(
                &state.db,
                config,
                "agent.retry",
                config.tool_permissions.agent_launch,
            )
            .await?;
            let job_id = slash_job_id_arg(args, "/agent retry <job-id> --yes")?;
            if !args.iter().any(|arg| arg == "--yes" || arg == "--approve") {
                return Ok(slash_reply(
                    "Retry creates a new queued job. Use: /agent retry <job-id> --yes",
                    serde_json::json!({
                        "tool": "agent",
                        "command": command,
                        "status": "needs_explicit_confirmation",
                        "job_id": job_id,
                    }),
                ));
            }
            let retry = state.db.retry_job(job_id).await?;
            slash_reply(
                &format!("Queued retry job.\n{}", format_job_summary(&retry)),
                serde_json::json!({
                    "tool": "agent",
                    "command": command,
                    "source_job_id": job_id,
                    "job": retry,
                }),
            )
        }
        _ => slash_reply(
            "Unknown agent command. Try /agent help.",
            serde_json::json!({ "tool": "agent", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
}

struct AgentLaunchSlashRequest {
    project: String,
    goal: String,
    provider: crate::domain::ProviderKind,
    secret_grant_token: Option<String>,
    allow_network: bool,
    read_only: bool,
    confirmed: bool,
}

fn parse_agent_launch_args(args: &[String]) -> Result<AgentLaunchSlashRequest> {
    let project = args
        .first()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Usage: /agent launch <project> <goal> [--provider codex] [--read-only] [--allow-network] [--secret-grant-token token] --yes"))?
        .clone();
    let mut provider = crate::domain::ProviderKind::Codex;
    let mut secret_grant_token = None;
    let mut allow_network = false;
    let mut read_only = false;
    let mut confirmed = false;
    let mut goal_parts = Vec::new();
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--provider" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?;
                provider = router::parse_provider_kind(value)?;
            }
            "--secret-grant-token" | "--secret" => {
                index += 1;
                secret_grant_token = Some(
                    args.get(index)
                        .ok_or_else(|| anyhow::anyhow!("--secret-grant-token requires a value"))?
                        .clone(),
                );
            }
            "--allow-network" | "--network" => allow_network = true,
            "--read-only" => read_only = true,
            "--yes" | "--approve" => confirmed = true,
            value if value.starts_with("--") => {
                anyhow::bail!("Unknown /agent launch flag `{value}`")
            }
            value => goal_parts.push(value.to_string()),
        }
        index += 1;
    }

    let goal = goal_parts.join(" ").trim().to_string();
    if goal.is_empty() {
        anyhow::bail!("Usage: /agent launch <project> <goal> [--provider codex] [--read-only] [--allow-network] [--secret-grant-token token] --yes");
    }

    Ok(AgentLaunchSlashRequest {
        project,
        goal,
        provider,
        secret_grant_token,
        allow_network,
        read_only,
        confirmed,
    })
}

fn slash_job_id_arg(args: &[String], usage: &str) -> Result<Uuid> {
    args.get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))?
        .parse::<Uuid>()
        .map_err(|error| anyhow::anyhow!("Invalid job id: {error}"))
}

fn format_job_summary(job: &crate::domain::Job) -> String {
    format!(
        "{} {:?} {:?} provider={} project={} goal={}",
        job.id,
        job.status,
        job.mount_mode,
        router::provider_name(&job.provider),
        job.project_id,
        job.goal
    )
}

fn agent_slash_help() -> &'static str {
    "Agent commands live under /agent and only run when called explicitly:\n/agent list [limit]\n/agent status <job-id>\n/agent events <job-id>\n/agent preflight <job-id>\n/agent launch <project> <goal> [--provider codex|openrouter|claude-code] [--read-only] [--allow-network] [--secret-grant-token token] --yes\n/agent cancel <job-id> --yes\n/agent retry <job-id> --yes\n\nUse /agent launch for background work. Normal chat never creates jobs."
}

fn parse_memory_kind_token(value: &str) -> Result<MemoryKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fact" => Ok(MemoryKind::Fact),
        "decision" => Ok(MemoryKind::Decision),
        "instruction" => Ok(MemoryKind::Instruction),
        "status" => Ok(MemoryKind::Status),
        "summary" => Ok(MemoryKind::Summary),
        "observation" | "run-observation" | "run_observation" => Ok(MemoryKind::RunObservation),
        _ => anyhow::bail!(
            "Memory kind must be fact, decision, instruction, status, summary, or observation"
        ),
    }
}

async fn ensure_tool_permission(
    db: &Database,
    config: &Config,
    action: &str,
    policy: ToolPermissionPolicy,
) -> Result<()> {
    let decision = match policy {
        ToolPermissionPolicy::Auto => "allowed_auto",
        ToolPermissionPolicy::Ask => "allowed_user_slash",
        ToolPermissionPolicy::Deny => "denied",
    };
    db.add_system_event(
        "tool_permission",
        serde_json::json!({
            "action": action,
            "policy": policy,
            "decision": decision,
            "source": "slash-command",
            "config_path": config.config_path,
        }),
    )
    .await?;
    if policy == ToolPermissionPolicy::Deny {
        anyhow::bail!("Tool action `{action}` is denied by tool permissions");
    }
    Ok(())
}

fn slash_single_path_arg<'a>(args: &'a [String], usage: &str) -> Result<&'a str> {
    args.get(1)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))
}

fn parse_line_number(value: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .map_err(|error| anyhow::anyhow!("Invalid line number `{value}`: {error}"))
}

fn slash_line_edit(
    config: &Config,
    args: &[String],
    replacement: Option<&str>,
    usage: &str,
) -> Result<library_tools::MarkdownEdit> {
    if args.len() < 4 {
        anyhow::bail!("Usage: {usage}");
    }
    let path = &args[1];
    let start = parse_line_number(&args[2])?;
    let end = parse_line_number(&args[3])?;
    match replacement {
        Some(replacement) => {
            library_tools::replace_markdown_lines(config, path, start, end, replacement)
        }
        None => library_tools::cut_markdown_lines(config, path, start, end),
    }
}

async fn log_slash_library_event(
    db: &Database,
    action: &str,
    payload: serde_json::Value,
) -> Result<()> {
    db.add_system_event(
        "library_tool",
        serde_json::json!({
            "action": action,
            "source": "slash-command",
            "payload": payload,
        }),
    )
    .await?;
    Ok(())
}

async fn log_workspace_event(
    db: &Database,
    action: &str,
    payload: serde_json::Value,
) -> Result<()> {
    db.add_system_event(
        "workspace_tool",
        serde_json::json!({
            "action": action,
            "source": "slash-command",
            "payload": payload,
        }),
    )
    .await?;
    Ok(())
}

fn parse_schedule_kind(value: &str) -> Result<ScheduleKind> {
    match value {
        "System" | "system" => Ok(ScheduleKind::System),
        "Reminder" | "reminder" => Ok(ScheduleKind::Reminder),
        "AgentTask" | "agent-task" | "agent_task" => Ok(ScheduleKind::AgentTask),
        _ => anyhow::bail!("Unknown schedule kind `{value}`"),
    }
}

fn schedule_payload(kind: &ScheduleKind, input: &CreateScheduleRequest) -> serde_json::Value {
    match kind {
        ScheduleKind::System => serde_json::json!({
            "task": input.message.clone().unwrap_or_else(|| "custom_system_task".to_string()),
        }),
        ScheduleKind::Reminder => serde_json::json!({
            "message": input.message.clone().unwrap_or_default(),
        }),
        ScheduleKind::AgentTask => serde_json::json!({
            "project": input.project.clone().unwrap_or_default(),
            "goal": input.goal.clone().unwrap_or_default(),
            "provider": input.provider.clone().unwrap_or_else(|| "codex".to_string()),
            "secret_grant_token": input.secret_grant_token.clone(),
            "allow_network": input.allow_network.unwrap_or(false),
            "read_only": input.read_only.unwrap_or(false),
        }),
    }
}

#[derive(Debug)]
struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(value: E) -> Self {
        Self(value.into())
    }
}

#[allow(dead_code)]
fn index_html(bind: &str, worker_concurrency: usize) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Librarian</title>
  <style>
    :root {{
      color-scheme: light dark;
      --bg: #101214;
      --panel: #181c20;
      --text: #edf1f5;
      --muted: #9aa8b6;
      --line: #303841;
      --accent: #58c4a5;
      --warn: #f0b35a;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: var(--bg);
      color: var(--text);
    }}
    header {{
      height: 56px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 20px;
      border-bottom: 1px solid var(--line);
      background: #15191d;
    }}
    main {{
      display: grid;
      grid-template-columns: minmax(280px, 360px) minmax(0, 1fr);
      min-height: calc(100vh - 56px);
    }}
    aside {{
      border-right: 1px solid var(--line);
      padding: 16px;
      overflow: auto;
    }}
    section {{
      padding: 18px;
      display: grid;
      grid-template-rows: 1fr auto;
      gap: 14px;
      min-width: 0;
    }}
    h1 {{ font-size: 18px; margin: 0; }}
    h2 {{ font-size: 13px; color: var(--muted); text-transform: uppercase; margin: 18px 0 8px; }}
    label {{ display: block; font-size: 13px; color: var(--muted); margin-bottom: 6px; }}
    input, textarea, select, button {{
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: var(--panel);
      color: var(--text);
      font: inherit;
    }}
    input {{ height: 38px; padding: 0 10px; }}
    select {{ height: 38px; padding: 0 10px; }}
    textarea {{ min-height: 104px; resize: vertical; padding: 10px; }}
    button {{
      height: 38px;
      cursor: pointer;
      background: var(--accent);
      color: #06100d;
      border-color: transparent;
      font-weight: 650;
    }}
    .row {{ display: flex; gap: 8px; align-items: center; margin: 10px 0; }}
    .row input[type="checkbox"] {{ width: 18px; height: 18px; }}
    .grid-2 {{ display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }}
    .log {{
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
      padding: 14px;
      overflow: auto;
      min-height: 260px;
      white-space: pre-wrap;
    }}
    .item {{
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 10px;
      margin-bottom: 8px;
      background: var(--panel);
    }}
    .action {{
      border-left: 3px solid var(--accent);
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
      line-height: 1.45;
    }}
    .actions {{
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 6px;
      margin-top: 8px;
    }}
    .actions button {{
      height: 32px;
      font-size: 12px;
    }}
    .mini {{
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 6px;
      margin-top: 8px;
    }}
    .pill {{
      display: inline-block;
      border: 1px solid var(--line);
      border-radius: 999px;
      padding: 2px 8px;
      margin: 2px 4px 2px 0;
      color: var(--muted);
      font-size: 12px;
    }}
    details {{
      margin-top: 8px;
    }}
    summary {{
      cursor: pointer;
      color: var(--accent);
    }}
    pre {{
      overflow: auto;
      white-space: pre-wrap;
      margin: 8px 0 0;
      color: var(--muted);
    }}
    .secondary {{
      background: #27313a;
      color: var(--text);
      border-color: var(--line);
    }}
    .danger {{
      background: #8e4f4f;
      color: #fff;
    }}
    .muted {{ color: var(--muted); }}
    @media (max-width: 820px) {{
      main {{ grid-template-columns: 1fr; }}
      aside {{ border-right: 0; border-bottom: 1px solid var(--line); }}
    }}
  </style>
</head>
<body>
  <header>
    <h1>Librarian</h1>
    <span class="muted">localhost {bind}</span>
  </header>
  <main>
    <aside>
      <h2>Worker</h2>
      <div id="worker" class="item">Loading...</div>
      <h2>Memory</h2>
      <div id="memory" class="item">Loading...</div>
      <h2>Providers</h2>
      <div id="providers" class="muted">Loading...</div>
      <h2>Usage</h2>
      <div id="usage" class="muted">Loading...</div>
      <h2>Third Eye</h2>
      <div id="third-eye" class="item">Loading...</div>
      <h2>Secrets</h2>
      <div id="secrets" class="muted">Loading...</div>
      <div id="secret-grants" class="muted">Loading...</div>
      <form id="secret-form" class="item">
        <label for="secret_name">Secret name</label>
        <input id="secret_name" autocomplete="off" placeholder="openrouter.default">
        <label for="secret_provider">Provider</label>
        <input id="secret_provider" autocomplete="off" placeholder="openrouter">
        <label for="secret_kind">Kind</label>
        <input id="secret_kind" autocomplete="off" value="api-key">
        <label for="secret_value">Value</label>
        <input id="secret_value" type="password" autocomplete="off">
        <button type="submit">Store Secret</button>
      </form>
      <form id="grant-form" class="item">
        <label for="grant_secret">Secret name or id</label>
        <input id="grant_secret" autocomplete="off" placeholder="openrouter.default">
        <label for="grant_provider">Provider</label>
        <input id="grant_provider" autocomplete="off" placeholder="openrouter">
        <div class="grid-2">
          <div>
            <label for="grant_capability">Capability</label>
            <input id="grant_capability" autocomplete="off" value="provider-proxy">
          </div>
          <div>
            <label for="grant_ttl">TTL seconds</label>
            <input id="grant_ttl" type="number" min="1" value="900">
          </div>
        </div>
        <label for="grant_max_uses">Max uses</label>
        <input id="grant_max_uses" type="number" min="1" value="1">
        <button type="submit">Create Grant</button>
      </form>
      <form id="worker-form" class="item">
        <label for="worker_concurrency">Max concurrent jobs</label>
        <div class="grid-2">
          <input id="worker_concurrency" type="number" min="1" value="{worker_concurrency}">
          <button type="submit">Save</button>
        </div>
      </form>
      <form id="routing-form" class="item">
        <div class="row">
          <input id="fallback_enabled" type="checkbox">
          <label for="fallback_enabled">Use fallback provider when paused</label>
        </div>
        <label for="fallback_order">Fallback order</label>
        <input id="fallback_order" autocomplete="off" value="codex, openrouter, claude-code">
        <button type="submit">Save Routing</button>
      </form>
      <form id="budget-form" class="item">
        <div class="row">
          <input id="budget_enabled" type="checkbox">
          <label for="budget_enabled">Enforce daily budget guardrails</label>
        </div>
        <div class="grid-2">
          <div>
            <label for="budget_total">Total USD/day</label>
            <input id="budget_total" type="number" min="0" step="0.01">
          </div>
          <div>
            <label for="budget_provider">Provider USD/day</label>
            <input id="budget_provider" type="number" min="0" step="0.01">
          </div>
        </div>
        <label for="budget_project">Project USD/day</label>
        <input id="budget_project" type="number" min="0" step="0.01">
        <button type="submit">Save Budget</button>
      </form>
      <h2>Projects</h2>
      <div id="projects" class="muted">Loading...</div>
      <h2>Jobs</h2>
      <div id="jobs" class="muted">Loading...</div>
      <h2>Schedules</h2>
      <div id="schedules" class="muted">Loading...</div>
      <form id="schedule-form" class="item">
        <label for="schedule_name">Schedule name</label>
        <input id="schedule_name" autocomplete="off" placeholder="daily.status">
        <div class="grid-2">
          <div>
            <label for="schedule_kind">Kind</label>
            <select id="schedule_kind">
              <option value="reminder">Reminder</option>
              <option value="agent-task">Agent task</option>
            </select>
          </div>
          <div>
            <label for="schedule_every">Every seconds</label>
            <input id="schedule_every" type="number" min="1" value="3600">
          </div>
        </div>
        <label for="schedule_message">Message</label>
        <input id="schedule_message" autocomplete="off">
        <label for="schedule_project">Project</label>
        <input id="schedule_project" autocomplete="off">
        <label for="schedule_provider">Provider</label>
        <select id="schedule_provider">
          <option value="codex">Codex</option>
          <option value="openrouter">OpenRouter</option>
          <option value="claude-code">Claude Code</option>
        </select>
        <label for="schedule_secret_grant_token">Secret grant token</label>
        <input id="schedule_secret_grant_token" autocomplete="off">
        <label for="schedule_goal">Agent goal</label>
        <textarea id="schedule_goal"></textarea>
        <div class="row">
          <input id="schedule_network" type="checkbox">
          <label for="schedule_network">Allow network</label>
        </div>
        <div class="grid-2">
          <button type="submit">Save Schedule</button>
          <button type="button" class="secondary" onclick="resetScheduleForm()">Clear</button>
        </div>
      </form>
      <h2>Recent Actions</h2>
      <div id="system-events" class="muted">Loading...</div>
      <h2>Settings</h2>
      <div class="item">
        <div>Provider: Codex</div>
        <div class="muted">Network is disabled by default.</div>
        <div class="muted">Worker concurrency: {worker_concurrency}</div>
        <div class="muted">Auth bootstrap: run <code>librarian auth codex</code>.</div>
      </div>
    </aside>
    <section>
      <div class="log" id="output">Ready.</div>
      <form id="chat">
        <label for="project">Project name or id</label>
        <input id="project" name="project" autocomplete="off">
        <label for="provider">Provider</label>
        <select id="provider" name="provider">
          <option value="codex">Codex</option>
          <option value="openrouter">OpenRouter</option>
          <option value="claude-code">Claude Code</option>
        </select>
        <label for="secret_grant_token">Secret grant token</label>
        <input id="secret_grant_token" name="secret_grant_token" autocomplete="off">
        <label for="goal">Goal</label>
        <textarea id="goal" name="goal"></textarea>
        <div class="row">
          <input id="allow_network" name="allow_network" type="checkbox">
          <label for="allow_network">Allow network for this session</label>
        </div>
        <button type="submit">Queue Agent Job</button>
      </form>
    </section>
  </main>
  <script>
    function escapeHtml(value) {{
      return String(value ?? '').replace(/[&<>"']/g, character => ({{
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;'
      }}[character]));
    }}
    function asJson(value) {{
      return escapeHtml(JSON.stringify(value, null, 2));
    }}
    function shortId(value) {{
      return String(value || '').slice(0, 8);
    }}
    async function load() {{
      const [health, projects, jobs, schedules, systemEvents, providers, usage, thirdEye, secrets, grants] = await Promise.all([
        fetch('/api/health').then(r => r.json()),
        fetch('/api/projects').then(r => r.json()),
        fetch('/api/jobs').then(r => r.json()),
        fetch('/api/schedules').then(r => r.json()),
        fetch('/api/system-events').then(r => r.json()),
        fetch('/api/providers').then(r => r.json()),
        fetch('/api/usage').then(r => r.json()),
        fetch('/api/third-eye').then(r => r.json()),
        fetch('/api/secrets').then(r => r.json()),
        fetch('/api/secrets/grants').then(r => r.json())
      ]);
      document.querySelector('#worker').innerHTML = `
        <b>${{health.worker.running_jobs}} / ${{health.worker.max_concurrent_jobs}}</b> slots used<br>
        <span class="muted">Queued: ${{health.worker.queued_jobs}} · Available: ${{health.worker.available_slots}}<br>Runtime: ${{health.container_runtime}}</span>
      `;
      document.querySelector('#memory').innerHTML = `
        <b>${{health.memory.embedded_items}} / ${{health.memory.items}}</b> embedded<br>
        <span class="muted">${{escapeHtml(health.memory.embedding_backend)}} &middot; ${{escapeHtml(health.memory.embedding_model)}} &middot; ${{health.memory.embedding_dimensions}}d<br>Missing: ${{health.memory.missing_embeddings}}</span>
      `;
      const stateByKey = new Map((providers.states || []).map(state => [`${{state.provider}}:${{state.model || ''}}`, state]));
      document.querySelector('#providers').innerHTML = providers.catalog.length
        ? providers.catalog.map(model => {{
            const state = stateByKey.get(`${{model.provider}}:${{model.model}}`) || stateByKey.get(`${{model.provider}}:`) || {{}};
            const paused = state.status === 'Paused';
            return `<div class="item">
              <b>${{escapeHtml(model.provider)}}</b><br>
              <span class="muted">${{escapeHtml(model.model)}} &middot; ${{escapeHtml(state.status || 'Ready')}}</span><br>
              ${{(model.task_hints || []).map(hint => `<span class="pill">${{escapeHtml(hint)}}</span>`).join('')}}
              ${{paused ? `<br><span class="muted">Paused until ${{escapeHtml(state.paused_until || '-')}}<br>${{escapeHtml(state.reason || '')}}</span>` : ''}}
              <div class="mini">
                <button type="button" class="secondary" onclick="pauseProvider('${{escapeHtml(model.provider)}}', '${{escapeHtml(model.model)}}')">Pause 30m</button>
                <button type="button" onclick="resumeProvider('${{escapeHtml(model.provider)}}', '${{escapeHtml(model.model)}}')">Resume</button>
              </div>
            </div>`;
          }}).join('')
        : 'No providers.';
      document.querySelector('#usage').innerHTML = usage.length
        ? usage.slice(0, 8).map(event => `<div class="item action">
            <b>${{escapeHtml(event.provider)}}</b> <span class="muted">${{escapeHtml(event.model || '-')}}</span><br>
            <span class="muted">${{escapeHtml(event.observed_at)}} &middot; job ${{escapeHtml(shortId(event.job_id) || '-')}}</span><br>
            input=${{event.input_tokens ?? '-'}} output=${{event.output_tokens ?? '-'}} cost=${{event.cost_usd ?? '-'}} limit=${{event.limit_event}}
          </div>`).join('')
        : 'No usage observations.';
      document.querySelector('#third-eye').innerHTML = `
        <b>${{thirdEye.enabled ? 'Enabled' : 'Disabled'}}</b><br>
        <span class="muted">${{escapeHtml(thirdEye.base_url)}}<br>
        API: ${{thirdEye.health.reachable ? 'reachable' : 'offline'}} / ${{thirdEye.health.api_ok ? 'ok' : 'not ok'}}<br>
        DB: ${{thirdEye.db_summary ? `${{thirdEye.db_summary.api_calls}} calls, $${{Number(thirdEye.db_summary.total_cost_usd || 0).toFixed(4)}}` : 'not configured'}}</span>
      `;
      document.querySelector('#secrets').innerHTML = secrets.length
        ? secrets.slice(0, 8).map(secret => `<div class="item">
            <b>${{escapeHtml(secret.name)}}</b><br>
            <span class="muted">${{escapeHtml(secret.provider)}} &middot; ${{escapeHtml(secret.kind)}} &middot; ${{escapeHtml(secret.encryption)}}<br>${{escapeHtml(secret.updated_at)}}</span>
          </div>`).join('')
        : 'No secrets stored.';
      document.querySelector('#secret-grants').innerHTML = grants.length
        ? grants.slice(0, 6).map(grant => `<div class="item">
            <b>${{escapeHtml(shortId(grant.id))}}</b> <span class="muted">${{escapeHtml(grant.provider || '-')}}</span><br>
            <span class="muted">capability=${{escapeHtml(grant.capability)}} uses=${{grant.uses}}/${{grant.max_uses}} expires=${{escapeHtml(grant.expires_at)}}</span>
          </div>`).join('')
        : 'No active grants listed.';
      worker_concurrency.value = health.worker.max_concurrent_jobs;
      fallback_enabled.checked = Boolean(health.routing.fallback_enabled);
      fallback_order.value = (health.routing.fallback_order || []).join(', ');
      budget_enabled.checked = Boolean(health.budget.enabled);
      budget_total.value = health.budget.daily_total_usd ?? '';
      budget_provider.value = health.budget.daily_provider_usd ?? '';
      budget_project.value = health.budget.daily_project_usd ?? '';
      wireGrantTokenHints(grants);
      document.querySelector('#projects').innerHTML = projects.length
        ? projects.map(p => `<div class="item"><b>${{escapeHtml(p.name)}}</b><br><span class="muted">${{escapeHtml(p.path)}}</span></div>`).join('')
        : 'No projects registered.';
      document.querySelector('#jobs').innerHTML = renderJobs(jobs);
      document.querySelector('#schedules').innerHTML = schedules.length
        ? schedules.map(s => `<div class="item"><b>${{s.name}}</b><br><span class="muted">${{s.kind}} · ${{s.status}} · every ${{s.interval_seconds}}s<br>Next: ${{s.next_run_at}}</span><div class="actions"><button type="button" onclick="runSchedule('${{s.id}}')">Run</button><button type="button" class="secondary" onclick='editSchedule(${{JSON.stringify(s)}})'>Edit</button><button type="button" class="danger" onclick="deleteSchedule('${{s.id}}')">Delete</button><button type="button" class="secondary" onclick="enableSchedule('${{s.id}}')">Enable</button><button type="button" class="danger" onclick="disableSchedule('${{s.id}}')">Disable</button></div></div>`).join('')
        : 'No schedules.';
      document.querySelector('#system-events').innerHTML = systemEvents.length
        ? systemEvents.map(e => `<div class="item action"><b>${{escapeHtml(e.kind)}}</b><br><span class="muted">${{escapeHtml(e.created_at)}}</span><br><pre>${{asJson(e.payload)}}</pre></div>`).join('')
        : 'No actions recorded yet.';
    }}
    async function detailsFor(id) {{
      const [job, events] = await Promise.all([
        fetch(`/api/jobs/${{id}}`).then(r => r.json()),
        fetch(`/api/jobs/${{id}}/events`).then(r => r.json())
      ]);
      output.innerHTML = renderJobDetail(job, events);
    }}
    function renderJobDetail(job, events) {{
      return `<div class="item">
        <b>${{escapeHtml(job.status)}}</b> <span class="muted">${{escapeHtml(job.provider)}} &middot; ${{escapeHtml(job.id)}}</span><br>
        <div>${{escapeHtml(job.goal)}}</div>
        <div class="muted">Secret grant: ${{job.secret_grant_token ? escapeHtml(shortToken(job.secret_grant_token)) : '-'}}</div>
        <div class="mini">
          <div><span class="muted">Created</span><br>${{escapeHtml(job.created_at)}}</div>
          <div><span class="muted">Started</span><br>${{escapeHtml(job.started_at || '-')}}</div>
          <div><span class="muted">Heartbeat</span><br>${{escapeHtml(job.last_heartbeat_at || '-')}}</div>
          <div><span class="muted">Finished</span><br>${{escapeHtml(job.finished_at || '-')}}</div>
        </div>
        <div class="actions">
          <button type="button" onclick="preflightJob('${{job.id}}')">Preflight</button>
          <button type="button" class="danger" onclick="cancelJob('${{job.id}}')">Cancel</button>
          <button type="button" onclick="retryJob('${{job.id}}')">Retry</button>
        </div>
      </div>${{renderJobEvents(events)}}`;
    }}
    function renderJobs(jobs) {{
      if (!jobs.length) {{
        return 'No jobs yet.';
      }}
      const groups = [
        ['Active', job => ['Preparing', 'Running', 'HeartbeatMissed', 'Recovering'].includes(job.status)],
        ['Queued', job => job.status === 'Queued'],
        ['Failed / Cancelled', job => ['Failed', 'Cancelled'].includes(job.status)],
        ['Completed', job => job.status === 'Completed']
      ];
      return groups.map(([label, predicate]) => {{
        const groupJobs = jobs.filter(predicate);
        if (!groupJobs.length) {{
          return '';
        }}
        return `<details open><summary>${{label}} (${{groupJobs.length}})</summary>` +
          groupJobs.map(renderJobCard).join('') +
          `</details>`;
      }}).join('') || 'No jobs yet.';
    }}
    function shortToken(value) {{
      const token = String(value || '');
      return token.length > 18 ? `${{token.slice(0, 10)}}...${{token.slice(-6)}}` : token;
    }}
    function wireGrantTokenHints(grants) {{
      const tokens = grants
        .filter(grant => grant.token)
        .map(grant => grant.token);
      const value = tokens[0] || '';
      secret_grant_token.placeholder = value;
      schedule_secret_grant_token.placeholder = value;
    }}
    function renderJobCard(j) {{
      return `<div class="item">
        <b>${{escapeHtml(j.status)}}</b> <span class="muted">${{escapeHtml(j.provider)}} &middot; ${{escapeHtml(shortId(j.id))}}</span><br>
        ${{escapeHtml(j.goal)}}<br>
        <span class="muted">Created: ${{escapeHtml(j.created_at)}}<br>Started: ${{escapeHtml(j.started_at || '-')}}<br>Heartbeat: ${{escapeHtml(j.last_heartbeat_at || '-')}}<br>Finished: ${{escapeHtml(j.finished_at || '-')}}</span>
        <div class="actions">
          <button type="button" class="secondary" onclick="detailsFor('${{j.id}}')">Details</button>
          <button type="button" onclick="preflightJob('${{j.id}}')">Preflight</button>
          <button type="button" class="danger" onclick="cancelJob('${{j.id}}')">Cancel</button>
          <button type="button" onclick="retryJob('${{j.id}}')">Retry</button>
        </div>
      </div>`;
    }}
    function renderJobEvents(events) {{
      if (!events.length) {{
        return 'No events for this job.';
      }}
      return events.map(event => {{
        const payload = event.payload || {{}};
        let body = '';
        if (event.kind === 'context_pack') {{
          const hits = payload.hits || [];
          body = `<div class="muted">Query: ${{escapeHtml(payload.query || '-')}}<br>Hits: ${{hits.length}}</div>` +
            hits.slice(0, 5).map(hit => `<details><summary>${{escapeHtml(hit.reason || 'memory hit')}} score=${{Number(hit.score || 0).toFixed(3)}}</summary><pre>${{asJson(hit.item || hit)}}</pre></details>`).join('');
        }} else if (event.kind === 'prepared') {{
          body = `<div class="muted">Context hits=${{payload.context_hits ?? 0}} &middot; prompt chars=${{payload.prompt_chars ?? 0}}</div>
            <details><summary>Prepared command</summary><pre>${{asJson(payload.command || [])}}</pre></details>
            <details><summary>Project note</summary><pre>${{escapeHtml(payload.project_note || '-')}}</pre></details>`;
        }} else if (event.kind === 'gate_events') {{
          body = (payload.events || []).map(gate => `<div><span class="pill">${{escapeHtml(gate.kind || gate.action || 'gate')}}</span><pre>${{asJson(gate)}}</pre></div>`).join('') || '<span class="muted">No gate changes.</span>';
        }} else if (event.kind === 'provider_fallback_selected') {{
          body = `<div><span class="pill">fallback</span> ${{escapeHtml(payload.from || '-')}} -> ${{escapeHtml(payload.to || '-')}}</div>
            <div class="muted">${{escapeHtml(payload.reason || '')}}</div>`;
        }} else if (event.kind === 'budget_checked') {{
          body = `<div><span class="pill">budget</span> checked</div><pre>${{asJson(payload.checks || [])}}</pre>`;
        }} else if (event.kind === 'budget_blocked' || event.kind === 'provider_paused') {{
          const category = payload.category || {{}};
          body = `<div><span class="pill">${{escapeHtml(category.severity || 'warn')}}</span> ${{escapeHtml(category.code || event.kind)}}</div>
            <div>${{escapeHtml(category.message || payload.error || '')}}</div>
            <div class="muted">${{escapeHtml(category.next_step || '')}}</div>`;
        }} else if (event.kind === 'provider_diagnostic') {{
          const diagnostic = payload.diagnostic || {{}};
          body = `<div><span class="pill">${{escapeHtml(diagnostic.severity || 'info')}}</span> ${{escapeHtml(diagnostic.code || 'provider_diagnostic')}}</div>
            <div>${{escapeHtml(diagnostic.message || '')}}</div>
            <div class="muted">${{escapeHtml(diagnostic.next_step || '')}}</div>
            <details><summary>Raw line</summary><pre>${{escapeHtml(payload.line || '')}}</pre></details>`;
        }} else if (event.kind === 'preflight') {{
          body = `<div><span class="pill">${{escapeHtml(payload.selected_provider || 'provider')}}</span> launched=${{Boolean(payload.launched)}}</div>
            <div class="muted">${{escapeHtml(payload.project_name || '-')}} &middot; context hits=${{payload.context_hits ?? 0}} &middot; prompt chars=${{payload.prompt_chars ?? 0}}</div>
            ${{payload.fallback_from ? `<div class="muted">Fallback: ${{escapeHtml(payload.fallback_from)}} -> ${{escapeHtml(payload.selected_provider)}}<br>${{escapeHtml(payload.fallback_reason || '')}}</div>` : ''}}
            <details><summary>Prepared command</summary><pre>${{asJson(payload.command || [])}}</pre></details>
            <details><summary>Budget checks</summary><pre>${{asJson(payload.budget_checks || [])}}</pre></details>`;
        }} else if (event.kind === 'failure_category') {{
          const category = payload.category || {{}};
          body = `<div><span class="pill">${{escapeHtml(category.severity || 'error')}}</span> ${{escapeHtml(category.code || 'unknown_failure')}}</div>
            <div>${{escapeHtml(category.message || '')}}</div>
            <div class="muted">${{escapeHtml(category.next_step || '')}}</div>
            ${{payload.exit_code !== undefined ? `<div class="muted">Exit code: ${{payload.exit_code}}</div>` : ''}}
            ${{payload.line ? `<details><summary>Matched line</summary><pre>${{escapeHtml(payload.line)}}</pre></details>` : ''}}`;
        }} else if (event.kind === 'vault') {{
          body = `<div><span class="pill">knowledge base</span> run summary</div><pre>${{escapeHtml(payload.run_summary || '-')}}</pre>`;
        }} else if (event.kind === 'stdout' || event.kind === 'stderr') {{
          body = `<pre>${{escapeHtml(payload.line || '')}}</pre>`;
        }} else {{
          body = `<pre>${{asJson(payload)}}</pre>`;
        }}
        return `<div class="item action"><b>${{escapeHtml(event.kind)}}</b><br><span class="muted">${{escapeHtml(event.created_at)}}</span>${{body}}</div>`;
      }}).join('');
    }}
    async function pauseProvider(provider, model) {{
      const data = await fetch('/api/providers/pause', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{ provider, model, seconds: 1800, reason: 'manual admin pause' }})
      }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function resumeProvider(provider, model) {{
      const data = await fetch('/api/providers/resume', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{ provider, model }})
      }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function cancelJob(id) {{
      const data = await fetch(`/api/jobs/${{id}}/cancel`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function preflightJob(id) {{
      const data = await fetch(`/api/jobs/${{id}}/preflight`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function retryJob(id) {{
      const data = await fetch(`/api/jobs/${{id}}/retry`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function runSchedule(id) {{
      const data = await fetch(`/api/schedules/${{id}}/run`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function enableSchedule(id) {{
      const data = await fetch(`/api/schedules/${{id}}/enable`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function disableSchedule(id) {{
      const data = await fetch(`/api/schedules/${{id}}/disable`, {{ method: 'POST' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    async function deleteSchedule(id) {{
      const data = await fetch(`/api/schedules/${{id}}`, {{ method: 'DELETE' }}).then(r => r.json());
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }}
    function editSchedule(schedule) {{
      schedule_name.value = schedule.name;
      schedule_kind.value = schedule.kind === 'AgentTask' ? 'agent-task' : schedule.kind.toLowerCase();
      schedule_every.value = schedule.interval_seconds;
      schedule_message.value = schedule.payload.message || schedule.payload.task || '';
      schedule_project.value = schedule.payload.project || '';
      schedule_provider.value = schedule.payload.provider || 'codex';
      schedule_secret_grant_token.value = schedule.payload.secret_grant_token || '';
      schedule_goal.value = schedule.payload.goal || '';
      schedule_network.checked = Boolean(schedule.payload.allow_network);
      schedule_form.dataset.scheduleId = schedule.id;
    }}
    function resetScheduleForm() {{
      schedule_form.reset();
      schedule_every.value = 3600;
      delete schedule_form.dataset.scheduleId;
    }}
    document.querySelector('#schedule-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const body = {{
        name: schedule_name.value,
        kind: schedule_kind.value,
        every_seconds: Number(schedule_every.value || 1),
        message: schedule_message.value,
        project: schedule_project.value,
        provider: schedule_provider.value,
        secret_grant_token: schedule_secret_grant_token.value || null,
        goal: schedule_goal.value,
        allow_network: schedule_network.checked
      }};
      const id = schedule_form.dataset.scheduleId;
      const response = await fetch(id ? `/api/schedules/${{id}}` : '/api/schedules', {{
        method: id ? 'PATCH' : 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify(body)
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      resetScheduleForm();
      await load();
    }});
    document.querySelector('#worker-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/settings/worker', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{ max_concurrent_jobs: Number(worker_concurrency.value || 1) }})
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    document.querySelector('#routing-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/settings/routing', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{
          fallback_enabled: fallback_enabled.checked,
          fallback_order: fallback_order.value.split(',').map(value => value.trim()).filter(Boolean)
        }})
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    function optionalNumber(value) {{
      return value === '' ? null : Number(value);
    }}
    document.querySelector('#budget-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/settings/budget', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{
          enabled: budget_enabled.checked,
          daily_total_usd: optionalNumber(budget_total.value),
          daily_provider_usd: optionalNumber(budget_provider.value),
          daily_project_usd: optionalNumber(budget_project.value)
        }})
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    document.querySelector('#secret-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/secrets', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{
          name: secret_name.value,
          provider: secret_provider.value,
          kind: secret_kind.value || 'api-key',
          value: secret_value.value
        }})
      }});
      const data = await response.json();
      secret_value.value = '';
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    document.querySelector('#grant-form').addEventListener('submit', async event => {{
      event.preventDefault();
      const response = await fetch('/api/secrets/grants', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify({{
          secret: grant_secret.value,
          provider: grant_provider.value || null,
          capability: grant_capability.value || 'provider-proxy',
          ttl_seconds: Number(grant_ttl.value || 900),
          max_uses: Number(grant_max_uses.value || 1)
        }})
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    document.querySelector('#chat').addEventListener('submit', async event => {{
      event.preventDefault();
      const body = {{
        project: project.value,
        provider: provider.value,
        secret_grant_token: secret_grant_token.value || null,
        goal: goal.value,
        allow_network: allow_network.checked
      }};
      const response = await fetch('/api/chat', {{
        method: 'POST',
        headers: {{ 'content-type': 'application/json' }},
        body: JSON.stringify(body)
      }});
      const data = await response.json();
      output.textContent = JSON.stringify(data, null, 2);
      await load();
    }});
    load();
  </script>
</body>
</html>"#
    )
}

#[cfg(test)]
mod tests {
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
            assert!(recent_memory.iter().any(
                |item| matches!(item.kind, MemoryKind::UserMessage) && item.content == "/help"
            ));
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
}
