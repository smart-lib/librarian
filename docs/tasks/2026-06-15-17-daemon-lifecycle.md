# Task 17: Autonomous Daemon Lifecycle

## Goal

Librarian must keep working after a user approves background work. The web admin
UI is only an interface; queued agent jobs must be executed by a backend process
that can run without a browser and without the admin server being open.

## Current State

- `librarian worker --once` runs queued jobs manually.
- `librarian worker` loops forever and polls queued jobs.
- `librarian scheduler --once` runs schedule/heartbeat maintenance once.
- `librarian scheduler` loops forever and runs schedule/heartbeat maintenance.
- `librarian admin` serves the web UI/API but does not execute queued jobs.
- No installer currently creates a background service.

## MVP Daemon Design

- Add `librarian daemon`.
- The daemon does not start the web admin by default.
- The daemon loop runs:
  - scheduler tick every configured interval;
  - worker batch execution with configured concurrency;
  - short idle sleep when no jobs were run.
- The daemon can run in the foreground for visible logs.
- `librarian daemon --background` delegates to the service lifecycle on systems
  with user systemd support, installing the service if needed.
- Manual `worker` and `scheduler` commands remain for debugging and smoke tests.

## Service Lifecycle

First target: Ubuntu/Linux user systemd.

Commands:

- `librarian service install`
- `librarian service start`
- `librarian service stop`
- `librarian service restart`
- `librarian service status`
- `librarian service uninstall`

The installed user service:

- runs the installed binary with `--home <root> daemon`;
- does not bind the admin UI;
- restarts on failure;
- writes stdout/stderr to the user journal;
- can be enabled for user login startup.

## Upgrade Behavior

The Ubuntu bootstrap/upgrade path should:

- detect whether the user service exists and is active before replacing the
  binary;
- stop it before replacing the binary;
- restart it after install if it had been active.

If no service is installed/running, upgrade should leave it alone.

## Thin Client Direction

This pass does not rewrite the admin API into a separate daemon API. It changes
the process model so the daemon owns autonomous work, while the admin remains a
client/operator UI over the same state. The next architectural pass can move
shared admin/channel operations behind an internal daemon API without changing
the worker lifecycle contract.

## Definition Of Done

- Foreground daemon loop exists and runs scheduler + worker.
- Background service commands exist for Ubuntu/Linux user systemd.
- Doctor reports daemon/service status and warns when queued jobs cannot run
  because no daemon/worker is active.
- Ubuntu upgrade stops/restarts the daemon service when appropriate.
- Follow-up repair: service install/start/restart regenerates the user unit,
  includes `SupplementaryGroups=docker` when the current user is in the docker
  group, and doctor/status probe the configured runtime through `systemd-run
  --user` so Docker socket permission mismatches in the service context are
  visible instead of looking like agent failures.
- Tests cover unit rendering/status logic that does not require a real systemd
  session.
- Roadmap and version are updated.
- `cargo test --quiet` passes.

## Progress

- [x] Scope captured.
- [x] Implementation completed.
- [x] Tests run: `cargo test --quiet` passed.
- [x] Committed separately.
