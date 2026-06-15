# Task 16: Canonical Tool Registry And Agent Launch Proposals

## Goal

Chat should turn user requests for background project work into a generic
approval-backed agent launch, not into one-off special tools or action names
invented from user text.

## Scope

- Render a canonical tool/action manifest into the Librarian chat prompt.
- Make the prompt explicit that `tool_action` must be one exact manifest action.
- Add canonical `agent.launch` proposal validation without clone-specific aliases.
- Use the current chat project context as the default launch project.
- Approval execution should queue a normal background agent job with explicit
  mount/network/provider settings.
- Keep actual work inside the agent job prompt; do not add clone-specific host
  filesystem tooling.

## Definition Of Done

- A chat request to clone a repo into the current project can produce an
  approval card for `agent.launch`.
- Approving the card queues a read-write agent job for the selected project.
- No special-purpose clone executor is added.
- Unsupported invented agent actions fail with guidance to use `agent.launch`.
- `cargo test --quiet` passes.
- The task is committed separately.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run: `cargo test --quiet` passed with 110 tests.
- [x] Committed separately.
