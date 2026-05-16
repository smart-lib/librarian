# Librarian

Librarian is a local-first automation harness for ideas, projects, and coding agents.
It runs a small root orchestrator on the host, keeps durable memory in SQLite and
an Obsidian-compatible Markdown vault, and launches worker agents inside Docker
containers with explicit filesystem and network policies.

The first provider target is OpenAI Codex CLI. Additional providers such as
OpenRouter, Claude Code, and local models are planned behind the same provider
adapter interface.

## Goals

- Cross-platform host process: Windows, Linux, and macOS.
- Containerized agent execution with Docker.
- No network access for worker containers by default.
- Configurable project mounts, defaulting to read-write for MVP ergonomics.
- Localhost admin UI as the default interaction path.
- SQLite for internal state, jobs, schedules, and memory.
- Vector-ready SQLite memory with recency-aware retrieval and project/activity
  filters.
- One global Obsidian-compatible Markdown vault for chats, project notes,
  decisions, and background run summaries.
- Local, open-source secret handling with a future broker mode where agents can
  use capabilities without reading raw credentials.
- MIT licensed, documented, and configurable from the beginning.

## Current Status

This repository contains the initial scaffold:

- CLI commands for initialization, project registration, job creation, and admin UI.
- SQLite schema bootstrap.
- Docker runner abstraction with network disabled by default.
- Codex provider adapter shell.
- A worker loop that executes queued jobs, records stdout/stderr events, and
  supports cancellation, retry, heartbeats, and configurable concurrency.
- A scheduler for lifecycle tasks, reminders, and scheduled agent jobs.
- Vector-backed, recency-aware memory retrieval with project-scoped context
  enrichment and a local deterministic embedding backend.
- Admin UI panels for worker capacity, projects, jobs, schedules, and recent
  Librarian actions.
- Admin UI controls for creating schedules, manually running schedules, and
  enabling/disabling them.
- Alpine-based agent image Dockerfile.
- Architecture and security notes.

## Quick Start

Prerequisites:

- Rust stable toolchain. On Windows, the bootstrap script uses the GNU target
  with MSYS2/UCRT GCC.
- Podman on Windows, or Docker/Podman on Linux and macOS.
- Codex CLI installed on the host for authentication bootstrap.

```powershell
.\scripts\bootstrap-windows.ps1
cargo +stable-x86_64-pc-windows-gnu run -- auth codex
cargo +stable-x86_64-pc-windows-gnu run -- project add c:\path\to\project
cargo +stable-x86_64-pc-windows-gnu run -- admin
```

Then open `http://127.0.0.1:17377`.

Queue a job from the UI or CLI:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- run --project my-project --goal "Inspect the repo and suggest the next implementation step"
```

Process queued jobs:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- worker --once
```

Without `--once`, the worker loops and polls for queued jobs.
Worker parallelism defaults to `1`. Override it for a stronger host:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- worker --concurrency 2
$env:LIBRARIAN_WORKER_CONCURRENCY="2"
cargo +stable-x86_64-pc-windows-gnu run -- config set-concurrency 2
```

Manage jobs:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- jobs list
cargo +stable-x86_64-pc-windows-gnu run -- jobs events <job-id>
cargo +stable-x86_64-pc-windows-gnu run -- jobs cancel <job-id>
cargo +stable-x86_64-pc-windows-gnu run -- jobs retry <job-id>
```

Manage secrets:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- secrets status
cargo +stable-x86_64-pc-windows-gnu run -- secrets set openai.default --provider openai --kind api-key --value "sk-..."
cargo +stable-x86_64-pc-windows-gnu run -- secrets grant openai.default --provider openai --capability provider-proxy --ttl-seconds 900 --max-uses 5
cargo +stable-x86_64-pc-windows-gnu run -- broker
```

Agents should prefer the broker provider proxy over raw secret resolution. The
proxy accepts a grant token in `x-librarian-grant-token` and keeps the raw key on
the host:

```text
POST http://host.containers.internal:17379/v1/proxy/openai/v1/chat/completions
```

Inspect provider routing and limits:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- providers catalog
cargo +stable-x86_64-pc-windows-gnu run -- providers status
cargo +stable-x86_64-pc-windows-gnu run -- providers pause codex --model codex-cli-default --seconds 1800 --reason "rate limit"
cargo +stable-x86_64-pc-windows-gnu run -- providers resume codex --model codex-cli-default
cargo +stable-x86_64-pc-windows-gnu run -- usage list --limit 20
```

Probe an optional Third Eye instance for external Codex/Claude cost telemetry:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- third-eye configure --enabled true --base-url http://127.0.0.1:4317
cargo +stable-x86_64-pc-windows-gnu run -- third-eye status
cargo +stable-x86_64-pc-windows-gnu run -- third-eye providers
cargo +stable-x86_64-pc-windows-gnu run -- third-eye refresh --since 1h
cargo +stable-x86_64-pc-windows-gnu run -- third-eye db-summary
```

See `docs/third-eye.md` for the current integration notes and container log
export caveats.

Prompt and output gates run automatically. Secret-shaped tokens in prompts are
captured into the vault and replaced with `secret://...` references; known
secrets are redacted from worker output before event storage.

Run scheduler maintenance:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- scheduler --once
cargo +stable-x86_64-pc-windows-gnu run -- schedule list
cargo +stable-x86_64-pc-windows-gnu run -- events --limit 20
```

Create a reminder or scheduled Codex task:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- schedule add daily.check --every-seconds 86400 --kind reminder
cargo +stable-x86_64-pc-windows-gnu run -- schedule add weekly.agent --every-seconds 604800 --kind agent-task --payload '{\"project\":\"my-project\",\"goal\":\"Review status and update the roadmap\",\"allow_network\":false}'
cargo +stable-x86_64-pc-windows-gnu run -- schedule run <schedule-id>
cargo +stable-x86_64-pc-windows-gnu run -- schedule disable <schedule-id>
cargo +stable-x86_64-pc-windows-gnu run -- schedule enable <schedule-id>
cargo +stable-x86_64-pc-windows-gnu run -- schedule update <schedule-id> --name daily.check --every-seconds 43200 --kind reminder --payload '{\"message\":\"check status\"}'
cargo +stable-x86_64-pc-windows-gnu run -- schedule delete <schedule-id>
```

Inspect memory enrichment:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- context "what is the studio site roadmap status?" --project studio-site
cargo +stable-x86_64-pc-windows-gnu run -- memory status
cargo +stable-x86_64-pc-windows-gnu run -- memory embed --limit 1000
```

Preview the enriched prompt for a project-scoped agent:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- context "what should happen next?" --project studio-site --prompt
```

On Windows, the default container runtime is Podman. On Linux and macOS, the
default is Docker. The runtime command is configurable.

## Codex Authentication

Librarian does not store Codex credentials in project files. The MVP expects you
to authenticate Codex on the host first, then later uses a scoped runtime profile
for containers.

```powershell
codex
```

Future work will add a secret broker so containers can request provider actions
without receiving plaintext credentials.

## Default Safety Model

Worker containers start with:

- no published ports;
- network disabled unless a session explicitly enables it;
- only the selected project mounted;
- configurable read-write or read-only project mount;
- full project autonomy inside a read-write mounted project, with git behavior
  controlled by project policy;
- separate ephemeral work directory for run artifacts.

This is a defense-in-depth boundary, not a perfect sandbox. Docker bind mounts
and Docker daemon access must still be treated as privileged local capabilities.

## Documentation

- [Architecture](docs/architecture.md)
- [Security model](docs/security.md)
- [Research notes](docs/research-notes.md)
- [Memory](docs/memory.md)
- [Todo](docs/todo.md)
- [Roadmap](docs/roadmap.md)
- [Open questions](docs/open-questions.md)

## Development Notes

The project currently checks with Rust stable GNU on Windows. The Windows
bootstrap path uses Podman as the Docker-compatible runtime.
