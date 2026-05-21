# Roadmap

This roadmap is the canonical project planning document. Short-lived notes,
research findings, and todo items should be folded back here once they affect
the product direction.

## Current Status

- Branch: `develop`.
- Baseline checkpoint: `main` contains the initial scaffold commit.
- Current phase: MVP readiness after Milestone 7 feature buildout.
- Next implementation focus: make the core flow testable end to end from a
  real user launch path, then hand off to environment setup for manual and
  automated testing.

## Product Defaults

- MVP provider: Codex CLI.
- Containers have no network access by default.
- Project mounts are read-write by default, configurable per run/project.
- No required paid or proprietary external secret service.
- Localhost admin UI is the default interaction path.
- First-run setup chooses a stable Librarian root. Silent/default setup uses
  `%APPDATA%\Librarian` on Windows, `~/Librarian` on Linux, and
  `~/Library/Application Support/Librarian` on macOS; `--home` and
  `LIBRARIAN_HOME` support portable roots.
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

## MVP Readiness

These tasks come before new product milestones. The goal is not more breadth;
it is making the current MVP reliable enough that the environment can be set up
and the core flows can be tested manually and automatically.

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

## Priority 2: Job Dispatch Dry Run and Preflight

Status: Done for CLI no-container preflight. Admin presentation can be improved
under Priority 4.

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

## Priority 3: Core Manual MVP Smoke Flow

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

## Priority 4: Admin Job Detail and Operations View

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
- Priority 2 preflight events improve this view but do not block the first UI
  pass.

## Priority 5: Runtime Cleanup and Failure Categories

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

- Priority 3 smoke flow, because cleanup behavior must be checked against the
  real runtime.

## Priority 6: Minimal Automated Test Harness

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
- Add a no-container integration path for job preflight once Priority 2 exists.
- Add tests for platform-root resolution and setup persistence without touching
  the real user home.

Dependencies:

- Priority 2 for preflight integration tests.
- No container runtime required for the first test layer.

## Priority 7: Secret and API Provider MVP Path

Status: In progress. Minimal admin UI exists for storing redacted secrets,
listing recent grants, and creating capability grants. Jobs and scheduled agent
tasks can now carry a grant token into the worker/container. Provider proxy
policy hardening remains.

Goal: support OpenRouter-style provider testing without putting raw API keys
inside agent containers.

Tasks:

- Add admin UI forms for storing secrets and creating grants. First pass done.
- Add per-job secret grant selection when queueing a job. First pass done for
  persisted grant tokens in CLI/admin/scheduled jobs.
- Verify OpenRouter through the host broker/proxy path.
- Add provider-specific proxy policies for allowed paths and HTTP methods.
- Keep Codex CLI as the primary MVP path; this is the secondary provider/API
  validation path.

Dependencies:

- Secret broker backend from Milestone 6.
- Provider routing and job creation provider selection from Milestone 7.

## Backlog From Completed Milestones

These are real tasks, but they are not allowed to hide inside completed
milestones. They should be pulled into active work only when they support MVP
readiness or a later planned milestone.

## Admin UI Backlog

- Show running jobs separately from queued, completed, failed, and cancelled
  jobs. Covered by MVP Priority 4.
- Show per-job lifecycle fields: created, started, last heartbeat, finished,
  cancellation requested. Covered by MVP Priority 4.
- Show recent job events inline: context pack, prepared command, stdout/stderr,
  vault summary, retry source. Covered by MVP Priority 4.
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
  Priority 7.
- Add per-job secret grant selection when queueing a job. Covered by MVP
  Priority 7.
- Add provider-specific proxy policies for allowed paths and HTTP methods.
  Covered by MVP Priority 7.
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

## Agent Instruction Authoring Backlog

Goal: let the user visually compose provider-specific agent instruction files
and launch-time prompt layers from reusable blocks instead of hand-editing
separate Markdown files.

Tasks:

- Add a visual admin editor for instruction blocks with drag-and-drop ordering,
  enable/disable toggles, add/delete actions, and per-block controls.
- Support block presets for identity, operating principles, git policy,
  Obsidian/vault behavior, task planning style, project goals, and provider
  caveats.
- Render a large Markdown preview for each block and a full compiled preview
  for each target output.
- Add top-row block options for Markdown structure, such as heading level,
  wrapping, separators, and whether the block is included in file output,
  launch prompt output, or both.
- Compile active blocks into provider-specific files such as `AGENTS.md`,
  `CLAUDE.md`, and future provider/user identity files, based on configured or
  connected providers.
- Support channel/profile variants later, so host-level, project-level, and
  agent-launch instructions can differ without duplicating every block.
- Decide whether launch-time instruction bundles are injected only into prompts,
  mounted into the container as additional read-only files, or both, so agents
  can reread their operating instructions during a run.

Dependencies:

- Provider registry and routing metadata, so Librarian knows which target files
  are relevant.
- Admin UI improvements from MVP Priority 4.
- Context economy work, because stable instruction blocks should become part of
  the prompt prefix/cache strategy.

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
