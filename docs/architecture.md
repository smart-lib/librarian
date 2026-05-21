# Architecture

Librarian has three layers:

1. Host orchestrator.
2. Containerized agent runtime.
3. Local memory and knowledge stores.

## Host Orchestrator

The root process is a Rust application. It owns:

- CLI commands;
- localhost admin API/UI;
- SQLite state;
- project registry;
- Docker session creation;
- provider routing;
- scheduling and lifecycle control;
- secret broker process in later milestones.

The root process should avoid becoming a general-purpose filesystem agent. It
reads project metadata only when building context and delegates actual work to
containerized agents.

## Agent Runtime

Each agent run is represented by a job and a Docker container. The runner builds
a prompt bundle, mounts the target project, and executes the selected provider
adapter command.

Default container policy:

- network: disabled;
- exposed ports: none;
- project mount: read-write unless overridden;
- workdir: `/workspace/project`;
- run artifacts: `/workspace/run`;
- image: `librarian-agent:latest`.

Network access is a session-level capability. A Codex session that calls OpenAI
directly needs network enabled, or a future host-side provider proxy.

## Job Lifecycle

The current worker flow is:

1. Pick the oldest `Queued` job.
2. Mark it `Preparing`.
3. Resolve the project and provider command.
4. Create or reuse the project note in the global vault.
5. Mark it `Running`.
6. Execute the Docker command.
7. Store stdout and stderr as job events.
8. Mark it `Completed` or `Failed`.
9. Write a run summary note to the global vault.

The next lifecycle layer should add heartbeats, cancellation, retry policy,
container cleanup, and process supervision.

Before execution, the worker rebuilds memory context and turns the job goal into
an enriched provider prompt. This means a queued job can still benefit from
memory created after the job was queued but before it starts.

The lifecycle layer now records `started_at`, `finished_at`, heartbeat time, and
cancel requests. Queued jobs can be cancelled before execution. Running jobs are
polled by the worker, heartbeat is updated, and cancellation kills the child
process before marking the job `Cancelled`.

Workers claim queued jobs atomically before execution. Default parallelism is
`1` to avoid surprise memory and provider cost growth. It can be overridden with
`--concurrency` or `LIBRARIAN_WORKER_CONCURRENCY`.

## Scheduler

The scheduler stores recurring tasks in SQLite. The first default system
schedule is `system.heartbeat-recovery`, which periodically checks running or
preparing jobs whose heartbeat is too old and marks them `HeartbeatMissed`.

The scheduler can run once for maintenance or loop as a long-running process:

- `librarian scheduler --once`
- `librarian scheduler`

Future schedules will cover reminders, memory compaction, vault sync, provider
limit refresh, and queued agent tasks.

## Memory

SQLite stores operational memory:

- jobs and lifecycle events;
- project registry;
- schedules;
- provider runs;
- raw memories, compacted facts, summaries, and embeddings;
- audit records.

Every user request is enriched with memory before the overseer answers or starts
work. Retrieval combines vector similarity, lexical fallback, project/activity
filters, recency weighting, and contradiction handling.

The first implemented context pack uses SQLite candidates and lexical scoring as
the fallback backend. Its interface is intentionally shaped so `vec1` or
`sqlite-vec` can become the vector candidate source without changing job
creation, admin chat, or provider prompts.

The Markdown vault is global to the Librarian root and stores human-readable
knowledge across chats, projects, and background runs:

- project briefs;
- decisions;
- plans;
- run summaries;
- notes and links.

The vault is Obsidian-compatible: Markdown files, YAML frontmatter, and ordinary
folder structure under version control.

By default, the Librarian root is a stable per-user application directory:
`%APPDATA%\Librarian` on Windows, `~/Librarian` on Linux, and
`~/Library/Application Support/Librarian` on macOS. `setup` asks for the desired
root on first interactive setup; silent setup accepts the platform default.
`--home` and `LIBRARIAN_HOME` can point it elsewhere for portable installs,
test roots, and self-contained release folders. Paths inside `config.toml` are
stored relative to the Librarian root when possible so the folder can be moved
between systems.

The process current directory is treated as launch context rather than storage
location. Future project auto-detection can use that context to suggest the
current directory as an active project without changing where Librarian keeps
its durable state.

## Provider Adapters

Providers are command or API adapters behind one trait:

- prepare context;
- produce command/container spec;
- parse output/events;
- report usage where available.

The first provider is Codex CLI, with OpenRouter and Claude Code adapters behind
the same boundary. Provider routing records observed usage, limit hits, and
pause windows in SQLite. Before a worker dispatches a job, routing also checks
configured daily budget guardrails against observed `cost_usd` telemetry for the
current UTC day. These checks cover global, provider, and project scopes; future
provider adapters can add estimated-cost reservation before execution.

Third Eye can be attached as an optional external usage observer. Librarian
talks to its localhost API for health, refresh, and provider summaries, and can
read its SQLite database in read-only mode when configured. Because Third Eye
discovers Codex and Claude sessions from host log directories, containerized
agents need an explicit log strategy: mount `CODEX_HOME`/Claude session roots to
host-visible paths, or export Librarian usage observations into a compatible
project directory.
