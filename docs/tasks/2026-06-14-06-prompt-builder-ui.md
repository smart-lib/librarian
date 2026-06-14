# Task 06: Prompt Builder UI Completion

## Goal

The prompt builder must be a usable editor for chat, agent, and provider prompt
profiles, not a partial list of blocks.

## Scope

- Separate profiles/targets for Librarian chat, agents, and provider-specific
  instruction files.
- Support reorder, enable/disable, markdown toggles, preview, diff, import, and
  export from the UI.
- Preserve slash-command and API flows for automation.
- Add smoke/test coverage for profile editing and rendered output.

## Definition Of Done

- A user can assemble and preview the generated prompt/instruction file from
  blocks without editing raw database rows.
- Import/export proposals remain approval-gated where they touch files.
- Rendered output for chat/agent/provider profiles is covered by tests.

## Progress

- [ ] Scope captured.
- [ ] Implementation completed.
- [ ] Tests/smokes run.
- [ ] Roadmap updated.
- [ ] Committed separately.
