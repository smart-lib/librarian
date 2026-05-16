# Roadmap

This roadmap is the canonical project planning document. Short-lived notes,
research findings, and todo items should be folded back here once they affect
the product direction.

## Current Status

- Branch: `develop`.
- Baseline checkpoint: `main` contains the initial scaffold commit.
- Current phase: Milestone 7, provider routing, gates, and cost/limit telemetry.
- Next implementation focus: admin UI surfaces for provider status, Third Eye
  status, queue/worker diagnostics, and gate/action rendering.

## Product Defaults

- MVP provider: Codex CLI.
- Containers have no network access by default.
- Project mounts are read-write by default, configurable per run/project.
- No required paid or proprietary external secret service.
- Localhost admin UI is the default interaction path.
- The Obsidian-compatible vault is global at the Librarian root, so chats,
  project notes, decisions, and background runs across many projects share one
  knowledge base.
- Agents have full privileges inside the mounted project boundary by default.
  Irreversible behavior such as commits and pushes is controlled by configurable
  project/git policy, not by hardcoded global blocking.
- Worker parallelism defaults to `1` and can be raised through config or CLI.

## Completed

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

Status: Done for MVP behavior, with provider/runtime hardening continuing in
later milestones.

- Build `librarian-agent` image.
- Run Codex in a container.
- Stream logs into SQLite with UI refresh.
- Persist run summaries into the Markdown vault.
- Feed enriched memory context into provider prompts.
- Store run outcomes back into memory.
- Add pause/cancel/retry lifecycle commands.
- Heartbeat and cancellation tracking.

## Milestone 3: Admin UI

Status: MVP done, richer operational views planned.

- Chat-first interface.
- Manual settings panels.
- Project registry editor.
- Job monitor.
- Worker capacity panel with configured slots, active jobs, queued jobs, and
  available slots.
- Recent Librarian actions panel backed by structured system events.
- Vault editor for basic notes.
- Codex auth status check and onboarding.

Remaining:

- Show running jobs separately from queued, completed, failed, and cancelled
  jobs.
- Show per-job lifecycle fields: created, started, last heartbeat, finished,
  cancellation requested.
- Show recent job events inline: context pack, prepared command, stdout/stderr,
  vault summary, retry source.
- Add compact expandable action blocks in chat for command execution, task
  creation, agent launch, memory retrieval, scheduling decisions, and provider
  routing.

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

Remaining:

- Promote memory compaction candidate scans into summarization jobs once the
  provider router and compaction prompt policy are in place.
- Add memory/cost guardrails before allowing high worker parallelism.

## Milestone 5: Vector Memory

Status: Done for local MVP, richer backends planned.

- Embedding provider abstraction with a local deterministic backend.
- SQLite-backed vector storage in `memory_embeddings`.
- Local brute-force vector scoring over scoped SQLite candidates.
- FTS/lexical fallback and complement.
- Recency-aware retrieval ranking.
- Project/activity scoped context packs.
- Contradiction and supersession handling.
- CLI/admin memory status and embedding backfill.

Remaining:

- Replace or augment `local-hash` with `sqlite-vec` or official SQLite `vec1`
  when a portable extension packaging story is chosen.
- Add model-backed embedding providers after the secret broker/provider router
  can safely expose API credentials.
- Add explicit project multi-select filters to context retrieval.
- Add admin UI controls for memory backfill and context inspection.
- Add contradiction/supersession editing tools instead of only honoring stored
  links.

## Milestone 6: Secret Broker

Status: Done for local MVP, hardening planned.

- Local encrypted vault.
- Windows DPAPI and explicit AES-GCM fallback encryption modes.
- Capability grants per provider/session/job with TTL and max-use limits.
- Host-side broker HTTP endpoint.
- Audited secret store, grant, and resolve/proxy use.
- Provider proxy mode for OpenAI/OpenRouter-style HTTP APIs.

Remaining:

- Add admin UI forms for storing secrets and creating grants.
- Add per-job secret grant selection when queueing a job.
- Add provider-specific proxy policies for allowed paths and HTTP methods.
- Add short-lived derived provider credentials where providers support them.
- Add Unix-domain/named-pipe broker transport as a tighter local alternative to
  localhost HTTP.

## In Progress

## Milestone 7: Provider Router, Gates, and Limits

Status: In progress.

Done:

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

Remaining:

- Run a real containerized Codex self-hosting task after Podman/Docker is
  connected, the agent image is built, and host Codex auth is present.
- Add clearer Codex auth diagnostics; missing/invalid auth currently appears as
  provider output with `401 Missing bearer`.
- Add richer structured parsing for provider responses and CLI error formats.
- Route new work to fallback providers when the selected provider/model is
  paused and a fallback policy is configured.
- Add budget policies before dispatch, including per-provider, per-project, and
  per-day caps.
- Add per-project Third Eye mapping/export policy: host-visible provider logs,
  mounted container `CODEX_HOME`/Claude dirs, or Librarian-generated export
  from `usage_observations`.
- Add admin UI cards for Third Eye status, last refresh, total cost, provider
  limits, and project mapping diagnostics.
- Add admin UI controls for provider pause/resume and routing preferences.
- Keep gates cheap and automatic by default; expensive gates should be opt-in
  per provider, project, or session.

## Planned

## Milestone 8: Context Economy

Status: Planned.

- Prompt prefix stabilization.
- Provider-aware prompt caching.
- Project context packs.
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

Status: Planned.

- Minimal self-deploying config for Windows, Linux, and macOS.
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
- What budget policy should block dispatch before full provider routing is
  finished?
- Should Third Eye be read-only observer only, or should Librarian generate
  Third Eye-compatible exports for all provider sessions?
