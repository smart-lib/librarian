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
- [ ] Implementation completed.
- [ ] Tests run.
- [ ] Committed separately.
