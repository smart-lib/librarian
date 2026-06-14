# Task 13: Self-Host Workspace Collision

## Goal

Self-host smoke must not fail when `Projects/Librarian` already exists as a
normal non-Git user workspace.

## Scope

- Keep `Projects/Librarian` as the preferred self-host source workspace.
- If the preferred path is occupied by a non-source folder, use a safe fallback
  source workspace instead of asking the user to move files.
- Never delete or overwrite an existing user folder.
- Update docs to describe the fallback path behavior.

## Definition Of Done

- Installed-root self-host smoke can proceed when `Projects/Librarian` exists
  but is not a Git checkout.
- Tests cover fallback candidate selection.
- `cargo test --quiet` passes.
- The task is committed separately.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run.
- [x] Committed separately.

## Result

- Self-host smoke now tries ordered source workspace candidates:
  `Projects/Librarian`, `Projects/LibrarianSource`, then
  `Projects/_self-host/Librarian`.
- Existing non-source folders are skipped and never overwritten.
- Docs now describe fallback behavior instead of promising a single fixed
  workspace path.
