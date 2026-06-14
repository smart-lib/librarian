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

- [ ] Scope captured.
- [ ] Implementation completed.
- [ ] Tests run.
- [ ] Roadmap updated.
- [ ] Committed separately.
