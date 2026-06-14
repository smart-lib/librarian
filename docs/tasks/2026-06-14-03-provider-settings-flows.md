# Task 03: Provider Settings Flows

## Goal

Provider settings should show real status and expose the actual setup/smoke
actions a user needs, grouped by provider.

## Scope

- Make provider status cards readable and grouped by Codex, Claude, and
  OpenRouter.
- Add UI-triggered setup command helpers where auth must still happen in shell.
- Add UI-triggered smoke/preflight calls with clear result cards.
- Ensure Claude's provider-specific launch contract is visible and configurable.

## Definition Of Done

- No provider card claims readiness from placeholder data.
- The user can see what is configured, what is missing, and which action to run.
- One-click smoke/preflight actions return readable success/failure details.
- Existing provider smokes pass.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run: `cargo test --quiet`.
- [x] Roadmap updated.
- [x] Committed separately: `60b00be Add provider smoke actions to admin UI`.

## Result

Provider settings now keep auth/build as explicit shell commands while provider
Smoke buttons call a real admin endpoint. `POST /api/providers/{provider}/smoke`
runs the installed Librarian binary in MVP smoke preflight mode for Codex,
Claude Code, or OpenRouter, and returns command/stdout/stderr/status for the UI.
The endpoint also supports dry-run command inspection for automated coverage.
