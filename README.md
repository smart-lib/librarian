# Librarian

## One-Line Ubuntu Install

```bash
wget -qO- https://raw.githubusercontent.com/smart-lib/librarian/main/scripts/install-ubuntu.sh | bash
```

Nightly/develop build:

```bash
wget -qO- https://raw.githubusercontent.com/smart-lib/librarian/main/scripts/install-ubuntu.sh | bash -s -- --nightly
```

Librarian is a local-first automation harness for ideas, projects, and coding agents.
It runs a small root orchestrator on the host, keeps durable memory in SQLite and
an Obsidian-compatible Markdown knowledge base, and launches worker agents inside Docker
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
- One global Obsidian-compatible Markdown knowledge base for chats, project notes,
  decisions, and background run summaries.
- Local, open-source secret handling with a future broker mode where agents can
  use capabilities without reading raw credentials.
- MIT licensed for now, documented, and configurable from the beginning.

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

## License

Librarian is currently MIT licensed. That keeps adoption, embedding, and
commercial/internal use simple while the architecture is still moving quickly.

GPLv3 would be a good fit if the project should force redistributed modified
versions to stay open. AGPLv3 is the stronger choice if the long-term concern is
hosted/network service forks, because it closes the common "run it as a service"
gap. The trade-off is adoption friction: GPL/AGPL can make companies and plugin
authors more cautious. The practical recommendation for the MVP is to keep MIT
until the extension/provider/plugin boundaries are clearer, then decide whether
the core should move to GPLv3/AGPLv3 before there are many outside contributors.

## Quick Start

### Ubuntu Golden Path

For a normal Ubuntu or WSL Ubuntu setup, start with Git and let the project
bootstrap install the missing pieces. This developer path keeps the checkout
where you cloned it, but installs the runnable binary into `~/Librarian/.app`.

```bash
git clone https://github.com/smart-lib/librarian.git
cd librarian
bash scripts/bootstrap-ubuntu.sh --yes
```

The bootstrap installs system packages, Rust, Node.js/npm, Codex CLI, Docker,
builds Librarian, creates the default single root at `~/Librarian`, installs
the binary to `~/Librarian/.app/bin/librarian`, and tries to build the agent
image. It also links `librarian` into `/usr/local/bin` for immediate shell use
and `~/.local/bin` for a user-local fallback.
If `~/.local/bin` is not already on `PATH`, the bootstrap adds it to
`~/.profile` and `~/.bashrc`.
Start the admin UI with the command printed at the end:

```bash
librarian --home "$HOME/Librarian" admin --bind 0.0.0.0:17377
```

From Windows with WSL2, open:

```text
http://127.0.0.1:17377
```

Run the broad local/preflight smoke suite with one command:

```bash
librarian --home "$HOME/Librarian" smoke all --provider codex
```

This runs provider diagnostics, context/tree memory checks, Library/Projects
tool checks, approval persistence, and the MVP provider preflight. Add
`--run-agent` when you want the final MVP step to call the real provider in the
agent container:

```bash
librarian --home "$HOME/Librarian" smoke all --provider codex --run-agent
```

Run only the MVP integration smoke test:

```bash
librarian --home "$HOME/Librarian" smoke mvp --provider codex --run-agent
```

This creates a disposable Library/Projects pair, exercises the file-tool
sandbox, writes searchable memory, queues a read-only provider job, preflights
the container command, then runs that exact job. If you only want the cheap
local/preflight part without calling the model:

```bash
librarian --home "$HOME/Librarian" smoke mvp --provider codex
```

Run the context/tree-memory smoke without calling a provider:

```bash
librarian --home "$HOME/Librarian" smoke context
```

Run the focused tool smoke without calling a provider:

```bash
librarian --home "$HOME/Librarian" smoke tools
```

This checks Library Markdown edits, Projects sandbox file operations, project
context registration, durable memory retrieval, approval persistence, chat-card
contracts, job cancel/retry lifecycle, launch-context hints, and `/project`
create/attach/status/map workflow.

Run provider health diagnostics without launching containers:

```bash
librarian --home "$HOME/Librarian" smoke providers
```

Add `--require-ready` when you want the command to fail if any configured
provider is not ready.

The same broad local/preflight smoke suite can also run through doctor:

```bash
librarian --home "$HOME/Librarian" doctor --smoke
```

API-proxy providers such as OpenRouter can use the same smoke runner after a
secret grant exists:

```bash
librarian --home "$HOME/Librarian" smoke mvp --provider open-router --secret-grant-token <grant-token> --run-agent
```

If you have exactly one stored OpenRouter secret, the smoke runner can create a
short-lived grant automatically:

```bash
librarian --home "$HOME/Librarian" smoke mvp --provider open-router --secret <secret-name-or-id> --run-agent
```

Print the expanded WSL/Ubuntu smoke-test sequence with:

```bash
librarian --home "$HOME/Librarian" runtime smoke-plan
```

That command does not modify state; it prints the one-command smoke entry point
plus the manual doctor, image build, project creation, agent queue, worker, and
inspection commands.

If `doctor` reports a missing Codex profile, sign in once with Librarian's
portable profile:

```bash
CODEX_HOME="$HOME/Librarian/.cfg/codex-home" codex
"$HOME/Librarian/.app/bin/librarian" --home "$HOME/Librarian" auth codex --enable-container-mount --codex-home "$HOME/Librarian/.cfg/codex-home"
"$HOME/Librarian/.app/bin/librarian" --home "$HOME/Librarian" doctor
```

The one-line installer uses a temporary source checkout under
`~/Librarian/.app/source`, builds the binary, installs it into
`~/Librarian/.app/bin`, then removes the checkout. Normal use does not require a
Git working tree. It also writes install metadata to
`~/Librarian/.app/version.json`.

Upgrade an installed Ubuntu/WSL Librarian with:

```bash
librarian --home "$HOME/Librarian" upgrade
```

Use `--nightly` to upgrade from `develop`, or `--ref <branch-or-tag>` for a
specific ref. `doctor` prints the same upgrade command and reports the running
binary version plus the recorded install metadata.

Default installed layout:

```text
~/Librarian/
  .app/       installed binary, temporary source checkout, run artifacts
  .cfg/       config.toml, Codex profile, other portable settings
  .mdb/       SQLite database and machine-readable data
  Library/    Markdown project memory and Obsidian-style notes
  Projects/   default working directories for user projects
```

### What The Ubuntu Bootstrap Does

The short command above is equivalent to:

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates curl git build-essential pkg-config libssl-dev python3 util-linux-extra docker.io
curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt-get install -y nodejs
sudo npm install -g @openai/codex
sudo systemctl enable --now docker
sudo usermod -aG docker "$USER"
cargo build --release
install -Dm755 ./target/release/librarian "$HOME/Librarian/.app/bin/librarian"
sudo ln -sfn "$HOME/Librarian/.app/bin/librarian" /usr/local/bin/librarian
mkdir -p "$HOME/.local/bin"
ln -sfn "$HOME/Librarian/.app/bin/librarian" "$HOME/.local/bin/librarian"
export PATH="$HOME/.local/bin:$PATH"
"$HOME/Librarian/.app/bin/librarian" --home "$HOME/Librarian" setup --yes --runtime host
sg docker -c '"$HOME/Librarian/.app/bin/librarian" --home "$HOME/Librarian" runtime build-agent-image'
"$HOME/Librarian/.app/bin/librarian" --home "$HOME/Librarian" doctor
```

The agent image installs Codex and Claude Code by default. Use
`runtime build-agent-image --no-claude` or `--no-codex` only for a deliberately
smaller provider-specific image.

On some Ubuntu/WSL installations, Docker group membership is not active until
the next login. The bootstrap tries to use a fresh `docker` group session for
the image build; if the system refuses, log out/in or rerun:

```bash
sg docker -c '"$HOME/Librarian/.app/bin/librarian" --home "$HOME/Librarian" runtime build-agent-image'
```

### Windows Developer Path

Prerequisites:

- Rust stable toolchain. On Windows, the bootstrap script uses the GNU target
  with MSYS2/UCRT GCC.
- Podman on Windows, or Docker/Podman on Linux and macOS.
- Codex CLI installed on the host for authentication bootstrap.

By default, Librarian stores one root in the current user's application area:
`%APPDATA%\Librarian` on Windows, `~/Librarian` on Linux, and
`~/Library/Application Support/Librarian` on macOS. The root contains hidden
application/config/database folders plus public `Library` and `Projects`
folders. Override this with
`--home <path>` or `LIBRARIAN_HOME` when you intentionally want a portable or
project-local root.

`doctor` calls this the `librarian root (state)`. It is intentionally separate
from the `launch context (cwd)`, which is simply the directory where the command
was started and can later be used as a current-project hint.

For first setup, use `setup`. It creates the root, migrates SQLite, records the
launch directory as the current context, optionally selects the WSL Podman
runtime on Windows, and runs `doctor`.

```powershell
.\scripts\bootstrap-windows.ps1
cargo +stable-x86_64-pc-windows-gnu run -- setup
cargo +stable-x86_64-pc-windows-gnu run -- auth codex --enable-container-mount --codex-home "$env:APPDATA\Librarian\codex-home"
cargo +stable-x86_64-pc-windows-gnu run -- runtime build-agent-image
cargo +stable-x86_64-pc-windows-gnu run -- doctor
cargo +stable-x86_64-pc-windows-gnu run -- project add c:\path\to\project
cargo +stable-x86_64-pc-windows-gnu run -- admin
```

`doctor` prints an actionable readiness report with `ok`, `warn`, and `error`
checks for the config layout, SQLite, container runtime, agent image, Codex CLI,
and Codex profile mount. Treat `blocked` as the environment setup todo list
before attempting a real worker run.

For portable Codex auth, sign in with `CODEX_HOME` pointing at Librarian's
`codex-home`, then enable the explicit mount:

```powershell
$env:CODEX_HOME = "$env:APPDATA\Librarian\codex-home"
codex
cargo +stable-x86_64-pc-windows-gnu run -- auth codex --enable-container-mount --codex-home "$env:APPDATA\Librarian\codex-home"
```

For a self-contained folder build, copy `librarian.exe` and
`scripts/librarian-launcher.ps1` into one directory as `librarian.ps1`. The
launcher sets `LIBRARIAN_HOME` to `.librarian` next to the executable, so the
whole folder can be moved between machines.

If the Windows Podman CLI loses its machine connection while the WSL
`podman-machine-default` distro is still usable, switch Librarian to the WSL
fallback runtime:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- runtime use-wsl-podman
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
cargo +stable-x86_64-pc-windows-gnu run -- jobs preflight <job-id>
cargo +stable-x86_64-pc-windows-gnu run -- jobs cancel <job-id>
cargo +stable-x86_64-pc-windows-gnu run -- jobs retry <job-id>
```

`jobs preflight <job-id>` resolves routing, budget checks, project context,
prompt construction, run artifacts, and the prepared container command without
launching the container or completing the job. It records a `preflight` event
for later inspection.

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

Jobs can carry a grant token so the worker injects `LIBRARIAN_SECRET_GRANT_TOKEN`
and `LIBRARIAN_BROKER_URL` into the container without exposing the raw secret:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- run --project my-project --provider open-router --goal "Use the brokered provider" --secret-grant-token <token>
```

Inspect provider routing and limits:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- providers catalog
cargo +stable-x86_64-pc-windows-gnu run -- providers status
cargo +stable-x86_64-pc-windows-gnu run -- providers pause codex --model codex-cli-default --seconds 1800 --reason "rate limit"
cargo +stable-x86_64-pc-windows-gnu run -- providers resume codex --model codex-cli-default
cargo +stable-x86_64-pc-windows-gnu run -- usage list --limit 20
cargo +stable-x86_64-pc-windows-gnu run -- config set-fallbacks true
cargo +stable-x86_64-pc-windows-gnu run -- config set-fallback-order codex openrouter claude-code
cargo +stable-x86_64-pc-windows-gnu run -- config set-budget true --daily-total-usd 5 --daily-provider-usd 3 --daily-project-usd 2
```

Budget guardrails are enforced before dispatch and use known `cost_usd`
observations for the current UTC day. CLI runs that only produce local token
estimates are recorded for visibility, but they do not count against USD limits
until a provider adapter or imported telemetry reports actual cost.

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
captured into the local secret vault and replaced with `secret://...` references; known
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
to authenticate Codex on the host first. Containerized Codex runs can then mount
the host Codex profile explicitly.

```powershell
codex
cargo +stable-x86_64-pc-windows-gnu run -- auth codex --enable-container-mount
cargo +stable-x86_64-pc-windows-gnu run -- runtime build-agent-image
cargo +stable-x86_64-pc-windows-gnu run -- doctor
```

This mount is intentionally opt-in: the current self-hosting path exposes the
host Codex profile to the Codex process inside the project container. Use it for
trusted local self-hosting while the stronger brokered provider path matures.

Codex prompts are written to a per-run `/workspace/run/prompt.txt` mount and
fed through stdin. This avoids brittle long prompt arguments, especially through
Windows-to-WSL runtime wrappers.

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
