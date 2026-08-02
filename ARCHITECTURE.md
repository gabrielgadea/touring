# Touring Workspace — Architecture Reference

> **v30.0.0** | 42 crates | ~532k LOC (src) / ~603k (workspace incl. tests) | ~14k test fns | clippy deny-all (0 warnings) | SCHEMA_VERSION=8 | **daemon-lib-rearch COMPLETE (2026-06-11, 5 PoNRs + Wave H)**: the touring-hooks monolith (169k = 36% of workspace) is fully decomposed into 6 crates — `touring-hooks` façade 1.1k (0,2%) → `touring-dispatch` 37.5k (7,8%, hook_registry + daemon + lifecycle/) → `touring-hooks-core` 31.6k (knowledge + tantivy_index + health_delta + bridges) → `touring-hook-handlers` 26.2k (24 pre/post/session hooks) → `touring-cli` 25.4k (55 cli/ handlers + cli_suggester) → `touring-hook-runtime` 18.5k (HookRuntime + runtime/impls_* + inferlets + wiring); no crate exceeds 15% (largest is touring-intelligence 64.5k ≈ 11,8%, never part of the monolith); 3.154 tests zero-loss; doctor 6/6 green (wiring_diagnostic kind_unknown=0, first time). Earlier seams preserved: `touring-ceg` leaf (gateway/ X0-X9 + capability/), `touring-contracts` IoC leaf (LearnRuntime/CegRuntime), saga + agentic_rl leaf crates
> <!-- METRICS: measured in loco 2026-07-24 via docs/sync_metrics.py --json (crates=42, loc_src=584241, loc_workspace=655305, test_fns=15592). Keep in sync via docs/sync_metrics.py (--check gate, now covers crates+LOC+test_fns). -->
>
> **Diátaxis role**: this file is the canonical *detailed reference* (crate map + metrics, auto-synced by `docs/sync_metrics.py`). It is intentionally **not** a thin pointer (the metrics-as-code gate lives here). For the narrative *why* see [`docs/explanation/architecture.md`](docs/explanation/architecture.md); for generated catalogs see [`docs/reference/`](docs/reference/) (generators · mcp-tools · hooks · modules); for tasks see [`docs/how-to/`](docs/how-to/).
> Location: `~/.claude/rust/` | Binaries: `touring-hook` (~60 MB) + `touring-daemon` (~60 MB) + `touring` (~70 MB)
> Previous: [ARCHITECTURE.v29.5.0.md](ARCHITECTURE.v29.5.0.md) — full version history

## THSF WASM Component Architecture — Option D (2026-04-25)

Workspace refatorado em duas árvores de build independentes:

```
holon-wasm-components/     ← wasm32-wasip2 workspace (4 components)
├── spec-version/           → holon_spec_version.wasm (62 KB)
├── blast-radius/           → holon_blast_radius.wasm (169 KB)
├── quality-gate/           → holon_quality_gate.wasm (145 KB)
├── generator-health/       → holon_generator_health.wasm (157 KB)
├── compose/
├── scripts/
└── .holon/manifest.toml

holon-wasm-runner/          ← x86_64-unknown-linux-gnu standalone host crate
└── src/main.rs             → holon-wasm-runner (30 MB binary)
```

**Option D**: O `runner/` foi extraído como crate host separado
(`holon-wasm-runner/`, target x86_64-unknown-linux-gnu). Isso previne
que o workspace wasm32-wasip2 seja detectado pelo workspace pai
`~/.claude/rust/`. Cada crate builda independentemente — zero
cross-contamination.

### WIT Contract

```wit
package holon:core@0.1.0;

interface types {
    record invoke-request {
        capability: string,
        args: list<u8>,
        requester: string,
        timeout-ms: u32,
    }
    record invoke-response {
        exit-code: s32,
        stdout: list<u8>,
        stderr: list<u8>,
        duration-ms: u32,
        logged: bool,
    }
    variant invoke-error {
        unknown-capability(string),
        invalid-args(string),
        internal(string),
    }
}

interface capabilities {
    list-capabilities: func() -> list<string>;
    invoke: func(request: invoke-request) -> result<invoke-response, invoke-error>;
}

world holon-component { export capabilities; }
```

### Capabilities

| Name | Input | Output |
|------|-------|--------|
| `spec-version` | `{}` | `{"spec_version":"0.1.0"}` |
| `blast-radius` | `{"graph": {...}, "target": "x.rs"}` | `{"affected": [...], "count": N}` |
| `quality-gate` | `{"source": "...", "lang": "rust\|python"}` | `{"score": 1.0, "issues": [...], "density": 0.0}` |
| `generator-health` | `{"counters": {...}, "per_path": [...]}` | `{"summary": "...", "alerts": [...], "metrics": {..., "health_score": 0.9}}` |

### Cross-Audit Fixes Applied (2026-04-25)

| Bug | Fix |
|-----|-----|
| `record_count` dead field in `generator-health` | Removido — era escrito mas nunca lido |
| Deprecated `post_return()` calls no runner | Removido — wasmtime 42 deprecou a API |
| Output mostrava byte arrays crus (`[123, 34, ...]`) | Adicionados `try_unwrap_invoke_response()` + `try_format_error()` — auto-extract + pretty-print JSON |
| Duplicate `[lib]` keys em 4 Cargo.toml | Reescritos sem duplicatas |

### E2E Test Suite

```
./tests/e2e_run.sh  →  14/14 PASS
```

Cobertura: spec-version (2), blast-radius (3), quality-gate (5),
generator-health (3), error handling (1).

---

## `touring serve` Runtime Model (v30.3.5, 2026-04-21)

The MCP stdio server binary (`src/bin/touring` from `touring-server` crate)
no longer uses `#[tokio::main]`. `main.rs` now builds the rayon global pool
first, then an explicit `tokio::runtime::Builder::new_multi_thread()` with
env-tunable worker counts, before dispatching to `async_main()`.

| Env var | Default | Controls |
|---|---|---|
| `TOURING_MCP_WORKERS` | `num_cpus::get_physical()` | Tokio worker threads (physical cores, not SMT logical) |
| `TOURING_BLOCKING_WORKERS` | 512 | `spawn_blocking` pool cap (SQLite, Tantivy) |
| `TOURING_RAYON_THREADS` | `physical / 2` | Rayon global pool, isolated from tokio workers |

Thread stack size is 4 MiB (AST recursion). Thread names are
`touring-mcp-worker-*` and `touring-rayon-*` (visible in tokio-console,
`pidstat -t`, and OTLP traces).

**Anti-pattern eliminated**: `streaming_mcts_search` previously nested
`Builder::new_current_thread()` inside `spawn_blocking` + `spin_loop()`,
pinning 1 core to 100% for the 20ms deadline window. Now uses
`spawn_blocking` + `thread::yield_now()` directly — `StreamingMCTS`'s
internal rayon pool does the multi-core work.

**Boot stabilization**: Two recurring warnings silenced to match their
actual severity — eBPF without compiled bytecode is `info!` (expected
degradation on workstations); missing optional `touring_bootstrap_symbols.py`
is `debug!` + skip, not `warn!` + ENOENT loop every 30 min.

See `docs/2026-04-21-touring-serve-multicore-scaling.md` for the full
rationale, fixes (S1, S2, S5, W1, W2), and roadmap (S3-S8).

---

---

## What Touring Is

A Rust-native intelligence layer for Claude Code. Every tool call (Read, Edit, Write, Bash)
flows through Touring hooks that inject contextual signals, learn from outcomes, and persist
knowledge across sessions. 18 hooks across 12 Claude Code events, all < 50ms latency.

## Key Metrics

| Metric | Value |
|--------|-------|
| Crates | **45** workspace crates |
| Tests | **~13,964** test fns (0 failed; touring-python excluded by design — pyo3 linking, not compilation errors; touring-server compiles clean, see Quality Gates note) |
| Hook Registry | **218** (per docs/reference/hooks.md — generated by docs/gen_reference.py) |
| Cortex handlers | 84 (H01–H84) |
| CLI subcommands | ~120 (table-driven dispatch) |
| MCP tools | **~164** exposed in the default build (per docs/reference/mcp-tools.md). A curated **22**-tool surface is scaffolded behind `--features mcp-curated` (default **OFF**); the 102→22 reduction is authored (W1-W4) but **not yet the default**. NB: `mcp-legacy` gates 0 blocks — historical tools are not actually feature-gated |
| AST languages | 14 |
| Hook latency (warm) | P50=1ms, avg=2ms |
| SCHEMA_VERSION | 8 |
| Databases | 3 consolidated: knowledge.db, memory.db, graph.db |
| Feature flags active | 11 (incl. u4-quantization, hnsw-working-memory) |

## Workspace Topology

<!-- CRATES:BEGIN (generated by docs/sync_metrics.py --sync — do not edit by hand) -->
**41 crates · 584,671 LOC (src)** — generated by `docs/sync_metrics.py --sync`; each crate's own `ARCHITECTURE.md` carries type-level detail. Largest crate is well under the 15% single-crate ceiling.

| # | crate | src LOC | % |
|--:|-------|--------:|--:|
| 1 | `touring-server` | 77,604 | 13.3% |
| 2 | `touring-intelligence` | 68,833 | 11.8% |
| 3 | `touring-analysis` | 39,177 | 6.7% |
| 4 | `touring-dispatch` | 37,350 | 6.4% |
| 5 | `touring-hooks-core` | 32,682 | 5.6% |
| 6 | `touring-cortex` | 30,345 | 5.2% |
| 7 | `touring-bindings` | 30,284 | 5.2% |
| 8 | `touring-code` | 30,024 | 5.1% |
| 9 | `touring-cli` | 28,584 | 4.9% |
| 10 | `touring-hook-handlers` | 26,855 | 4.6% |
| 11 | `touring-foundation` | 26,405 | 4.5% |
| 12 | `touring-ceg` | 19,204 | 3.3% |
| 13 | `touring-hook-runtime` | 19,149 | 3.3% |
| 14 | `touring-quality` | 18,823 | 3.2% |
| 15 | `touring-storage` | 16,143 | 2.8% |
| 16 | `touring-hooks-shared` | 14,755 | 2.5% |
| 17 | `touring-generator` | 13,881 | 2.4% |
| 18 | `touring-offensive` | 10,896 | 1.9% |
| 19 | `touring-simd` | 9,356 | 1.6% |
| 20 | `touring-hooks-prediction` | 5,556 | 1.0% |
| 21 | `touring-server-reasoning` | 5,162 | 0.9% |
| 22 | `inferlets` | 4,000 | 0.7% |
| 23 | `touring-resilience` | 3,991 | 0.7% |
| 24 | `touring-orchestration` | 2,942 | 0.5% |
| 25 | `touring-server-visual` | 2,922 | 0.5% |
| 26 | `touring-assists` | 2,021 | 0.3% |
| 27 | `touring-identity` | 1,521 | 0.3% |
| 28 | `touring-hooks-rl` | 1,272 | 0.2% |
| 29 | `touring-hooks` | 1,254 | 0.2% |
| 30 | `touring-rkyv` | 916 | 0.2% |
| 31 | `touring-lsp` | 824 | 0.1% |
| 32 | `touring-server-session` | 726 | 0.1% |
| 33 | `touring-hooks-saga` | 672 | 0.1% |
| 34 | `touring-license` | 345 | 0.1% |
| 35 | `touring-contracts` | 103 | 0.0% |
| 36 | `touring-web-server` | 25 | 0.0% |
| 37 | `touring-capnp-server` | 17 | 0.0% |
| 38 | `touring-loom-proofs` | 17 | 0.0% |
| 39 | `touring-integration-tests` | 12 | 0.0% |
| 40 | `touring-web` | 12 | 0.0% |
| 41 | `touring-python` | 11 | 0.0% |
<!-- CRATES:END -->

Each crate has its own `ARCHITECTURE.md` with detailed types, modules, and invariants.

## L7-B Metabolic Evolution (2026-04-10)

```
                   [Phase Alpha: Dormant Awakening]
                              │
     ┌────────────────────────┼────────────────────────┐
     ▼                        ▼                        ▼
 crdt_graph             cognitive_runtime      enrichment_pipeline
 cold_start              lazy_init               requires trigger
     │                        │                        │
     └─────► load_crdt_graph() + spawn_cognitive_background_tasks()
                      + trigger_enrichment()
     │                        │                        │
     ▼                        ▼                        ▼
 healthy/loaded         healthy/initialized    active/auto-triggered

                    [Phase Beta: Gate Instrumentation]
                              │
           ┌──────────────────┴──────────────────┐
           ▼                                     ▼
    should_enrich(active, cila, tool)    WASM inferlets embedded
    (Layer 1: active ✓)                  (wasm32-unknown-unknown target)
    (Layer 2: cila ≥ 2 ✓)                (sha2 no-std → 59817 bytes)
    (Layer 3: tool ∈ mutation_set ✓)     (include_bytes! → touring-hook 17.8MB)

                    [Phase Gamma: Observability + Async]
                              │
           ┌──────────────────┴──────────────────┐
           ▼                                     ▼
    gate_metrics.rs (AtomicU64 × 5)       job_registry.rs (DashMap)
    pre_edit_fast/full                    JobState { Running, Completed, Failed }
    pre_write_fast/full                   spawn_worker (execve, no shell)
    post_tool_l4_mandatory                poll_worker (Running→terminal)
    GateMetricsSnapshot                   list_jobs / drop_job

                    [Phase Delta: MCP + Lifecycle Completion]
                              │
           ┌──────────────────┴──────────────────┐
           ▼                                     ▼
   rmcp #[tool] macros                   Inferlets fix (sha2 no-std)
   touring_spawn_worker                  wasm32-unknown-unknown
   touring_poll_worker                  4 pools loaded (fuel 10K–30K)
   touring_list_jobs                     gate_metrics in `touring status -j`
   touring_drop_job (audit)              poll_worker CC 21→8 refactor

                    [Cross-Audit: Potentialization]
                              │
           ┌──────────────────┴──────────────────┐
           ▼                                     ▼
   Connection semaphore wired            drop_job wired end-to-end
   (was dormant since v30.0.0)           (5-point registration)
   MAX=16 per-project quota              SPAWN→POLL→LIST→DROP
   acquire_owned in dispatch             cli + MCP tool + handler

                    [E2E Integration Test Suite]
                              │
                  tests/l7b_e2e.rs (15 tests)
                              │
                  15 passed, 0 failed, 0.05s
                              │
                  composite_score = 1.0
```

### L7-B New Modules & Wiring

**`touring-hooks/src/shared/gate_metrics.rs`** (253 lines, 5 tests)
- `GateMetrics` struct with 5 `AtomicU64` counters
- `global() -> &'static GateMetrics` via `OnceLock`
- 5 `record_*` helper functions + `GateMetricsSnapshot::capture()`
- **Consumers (7 sites)**: pre_edit.rs:70,74 + pre_write.rs:118,128 + post_tool_use.rs:95 + cli_handlers.rs:2271–2272 + cli_status.rs (via daemon query)

**`touring-hooks/src/shared/job_registry.rs`** (480 lines, 7 tests)
- `JobState` enum (Running / Completed / Failed) + `is_terminal()` / `status_str()` / `started_at()` accessors
- Singleton `Arc<DashMap<String, JobState>>` via `OnceLock`
- `spawn_worker(tool_name, program, args)` — tokio::spawn + execve, no shell
- `poll_worker(job_id)` — refactored to CC=8 via helpers (`try_transition_to_terminal`, `state_to_json`, `terminal_from_join`, `placeholder_state`)
- `list_jobs()` / `drop_job(id)` — full lifecycle primitives
- **Consumers (10 sites)**: cli_handlers.rs (4 handlers) + server/mod.rs (4 MCP tools) + hook_registry.rs (5 dispatch entries) + shared/mod.rs (module)

**`touring-hooks/src/shared/cila.rs`** (extended +75 lines)
- `should_enrich(enrichment_active, cila_level, tool_name) -> bool` — 3-layer gate
- `is_enrichment_mandatory(active, cila) -> bool` — L4+ mandatory check
- **AD-L7B-01**: Deliberate duplicate of `touring-cortex::enrichment::EnrichmentPolicy` to avoid circular dependency (cortex → hooks, not the reverse)
- **Consumers**: pre_edit.rs:64 + pre_write.rs:110 + post_tool_use.rs:94

**`touring-cortex/src/enrichment.rs`** (extended +131 lines)
- `EnrichmentPolicy` struct with `cila_level` + `enrichment_active`
- `should_enrich(&self, tool_name) -> bool` method + `is_mandatory() -> bool`
- 8 unit tests
- **Consumers**: cross_audit.rs:15 + lib.rs:73 (independent from hooks version)

**`touring-hooks/src/daemon.rs`** (refactored 2026-04-12 — actor pattern)
- `ProjectCommand` enum: `RunHook { hook_name, payload, response: oneshot::Sender<String> }` + `Shutdown { done: oneshot::Sender<()> }`
- `ProjectRuntime { cmd_tx: mpsc::Sender<ProjectCommand>, last_accessed, connection_semaphore }` — `Arc<Mutex<HookRuntime>>` removed
- `run_project_actor(runtime, cmd_rx)` — dedicated OS thread (`std::thread::Builder::name("touring-project-actor")`) owns `HookRuntime` and processes commands serially
- `dispatch_request_async` extracts `(mpsc::Sender<ProjectCommand>, Arc<Semaphore>)` tuple, sends `RunHook` via channel (timeout-bounded), awaits oneshot response with per-hook budget (15s light / **300s heavy**)
- Both global (64) and per-project (**56**, raised from 16) semaphores wrapped in `tokio::time::timeout(REQUEST_TIMEOUT, acquire_owned())` → fail-fast under saturation, no FD leak
- Accept loop: exponential backoff (`100ms × 2^streak`, cap 2s) on transient errors
- Panic-safe: `catch_unwind(AssertUnwindSafe(...))` around each handler + E2E scan
- `is_heavy_hook()` expanded: +cli-tantivy-reindex, +cli-wiring-chains, +cli-wiring-audit, +cli-e2e, +cli-ast-blast-cross-feature
- `graceful_shutdown` rewritten: send `Shutdown { done }` per actor, await oneshot with 5s timeout, actor runs WAL+LinUCB+CRDT saves inside its thread

**`touring-server/src/server/mod.rs`** (extended — 4 MCP tools)
- `#[tool(name = "touring_spawn_worker")]` → `JobsSpawnParams { tool_name, program, args }`
- `#[tool(name = "touring_poll_worker")]` → `JobsPollParams { job_id }`
- `#[tool(name = "touring_list_jobs")]` → `JobsListParams {}` (empty)
- `#[tool(name = "touring_drop_job")]` → `JobsDropParams { job_id }`

**`touring-server/src/cli/{gate_metrics,inferlets,jobs}.rs`** (3 new files)
- CLI dispatchers calling `daemon_query()` for each subcommand
- Registered in `cli/mod.rs` + `cli/common.rs::command_table()`

**`touring-server/src/cli/status.rs`** (extended)
- `STATUS_QUERIES` array includes `("gate_metrics", "cli-gate-metrics")` — aggregated view

**`touring-hooks/tests/l7b_e2e.rs`** (NEW, 15 integration tests)
- Exercises Alpha+Beta+Gamma+Delta composition end-to-end
- Includes `e2e_full_pipeline_integration` that chains should_enrich → gate_metrics →
  spawn_worker → poll → drop across CILA levels 0–4
- **Result**: `15 passed; 0 failed; finished in 0.05s`

## Dependency Graph (simplified)

```
                    touring-core
                   /     |      \
            touring-simd  |   touring-rules
              /    |      |
           touring-ast |  touring-learning ← u4-quantization feature
               |       |      |
            touring-index |  touring-cognitive
               |       |      |
            touring-analysis ← blast radius, quality, wiring, health score
               |       |      |
            touring-hooks ← RefCell<ANN>, daemon, 8 hook files
                |       \
         touring-cortex  touring-wasm ← inferlets
                |
          touring-server (MCP + CLI + migrate)
                |
          touring-python (PyO3 FFI)
```

## 3 Consolidated Databases (SCHEMA_VERSION=8)

| Database | Path | Content | Owner |
|----------|------|---------|-------|
| `knowledge.db` | `.claude/touring/knowledge.db` | Symbols, file knowledge, wiring map, gotchas, edit history + **PLN2 11 tables** | `FileKnowledgeDB` |
| `memory.db` | `.claude/touring/memory.db` | RLM entries, semantic recall + embeddings, ANN embeddings, **Palace Hierarchy**, **Agent Diary** (PLN2 P4.1/P4.2) | `RlmMemory`, `SemanticRecall`, `PersistedAnnMemoryRecall` |
| `graph.db` | `.claude/touring/graph.db` | GoT snapshots, RL pipeline (Wilson/QTable/LinUCB/Drift), sessions, hook events | `LearningPersistence`, `GoTSnapshotStore` |

### PLN2 Extended Knowledge Tables (knowledge.db — v30+)

11 tables added via `reindex_file()` (touring-hooks/src/shared/reindex.rs:105-165) on every edit/write:

| Table | Purpose | Wired by |
|-------|---------|----------|
| `file_feature_flags` | Feature flags from Cargo.toml, pyproject.toml, package.json | `upsert_feature_flags_batch()` |
| `file_todos` | TODO/FIXME/XXX extracted from file content | `insert_todo()` |
| `edge_confidence` | Confidence scores for import graph edges | `upsert_edge_confidence()` |
| `file_communities` | Louvain community assignments per file | `upsert_file_community()` |
| `file_test_coverage` | Test coverage %, tested/total functions | `upsert_test_coverage()` |
| `file_blake3_registry` | BLAKE3 content hash + symbol count | `upsert_blake3_registry()` |
| `session_file_summary` | Session-file skeleton summaries with purpose | `upsert_session_file_summary()` |
| `symbol_events_log` | Symbol create/modify/delete events | `insert_symbol_event()` |
| `wiring_suggestions` | Orphan symbol wiring recommendations | `upsert_wiring_suggestion()` |
| `metadata_benchmark_runs` | Benchmark results (commit_hash, p50/p95/p99) | `insert_benchmark_run()` |
| `cognitive_enrichment` | Cognitive scores (complexity, fan-in, fan-out, doc) | `upsert_cognitive_enrichment()` |

**Schema constants**: `touring-analysis/src/e2e/schema_guard.rs:57-87` (single source of truth)
**E2E tests**: `cargo test -p touring-hooks --test pln2_e2e` — 32 tests covering all 11 tables

Canonical path resolution: `TouringConfig::knowledge_db_canonical(&root)`, `memory_db_canonical()`, `graph_db_canonical()`.

Migration tool: `touring migrate {status|plan|run|validate|cleanup|rollback}`.

## Hook Pipeline (18 hooks in settings.json)

```
SessionStart → touring-hook session-start (LinUCB warm, cache pre-warm)

Read → pre-read (gotchas + blast radius + ANN recall + symbol map)
     → post-read (record file metadata)
     → post-tool-rl (RL reward)

Edit → pre-edit (scored signals: quality, wiring, complexity, callers, Tarjan SCC cycle check)
     → pre-edit-prevention (syntax check, CC threshold via Speculate L6)
     → post-edit (track + reindex + ANN store + verify + feedback)
     → post-tool-rl (RL reward from quality delta)

Write → pre-write (6-layer speculate + antipatterns + wiring prediction)
      → post-write (reindex + wiring registration)
      → post-tool-rl

Bash → pre-bash (recall past failures)
     → post-bash (record outcome)
     → post-tool-rl

PreCompact → pre-compact (flush LinUCB + WAL checkpoint + GoT snapshot persist)
FileChanged → file-changed (invalidate wiring cache)
SessionEnd  → session-stop (persist metrics)
```

## ANN Memory Recall (v29.8.0)

- **pre_read.rs**: `ann_recall_signal()` searches persistent ANN index with path-hash embeddings (FNV-1a, 64-dim, <1μs)
- **post_edit.rs**: `ann_recall.add_memory()` stores edit context via `RefCell<Option<PersistedAnnMemoryRecall>>`
- **Persistence**: SQLite WAL in `memory.db`, warm restart loads all entries on session-start

## U4 Quantization (v29.8.0)

- Feature chain: `touring-server → touring-hooks → touring-learning → touring-simd → dep:half`
- `EmbeddingU4`: Per-vector min/max 4-bit, 8× compression, ~90% Recall@10
- `SemanticRecall::ann_search()`: prefers `ann_search_u4()` path, falls back to f32
- `store_chunk()`: writes both f32 + u4 columns simultaneously

## PLN2 P4 — Meta-Optimization (2026-04-09)

### P4.1 — Agent Diary System

`crates/touring-server/src/cli/diary.rs` + `crates/touring-server/src/agent_diary.rs`

- **Key Hierarchy**: `wing_{agent}/diary/{meta,entries/{ts},topics/{topic}}`
- **AAAK Dialect** (Adaptive Abbreviated Agent Knowledge):
  - `#P` = phase marker | `#R` = result/score | `#L` = lesson | `#W` = warning | `#E` = error
- **CLI** (direct, no daemon socket):
  - `touring diary write <agent> <entry> [--topic <topic>] [--aaak]`
  - `touring diary read <agent> [--last N] [--topic <topic>]`
  - `touring diary list` | `touring diary meta <agent>`
- **Schema isolation**: CLI uses `memory.db` (new, `last_accessed_at TEXT`) directly — daemon uses `rlm_memory.db` (legacy, `accessed_at INTEGER`)
- **E2E**: 8/8 PASS (write_and_read, aaak_markers, topic_filter, last_n, meta_after_write, write_exit_code, multiple_entries_ordered, no_diary_status)

### P4.2 — Palace Hierarchy Memory

`crates/touring-learning/src/memory/rlm.rs` (lines 179-191) + `crates/touring-server/src/memory_store.rs`

- **Schema**: `ALTER TABLE memory_entries ADD COLUMN palace_path TEXT` (idempotent migration)
- **Palace Path**: `wing_{name}/room_{name}/closet_{name}/drawer_{name}` (4-level hierarchy)
- **API**: `store_with_palace()`, `query_by_palace()`
- **Index**: `idx_memory_palace_path ON memory_entries(palace_path) WHERE palace_path IS NOT NULL`

### P4.3 — Evolution Drift Self-Correction

`crates/touring-hooks/src/cli_handlers_evolution.rs`

| Alert Level | Trigger | Action |
|------------|---------|--------|
| `none` | No degradation | — |
| `degraded` | bash_success < 0.8 OR edit spike | `inject_reward("evolution:drift_detected", severity)` |
| `structural` | 3+ metrics degrading | `tracing::warn!` + RL injection |

- **Self-correction**: `runtime.learning.inject_reward("evolution:drift_detected", drift_severity, "evolution_drift")`
- **Output**: `{detected, alert_level, self_correction_applied, degrading_metrics, summary}`

### P4.4 — AutoSaveHook

`crates/touring-hooks/src/auto_save_hook.rs` + `post_tool_rl.rs:177`

- **Config**: `exchange_count` (current) + `interval` (default: 15) + `last_save_ts`
- **Wiring**: `post_tool_rl.rs:177` — `increment_exchange()` → `run_auto_save()` every `interval` exchanges
- **Checkpoint format**: `{session_id, timestamp, exchange_count, state_snapshot}`

## Cognitive Architecture Upgrades (2026-04-09)

### Tarjan SCC Cycle Detection

`touring-ast/src/call_graph.rs` — `CallGraph::detect_cycles()` uses `petgraph::algo::tarjan_scc`
in O(|V|+|E|) to detect mutual recursion and self-loops in LLM-generated code before it reaches disk.

### 6-Layer Speculative Validation

`touring-ast/src/speculate.rs` — `speculate_v2()` now validates 6 layers:

| Layer | Weight | Purpose |
|-------|--------|---------|
| Syntax | 0.35 | tree-sitter parse errors |
| SymbolResolution | 0.20 | referenced symbols exist |
| Structural | 0.20 | anti-patterns (unwrap, todo!, bare except) |
| Import | 0.10 | import completeness |
| **Complexity** | **0.15** | **CC > 15 per function penalizes score** |
| CfgImpact | info | feature-gate blast radius (informational) |

### GoT Snapshot Persistence in pre-compact

`touring-hooks/src/lifecycle.rs` — `handle_pre_compact` persists GoT snapshot via
`GoTSnapshotStore` (rkyv + SQLite WAL) before context compaction, preserving reasoning
state that would otherwise be lost during summarization.

### SessionBus — Typed Inter-Hook Communication

`touring-hooks/src/shared/session_bus.rs` — Bidirectional channel replacing ad-hoc
`result_cache["__meta__"]` keys. Signals: `signal_file_read`, `cache_blast_radius`,
`signal_plan_active`, `signal_tool_outcome`, `update_arm_effectiveness`.

### Priority Queue Context Output

`touring-cortex/src/context.rs` — `emit_scored(score, line)` stores scored fragments;
`merge_and_rank()` sorts descending by score before budget truncation, replacing FIFO ordering.

### File Digest Signal

`touring-hooks/src/precomputed_signals.rs` — `file_digest_signal(source, lang)` produces
compact AST summary: `digest(NL, M symbols, CC avg=X max=Y, hot: fn1/fn2)`.

### HNSW Working Memory Activated

`touring-server/Cargo.toml` enables `hnsw-working-memory` on `touring-learning`,
activating `instant-distance` + `bumpalo` arena allocation for in-memory ANN recall.

## Dual-Mode Operation

| Mode | Binary | Latency | When |
|------|--------|---------|------|
| **Daemon** (default) | `touring-hook` → `touring-daemon` (Unix socket) | P50=1ms | Normal operation |
| **Standalone** (fallback) | `touring-hook` runs HookRuntime directly | ~15ms | Daemon unavailable |
| **MCP Server** | `touring` binary via stdio | N/A | Claude Code tool calls |

Circuit breaker: 3 failures in 60s → skip daemon for 60s → auto-recover.

## Concurrency Model

- **Daemon**: Actor pattern (one OS thread per project, `mpsc::Sender<ProjectCommand>` + `oneshot::Sender<String>`). Requests to different projects run in parallel; same-project requests serialize inside the actor loop. `Arc<Mutex<HookRuntime>>` removed 2026-04-12 to eliminate kernel-Mutex contention under hook storms (see §L7-B).
- **Hooks**: Single-threaded per invocation. `rayon` thread pool for parallel AST analysis within a hook.
- **RL Engine**: QTable in-memory cache with batch LinUCB save every 10 updates.
- **ANN Memory**: `RefCell` interior mutability (single-threaded hook context).


## GPU Optimization Wave (2026-04-20)

Quatro vetores de otimização GPU para NVIDIA RTX 4060 Laptop (8GB VRAM).

### Vector A — WGSL U4 Dequantization [touring-simd]
- **`U4_DOT_SHADER`**: compute shader WGSL para dot product quantized U4
- **`REDUCE_SHADER`**: compute shader para reduction (all-reduce on GPU, no CPU copy-back)
- **`compute_dot_u4(input, weights, scale) -> Result<f32>`**: GPU dot product for quantized inference
- Staging buffer pattern: `STORAGE | COPY_SRC` → `copy_buffer_to_buffer` → `COPY_DST | MAP_READ`

### Vector B — Zero-Copy rkyv IPC [touring-core]
- **`RkyvGpuBackend`**: reqwest client wrapping rkyv serialization
- **`IpcEmbedRequest` / `IpcEmbedResponse`**: rkyv archived, zero-copy
- Feature gate: `ipc-embed` (default off, opt-in)

### Vector C — LinUCB GPU Offload [touring-learning]
- **`LINUCB_UCB_SHADER`**: WGSL compute shader for UCB computation (8 arms × 25 dims)
- **`predict_ucb_gpu(arms, features) -> Vec<f32>`**: GPU batch prediction
- **`update_gpu(context, reward)`**: GPU reward update
- Shader bindings: `@binding(0)` features, `@binding(1)` A_inv, `@binding(2)` b_vec, `@binding(3)` ucb_scores

### Vector D — MCTS GPU Rollouts [touring-cognitive]
- **`MCTS_ROLLOUT_SHADER`**: WGSL parallel frontier evaluation
- **`PheromoneMCTS::rollout_gpu(frontier, depth)`**: GPU dispatch real via wgpu 0.26, rayon fallback
- **`PheromoneMCTS::search_gpu()`**: novo método público com GPU batch rollout
- Shader: each workitem processes 1 frontier node: `score = Σ_{d=0}^{depth-1} pheromone_strength * d`
- Orphan rule workaround: local extension via `impl GpuResources` + `include_str!` inline shader

### WGSL Language Constraints (all fixed)

| Constraint | Fix |
|-----------|-----|
| `u8` not a WGSL type | `array<i32>` with bitcast |
| `meta` reserved keyword | `dequant_meta` |
| Ternary `? :` not supported | `select(a, b, cond)` |
| `if` expression not supported | `var` + `if/else` block |
| `var x = 32` type inference | `var x: u32 = 32` |
| `stride / 2` type mismatch | `stride >> 1` |

### Key Lessons

1. GPU reduction originally on CPU — fixed to stay on GPU
2. `GpuBackend` trait doesn't expose wgpu types — touring-cognitive uses direct wgpu
3. `MAP_READ` can only combine with `COPY_DST` — staging buffer pattern required
4. Orphan rule: `impl touring_simd::gpu::GpuResources` in touring-cognitive blocked — solved with local extension

---

## Predictive Wave (2026-04-20)

Quatro deliverables (D2–D5) que transformam hooks `PreToolUse`/`PostToolUse` de observadores
passivos em participantes ativos no ciclo de raciocínio do Claude Code.

```
Claude Code ──► PreToolUse[TaskCreate|TaskUpdate]  ──► [D2 Blast Injection]   ──► ContextWithUpdatedInput
             ──► PostToolUse[TaskList]              ──► [D3 LinUCB Routing]    ──► delegation hint (additionalContext)
             ──► PreToolUse[EnterPlanMode]          ──► [D4 MCTS Shadow]       ──► deadlock avoidance hint
                                                           ↓
                                          [D5 GateMetrics] — 9 AtomicU64 counters
                                          exposed via `touring gate-metrics -j`
```

### D2 — Predictive Blast Injection at PreToolUse[Task*]

Arquivo: `crates/touring-hooks/src/pre_tool_use.rs:245`

Quando Claude Code cria ou atualiza uma task (`TaskCreate`/`TaskUpdate`), o hook extrai
símbolos do subject e calcula blast radius via `BlastRadiusEngine::compute_with_timeout`
(40ms budget). Se o blast cruza > 3 módulos, o input da task é aumentado com contexto
de impacto via `HookResponse::ContextWithUpdatedInput`.

| Símbolo | Tipo | Linha |
|---------|------|-------|
| `compute_predictive_blast_injection` | `fn` (private) | `pre_tool_use.rs:245` |
| `scan_blast_modules` | `fn` (private) | `pre_tool_use.rs:289` |
| `accumulate_blast_modules` | `fn` (private) | `pre_tool_use.rs:325` |
| `blast_via_engine` | `fn` (private) | `pre_tool_use.rs:365` (wired 2026-04-20) |
| `BlastRadiusEngine::compute_with_timeout` | `pub fn` | `blast_radius/mod.rs:248` |

### D3 — LinUCB Routing Hint at PostToolUse[TaskList]

Arquivo: `crates/touring-hooks/src/lifecycle/task_list.rs:185`

Após cada TaskList, extrai 25 features da tarefa pendente (CILA level, subject length,
keyword signals, symbol count, etc.) e consulta o `LinUCBBandit` para recomendar um
dos 8 arms de delegação. Confidence margin threshold: 0.15.

| Arm | Significado |
|-----|-------------|
| `ManualEdit` | Edição direta pelo operador |
| `GeneratorStruct` / `GeneratorEnum` / `GeneratorFn` / `GeneratorTrait` | touring-generator |
| `DelegateAgent` | Subagent especializado |
| `SplitTask` | Decomposição em subtasks |
| `DeferTask` | Adiamento por dependência |

### D4 — MCTS Shadow Rollouts at PreToolUse[EnterPlanMode]

Arquivo: `crates/touring-hooks/src/lifecycle/plan_mode/enter.rs:388`

Gate: `cila_level >= L3`. Ao entrar em plan mode, executa `run_shadow_rollout` em
thread separada (12s budget, 200ms join_timeout no handler). Detecta ciclos de deadlock
via heurística **(MVP — Tarjan SCC completo via `petgraph::algo::tarjan_scc`: TODO)** e emite
hint `[TOURING MCTS-SYNTHESIS]` se deadlock previsto.

| Símbolo | Tipo | Arquivo |
|--------|------|--------|
| `ShadowRolloutResult` | `pub struct` | `shared/shadow_rollout.rs:42` |
| `run_shadow_rollout` | `pub fn` | `shared/shadow_rollout.rs:145` |
| `mcts_shadow_rollout_hint` | `pub(crate) fn` | `plan_mode/enter.rs:388` |
| `PheromoneMCTS` | `pub struct` | `touring-cognitive/mcts.rs:649` |

`PheromoneMCTS` é o struct MCTS com pheromone layer (IC-1). `CognitiveMCTS` é type alias
para `GraphInformedMCTS` (COG-1+S6) em `cognitive_mcts.rs:170` — sistemas distintos.

### D5 — Predictive Observability Counters

Arquivo: `crates/touring-hooks/src/shared/gate_metrics.rs`

9 novos `AtomicU64` adicionados a `GateMetrics` + 9 `record_*` helpers + 9 campos em
`GateMetricsSnapshot`. Expostos via `touring gate-metrics -j` e `touring status -j`.

| Família | Contadores |
|---------|-----------|
| Blast | `blast_inject_count`, `blast_timeout_count`, `blast_mutation_count` |
| LinUCB | `linucb_route_manual_count`, `linucb_route_generator_count`, `linucb_route_hint_count` |
| MCTS | `mcts_shadow_run_count`, `mcts_shadow_timeout_count`, `mcts_shadow_deadlock_detected_count` |

**Test coverage**: 47 testes unitários + 9 P99 latency guards adicionados na Predictive Wave.
**Referência completa**: `crates/touring-hooks/touring-hooks-ARCHITECTURE.md` — Seções §D2–§D5.

---

## StringZilla Performance Layer (2026-04-25)

SIMD-accelerated string search, matching, and hashing integrated across 4 crates via
`stringzilla` v4.6.0 + `memchr::memmem`. 8 hotspot optimizations eliminating sequential
`.contains()` loops, `Regex::new()` hot paths, and SQL `LIKE '%...%'` leading wildcards.
46 new E2E tests (13+10+13+10) across 4 crates — all PASS.

### Hotspot Optimizations

| ID | Crate | Change | Speedup |
|----|-------|--------|---------|
| T0.1 | `touring-antt` | AhoCorasick `ANTT_PATTERNS`/`TECHNICAL_KEYWORDS` in `reranker.rs:get_authority()` + `compute_keyword_match()` — replaces 8× sequential `.contains()` | ~8× |
| T0.2 | `touring-hooks` | `StaticPrefixPattern` type in `pre_tool_validator.rs` — 29/30+ dangerous command patterns migrated from `Regex::new` to O(m) `starts_with` | ~15× |
| T0.3 | `touring-hooks` | `memmem::Finder` + `OnceLock<Vec<String>>` cache in `gotcha_count_for_file` — eliminates SQL `LIKE '%'` leading wildcard | eliminates full-scan |
| T1.1 | `touring-analysis` | `RangeUtf8NewlineSplits` (10+ GB/s) in `estimate_lloc()` — replaces `str.lines()` | ~3-5× |
| T1.3 | `touring-analysis` | `fast_content_hash` module — `stringzilla::hash` (AES-NI) as blake3 pre-filter in `quick_content_changed()` — skips blake3 for 90%+ unchanged files | pre-filter |
| T2.1 | `touring-generator` | BK-tree O(log N) + `sz_edit_distance` in `BkTreeFuzzyAdapter` — replaces Vec brute-force O(N×m×n) | ~2125× |
| T3.1 | `touring-hooks` | `utf8_case_insensitive_find` in `cli_index_find` — adds `--ignore-case` flag | new feature |
| T3.3 | `touring-hooks` | `SKILL_PATTERNS: LazyLock<AhoCorasick>` with 18 routing patterns in `cli_suggest_skill` | ~18× routing |

### New Modules

| Module | Location | Purpose |
|--------|----------|---------|
| `fast_hash.rs` | `touring-analysis/src/quality/fast_hash.rs` | `fast_content_hash` — stringzilla AES-NI pre-filter for `quick_content_changed` |
| `StaticPrefixPattern` | `touring-hooks/src/pre_tool_validator.rs` | O(m) prefix-based command validation type; complement to `DangerousPattern` |

### Key Invariants

- **`fast_content_hash` is NOT cryptographic**: uses `stringzilla::hash` (AES-NI/CRC32) as
  pre-filter only; blake3 remains the authoritative content hash
- **`sz_edit_distance` is feature-gated**: `BkTreeFuzzyAdapter::top_k()` requires `simd-fuzzy` feature
- **BK-tree is lazy-seeded**: first `top_k()` call seeds from symbol pool when tree is empty
- **OnceLock gotcha cache**: `gotcha_count_for_file` pre-loads patterns once per daemon lifetime

### Hook Registry Fix (pre-existing bug, fixed in this wave)

`ALL_DAEMON_HOOK_NAMES` constant had 169 entries while `all_daemon_hook_names()` returned 171.
Root cause: Wave B (2026-04-24) added `cli-workflow-resume` and `cli-workflow-status` to the
function but not the constant. Fixed: constant updated to 171 entries; 3 test assertions updated.
Wave A.2/A.3/A.4 later expanded to 182 entries (StringZilla wave + additional workflow hooks).

### E2E Test Coverage

| Test File | Crate | Tests | Focus |
|-----------|-------|-------|-------|
| `tests/stringzilla_e2e.rs` | `touring-hooks` | 13 | StaticPrefixPattern, AhoCorasick routing, memmem gotcha, registry sync |
| `tests/reranker_e2e.rs` | `touring-antt` | 10 | `get_authority` patterns, `compute_keyword_match`, E2E ranking |
| `tests/stringzilla_quality_e2e.rs` | `touring-analysis` | 13 | RangeUtf8NewlineSplits vs stdlib semantics, fast_content_hash |
| `tests/bktree_e2e.rs` | `touring-generator` | 10 | BkTree insert/query/top_k, confidence, reseed, lazy-seed |
| **Total** | 4 crates | **46** | All PASS |

**Session report**: `docs/2026-04-25-stringzilla-wave-complete.md`

---

## Fascículo Arqueado — Semantic Routing (completed 2026-04-11)

Inspired by the Arcuate Fasciculus in human neuroscience — a bidirectional white-matter tract
enabling ultrafast integration between language perception and motor planning areas.

### 4 Express Routes (FA paths)
- **FA-1**: Perception→Execution — `last_read_file`, `blast_radius_cache` (SessionBus)
- **FA-2**: Planning→Execution — `active_plan_hint` (SessionBus)
- **FA-3**: Execution→Learning — `last_tool_accepted`, `last_quality_score` (SessionBus, `post_tool_rl.rs:241`)
- **FA-4**: Learning→Perception — `arm_effectiveness` HashMap (SessionBus, `AcoRewardPropagator`)

### IC-1↔IC-4 Pheromone Loop (closed 2026-04-11)
- **IC-1**: `CognitiveMCTS` deposits pheromone trail based on planning outcomes
- **IC-4**: `AcoRewardPropagator` propagates RL rewards backward via TD(λ) into MCTS pheromone
- **Closed via**: `AcoWiringState::process_tracker_report()` calls `propagate_from_report()` after every `TrackerReport`
- **Shared state**: `MctsPheromonoLayer` shared via `Arc<Mutex<>>` between `CognitiveMCTS` and `AcoRewardPropagator`
- **Propagation chain**: `TrackerReport.as_rl_reward()` → `propagate_from_report(report, history)` → TD(λ) backward trace → `MctsPheromonoLayer.deposit_rl_signal()` → MCTS pheromone updated
- **Init params**: `MctsPheromonoLayer::new(alpha=0.1, evap=0.0)`, `AcoRewardPropagator::new(mcts_pheromone, lambda=0.8, gamma=0.99)`

### FascicleDispatcher — O(1) Semantic Routing
- **Location**: `touring-cortex::FascicleDispatcher` — HashMap-based O(1) dispatch vs O(n) pipeline scan
- **Bridge to touring-hooks**: `subscribe_fascicle_to_touring_hooks_bus()` in `CortexRuntime` (`runtime.rs`)
- **Pattern**: Clone `SyncSender` from `fd.dispatch_channel().sender()` once, move into async task
- **Thread safety**: `Arc<FascicleDispatcher>` is `!Send`; only `SyncSender<Evidence>` crosses thread boundary
- **Fixed (2026-04-11)**: Previous implementation created throwaway `sync_channel(1)` per event (complete data-loss)

### AgenticRL arm_effectiveness Fix (2026-04-11)
- **File**: `touring-hooks/src/post_tool_rl.rs`
- **Bug**: `bus.arm_effectiveness.get(&255).copied()` — hardcoded invalid arm ID 255
- **Fix**: `bus.last_arm_selected.and_then(|arm_id| bus.arm_effectiveness.get(&arm_id).copied()).or_else(|| bus.arm_effectiveness.values().copied().reduce(f64::max))`
- **Result**: PPO learning phase receives real arm effectiveness (valid IDs 0-7)

### LinUCB Warmup Cold-Start Fix (2026-04-11)
- **File**: `crates/touring-server/src/cli/e2e.rs` (`cli_e2e.rs`)
- **Root cause**: `inject_warmup_reward()` uses a LOCAL throw-away `LinUCBBandit`; the real shared bandit `total_pulls` stays 0
- **Fix**: Gate changed to check `OnlineRLEngine.update_count() > 0` (IS incremented by warmup) instead of `linucb.total_pulls() == 0`
- **Invariant**: To detect if LinUCB warmup ran: use `update_count() > 0`, NOT `total_pulls() > 0`

## Quality Gates

```bash
cargo check --workspace                                    # 0 errors
cargo clippy --workspace -- -D warnings                    # 0 warnings
cargo test --workspace --exclude touring-python            # ~13,964 test fns, 0 failed (touring-python excluded: pyo3 linking)
touring doctor -j | jq '.[] | select(.status != "ok")'     # all OK
```

Note: `touring-server` lib tests now compile clean — `cargo check -p touring-server --tests` returns **0 errors** (verified in loco 2026-06-04; the previously documented "122 errors" are stale/resolved). `touring-python` is excluded from the default test run **by design** (pyo3 linking, not compilation errors). Core workspace test functions: **13,272** annotations (`#[test]`/`#[tokio::test]`/`#[rstest]`).

## Invariants

1. **Exit 0 always** — `touring-hook` never blocks Claude Code
2. **Clippy deny-all** — 0 warnings = compile error
3. **No production unwrap()** — use `?`, `.expect()`, `.unwrap_or_default()`
4. **SCHEMA_VERSION gate** — increment on DDL changes
5. **Canonical paths** — all DB access via `TouringConfig::*_canonical()`
6. **Feature-gated u4** — `#[cfg(feature = "u4-quantization")]` for quantization paths

## Per-Crate Architecture Docs

The authoritative crate inventory (name · src LOC · %) is generated under [Workspace Topology](#workspace-topology) by `docs/sync_metrics.py --sync` and gated by `docs/sync_metrics.py --check`. Each crate carries its own `crates/<name>/ARCHITECTURE.md` with type-level detail.
