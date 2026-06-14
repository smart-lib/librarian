# Self-Hosting Readiness

This document tracks how ready Librarian is to continue developing itself by
launching supervised background agents against the Librarian repository.

## Current Verdict

Status: ready for cautious supervised Codex self-hosting; not ready for
unattended autonomous development loops.

Librarian can register its own repository as a project, retrieve project-scoped
context, prepare and run containerized Codex jobs, mount provider profiles, pass
prompt-builder instruction files, record job events, write run summaries, build
review packets, expose those packets in chat/UI, and create commit/revert
approval proposals from the review card. External admin binds are protected by
token auth. That is enough for cautious read-only inspection tasks and narrow
write tasks where a human approves review, commit, revert, and push decisions.

It is still not ready for unattended autonomous development loops. The remaining
barrier is operational confidence: repeated real Codex self-host runs on the
target Ubuntu host, routine use of the opt-in test-backed review gate before
approval, and a final policy decision for when Librarian may ask agents to write
without first asking the user.

## One-Line Checks

Preflight only:

```bash
librarian --home "$HOME/Librarian" smoke self-host --project-path "$PWD"
```

Real read-only provider call after preflight:

```bash
librarian --home "$HOME/Librarian" smoke self-host --project-path "$PWD" --run-agent
```

Full supervised self-host check after the current batch:

```bash
librarian --home "$HOME/Librarian" smoke self-host --project-path "$PWD" --run-agent --review
```

Full supervised self-host check with a test-backed review packet:

```bash
librarian --home "$HOME/Librarian" smoke self-host --project-path "$PWD" --run-agent --review --run-tests
```

Review a job's repository state before continuing:

```bash
librarian --home "$HOME/Librarian" jobs review <job-id> --run-tests
librarian --home "$HOME/Librarian" jobs review-packet <job-id> --run-tests --revert-commit <sha>
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
- `jobs review-packet` records one combined review artifact with review output,
  commit gate, revert plan, push plan, and a compact next-step summary for the
  chat/UI approval card.
- Worker completion now automatically records a post-run review packet for Git
  worktrees, or a structured skip diagnostic for non-Git projects/review
  failures.
- `jobs gate` records whether commit, push, or revert is allowed by project policy,
  branch protection, branch pattern, dirty state, and upstream state.
- `jobs push-plan` records the upstream branch, outgoing commit list, ahead
  count, remote list, and outgoing diff stat before a manual push.
- `jobs propose-git` creates a git approval only after the gate passes. Commit
  and revert execution recheck policy before mutating the repository.
- Chat review cards can create commit/revert approval proposals without copying
  approval ids; approve/reject remains explicit.
- Context retrieval supports project-scoped memory and tree-aware context
  primitives.
- Tool permissions and approvals exist for file/project/memory/settings
  operations.
- Prompt builder blocks can generate provider instruction files such as
  `CLAUDE.md` and generic agent guidance.
- Prompt Builder UI can preview profile output, import/export portable preset
  JSON, and create approval-gated Markdown exports into the Library.
- Provider diagnostics are shared by doctor, admin API, and provider smoke.
  Current parsing covers common Codex, Claude Code, and OpenRouter auth, quota,
  model, timeout, and network failures.
- `smoke self-host` now checks that the Librarian repository can be registered
  and prepared for a supervised read-only agent job, and that `/agent
  review-packet` returns chat UI metadata for the same job. `--review` verifies
  the worker-created post-run review packet after a real agent run, and
  `--run-tests` makes the review packet execute and require `cargo test --quiet`
  success.
- Worker preflight now reports a budget reservation estimate. Known-price
  estimates create active SQLite budget reservations before dispatch; those
  reservations are released on job finish and counted as pending spend in budget
  guardrails.
- Provider smoke now includes a Claude Code launch-contract check: the generated
  container command mounts the configured instruction file into the project root
  and uses `claude -p` from the project directory.
- Prompt profile targets are centralized in code, so chat, generic agent, and
  provider instruction-file targets can grow without string literal drift.
- Provider settings show grouped provider status and can run local MVP smoke
  preflight from the UI.
- Admin auth blocks externally reachable binds unless an admin token is
  configured.

## Main Gaps Before Continuous Self-Development

- A real containerized self-host Codex task still needs repeated validation on
  the user's target Ubuntu host.
- Test gates before approval should become a routine workflow: review packet,
  `cargo fmt`, `cargo test`, focused smoke where appropriate, then commit
  proposal. Push remains manual after policy review. `smoke self-host
  --review --run-tests` covers the Cargo-test part; formatting/focused smoke
  still need an explicit gate story.
- Automatic write tasks now pass through a first project policy gate; richer UI
  policy editing and audit explanations are still needed.
- Budget/cost control has pending reservation accounting when model pricing is
  known. The remaining gap is provider/model-specific price reconciliation with
  observed provider usage.
- Browser login/session UX and CSRF are still polish before broad internet
  exposure, even though token auth now protects external binds.
- `src/admin.rs` is smaller and has explicit child modules, but future
  extraction should happen only at concrete feature boundaries.
- OpenRouter and Claude Code paths exist but are not yet proven as production
  self-hosting providers.
- Prompt/profile variants are usable for MVP; host, channel, and provider
  overrides need cleaner separation before many channels/providers are active.

## Practical Operating Mode Now

Use Librarian for supervised self-development in short loops:

1. Discuss the task in chat and select the Librarian project context.
2. Launch a read-only agent inspection or preflight first.
3. Review job events, generated run summary, and the review card.
4. Run or inspect `jobs review-packet <job-id> --run-tests`.
5. Run or inspect `jobs gate <job-id> --action commit` before any commit if the packet
   shows worktree changes.
6. Create a commit proposal from the review card or with
   `jobs propose-git <job-id> --action commit --message ...` when the review and
   gate are acceptable.
7. If that commit is wrong, run `jobs revert-plan <job-id> --commit <sha>` and
   approve an explicit revert proposal.
8. Only then launch a narrow write task or approve follow-up work.
9. Run `cargo test` or `doctor --smoke`.
10. Push manually only after a separate push gate and `jobs push-plan` review.

Avoid unattended multi-agent write loops until repeated real Codex self-host
runs are boringly reliable and the full review/test/format gate is automatic.
