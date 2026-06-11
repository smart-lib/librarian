# Self-Hosting Readiness

This document tracks how ready Librarian is to continue developing itself by
launching supervised background agents against the Librarian repository.

## Current Verdict

Status: partially ready for supervised self-hosting.

Librarian can already register its own repository as a project, retrieve
project-scoped context, prepare containerized agent jobs, mount provider
profiles, pass prompt-builder instruction files, record job events, and keep
run summaries in the knowledge base. That is enough for cautious read-only
inspection tasks and small manually reviewed implementation tasks.

It is not yet ready for unattended autonomous development loops. The missing
pieces are stronger self-host smoke coverage with real provider runs, richer
agent result review, automated patch/test policy gates, and safer UI flows for
approving write/commit/push operations.

## One-Line Checks

Preflight only:

```bash
librarian --home "$HOME/Librarian" smoke self-host --project-path "$PWD"
```

Real read-only provider call after preflight:

```bash
librarian --home "$HOME/Librarian" smoke self-host --project-path "$PWD" --run-agent
```

Broad local checks:

```bash
librarian --home "$HOME/Librarian" doctor --smoke
```

Provider readiness:

```bash
librarian --home "$HOME/Librarian" smoke providers
```

## Ready Capabilities

- Stable portable root layout: `.app`, `.cfg`, `.mdb`, `Library`, `Projects`.
- Codex-backed chat and containerized Codex agent path are working on the
  Ubuntu/WSL golden path.
- Project registry supports attaching a knowledge-base path to a working
  repository path.
- Agent jobs are explicit slash/API/CLI actions; normal chat does not
  accidentally spawn background work.
- Jobs have lifecycle events, preflight, cancel, retry, stdout/stderr capture,
  and Markdown run summaries.
- Context retrieval supports project-scoped memory and tree-aware context
  primitives.
- Tool permissions and approvals exist for file/project/memory/settings
  operations.
- Prompt builder blocks can generate provider instruction files such as
  `CLAUDE.md` and generic agent guidance.
- Provider diagnostics are shared by doctor, admin API, and provider smoke.
- `smoke self-host` now checks that the Librarian repository can be registered
  and prepared for a supervised read-only agent job.

## Main Gaps Before Continuous Self-Development

- A real containerized self-host Codex task still needs repeated validation on
  the user's target Ubuntu host.
- Agent-written patches need a stronger review loop: diff summary, tests run,
  approval, commit policy, and rollback guidance.
- Automatic write tasks should require project policy gates, not only prompt
  instructions.
- Budget/cost control is observed-spend based; estimated reservations before
  dispatch are still missing.
- Admin auth is missing, so remote admin/channel exposure is not ready.
- `src/admin.rs` remains too large and still contains mixed UI/API/helper
  responsibilities.
- OpenRouter and Claude Code paths exist but are not yet proven as production
  self-hosting providers.
- Prompt/profile variants are still first-pass; host, channel, and provider
  profiles need cleaner separation.

## Practical Operating Mode Now

Use Librarian for supervised self-development in short loops:

1. Discuss the task in chat and select the Librarian project context.
2. Launch a read-only agent inspection or preflight first.
3. Review job events and generated run summary.
4. Only then launch a narrow write task.
5. Run `cargo test` or `doctor --smoke`.
6. Commit manually or through an explicit approved action.

Avoid unattended multi-agent write loops until patch review, policy gates, and
provider-specific smoke runs are stronger.
