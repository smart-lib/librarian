pub fn chat_first_app_html(bind: &str, worker_concurrency: usize) -> String {
    let html = r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Librarian</title>
  <style>
    :root {
      color-scheme: dark;
      --bg: #070a0f;
      --panel: rgba(14, 22, 40, .72);
      --panel-2: rgba(28, 38, 60, .82);
      --text: #efe6d0;
      --muted: rgba(239, 230, 208, .58);
      --dim: rgba(239, 230, 208, .32);
      --line: rgba(231, 185, 97, .18);
      --line-strong: rgba(231, 185, 97, .42);
      --accent: #70dcc0;
      --accent-2: #7bb1ff;
      --violet: #a58cff;
      --chrome: #e8c86d;
      --chrome-hover: #f0cd86;
      --danger: #c76f6f;
      --shadow: 0 22px 70px rgba(0, 0, 0, .45);
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
      background:
        radial-gradient(circle at 18% 12%, rgba(112, 220, 192, .12), transparent 30%),
        radial-gradient(circle at 82% 8%, rgba(232, 200, 109, .10), transparent 28%),
        linear-gradient(135deg, #070a0f 0%, #0d1118 48%, #080b10 100%);
      color: var(--text);
    }
    #atlas-bg {
      position: fixed;
      inset: 0;
      z-index: 0;
      width: 100%;
      height: 100%;
      filter: saturate(.86);
      pointer-events: none;
    }
    #atlas-bg::after {
      content: "";
      position: fixed;
      inset: 0;
      background:
        radial-gradient(circle at 50% 35%, rgba(123, 177, 255, .07), transparent 45%),
        radial-gradient(circle at 100% 100%, rgba(165, 140, 255, .08), transparent 50%),
        linear-gradient(180deg, rgba(6, 8, 26, .20) 0%, rgba(6, 8, 26, .72) 100%);
      pointer-events: none;
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
      background: rgba(7, 10, 15, .72);
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
    .pill.ok { border-color: var(--accent); color: var(--accent); }
    .pill.warn { border-color: #e0c56f; color: #e0c56f; }
    .pill.error { border-color: #ef7777; color: #ef7777; }
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
      position: relative;
      z-index: 1;
      height: 100dvh;
      min-height: 560px;
      display: grid;
      grid-template-rows: 64px minmax(0, 1fr) auto;
      overflow: hidden;
      pointer-events: none;
    }
    .app > * { pointer-events: auto; }
    .shell {
      position: fixed;
      inset: 0;
      z-index: 1;
    }
    .topbar {
      position: relative;
      z-index: 6;
      height: 64px;
      display: grid;
      grid-template-columns: 60px minmax(0, 1fr) 60px;
      align-items: start;
      padding: 14px 22px 0;
      gap: 16px;
    }
    .drawer {
      position: absolute;
      top: 0;
      left: 50%;
      transform: translateX(-50%);
      z-index: 5;
      width: min(420px, calc(100vw - 200px));
    }
    .drawer-card {
      padding: 8px 22px 10px;
      text-align: center;
      border: 1px solid var(--line);
      border-top: 0;
      border-radius: 0 0 18px 18px;
      background:
        radial-gradient(circle at 50% 0%, rgba(231,185,97,.12), transparent 55%),
        rgba(10, 14, 30, .94);
      box-shadow: var(--shadow), inset 0 1px 0 rgba(255, 255, 255, .05);
      backdrop-filter: blur(14px);
      line-height: 1.2;
      cursor: pointer;
      transition: border-color .2s, box-shadow .2s;
    }
    .drawer-card:hover {
      border-color: var(--chrome);
      box-shadow: 0 14px 32px rgba(0,0,0,.55), 0 0 26px rgba(231,185,97,.20);
    }
    .drawer-card::after {
      content: "";
      display: block;
      width: 36px;
      height: 3px;
      border-radius: 3px;
      margin: 7px auto -3px;
      background: var(--line-strong);
      transition: background .2s, width .2s;
    }
    .drawer.open .drawer-card::after { background: var(--chrome); width: 56px; }
    .d-name {
      font-family: "Iowan Old Style", "Palatino Linotype", Georgia, serif;
      font-style: italic;
      font-size: 22px;
      font-weight: 500;
      letter-spacing: .02em;
      color: var(--chrome-hover);
      line-height: 1.05;
    }
    .d-context {
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 10px;
      letter-spacing: .3em;
      color: var(--chrome);
      text-transform: uppercase;
      margin-top: 4px;
    }
    .drawer-extra {
      max-height: 0;
      overflow: hidden;
      opacity: 0;
      transition: max-height .35s ease, opacity .25s ease, margin-top .25s ease;
      margin-top: 0;
      text-align: left;
    }
    .drawer.open .drawer-extra {
      max-height: 420px;
      opacity: 1;
      margin-top: 10px;
    }
    .d-slogan-inner {
      text-align: center;
      font-style: italic;
      font-size: 12px;
      color: var(--muted);
      letter-spacing: .04em;
      margin-bottom: 12px;
    }
    .d-row {
      display: grid;
      grid-template-columns: 110px minmax(0, 1fr);
      align-items: baseline;
      padding: 8px 0;
      border-top: 1px solid var(--line);
    }
    .d-row-label {
      font-size: 9px;
      letter-spacing: .28em;
      color: var(--dim);
      text-transform: uppercase;
    }
    .d-row-value {
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 11.5px;
      color: var(--text);
      word-break: break-word;
    }
    .d-row-value b { color: var(--chrome-hover); font-weight: 500; }
    .sep { color: var(--dim); margin: 0 5px; }
    .corner-btn {
      width: 48px;
      height: 48px;
      min-height: 48px;
      display: grid;
      place-items: center;
      border-radius: 12px;
      border: 1px solid var(--chrome);
      background: rgba(10, 14, 30, .82);
      backdrop-filter: blur(10px);
      color: var(--chrome);
      cursor: pointer;
      transition: border-color .15s, background .15s, transform .15s, box-shadow .15s;
      box-shadow: 0 8px 22px rgba(0,0,0,.45), 0 0 0 0 rgba(231,185,97,0);
      position: relative;
      padding: 0;
    }
    .corner-btn:hover {
      border-color: var(--chrome-hover);
      background: rgba(28, 38, 60, .92);
      transform: translateY(-1px);
      box-shadow: 0 12px 28px rgba(0,0,0,.55), 0 0 14px rgba(231,185,97,.35);
    }
    .corner-btn svg {
      width: 22px;
      height: 22px;
      stroke: currentColor;
      fill: none;
      stroke-width: 1.8;
      stroke-linecap: round;
      stroke-linejoin: round;
    }
    .corner-btn[data-tip]::after {
      content: attr(data-tip);
      position: absolute;
      top: 100%;
      left: 50%;
      transform: translateX(-50%);
      margin-top: 8px;
      padding: 4px 8px;
      font-size: 10px;
      letter-spacing: .15em;
      text-transform: uppercase;
      color: var(--muted);
      background: rgba(8, 12, 24, .95);
      border: 1px solid var(--line);
      border-radius: 4px;
      white-space: nowrap;
      opacity: 0;
      pointer-events: none;
      transition: opacity .15s;
    }
    .corner-btn:hover[data-tip]::after { opacity: 1; }
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
    #settings-open { grid-column: 1; justify-self: start; }
    #projects-open { grid-column: 3; justify-self: end; }
    #new-chat {
      grid-column: 3;
      justify-self: end;
      margin-right: 58px;
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
      padding: 8px 22px 0;
      scroll-behavior: smooth;
      scrollbar-width: thin;
      scrollbar-color: var(--line-strong) transparent;
      display: grid;
      grid-template-columns: 1fr min(820px, 100%) 1fr;
    }
    .chat-log::-webkit-scrollbar { width: 6px; }
    .chat-log::-webkit-scrollbar-thumb { background: var(--line-strong); border-radius: 6px; }
    .thread {
      grid-column: 2;
      width: 100%;
      display: flex;
      flex-direction: column;
      gap: 18px;
      padding: 28px 6px 80px;
    }
    .message {
      max-width: 100%;
      padding: 15px 18px;
      border: 1px solid var(--line);
      border-radius: 14px;
      background: var(--panel);
      backdrop-filter: blur(16px);
      white-space: pre-wrap;
      line-height: 1.55;
      position: relative;
      box-shadow: 0 22px 58px rgba(0,0,0,.34);
    }
    .message.user {
      align-self: flex-end;
      margin-left: 16%;
      background: linear-gradient(180deg, rgba(165, 140, 255, .12), rgba(123, 177, 255, .06));
      border-color: rgba(165, 140, 255, .25);
      color: var(--text);
    }
    .message.user::before {
      content: "YOU";
      position: absolute;
      top: -8px;
      left: 14px;
      padding: 0 8px;
      background: var(--bg);
      color: var(--violet);
      font-size: 9px;
      letter-spacing: .35em;
      font-weight: 700;
    }
    .message.assistant {
      margin-right: 10%;
      border-left: 2px solid var(--chrome);
    }
    .message.assistant::before {
      content: "LIBRARIAN";
      position: absolute;
      top: -8px;
      left: 14px;
      padding: 0 8px;
      background: var(--bg);
      color: var(--chrome);
      font-size: 9px;
      letter-spacing: .35em;
      font-weight: 700;
    }
    .message.assistant, .message.system { align-self: flex-start; }
    .message.system {
      margin-left: 24px;
      margin-right: 24px;
      border-style: dashed;
      border-color: rgba(112, 220, 192, .35);
      background: rgba(8, 12, 24, .55);
      color: var(--muted);
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
      box-shadow: none;
    }
    .message.system::before {
      content: "SYSTEM";
      position: absolute;
      top: -8px;
      left: 14px;
      padding: 0 8px;
      background: var(--bg);
      color: var(--accent);
      font-size: 9px;
      letter-spacing: .35em;
    }
    .message.command {
      border-color: rgba(123, 177, 255, .2);
      border-left: 2px solid var(--accent-2);
      background: rgba(8, 12, 24, .35);
      color: var(--accent-2);
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12.5px;
    }
    .message.command small { color: var(--accent-2); }
    .message.thinking {
      color: var(--muted);
      border-style: solid;
      display: flex;
      align-items: center;
      gap: 12px;
      font-style: italic;
    }
    .message.approval {
      border-color: var(--chrome);
      background: linear-gradient(180deg, rgba(232, 200, 109, .14), rgba(232, 200, 109, .04));
      box-shadow: 0 0 24px rgba(232, 200, 109, .12);
    }
    .message.approval::before {
      content: "APPROVAL";
    }
    .message.agent-card {
      border-color: rgba(123, 177, 255, .42);
      background: linear-gradient(180deg, rgba(123, 177, 255, .13), rgba(112, 220, 192, .045));
      box-shadow: 0 0 28px rgba(123, 177, 255, .10);
    }
    .message.agent-card::before {
      content: "AGENT";
      color: var(--accent-2);
    }
    .message blockquote,
    .message .quote {
      margin: 10px 0;
      padding: 6px 14px;
      border-left: 2px solid var(--chrome);
      background: rgba(232, 200, 109, .06);
      color: var(--muted);
      font-style: italic;
    }
    .message pre {
      margin: 12px 0;
      padding: 12px 14px;
      border: 1px solid var(--line);
      border-radius: 10px;
      background: rgba(0,0,0,.42);
      color: #cce6ff;
      overflow-x: auto;
      white-space: pre-wrap;
    }
    .message code {
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
    }
    .message p { margin: 0 0 10px; }
    .message p:last-child { margin-bottom: 0; }
    .message a {
      color: var(--accent-2);
      text-decoration: none;
      border-bottom: 1px dotted rgba(123,177,255,.5);
    }
    .message a:hover {
      color: var(--accent);
      border-bottom-color: var(--accent);
    }
    .message .attachment-row {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr) auto;
      gap: 10px;
      align-items: center;
      margin: 6px 0;
      padding: 8px 10px;
      border-radius: 8px;
      background: rgba(123,177,255,.06);
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
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
    .agent-event-list {
      display: grid;
      gap: 6px;
      margin-top: 10px;
      color: var(--muted);
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 11.5px;
    }
    .agent-event-list div {
      padding: 7px 9px;
      border: 1px solid rgba(123,177,255,.14);
      border-radius: 8px;
      background: rgba(0,0,0,.18);
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
      margin-top: 11px;
      padding-top: 8px;
      border-top: 1px dashed var(--line);
      color: var(--dim);
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 10px;
      letter-spacing: .12em;
      text-align: right;
    }
    .composer {
      display: grid;
      grid-template-columns: 1fr min(820px, 100%) 1fr;
      border-top: 0;
      background: transparent;
      padding: 8px 22px 18px;
      position: relative;
    }
    .composer-inner {
      grid-column: 2;
      width: 100%;
      margin: 0;
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 10px;
      align-items: end;
      border: 1px solid var(--line);
      border-radius: 14px;
      background: rgba(10, 14, 30, .82);
      backdrop-filter: blur(12px);
      box-shadow: 0 -10px 40px rgba(0,0,0,.4);
      padding: 10px 12px;
    }
    .composer-inner:focus-within {
      border-color: var(--chrome);
    }
    .composer-tools {
      grid-column: 1 / -1;
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }
    .composer-tool {
      min-height: 28px;
      padding: 0 10px;
      border-radius: 999px;
      border-color: var(--line);
      background: rgba(8, 12, 24, .55);
      color: var(--muted);
      font-size: 11px;
      letter-spacing: .08em;
      text-transform: uppercase;
    }
    .composer-tool:hover {
      color: var(--chrome);
      border-color: var(--line-strong);
    }
    #goal-input {
      height: 58px;
      max-height: 30vh;
      resize: none;
      background: transparent;
      border: 0;
      padding: 6px 4px;
      outline: 0;
    }
    .send-button {
      width: 42px;
      height: 42px;
      min-height: 42px;
      padding: 0;
      border: 0;
      border-radius: 12px;
      display: grid;
      place-items: center;
      background: var(--chrome);
      color: #1a0f02;
      transition: transform .15s ease, background .15s ease;
    }
    .send-button:hover {
      background: var(--chrome-hover);
      transform: scale(1.04);
    }
    .send-button::before {
      content: "";
      width: 0;
      height: 0;
      border-top: 8px solid transparent;
      border-bottom: 8px solid transparent;
      border-left: 13px solid currentColor;
      transform: translateX(1px);
    }
    .slash-palette {
      position: absolute;
      left: 50%;
      right: auto;
      transform: translateX(-50%);
      width: min(620px, calc(100vw - 40px));
      bottom: calc(100% + 10px);
      max-height: 260px;
      overflow: auto;
      display: none;
      border: 1px solid var(--line-strong);
      border-radius: 12px;
      background: rgba(14, 22, 40, .94);
      backdrop-filter: blur(14px);
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
      background: rgba(6, 8, 26, .82);
      backdrop-filter: blur(8px);
    }
    .overlay.open { display: flex; }
    #projects-overlay.open {
      display: grid;
      grid-template-rows: minmax(0, 1fr);
    }
    #projects-overlay .overlay-head {
      display: none;
    }
    .settings-frame {
      margin: 70px auto;
      width: min(1160px, 92vw);
      max-height: calc(100vh - 100px);
      background: #0e1628;
      border: 1px solid var(--line);
      border-radius: 18px;
      box-shadow: var(--shadow);
      display: grid;
      grid-template-rows: auto auto minmax(0, 1fr);
      overflow: hidden;
    }
    .settings-head {
      display: flex;
      align-items: center;
      gap: 4px;
      padding: 18px 26px 0;
    }
    .settings-head h2 {
      flex: 1;
      margin: 0;
      font-size: 18px;
      font-weight: 500;
      letter-spacing: .04em;
      color: var(--chrome-hover);
    }
    .settings-tabs {
      display: flex;
      gap: 4px;
      padding: 14px 22px 0;
      border-bottom: 1px solid var(--line);
    }
    .settings-tabs .tab-button {
      padding: 10px 18px;
      border: 0;
      background: transparent;
      color: var(--muted);
      cursor: pointer;
      font-size: 12px;
      letter-spacing: .15em;
      text-transform: uppercase;
      border-bottom: 2px solid transparent;
      border-radius: 0;
      min-height: 38px;
    }
    .settings-tabs .tab-button:hover { color: var(--text); }
    .settings-tabs .tab-button.active {
      color: var(--chrome);
      border-color: var(--chrome);
      background: transparent;
    }
    .settings-body {
      min-height: 0;
      overflow: auto;
      padding: 22px 26px 30px;
    }
    .close-btn {
      position: absolute;
      top: 18px;
      right: 22px;
      width: 38px;
      height: 38px;
      min-height: 38px;
      padding: 0;
      border-radius: 50%;
      border: 1px solid var(--line);
      background: rgba(8, 12, 24, .85);
      color: var(--chrome);
      cursor: pointer;
      display: grid;
      place-items: center;
      z-index: 5;
    }
    .close-btn:hover { border-color: var(--chrome); }
    .close-btn svg { width: 18px; height: 18px; stroke: currentColor; stroke-width: 2; fill: none; }
    .overlay-head {
      display: grid;
      grid-template-columns: 64px minmax(0, 1fr) 64px;
      align-items: center;
      position: relative;
      border-bottom: 1px solid var(--line);
      background: rgba(8, 12, 18, .92);
      backdrop-filter: blur(16px);
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
      background: rgba(9, 13, 19, .72);
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
    .tab-pane { display: none; max-width: 1320px; }
    .tab-pane.active { display: block; }
    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      gap: 12px;
    }
    #providers.grid {
      grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
      align-items: start;
    }
    #providers > .card:first-child {
      grid-column: 1 / -1;
    }
    #providers .card {
      min-width: 0;
      overflow-wrap: anywhere;
    }
    .provider-runtime .form-grid {
      grid-template-columns: minmax(0, 1fr) minmax(150px, 220px) auto;
    }
    .provider-runtime .form-grid .wide {
      grid-column: auto;
    }
    .provider-actions {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
      gap: 8px;
      margin-top: 12px;
    }
    .card {
      border: 1px solid var(--line);
      border-radius: 8px;
      background: linear-gradient(180deg, rgba(18, 25, 34, .92), rgba(13, 18, 25, .88));
      box-shadow: inset 0 1px 0 rgba(255,255,255,.035);
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
      overflow: hidden;
      padding: 0;
      position: relative;
    }
    .project-layout {
      display: block;
      position: relative;
      min-height: 100%;
      height: 100%;
    }
    .atlas-panel {
      position: relative;
      min-height: 0;
      height: 100%;
      overflow: hidden;
      border: 0;
      border-radius: 0;
      background: #04050d;
      box-shadow: none;
    }
    .atlas-canvas {
      width: 100%;
      height: 100%;
      display: block;
      cursor: default;
    }
    .atlas-canvas.clickable { cursor: pointer; }
    .atlas-stamp,
    .atlas-help {
      position: absolute;
      left: 28px;
      z-index: 2;
      color: rgba(180,220,255,.45);
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 11px;
      letter-spacing: .5em;
      text-transform: uppercase;
      pointer-events: none;
    }
    .atlas-stamp { top: 22px; }
    .atlas-help {
      bottom: 22px;
      color: rgba(180,220,255,.35);
      letter-spacing: .25em;
    }
    .atlas-title {
      max-width: min(640px, 68%);
      padding: 12px 16px;
      border: 1px solid rgba(112, 220, 192, .24);
      border-radius: 18px;
      background: rgba(5, 8, 13, .64);
      box-shadow: 0 12px 36px rgba(0,0,0,.24);
      backdrop-filter: blur(14px);
    }
    .atlas-title strong {
      display: block;
      font-size: clamp(18px, 2.2vw, 28px);
      line-height: 1.05;
    }
    .atlas-title span {
      display: block;
      margin-top: 6px;
      color: var(--muted);
      font-size: 12px;
    }
    .atlas-status {
      padding: 8px 12px;
      border: 1px solid rgba(232, 200, 109, .26);
      border-radius: 999px;
      color: var(--chrome);
      background: rgba(5, 8, 13, .58);
      font-size: 12px;
      white-space: nowrap;
      backdrop-filter: blur(12px);
    }
    .atlas-detail {
      display: grid;
      gap: 8px;
    }
    .atlas-detail .metric-grid {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 8px;
    }
    .atlas-detail .metric {
      border: 1px solid var(--line);
      border-radius: 12px;
      padding: 10px;
      background: rgba(7, 10, 15, .36);
    }
    .atlas-detail .metric b {
      display: block;
      font-size: 20px;
    }
    .atlas-detail .metric span {
      color: var(--muted);
      font-size: 12px;
    }
    .atlas-breadcrumbs {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }
    .atlas-breadcrumbs button {
      min-height: 30px;
      padding: 0 10px;
      border-radius: 999px;
      background: rgba(9, 14, 21, .72);
      border-color: var(--line);
      color: var(--muted);
      font-size: 12px;
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
    .compact-project-list {
      max-height: 260px;
      overflow: auto;
      display: grid;
      gap: 8px;
    }
    .compact-project {
      display: grid;
      gap: 5px;
      padding: 10px;
      border: 1px solid var(--line);
      border-radius: 12px;
      background: rgba(7, 10, 15, .35);
      text-align: left;
      color: var(--text);
    }
    .compact-project.active {
      border-color: rgba(112, 220, 192, .72);
      box-shadow: 0 0 0 1px rgba(112, 220, 192, .16);
    }
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
      .project-layout { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <canvas id="atlas-bg" aria-hidden="true"></canvas>
  <div class="app shell">
    <header class="topbar">
      <button id="settings-open" class="corner-btn" data-tip="Settings" type="button" aria-label="Settings">
        <svg viewBox="0 0 24 24">
          <circle cx="12" cy="12" r="3.2"/>
          <line x1="12" y1="2" x2="12" y2="6"/>
          <line x1="12" y1="18" x2="12" y2="22"/>
          <line x1="2" y1="12" x2="6" y2="12"/>
          <line x1="18" y1="12" x2="22" y2="12"/>
          <line x1="4.9" y1="4.9" x2="7.8" y2="7.8"/>
          <line x1="16.2" y1="16.2" x2="19.1" y2="19.1"/>
          <line x1="4.9" y1="19.1" x2="7.8" y2="16.2"/>
          <line x1="16.2" y1="7.8" x2="19.1" y2="4.9"/>
        </svg>
      </button>
      <div class="drawer" id="drawer">
        <div class="drawer-card" id="drawer-card">
          <div class="d-name" id="drawer-name">Librarian</div>
          <div class="d-context" id="context-line">GLOBAL CONVERSATION</div>
          <div class="drawer-extra">
            <div class="d-slogan-inner" id="motto-line">Smart. Silent. Steady.</div>
            <div class="d-row"><div class="d-row-label">PATH</div><div class="d-row-value" id="drawer-path">Global conversation</div></div>
            <div class="d-row"><div class="d-row-label">SESSION</div><div class="d-row-value" id="drawer-session">new chat</div></div>
            <div class="d-row"><div class="d-row-label">MEMORY</div><div class="d-row-value" id="drawer-memory">loading</div></div>
            <div class="d-row"><div class="d-row-label">WORKER</div><div class="d-row-value" id="drawer-worker">loading</div></div>
            <div class="d-row"><div class="d-row-label">PROVIDER</div><div class="d-row-value" id="drawer-provider">Codex CLI</div></div>
          </div>
        </div>
      </div>
      <button id="new-chat" class="corner-btn" data-tip="New chat" type="button" aria-label="New chat">+</button>
      <button id="projects-open" class="corner-btn" data-tip="Library" type="button" aria-label="Library">
        <svg viewBox="0 0 24 24">
          <circle cx="12" cy="12" r="2.6" fill="currentColor" stroke="none"/>
          <circle cx="4" cy="5" r="1.8"/>
          <circle cx="20" cy="5" r="1.8"/>
          <circle cx="4" cy="19" r="1.8"/>
          <circle cx="20" cy="19" r="1.8"/>
          <line x1="12" y1="12" x2="5.4" y2="5.8"/>
          <line x1="12" y1="12" x2="18.6" y2="5.8"/>
          <line x1="12" y1="12" x2="5.4" y2="18.2"/>
          <line x1="12" y1="12" x2="18.6" y2="18.2"/>
        </svg>
      </button>
    </header>
    <main id="chat-log" class="chat-log">
      <div id="thread" class="thread">
        <article class="message assistant">Ready. Write what you want Librarian to do.</article>
      </div>
    </main>
    <form id="chat-form" class="composer" autocomplete="off">
      <div id="slash-palette" class="slash-palette" role="listbox" aria-label="Slash commands"></div>
      <div class="composer-inner">
        <div class="composer-tools">
          <button class="composer-tool" type="button" id="composer-slash">Commands</button>
          <button class="composer-tool" type="button" id="composer-context">Context</button>
          <button class="composer-tool" type="button" id="composer-library">Library</button>
        </div>
        <textarea id="goal-input" name="goal" placeholder="Message Librarian" autocomplete="off" required></textarea>
        <button class="send-button" type="submit" aria-label="Send message" title="Send"></button>
      </div>
    </form>
  </div>

  <section id="settings-overlay" class="overlay" aria-hidden="true">
    <button class="close-btn" type="button" data-close="settings-overlay" aria-label="Close settings">
      <svg viewBox="0 0 24 24"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
    </button>
    <div class="settings-frame">
      <div class="settings-head"><h2>SETTINGS</h2></div>
      <nav class="settings-tabs">
        <button class="tab-button active" type="button" data-tab="overview">Overview</button>
        <button class="tab-button" type="button" data-tab="chats">Chats</button>
        <button class="tab-button" type="button" data-tab="library">Library</button>
        <button class="tab-button" type="button" data-tab="providers">Providers</button>
        <button class="tab-button" type="button" data-tab="jobs">Jobs</button>
        <button class="tab-button" type="button" data-tab="prompt">Prompt</button>
        <button class="tab-button" type="button" data-tab="system">System</button>
      </nav>
      <div class="settings-body tab-content">
        <section class="tab-pane active" data-pane="overview"><h2>Overview</h2><div id="overview" class="grid"></div></section>
        <section class="tab-pane" data-pane="chats"><h2>Chats</h2><div id="chat-sessions" class="stack"></div></section>
        <section class="tab-pane" data-pane="library"><h2>Library</h2><div id="library-settings" class="stack"></div></section>
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
      <div class="overlay-title">Knowledge Atlas</div>
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
        contextScope: 'subtree',
        chatSessionId: null,
        atlasPath: [''],
        atlasSelectedPath: '',
        atlasBackStack: [],
        atlasForwardStack: [],
        atlasHit: null,
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
        ['/mem cleanup-legacy-local-responder', 'Clean old placeholder memory replies'],
        ['/approval list', 'Review pending approvals'],
        ['/prompt blocks', 'List prompt blocks'],
        ['/prompt export-presets ', 'Export prompt blocks as portable JSON'],
        ['/prompt import-presets ', 'Import portable prompt preset JSON'],
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
        article.querySelector('[data-context-decision="approve"]').addEventListener('click', async () => {
          if (ui?.approval?.id) {
            const response = await fetch(`/api/approvals/${encodeURIComponent(ui.approval.id)}/approve`, { method: 'POST' });
            const data = await response.json();
            if (!response.ok) {
              appendMessage('system', `Could not switch context: ${data.error || response.status}`, 'Context');
              return;
            }
          }
          state.activeContext = nodes;
          state.contextScope = context.scope || state.contextScope || 'subtree';
          state.activeProject = nodes[0]?.name || '';
          renderContext();
          appendMessage('system', `Context switched to ${label}.`, 'Context');
          article.querySelectorAll('[data-context-decision]').forEach(button => button.disabled = true);
        });
        article.querySelector('[data-context-decision="reject"]').addEventListener('click', async () => {
          if (ui?.approval?.id) {
            await fetch(`/api/approvals/${encodeURIComponent(ui.approval.id)}/reject`, { method: 'POST' }).catch(() => {});
          }
          appendMessage('system', 'Context suggestion dismissed.', 'Context');
          article.querySelectorAll('[data-context-decision]').forEach(button => button.disabled = true);
        });
        el('chat-log').scrollTop = el('chat-log').scrollHeight;
      }
      function agentJobFromUi(ui) {
        return ui?.job || (Array.isArray(ui?.jobs) ? ui.jobs[0] : null) || null;
      }
      function agentJobStatus(job) {
        return job?.status || job?.state || job?.status_label || 'Queued';
      }
      function setAgentActionCard(article, ui, fallbackText, detail) {
        const job = agentJobFromUi(ui);
        const jobId = ui?.job_id || job?.id || '';
        const command = ui?.command || 'agent';
        const status = ui?.status || (job ? agentJobStatus(job) : 'ready');
        const project = ui?.project || job?.project_name || job?.project || '';
        const goal = ui?.goal || job?.goal || fallbackText || '';
        const title = command === 'launch' || command === 'queue'
          ? 'Background agent queued'
          : command === 'preflight'
            ? 'Agent preflight'
            : command === 'list'
              ? 'Agent jobs'
              : 'Agent action';
        const rows = [];
        if (project) rows.push(`Project: ${project}`);
        if (jobId) rows.push(`Job: ${jobId}`);
        if (goal) rows.push(`Goal: ${goal}`);
        if (Array.isArray(ui?.jobs)) rows.push(`Jobs shown: ${ui.jobs.length}`);
        article.className = 'message assistant agent-card';
        article.innerHTML = `
          <div class="approval-head"><span>${htmlEscape(title)}</span><span class="approval-risk">${htmlEscape(status)}</span></div>
          <div class="approval-summary">${htmlEscape(goal || fallbackText || 'Agent command completed.')}</div>
          <div class="approval-paths">${rows.length ? rows.map(row => `<div>${htmlEscape(row)}</div>`).join('') : '<div>No job metadata returned.</div>'}</div>
          <div class="approval-actions">
            ${jobId ? '<button type="button" data-agent-events>Refresh events</button>' : ''}
            ${jobId ? '<button type="button" data-agent-preflight>Preflight</button>' : ''}
          </div>
          <div class="agent-event-list" data-agent-event-list hidden></div>
          <details><summary>Technical details</summary><pre>${htmlEscape(JSON.stringify(ui, null, 2))}</pre></details>
        `;
        if (detail) {
          const small = document.createElement('small');
          small.textContent = detail;
          article.appendChild(small);
        }
        const eventList = article.querySelector('[data-agent-event-list]');
        async function loadAgentEvents(runPreflight) {
          if (!jobId) return;
          if (runPreflight) {
            await fetch(`/api/jobs/${encodeURIComponent(jobId)}/preflight`, { method: 'POST' }).catch(() => {});
          }
          const events = await loadJson(`/api/jobs/${encodeURIComponent(jobId)}/events`, []);
          eventList.hidden = false;
          const recent = Array.isArray(events) ? events.slice(-5).reverse() : [];
          eventList.innerHTML = recent.length
            ? recent.map(event => `<div>${htmlEscape(`${event.created_at || ''} ${event.kind || 'event'} ${JSON.stringify(event.payload || {})}`)}</div>`).join('')
            : '<div>No events yet.</div>';
          el('chat-log').scrollTop = el('chat-log').scrollHeight;
        }
        article.querySelector('[data-agent-events]')?.addEventListener('click', () => loadAgentEvents(false));
        article.querySelector('[data-agent-preflight]')?.addEventListener('click', () => loadAgentEvents(true));
        el('chat-log').scrollTop = el('chat-log').scrollHeight;
      }
      function setJobReviewCard(article, ui, fallbackText, detail) {
        const packet = ui?.packet || {};
        const summary = packet.summary || {};
        const project = packet.project || {};
        const jobId = ui?.job_id || packet.job_id || '';
        const review = packet.review || {};
        const git = review.git || {};
        const commitGate = packet.commit_gate || {};
        const revertPlan = packet.revert_plan || {};
        const blockers = Array.isArray(commitGate.blockers) ? commitGate.blockers : [];
        const nextStep = summary.next_step || 'inspect_packet';
        const status = blockers.length ? 'blocked' : 'ready';
        const rows = [
          `Project: ${project.name || ui?.project || '-'}`,
          `Job: ${jobId || '-'}`,
          `Recommendation: ${summary.review_recommendation || review.recommendation || '-'}`,
          `Worktree changes: ${summary.has_worktree_changes ? 'yes' : 'no'}`,
          `Commit gate: ${summary.commit_allowed ? 'allowed' : 'blocked'}`,
          `Push plan: ${summary.push_allowed ? 'available' : 'not ready'}`
        ];
        const statusText = git.status?.stdout || '';
        const diffText = git.diff_stat?.stdout || '';
        article.className = 'message assistant agent-card review-card';
        article.innerHTML = `
          <div class="approval-head"><span>Job review packet</span><span class="approval-risk">${htmlEscape(status)}</span></div>
          <div class="approval-summary">${htmlEscape(fallbackText || `Next step: ${nextStep}`)}</div>
          <div class="approval-paths">${rows.map(row => `<div>${htmlEscape(row)}</div>`).join('')}</div>
          ${blockers.length ? `<div class="approval-paths">${blockers.map(row => `<div>${htmlEscape(row)}</div>`).join('')}</div>` : '<div class="muted tiny">No commit gate blockers.</div>'}
          <div class="approval-actions">
            ${jobId ? `<button type="button" data-review-propose="commit" ${summary.commit_allowed ? '' : 'disabled'}>Propose commit</button>` : ''}
            ${jobId ? `<button type="button" class="reject" data-review-propose="revert" ${revertPlan.allowed ? '' : 'disabled'}>Propose revert</button>` : ''}
            ${jobId ? '<button type="button" data-agent-events>Refresh events</button>' : ''}
            ${jobId ? '<button type="button" data-agent-preflight>Preflight</button>' : ''}
          </div>
          <div class="approval-status" hidden></div>
          <details ${statusText ? 'open' : ''}><summary>Git status</summary><pre>${htmlEscape(statusText || 'clean')}</pre></details>
          <details><summary>Diff summary</summary><pre>${htmlEscape(diffText || 'no diff')}</pre></details>
          <details><summary>Technical details</summary><pre>${htmlEscape(JSON.stringify(packet, null, 2))}</pre></details>
        `;
        if (detail) {
          const small = document.createElement('small');
          small.textContent = detail;
          article.appendChild(small);
        }
        const eventList = document.createElement('div');
        eventList.className = 'agent-event-list';
        eventList.hidden = true;
        article.appendChild(eventList);
        async function loadReviewEvents(runPreflight) {
          if (!jobId) return;
          if (runPreflight) {
            await fetch(`/api/jobs/${encodeURIComponent(jobId)}/preflight`, { method: 'POST' }).catch(() => {});
          }
          const events = await loadJson(`/api/jobs/${encodeURIComponent(jobId)}/events`, []);
          eventList.hidden = false;
          const recent = Array.isArray(events) ? events.slice(-5).reverse() : [];
          eventList.innerHTML = recent.length
            ? recent.map(event => `<div>${htmlEscape(`${event.created_at || ''} ${event.kind || 'event'} ${JSON.stringify(event.payload || {})}`)}</div>`).join('')
            : '<div>No events yet.</div>';
          el('chat-log').scrollTop = el('chat-log').scrollHeight;
        }
        async function proposeReviewAction(action) {
          if (!jobId) return;
          const buttons = Array.from(article.querySelectorAll('[data-review-propose]'));
          const statusLine = article.querySelector('.approval-status');
          buttons.forEach(button => button.disabled = true);
          statusLine.hidden = false;
          statusLine.textContent = action === 'commit' ? 'Preparing commit proposal...' : 'Preparing revert proposal...';
          try {
            const body = { action };
            if (action === 'commit') {
              const defaultMessage = `Librarian job ${shortId(jobId)} changes`;
              const message = prompt('Commit message', defaultMessage);
              if (!message) {
                statusLine.textContent = 'Commit proposal cancelled.';
                buttons.forEach(button => button.disabled = false);
                return;
              }
              body.message = message;
            }
            if (action === 'revert') {
              const defaultCommit = revertPlan.target_commit || '';
              const commit = prompt('Commit to revert', defaultCommit);
              if (!commit) {
                statusLine.textContent = 'Revert proposal cancelled.';
                buttons.forEach(button => button.disabled = false);
                return;
              }
              body.commit = commit;
            }
            const response = await fetch(`/api/jobs/${encodeURIComponent(jobId)}/git-action-proposal`, {
              method: 'POST',
              headers: { 'content-type': 'application/json' },
              body: JSON.stringify(body)
            });
            const data = await response.json();
            if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
            setApprovalCard(article, data.approval, data.approval?.payload?.summary || `${action} proposal ready.`, detail);
          } catch (error) {
            statusLine.textContent = `Could not create ${action} proposal: ${error.message || error}`;
            buttons.forEach(button => button.disabled = false);
          }
        }
        article.querySelectorAll('[data-review-propose]').forEach(button => {
          button.addEventListener('click', () => proposeReviewAction(button.dataset.reviewPropose));
        });
        article.querySelector('[data-agent-events]')?.addEventListener('click', () => loadReviewEvents(false));
        article.querySelector('[data-agent-preflight]')?.addEventListener('click', () => loadReviewEvents(true));
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
        article.innerHTML = renderMessageContent(text);
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
      function renderMessageContent(text) {
        const source = String(text ?? '');
        const blocks = [];
        let escaped = htmlEscape(source).replace(/```([\s\S]*?)```/g, (_, code) => {
          const key = `@@CODE${blocks.length}@@`;
          blocks.push(`<pre><code>${code.trim()}</code></pre>`);
          return key;
        });
        escaped = escaped
          .split(/\n{2,}/)
          .map(block => {
            const lines = block.split('\n');
            if (lines.every(line => line.trim().startsWith('&gt;'))) {
              return `<blockquote>${lines.map(line => line.replace(/^\s*&gt;\s?/, '')).join('<br>')}</blockquote>`;
            }
            if (lines.length && lines.every(line => /^\s*(?:[-*]\s+|\d+\.\s+)/.test(line))) {
              return `<p>${lines.join('<br>')}</p>`;
            }
            return `<p>${lines.join('<br>')}</p>`;
          })
          .join('');
        escaped = escaped.replace(/https?:\/\/[^\s<]+/g, url => `<a href="${url}" target="_blank" rel="noreferrer">${url}</a>`);
        blocks.forEach((block, index) => {
          escaped = escaped.replace(`@@CODE${index}@@`, block);
        });
        return escaped || '<p></p>';
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
          active.context_path || active.library_path || state.projects.some(project => project.id === active.id || project.name === active.name)
        );
        renderOverview();
        renderChatSessions();
        renderLibrarySettings();
        renderProviders();
        renderJobs();
        renderPromptBuilder();
        renderSystemEvents(events);
        if (el('projects-overlay').classList.contains('open')) renderProjects();
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
          } else if (turn.role === 'assistant' && turn.metadata?.ui?.type === 'context_switch') {
            setContextSwitchCard(article, turn.metadata.ui, assistantName());
          } else if (turn.role === 'assistant' && turn.metadata?.ui?.type === 'agent_action') {
            setAgentActionCard(article, turn.metadata.ui, turn.content, assistantName());
          } else if (turn.role === 'assistant' && turn.metadata?.ui?.type === 'job_review') {
            setJobReviewCard(article, turn.metadata.ui, turn.content, assistantName());
          }
        }
        if (!transcript.turns.length && announce) {
          appendMessage('system', `Restored empty chat session ${shortId(state.chatSessionId)}.`);
        } else if (announce) {
          appendMessage('system', `Restored chat session ${shortId(state.chatSessionId)}.`);
        }
      }
      function renderContext() {
        el('drawer-name').textContent = assistantName();
        const label = currentContextLabel();
        el('context-line').textContent = label.toUpperCase();
        el('drawer-path').innerHTML = htmlEscape(label).replace(/\s\+\s/g, ' <span class="sep">+</span> ');
        el('drawer-session').textContent = state.chatSessionId ? `chat-${shortId(state.chatSessionId)}` : 'new chat';
        const memory = state.health?.memory || {};
        el('drawer-memory').innerHTML = `${memory.items ?? 0} items <span class="sep">·</span> ${memory.embedded_items ?? 0} embedded`;
        const worker = state.health?.worker || {};
        el('drawer-worker').innerHTML = `${worker.running_jobs ?? 0} running <span class="sep">·</span> ${worker.queued_jobs ?? 0} queued`;
        const ready = (state.providers.diagnostics || []).find(item => item.provider === 'codex')?.status || 'Codex CLI';
        el('drawer-provider').textContent = ready;
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
          renderPermissionSettings(),
          card('Memory', `items=${memory.items ?? 0}<br>embedded=${memory.embedded_items ?? 0}<br>missing=${memory.missing_embeddings ?? 0}`),
          card('Knowledge base', `${htmlEscape(health.vault_path || 'Library')}<br><span class="muted">${htmlEscape(health.database_path || '.mdb/librarian.db')}</span>`),
          card('Secrets', `${htmlEscape(secrets.status || 'unknown')}<br><span class="muted">${htmlEscape(secrets.location || '')}</span>`)
        ].join('');
        const chatForm = el('chat-settings-form');
        if (chatForm) chatForm.addEventListener('submit', saveChatSettings);
        const permissionsForm = el('tool-permissions-form');
        if (permissionsForm) {
          qsa('[data-permission]').forEach(select => select.addEventListener('change', () => { el('permission-preset').value = 'custom'; }));
          permissionsForm.addEventListener('submit', saveToolPermissions);
        }
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
      function policySelect(name, value) {
        return `<select id="perm-${name}" data-permission="${name}">
          ${['auto', 'ask', 'deny'].map(option => `<option value="${option}" ${option === value ? 'selected' : ''}>${option}</option>`).join('')}
        </select>`;
      }
      function renderPermissionSettings() {
        const permissions = state.health?.tool_permissions || {};
        const keys = [
          'library_read', 'library_create', 'library_edit_markdown', 'library_move', 'library_delete',
          'workspace_create', 'workspace_move', 'workspace_delete',
          'memory_write', 'settings_change', 'agent_launch', 'context_switch'
        ];
        return `<form id="tool-permissions-form" class="card stack">
          <h3>Tool Permissions</h3>
          <div class="form-grid">
            <div><label for="permission-preset">Preset</label><select id="permission-preset">
              ${['balanced', 'autopilot', 'confirm', 'locked_down', 'custom'].map(option => `<option value="${option}" ${option === (permissions.preset || 'balanced') ? 'selected' : ''}>${option}</option>`).join('')}
            </select></div>
            ${keys.map(key => `<div><label for="perm-${key}">${key}</label>${policySelect(key, permissions[key] || 'ask')}</div>`).join('')}
            <button type="submit">Save</button>
          </div>
        </form>`;
      }
      async function saveToolPermissions(event) {
        event.preventDefault();
        const body = { preset: el('permission-preset').value };
        qsa('[data-permission]').forEach(select => { body[select.dataset.permission] = select.value; });
        await postJson('/api/settings/tool-permissions', body);
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
      function renderLibrarySettings() {
        const linkedCount = state.projectMap?.linked_project_count ?? 0;
        const detachedCount = Array.isArray(state.projectMap?.detached_projects) ? state.projectMap.detached_projects.length : 0;
        const projectCards = state.projects.length ? state.projects.map(project => {
          const active = currentContextProjects().some(active => active.id === project.id || active.name === project.name) ? ' active' : '';
          return `<div class="compact-project${active}">
            <strong>${htmlEscape(projectDisplayName(project) || project.name)}</strong>
            <div class="muted tiny">Knowledge: ${htmlEscape(project.library_path || '-')}</div>
            <div class="muted tiny">Workspace: ${htmlEscape(project.path || '-')}</div>
            <div class="row">
              <button type="button" class="secondary" data-project-context="${htmlEscape(project.name)}">Use context</button>
              <button type="button" class="secondary" data-attach-library="${htmlEscape(project.id)}">Attach knowledge</button>
              <button type="button" class="secondary" data-attach-workspace="${htmlEscape(project.id)}">Attach workspace</button>
            </div>
          </div>`;
        }).join('') : '<div class="card muted">No registered workspaces yet. Use the form below or slash commands such as /project create.</div>';
        el('library-settings').innerHTML = `
          <div class="grid">
            ${card('Knowledge tree', `linked=${linkedCount}<br>detached=${detachedCount}<br><span class="muted">Open the Library atlas to choose any node as chat context.</span>`)}
            ${card('Context scope', `${htmlEscape(state.contextScope || 'subtree')}<br><span class="muted">Default searches include the selected node and descendants.</span>`)}
          </div>
          <form id="project-create-form" class="card stack">
            <h3>Create project</h3>
            <div class="form-grid">
              <div><label for="project-name">Name</label><input id="project-name" required placeholder="My Project"></div>
              <div><label for="project-library-path">Knowledge path</label><input id="project-library-path" placeholder="Projects/MyProject"></div>
              <div class="wide"><label for="project-workspace-path">Existing workspace path</label><input id="project-workspace-path" placeholder="optional existing directory"></div>
              <button type="submit">Create</button>
            </div>
          </form>
          <section class="card stack">
            <h3>Registered workspaces</h3>
            <div class="compact-project-list">${projectCards}</div>
          </section>`;
        wireProjectForms();
        qsa('[data-project-context]').forEach(button => button.addEventListener('click', () => {
          const project = state.projects.find(project => project.name === button.dataset.projectContext);
          if (!project) return;
          state.activeProject = project.name;
          state.activeContext = [contextNodeFromMetadata(project)];
          state.contextScope = 'subtree';
          state.chatSessionId = null;
          renderContext();
          renderLibrarySettings();
          appendMessage('system', `Context set to ${projectDisplayName(project) || project.name}.`);
        }));
      }
      function renderProviders() {
        const states = new Map((state.providers.states || []).map(item => [`${item.provider}:${item.model || ''}`, item]));
        const models = state.providers.catalog || [];
        const runtime = state.providers.runtime || {};
        const diagnostics = new Map((state.providers.diagnostics || []).map(item => [item.provider, item]));
        const commands = state.providers.commands || {};
        const providerOrder = ['codex', 'claude-code', 'openrouter'];
        const modelsByProvider = new Map();
        for (const model of models) {
          const list = modelsByProvider.get(model.provider) || [];
          list.push(model);
          modelsByProvider.set(model.provider, list);
          if (!providerOrder.includes(model.provider)) providerOrder.push(model.provider);
        }
        function providerLabel(provider) {
          if (provider === 'codex') return 'Codex';
          if (provider === 'claude-code') return 'Claude Code';
          if (provider === 'openrouter') return 'OpenRouter';
          return provider;
        }
        function providerAuthKey(provider) {
          if (provider === 'codex') return 'codex';
          if (provider === 'claude-code') return 'claude';
          return '';
        }
        function runtimeSummary(providerRuntime) {
          if (!providerRuntime || !Object.keys(providerRuntime).length) {
            return '<div class="muted tiny">No local runtime profile required.</div>';
          }
          return `
            <div class="muted tiny">Host profile: ${htmlEscape(providerRuntime.host_home || '-')}</div>
            <div class="muted tiny">Mount: ${providerRuntime.mount_host_home ? 'enabled' : 'disabled'}${providerRuntime.mount_read_only ? ' - read-only' : ''}${providerRuntime.host_home_exists === false ? ' - missing profile' : ''}</div>
            ${providerRuntime.instruction_file ? `<div class="muted tiny">Instruction file: ${htmlEscape(providerRuntime.instruction_file)}</div>` : ''}
          `;
        }
        function diagnosticSummary(diagnostic, current) {
          const level = diagnostic.level || 'muted';
          const status = diagnostic.status || current.status || 'Unknown';
          const detail = diagnostic.detail ? `<div class="muted tiny">${htmlEscape(diagnostic.detail)}</div>` : '';
          const next = diagnostic.next_step ? `<details><summary>Next step</summary><pre>${htmlEscape(diagnostic.next_step)}</pre></details>` : '';
          return `<span class="pill ${htmlEscape(level)}">${htmlEscape(status)}</span>${detail}${next}`;
        }
        function renderRuntimeForm(provider) {
          if (provider === 'codex') {
            const codex = runtime.codex || {};
            return `<form id="codex-runtime-form" class="card stack provider-runtime">
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
          }
          if (provider === 'claude-code') {
            const claude = runtime['claude-code'] || {};
            return `<form id="claude-runtime-form" class="card stack provider-runtime">
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
          }
          return '';
        }
        function renderProviderGroup(provider) {
          const providerModels = modelsByProvider.get(provider) || [];
          const providerRuntime = runtime[provider] || {};
          const diagnostic = diagnostics.get(provider) || {};
          const authKey = providerAuthKey(provider);
          const providerStates = providerModels.map(model => {
            const current = states.get(`${provider}:${model.model}`) || states.get(`${provider}:`) || {};
            return `<div class="compact-project">
              <strong>${htmlEscape(model.model || 'default')}</strong>
              <div class="muted tiny">${(model.task_hints || []).map(hint => htmlEscape(hint)).join(' - ') || 'general'}</div>
              <div class="muted tiny">Pricing: ${htmlEscape(model.pricing_kind || 'unknown')} · ${htmlEscape(model.pricing_source || '-')}</div>
              ${model.pricing_note ? `<details><summary>Pricing note</summary><pre>${htmlEscape(model.pricing_note)}</pre></details>` : ''}
              ${diagnosticSummary(diagnostic, current)}
            </div>`;
          }).join('') || '<div class="muted tiny">No model catalog entry yet.</div>';
          return `<section class="card stack provider-group">
            <div class="row">
              <h3>${htmlEscape(providerLabel(provider))}</h3>
              <div class="row">
                ${authKey ? `<button type="button" class="secondary" data-provider-command="${authKey}">Auth</button>` : ''}
                <button type="button" class="secondary" data-provider-smoke="${htmlEscape(provider)}">Smoke</button>
              </div>
            </div>
            ${runtimeSummary(providerRuntime)}
            <div class="compact-project-list">${providerStates}</div>
            ${renderRuntimeForm(provider)}
          </section>`;
        }
        const providerTools = `<div class="card">
          <h3>Provider Setup</h3>
          <div class="muted tiny">Auth still opens in the host shell. Smoke buttons on provider cards run local MVP preflight without a real agent call.</div>
          <div class="provider-actions">
            <button type="button" class="secondary" data-provider-command="codex">Codex auth command</button>
            <button type="button" class="secondary" data-provider-command="claude">Claude auth command</button>
            <button type="button" class="secondary" data-provider-command="image">Build image</button>
          </div>
        </div>`;
        const groups = providerOrder.map(renderProviderGroup).join('');
        el('providers').innerHTML = providerTools + (groups || '<div class="card muted">No providers reported.</div>');
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
            'smoke-claude': commands.smoke_claude,
            'smoke-openrouter': commands.smoke_openrouter
          }[key] || 'Command is not available yet.';
          appendMessage('system', command, 'Provider command');
        }));
        qsa('[data-provider-smoke]').forEach(button => button.addEventListener('click', () => runProviderSmoke(button.dataset.providerSmoke, button)));
      }
      async function runProviderSmoke(provider, button) {
        const original = button.textContent;
        button.disabled = true;
        button.textContent = 'Running...';
        try {
          const response = await fetch(`/api/providers/${encodeURIComponent(provider)}/smoke`, { method: 'POST' });
          const data = await response.json();
          if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
          const output = [
            `Provider smoke: ${data.provider}`,
            `Status: ${data.success ? 'passed' : 'failed'}${data.status === null || data.status === undefined ? '' : ` (${data.status})`}`,
            '',
            data.stdout || '',
            data.stderr ? `stderr:\n${data.stderr}` : ''
          ].filter(Boolean).join('\n');
          appendMessage(data.success ? 'system' : 'error', output, 'Provider smoke');
        } catch (error) {
          appendMessage('error', `Provider smoke failed: ${error.message || error}`, 'Provider smoke');
        } finally {
          button.disabled = false;
          button.textContent = original;
        }
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
              <button type="button" class="secondary" data-job-review-packet="${htmlEscape(job.id)}">Review packet</button>
              <button type="button" class="secondary" data-job-retry="${htmlEscape(job.id)}">Retry</button>
              <button type="button" class="danger" data-job-cancel="${htmlEscape(job.id)}">Cancel</button>
            </div>
            <div id="job-events-${htmlEscape(job.id)}" class="stack"></div>
          </div>`;
        }).join('') : '<div class="card muted">No jobs yet.</div>';
        qsa('[data-job-events]').forEach(button => button.addEventListener('click', () => showJobEvents(button.dataset.jobEvents)));
        qsa('[data-job-preflight]').forEach(button => button.addEventListener('click', () => runJobAction(button.dataset.jobPreflight, 'preflight')));
        qsa('[data-job-review-packet]').forEach(button => button.addEventListener('click', () => showJobReviewPacket(button.dataset.jobReviewPacket)));
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
      async function showJobReviewPacket(id) {
        const target = el(`job-events-${id}`);
        if (!target) return;
        target.innerHTML = '<div class="muted tiny">Building review packet...</div>';
        const response = await fetch(`/api/jobs/${encodeURIComponent(id)}/review-packet`, { method: 'POST' });
        const packet = await response.json().catch(() => ({}));
        if (!response.ok) {
          target.innerHTML = `<div class="card action"><b>Review packet failed</b><br>${htmlEscape(packet.error || `HTTP ${response.status}`)}</div>`;
          return;
        }
        const summary = packet.summary || {};
        const rows = [
          `Next: ${summary.next_step || 'inspect_packet'}`,
          `Changes: ${summary.has_worktree_changes ? 'yes' : 'no'}`,
          `Commit: ${summary.commit_allowed ? 'allowed' : 'blocked'}`,
          `Revert: ${summary.revert_allowed ? 'available' : 'blocked'}`,
          `Push: ${summary.push_allowed ? 'ready for manual review' : 'blocked'}`
        ];
        const blockers = [
          ...(packet.commit_gate?.blockers || []).map(value => `commit: ${value}`),
          ...(packet.revert_plan?.blockers || []).map(value => `revert: ${value}`),
          ...(packet.push_plan?.blockers || []).map(value => `push: ${value}`)
        ];
        target.innerHTML = `<div class="card action">
          <b>Review packet</b> <span class="muted tiny">${htmlEscape(packet.project?.name || '')}</span>
          <div class="approval-paths">${rows.map(row => `<div>${htmlEscape(row)}</div>`).join('')}</div>
          ${blockers.length ? `<div class="approval-paths">${blockers.map(row => `<div>${htmlEscape(row)}</div>`).join('')}</div>` : '<div class="muted tiny">No gate blockers.</div>'}
          <details><summary>Technical details</summary><pre>${htmlEscape(JSON.stringify(packet, null, 2))}</pre></details>
        </div>`;
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
        const profiles = `<section class="card stack">
          <h3>Prompt Profiles</h3>
          <div class="form-grid">
            <div><label for="prompt-active-target">Profile</label><select id="prompt-active-target">${targetOptions}</select></div>
            <button type="button" data-render-active-prompt>Preview</button>
            <button type="button" class="secondary" data-export-json-prompt>Export JSON</button>
            <button type="button" class="secondary" data-export-active-prompt>Export to Library</button>
            <div class="wide"><label for="prompt-import-json">Import preset JSON</label><textarea id="prompt-import-json" rows="5" placeholder='{"schema":"librarian.prompt-presets.v1","blocks":[]}'></textarea></div>
            <button type="button" data-import-json-prompt>Import JSON</button>
          </div>
        </section>`;
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
        el('prompt-builder').innerHTML = `${profiles}${form}<div id="prompt-preview" class="card muted">Choose a profile preview.</div><div id="prompt-json" class="card muted" hidden></div>${list}`;
        el('prompt-block-form').addEventListener('submit', createPromptBlockFromUi);
        el('prompt-active-target').addEventListener('change', event => renderPromptPreview(event.target.value));
        el('prompt-active-target').value = targets.includes('librarian') ? 'librarian' : targets[0] || 'librarian';
        qsa('[data-render-active-prompt]').forEach(button => button.addEventListener('click', () => renderPromptPreview(el('prompt-active-target').value)));
        qsa('[data-export-json-prompt]').forEach(button => button.addEventListener('click', () => exportPromptJson(el('prompt-active-target').value)));
        qsa('[data-import-json-prompt]').forEach(button => button.addEventListener('click', importPromptJson));
        qsa('[data-export-active-prompt]').forEach(button => button.addEventListener('click', () => proposePromptExport(el('prompt-active-target').value)));
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
        const activeBlocks = state.promptBlocks
          .filter(block => block.target === target && block.enabled)
          .sort((a, b) => a.position - b.position);
        const disabled = state.promptBlocks.filter(block => block.target === target && !block.enabled).length;
        el('prompt-preview').innerHTML = data ? `
          <h3>${htmlEscape(target)} <span class="muted tiny">${activeBlocks.length} active · ${disabled} disabled</span></h3>
          <div class="muted tiny">Render order: ${activeBlocks.map(block => htmlEscape(block.name)).join(' -> ') || 'empty'}</div>
          <pre>${htmlEscape(data.rendered || '')}</pre>
        ` : 'Could not render prompt.';
      }
      async function exportPromptJson(target) {
        const data = await loadJson(`/api/prompt-blocks/presets?target=${encodeURIComponent(target)}`, null);
        const box = el('prompt-json');
        box.hidden = false;
        box.className = 'card';
        box.innerHTML = data ? `<h3>Portable JSON: ${htmlEscape(target)}</h3><pre>${htmlEscape(JSON.stringify(data, null, 2))}</pre>` : 'Could not export prompt presets.';
      }
      async function importPromptJson() {
        const raw = el('prompt-import-json').value.trim();
        if (!raw) {
          appendMessage('system', 'Paste prompt preset JSON before importing.', 'Prompt builder');
          return;
        }
        let document;
        try {
          document = JSON.parse(raw);
        } catch (error) {
          appendMessage('error', `Prompt JSON is invalid: ${error.message || error}`, 'Prompt builder');
          return;
        }
        const response = await fetch('/api/prompt-blocks/import-presets', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ document })
        });
        const data = await response.json().catch(() => ({}));
        if (!response.ok) {
          appendMessage('error', data.error || `Prompt import failed: ${response.status}`, 'Prompt builder');
          return;
        }
        appendMessage('system', `Imported ${Array.isArray(data.imported) ? data.imported.length : 0} prompt block(s).`, 'Prompt builder');
        await refresh();
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
        el('project-stage').innerHTML = `<div class="project-layout">
          <section class="atlas-panel">
            <canvas id="neural-atlas" class="atlas-canvas" aria-label="Knowledge atlas"></canvas>
            <div class="atlas-stamp">Neural Atlas В· v1</div>
            <div class="atlas-help">Neuron - zoom В· core - back В· prima - root В· synapse - select</div>
          </section>
        </div>`;
        renderNeuralAtlas();
      }
      function projectMapRoot() {
        return state.projectMap?.root || { name: 'Knowledge Base', path: '', kind: 'Folder', visual_kind: 'rack', children: [], projects: [] };
      }
      function atlasNodePath(node) {
        return String(node?.path || '');
      }
      function projectMapNodeLabel(node) {
        if (!node) return 'Knowledge Base';
        const name = node.name || node.path || 'Knowledge Base';
        return humanProjectName(name) || 'Knowledge Base';
      }
      function atlasFindNode(path, node = projectMapRoot()) {
        if (atlasNodePath(node) === String(path || '')) return node;
        for (const child of (node.children || [])) {
          const found = atlasFindNode(path, child);
          if (found) return found;
        }
        return null;
      }
      function currentAtlasNode() {
        return atlasFindNode(state.atlasPath[state.atlasPath.length - 1]) || projectMapRoot();
      }
      function selectedAtlasNode() {
        return atlasFindNode(state.atlasSelectedPath) || currentAtlasNode();
      }
      function atlasBreadcrumbNodes() {
        return state.atlasPath.map(path => atlasFindNode(path)).filter(Boolean);
      }
      function atlasStats(node) {
        const stats = { folders: 0, files: 0, projects: 0, depth: Math.max(0, state.atlasPath.length - 1) };
        function walk(entry) {
          for (const project of (entry.projects || [])) {
            if (project) stats.projects += 1;
          }
          for (const child of (entry.children || [])) {
            const kind = String(child.visual_kind || child.kind || '').toLowerCase();
            if (kind === 'book' || kind === 'markdown' || kind === 'file' || kind === 'artifact') stats.files += 1;
            else stats.folders += 1;
            walk(child);
          }
        }
        if (node) walk(node);
        return stats;
      }
      function atlasNavigateTo(path, pushHistory) {
        const node = atlasFindNode(path);
        if (!node) return;
        const currentPath = atlasNodePath(currentAtlasNode());
        if (pushHistory && currentPath !== atlasNodePath(node)) {
          state.atlasBackStack.push(currentPath);
          state.atlasForwardStack = [];
        }
        const trail = [];
        function collect(entry, target, acc) {
          const next = acc.concat(atlasNodePath(entry));
          if (atlasNodePath(entry) === target) {
            trail.push(...next);
            return true;
          }
          return (entry.children || []).some(child => collect(child, target, next));
        }
        collect(projectMapRoot(), atlasNodePath(node), []);
        state.atlasPath = trail.length ? trail : [''];
        state.atlasSelectedPath = atlasNodePath(node);
        renderProjects();
      }
      function atlasContextFromNode(node) {
        const selected = node || selectedAtlasNode();
        const linkedProject = Array.isArray(selected.projects) && selected.projects.length ? selected.projects[0] : null;
        if (linkedProject) return contextNodeFromMetadata({ ...selected, project: linkedProject, context_path: selected.path, library_path: selected.path });
        return contextNodeFromMetadata({ ...selected, context_path: selected.path, library_path: selected.path });
      }
      function useAtlasContext(closeAfter) {
        const node = selectedAtlasNode();
        const context = atlasContextFromNode(node);
        state.activeContext = [context];
        state.activeProject = context.id ? (context.name || context.context_path || '') : '';
        state.contextScope = 'subtree';
        state.chatSessionId = null;
        renderContext();
        renderProjects();
        if (closeAfter) closeOverlay('projects-overlay');
      }
      function atlasBack() {
        const previous = state.atlasBackStack.pop();
        if (previous === undefined) {
          if (state.atlasPath.length > 1) atlasNavigateTo(state.atlasPath[state.atlasPath.length - 2], true);
          return;
        }
        state.atlasForwardStack.push(atlasNodePath(currentAtlasNode()));
        atlasNavigateTo(previous, false);
      }
      function atlasForward() {
        const next = state.atlasForwardStack.pop();
        if (next === undefined) return;
        state.atlasBackStack.push(atlasNodePath(currentAtlasNode()));
        atlasNavigateTo(next, false);
      }
      function renderNeuralAtlas() {
        const canvas = el('neural-atlas');
        if (!canvas) return;
        const atlas = ensureAtlasState(canvas);
        atlas.nodes = projectMapToNeuralTree(projectMapRoot());
        if (!atlas.path.length || !atlasFindNeuralNode(atlas.nodes, atlas.path[atlas.path.length - 1]?.id)) {
          atlas.path = [atlas.nodes];
          atlas.openedFile = null;
        }
        if (!atlas.raf) atlasTick();
      }
      function ensureAtlasState(canvas) {
        if (state.neuralAtlas?.canvas === canvas) return state.neuralAtlas;
        const atlas = {
          canvas,
          ctx: canvas.getContext('2d'),
          nodes: projectMapToNeuralTree(projectMapRoot()),
          path: [],
          openedFile: null,
          t: 0,
          tilt: Math.PI / 3,
          cam: { zoom: 1, tzoom: 1, focusX: 0, focusY: 0, tfocusX: 0, tfocusY: 0 },
          hits: [],
          mouse: { x: -1, y: -1 },
          transitioning: false,
          pendingPathOp: null,
          zoomTargetMoon: null,
          width: 0,
          height: 0,
          dpr: 1,
          raf: null
        };
        atlas.path = [atlas.nodes];
        state.neuralAtlas = atlas;
        canvas.onmousemove = event => {
          const point = atlasPointer(canvas, event);
          atlas.mouse = point;
          const hit = atlas.hits.find(item => atlasInside(item, point));
          canvas.style.cursor = hit ? 'pointer' : 'default';
          state.atlasHit = hit || null;
        };
        canvas.onclick = () => {
          if (atlas.transitioning) return;
          const hit = atlas.hits.find(item => atlasInside(item, atlas.mouse));
          if (!hit) return;
          if (hit.kind === 'file') {
            atlas.openedFile = hit.payload;
            state.atlasSelectedPath = hit.payload.path || hit.payload.id || '';
            return;
          }
          if (hit.kind === 'useContext') {
            const node = atlasCurrentNode();
            state.atlasSelectedPath = node?.path || '';
            useAtlasContext(true);
            return;
          }
          if (hit.kind === 'closeRoot') {
            closeOverlay('projects-overlay');
            return;
          }
          if (hit.kind === 'neuron') {
            state.atlasSelectedPath = hit.payload.path || hit.payload.id || '';
            atlas.transitioning = true;
            atlas.zoomTargetMoon = hit.payload;
            atlas.cam.tfocusX = hit.x - atlas.width / 2;
            atlas.cam.tfocusY = hit.y - atlas.height / 2;
            atlas.cam.tzoom = 4;
            window.setTimeout(() => {
              atlas.path.push(hit.payload);
              state.atlasPath = atlas.path.map(node => node.path || '');
              atlas.cam.tzoom = 1;
              atlas.cam.zoom = 1;
              atlas.cam.tfocusX = 0;
              atlas.cam.tfocusY = 0;
              atlas.cam.focusX = 0;
              atlas.cam.focusY = 0;
              atlas.transitioning = false;
              atlas.zoomTargetMoon = null;
              renderProjects();
            }, 360);
            return;
          }
          if (hit.kind === 'centerBack') {
            if (atlas.openedFile) {
              atlas.openedFile = null;
              return;
            }
            if (atlas.path.length > 1) {
              atlas.transitioning = true;
              atlas.cam.tzoom = .25;
              window.setTimeout(() => {
                atlas.path.pop();
                state.atlasPath = atlas.path.map(node => node.path || '');
                state.atlasSelectedPath = atlas.path[atlas.path.length - 1]?.path || '';
                atlas.cam.tzoom = 1;
                atlas.cam.tfocusX = 0;
                atlas.cam.tfocusY = 0;
                atlas.cam.focusX = 0;
                atlas.cam.focusY = 0;
                atlas.transitioning = false;
                renderProjects();
              }, 260);
            }
            return;
          }
          if (hit.kind === 'super') {
            atlas.path = [atlas.nodes];
            state.atlasPath = [''];
            state.atlasSelectedPath = '';
            atlas.cam.tzoom = 1;
            atlas.cam.zoom = 1;
            atlas.cam.tfocusX = 0;
            atlas.cam.tfocusY = 0;
            atlas.cam.focusX = 0;
            atlas.cam.focusY = 0;
            renderProjects();
            return;
          }
          if (hit.kind === 'close') atlas.openedFile = null;
        };
        return atlas;
      }
      function projectMapToNeuralTree(node) {
        const children = (node.children || []).map(projectMapToNeuralTree);
        const visual = String(node.visual_kind || '').toLowerCase();
        const type = visual === 'book' || visual === 'artifact' ? 'file' : 'folder';
        return {
          id: atlasNodePath(node) || 'root',
          name: projectMapNodeLabel(node),
          rawName: node.name || 'Library',
          path: atlasNodePath(node),
          kind: node.kind || node.visual_kind || 'Folder',
          type,
          body: `${projectMapNodeLabel(node)}\n${atlasNodePath(node) || '/'}\n${atlasStats(node).folders} folders / ${atlasStats(node).files} files`,
          projects: node.projects || [],
          children,
          updatedAt: Date.now() - Math.max(0, atlasStats(node).depth) * 86400000 * 12
        };
      }
      function atlasFindNeuralNode(root, id) {
        if (!root) return null;
        if (root.id === id) return root;
        for (const child of root.children || []) {
          const found = atlasFindNeuralNode(child, id);
          if (found) return found;
        }
        return null;
      }
      function atlasPointer(canvas, event) {
        const rect = canvas.getBoundingClientRect();
        return { x: event.clientX - rect.left, y: event.clientY - rect.top };
      }
      function atlasInside(hit, point) {
        if (hit.r) {
          const dx = point.x - hit.x;
          const dy = point.y - hit.y;
          return dx * dx + dy * dy <= hit.r * hit.r;
        }
        return point.x >= hit.x && point.x <= hit.x + hit.w && point.y >= hit.y && point.y <= hit.y + hit.h;
      }
      function atlasTick() {
        const atlas = state.neuralAtlas;
        if (!atlas) return;
        atlasResize(atlas);
        atlas.t += .016;
        atlas.cam.zoom += (atlas.cam.tzoom - atlas.cam.zoom) * .18;
        atlas.cam.focusX += (atlas.cam.tfocusX - atlas.cam.focusX) * .18;
        atlas.cam.focusY += (atlas.cam.tfocusY - atlas.cam.focusY) * .18;
        atlas.hits.length = 0;
        atlasBg(atlas);
        atlasDrawSystem(atlas);
        atlasDrawUseContextButton(atlas);
        atlasDrawFile(atlas);
        atlas.raf = requestAnimationFrame(atlasTick);
      }
      function atlasResize(atlas) {
        const rect = atlas.canvas.getBoundingClientRect();
        const width = Math.max(320, rect.width);
        const height = Math.max(320, rect.height);
        const dpr = Math.max(1, Math.min(2, window.devicePixelRatio || 1));
        if (atlas.width === width && atlas.height === height && atlas.dpr === dpr) return;
        atlas.width = width;
        atlas.height = height;
        atlas.dpr = dpr;
        atlas.canvas.width = Math.floor(width * dpr);
        atlas.canvas.height = Math.floor(height * dpr);
        atlas.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      }
      function atlasCurrentNode() {
        const atlas = state.neuralAtlas;
        return atlas?.path[atlas.path.length - 1] || atlas?.nodes || projectMapToNeuralTree(projectMapRoot());
      }
      function atlasTotalCount(node) {
        return 1 + (node.children || []).reduce((sum, child) => sum + atlasTotalCount(child), 0);
      }
      function atlasDaysSince(node) {
        return Math.max(0, (Date.now() - (node.updatedAt || Date.now())) / 86400000);
      }
      function atlasVitality(node) {
        const direct = (node.children || []).length;
        const recent = Math.exp(-atlasDaysSince(node) / 120);
        return Math.max(.05, Math.min(1, recent * .6 + Math.min(.4, direct / 12)));
      }
      function atlasFileBrightness(node) {
        return Math.max(.05, Math.exp(-atlasDaysSince(node) / 280));
      }
      function atlasHash(text) {
        let hash = 2166136261 >>> 0;
        for (let i = 0; i < String(text).length; i++) {
          hash ^= String(text).charCodeAt(i);
          hash = Math.imul(hash, 16777619);
        }
        return hash >>> 0;
      }
      function atlasBg(atlas) {
        const { ctx, width: W, height: H, t } = atlas;
        const g = ctx.createRadialGradient(W / 2, H / 2, 0, W / 2, H / 2, Math.max(W, H));
        g.addColorStop(0, '#0a1738');
        g.addColorStop(.45, '#05071c');
        g.addColorStop(1, '#020208');
        ctx.fillStyle = g;
        ctx.fillRect(0, 0, W, H);
        for (let i = 0; i < 110; i++) {
          const x = (i * 137 + t * 6) % W;
          const y = (i * 73 + t * 3) % H;
          ctx.fillStyle = `rgba(170, 210, 255, ${(0.04 + 0.07 * Math.sin(t + i)).toFixed(2)})`;
          ctx.fillRect(x, y, 1.6, 1.6);
        }
        atlasBlob(ctx, W * .18, H * .78, 360, 'rgba(140, 90, 220, .12)');
        atlasBlob(ctx, W * .82, H * .22, 320, 'rgba(70, 180, 220, .10)');
      }
      function atlasBlob(ctx, x, y, r, color) {
        const g = ctx.createRadialGradient(x, y, 0, x, y, r);
        g.addColorStop(0, color);
        g.addColorStop(1, 'transparent');
        ctx.fillStyle = g;
        ctx.beginPath();
        ctx.arc(x, y, r, 0, Math.PI * 2);
        ctx.fill();
      }
      function atlasNeuronRadius(node) {
        return 10 + 12 * Math.log10(1 + atlasTotalCount(node));
      }
      function atlasOrbitSpeed(node) {
        const v = atlasVitality(node);
        return .02 + Math.pow(v, 1.4) * .65;
      }
      function atlasHue(node) {
        return (190 + atlasVitality(node) * 90) | 0;
      }
      function atlasTiltPoint(atlas, cx, cy, dx, dy) {
        return { x: cx + dx, y: cy + dy * Math.cos(atlas.tilt), depth: -dy * Math.sin(atlas.tilt) };
      }
      function atlasDrawNeuron(atlas, cx, cy, r, opts = {}) {
        const { ctx, t } = atlas;
        const hue = opts.hue ?? 220;
        const beatRate = opts.beat ?? 1.4;
        const dendrites = opts.dendrites ?? 8;
        const isSuper = !!opts.isSuper;
        ctx.save();
        ctx.strokeStyle = `hsla(${hue}, 85%, 60%, .42)`;
        ctx.lineWidth = 1;
        for (let i = 0; i < dendrites; i++) {
          const a = (i / dendrites) * Math.PI * 2 + cx * .0008;
          const tip = r + 12 + Math.sin(t * .8 + i + cx * .02) * 5;
          ctx.beginPath();
          ctx.moveTo(cx + Math.cos(a) * r, cy + Math.sin(a) * r);
          ctx.lineTo(cx + Math.cos(a) * tip, cy + Math.sin(a) * tip);
          ctx.stroke();
          ctx.fillStyle = `hsla(${hue}, 90%, 70%, ${(0.4 + 0.4 * Math.sin(t * 2 + i)).toFixed(2)})`;
          ctx.beginPath();
          ctx.arc(cx + Math.cos(a) * tip, cy + Math.sin(a) * tip, 1.6, 0, Math.PI * 2);
          ctx.fill();
        }
        ctx.restore();
        ctx.save();
        ctx.shadowColor = `hsl(${hue}, 80%, 65%)`;
        ctx.shadowBlur = isSuper ? 36 : 16;
        ctx.fillStyle = `hsla(${hue}, 60%, ${isSuper ? 35 : 28}%, .85)`;
        ctx.beginPath();
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
        ctx.fill();
        ctx.restore();
        const grad = ctx.createRadialGradient(cx - r * .3, cy - r * .4, 0, cx, cy, r);
        grad.addColorStop(0, `hsla(${hue}, 100%, ${isSuper ? 95 : 90}%, .92)`);
        grad.addColorStop(.4, `hsl(${hue}, 80%, ${isSuper ? 70 : 60}%)`);
        grad.addColorStop(1, `hsla(${hue}, 70%, ${isSuper ? 25 : 18}%, 1)`);
        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
        ctx.fill();
        const beat = .4 + .45 * Math.sin(t * beatRate);
        ctx.fillStyle = `hsla(${hue}, 100%, 88%, ${beat.toFixed(2)})`;
        ctx.beginPath();
        ctx.arc(cx, cy, r * .42, 0, Math.PI * 2);
        ctx.fill();
      }
      function atlasDrawTether(atlas, ax, ay, bx, by, vit, hue) {
        const { ctx, t } = atlas;
        const dx = bx - ax;
        const dy = by - ay;
        const len = Math.hypot(dx, dy) || 1;
        const nx = -dy / len;
        const ny = dx / len;
        const bend = Math.min(28, len * .12) * Math.sin(t * .9 + ax * .01);
        const cx1 = ax + dx * .35 + nx * bend;
        const cy1 = ay + dy * .35 + ny * bend;
        const cx2 = ax + dx * .65 - nx * bend;
        const cy2 = ay + dy * .65 - ny * bend;
        ctx.save();
        ctx.shadowColor = `hsla(${hue}, 90%, 65%, .8)`;
        ctx.shadowBlur = 14 + vit * 14;
        ctx.strokeStyle = `hsla(${hue}, 80%, 60%, ${(0.20 + vit * .4).toFixed(2)})`;
        ctx.lineWidth = 1.5 + vit * 2.5;
        ctx.beginPath();
        ctx.moveTo(ax, ay);
        ctx.bezierCurveTo(cx1, cy1, cx2, cy2, bx, by);
        ctx.stroke();
        ctx.restore();
        ctx.strokeStyle = `hsla(${hue}, 90%, 80%, ${(0.45 + vit * .4).toFixed(2)})`;
        ctx.lineWidth = .8 + vit * 1.3;
        ctx.beginPath();
        ctx.moveTo(ax, ay);
        ctx.bezierCurveTo(cx1, cy1, cx2, cy2, bx, by);
        ctx.stroke();
        const speed = .35 + vit;
        for (let k = 0; k < 2; k++) {
          const u = (t * speed + k * .5) % 1;
          const m = 1 - u;
          const px = m*m*m*ax + 3*m*m*u*cx1 + 3*m*u*u*cx2 + u*u*u*bx;
          const py = m*m*m*ay + 3*m*m*u*cy1 + 3*m*u*u*cy2 + u*u*u*by;
          ctx.fillStyle = '#fff';
          ctx.shadowColor = `hsl(${hue}, 100%, 80%)`;
          ctx.shadowBlur = 12;
          ctx.beginPath();
          ctx.arc(px, py, 2.2 + vit * 1.5, 0, Math.PI * 2);
          ctx.fill();
          ctx.shadowBlur = 0;
        }
      }
      function atlasDrawSuperNeuron(atlas, cx, cy, opts = {}) {
        const { ctx, mouse, hits } = atlas;
        const centered = !!opts.centered;
        const pushHit = opts.pushHit !== false;
        const x = centered ? cx : cx + 220;
        const y = centered ? cy : cy - 200;
        const r = centered ? 90 : 60;
        const over = (mouse.x - x) ** 2 + (mouse.y - y) ** 2 <= r * r;
        const g = ctx.createRadialGradient(x, y, 4, x, y, r * 3.5);
        g.addColorStop(0, 'hsla(50, 100%, 90%, .9)');
        g.addColorStop(.15, 'hsla(45, 100%, 75%, .55)');
        g.addColorStop(.5, 'hsla(35, 90%, 55%, .12)');
        g.addColorStop(1, 'transparent');
        ctx.fillStyle = g;
        ctx.beginPath();
        ctx.arc(x, y, r * 3.5, 0, Math.PI * 2);
        ctx.fill();
        atlasDrawNeuron(atlas, x, y, r, { hue: 45, beat: 1, dendrites: centered ? 16 : 10, isSuper: true });
        if (over && !centered) {
          ctx.strokeStyle = 'rgba(255, 240, 180, .85)';
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.arc(x, y, r * .78, 0, Math.PI * 2);
          ctx.stroke();
        }
        ctx.font = centered ? 'bold 11px ui-monospace, monospace' : '10px ui-monospace, monospace';
        ctx.fillStyle = 'rgba(255, 220, 160, .65)';
        ctx.textAlign = 'center';
        if (centered) {
          ctx.fillText('PRIMA', x, y + r + 26);
          ctx.font = '10px ui-monospace, monospace';
          ctx.fillStyle = 'rgba(255, 220, 160, .45)';
          ctx.fillText('super-neuron · root', x, y + r + 42);
        } else {
          ctx.fillText('PRIMA · click for root', x, y + r + 18);
        }
        ctx.textAlign = 'left';
        if (pushHit && !centered) hits.push({ kind: 'super', x, y, r });
      }
      function atlasDrawFileRing(atlas, cx, cy, ringR, files) {
        if (!files.length) return;
        const { ctx, t, tilt, hits } = atlas;
        const cT = Math.cos(tilt);
        const sT = Math.sin(tilt);
        const baseAng = t * .08;
        const projected = [];
        for (let i = 0; i < files.length; i++) {
          const a = baseAng + (i / files.length) * Math.PI * 2;
          const dx = Math.cos(a) * ringR;
          const dy = Math.sin(a) * ringR;
          projected.push({ node: files[i], sx: cx + dx, sy: cy + dy * cT, depth: -dy * sT });
        }
        projected.sort((a, b) => b.depth - a.depth);
        for (const item of projected) {
          const brightness = atlasFileBrightness(item.node);
          const color = brightness > .5 ? '#fff8c0' : (brightness > .2 ? '#a8d4ff' : '#5b7aa0');
          ctx.save();
          ctx.strokeStyle = `rgba(180, 220, 255, ${(0.10 + 0.25 * brightness).toFixed(2)})`;
          ctx.lineWidth = .7;
          ctx.beginPath();
          ctx.moveTo(cx, cy);
          ctx.lineTo(item.sx, item.sy);
          ctx.stroke();
          ctx.restore();
          ctx.save();
          ctx.shadowColor = color;
          ctx.shadowBlur = 6 * brightness;
          ctx.fillStyle = color;
          ctx.globalAlpha = .55 + .45 * brightness;
          ctx.beginPath();
          ctx.arc(item.sx, item.sy, 1.8 + brightness * 1.6, 0, Math.PI * 2);
          ctx.fill();
          ctx.restore();
          hits.push({ kind: 'file', payload: item.node, x: item.sx, y: item.sy, r: 8 });
        }
      }
      function atlasDrawCentralNeuron(atlas, cx, cy, r, node) {
        const { ctx, hits } = atlas;
        atlasDrawNeuron(atlas, cx, cy, r, { hue: atlasHue(node), beat: 1.2 + atlasVitality(node) * 1.4, dendrites: 14 });
        ctx.fillStyle = '#fff';
        ctx.font = 'bold 22px system-ui, sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(node.name, cx, cy + r + 32);
        ctx.fillStyle = 'rgba(180, 220, 255, .65)';
        ctx.font = '11px ui-monospace, monospace';
        ctx.fillText(`${atlasTotalCount(node)} nodes · v${(atlasVitality(node) * 100) | 0}%`, cx, cy + r + 52);
        ctx.fillText(atlas.path.map(item => item.name.toLowerCase()).join(' / '), cx, cy + r + 68);
        ctx.textAlign = 'left';
        hits.push({ kind: 'centerBack', x: cx, y: cy, r });
      }
      function atlasDrawOrbitingNeuron(atlas, parent, moon) {
        const { ctx, t, hits } = atlas;
        const { node, p, r } = moon;
        const hue = atlasHue(node);
        const vit = atlasVitality(node);
        atlasDrawTether(atlas, parent.x, parent.y, p.x, p.y, vit, hue);
        atlasDrawNeuron(atlas, p.x, p.y, r, { hue, beat: 1.2 + vit * 1.4, dendrites: 8 });
        const subFolders = (node.children || []).filter(child => child.type === 'folder').slice(0, 5);
        const cT = Math.cos(atlas.tilt);
        for (let i = 0; i < subFolders.length; i++) {
          const child = subFolders[i];
          const angle = t * atlasOrbitSpeed(child) * 1.5 + i * 1.3 + child.id.length;
          const orbit = r * 1.7 + i * 8;
          const sx = p.x + Math.cos(angle) * orbit;
          const sy = p.y + Math.sin(angle) * orbit * cT;
          const sr = Math.max(4, atlasNeuronRadius(child) * .32);
          atlasDrawTether(atlas, p.x, p.y, sx, sy, atlasVitality(child) * .7, atlasHue(child));
          atlasDrawNeuron(atlas, sx, sy, sr, { hue: atlasHue(child), beat: 1.6, dendrites: 5 });
        }
        const subFiles = (node.children || []).filter(child => child.type === 'file');
        atlasDrawFileRing(atlas, p.x, p.y, r * 1.45, subFiles);
        ctx.fillStyle = '#fff';
        ctx.font = '12px ui-monospace, monospace';
        ctx.textAlign = 'center';
        const textWidth = ctx.measureText(node.name).width + 14;
        ctx.fillStyle = 'rgba(2, 4, 14, .65)';
        atlasRoundRect(ctx, p.x - textWidth / 2, p.y + r + 6, textWidth, 18, 5);
        ctx.fill();
        ctx.fillStyle = '#e9efff';
        ctx.textBaseline = 'top';
        ctx.fillText(node.name, p.x, p.y + r + 9);
        ctx.fillStyle = 'rgba(180,220,255,.55)';
        ctx.font = '10px ui-monospace, monospace';
        ctx.fillText(`${atlasTotalCount(node)}n · v${(vit * 100) | 0}%`, p.x, p.y + r + 26);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'alphabetic';
        hits.push({ kind: 'neuron', payload: node, x: p.x, y: p.y, r: r + 6 });
      }
      function atlasDrawSystem(atlas) {
        const { ctx, width: W, height: H, cam } = atlas;
        ctx.save();
        ctx.translate(-cam.focusX, -cam.focusY);
        const cx = W / 2;
        const cy = H / 2 + 30;
        const node = atlasCurrentNode();
        const isRoot = atlas.path.length === 1;
        const folders = (node.children || []).filter(child => child.type === 'folder');
        const files = (node.children || []).filter(child => child.type === 'file');
        let centerR;
        let centre;
        if (isRoot) {
          atlasDrawSuperNeuron(atlas, cx, cy, { centered: true });
          centerR = 90;
          centre = { x: cx, y: cy };
          atlas.hits.push({ kind: 'closeRoot', x: cx, y: cy, r: centerR + 18 });
          atlasDrawFileRing(atlas, cx, cy, centerR + 35, files);
        } else {
          atlasDrawSuperNeuron(atlas, cx, cy);
          centerR = Math.min(75, 22 + 13 * Math.log10(1 + atlasTotalCount(node))) * cam.zoom;
          atlasDrawCentralNeuron(atlas, cx, cy, centerR, node);
          centre = { x: cx, y: cy };
          atlasDrawFileRing(atlas, cx, cy, centerR + 25, files);
        }
        const moons = [];
        const minOrbit = centerR + ((isRoot && files.length) ? 110 : 80);
        const labelMargin = 100;
        const hardMaxOrbit = Math.max(minOrbit + 80, Math.min(W / 2 - labelMargin, (H / 2 - labelMargin) / Math.cos(atlas.tilt)));
        const naturalSpan = (folders.length - 1) * 105;
        const maxOrbit = folders.length > 1 ? Math.min(hardMaxOrbit, minOrbit + Math.max(naturalSpan, (folders.length - 1) * 55)) : minOrbit;
        const range = Math.max(0, maxOrbit - minOrbit);
        const sinT = Math.abs(Math.sin(atlas.tilt));
        for (let i = 0; i < folders.length; i++) {
          const norm = folders.length > 1 ? i / (folders.length - 1) : .5;
          const orbit = minOrbit + norm * range;
          const angle = atlas.t * atlasOrbitSpeed(folders[i]) + i * 1.3;
          const p = atlasTiltPoint(atlas, cx, cy, Math.cos(angle) * orbit, Math.sin(angle) * orbit);
          const depthNorm = p.depth / Math.max(1, orbit * sinT);
          const r = atlasNeuronRadius(folders[i]) * Math.max(.70, Math.min(1.30, 1 - depthNorm * .30));
          moons.push({ node: folders[i], p, r, orbit });
        }
        if (atlas.transitioning && atlas.zoomTargetMoon) {
          const target = moons.find(moon => moon.node === atlas.zoomTargetMoon);
          if (target) {
            cam.tfocusX = target.p.x - W / 2;
            cam.tfocusY = target.p.y - H / 2;
          }
        }
        moons.sort((a, b) => a.p.depth - b.p.depth);
        for (const moon of moons) atlasDrawOrbitingNeuron(atlas, centre, moon);
        ctx.restore();
      }
      function atlasDrawUseContextButton(atlas) {
        if (atlas.openedFile) return;
        const { ctx, width: W, height: H, hits } = atlas;
        const node = atlasCurrentNode();
        const label = 'USE AS CONTEXT';
        const subtitle = atlasNodePath(node) || 'root';
        const w = Math.min(240, Math.max(170, W * .18));
        const h = 44;
        const x = (W - w) / 2;
        const y = H - 76;
        ctx.save();
        ctx.shadowColor = 'rgba(112,220,192,.32)';
        ctx.shadowBlur = 28;
        const gradient = ctx.createLinearGradient(x, y, x + w, y + h);
        gradient.addColorStop(0, 'rgba(112,220,192,.94)');
        gradient.addColorStop(1, 'rgba(232,200,109,.94)');
        ctx.fillStyle = gradient;
        atlasRoundRect(ctx, x, y, w, h, 22);
        ctx.fill();
        ctx.shadowBlur = 0;
        ctx.strokeStyle = 'rgba(255,255,255,.32)';
        ctx.lineWidth = 1;
        ctx.stroke();
        ctx.fillStyle = '#06100d';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.font = 'bold 12px ui-monospace, monospace';
        ctx.fillText(label, x + w / 2, y + 18);
        ctx.globalAlpha = .62;
        ctx.font = '10px ui-monospace, monospace';
        ctx.fillText(subtitle.length > 32 ? `${subtitle.slice(0, 29)}...` : subtitle, x + w / 2, y + 32);
        ctx.restore();
        hits.push({ kind: 'useContext', x, y, w, h });
      }
      function atlasDrawFile(atlas) {
        const file = atlas.openedFile;
        if (!file) return;
        const { ctx, width: W, height: H, hits } = atlas;
        ctx.fillStyle = 'rgba(2, 4, 12, .85)';
        ctx.fillRect(0, 0, W, H);
        const w = Math.min(960, W * .9);
        const h = Math.min(640, H * .88);
        const x = (W - w) / 2;
        const y = (H - h) / 2;
        ctx.fillStyle = '#0a132a';
        ctx.strokeStyle = 'rgba(180, 220, 255, .35)';
        ctx.lineWidth = 1;
        atlasRoundRect(ctx, x, y, w, h, 16);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = '#fff';
        ctx.font = 'bold 22px system-ui, sans-serif';
        ctx.fillText(file.name, x + 28, y + 56);
        ctx.font = '11px ui-monospace, monospace';
        ctx.fillStyle = 'rgba(180, 220, 255, .55)';
        ctx.fillText(`synapse · ${atlas.path.map(item => item.name).join('/')} · ${file.kind}`, x + 28, y + 78);
        ctx.font = '15px system-ui, sans-serif';
        ctx.fillStyle = '#e0e8ff';
        atlasWrap(ctx, file.body || '', x + 28, y + 120, w - 56, 22);
        const closeX = x + w - 84;
        const closeY = y + 14;
        ctx.fillStyle = '#0a132a';
        atlasRoundRect(ctx, closeX, closeY, 80, 28, 14);
        ctx.fill();
        ctx.strokeStyle = 'rgba(180,220,255,.5)';
        ctx.stroke();
        ctx.fillStyle = '#fff';
        ctx.font = 'bold 11px ui-monospace, monospace';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('X CLOSE', closeX + 40, closeY + 14);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'alphabetic';
        hits.push({ kind: 'close', x: closeX, y: closeY, w: 80, h: 28 });
      }
      function atlasWrap(ctx, text, x, y, maxWidth, lineHeight) {
        let cy = y;
        for (const paragraph of String(text).split('\n')) {
          const words = paragraph.split(' ');
          let line = '';
          for (const word of words) {
            const test = line ? `${line} ${word}` : word;
            if (ctx.measureText(test).width > maxWidth) {
              ctx.fillText(line, x, cy);
              cy += lineHeight;
              line = word;
            } else {
              line = test;
            }
          }
          if (line) {
            ctx.fillText(line, x, cy);
            cy += lineHeight;
          }
          cy += 6;
        }
      }
      function atlasRoundRect(ctx, x, y, w, h, r) {
        ctx.beginPath();
        ctx.moveTo(x + r, y);
        ctx.arcTo(x + w, y, x + w, y + h, r);
        ctx.arcTo(x + w, y + h, x, y + h, r);
        ctx.arcTo(x, y + h, x, y, r);
        ctx.arcTo(x, y, x + w, y, r);
        ctx.closePath();
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
          await refresh();
        }));
        qsa('[data-attach-workspace]').forEach(button => button.addEventListener('click', async () => {
          const value = prompt('Existing workspace directory path');
          if (!value) return;
          await postJson(`/api/projects/${button.dataset.attachWorkspace}/attach-workspace`, { workspace_path: value });
          await refresh();
        }));
      }
      async function createProjectFromUi(event) {
        event.preventDefault();
        await postJson('/api/projects', {
          name: el('project-name').value,
          library_path: el('project-library-path').value || null,
          workspace_path: el('project-workspace-path').value || null
        });
        await refresh();
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
              project_context_scope: state.contextScope || 'subtree',
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
            state.contextScope = data.ui.context?.scope || state.contextScope || 'subtree';
            state.activeProject = nodes[0]?.name || '';
            renderContext();
            if (data.mode === 'slash-command') pending.className = 'message system command';
            setMessage(pending, data.reply || 'Context updated.', detail, data.ui.context?.label || contextLabelFromProjects(nodes));
          } else if (data.ui?.type === 'approval') {
            setApprovalCard(pending, data.ui.approval, data.reply || 'Approval requested.', detail);
          } else if (data.ui?.type === 'agent_action') {
            setAgentActionCard(pending, data.ui, data.reply || 'Agent action completed.', detail);
          } else if (data.ui?.type === 'job_review') {
            setJobReviewCard(pending, data.ui, data.reply || 'Review packet ready.', detail);
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
      el('projects-open').addEventListener('click', () => {
        openOverlay('projects-overlay');
        renderProjects();
      });
      el('drawer-card').addEventListener('click', () => el('drawer').classList.toggle('open'));
      document.addEventListener('click', event => {
        if (el('drawer').classList.contains('open') && !el('drawer').contains(event.target)) {
          el('drawer').classList.remove('open');
        }
      });
      el('composer-library').addEventListener('click', () => openOverlay('projects-overlay'));
      el('composer-context').addEventListener('click', () => {
        appendMessage('system', `Current context: ${currentContextLabel()}`, 'Context');
      });
      el('composer-slash').addEventListener('click', () => {
        const input = el('goal-input');
        if (!input.value.startsWith('/')) input.value = '/';
        input.focus();
        input.setSelectionRange(input.value.length, input.value.length);
        updateSlashPalette();
      });
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
      drawAmbientAtlasBackground();
      window.addEventListener('resize', () => {
        drawAmbientAtlasBackground();
        if (el('projects-overlay').classList.contains('open')) renderNeuralAtlas();
      });
      function drawAmbientAtlasBackground() {
        const canvas = el('atlas-bg');
        if (!canvas) return;
        if (!state.ambientAtlas) {
          const nodes = [];
          for (let i = 0; i < 22; i++) {
            nodes.push({
              x: Math.random(),
              y: Math.random(),
              r: 10 + Math.random() * 22,
              hue: 200 + Math.random() * 90,
              phase: Math.random() * Math.PI * 2,
              driftSpeed: 0.04 + Math.random() * 0.08
            });
          }
          const links = [];
          for (let i = 0; i < nodes.length; i++) {
            let best = -1;
            let bestDistance = Number.POSITIVE_INFINITY;
            for (let j = 0; j < nodes.length; j++) {
              if (i === j) continue;
              const dx = nodes[i].x - nodes[j].x;
              const dy = nodes[i].y - nodes[j].y;
              const distance = dx * dx + dy * dy;
              if (distance < bestDistance) {
                bestDistance = distance;
                best = j;
              }
            }
            if (best > i) links.push({ a: i, b: best });
          }
          state.ambientAtlas = { nodes, links, t: 0, raf: null };
        }
        if (!state.ambientAtlas.raf) animateAmbientAtlasBackground();
      }
      function animateAmbientAtlasBackground() {
        const canvas = el('atlas-bg');
        const ambient = state.ambientAtlas;
        if (!canvas || !ambient) return;
        const width = window.innerWidth;
        const height = window.innerHeight;
        const dpr = Math.max(1, Math.min(2, window.devicePixelRatio || 1));
        canvas.width = Math.floor(width * dpr);
        canvas.height = Math.floor(height * dpr);
        canvas.style.width = `${width}px`;
        canvas.style.height = `${height}px`;
        const ctx = canvas.getContext('2d');
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ambient.t += 0.012;
        ctx.fillStyle = '#06081a';
        ctx.fillRect(0, 0, width, height);
        for (const [cx, cy, radius, color] of [
          [width * 0.22, height * 0.78, 360, 'rgba(140, 90, 220, .12)'],
          [width * 0.82, height * 0.22, 320, 'rgba(70, 180, 220, .10)'],
          [width * 0.50, height * 0.50, 280, 'rgba(231, 185, 97, .04)']
        ]) {
          const gradient = ctx.createRadialGradient(cx, cy, 0, cx, cy, radius);
          gradient.addColorStop(0, color);
          gradient.addColorStop(1, 'transparent');
          ctx.fillStyle = gradient;
          ctx.beginPath();
          ctx.arc(cx, cy, radius, 0, Math.PI * 2);
          ctx.fill();
        }
        for (const link of ambient.links) {
          const a = ambient.nodes[link.a];
          const b = ambient.nodes[link.b];
          const ax = a.x * width;
          const ay = a.y * height;
          const bx = b.x * width;
          const by = b.y * height;
          const hue = (a.hue + b.hue) / 2;
          ctx.save();
          ctx.strokeStyle = `hsla(${hue | 0}, 80%, 65%, .12)`;
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(ax, ay);
          ctx.lineTo(bx, by);
          ctx.stroke();
          const u = (ambient.t * a.driftSpeed * 2) % 1;
          ctx.fillStyle = `hsla(${hue | 0}, 90%, 80%, .8)`;
          ctx.beginPath();
          ctx.arc(ax + (bx - ax) * u, ay + (by - ay) * u, 1.6, 0, Math.PI * 2);
          ctx.fill();
          ctx.restore();
        }
        for (const node of ambient.nodes) {
          const x = node.x * width;
          const y = node.y * height;
          const radius = Math.max(2, node.r * (0.85 + 0.15 * Math.sin(ambient.t + node.phase)));
          ctx.save();
          ctx.shadowColor = `hsl(${node.hue}, 80%, 65%)`;
          ctx.shadowBlur = 12;
          ctx.fillStyle = `hsla(${node.hue}, 70%, 30%, .58)`;
          ctx.beginPath();
          ctx.arc(x, y, radius, 0, Math.PI * 2);
          ctx.fill();
          ctx.restore();
          const beat = 0.4 + 0.45 * Math.sin(ambient.t * 1.4 + node.phase);
          ctx.fillStyle = `hsla(${node.hue}, 100%, 88%, ${beat.toFixed(2)})`;
          ctx.beginPath();
          ctx.arc(x, y, Math.max(1, radius * 0.42), 0, Math.PI * 2);
          ctx.fill();
        }
        ambient.raf = requestAnimationFrame(animateAmbientAtlasBackground);
      }
    })();
  </script>
</body>
</html>"##;
    html.replace("__BIND__", bind)
        .replace("__WORKER_CONCURRENCY__", &worker_concurrency.to_string())
}
