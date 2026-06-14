# Task 10: Roadmap Self-Host Sync

## Goal

Fold the new self-host work back into the roadmap so the next chat can continue
from the real current state.

## Scope

- Update self-host related roadmap sections.
- Remove stale "future" wording for review card/prompt/auth items that are now
  implemented.
- Keep remaining tasks concrete and not duplicated under multiple headings.
- Bump version if the batch lands as a visible capability group.

## Definition Of Done

- Roadmap and task docs describe the same state.
- Version is updated if needed.
- `cargo test --quiet` passes at batch end.
- The task is committed separately.

## Progress

- [x] Scope captured.
- [x] Roadmap updated.
- [x] Tests run.
- [x] Committed separately.

## Result

- Roadmap now describes the current self-host smoke contract:
  `smoke self-host --review` checks review/gate/UI card behavior without a
  provider call, while `--run-agent --review` validates the worker-created
  post-run review packet after a real provider run.
- Provider/gates backlog now treats post-run review packets as implemented
  worker behavior for Git worktrees, with structured skip diagnostics for
  non-Git projects.
- Crate version bumped to `0.2.18` for the self-host review capability group.
