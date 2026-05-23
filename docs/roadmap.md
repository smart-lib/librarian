# Roadmap

This roadmap is the canonical project planning document. Short-lived notes,
research findings, and todo items should be folded back here once they affect
the product direction.

## Current Status

- Branch: `develop`.
- Baseline checkpoint: `main` contains the initial scaffold commit.
- Current phase: working Librarian chat MVP.
- Next implementation focus: replace the local memory responder with a real
  provider-backed Librarian chat loop, then add explicit tools, permissions,
  and background agent launch as separate actions.

## Product Defaults

- MVP chat provider: Codex CLI on the host profile already configured through
  Librarian auth. Background coding agents also use Codex CLI first.
- Containers have no network access by default.
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
- The Obsidian-compatible vault is global at the Librarian root, so chats,
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
- Job event log for status, stdout, stderr, and vault notes.

## Milestone 2: Usable Agent Runs

Status: Done for MVP code path. Runtime hardening is tracked below.

- Build `librarian-agent` image.
- Run Codex in a container.
- Stream logs into SQLite with UI refresh.
- Persist run summaries into the Markdown vault.
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
- Vault editor for basic notes.
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

- Local encrypted vault.
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
  secret vault, replace them with vault/grant references, and audit the action.
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
- Keep `/api/chat` as conversation only. It must not create background jobs or
  wait for agent execution.
- Remove current self-echo behavior: placeholder assistant replies are filtered
  from the chat prompt, and new responses are stored as `mode=codex-chat`.
- Add persisted chat runtime settings for Codex timeout, memory hit limit, and
  future max iterations. First pass done in `[chat]`.
- Save useful user and assistant turns into memory, but distinguish raw chat
  transcript from durable facts/decisions/instructions.
- Add a small chat transcript model: session/thread id, ordered turns, selected
  project context, and durable memory links.
- Add a clear fallback when the chat provider is unavailable: actionable
  “Codex auth/runtime missing” message, not memory dump output.
- Add tests for the chat endpoint that prove it does not create jobs and that
  placeholder/self-echo memories are excluded.

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
  coverage; bounded async loop coverage remains.
- Add admin controls/readout for chat iteration settings and optionally expose a
  compact developer trace when diagnostics are enabled.

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
  vault path, runtime command, agent image, Codex host CLI, Codex profile path,
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
- After Codex auth, build `librarian-agent` and run the MVP smoke flow.

## Priority 1B: Project Library and Friendly Admin UX

Status: In progress. Chat-first shell is active and `/api/chat` is separated
from background agent jobs; backend project library workflows remain. Real chat
model integration moved to Priority 0.

Goal: make Librarian feel like a project library first, and hide low-level
agent dispatch mechanics until they are needed.

Tasks:

- Keep the browser viewport fixed and use application-owned scroll regions for
  chat, settings tabs, and project map panels. First pass done.
- Make chat the primary surface; move providers, schedules, secrets, budgets,
  and events behind a settings button and full-screen tabbed overlay. First
  pass done; richer settings controls can be restored inside tabs only.
- Add a project-map view that renders registered projects as a branching node
  tree. First pass done against the project registry; backend map data is now
  available through `/api/project-map` and `/project map`, annotated with
  library visual kinds (`book`, `shelf`, `rack`, `artifact`) for the richer UI.
  Chat-first project overlay now renders the annotated library tree with a
  legend and linked/detached counts.
- Keep low-level dispatch fields such as provider, project id, secret grant
  token, and network mode out of the main chat composer. First pass done with
  Codex as the default MVP provider and the selected/first project as context.
- Keep Librarian chat as a normal AI conversation surface, usable without a
  specific project. First pass separates it from job dispatch; real model
  response is tracked in Priority 0.
- Polish the main chat shell for actual conversation: full-width thread and
  prompt input, Enter-to-send with Ctrl+Enter newline, floating corner controls,
  and a centered pull-tab identity marker. First pass done.
- Move background agent launch into explicit project actions and command blocks,
  so agents can run without interrupting the Librarian conversation.
- Define the project library model: Markdown project memory folders live under
  `Librarian/Library/projects/{ProjectName}` by default, and each can attach to an
  external working directory mounted into agent containers. Refined model:
  any folder or Markdown note in `Library` can behave as a project-like library
  item (book/shelf/rack/row metaphor), while implementation/product folders stay
  separate attached working directories. First DB pass adds optional
  `library_path` to project records while keeping existing `path` as the
  worker-mounted implementation/workspace path.
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
  current chat scope.
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
  rendering target previews.
- Support presets for Librarian identity, operating principles, memory policy,
  tool permissions, git policy, Obsidian/vault behavior, task planning style,
  project goals, and provider caveats.
- Render Markdown preview per block plus compiled preview for each target:
  Librarian chat prompt, agent launch prompt, `AGENTS.md`, `CLAUDE.md`, and
  future provider/user identity files.
- Add block options for Markdown structure: heading level, wrapping, separators,
  and whether the block is included in prompt output, file output, or both.
- Store prompt/instruction versions so chat runs and agent jobs can cite which
  instruction set they used.

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
  mounted portable Codex profile. `librarian runtime smoke-plan` now prints the
  exact WSL/Ubuntu command sequence for this disposable smoke flow.
- Verify `codex exec` works in the agent image and diagnose auth/profile issues
  without inspecting undocumented auth files.
- Keep background runs non-blocking from the chat perspective: chat records
  launch/preflight/result events but remains conversational.
- Confirm status transitions, stdout/stderr events, provider diagnostics, usage
  observation, vault run summary, and memory run observation.
- Retry a failed job and cancel a queued/running job.
- Add chat-visible compact action blocks for explicit agent launch, preflight,
  progress, and result artifacts.

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

- Initialize Librarian home, DB, and vault.
- Launch the built binary through the release-folder launcher.
- Configure Codex host authentication and explicit container mount.
- Build `librarian-agent`.
- Register a local test project.
- Queue a Codex job from CLI and admin UI.
- Run `worker --once`.
- Confirm status transitions, stdout/stderr events, provider diagnostics,
  usage observation, vault run summary, and memory run observation.
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
  fallback, budget check/block, provider diagnostics, stdout/stderr, vault
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
- Add tests for platform-root resolution and setup persistence without touching
  the real user home.

Dependencies:

- Priority 4 for preflight integration tests.
- No container runtime required for the first test layer.

## Priority 9: OpenRouter, Claude, and API Provider Path

Status: Later MVP. Minimal secret/grant backend and first-pass provider
adapters exist, but OpenRouter and Claude are not the first priority until the
Librarian chat and Codex paths work.

Goal: support OpenRouter-style provider testing without putting raw API keys
inside agent containers.

Tasks:

- Add admin UI forms for storing secrets and creating grants. First pass done.
- Add per-job secret grant selection when queueing a job. First pass done for
  persisted grant tokens in CLI/admin/scheduled jobs.
- Verify OpenRouter through the host broker/proxy path.
- Add provider-specific proxy policies for allowed paths and HTTP methods.
- Keep Codex CLI as the primary chat and agent path until the basic flows work.
- Add OpenRouter as the first API provider once chat/agent boundaries are
  stable.
- Add Claude Code after OpenRouter and Codex are verified, including provider
  auth/config UX.

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
  vault summary, retry source. Covered by MVP Priority 6.
- Add compact expandable action blocks in chat for command execution, task
  creation, agent launch, memory retrieval, scheduling decisions, and provider
  routing.

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
  project map UI, and legacy removal.
- `src/admin.rs` still contains old inactive HTML functions behind
  `#[allow(dead_code)]`. Delete or move them into archived design notes once
  the new chat-first UI has covered the needed controls.
- `/api/chat` now has a first Codex-backed path, but it is still embedded in
  `src/admin.rs`; move chat prompting/provider execution into dedicated modules.
- Legacy `local-memory-responder` memories may remain in existing user
  databases. Keep filtering them from chat context and add a cleanup/backfill
  command later.
- The previous hardcoded `looks_like_agent_request` intent detector had mojibake
  Russian literals and was removed. Do not reintroduce multilingual intent
  heuristics; use slash commands and the tool/permission intent layer.
- Chat has no session/thread model yet; messages are stored as generic memory
  without ordered transcript structure.
- Memory retrieval lacks filters for source/mode, so placeholder assistant
  output and low-value operational messages can pollute context.
- `local-hash` embeddings are useful for offline MVP plumbing but weak for
  semantic quality. Keep as fallback; add real embedding providers later.
- Codex agent adapter exists, but real containerized Codex execution has not
  been validated end to end in the Ubuntu/WSL install.
- OpenRouter adapter is a first-pass shell command against the broker and has
  not been validated as a production chat/agent provider.
- Claude Code adapter is a minimal command wrapper and lacks real auth/config
  UX, diagnostics, and validation.
- Provider cost/budget logic uses observed spend only; no estimated reservation
  exists before dispatch.
- Gate/redaction logic is heuristic. It can over-capture high-entropy strings
  and needs review/undo UX plus stronger tests.
- Tool permissions do not exist yet. Any future filesystem, memory, settings,
  or agent-launch operation must go through a policy/audit layer.
- Admin authentication is not implemented. Before exposing non-localhost admin
  access, add auth, CSRF/session handling where relevant, and clear bind/router
  guidance.
- External HTTP access by IP/router is not a current MVP target. Track it as
  polish after auth exists.
- Project library UI cannot yet create/link memory folders and working
  directories from the admin surface.
- Vault writes are basic Markdown files without conflict handling, rename
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
- Obsidian-compatible storage is plain Markdown inside a vault, so Librarian can
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
