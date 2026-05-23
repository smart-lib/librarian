use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use axum::{
    extract::{Path as AxumPath, Query, State},
    response::{Html, IntoResponse},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use tokio::time::timeout;
use tokio::{io::AsyncWriteExt, process::Command as TokioCommand, sync::RwLock};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

use crate::{
    config::{Config, ToolPermissionPolicy, ToolPermissionsConfig},
    db::Database,
    domain::{
        ContextPack, JobStatus, MemoryKind, MountMode, NetworkMode, Project, ScheduleKind,
        ScheduleStatus, ToolApprovalStatus,
    },
    gates, library_tools,
    library_tools::LibraryRoot,
    memory, router, scheduler,
    secrets::SecretVault,
    third_eye, worker,
};

#[derive(Clone)]
struct AppState {
    db: Database,
    config: Arc<RwLock<Config>>,
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
      --danger: #c76f6f;
      --shadow: 0 18px 60px rgba(0, 0, 0, .38);
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
      color: var(--muted);
      pointer-events: auto;
      transition: color .16s ease, transform .18s cubic-bezier(.2, 1.4, .4, 1);
    }
    #settings-open { left: 12px; }
    #projects-open { right: 12px; }
    .icon-button:hover, .icon-button:focus-visible {
      color: var(--accent);
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
      padding: 86px 18px 28px;
      scroll-behavior: smooth;
    }
    .thread {
      width: 100%;
      margin: 0;
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
    .message small {
      display: block;
      margin-top: 8px;
      color: var(--muted);
    }
    .composer {
      border-top: 1px solid var(--line);
      background: rgba(18, 22, 25, .98);
      padding: 12px 14px 14px;
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
    .muted { color: var(--muted); }
    .tiny { font-size: 12px; }
    .stack { display: grid; gap: 12px; }
    .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
    .project-stage {
      min-height: 0;
      overflow: auto;
      padding: 28px clamp(20px, 5vw, 70px);
    }
    .tree {
      min-height: 100%;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 34px;
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
    .node.root {
      background: #1d2529;
      text-align: center;
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
      <div class="brand">Librarian<span id="context-line">Smart. Silent. Steady.</span></div>
      <button id="projects-open" class="icon-button" type="button" aria-label="Projects" title="Projects"><span class="map-icon"></span></button>
    </header>
    <main id="chat-log" class="chat-log">
      <div id="thread" class="thread">
        <article class="message assistant">Ready. Write what you want Librarian to do.</article>
      </div>
    </main>
    <form id="chat-form" class="composer" autocomplete="off">
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
        <button class="tab-button" type="button" data-tab="providers">Providers</button>
        <button class="tab-button" type="button" data-tab="jobs">Jobs</button>
        <button class="tab-button" type="button" data-tab="system">System</button>
      </nav>
      <div class="tab-content">
        <section class="tab-pane active" data-pane="overview"><h2>Overview</h2><div id="overview" class="grid"></div></section>
        <section class="tab-pane" data-pane="providers"><h2>Providers</h2><div id="providers" class="grid"></div></section>
        <section class="tab-pane" data-pane="jobs"><h2>Jobs</h2><div id="jobs" class="stack"></div></section>
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
        jobs: [],
        providers: { catalog: [], states: [] },
        health: null,
        activeProject: ''
      };
      const el = id => document.getElementById(id);
      const qsa = selector => Array.from(document.querySelectorAll(selector));
      const htmlEscape = value => String(value ?? '').replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
      const shortId = value => value ? String(value).slice(0, 8) : '';

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
      function appendMessage(role, text, detail) {
        const article = document.createElement('article');
        article.className = `message ${role}`;
        setMessage(article, text, detail);
        el('thread').appendChild(article);
        el('chat-log').scrollTop = el('chat-log').scrollHeight;
        return article;
      }
      function setMessage(article, text, detail) {
        article.textContent = text;
        if (detail !== undefined && detail !== null && detail !== '') {
          const small = document.createElement('small');
          small.textContent = detail;
          article.appendChild(small);
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
        const [health, projects, jobs, providers, events] = await Promise.all([
          loadJson('/api/health', null),
          loadJson('/api/projects', []),
          loadJson('/api/jobs', []),
          loadJson('/api/providers', { catalog: [], states: [] }),
          loadJson('/api/system-events', [])
        ]);
        state.health = health;
        state.projects = Array.isArray(projects) ? projects : [];
        state.jobs = Array.isArray(jobs) ? jobs : [];
        state.providers = providers || { catalog: [], states: [] };
        if (!state.projects.some(project => project.name === state.activeProject)) {
          state.activeProject = '';
        }
        renderOverview();
        renderProviders();
        renderJobs();
        renderSystemEvents(events);
        renderProjects();
        renderContext();
      }
      function renderContext() {
        el('context-line').textContent = 'Smart. Silent. Steady.';
      }
      function renderOverview() {
        const health = state.health || {};
        const worker = health.worker || {};
        const chat = health.chat || {};
        const memory = health.memory || {};
        const secrets = health.secrets || {};
        el('overview').innerHTML = [
          card('Worker', `queued=${worker.queued_jobs ?? 0}<br>running=${worker.running_jobs ?? 0}<br>slots=${worker.available_slots ?? '__WORKER_CONCURRENCY__'}`),
          card('Chat', `timeout=${chat.codex_timeout_seconds ?? 180}s<br>memory hits=${chat.memory_hit_limit ?? 12}<br>max iterations=${chat.max_iterations ?? 6}`),
          card('Memory', `items=${memory.items ?? 0}<br>embedded=${memory.embedded_items ?? 0}<br>missing=${memory.missing_embeddings ?? 0}`),
          card('Storage', `${htmlEscape(health.vault_path || 'Library')}<br><span class="muted">${htmlEscape(health.database_path || '.mdb/librarian.db')}</span>`),
          card('Secrets', `${htmlEscape(secrets.status || 'unknown')}<br><span class="muted">${htmlEscape(secrets.location || '')}</span>`)
        ].join('');
      }
      function renderProviders() {
        const states = new Map((state.providers.states || []).map(item => [`${item.provider}:${item.model || ''}`, item]));
        const models = state.providers.catalog || [];
        el('providers').innerHTML = models.length ? models.map(model => {
          const current = states.get(`${model.provider}:${model.model}`) || states.get(`${model.provider}:`) || {};
          return card(htmlEscape(model.provider), `${htmlEscape(model.model || 'default')}<br><span class="muted">${htmlEscape(current.status || 'Ready')}</span>`);
        }).join('') : '<div class="card muted">No providers reported.</div>';
      }
      function renderJobs() {
        el('jobs').innerHTML = state.jobs.length ? state.jobs.slice(0, 12).map(job => {
          return `<div class="card"><b>${htmlEscape(job.status)}</b> <span class="muted">${htmlEscape(job.provider)} ${shortId(job.id)}</span><br>${htmlEscape(job.goal)}<br><span class="muted tiny">${htmlEscape(job.created_at || '')}</span></div>`;
        }).join('') : '<div class="card muted">No jobs yet.</div>';
      }
      function renderSystemEvents(events) {
        el('system-events').innerHTML = Array.isArray(events) && events.length ? events.slice(0, 20).map(event => {
          return `<div class="card"><b>${htmlEscape(event.kind)}</b><br><span class="muted tiny">${htmlEscape(event.created_at || '')}</span></div>`;
        }).join('') : '<div class="card muted">No system events.</div>';
      }
      function renderProjects() {
        if (!state.projects.length) {
          el('project-stage').innerHTML = `<div class="empty"><h2>No projects yet</h2><div class="card muted">Add a project from the terminal for this build:<br><br><code>librarian --home ~/Librarian project add &lt;path&gt;</code></div></div>`;
          return;
        }
        const nodes = state.projects.map(project => {
          const active = project.name === state.activeProject ? ' active' : '';
          return `<div class="node${active}"><h3>${htmlEscape(project.name)}</h3><div class="muted tiny">${htmlEscape(project.path)}</div><button type="button" data-project="${htmlEscape(project.name)}">Use</button></div>`;
        }).join('');
        el('project-stage').innerHTML = `<div class="tree"><div class="node root"><h3>Librarian</h3><div class="muted tiny">Library and working projects</div></div><div class="node-column">${nodes}</div></div>`;
        qsa('[data-project]').forEach(button => button.addEventListener('click', () => {
          state.activeProject = button.dataset.project || '';
          renderProjects();
          renderContext();
          closeOverlay('projects-overlay');
        }));
      }
      function card(title, body) {
        return `<div class="card"><h3>${htmlEscape(title)}</h3><div>${body}</div></div>`;
      }
      async function submitChat(event) {
        event.preventDefault();
        const input = el('goal-input');
        const goal = input.value.trim();
        if (!goal) return;
        appendMessage('user', goal);
        input.value = '';
        const project = activeProjectName();
        try {
          const response = await fetch('/api/chat', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ message: goal, project: project || null })
          });
          const data = await response.json();
          if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
          appendMessage('assistant', data.reply || 'I am here.', data.project ? `project: ${data.project}` : 'global library');
          await refresh();
        } catch (error) {
          appendMessage('system', `Could not answer: ${error.message || error}`);
        }
      }

      el('settings-open').addEventListener('click', () => openOverlay('settings-overlay'));
      el('projects-open').addEventListener('click', () => openOverlay('projects-overlay'));
      qsa('[data-close]').forEach(button => button.addEventListener('click', () => closeOverlay(button.dataset.close)));
      qsa('.tab-button').forEach(button => button.addEventListener('click', () => setTab(button.dataset.tab)));
      el('chat-form').addEventListener('submit', submitChat);
      el('goal-input').addEventListener('keydown', event => {
        if (event.key === 'Enter' && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey) {
          event.preventDefault();
          el('chat-form').requestSubmit();
        }
      });
      refresh().catch(error => appendMessage('system', `Admin data failed to load: ${error.message || error}`));
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
      <div class="overlay-head"><div><h1>Project Map</h1><div class="muted tiny">A visual project tree from Librarian's project registry. Vault-backed project folders come next.</div></div><button type="button" class="secondary" onclick="closeOverlay('map-overlay')">Close</button></div>
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
      html += `<div class="node root" style="left:${rootX}px;top:${centerY}px"><b>Librarian</b><br><span class="muted">Vault projects</span></div>`;
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

#[derive(Debug, Deserialize)]
struct CreateJobRequest {
    project: String,
    goal: String,
    provider: Option<String>,
    secret_grant_token: Option<String>,
    allow_network: Option<bool>,
    read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct LibrarianChatRequest {
    message: String,
    project: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LibraryTreeQuery {
    root: Option<LibraryRoot>,
    max_depth: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct LibraryPathRequest {
    root: LibraryRoot,
    path: String,
}

#[derive(Debug, Deserialize)]
struct LibraryMoveRequest {
    root: LibraryRoot,
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
struct LibraryDeleteRequest {
    root: LibraryRoot,
    path: String,
    recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct LibraryMarkdownRequest {
    path: String,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateScheduleRequest {
    name: String,
    kind: String,
    every_seconds: i64,
    project: Option<String>,
    goal: Option<String>,
    provider: Option<String>,
    secret_grant_token: Option<String>,
    message: Option<String>,
    allow_network: Option<bool>,
    read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateWorkerRequest {
    max_concurrent_jobs: usize,
}

#[derive(Debug, Deserialize)]
struct UpdateRoutingRequest {
    fallback_enabled: bool,
    fallback_order: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateBudgetRequest {
    enabled: bool,
    daily_total_usd: Option<f64>,
    daily_provider_usd: Option<f64>,
    daily_project_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CreateSecretRequest {
    name: String,
    provider: String,
    kind: Option<String>,
    value: String,
}

#[derive(Debug, Deserialize)]
struct CreateSecretGrantRequest {
    secret: String,
    provider: Option<String>,
    capability: Option<String>,
    ttl_seconds: Option<i64>,
    max_uses: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ProviderControlRequest {
    provider: String,
    model: Option<String>,
    seconds: Option<i64>,
    reason: Option<String>,
}

pub async fn serve(bind: String, db: Database, config: Config) -> Result<()> {
    let state = AppState {
        db,
        config: Arc::new(RwLock::new(config)),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/projects", get(projects))
        .route("/api/project-map", get(project_map))
        .route("/api/jobs", get(jobs).post(create_job))
        .route("/api/schedules", get(schedules).post(create_schedule))
        .route("/api/settings/worker", post(update_worker_settings))
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
        .route("/api/chat", post(librarian_chat))
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
    Ok(Json(state.db.list_projects().await?))
}

async fn project_map(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.clone();
    let projects = state.db.list_projects().await?;
    Ok(Json(build_project_map(&config, projects)?))
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
    Ok(Json(serde_json::json!({
        "catalog": catalog,
        "states": states,
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

async fn librarian_chat(
    State(state): State<AppState>,
    Json(input): Json<LibrarianChatRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let message = input.message.trim();
    if message.is_empty() {
        return Err(anyhow::anyhow!("message must not be empty").into());
    }

    let project = match input
        .project
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Some(state.db.get_project_by_name_or_id(value).await?),
        None => None,
    };
    let project_id = project.as_ref().map(|project| project.id);
    let config = state.config.read().await.clone();
    let gated = gates::process_user_prompt(&state.db, &config, message, "librarian-chat").await?;

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
                "project": project.as_ref().map(|project| project.name.clone()),
                "scope": if project.is_some() { "project" } else { "global" },
            }),
        )
        .await?;
    memory::embed_item(&state.db, &config, &user_memory).await?;

    let chat_result = if let Some(result) =
        execute_slash_command(&state, &config, project.as_ref(), &gated.content).await?
    {
        result
    } else {
        let initial_context_pack = memory::retrieve_context_with_config(
            &state.db,
            Some(&config),
            memory::RetrievalRequest {
                query: gated.content.clone(),
                project_id,
                activity_id: None,
                limit: config.chat.memory_hit_limit,
            },
        )
        .await?;
        run_librarian_chat_loop(
            &state.db,
            &config,
            &gated.content,
            project.as_ref(),
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
                "project": project.as_ref().map(|project| project.name.clone()),
                "scope": if project.is_some() { "project" } else { "global" },
                "mode": chat_result.mode,
                "iterations": chat_result.iterations,
                "trace": chat_result.trace,
            }),
        )
        .await?;
    memory::embed_item(&state.db, &config, &assistant_memory).await?;

    Ok(Json(serde_json::json!({
        "reply": reply,
        "project": project.as_ref().map(|project| project.name.clone()),
        "memory_hits": chat_result.memory_hits,
        "mode": chat_result.mode,
        "iterations": chat_result.iterations,
    })))
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
    let network_mode = if input.allow_network.unwrap_or(false) {
        NetworkMode::Open
    } else if input.secret_grant_token.is_some() {
        NetworkMode::Open
    } else {
        NetworkMode::None
    };
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
            router::parse_provider_kind(input.provider.as_deref().unwrap_or("codex"))?,
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

struct LibrarianChatResult {
    reply: String,
    iterations: usize,
    memory_hits: Vec<crate::domain::MemoryHit>,
    trace: Vec<serde_json::Value>,
    mode: &'static str,
}

#[derive(Debug, Deserialize)]
struct LibrarianChatDirective {
    action: String,
    query: Option<String>,
    answer: Option<String>,
    question: Option<String>,
    reason: Option<String>,
}

async fn execute_slash_command(
    state: &AppState,
    config: &Config,
    project: Option<&Project>,
    message: &str,
) -> Result<Option<LibrarianChatResult>> {
    let Some(command_line) = message.trim().strip_prefix('/') else {
        return Ok(None);
    };
    let args = split_slash_args(command_line)?;
    if args.is_empty() {
        return Ok(Some(slash_reply(
            "Available commands: /lib, /work, /mem, /settings, /remember, /help",
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
    } else if matches!(command.as_str(), "agent" | "agents" | "job" | "jobs") {
        execute_agent_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "project" | "projects") {
        execute_project_slash_command(state, config, &args[1..]).await?
    } else if matches!(command.as_str(), "approval" | "approvals") {
        execute_approval_slash_command(state, &args[1..]).await?
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
                "Unknown slash command. Try /help. Library commands live under /lib; working-folder commands live under /work; memory commands live under /mem; project commands live under /project; approvals live under /approval; settings commands live under /settings; background agent jobs live under /agent.",
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
    }
}

fn slash_help() -> &'static str {
    "Available command groups:\n/lib help - Markdown library and library hierarchy tools\n/work help - default working-folder tools under Projects\n/project help - library project and workspace attachment tools\n/mem help - durable memory tools\n/approval help - pending tool approval proposals\n/settings help - inspect and change guarded settings\n/agent help - explicit background agent jobs\n\nLibrary projects live in /lib. Implementation/product working folders live in /work or attached external project records."
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
                reply.push_str(&format!(
                    "\n{} {:?} {}.{} {}",
                    approval.id, approval.status, approval.tool, approval.action, approval.payload
                ));
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
            let approval = state
                .db
                .update_tool_approval_status(id, ToolApprovalStatus::Approved)
                .await?;
            state
                .db
                .add_system_event(
                    "tool_approval",
                    serde_json::json!({ "action": "approve", "approval_id": approval.id }),
                )
                .await?;
            slash_reply(
                &format!("Approved tool proposal {}.", approval.id),
                serde_json::json!({ "tool": "approval", "command": command, "approval": approval }),
            )
        }
        "reject" => {
            let id = slash_approval_id_arg(args, "/approval reject <approval-id>")?;
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
            slash_reply(
                &format!("Rejected tool proposal {}.", approval.id),
                serde_json::json!({ "tool": "approval", "command": command, "approval": approval }),
            )
        }
        _ => slash_reply(
            "Unknown approval command. Try /approval help.",
            serde_json::json!({ "tool": "approval", "command": command, "status": "unknown" }),
        ),
    };

    Ok(result)
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

fn approval_slash_help() -> &'static str {
    "Approval commands live under /approval:\n/approval list [limit]\n/approval propose <tool> <action> <payload-json>\n/approval approve <approval-id>\n/approval reject <approval-id>\n\nThis is the approval queue for future assistant-initiated tool calls. Approving marks intent; executors still need to run the approved action explicitly."
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
            let mut reply = format!("Recent memory: {} item(s).", items.len());
            for item in &items {
                reply.push_str(&format!(
                    "\n{} {:?}: {}",
                    item.observed_at.format("%Y-%m-%d %H:%M"),
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

fn memory_slash_help() -> &'static str {
    "Memory commands live under /mem:\n/mem remember <fact|decision|instruction|status|summary> <content>\n/remember <content> - shortcut for /mem remember fact <content>\n/mem recent [limit]\n\nMemory is stored in the current chat scope: selected project when present, otherwise global."
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
    "Settings commands live under /settings:\n/settings tool-permissions - show current tool permission policies\n/settings set-tool-permission <key> <auto|ask|deny> --yes - update one permission\n\nPermission keys: library_read, library_create, library_edit_markdown, library_move, library_delete, workspace_create, workspace_move, workspace_delete, memory_write, settings_change, agent_launch."
}

fn format_tool_permissions(permissions: &ToolPermissionsConfig) -> String {
    format!(
        "Tool permissions:\n\
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
agent_launch = {}",
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
    )
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
        _ => anyhow::bail!("Unknown tool permission key `{key}`. Try /settings tool-permissions."),
    }
    Ok(())
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
            let network_mode = if request.allow_network || request.secret_grant_token.is_some() {
                NetworkMode::Open
            } else {
                NetworkMode::None
            };
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

fn split_slash_args(command_line: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = command_line.trim().chars().peekable();
    let mut quote = None;

    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(active), ch) if ch == active => quote = None,
            (Some(_), '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), ch) => current.push(ch),
            (None, '"' | '\'') => quote = Some(ch),
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (None, ch) => current.push(ch),
        }
    }

    if quote.is_some() {
        anyhow::bail!("Unclosed quote in slash command");
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

async fn run_librarian_chat_loop(
    db: &Database,
    config: &Config,
    message: &str,
    project: Option<&Project>,
    initial_context_pack: ContextPack,
) -> Result<LibrarianChatResult> {
    let project_id = project.map(|project| project.id);
    let max_iterations = config.chat.max_iterations.clamp(1, 100);
    let mut context_packs = vec![initial_context_pack];
    let mut trace = Vec::new();
    let mut last_raw_reply = String::new();

    for iteration in 1..=max_iterations {
        let prompt = build_librarian_chat_prompt(
            message,
            project,
            &context_packs,
            iteration,
            max_iterations,
        );
        let raw_reply = run_librarian_codex_chat(config, &prompt).await?;
        last_raw_reply = raw_reply.clone();

        let Some(directive) = parse_librarian_chat_directive(&raw_reply) else {
            return Ok(LibrarianChatResult {
                reply: raw_reply,
                iterations: iteration,
                memory_hits: combined_memory_hits(&context_packs),
                trace,
                mode: "codex-chat",
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
                }));
                return Ok(LibrarianChatResult {
                    reply,
                    iterations: iteration,
                    memory_hits: combined_memory_hits(&context_packs),
                    trace,
                    mode: "codex-chat",
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
                }));
                return Ok(LibrarianChatResult {
                    reply,
                    iterations: iteration,
                    memory_hits: combined_memory_hits(&context_packs),
                    trace,
                    mode: "codex-chat",
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
            _ => {
                return Ok(LibrarianChatResult {
                    reply: raw_reply,
                    iterations: iteration,
                    memory_hits: combined_memory_hits(&context_packs),
                    trace,
                    mode: "codex-chat",
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
    })
}

fn build_librarian_chat_prompt(
    message: &str,
    project: Option<&Project>,
    context_packs: &[ContextPack],
    iteration: usize,
    max_iterations: usize,
) -> String {
    let scope = project
        .map(|project| format!("project `{}`", project.name))
        .unwrap_or_else(|| "global conversation".to_string());
    let mut prompt = String::new();
    prompt.push_str("You are Librarian: a calm, practical assistant for organizing ideas, projects, memory, and work.\n");
    prompt.push_str("You are speaking directly with the user in the admin chat.\n");
    prompt.push_str("You are not a background coding agent in this conversation.\n");
    prompt.push_str("Do not claim to have launched agents, edited files, changed settings, or used tools unless the provided context explicitly says so.\n");
    prompt.push_str("Use the retrieved memory as context, but do not dump it back verbatim. Answer naturally and helpfully.\n");
    prompt.push_str("If the user asks for work that should become an agent task, discuss the plan and say that launching a background agent should be an explicit separate action.\n");
    prompt.push_str("Keep the answer concise unless the user asks for detail.\n\n");
    prompt.push_str("You may answer directly in plain text. If and only if you need another memory search before answering, reply with a single JSON object and no prose: {\"action\":\"search_memory\",\"query\":\"short search query\",\"reason\":\"why this extra lookup is needed\"}. If you need the user to clarify, reply with {\"action\":\"clarify\",\"question\":\"your question\"}. If you use JSON, it is an internal control message and will not be shown directly.\n\n");

    prompt.push_str(&format!("## Current Scope\n\n{scope}\n\n"));
    prompt.push_str(&format!(
        "## Loop Budget\n\nIteration {iteration} of {max_iterations}. Stop early and answer when you have enough context.\n\n"
    ));
    if iteration >= max_iterations {
        prompt.push_str("This is the final allowed iteration. Do not request another memory search; answer with the available context or ask one clarifying question.\n\n");
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

fn filtered_memory_hits(context_packs: &[ContextPack]) -> Vec<&crate::domain::MemoryHit> {
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

fn combined_memory_hits(context_packs: &[ContextPack]) -> Vec<crate::domain::MemoryHit> {
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
          body = `<div><span class="pill">vault</span> run summary</div><pre>${{escapeHtml(payload.run_summary || '-')}}</pre>`;
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
    fn leaves_plain_chat_reply_as_final_text() {
        assert!(parse_librarian_chat_directive("Yes, I am here and I see the context.").is_none());
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
        assert!(set_tool_permission(
            &mut permissions,
            "unknown_permission",
            ToolPermissionPolicy::Deny,
        )
        .is_err());
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
    }

    #[test]
    fn parses_approval_json_payload() {
        let payload = parse_json_payload(r#"{"tool":"library","path":"note.md"}"#).expect("json");
        assert_eq!(payload["tool"], "library");
        assert!(parse_json_payload("not-json").is_err());
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
}
