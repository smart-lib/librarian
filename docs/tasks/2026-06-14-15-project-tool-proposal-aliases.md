# Task 15: Project Tool Proposal Aliases

## Goal

Librarian chat should accept common project-creation proposal names from the
LLM instead of failing with `Unsupported tool proposal`.

## Scope

- Keep `project.create_starting_docs_and_project_folder` as the canonical
  executable action.
- Accept site/project-specific aliases such as
  `create_site_library_and_project_folder`.
- Ensure approval execution supports the same aliases as proposal validation.
- Nudge the chat prompt toward the canonical action name.
- Add tests for proposal validation and execution through an alias.

## Definition Of Done

- Asking chat to create `/sites/nomorecare.gg/` can produce an approval card
  instead of an unsupported proposal error.
- Approved alias proposals create Library docs, workspace folder, and project
  registry entry.
- `cargo test --quiet` passes.
- The task is committed separately.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run.
- [x] Committed separately.

## Result

- Chat proposal validation accepts project creation aliases:
  `create_library_and_project_folder`,
  `create_site_library_and_project_folder`,
  `create_project_library_and_workspace`, and
  `create_library_and_workspace`.
- Approval execution supports the same aliases and reuses the existing
  project starter docs/workspace/registry implementation.
- Chat prompt now tells the LLM to prefer the canonical
  `create_starting_docs_and_project_folder` action.
