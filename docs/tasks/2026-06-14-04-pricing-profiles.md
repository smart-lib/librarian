# Task 04: Provider Pricing Profiles

## Goal

Budget reservations and usage displays must use real provider pricing profiles
instead of vague defaults.

## Scope

- Add pricing profile definitions for Codex, OpenRouter, and Claude Code where
  reliable model-level values are available.
- Mark unknown or session-derived costs explicitly instead of pretending they are
  exact.
- Feed pricing metadata into budget preflight/reservation summaries.
- Add tests that prove pending reservations and observed usage are combined.

## Definition Of Done

- Budget checks display observed, pending, and projected amounts with provider
  labels.
- Unknown pricing is explicit and does not silently produce false precision.
- Existing budget tests pass and new pricing coverage is included.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run: `cargo test --quiet`.
- [x] Roadmap updated.
- [x] Committed separately: `67d5f9f Expose provider pricing profiles`.

## Result

Model metadata now exposes explicit pricing profiles with `pricing_kind`,
`pricing_source`, and `pricing_note`. CLI-backed defaults for Codex and Claude
Code are marked `observed_only`; OpenRouter default is marked `model_required`
until a concrete OpenRouter model is configured. Budget reservation estimates no
longer report a generic unknown reason when the real state is observed-only or
model-specific.
