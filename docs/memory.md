# Memory

Librarian memory has two audiences:

- the overseer process, which enriches every user request before planning or
  running agents;
- humans, who inspect project knowledge through the global Obsidian-compatible
  vault.

SQLite is the source of operational truth. The vault is the readable notebook.

## Layers

1. Recent conversation memory: raw user/assistant turns and short summaries.
2. Project memory: decisions, instructions, status, run observations, and facts.
3. Activity memory: context for a specific thread, goal, task, or long-running
   agent lifecycle.
4. Global memory: preferences, root-level plans, provider policy, and cross-
   project knowledge.
5. Vector memory: embeddings for semantic retrieval over all of the above.

## SQLite Storage

The baseline schema keeps memory in normal SQLite tables:

- `memory_items`: text, scope, source, temporal metadata, confidence, salience,
  contradiction/supersession links, and JSON metadata.
- `memory_embeddings`: embedding blobs keyed by memory item and model.
- `memory_fts`: FTS5 fallback for lexical search.

Vector search should be extension-backed when available. Preferred backends:

1. SQLite `vec1`, once it is available in the bundled/runtime SQLite build.
2. `sqlite-vec`, because it is small, cross-platform, and successor-oriented.
3. FTS5 plus recency scoring as a fallback.

The backend must be configurable because Windows, Linux, macOS, containers, and
packaged builds may expose different SQLite extension capabilities.

Current backend:

- `local-hash`: deterministic local embeddings stored as `f32` little-endian
  BLOBs in `memory_embeddings`.
- It has no network, paid API, or native extension dependency.
- Retrieval computes a query vector, loads scoped candidate vectors from
  SQLite, scores cosine similarity in Rust, and combines that with lexical,
  recency, kind, confidence, salience, validity, and scope weights.
- The schema remains compatible with a future `sqlite-vec` or `vec1` backend;
  those backends can reuse `memory_embeddings` or add an extension table while
  preserving the context-pack contract.

## Retrieval

Every user request is enriched, including vague requests such as "what now?" or
"status?". The enrichment step should run before chat response generation,
planning, scheduling, or agent launch.

Retrieval inputs:

- current user message;
- active project, if known;
- selected project filters, if explicit;
- active activity/thread, if known;
- recent chat turns;
- current job/run context;
- time of request.

Retrieval stages:

1. Resolve scope: global, active project, selected projects, activity, or mixed.
2. Run vector search against eligible memory items.
3. Run FTS/keyword search as a fallback or complement.
4. Add pinned or active instructions.
5. Apply temporal and contradiction policy.
6. Produce a compact context pack with citations to memory ids and vault paths.

Current implementation status:

- `librarian context "<query>"` renders a context pack from SQLite memory.
- `librarian memory status` reports embedding backend, model, dimensions, total
  memory items, embedded items, and missing embeddings.
- `librarian memory embed --limit N` backfills embeddings for older items.
- `librarian run ...` records the run goal as memory and attaches a context pack
  to the job event log.
- The admin chat endpoint follows the same path.
- The worker rebuilds a fresh context pack immediately before execution and
  passes an enriched prompt to the provider adapter.
- Run outcomes are written back as `RunObservation` memory.
- New memory items are embedded when they are written by CLI, admin chat, or
  worker run observations.
- Lexical overlap remains a complement and fallback when an item does not yet
  have an embedding.

## Temporal Priority

Memory gets a recency score. Newer memory is more important by default:

- yesterday outranks last month;
- last month outranks last year;
- old memories are still useful when they complement newer ones;
- old memories lose when they directly contradict newer ones.

Suggested scoring:

```text
score = semantic_similarity
      * scope_boost
      * salience
      * confidence
      * recency_decay
      * validity_multiplier
```

`recency_decay` should be gentle for durable project facts and stronger for
plans, preferences, status, and instructions. A status update from yesterday
should easily outrank one from a year ago.

The first implementation uses deterministic local vectors for semantic score.
This is less expressive than a model embedding, but it exercises the full vector
storage and scoring path while remaining free, local, and cross-platform.

## Contradictions

When two memories cannot both be true:

- the newer memory wins unless it has much lower confidence;
- the older memory is excluded from the active context pack;
- the relationship is stored through `contradicts_id` or `supersedes_id`;
- both memories remain auditable.

When two memories complement each other:

- both may be included;
- the older one receives lower rank;
- summarization can merge them into a newer compact memory.

## Scope Filters

Retrieval must support:

- global/root context;
- one active project;
- multiple selected projects;
- activity or thread context;
- provider/job/run context;
- memory kind filters such as `Instruction`, `Decision`, `Status`, or `Fact`.

Questions like "what are we working on?" should search global and active project
status. Questions like "what is the studio site roadmap status?" should prefer
the studio-site project and roadmap/status memories.

## Compaction

Compaction is a scheduled process:

1. Gather related older memories.
2. Preserve decisions and instructions explicitly.
3. Merge complementary memories into a newer summary.
4. Mark obsolete entries with `supersedes_id`.
5. Re-embed the new summary.

Compaction must not silently delete raw memory. Deletion is a retention policy,
not a summarization side effect.
