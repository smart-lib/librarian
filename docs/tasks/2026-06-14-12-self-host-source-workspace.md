# Task 12: Self-Host Source Workspace

## Goal

`smoke self-host` should work from an installed Librarian root where the
installer has removed `.app/source`.

## Scope

- Treat `~/Librarian` as the installed state root, not as the source repository.
- If the requested path is not a Librarian checkout, prepare or reuse
  `~/Librarian/Projects/Librarian` as the self-host source workspace.
- Clone the public repository into that workspace when it is missing.
- Keep existing explicit checkout paths working unchanged.
- Improve docs/readiness guidance so users do not point self-host smoke at
  `.app/source`.

## Definition Of Done

- Installed-root self-host smoke resolves to a real source checkout.
- Missing checkout errors are actionable and no longer mention only
  `.app/source`.
- `cargo test --quiet` passes.
- The task is committed separately.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run.
- [x] Committed separately.

## Result

- `smoke self-host` now accepts the installed Librarian root as input and
  resolves it to `Projects/Librarian`.
- If that workspace is missing, the command clones the public Librarian
  repository there before continuing.
- Explicit source checkout paths still work unchanged.
- README, roadmap, and self-host readiness docs now describe
  `Projects/Librarian` as the self-development workspace instead of relying on
  the temporary `.app/source` installer checkout.
