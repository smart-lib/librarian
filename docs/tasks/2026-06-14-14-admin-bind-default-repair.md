# Task 14: Admin Bind Default Repair

## Goal

Installed Ubuntu/WSL Librarian should start the admin UI with the documented
`librarian admin` command after upgrade.

## Scope

- Change Ubuntu bootstrap's default admin bind back to localhost.
- Ensure upgrade/bootstrap writes the selected bind into config, repairing old
  installs that persisted `0.0.0.0:17377` without auth.
- Add a doctor check for externally reachable admin binds without auth so the
  next step is actionable.
- Keep external binds blocked unless auth is configured.

## Definition Of Done

- `librarian admin` works after default Ubuntu upgrade.
- Existing unsafe default bind is repaired by upgrade.
- Doctor reports unsafe admin bind as a warning with localhost/auth guidance.
- `cargo test --quiet` passes.
- The task is committed separately.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run.
- [x] Committed separately.

## Result

- Ubuntu bootstrap now defaults `LIBRARIAN_ADMIN_BIND` to
  `127.0.0.1:17377`.
- Bootstrap writes the selected bind into config on every install/upgrade, so
  old installs with `0.0.0.0:17377` are repaired by the next upgrade unless the
  user explicitly sets an external bind.
- Doctor now reports externally reachable admin binds without auth as a warning
  and suggests localhost or explicit auth configuration.
- External admin binds remain blocked by `admin::serve` unless auth is
  configured.
