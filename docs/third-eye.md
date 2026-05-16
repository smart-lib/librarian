# Third Eye Integration Notes

Third Eye is a MIT-licensed local telemetry dashboard for Claude and Codex
usage. Librarian treats it as an optional observer for cost and limit data, not
as the source of truth for job lifecycle.

## What It Reads

The current Third Eye code has two provider ingesters:

- Codex: scans `CODEX_HOME` or `~/.codex`, then walks
  `sessions/YYYY/MM/DD/rollout-*.jsonl`.
- Claude: scans Claude project/session directories.

Codex project names are derived from `session_meta.payload.cwd` in the rollout
file. Codex rate-limit snapshots and `usage_limit_exceeded` errors are parsed
from rollout JSONL entries.

This means Third Eye can see host-user Codex/Claude logs automatically, but it
will not see container-local agent logs unless Librarian deliberately exports or
mounts those logs into paths Third Eye scans.

## API Surface

Useful HTTP endpoints:

- `GET /api/health`
- `GET /api/providers`
- `POST /api/refresh?since=1h`
- `POST /api/refresh?mode=full`
- `GET /api/overview`
- `GET /api/insights/:projectId`
- `GET /api/settings`
- `PATCH /api/settings`

The server binds to `127.0.0.1:4317` by default.

## SQLite Surface

Third Eye stores data in SQLite. The path can be overridden with `THIRD_EYE_DB`
or `CODEBURN_DB`.

Tables that are useful for Librarian:

- `api_calls`: raw provider calls, project, model, tokens, cost, branch/version
  enrichment columns.
- `projects`: Third Eye project registry.
- `tool_events`: tool-use telemetry by project and kind.
- `agent_sessions`: Claude subagent/task rollups.
- `codex_plan_daily`: daily Codex plan and limit snapshots.

The API is safer for dashboard-grade data because it preserves Third Eye's
aggregation behavior. Direct read-only SQLite access is useful for local cost
summaries, raw current usage, and fallback operation when the API server is not
running.

## Librarian Plan

Short term:

- Keep Librarian's own `usage_observations` table as primary operational memory.
- Add CLI probes for Third Eye health, providers, refresh, and read-only DB
  summary.
- Allow `third_eye.db_path` configuration for direct SQLite reads.

Next step:

- Decide per project whether provider logs are host-visible, mounted, or
  exported.
- For containerized Codex, set a per-run `CODEX_HOME` that is mounted to a
  host path Third Eye can scan, or emit a Third Eye-compatible export from
  Librarian usage observations.
- Keep project mapping explicit so Third Eye's sanitized project key can be
  tied back to Librarian project IDs.

