# Task 08: Automatic Post-Run Review Packet

## Goal

After an agent job finishes, Librarian should automatically create a review
packet when the project is a Git worktree, so the next step is visible without a
manual `jobs review-packet` command.

## Scope

- On completed/failed/cancelled job finish, try to build a review packet.
- Skip cleanly for non-Git projects or review failures and record a diagnostic
  event instead of failing the worker.
- Keep expensive test execution opt-in; automatic packet should not run tests by
  default.
- Add tests for successful post-run packet and non-Git skip behavior where
  feasible.

## Definition Of Done

- Worker emits `post_run_review_packet` or `post_run_review_skipped`.
- A successful agent job no longer requires manual review-packet creation before
  UI can show the next step.
- `cargo test --quiet` passes.
- The task is committed separately.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run.
- [x] Committed separately.

## Result

- Worker now records a `post_run_review_packet` event after completed, failed,
  or cancelled agent execution when the linked project is a Git worktree.
- Non-Git projects and review-packet failures are non-fatal and are recorded as
  `post_run_review_skipped` diagnostics.
- Automatic packet creation keeps `run_tests=false`; expensive validation stays
  under explicit smoke/manual control.
- Added unit coverage for Git worktree packet creation and non-Git skip
  behavior.
