# Task 02: Review And Approval UX

## Goal

Make review/approval actions usable from the chat UI without exposing raw CLI
approval ids as the primary user workflow.

## Scope

- Add chat review packet buttons for propose commit, propose revert, and refresh.
- Display approval proposals as chat-native cards with approve/reject actions.
- Keep slash commands as a power-user fallback, not the primary UX.
- Add smoke coverage for review-card actions and approval-card rendering.

## Definition Of Done

- User can move from job review to proposal to approval/rejection inside the UI.
- The UI does not require copying approval ids for the normal path.
- Existing `/approval` commands continue to work.
- Automated tests or smokes cover the UI payload contract.

## Progress

- [ ] Scope captured.
- [ ] Implementation completed.
- [ ] Tests/smokes run.
- [ ] Roadmap updated.
- [ ] Committed separately.
