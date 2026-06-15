# Job Diagnostics and Approval State

Date: 2026-06-15
Status: Done in code; needs user WSL validation after upgrade.

## Problem

Real background jobs can fail before the agent reaches the requested work. The
observed `nomorecare.gg` clone job failed because the worker process could not
connect to `/var/run/docker.sock`, but the chat only had generic `exit code 1`
memory and tried to launch another agent to inspect Librarian-owned logs.

Approval cards also restored stale `Pending` snapshots from chat history after a
page reload, so already executed approvals could show buttons again.

## Target Behavior

- Librarian chat answers job-status and failure questions from SQLite job state
  and recent job events.
- Chat prompts include compact terminal job event summaries for the current
  context, including stderr and structured failure categories.
- Worker failure categorization distinguishes Docker socket permission failures
  from generic non-zero agent exits.
- Approval cards fetch the current approval record on transcript restore.
- Executed/rejected approval cards collapse into compact terminal summaries.

## Implementation Notes

- `chat.rs` now adds a `Recent Agent Job Events` prompt section and explicitly
  instructs the model not to launch a new agent merely to inspect Librarian's own
  job state.
- `admin.rs` exposes `GET /api/approvals/:id` for state refresh.
- `admin_ui.rs` refreshes approval status while restoring chat history and uses a
  compact terminal approval-card layout.
- `worker.rs` emits `runtime_permission_denied` for Docker socket permission
  errors, and generic final exit categories no longer overwrite a more specific
  category already observed from stdout/stderr.

## Validation

- Unit tests cover prompt inclusion of recent failed job stderr and the new
  Docker socket failure category.
- Full local test run should pass before pushing.
