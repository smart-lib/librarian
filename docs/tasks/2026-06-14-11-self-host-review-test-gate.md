# Task 11: Self-Host Review Test Gate

## Goal

Make `smoke self-host` able to validate the review packet with explicit test
execution when the user wants a stronger supervised self-development check.

## Scope

- Add an opt-in test gate flag to `librarian smoke self-host`.
- Keep default self-host smoke fast and no-test.
- In preflight-only mode, include tests in the manual review snapshot/packet
  only when requested.
- In `--run-agent --review` mode, verify the worker-created post-run packet and
  optionally build a fresh test-backed packet after the real run.
- Update self-host readiness docs so automatic post-run review is no longer
  listed as a gap.

## Definition Of Done

- `smoke self-host --review --run-tests` performs a test-backed review packet
  without a provider call.
- `smoke self-host --run-agent --review --run-tests` performs the same stronger
  check after the real provider call.
- `cargo test --quiet` passes.
- The task is committed separately.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run.
- [x] Committed separately.

## Result

- Added `--run-tests` to `librarian smoke self-host`.
- Preflight review and review-packet creation now pass the flag through to the
  existing Cargo test gate.
- `--run-agent --review --run-tests` verifies the automatic post-run review
  packet, then builds a fresh test-backed packet and fails if tests are not
  successful.
- Self-host readiness docs no longer list automatic post-run review as a gap.
