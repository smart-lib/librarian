# Roadmap

## Milestone 1: Local Core

- Rust CLI scaffold.
- SQLite bootstrap.
- Project registry.
- Localhost admin shell.
- Docker runner abstraction.
- Codex provider shell.
- Worker command for queued jobs.
- Job event log for status, stdout, stderr, and vault notes.

## Milestone 2: Usable Agent Runs

- Build `librarian-agent` image.
- Run Codex in a container.
- Stream logs into SQLite with UI refresh.
- Persist run summaries into the Markdown vault.
- Feed enriched memory context into provider prompts.
- Store run outcomes back into memory.
- Add pause/cancel/retry lifecycle commands.
- Heartbeat and cancellation tracking.

## Milestone 3: Admin UI

- Chat-first interface.
- Manual settings panels.
- Project registry editor.
- Job monitor.
- Operational dashboard for current queue, running jobs, worker status,
  heartbeats, retries, cancellations, and recent job events.
- Distinct chat rendering for Librarian actions such as commands, task
  creation, agent launch, memory retrieval, and scheduler decisions, visually
  separated from natural-language replies.
- Vault editor for basic notes.
- Codex auth status check and onboarding.

## Milestone 4: Schedules and Heartbeats

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

- Embedding provider abstraction with a local deterministic backend.
- SQLite-backed vector storage in `memory_embeddings`.
- Local brute-force vector scoring over scoped SQLite candidates.
- `vec1` or `sqlite-vec` integration remains a future backend behind the same
  context-pack contract.
- FTS/lexical fallback and complement.
- Recency-aware retrieval ranking.
- Project/activity scoped context packs.
- Contradiction and supersession handling.
- CLI/admin memory status and embedding backfill.

## Milestone 6: Secret Broker

- Local encrypted vault.
- Windows DPAPI and explicit AES-GCM fallback encryption modes.
- Capability grants per provider/session/job with TTL and max-use limits.
- Host-side broker HTTP endpoint.
- Audited secret store, grant, and resolve/proxy use.
- Provider proxy mode for OpenAI/OpenRouter-style HTTP APIs.

## Milestone 7: Provider Router

- Prompt/response gate pipeline for lightweight validation, filtering,
  transformation, provider-specific prompt shaping, and redaction.
- Automatic secret detection before prompt enrichment: move raw tokens into the
  secret vault, replace them with vault/grant references, and audit the action.
- Output/tool-result leak scanning: redact known secrets or secret-shaped values
  before they are stored, displayed, or reintroduced into prompts.
- OpenRouter API adapter.
- Claude Code adapter.
- Model/provider cost metadata.
- Rate limit and budget policies.
- Provider pause windows when sessions hit rate, quota, or spend limits.
- Usage/cost observation from provider responses, CLI logs, and indirect
  session telemetry.
- Optional integration point for open cost-analysis tools such as Third Eye
  instead of hardcoding all accounting logic.
- Third Eye probes for health, provider list, refresh, and direct read-only
  SQLite summaries.
- Container log export/mount strategy so Third Eye can see per-project
  Librarian worker sessions instead of only host-user Codex/Claude sessions.
- Retry and fallback routing.

## Milestone 8: Context Economy

- Prompt prefix stabilization.
- Provider-aware prompt caching.
- Project context packs.
- Memory summarization.
- Optional terse-output modes for non-human intermediate steps.
