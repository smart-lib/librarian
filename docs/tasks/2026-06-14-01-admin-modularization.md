# Task 01: Admin Module Boundaries

## Goal

Turn `src/admin.rs` from a catch-all file into a thin admin server entry point plus
cohesive child modules. This task is done only when complete subsystems are moved
out, compiled, tested, and the roadmap no longer describes this exact work as
"continue splitting".

## Scope

- Extract admin tests into `src/admin/tests.rs`.
- Extract admin smoke helpers into `src/admin/smoke.rs`.
- Extract approval slash/API execution helpers into `src/admin/approvals.rs`.
- Extract prompt builder slash/preset helpers into `src/admin/prompt_commands.rs`.
- Keep `src/admin.rs` responsible for router wiring, API handlers, and shared
  orchestration that has not yet earned a standalone module.
- Preserve public call sites from `main.rs` and existing admin endpoints.

## Definition Of Done

- `cargo test --quiet` passes.
- Admin smoke entry points still compile and are reachable from `main.rs`.
- No extracted module leaves duplicate or dead copies in `src/admin.rs`.
- Roadmap tech-debt wording reflects the completed extraction instead of adding
  another vague "continue splitting" item.

## Progress

- [x] Scope captured.
- [x] Modules extracted.
- [x] Tests run: `cargo test --quiet`.
- [x] Roadmap updated.
- [x] Committed separately: `e43060f Split admin subsystems into modules`.

## Result

`src/admin.rs` now keeps router/API/chat orchestration in the root module while
completed subsystems live in child modules:

- `src/admin/tests.rs`
- `src/admin/smoke.rs`
- `src/admin/approvals.rs`
- `src/admin/prompt_commands.rs`

The root file was reduced from 8377 lines to 4888 lines. The remaining large
areas are not recorded as another vague split task; future extraction should be
tied to feature boundaries such as admin auth, provider settings, or prompt UI.
