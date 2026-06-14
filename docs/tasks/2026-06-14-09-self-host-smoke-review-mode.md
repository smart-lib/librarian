# Task 09: Self-Host Smoke Review Mode

## Goal

Extend `smoke self-host` so one command can validate not only preflight/run, but
also the review packet/gate contract after a real Codex run.

## Scope

- Add `--review` to `librarian smoke self-host`.
- In preflight-only mode, verify existing review/gate contracts as before.
- In `--run-agent --review` mode, run the provider job and then verify the
  post-run review packet event exists and contains a next step.
- Keep the actual provider call optional because manual auth/runtime validation
  belongs to the user environment.

## Definition Of Done

- `smoke self-host --review` works without provider execution.
- `smoke self-host --run-agent --review` validates the post-run review contract.
- `cargo test --quiet` passes.
- The task is committed separately.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run.
- [x] Committed separately.

## Result

- Added `--review` to `librarian smoke self-host`.
- Preflight-only mode still validates the review/gate/UI review-card contracts
  without making a provider call.
- `--run-agent --review` now asserts that the worker emitted an automatic
  `post_run_review_packet` event and that it contains `summary.next_step`.
- Added unit coverage for post-run packet detection and skipped review
  diagnostics.
