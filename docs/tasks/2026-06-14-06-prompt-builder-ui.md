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

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run: `cargo test --quiet`.
- [x] Roadmap updated.
- [x] Committed separately: `8966a78 Complete prompt builder profile UI`.

## Result

Prompt Builder settings now expose profile-level controls for chat, agent, and
provider instruction targets. The UI can preview compiled output, export
portable preset JSON, import preset JSON, and create approval-gated Markdown
exports into the Library. Backend API coverage verifies export/import upsert
behavior using the same `librarian.prompt-presets.v1` format as slash commands.
