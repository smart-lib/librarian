# Self-Hosting Readiness

This document tracks how ready Librarian is to continue developing itself by
launching supervised background agents against the Librarian repository.

## Current Verdict

Status: partially ready for supervised self-hosting.

Librarian can already register its own repository as a project, retrieve
project-scoped context, prepare containerized agent jobs, mount provider
profiles, pass prompt-builder instruction files, record job events, collect a
worktree review snapshot, and keep run summaries in the knowledge base. That is
enough for cautious read-only inspection tasks and small manually reviewed
implementation tasks. Commit/push policy can now be checked explicitly before
any human or future automated approval step.

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

Review a job's repository state before continuing:

```bash
librarian --home "$HOME/Librarian" jobs review <job-id> --run-tests
```

Check commit/push policy gates:

```bash
librarian --home "$HOME/Librarian" jobs gate <job-id> --action commit
librarian --home "$HOME/Librarian" jobs gate <job-id> --action push
librarian --home "$HOME/Librarian" jobs gate <job-id> --action revert
```

Preview the outgoing commits and diff before a manual push:

```bash
librarian --home "$HOME/Librarian" jobs push-plan <job-id>
```

Create a gated local commit approval:

```bash
librarian --home "$HOME/Librarian" jobs propose-git <job-id> --action commit --message "Describe the change"
```

Plan and propose a local revert if the approved commit is wrong:

```bash
librarian --home "$HOME/Librarian" jobs revert-plan <job-id> --commit <sha>
librarian --home "$HOME/Librarian" jobs propose-git <job-id> --action revert --commit <sha>
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
- `jobs review` records git status, diff summaries, optional Cargo test output,
  and a machine-readable recommendation as a job event.
- `jobs gate` records whether commit, push, or revert is allowed by project policy,
  branch protection, branch pattern, dirty state, and upstream state.
- `jobs push-plan` records the upstream branch, outgoing commit list, ahead
  count, remote list, and outgoing diff stat before a manual push.
- `jobs propose-git` creates a git approval only after the gate passes. Commit
  and revert execution recheck policy before mutating the repository.
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
- Agent-written patches still need richer UI approval cards. CLI diff/test
  review, commit/push/revert policy gates, push planning, gated commit
  approvals, and revert proposals now exist as machine contracts. Push remains
  manual after policy review.
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
4. Run `jobs review <job-id> --run-tests`.
5. Run `jobs gate <job-id> --action commit` before any commit.
6. Create `jobs propose-git <job-id> --action commit --message ...` when the
   review and gate are acceptable.
7. If that commit is wrong, run `jobs revert-plan <job-id> --commit <sha>` and
   approve an explicit revert proposal.
8. Only then launch a narrow write task or approve follow-up work.
9. Run `cargo test` or `doctor --smoke`.
10. Push manually only after a separate push gate and `jobs push-plan` review.

Avoid unattended multi-agent write loops until patch review, policy gates, and
provider-specific smoke runs are stronger.
