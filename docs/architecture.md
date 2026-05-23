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

Project memory is modeled as a real library hierarchy. A "project" can be any
folder or Markdown note inside `Library`: a book, shelf, rack, or row of racks in
the future visual metaphor. This documentation/product knowledge is separate
from the implementation working directory. When a project needs executable or
editable product files, Librarian attaches it to a working directory: by default
`Projects/{ProjectName}` under the Librarian root, or an existing user-selected
external directory. Librarian must not create arbitrary external directories.

The first library/workspace tool layer exposes narrow operations instead of
general filesystem access:

- list the `Library` tree through library tools and the `Projects` tree through
  workspace tools;
- create folders and empty files inside the selected tool root;
- rename or move paths inside the same approved root;
- delete paths inside the same approved root, with explicit recursive delete for
  folders;
- read and write Markdown content only under `Library`.

All tool paths are relative to the selected root, reject absolute paths and
parent traversal, and are checked against the canonical root before use. Obsidian
integration is currently compatibility-level only: Markdown files, folders,
frontmatter, and wikilinks. No Obsidian CLI or plugin API is required or invoked
yet.

Direct slash commands are dispatched before the LLM chat provider. Library
commands live under the `/lib` namespace so future tool groups can have their
own namespaces. The first library command set covers tree/folder/file
operations, whole-note overwrite/append, range reads, line-range cut/replace,
and search-based cut/replace. Whole-note overwrite is intentionally explicit as
`/lib write-overwrite`; routine edits should use line or search operations so a
large note does not need to be loaded and rewritten by the caller. Deterministic
slash commands are logged into chat memory, and mutating library commands also
write `library_tool` system events. Destructive delete requires an explicit
`--yes` flag even though it is already inside the sandbox.

Default working directories under `Projects` are handled by a separate
workspace tool namespace, `/work`. This keeps knowledge-library operations
separate from implementation/product folder operations even when the underlying
sandboxing mechanics are similar. External attached implementation directories
are still project-record behavior, not `/work` folder creation.

Tool execution is controlled by persisted `[tool_permissions]` config. The first
policy gate supports `auto`, `ask`, and `deny`; slash commands are explicit user
actions, so `ask` is allowed and audited as `allowed_user_slash`. `deny` blocks
the operation and records a `tool_permission` system event. Future
assistant-initiated tool calls should use the same policy gate but surface an
interactive approval request for `ask`.

Memory tools use a separate `/mem` namespace. `/mem remember <kind> <content>`
stores durable memory in the current chat scope, meaning the selected project
when one is active or global memory otherwise. `/remember <content>` is a
shortcut for a global/project-scoped fact. Memory writes pass through
`tool_permissions.memory_write` and produce `memory_tool` system events.

Settings inspection and guarded tool-permission updates use the `/settings`
namespace. `/settings tool-permissions` reports the current policy matrix, while
`/settings set-tool-permission <key> <auto|ask|deny> --yes` passes through
`tool_permissions.settings_change`, persists `.cfg/config.toml`, and records a
`settings_tool` event.

Projects now carry two separate path concepts. `Project.path` remains the
implementation/workspace path mounted for worker jobs. `Project.library_path`
is optional and points to the documentation/memory project inside `Library`,
stored as a sandbox-relative path. Existing project records continue to work
with no library attachment until the user links one. Chat exposes this through
the `/project` namespace: create a default Library/Projects pair, attach or
detach a library path, attach an existing external workspace, list projects, and
inspect one project.

The project map API (`/api/project-map`) returns the `Library` tree annotated
with linked project records and visual kinds for the library metaphor: Markdown
notes are books, folders with files are shelves, folders with nested folders are
racks/rows, and other files are artifacts. `/project map` exposes the same data
to chat without spending provider tokens.

Background agent jobs use an explicit `/agent` namespace in chat. Listing,
status, event history, and preflight are read/diagnostic commands. Launch,
cancel, and retry mutate job state, pass through `tool_permissions.agent_launch`,
and require an explicit confirmation flag. Plain chat requests remain
conversation-only and do not create jobs.

Assistant-initiated tool calls should not execute directly when a policy asks
for confirmation. The first approval-queue scaffold stores proposed tool calls
in `tool_approvals` with `pending`, `approved`, `rejected`, and `executed`
states, exposed through `/approval list`, `/approval propose`, `/approval
approve`, and `/approval reject`. Approval records user intent; a later executor
layer must still run approved actions through the normal tool boundary.

By default, the Librarian root is a single stable per-user application
directory: `%APPDATA%\Librarian` on Windows, `~/Librarian` on Linux, and
`~/Library/Application Support/Librarian` on macOS. `setup` asks for the desired
root on first interactive setup; silent setup accepts the platform default.
`--home` and `LIBRARIAN_HOME` can point it elsewhere for portable installs,
test roots, and self-contained release folders. Paths inside `config.toml` are
stored relative to the Librarian root when possible so the folder can be moved
between systems.

The default root layout is:

- `.app/` for the installed binary, temporary source checkout, and run
  artifacts;
- `.cfg/` for `config.toml`, Codex profile data, and portable settings;
- `.mdb/` for SQLite and other machine-readable databases/exports;
- `Library/` for Markdown memory, decisions, project notes, and run summaries;
- `Projects/` for default user working directories.

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
