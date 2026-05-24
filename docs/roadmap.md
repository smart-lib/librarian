# Roadmap

This roadmap is the canonical project planning document. Short-lived notes,
research findings, and todo items should be folded back here once they affect
the product direction.

## Current Status

- Branch: `develop`.
- Baseline checkpoint: `main` contains the initial scaffold commit.
- Current phase: working Librarian chat MVP.
- Current crate version: `0.2.1`; bump at least the minor version when a visible
  MVP capability group lands, not only patch fixes.
- Next implementation focus: replace the local memory responder with a real
  provider-backed Librarian chat loop, then add explicit tools, permissions,
  and background agent launch as separate actions.

## Product Defaults

- MVP chat provider: Codex CLI on the host profile already configured through
  Librarian auth. Background coding agents also use Codex CLI first.
- Non-provider containers have no network access by default. Provider-backed
  agent jobs, such as Codex CLI runs, use provider network by default so the
  model endpoint is reachable; explicit `--allow-network` remains the broader
  open-network opt-in.
- Project mounts are read-write by default, configurable per run/project.
- No required paid or proprietary external secret service.
- Localhost admin UI is the default interaction path.
- First-run setup chooses one stable Librarian root. Silent/default setup uses
  `%APPDATA%\Librarian` on Windows, `~/Librarian` on Linux, and
  `~/Library/Application Support/Librarian` on macOS; `--home` and
  `LIBRARIAN_HOME` support portable roots. The root layout separates hidden
  `.app`, `.cfg`, and `.mdb` folders from public `Library` and `Projects`.
- Ubuntu is the current golden-path install target: clone the repository and
  run one bootstrap script, or use the README one-line starter for clean hosts.
- The Obsidian-compatible knowledge base is global at the Librarian root, so chats,
  project notes, decisions, and background runs across many projects share one
  knowledge base.
- Agents have full privileges inside the mounted project boundary by default.
  Irreversible behavior such as commits and pushes is controlled by configurable
  project/git policy, not by hardcoded global blocking.
- Worker parallelism defaults to `1` and can be raised through config or CLI.

## Completed Baseline

The milestones below describe completed capability groups. Follow-up work from
these milestones is tracked in `Backlog From Completed Milestones` instead of
being left as unfinished items inside the completed sections.

## Milestone 1: Local Core

Status: Done.

- Rust CLI scaffold.
- SQLite bootstrap.
- Project registry.
- Localhost admin shell.
- Docker runner abstraction.
- Codex provider shell.
- Worker command for queued jobs.
- Job event log for status, stdout, stderr, and knowledge-base notes.

## Milestone 2: Usable Agent Runs

Status: Done for MVP code path. Runtime hardening is tracked below.

- Build `librarian-agent` image.
- Run Codex in a container.
- Stream logs into SQLite with UI refresh.
- Persist run summaries into the Markdown knowledge base.
- Feed enriched memory context into provider prompts.
- Store run outcomes back into memory.
- Add pause/cancel/retry lifecycle commands.
- Heartbeat and cancellation tracking.

## Milestone 3: Admin UI

Status: MVP shell done. Richer operator views are tracked below.

- Chat-first interface.
- Manual settings panels.
- Project registry editor.
- Job monitor.
- Worker capacity panel with configured slots, active jobs, queued jobs, and
  available slots.
- Recent Librarian actions panel backed by structured system events.
- Knowledge base editor for basic notes.
- Codex auth status check and onboarding.

## Milestone 4: Schedules and Heartbeats

Status: Done.

- System lifecycle scheduler.
- User reminders.
- Scheduled Codex jobs from `AgentTask` schedules.
- Job heartbeats and recovery.
- Container cleanup for stopped Librarian-managed containers.
- Memory compaction candidate scan.
- System event timeline for scheduler and Librarian actions.
- Configurable worker concurrency with default parallelism of `1`; allow
  changing it through persisted config, CLI, and admin commands for stronger
  hosts.
- Schedule create/update/delete, enable/disable, and manual run controls.

## Milestone 5: Vector Memory

Status: Done for local MVP.

- Embedding provider abstraction with a local deterministic backend.
- SQLite-backed vector storage in `memory_embeddings`.
- Local brute-force vector scoring over scoped SQLite candidates.
- FTS/lexical fallback and complement.
- Recency-aware retrieval ranking.
- Project/activity scoped context packs.
- Contradiction and supersession handling.
- CLI/admin memory status and embedding backfill.

## Milestone 6: Secret Broker

Status: Done for local MVP backend.

- Local encrypted secret vault.
- Windows DPAPI and explicit AES-GCM fallback encryption modes.
- Capability grants per provider/session/job with TTL and max-use limits.
- Host-side broker HTTP endpoint.
- Audited secret store, grant, and resolve/proxy use.
- Provider proxy mode for OpenAI/OpenRouter-style HTTP APIs.

## Milestone 7: Provider Router, Gates, and Limits

Status: Feature buildout done for MVP readiness. Real runtime validation is
next.

- Provider registry with model metadata, defaults, cost hints, and task-fit
  hints.
- Explicit Codex runtime profile configuration for self-hosted container runs:
  host `CODEX_HOME` can be mounted into the agent container only after opt-in.
- Runtime diagnostics for container engine, agent image, host Codex CLI, and
  Codex mount config.
- Agent image build command through the configured Docker/Podman runtime.
- WSL Podman fallback runtime for Windows when the global Podman CLI connection
  metadata is broken but the `podman-machine-default` WSL distro is usable.
- Per-run prompt file mount at `/workspace/run/prompt.txt`; Codex reads prompts
  through stdin instead of a long command-line argument.
- Prompt/response gate pipeline entry points for lightweight validation,
  filtering, transformation, provider-specific prompt shaping, and redaction.
- Automatic secret detection before prompt enrichment: move raw tokens into the
  secret vault, replace them with secret/grant references, and audit the action.
- Output/tool-result leak scanning: redact known secrets or secret-shaped values
  before they are stored, displayed, or reintroduced into prompts.
- OpenRouter API adapter.
- Claude Code adapter and auth-mode metadata.
- Usage/cost observation table for provider responses, CLI logs, and indirect
  session telemetry.
- Structured provider state table with pause windows.
- Rate-limit event detection from CLI/log text and manual observations.
- Third Eye probes for health, provider list, refresh, and direct read-only
  SQLite summaries.
- Admin UI cards for provider catalog/state, recent usage observations, Third
  Eye health/DB summary, provider pause/resume, job lifecycle timestamps, and
  compact gate/context event rendering.
- Clearer Codex auth/runtime diagnostics: `doctor` now reports Codex profile
  presence/mount state, Codex container preflight catches missing CLI/profile
  mount, and worker logs emit structured diagnostics for missing bearer/login
  failures.
- Admin job creation and scheduled agent tasks can select Codex, OpenRouter, or
  Claude Code instead of always defaulting to Codex.
- Persistent fallback routing policy: fallback enablement/order in config, CLI
  and admin controls, and worker reroute to the first available fallback when
  the selected provider/model is paused.
- Daily budget guardrails before dispatch for total, provider, and project
  cost, based on observed `cost_usd` telemetry for the current UTC day.
- Stable local Librarian root: default state now lives in a platform user app
  directory, first setup can choose a custom root, and portable launchers can
  pin `LIBRARIAN_HOME` to an in-folder `.librarian`.
- Actionable `doctor` readiness report with `ready / degraded / blocked`
  summary, severity-tagged checks, runtime engine validation, Codex profile
  mount diagnostics, and the MVP setup command sequence.
- First-run `setup` command creates the root, migrates SQLite, reports the
  launch context, can select host/WSL Podman runtime, and can optionally build
  the agent image before running `doctor`.
- Windows bootstrap now builds the binary, runs `setup`, and creates a local
  launcher folder for manual UI checks.
- Initial GitHub Actions release workflow packages Windows and Linux builds
  with checksums.
- Ubuntu starter/bootstrap scripts install missing dependencies, build
  Librarian, run silent setup, and prepare the Docker-based agent image when
  permissions allow it.
- Ubuntu one-line install now treats Git/source as an installer detail: it
  builds in `Librarian/.app/source`, installs the binary to
  `Librarian/.app/bin/librarian`, and removes the checkout unless explicitly
  preserved.
- Ubuntu installs now record `.app/version.json`, and `doctor` reports the
  running version plus install metadata and prints the `upgrade` command.
- Doctor output now highlights the overall status, distinguishes state root
  from launch context, and prints a single next important step plus remaining
  blockers.
- Admin UI shell now treats chat as the primary screen, keeps browser-level
  scrolling locked to the viewport, moves operational settings into a
  full-screen tabbed overlay, and adds the first visual project-map surface.

## MVP Readiness

These tasks come before new product milestones. The goal is not more breadth;
it is making the current MVP reliable enough that the environment can be set up
and the core flows can be tested manually and automatically.

## Priority 0: Working Librarian Chat

Status: First provider-backed pass implemented. `/api/chat` now builds a
dedicated Librarian prompt from the user message and filtered memory context,
then calls host `codex exec` with Librarian's configured portable `CODEX_HOME`.
It no longer creates background jobs. Transcript/session structure, richer
fallbacks, and tests remain.

Goal: Librarian must be a real conversational assistant before agent automation
is polished. A user must be able to talk without selecting a project, discuss
many projects at a high level, and ask Librarian to use memory context when
answering.

Tasks:

- Replace `local-memory-responder` with a provider-backed Librarian chat path.
  First MVP target, Codex CLI using the configured portable Codex profile, is
  implemented.
- Build a dedicated Librarian chat prompt, separate from coding-agent prompts.
  Inputs: identity/instructions, user message, recent conversation turns,
  global memory, selected project memory when explicit, and compact citations.
  First chat prompt pass now includes recent ordered turns from the current
  transcript session.
- Keep `/api/chat` as conversation only. It must not create background jobs or
  wait for agent execution.
- Remove current self-echo behavior: placeholder assistant replies are filtered
  from the chat prompt, and new responses are stored as `mode=codex-chat`.
- Add persisted chat runtime settings for Codex timeout, memory hit limit, and
  future max iterations. First pass done in `[chat]`.
- Save useful user and assistant turns into memory, but distinguish raw chat
  transcript from durable facts/decisions/instructions. First pass marks chat
  user/assistant memory with `chat_session_id`, `memory_role=raw_chat_turn`,
  and `durability=transcript`; durable facts/decisions remain separate memory
  kinds/tools.
- Add a small chat transcript model: session/thread id, ordered turns, selected
  project context, and durable memory links. First backend pass adds
  `chat_sessions` and `chat_turns`; `/api/chat` now creates or reuses a session,
  records ordered user/assistant turns, links them to memory ids, and returns
  `session_id`. Chat-first admin UI now keeps the returned session id across
  messages and resets it when the active project changes. Read API first pass
  adds `/api/chat/sessions` and `/api/chat/sessions/{id}/turns` for restoring
  transcript state. Chat-first admin UI now restores the latest session and
  thread on page load, and exposes a lightweight new-chat control that clears
  the active session without touching history. First admin history pass adds a
  Chats settings tab that lists recent sessions with turn counts and restores a
  selected session into the chat thread.
- Add a clear fallback when the chat provider is unavailable: actionable
  “Codex auth/runtime missing” message, not memory dump output.
- Add tests for the chat endpoint that prove it does not create jobs and that
  placeholder/self-echo memories are excluded.
- First provider-unavailable fallback now returns
  `mode=chat-provider-unavailable` with portable `CODEX_HOME`, `auth codex`,
  and `doctor` commands when the chat runner fails.

Dependencies:

- Existing Codex host auth/profile setup.
- Existing memory retrieval and gate/redaction pipeline.

## Priority 0A: Iterative Thinking Loop

Status: First safe pass implemented. Normal chat still answers with one Codex
call, but Librarian can now return an internal JSON control message to request
another memory search, ask a clarifying question, or finalize an answer. The
loop is bounded by `[chat].max_iterations` and stores a compact trace in
assistant memory metadata. UI controls, cancellation, richer traces, and tests
remain.

Goal: let Librarian decide when a question needs more reflection, memory search,
or a clarifying question, without always spending the maximum budget.

Tasks:

- Add configurable thinking depth: minimum `1`, default around `5-10`, maximum
  configurable up to `50-100` for deliberate long reasoning sessions. First
  pass is persisted as `[chat].max_iterations` and clamped to `1..=100`.
- Model each iteration as an internal loop step with a bounded budget, not as
  user-visible spam. Store a compact trace/summary when useful. First pass
  stores compact action/query/reason trace in assistant memory metadata.
- Let Librarian stop early when the answer is good enough. First pass treats
  plain text as a final answer, so simple turns remain one provider call.
- Let Librarian choose among actions per iteration: answer, search memory again,
  refine draft, ask user a clarifying question, propose a tool call, or request
  approval. First pass supports answer, search memory again, and clarify.
- Add guardrails for cost/time: max iterations, max wall-clock time, max context
  growth, and user-visible cancellation.
- Keep “thinking” implementation separate from provider-specific hidden
  reasoning. Librarian controls an iterative planning loop; provider hidden
  reasoning remains opaque.
- Add tests for internal JSON directives, fallback to plain text, and bounded
  memory-search iteration. Directive parsing and plain-text fallback have unit
  coverage; first bounded async loop test now uses a mock chat runner to verify
  search-memory iteration stops at the configured budget and includes retrieved
  memory in the next prompt.
- Add admin controls/readout for chat iteration settings and optionally expose a
  compact developer trace when diagnostics are enabled. First settings UI/API
  pass lets the admin edit assistant name, Codex timeout, memory hit limit, and
  max iterations.

## Priority 1: Actionable Bootstrap and Doctor

Status: Done for code readiness. First-run setup and basic packaging are now
part of this priority. Environment validation remains with the user once
Podman/Docker and portable Codex auth are configured.

Goal: a new local setup should reach a clear `ready / blocked / degraded`
answer without reading source code.

Tasks:

- Convert `doctor` output into structured severity checks: `ok`, `warn`,
  `error`, with concise next steps.
- Cover the MVP bootstrap chain in diagnostics: config layout, SQLite open,
  knowledge-base path, runtime command, agent image, Codex host CLI, Codex profile path,
  Codex container mount setting, and project mount path style.
- Add an explicit preflight result for the selected runtime path: host Docker or
  Podman, and WSL Podman fallback on Windows.
- Document the expected handoff command sequence for environment setup:
  `setup`, portable Codex sign-in, `auth codex --enable-container-mount`,
  `runtime build-agent-image`, `doctor`, `project add`, `admin`, `worker`.
- Choose a stable platform default root instead of using the process current
  directory as storage.
- Treat the process current directory as launch context for future project
  auto-detection.
- Add a release-folder launcher that pins `LIBRARIAN_HOME` beside the binary
  for portable/self-contained installs.
- Add an Ubuntu golden-path bootstrap: one command after clone, plus a README
  one-line starter that installs Git, clones the default branch, and runs silent
  setup.
- Add a first `upgrade` command that reuses the Ubuntu starter, preserves the
  Librarian root, and records install metadata. First source-build pass done;
  later release-binary upgrades should keep the same command.

Dependencies:

- Existing runtime config and Codex mount diagnostics from Milestone 7.
- No real container job needs to succeed before this lands, but the output must
  make failures easy to act on.

Owner split:

- Code: Librarian.
- Environment validation: user, after the readiness code is in place.

## Priority 1A: Ubuntu Golden Path Validation

Status: Ready for user validation.

Goal: make a clean Ubuntu/WSL host the easiest path to MVP testing and the
closest rehearsal for the later dedicated Ubuntu machine.

Tasks:

- Run the README one-line starter on clean Ubuntu/WSL.
- Confirm Rust, Node/npm, Codex CLI, Docker, release build, setup, and doctor.
- Confirm Docker group behavior after first install; if a relogin is still
  required, make the script's next step clearer or add a safer rootless Docker
  path.
- Start admin UI on `0.0.0.0:17377` and verify Windows/host access through
  `http://127.0.0.1:17377`.
- After Codex auth, build `librarian-agent` and run the MVP smoke flow. First
  pass now exposes `librarian smoke mvp --provider codex --run-agent`, which
  creates a disposable project, exercises Library/Projects file-tool sandbox
  operations, writes searchable memory, runs job preflight, and optionally runs
  that exact provider job. `doctor --smoke` runs the same preflight smoke from
  the readiness command.

## Priority 1B: Literal Project Library and Friendly Admin UX

Status: In progress. Chat-first shell is active and `/api/chat` is separated
from background agent jobs; backend project library workflows remain. Real chat
model integration moved to Priority 0. The current temporary project-map/admin
surfaces are not an acceptable product target; replace them with finished UX
flows instead of patching placeholders.

Goal: make Librarian feel like a literal project library first, where project
context is rooted in the knowledge tree and low-level agent dispatch mechanics
stay hidden until they are needed.

Project tree model:

- Every node in `Librarian/Library` can be treated as a project context: a root
  folder, a nested folder, or a Markdown note. For example, `/Games` can be the
  high-level game-development project, while `/Games/AdvenTableDays` is a
  concrete game project underneath it.
- Projects are hierarchical. A parent project can group several child projects:
  for example, `/Librarian` can contain `/Librarian/Core`,
  `/Librarian/Site`, and `/Librarian/Mobile`. Each child is independently
  addressable, while the parent remains a meaningful project context.
- The library path is the primary identity/index for project memory. The path
  acts as a block index: memories attached to `/Games` apply to that node and
  can be searched together with descendants; memories attached to
  `/Games/AdvenTableDays` are narrower.
- Context retrieval must understand tree scope:
  - current node only;
  - current node plus descendants;
  - current node plus ancestors;
  - a focused path selected by user command;
  - automatic project selection from the current dialogue when confidence is
    high, otherwise ask the user.
- A project may optionally attach a workspace/implementation folder. That
  workspace can be the default `Librarian/Projects/{ProjectName}` or an
  existing external directory chosen by the user. The library tree remains the
  source of meaning; attached workspaces are implementation targets for agents.

Literal library visualization:

- Opening "Projects" should become opening the "Library". The primary view is
  a literal library, not a generic graph or file-manager table.
- Root library folders are shown as library rows or corridors of shelves. These
  rows can visually recede into depth when the structure contains multiple
  large branches such as `Games`, `Personal Assistant`, `Tools`, or `Books`.
- A standalone top-level project may appear as an individual bookcase instead
  of a corridor.
- A project with only files is shown as books on one or more unnamed shelves.
- A project with subfolders is shown as a bookcase with named shelves. Each
  shelf contains books for Markdown files and/or doors/markers for nested
  shelves/bookcases.
- Deeper structures should become navigable spaces: click a row/corridor to
  enter the branch, click a bookcase to focus it, click a shelf to inspect the
  files and child projects on that shelf, click a book to open/read the note.
- Bookcases and shelves are labeled. Books can show title/spine labels derived
  from Markdown filenames and frontmatter/heading metadata later.
- Favorites must be supported. Favorite projects/bookcases/books should be
  visually highlighted in the library.
- Activity state should be visible without opening details: recently edited,
  actively developed, active background jobs, blocked jobs, pending approvals,
  or scheduled work can use distinct glows/badges.
- Hover/focus details should show useful metadata: last edited/developed time,
  active tasks, attached workspace status, memory count, recent decisions, and
  provider/job status. Do this as tooltips/side panels, not cluttered text on
  the main shelf.
- The library UI must support direct commands/actions from the selected node:
  start chat in this context, attach/create workspace, add note/book, create
  child project, mark favorite, queue explicit agent work, inspect memory.

Acceptance criteria for the finished Library UI:

- The first screen reads visually as a library: rows, bookcases, shelves, and
  books. It must not look like a temporary card grid or a generic node graph.
- Every visible item maps to a real `Library` path and can be selected as a
  project context.
- The selected context is reflected in chat and memory retrieval.
- Parent/child project scopes are testable with memory searches over a subtree.
- Attached workspace is visible and editable from the selected node details.
- The design works at the MVP minimum viewport without browser-level scrolling.

Tasks:

- Keep the browser viewport fixed and use application-owned scroll regions for
  chat, settings tabs, and project map panels. First pass done.
- Make chat the primary surface; move providers, schedules, secrets, budgets,
  and events behind a settings button and full-screen tabbed overlay. First
  pass done; richer settings controls can be restored inside tabs only.
- Replace the temporary project-map/card surface with the literal Library UI
  described above. The current `/api/project-map` and `/project map` data can
  be reused only as backend tree data; the product UI should be a finished
  library visualization, not a placeholder graph.
- Keep low-level dispatch fields such as provider, project id, secret grant
  token, and network mode out of the main chat composer. First pass done with
  Codex as the default MVP provider and the selected/first project as context.
- Keep Librarian chat as a normal AI conversation surface, usable without a
  specific project. First pass separates it from job dispatch; real model
  response is tracked in Priority 0.
- Polish the main chat shell for actual conversation: full-width thread and
  prompt input, Enter-to-send with Ctrl+Enter newline, floating corner controls,
  and a centered pull-tab identity marker. First pass done. Next finished
  design should turn the centered pull-tab into a real control drawer: new chat,
  recent chats, active context, quick context switch, and future chat actions
  live there. Do not place chat controls in the right corner where they collide
  with the Library button.
- Move background agent launch into explicit project actions and command blocks,
  so agents can run without interrupting the Librarian conversation.
- Define and implement hierarchical project contexts: every Library node can be
  a context/project; project records should attach to Library paths without
  forcing a flat `projects/{ProjectName}` namespace; parent/child paths should
  drive memory retrieval and UI navigation.
- Add tree-aware memory retrieval: when the active context is `/Games`, the
  default search should include `/Games` and descendants, with options to
  narrow to current node only or include ancestors.
- Add dialogue-aware context selection: Librarian should automatically infer
  the likely project/library node from the conversation when confidence is
  high, otherwise ask the user or accept an explicit command.
- Add project creation/linking from the admin UI: create the memory folder,
  optionally create the working directory under the default projects root, or
  attach an existing directory. First slash-command pass adds `/project list`,
  `/project status`, `/project create`, `/project attach-library`,
  `/project detach-library`, and `/project attach-workspace`. First UI pass adds
  admin project creation plus attach-library/attach-workspace controls backed by
  `/api/projects` mutation routes.
- When Librarian is launched from a directory that is not already known as a
  root or project, ask whether to register that directory as a working project
  and create/link the corresponding library folder.
- Capture the launched-from-unknown-folder behavior as a default reusable
  agent-instruction block once the visual instruction builder exists.

## Priority 1C: Tools, Permissions, and Slash Commands

Status: First backend tool boundary started. Library filesystem tools now exist
as narrow host-side operations for `Library`; default working-folder tools for
`Projects` are split into the separate `/work` namespace. Direct slash commands
can invoke both without an LLM call. First persisted `auto/ask/deny` policies
exist and slash commands pass through the policy/audit gate. Assistant-initiated
tool invocation, interactive approval prompts, richer command UX, and UI
controls remain.

Goal: Librarian should be useful inside its own root without unrestricted host
power. Tools must be explicit, logged, permissioned, and available both through
assistant decisions and direct slash commands that do not call the LLM.

Tool groups:

- Library filesystem tools within `Librarian/Library`: create empty
  folders/files, rename, move, and delete. First API pass implemented with
  relative-path sandboxing and `library_tool` system events.
- Workspace filesystem tools within `Librarian/Projects`: create empty
  folders/files, rename, move, and delete for default implementation/product
  folders. These are semantically separate from library knowledge tools and use
  the `/work` namespace plus `workspace_tool` system events.
- Markdown content tools for user content under `Librarian/Library`: read,
  create, edit, append, summarize, and reorganize `.md` notes. First API pass
  supports whole-file read/write for `.md` under `Library`; second pass adds
  range reads, line-range cut/replace, append, find, cut-first-match, and
  replace-first-match. Safer section-aware Markdown editing remains.
- Memory tools: write durable facts, decisions, instructions, preferences,
  status notes, and run observations; update/supersede/contradict older memory
  with audit trail. First slash pass supports `/mem remember <kind> <content>`,
  `/remember <content>` as a fact shortcut, and `/mem recent [limit]` in the
  current chat scope. Durable remember now marks memory with
  `memory_role=durable_memory` and `durability=durable`; `/mem recent` filters
  raw transcript turns and legacy rows known to come from `admin:librarian-chat`
  out of the user-visible memory list without hiding unclassified
  AssistantMessage/UserMessage notes, and shows memory ids so correction
  commands are usable. First correction pass adds
  `/mem supersede <old-id> <kind> <content>` and
  `/mem contradict <old-id> <kind> <content>` with linked durable memory
  records and `memory_tool` audit events.
- Settings/prompt tools: inspect settings, propose changes, and apply only
  after explicit user approval. First settings slash pass supports
  `/settings tool-permissions` and guarded
  `/settings set-tool-permission <key> <auto|ask|deny> --yes`.
- Background agent tools: create project-scoped agent jobs, preflight them, run
  worker actions, cancel/retry, and report results back into chat without
  blocking the conversation. First explicit slash pass adds `/agent list`,
  `/agent status`, `/agent events`, `/agent preflight`, guarded
  `/agent launch ... --yes`, `/agent cancel ... --yes`, and
  `/agent retry ... --yes`.

Permission model:

- Each tool has a policy: `auto`, `ask`, or `deny`. First persisted
  `[tool_permissions]` config is implemented for library, workspace, memory,
  settings, and agent-launch groups.
- Destructive filesystem operations default to `ask`. Slash commands are treated
  as explicit user intent when policy is `ask`, while `deny` blocks and logs.
- Editing user Markdown defaults to `ask` until the user grants broader trust.
- Memory writes can be `auto` for low-risk chat-derived notes but must expose
  what was remembered and allow correction.
- Settings, prompt changes, auth, provider config, and background agent launch
  default to `ask`. Tool-permission changes require both the `settings_change`
  gate and an explicit slash confirmation flag.
- Background agent launch, retry, and cancel pass through the `agent_launch`
  gate. Normal chat still never creates jobs.
- Assistant-initiated tool calls that need confirmation should create pending
  approvals instead of executing directly. First scaffold adds persisted
  `tool_approvals` plus `/approval list`, `/approval propose`,
  `/approval approve`, and `/approval reject`. Chat loop now also accepts a
  model-emitted `propose_tool` directive and records it as a pending approval.
  First executor pass added `/approval execute <id>` for a narrow whitelist:
  library create/write/append, memory remember, and prompt add-block. Second
  executor pass expands approved actions to library move/delete and line/search
  Markdown edits, workspace create/move/delete, and starter project creation.
  Execution still passes through the normal permission gates and tool sandboxes.
- All tool calls, including denied and direct slash-command calls, are logged to
  history/system events so Librarian can account for them in future context.
  First pass logs `tool_permission` decisions and mutating library/workspace
  events.

Slash commands:

- Add a command dispatcher before LLM invocation for commands such as
  `/remember`, `/project`, `/note`, `/move`, `/rename`, `/delete`, `/agent`,
  `/preflight`, `/settings`, and `/help`. First pass used root-level library
  commands; second pass moves the library surface under `/lib ...`, removes
  project-folder operations from `/lib`, and adds `/work ...` for default
  working folders. Memory commands now live under `/mem ...` with `/remember`
  as a shortcut. Settings inspection and tool-permission updates now live under
  `/settings ...`. Project library/workspace linking lives under `/project ...`.
  Approval queue inspection and decisions live under `/approval ...`.
  Background job operations now live under `/agent ...`.
- Slash commands should execute without spending provider tokens when they are
  deterministic. First library-tool pass bypasses Codex inside `/api/chat`.
- Slash-command results should still be added to the conversation/event history
  as context. First pass stores the command turn in memory and writes
  `library_tool`, `workspace_tool`, `memory_tool`, and `settings_tool` events
  for mutating commands.

## Priority 2: Prompt Builder and Instruction Authoring

Status: Backend model started.

Goal: provide a human-friendly visual editor for Librarian identity, chat
prompt layers, agent instruction files, and provider-specific launch prompts.

Tasks:

- Research UI/UX patterns for block-based prompt/instruction editors before
  implementing the admin surface.
- Add reusable instruction blocks with drag-and-drop ordering, enable/disable
  toggles, add/delete actions, and per-block controls. First backend pass adds
  persisted `prompt_blocks` with target, name, content, enabled state, order,
  and markdown mode, plus `/prompt blocks`, `/prompt add-block`,
  `/prompt enable`, `/prompt disable`, and `/prompt render`. First UI pass adds
  a Prompt tab in settings for adding blocks, enabling/disabling blocks, and
  rendering target previews. Second UI/API pass adds edit, delete, position
  moves, and export proposals that create approvals instead of writing files
  directly. Third UI pass groups blocks by target, adds inline textarea editing,
  markdown toggles, and position editing so the builder is usable without
  browser prompt dialogs.
- Support presets for Librarian identity, operating principles, memory policy,
  tool permissions, git policy, Obsidian/knowledge-base behavior, task planning style,
  project goals, and provider caveats.
- Render Markdown preview per block plus compiled preview for each target:
  Librarian chat prompt, agent launch prompt, `AGENTS.md`, `CLAUDE.md`, and
  future provider/user identity files.
- Add block options for Markdown structure: heading level, wrapping, separators,
  and whether the block is included in prompt output, file output, or both.
- Store prompt/instruction versions so chat runs and agent jobs can cite which
  instruction set they used. First integration pass injects `librarian` blocks
  into chat prompts and `agents` blocks into background agent prompts; job
  prepared events include enabled agent block metadata.

Dependencies:

- Priority 0 chat prompt contract.
- Priority 1C tools/permissions, because prompt blocks will define tool policy
  and approval behavior.

## Priority 3: Agent Runtime Validation and Background Work

Status: In progress. Backend job dispatch exists; real Codex agent validation
still needs to be run in the user Ubuntu/WSL environment.

Goal: verify background agents as explicit project-scoped work, separate from
normal Librarian chat.

Tasks:

- Register a test project and run a real containerized Codex job with the
  mounted portable Codex profile. `librarian smoke mvp --provider codex
  --run-agent` now performs the disposable smoke flow in one command; `librarian
  runtime smoke-plan` prints the expanded manual equivalent. First validation
  fixed portable profile permissions and provider network defaults; next manual
  smoke should confirm a complete `codex exec` response inside the agent
  container.
- Verify `codex exec` works in the agent image and diagnose auth/profile issues
  without inspecting undocumented auth files.
- Keep background runs non-blocking from the chat perspective: chat records
  launch/preflight/result events but remains conversational.
- Confirm status transitions, stdout/stderr events, provider diagnostics, usage
  observation, knowledge-base run summary, and memory run observation.
- Retry a failed job and cancel a queued/running job.
- Add chat-visible compact action blocks for explicit agent launch, preflight,
  progress, and result artifacts.
- First project-overlay agent form can queue explicit `/api/jobs` work from a
  selected project with provider, read-only, and network controls. Preflight and
  richer job cards remain next UI polish.

Dependencies:

- Priority 1 diagnostics.
- User environment setup: container runtime, Codex auth, agent image.
- Priority 1C tool permission model for launch approvals.

## Priority 4: Job Dispatch Dry Run and Preflight

Status: Done for CLI no-container preflight. Admin presentation can be improved
under the agent runtime/admin work.

Goal: test the expensive/risky part of the worker path without launching a real
agent container.

Tasks:

- Add a job dispatch dry-run path that resolves provider selection, budget
  checks, project lookup, context pack, prompt build, prompt mount, and prepared
  runtime command.
- Expose dry run through CLI with `jobs preflight <job-id>`.
- Store dry-run/preflight results as job events so the admin UI can show what
  would happen.
- Keep dry run side-effect-light: it may add diagnostic events, but it must not
  mark the job completed or launch the container.

Dependencies:

- Provider routing and budget checks.
- Prompt file mount and Docker command construction.

## Priority 5: Core Manual MVP Smoke Flow

Goal: prove the main path once the user's environment is ready.

Manual flow to validate:

- Initialize Librarian home, DB, and knowledge base.
- Launch the built binary through the release-folder launcher.
- Configure Codex host authentication and explicit container mount.
- Build `librarian-agent`.
- Register a local test project.
- Queue a Codex job from CLI and admin UI.
- Run `worker --once`.
- Confirm status transitions, stdout/stderr events, provider diagnostics,
  usage observation, knowledge-base run summary, and memory run observation.
- Retry a failed job and cancel a queued/running job.

Dependencies:

- Priority 1 diagnostics.
- User environment setup: container runtime, Codex auth, agent image.

Acceptance notes:

- This flow is allowed to fail on the first real environment attempt, but every
  failure should produce an actionable diagnostic rather than a mystery stack.

## Priority 6: Admin Job Detail and Operations View

Status: Done for MVP operations. Job grouping, UI preflight action, dedicated
job detail rendering, lifecycle fields, and compact rendering for key event
types are implemented. Further layout polish can continue in the admin backlog.

Goal: the UI should be useful during the smoke flow instead of requiring CLI and
database inspection.

Tasks:

- Split jobs by status: queued, preparing/running, completed, failed,
  cancelled, and heartbeat-missed.
- Add a job detail view or detail panel for lifecycle timestamps,
  cancellation state, provider/model, retry source, and prepared command.
- Render key event types compactly: context pack, gate events, provider
  fallback, budget check/block, provider diagnostics, stdout/stderr, knowledge-base
  summary, and retry source.
- Surface failure categories prominently: runtime unavailable, image missing,
  Codex CLI missing, Codex auth missing, provider paused, rate limit, budget
  blocked, and cancelled.

Dependencies:

- Existing job events.
- Priority 4 preflight events improve this view but do not block the first UI
  pass.

## Priority 7: Runtime Cleanup and Failure Categories

Status: In progress. Structured failure category events exist for provider
pause, budget block, runtime unavailable, missing agent image, cancellation,
spawn failure, and non-zero exits. Real cleanup validation still depends on the
manual runtime smoke flow.

Goal: failed or cancelled agent runs should leave the host in a predictable
state.

Tasks:

- Verify stopped-container cleanup against actual container names/labels from
  real runs.
- Ensure cancellation kills the child process and leaves a clear event trail.
- Add structured failure categories where the worker currently records generic
  errors. First pass is implemented; expand as real provider/runtime failures
  are observed.
- Keep recovery conservative: never delete or reset project files, only
  Librarian-managed runtime artifacts/containers.

Dependencies:

- Priority 5 smoke flow, because cleanup behavior must be checked against the
  real runtime.

## Priority 8: Minimal Automated Test Harness

Status: In progress. No-container tests now cover routing fallback selection,
budget blocking, routing/budget config persistence, schedule-created provider
selection, provider diagnostics, and worker failure categorization.

Goal: make regression checks possible before full environment automation exists.

Tasks:

- Add tests for routing fallback selection and budget blocking. First pass done.
- Add tests for config persistence of routing and budget settings. First pass
  done.
- Add tests for schedule-created agent jobs preserving provider selection.
  First pass done.
- Add tests around provider diagnostic parsing. First pass done.
- Add a no-container integration path for job preflight once Priority 4 exists.
  First pass now covers `worker::preflight_job`: it builds the command, includes
  prompt blocks, writes a preflight event, and leaves the job queued without
  launching a container.
- Add tests for platform-root resolution and setup persistence without touching
  the real user home. First portable-layout test verifies explicit root setup,
  hidden/public directories, relative config persistence, and reload behavior.
- Add tests for the chat endpoint that prove it does not create jobs and that
  placeholder/self-echo memories are excluded. First pass done: endpoint test
  covers slash-command chat turns with user/assistant memory and no job creation;
  context filtering test excludes the old local-memory responder echoes.
- Add tests for the explicit chat-to-agent boundary. First pass done: `/agent
  launch ... --yes` through `/api/chat` creates exactly one queued job and a
  `queued_from_chat` event.
- Add tests for bounded iterative chat behavior. First pass done: mock-runner
  coverage verifies that repeated `search_memory` directives stop at
  `[chat].max_iterations` without calling the real provider.
- Add tests for unavailable chat-provider fallback. First pass done: mock-runner
  failure returns an actionable Codex setup response instead of an endpoint
  error or memory dump.
- Add tests for chat transcript persistence. First pass extends the chat
  endpoint slash-flow test to assert returned `session_id`, ordered user and
  assistant turns, memory links, session listing, and turn retrieval.
- Add tests that recent transcript turns are included in the Librarian prompt.
  First pass covers prompt construction for prior user/assistant turns plus the
  new user message.
- Add tests for raw transcript separation from durable memory. First pass covers
  durable retrieval filtering and `/mem recent` hiding raw chat turns.
- Add tests for durable memory correction links. First pass covers
  `/mem supersede` creating a new durable item linked through `supersedes_id`
  and `/mem contradict` suppressing the older contradicted item from retrieval.

Dependencies:

- Priority 4 for preflight integration tests.
- No container runtime required for the first test layer.

## Priority 9: OpenRouter, Claude, and API Provider Path

Status: Moving up after Codex validation. Minimal secret/grant backend and
first-pass provider adapters exist. Claude Code should be made usable for
containerized jobs soon because a working host auth is already available but
idle; OpenRouter remains the first API-key/broker path.

Goal: support both OpenRouter-style API provider testing without putting raw API
keys inside agent containers and Claude Code CLI jobs that feel to Claude like a
normal project-local launch.

Tasks:

- Add admin UI forms for storing secrets and creating grants. First pass done.
- Add per-job secret grant selection when queueing a job. First pass done for
  persisted grant tokens in CLI/admin/scheduled jobs.
- Verify OpenRouter through the host broker/proxy path.
- Extend integration smoke coverage to API-proxy providers. First pass lets
  `librarian smoke mvp --provider open-router --secret-grant-token <token>
  --run-agent` carry an existing grant into the same disposable project and
  preflight/run flow used by Codex and Claude.
- Add provider-specific proxy policies for allowed paths and HTTP methods.
- Keep Codex CLI as the primary chat and agent path until the basic flows work.
- Add OpenRouter as the first API provider once chat/agent boundaries are
  stable.
- Add Claude Code container job support before broader provider polish if Codex
  background smoke succeeds: mount/copy the Claude auth/profile in the
  provider-specific way, run Claude from the mounted project directory, pass the
  task prompt as the normal Claude launch prompt, and ensure `CLAUDE.md` is
  present when Claude starts. First implementation pass adds `[claude]` runtime
  config, optional profile mount, `CLAUDE.md` run-layer mount, `claude -p`
  launch from `/workspace/project`, and default Claude Code installation in
  `Dockerfile.agent` / `runtime build-agent-image`.
- Add provider-specific launch-shape metadata. Codex expects `codex exec` plus
  `CODEX_HOME`; Claude should behave as if launched normally in a directory
  containing `CLAUDE.md`; future providers may need different identity files,
  env vars, profile homes, stdin handling, or prompt-file strategy. First pass
  models Claude home/container home/instruction filename in config.
- Generate or mount provider instruction files during job preparation according
  to connected providers: `CLAUDE.md` for Claude, `AGENTS.md` for generic agent
  profiles, and provider/user identity files later. The prompt builder should
  own these blocks. First pass renders prompt blocks targeting `CLAUDE.md`,
  falling back to `agents`, and reports mounted instruction files in preflight.
- Add real provider setup/status in Settings -> Providers: host CLI detection,
  auth/profile path, container mount state, image support, last diagnostic,
  and action buttons for auth/bootstrap/build/check where possible. Replace
  placeholder “ready” states with data from doctor/provider diagnostics. First
  UI pass shows stored Codex/Claude runtime state and allows saving Claude
  profile/mount settings. Second pass adds Codex runtime editing plus generated
  auth/build/smoke commands in the Providers tab.
- Add Claude-specific doctor checks and worker diagnostics: host command
  present, profile/auth available, container path readable, `CLAUDE.md`
  generated/mounted, and common login/network failures. First pass adds host
  Claude/profile doctor checks and structured `claude_*` provider diagnostics.

Dependencies:

- Secret broker backend from Milestone 6.
- Provider routing and job creation provider selection from Milestone 7.

## Backlog From Completed Milestones

These are real tasks, but they are not allowed to hide inside completed
milestones. They should be pulled into active work only when they support MVP
readiness or a later planned milestone.

## Admin UI Backlog

- Show running jobs separately from queued, completed, failed, and cancelled
  jobs. Covered by MVP Priority 6.
- Show per-job lifecycle fields: created, started, last heartbeat, finished,
  cancellation requested. Covered by MVP Priority 6.
- Show recent job events inline: context pack, prepared command, stdout/stderr,
  knowledge-base summary, retry source. Covered by MVP Priority 6.
- Add compact expandable action blocks in chat for command execution, task
  creation, agent launch, memory retrieval, scheduling decisions, and provider
  routing.
- Replace raw approval slash output with first-class chat approval cards:
  summarize the requested action in human language, show affected paths and
  risk level, and provide Approve/Reject buttons. Keep approval ids available
  only in technical details/tooltips. First card pass is active for chat-created
  approvals; slash list output now shows compact summaries instead of raw JSON.
- Keep chat latency visible: pending assistant messages should show an inline
  thinking/loading state, and completed turns should have backend timing events
  plus human-readable timing metadata in the UI. First UI pass now shows
  elapsed time, iteration count, and memory-hit count on completed model turns.
- Add per-message metadata affordances: hover/click tooltips for timestamp,
  generation time, iteration count, token/cost estimate where available, memory
  hits, tool calls, and technical ids. Keep normal message labels human-facing:
  Librarian/model replies, command results, and background-agent reports should
  be visually distinct.
- Add richer message formatting for citations, quoted user history, agent
  summaries, documentation excerpts, and external-source snippets.
- Add shell-like chat input ergonomics: when the input is empty or the caret is
  at a sensible boundary, Up/Down should cycle previous submitted prompts and
  commands; typing `/` should open discoverable slash-command suggestions with
  arrow navigation and Tab completion. First browser-side pass adds local
  per-page input history and a slash palette backed by `/api/slash-commands`;
  richer server-provided argument-aware completion remains.

## Scheduler and Worker Backlog

- Promote memory compaction candidate scans into summarization jobs once the
  provider router and compaction prompt policy are in place.
- Add stronger memory/cost guardrails before allowing high worker parallelism.
  The first observed-cost daily budget guardrail exists; estimated reservations
  and concurrency-aware budget checks remain.

## Memory Backlog

- Replace or augment `local-hash` with `sqlite-vec` or official SQLite `vec1`
  when a portable extension packaging story is chosen.
- Add model-backed embedding providers after the secret broker/provider router
  can safely expose API credentials.
- Add explicit project multi-select filters to context retrieval.
- Add admin UI controls for memory backfill and context inspection.
- Add contradiction/supersession editing tools instead of only honoring stored
  links.

## Secret Broker Backlog

- Add admin UI forms for storing secrets and creating grants. Covered by MVP
  Priority 9.
- Add per-job secret grant selection when queueing a job. Covered by MVP
  Priority 9.
- Add provider-specific proxy policies for allowed paths and HTTP methods.
  Covered by MVP Priority 9.
- Add short-lived derived provider credentials where providers support them.
- Add Unix-domain/named-pipe broker transport as a tighter local alternative to
  localhost HTTP.

## Provider, Gates, and Limits Backlog

- Run a real containerized Codex self-hosting task after Podman/Docker is
  connected, the agent image is built, and host Codex auth is present. Covered
  by MVP Priority 3 and user environment setup.
- Add richer structured parsing for provider responses and CLI error formats.
- Add estimated-cost reservation before dispatch once provider adapters can
  predict request cost, so budget checks can account for the pending run instead
  of only already-observed `cost_usd`.
- Add per-project Third Eye mapping/export policy: host-visible provider logs,
  mounted container `CODEX_HOME`/Claude dirs, or Librarian-generated export
  from `usage_observations`.
- Add richer Third Eye project mapping diagnostics and last-refresh display.
- Keep gates cheap and automatic by default; expensive gates should be opt-in
  per provider, project, or session.

## Prompt Builder Backlog

Goal: keep later prompt-builder details visible without duplicating the active
Priority 2 work.

Tasks:

- Support channel/profile variants later, so host-level, project-level, and
  agent-launch instructions can differ without duplicating every block.
- Decide whether launch-time instruction bundles are injected only into prompts,
  mounted into the container as additional read-only files, or both, so agents
  can reread their operating instructions during a run.
- Add import/export for prompt-builder presets so a working Librarian identity
  can be shared across installs.
- Add diff/review UI for prompt changes before applying them to the active
  chat or agent profile.

Dependencies:

- Provider registry and routing metadata, so Librarian knows which target files
  are relevant.
- Active Prompt Builder work from MVP Priority 2.
- Admin UI improvements from MVP Priority 6.
- Context economy work, because stable instruction blocks should become part of
  the prompt prefix/cache strategy.

## Technical Debt and Codebase Audit

Status: Active audit snapshot as of May 22, 2026.

Goal: keep temporary MVP scaffolding visible so it does not silently become
product architecture.

Findings and tasks:

- `src/admin.rs` is too large and mixes active UI, legacy UI, API handlers, and
  helper logic. Split into modules: routes/API, active chat UI, settings UI,
  project map UI, and legacy removal. First small split moves request/query
  DTOs into `src/admin_models.rs`; route handlers and HTML still need further
  extraction. Second small split moves slash argument tokenization into
  `src/slash_utils.rs`.
- `src/admin.rs` still contains old inactive HTML functions behind
  `#[allow(dead_code)]`. Delete or move them into archived design notes once
  the new chat-first UI has covered the needed controls.
- `/api/chat` now has a first Codex-backed path, but it is still embedded in
  `src/admin.rs`; move chat prompting/provider execution into dedicated modules.
  First split done: iterative chat loop, Codex chat runner, prompt assembly,
  directive parsing, and chat-loop tests now live in `src/chat.rs`.
- Legacy `local-memory-responder` memories may remain in existing user
  databases. Keep filtering them from chat context and add a cleanup/backfill
  command later.
- The previous hardcoded `looks_like_agent_request` intent detector had mojibake
  Russian literals and was removed. Do not reintroduce multilingual intent
  heuristics; use slash commands and the tool/permission intent layer.
- Chat now has first-pass `chat_sessions`/`chat_turns`, but transcript work
  still needs UI session switching, pruning/export policy, and a cleaner module
  boundary outside `src/admin.rs`.
- Memory retrieval lacks filters for source/mode, so placeholder assistant
  output and low-value operational messages can pollute context. First filter
  pass now excludes memory marked `durability=transcript` or
  `memory_role=raw_chat_turn` from durable context retrieval; the current
  session transcript is supplied separately.
- `local-hash` embeddings are useful for offline MVP plumbing but weak for
  semantic quality. Keep as fallback; add real embedding providers later.
- Codex agent adapter exists, but real containerized Codex execution has not
  been validated end to end in the Ubuntu/WSL install.
- OpenRouter adapter is a first-pass shell command against the broker and has
  not been validated as a production chat/agent provider.
- Claude Code adapter is a minimal command wrapper and lacks real auth/config
  UX, diagnostics, and validation. It also needs provider-specific launch
  semantics so Claude sees a normal project directory with `CLAUDE.md` rather
  than a generic stdin-only runner.
- Provider cost/budget logic uses observed spend only; no estimated reservation
  exists before dispatch.
- Gate/redaction logic is heuristic. It can over-capture high-entropy strings
  and needs review/undo UX plus stronger tests.
- Tool permissions now exist as a first-pass policy/audit layer. Remaining debt:
  richer interactive approval UX, policy UI, and clearer assistant-initiated
  tool execution review.
- Admin authentication is not implemented. Before exposing non-localhost admin
  access, add auth, CSRF/session handling where relevant, and clear bind/router
  guidance.
- External HTTP access by IP/router is not a current MVP target. Track it as
  polish after auth exists.
- Project library UI cannot yet create/link memory folders and working
  directories from the admin surface.
- Knowledge-base writes are basic Markdown files without conflict handling, rename
  policy, or richer Obsidian link maintenance.
- Installer upgrade still rebuilds from source through git. Keep command UX,
  but replace internals with release binary downloads once releases are stable.
- Windows path remains developer/bootstrap-oriented; Ubuntu is the current
  golden path.
- Automated coverage is mostly unit/no-container. Add endpoint tests for chat,
  memory filtering, slash commands, tool permission decisions, and project
  library operations.

## Planned

These milestones stay behind MVP readiness. Items can move forward only when
they unblock testing or stabilize the MVP path.

## Milestone 8: Context Economy

Status: Planned.

- Prompt prefix stabilization.
- Provider-aware prompt caching.
- Project context packs.
- Block-based instruction bundles for stable identity and operating policy
  prefixes.
- Memory summarization.
- Optional terse-output modes for non-human intermediate steps.
- Cache policy that puts stable instructions and project rules first, volatile
  task data last.

## Milestone 9: Runtime and Git Policy

Status: Planned.

- Per-project git strategy.
- Protected branch patterns, with `main` and `master` protected by default.
- Commit/push policy rules per project and branch.
- Remote allowlists.
- Audit records for every git write action performed by an agent.
- Runtime policy presets for `local`, `provider-proxy`, and `open-network`
  sessions.

## Milestone 10: Distribution and Bootstrap

Status: Partly pulled forward for MVP readiness. Keep broader installer polish
planned.

- Minimal self-deploying config for Windows, Linux, and macOS. First pass:
  `setup`, release-folder launcher, and CI artifacts.
- Windows runtime support for Podman by default, with Rancher Desktop/dockerd
  compatibility documented.
- Linux/macOS support for Docker or Podman.
- Runtime diagnostics for container engine availability, rootless mode, and
  project mount behavior.
- First-run auth flow for Codex, then provider selection later.

## Research Notes Folded Into Direction

- Docker rootless mode is preferred when available. It reduces daemon/container
  privilege but does not replace strict mount and network policy.
  Source: <https://docs.docker.com/engine/security/rootless/>
- Bind mounts are powerful and can expose host files to containers, so every
  mount is explicit per session/project.
  Source: <https://docs.docker.com/engine/storage/bind-mounts/>
- Docker Swarm secrets are not suitable as the default local secret mechanism
  because the project should not depend on Swarm.
  Source: <https://docs.docker.com/engine/swarm/secrets/>
- Podman on Windows uses a WSL2-backed machine and is the preferred default
  Docker Desktop alternative for this project.
  Source: <https://podman.io/docs/installation>
- Rancher Desktop is another open-source Windows option when Docker API
  compatibility through dockerd/moby is needed.
  Sources: <https://www.rancher.com/products/rancher-desktop>,
  <https://docs.rancherdesktop.io/ui/preferences/container-engine/general>
- Codex CLI supports non-interactive `codex exec`, but Librarian should avoid
  depending on undocumented auth file internals. Authentication is host-managed
  until brokered provider flows are stable.
  Sources: <https://help.openai.com/en/articles/11096431>,
  <https://help.openai.com/en/articles/11381614-api-codex-cli-and-sign-in-with-chatgpt>
- Obsidian-compatible storage is plain Markdown inside the knowledge base, so Librarian can
  use Git-managed Markdown and YAML frontmatter without depending on the
  Obsidian app.
  Source: <https://obsidian.md/help/data-storage>
- OpenRouter exposes API key limit/credit information. LiteLLM is a useful
  reference for retry, fallback, load balancing, and cost tracking, but
  Librarian keeps CLI and API adapters independent.
  Sources: <https://openrouter.ai/docs/api/reference/limits>,
  <https://docs.litellm.ai/>
- Prompt caching requires stable prompt prefixes. OpenAI caching is automatic
  on supported models for sufficiently long prompts; Anthropic caching requires
  cache controls and exact matching.
  Sources: <https://platform.openai.com/docs/guides/prompt-caching/prompt-caching>,
  <https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching>
- SQLite vector direction: prefer official `vec1` when available, keep
  `sqlite-vec` as a practical packaging fallback, and avoid hard-coding older
  `sqlite-vss` as the only backend.
  Sources: <https://sqlite.org/vec1>,
  <https://github.com/asg017/sqlite-vec>,
  <https://github.com/asg017/sqlite-vss>

## Open Questions

- Should first real Codex runs use direct container internet access, or should
  provider-proxy mode be required before running real tasks?
- Should the local encrypted vault support an app passphrase, OS keychain
  wrapping, or both?
- Should budget guardrails reserve estimated spend before dispatch, or only
  block on already-observed spend until provider cost prediction is reliable?
- Should Third Eye be read-only observer only, or should Librarian generate
  Third Eye-compatible exports for all provider sessions?
