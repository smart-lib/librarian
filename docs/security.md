# Security Model

Librarian is local-first and assumes the user controls the host machine. The
security goal is to reduce accidental damage and credential exposure when
running autonomous coding agents.

## Defaults

- Containers have no network access by default.
- Containers publish no ports.
- Only the selected project is mounted.
- Project mount mode is configurable.
- Within a mounted read-write project, the default autonomy model is full
  project control. The user grants that boundary by selecting the project mount.
- Jobs and events are audited in SQLite.
- Secrets are not written into images or project files.

## Secret Handling

The MVP uses local-only encrypted storage. No paid or proprietary external
service is required.

Implemented modes:

1. Local encrypted vault: stores encrypted provider metadata and tokens in
   SQLite.
2. Windows DPAPI encryption through the host PowerShell DPAPI cmdlets, avoiding
   direct Rust FFI code.
3. Linux/macOS encrypted fallback through AES-GCM with an explicit
   `LIBRARIAN_SECRET_KEY` local master secret.
4. Capability grants: short-lived, max-use tokens scoped to a secret, provider,
   capability, and optionally a job.
5. Host broker: agents can call the broker with a grant token.
6. Provider proxy: agents can ask the broker to call a provider API while the
   raw API key remains on the host side.

The broker mode is required for the stronger property "agent can use a key but
cannot read the key". The `/v1/proxy/<provider>/<path>` broker path is the
preferred mode for that property. The `/v1/secrets/resolve` endpoint exists for
local tooling and debugging; anything that receives the resolved value can read
it, so it should not be the default for untrusted agent work.

The default broker bind is `127.0.0.1:17379`. Containers use
`broker.container_url` from config, defaulting to
`http://host.containers.internal:17379`.

## Prompt And Response Gates

All user prompts, enriched prompts, tool outputs, provider responses, and final
display text should pass through a cheap gate pipeline. The initial gate set is
security-oriented:

- detect raw secret-shaped tokens in user input;
- store detected secrets in the vault;
- replace raw values with vault/grant references before prompt enrichment;
- redact known secret values if they appear in tool output, provider output, or
  UI responses;
- audit every automatic secret capture and redaction.

Provider-specific gates can also transform prompts before launch. For example,
some providers may require softer wording or suppressed mentions of competing
tools. These transforms should be explicit, configurable, and visible in the
system event log.

The Librarian should be allowed to know that a capability exists, such as "there
is an OpenRouter key for this job", without seeing the raw value. Agents should
receive grant tokens or broker URLs, not plaintext credentials.

## Docker Notes

Docker is a useful isolation layer, but bind mounts and Docker daemon access are
powerful. Rootless Docker is recommended where supported. Rootless mode reduces
daemon and container privileges, but it is not a substitute for careful mount and
network policy.

## Network Policy

The runner treats network as a capability:

- `none`: default for local analysis or patch generation.
- `provider`: allow only provider access through a future proxy.
- `open`: direct internet access for tools that require it.

The MVP exposes this as a run option. Fine-grained egress control is a later
hardening milestone.

## Project Autonomy

Librarian treats the mounted project as the agent's working boundary. In the
default `ProjectFull` mode, an agent may edit, delete, test, commit, and push
inside that boundary if the relevant tools and credentials were intentionally
made available to the container.

Git behavior is configured per project. The default policy allows commits and
pushes, while marking `main` and `master` as protected branch names for future
strategy checks. Later milestones should add branch-pattern rules, protected
branch handling, remote allowlists, and audit records for every git write.

## Library Tools Boundary

Librarian chat must not receive broad host filesystem access. The MVP tool
boundary is explicit:

- `Library` is the only root available to library tools.
- `Projects` is handled by a separate workspace/project tool namespace.
- Tool inputs are relative paths only; absolute paths and `..` traversal are
  rejected.
- Existing paths are canonicalized and checked to remain inside the selected
  root before use.
- Folder and empty-file creation is allowed only inside the selected tool root.
- Markdown content read/write is allowed only for `.md` files under `Library`.
- External implementation directories may be attached to project records only if
  the user selected an existing directory; library tools do not create arbitrary
  external directories.
- Tool writes are recorded as `library_tool` system events.

Tool execution also passes through persisted `[tool_permissions]` policy. The
first policy values are `auto`, `ask`, and `deny`. Direct slash commands count as
explicit user intent for `ask` policies, while `deny` blocks the operation and
logs a `tool_permission` event. Destructive delete still requires an explicit
`--yes` flag in addition to policy allowance.

Memory writes through `/mem remember` and `/remember` use the same policy gate
via `tool_permissions.memory_write` and are logged as `memory_tool` events.
Tool-permission changes are exposed through `/settings`, pass through
`tool_permissions.settings_change`, require an explicit `--yes` confirmation,
persist back to `.cfg/config.toml`, and are logged as `settings_tool` events.
Background agent launch, cancel, and retry are exposed only through explicit
`/agent` slash commands, pass through `tool_permissions.agent_launch`, and
require `--yes` for state-changing operations.
