# Touring — Nervous System for Claude Code

> Universal rules. Apply to ALL projects in all working directories.

## Hook Architecture Documentation

- **Full reference**: `crates/touring-hooks/HOOK-ARCHITECTURE.md` — complete architecture, data flows, CILA budgets, BM25 integration points, end-to-end example
- **Quick reference**: `~/.claude/rules/touring-hooks-architecture.md` — 1-page hook→tool mapping, shared infrastructure, invariants
- **Source**: `crates/touring-hooks/src/` — 8 hook files + `shared/` (7 modules)

## What Touring Is

A Rust-native intelligence layer (`~/.claude/rust/`, 13 crates) that runs as Claude Code hooks.
Binaries: `~/.claude/hooks/touring-hook` (~11MB, thin client) + `touring-daemon` (~10MB, persistent server).
Latency: **<2ms warm** (daemon running) / **~15-20ms cold start** (daemon auto-started once per session).
Storage: **3 consolidated SQLite DBs** per project (`<project>/.claude/touring/{knowledge,memory,graph}.db`, `SCHEMA_VERSION=8`).
**v30.3.5** (2026-04-30): 84 cortex handlers (H1-H84), 68 daemon hooks, **198 hook handlers** (was 153, +12 StringZilla + Wave A.2/A.3/A.4 hooks + 3 cross-audit fixes), 5,100+ tests (touring-server/touring-python excluded with known test errors), clippy 0 warnings, 14 AST languages, **99 MCP tools**. **StringZilla Performance Wave (2026-04-25)**: SIMD-accelerated string operations (Zeta strings, 12 algorithms including split, join, replace, regex) integrated into touring-ast via `stringzilla` crate. **Wave A.2/A.3/A.4 hooks**: SkipContext W-115 (post-edit idempotency guard), ProfileAggregator Default impl fix, profile_query MCP tool, 9 skip-region tests (Rust/JS/TS/Python + edge cases), Q-220 idempotency diagnostic downgrade. **PLN2 Distributed Saga Coordinator**: DistributedSagaCoordinator (2PC), SagaAgent trait, 7 CLI handlers, SAGA[4] zero-copy IPC, 12 E2E tests. **DB Consolidation**: 8 legacy DBs → 3 domain DBs (knowledge, memory, graph). **U4 Quantization**: 8× embedding compression enabled in production. **GPU Optimization Wave (2026-04-20)**: WGSL U4 dequantization + GPU reduction (touring-simd), rkyv zero-copy IPC embedding (touring-core), LinUCB GPU UCB computation (touring-learning), MCTS GPU rollouts via wgpu 0.26 (touring-cognitive). NVIDIA RTX 4060 Laptop (8GB VRAM) verified. **ANN memory persistence** fully wired: `pre_read` searches + `post_edit` stores via `RefCell<Option<PersistedAnnMemoryRecall>>`. **Functional Wiring v7**: purpose-based chains (Sequential/Complementary/Hierarchical/Broken). **settings.json**: 29 hooks across 15 events fully configured. **HookResponse v30**: 6 variants (Context, Deny, Block, Halt, ContextWithUpdatedInput, context truncation at 9,500 chars). **Predictive Wave (D2-D5)**: Blast injection, LinUCB routing hints, MCTS shadow rollouts, 9 gate metrics counters. **Telemetry**: async NDJSON event logging via `telemetry_logger.sh`. **CLAUDE_ENV_FILE**: `session_env_setup.sh` persists TOURING_* env vars across sessions.

## What Happens Automatically (You Don't Control This)

| Event | Hook | What It Does |
|-------|------|-------------|
| **Before Read** | `pre-read` | Injects gotchas, past failures, and dependent count for the file. Silence if nothing actionable. |
| **Before Bash** | `pre-bash` | Recalls relevant past command failures (same command + same file/dir). Silence if no history. |
| **Before Edit** | `pre-edit` | Scored signal injection (v29): blast radius, external callers, complexity delta, call graph impact, scope shadowing, ModuleTree re-exports, cognitive enrichment. CILA-aware budgets (L0-L1=1200, L2-L3=3000, L4+=6000 chars). Rayon parallel AST. |
| **Before Write** | `pre-write` | Speculative validation, anti-pattern detection, import completeness, wiring prediction, quality baseline. CILA budget (v29). |
| **After Edit** | `post-edit` | Tracks edit in knowledge graph. Returns quality verification feedback (v29): speculative validation, anti-patterns, complexity, wiring signals. Multi-language (Rust, Python, TS, JS, Go, C/C++, Java). |
| **After Write** | `post-write` | Quality verification + feedback. Multi-language anti-pattern detection. Wiring registration for new files (v29). |
| **After Read** | `post-read` | Records file metadata, symbols, imports into knowledge graph. |
| **After Bash** | `post-bash` | Records command outcome (exit code, error pattern) for future recall. |
| **After Any Tool** | `post-tool-rl` | Updates RL models (QTable + LinUCB) with reward signal from tool outcome. |
| **Prompt Submit** | `prompt-enhance` | Classifies intent and injects reasoning techniques (chain-of-thought, structured output, etc.). |
| **Session Start** | `session-start` | Reports knowledge stats. Classifies session CILA level (S6). Pre-warms result cache for top 15 accessed files (S12). Auto-starts daemon if not running. **v30**: `session_env_setup.sh` persists TOURING_* env vars via CLAUDE_ENV_FILE. |
| **Session Stop** | `session-stop` | Persists session metrics and learning updates. Updates gotcha decay scores (S8). |
| **Tool Failure** | `post-tool-failure` | Records failure in knowledge graph, auto-creates gotcha for the file. Circuit breaker: Halt after 5+ failures on same file (v30). |
| **Post Compact** | `post-compact` | Re-warms result cache for top accessed files after context compaction (v30). |
| **Instructions Loaded** | `instructions-loaded` | Injects project knowledge stats (files tracked, edits, commands, gotchas) on session init (v30). |

### Agent Teams Hooks (N1 — ACO Gateway)

| Event | Hook | What It Does |
|-------|------|-------------|
| **Teammate Idle** | `teammate-idle` | Deposits teammate productivity pheromone via ACO wiring. **Now routed through daemon** (S4a) for warm `AcoWiringState`. Fire-and-forget. |
| **Teammate Idle Gate** | `teammate-idle-gate` | Anti-limbo state machine with circuit breaker, ACO wiring. Returns `{context, exit_code}` via `TeamHookGateResult`. Python shim delegates to daemon. |
| **Task Completed** | `task-completed` | Deposits task success pheromone. **Now routed through daemon** (S4a) for warm cache. Fire-and-forget. |
| **Subagent Start** | `subagent-start` | Intelligent lifecycle handler (S4): injects project context snapshot for the subagent. |
| **Subagent Bootstrap** | `subagent-bootstrap` | 3-line minimal bootstrap context via `subagent_bootstrap_context()`. Transcript-aware state machine via `build_minimal_context()`. |
| **Subagent Stop** | `subagent-stop` | Records subagent outcome for learning. **Gate v4** (v29.1): transcript-based detection — only blocks genuine Agent Teams teammates (requires real `TaskUpdate` tool_use call, not text mention). Pure TACO subagents always allowed. |

**`touring serve` runtime** (v30.3.3, 2026-04-21): The MCP stdio server no longer uses `#[tokio::main]` — `main.rs` builds the rayon global pool first, then an explicit tokio multi-thread runtime driven by three env vars for zero-recompile tunability:

| Env var | Default | Controls |
|---|---|---|
| `TOURING_MCP_WORKERS` | `num_cpus::get_physical()` | Tokio worker threads (physical cores, not SMT logical — CPU-bound AST+SIMD benefits from 1 worker per physical core because SMT siblings compete for L1/L2 cache) |
| `TOURING_BLOCKING_WORKERS` | 512 | `spawn_blocking` pool cap (SQLite, Tantivy, python3 bootstrap) |
| `TOURING_RAYON_THREADS` | `physical / 2` | Rayon global pool (pre_edit signals, quality analysis) — isolated from tokio workers to prevent cross-pool starvation |

Thread stack raised to 4 MiB for AST recursion. Thread names are
`touring-mcp-worker-*` and `touring-rayon-*` (visible in tokio-console).
Fix log: `streaming_mcts_search` no longer pins 1 core — see
`docs/2026-04-21-touring-serve-multicore-scaling.md`.

**Daemon architecture** (v30.3.2): `touring-hook` is a thin client — it sends the request to the persistent `touring-daemon` via Unix socket (`/tmp/touring-daemon-{uid}.sock`). The daemon is **multi-threaded** with fine-grained per-project locking (`Arc<Mutex<HookRuntime>>` per project) — requests to different projects execute in parallel. A **circuit breaker** (file-based `/tmp/touring-circuit-{uid}.state`) skips the daemon after 3 failures in 60s, reducing fallback latency from 3.1s to <1ms. On daemon unavailability, `touring-hook` falls back to standalone mode (exit 0 always preserved). **Health check**: `touring-hook --daemon-health` returns JSON status. **Wiring Intelligence** (v28): 6-layer system (Signal→Tracker→Cascade→RL→Cortex→Feedback) with SCHEMA_VERSION=7, **50 wired_pairs** (W9 Synergy deepening) (`wiring_map` + `module_ecosystem` + `functional_signatures` + `functional_chains` tables). **Edit/Write Excellence** (v29): scored signals, CILA budgets, post-edit feedback loop, multi-language anti-patterns. **PLN2 Distributed Saga Coordinator** (v30.3.2): `DistributedSagaCoordinator` for multi-agent 2PC coordination via Unix socket with `SAGA[4]` magic framing and rkyv zero-copy serialization. **7 saga hook handlers**: `cli-saga-register`, `cli-saga-prepare`, `cli-saga-decide`, `cli-saga-delta`, `cli-saga-begin`, `cli-saga-status`, `cli-saga-abort`. Hook registry count: **198** (was 138, +7 saga). **v30 Enhancement**: 68 daemon hooks (+9), 6 HookResponse variants (Deny/Block/Halt/ContextWithUpdatedInput + context truncation at 9,500 chars).

**Action**: When hooks inject context (shown as `additionalContext`), USE it. It's grounded in real data, not inference. Ignoring good signals degrades future RL suggestions.

## Knowledge Graph (Per-Project SQLite)

Accumulated automatically from every Read, Edit, Bash:

| Data | Source | Use |
|------|--------|-----|
| File metadata | post-read | Language, line count, content hash |
| Symbols | post-read/post-edit | Function/class names per file |
| Imports/relations | post-read | Who depends on whom |
| Bash outcomes | post-bash | Command success/failure history with error patterns |
| Edit history | post-edit | What was changed, when, success/failure |
| Gotchas | accumulated | File-specific pitfalls from past errors |

### PLN2 — Extended Knowledge Tables (v30+)

11 additional tables wired via `reindex_file()` (shared/reindex.rs) on every edit/write:

| Table | Wired by | Purpose |
|-------|----------|---------|
| `file_feature_flags` | `upsert_feature_flags_batch` | Feature flags from Cargo.toml, pyproject.toml, package.json |
| `file_todos` | `insert_todo` | TODO/FIXME/XXX extracted from content |
| `edge_confidence` | `upsert_edge_confidence` | Confidence scores for import graph edges |
| `file_communities` | `upsert_file_community` | Louvain community assignments |
| `file_test_coverage` | `upsert_test_coverage` | Test coverage percentage per file |
| `file_blake3_registry` | `upsert_blake3_registry` | BLAKE3 content hash + symbol count |
| `session_file_summary` | `upsert_session_file_summary` | Session-file skeleton summaries |
| `symbol_events_log` | `insert_symbol_event` | Symbol-level event log (create/modify/delete) |
| `wiring_suggestions` | `upsert_wiring_suggestion` | Orphan symbol wiring recommendations |
| `metadata_benchmark_runs` | `insert_benchmark_run` | Benchmark results (commit_hash, p50/p95/p99) |
| `cognitive_enrichment` | `upsert_cognitive_enrichment` | Cognitive scores (complexity, fan-in, fan-out, doc) |

**Schema constants**: All 11 table names defined in `touring-analysis/src/e2e/schema_guard.rs:57-87` (single source of truth).

**E2E tests**: `cargo test -p touring-hooks --test pln2_e2e` — 23 tests covering all 11 tables.

## CILA Levels (Automatic Intent Classification)

Prompts are classified into complexity levels. You receive this as context:

| Level | Name | Meaning |
|-------|------|---------|
| L0 | Direct | Pure text response |
| L1 | PAL | Simple computation |
| L2 | Tool-Augmented | Search + MCP tools |
| L3 | Pipelines | Multi-step scripted workflow |
| L4 | Agent Loops | Iterative analysis with feedback |
| L5 | Self-Modifying | Parameter/config evolution |
| L6 | Multi-Agent | Parallel agent teams |

## Reinforcement Learning

Every tool use generates a reward signal. Over time, Touring learns:
- Which tools work best for which file types
- Which commands tend to fail in which contexts
- Optimal tool sequences for common task patterns

The RL engine (QTable + LinUCB bandit) runs at <1ms and silently improves context injection quality.

## Cognitive Layer

Available but not always active:
- **MCTS**: Monte Carlo Tree Search for multi-step planning
- **GoT**: Graph of Thought for complex reasoning (parallel node evaluation via `JoinSet`)
- **GoT Snapshots**: rkyv zero-copy serialization + deadpool-sqlite async persistence
- **Session Predictor**: Predicts next likely tool use
- **Coedit Predictor**: Predicts which files will be co-edited
- **Focus Cache**: Tracks attention across files in session

## S0-S8 Capabilities (v22.0.0)

Features added in sprints S0-S8 (2,671 → 2,840 tests):
- **BranchFs**: Copy-on-write file snapshots for safe edits (touring-hooks)
- **InferletPool**: Async pool of pre-compiled WASM modules with pooling allocator (touring-wasm)
- **TypedEvaluate**: Structured parameters and scored results for WASM plugins (touring-wasm)
- **ReminderBandit**: LinUCB 6-dim contextual bandit for adaptive reminder injection (touring-learning)
- **DriftMonitor**: Kolmogorov-Smirnov two-sample drift detection (touring-learning)
- **CrdtDelta**: Delta-based merge for CRDT semantic graphs (touring-learning)
- **RRF + CallGraph**: Reciprocal Rank Fusion and petgraph-backed call graph analysis (touring-cortex)
- **Tarjan SCC**: Cycle detection + topological ordering in dependency cache (touring-hooks)
- **EbpfObserver**: Feature-gated stub for kernel-level metrics (touring-learning)

## v25.0.0 Excellence Sprint (2,840 → 3,040 tests, SCHEMA_VERSION 4→5)

15 strategies implemented across 3 sprints:

### Sprint 1 — Quick Excellence
- **S4a**: `teammate-idle`/`task-completed` routed through daemon (warm ACO `AcoWiringState`)
- **S14**: File-based circuit breaker for IPC (`/tmp/touring-circuit-{uid}.state`). 3 failures in 60s → skip daemon for 60s. Fallback latency: 3.1s → <1ms. **TTL reset on graceful shutdown** — circuit state cleared on SIGTERM/SIGINT and idle-timeout shutdown (v25.1).
- **S5**: Token budget + relevance ranking in context injection (`DEFAULT_CONTEXT_BUDGET=3200` chars, scored by `recency × severity_weight`). **`TOURING_CILA_BUDGET_L0/L2/L4` env vars** for configurable override (v25.1).
- **S13**: Graceful daemon shutdown: WAL checkpoint + LinUCB rkyv flush + CRDT flush + circuit reset before `exit(0)`
- **S9**: Health check endpoint: `touring-hook --daemon-health` → `{"status":"healthy","projects_loaded":N,"version":"..."}`
- **S11**: Structured per-hook metrics: `HookEventMetrics` with 5 `AtomicU64` counters (invocations, latency, bytes, cache_hits, fallbacks)

### Sprint 2 — Intelligence Upgrade
- **S1**: Dispatch table refactor: `OnceLock<HashMap<&str, HookHandler>>` replaces CC=29 match. O(1) lookup
- **S4**: Intelligent lifecycle hooks: `file-changed` invalidates result_cache; `pre-compact` flushes rkyv snapshots + WAL; `cwd-changed`/`worktree-create` record access
- **S6**: CILA-aware context injection: session-start stores `__session_cila_level__`; pre-read adapts budget (L0-L1=800, L2-L3=2000, L4+=4000 chars)
- **S7**: Context-utility feedback loop: `context_injection_file` tracked in `HookRuntime`; `post-tool-rl` correlates context with outcome for RL reward
- **S8**: Staleness decay for gotchas: `SCHEMA_VERSION=5`, columns `decay_score`, `last_occurrence`, `resolved_at`. Auto-resolve after 5 successful edits

### Sprint 3 — Architecture Upgrade
- **S2**: HookRuntime god object decomposed into `ContextRuntime` (knowledge, classifier, pii, cache), `LearningRuntime` (linucb, bandit, online_rl, predictor, crdt), `InfraRuntime` (symbols, pipeline, deps). Access: `rt.ctx.knowledge`, `rt.learning.linucb`, `rt.infra.symbol_store`. 21 files migrated.
- **S3**: Multi-threaded daemon: `RuntimeMap = Arc<Mutex<HashMap<PathBuf, Arc<Mutex<HookRuntime>>>>>`. Per-project locking — requests to different projects execute in parallel
- **S10**: Centralized hook registry: `hook_registry.rs` with `ALL_DAEMON_HOOK_NAMES` (21 hooks) + `build_dispatch_table()`. Single source of truth — adding a hook = 1 line
- **S12**: Session-start cache pre-warming: `prewarm_result_cache()` pre-computes context for top 15 accessed files via `top_accessed_files()` query

### Lifecycle Hooks (new intelligent handlers via S4)

| Event | Hook | What It Does (v25) |
|-------|------|-------------|
| **File Changed** | `file-changed` | Invalidates `result_cache` for the changed file (forces fresh context on next pre-read). **v28**: also triggers Wiring cascade invalidation. |
| **CWD Changed** | `cwd-changed` | Records directory switch in knowledge DB |
| **Pre-Compact** | `pre-compact` | Flushes LinUCB rkyv snapshot + WAL checkpoint before context compaction |
| **Worktree Create** | `worktree-create` | Records worktree path in knowledge DB |

## v29.0.0 Pre/Post Edit/Write Excellence (3,851 → 3,983 tests, hooks 57→59)

5 hooks enhanced/created in touring-hooks:
- **pre_edit.rs** (REWRITE — 1500+ LOC): Scored signals + CILA budget + rayon parallel + 8 new AST signals (speculative validation, blast radius, external callers, complexity delta, call graph impact, scope shadowing, ModuleTree re-exports, cognitive enrichment)
- **pre_write.rs** (NEW — 850+ LOC): PreToolUse for Write. CILA budget, speculative validation, anti-pattern detection, import completeness, wiring prediction, quality baseline
- **pre_edit_prevention.rs** (ENHANCED — 510+ LOC): +4 new signals (syntax pre-check via speculate_v2, complexity threshold, scope shadowing, structural anti-patterns)
- **post_edit.rs** (ENHANCED — 1000+ LOC): Feedback-capable via `run_returning()` returning `HookResponse` with 5 verification signals. Multi-language anti-pattern detection (Rust, Python, TypeScript, JavaScript, Go, C/C++, Java)
- **post_write.rs** (NEW — 750+ LOC): PostToolUse for Write. Quality verification + feedback. Multi-language anti-patterns. Wiring registration

### Key Architectural Innovations
- **Scored signal ranking**: `Vec<(f32, String)>` sorted by priority, budget-truncated
- **CILA-aware budgets**: L0-L1=1200, L2-L3=3000, L4+=6000 chars (50% larger than reads)
- **Parallel signal computation**: `rayon::join` + `rayon::spawn` for AST analysis
- **Post-edit feedback loop**: PostToolUse hooks return `additionalContext`, enabling edit→verify→fix cycle
- **Speculative validation**: `speculate_v2` used in all 5 hooks for syntax/structural/symbol/import checking
- **Latency**: pre-edit <50ms, post-edit <80ms

## v28.12.0 Wiring Excellence (3,761 → 3,851 tests)

### Feature Activations (5 features enabled)
- `simd-search` enabled in touring-cortex, touring-hooks, touring-server — SIMD-accelerated SemanticSymbolIndex
- `wasm-plugins` enabled by default in touring-server — WasmPluginRunner active in production
- `more-languages` enabled in touring-hooks, touring-server — Go + Java (12→14 AST languages)
- `simd-similarity` new feature in touring-index — file similarity via CosineComputer
- `smart-cache` new feature in touring-index — RL-guided cache eviction via LinUCBBandit

### New Modules (8)
- `touring-index/similarity.rs` — FileSimilarityIndex (SIMD cosine, 8-dim feature vectors)
- `touring-index/smart_cache.rs` — SmartCachePriority (LinUCB bandit for cache eviction)
- `touring-antt/financial_analysis.rs` — ConcessionAnalysis (NPV/IRR/stress via touring-simd::financial)
- `touring-hooks/reranked_context.rs` — RRF reranking for pre_read context injection
- `touring-hooks/callgraph_enrichment.rs` — CallGraph blast radius enrichment
- `touring-cortex/signal_fusion.rs` — Bayesian signal fusion (touring-simd::reconciliation)
- `touring-cognitive/semantic_graph.rs` — AnnIndex wired (rebuild_ann_index + retrieve_by_embedding_ann)
- `touring-python/{cognitive,rules,financial}_bindings.rs` — MCTS, JDM eval, NPV/IRR to Python (v4.0.0)

### Critical Fixes
- H99 MCTSCodeSynthesisHandler: CognitiveMCTS name conflict resolved (GraphInformedMCTS)
- H100 DSPyIntegrationHandler: import path, doc comment, static→function signatures
- **97 cortex handlers** (H1-H97), 57 daemon hooks, +11 cross-crate wires

## v28.9.0 Cognitive Excellence + Wiring Intelligence (3,040 → 3,735 tests, SCHEMA_VERSION 6→7)
## v29.4.0 Functional Wiring Sprint: Functional Wiring (v7): purpose-based chains — `extract_functional_signature()` from `//!` doc comments, `register_functional_signature()`, `rebuild_functional_chains()`, `functional_chain_signal()` in pre_edit (Layer 1b) and pre_write (cache-first). Chain types: Sequential, Complementary, Hierarchical, Broken. **Cache-first pattern**: `HookResultCache` (moka TinyLFU, 256 entries) consulted before DB queries.

### touring-cognitive Excellence (15 strategies S1-S15)
- **S1-S5**: TfIdf vectorizer, AdaptiveEngine runtime optimization, confidence calibration, goal decomposition, session analytics
- **S6-S10**: SqliteGraphStore persistent backend, Nexus integration hub, pattern mining, anomaly detection, priority scheduler
- **S11-S15**: Context compression, incremental indexing, semantic clustering, feedback aggregation, cognitive profiling
- **7 new modules**, +96 tests
- **Auditoria cruzada cognitive**: 3 integrations wired — TfIdf→Nexus, AdaptiveEngine→Runtime, SqliteGraphStore

### Wiring Intelligence System (6 layers, SCHEMA_VERSION=7)
- **Layer 1 — Signal**: pre-edit hooks emit Signal 6a (impact), 6b (wiring map), 6c (module ecosystem)
- **Layer 2 — Tracker**: post-edit tracks changes in `wiring_map` table (who changed what, when, outcome)
- **Layer 3 — Cascade**: file-changed triggers cascade invalidation across wiring dependencies
- **Layer 4 — RL Reward**: edit outcomes feed back into LinUCB as reward signal
- **Layer 5 — Cortex**: H83 handler exposes wiring intelligence to MCP tools
- **Layer 6 — Feedback**: closed loop — wiring quality improves with usage
- **SCHEMA_VERSION 5→6**: adds `wiring_map` + `module_ecosystem` tables
- **SCHEMA_VERSION 6→7**: adds `functional_signatures` + `functional_chains` tables
- **4 new modules**, +56 tests wiring, audit fixed 5 bugs (2 P0, 3 P1)

### Cortex cross-audit (v28)
- 7 integration gaps between touring-cognitive and touring-cortex resolved
- +30 tests for cross-crate verification
- **94 cortex handlers** (H1-H94, H93=HybridCognitiveEnricher, H94=CognitiveMetricsObserver)

## v29.1.0 Hook Wiring Sprint (3,983 → 4,009 tests)

**8 gaps between `settings.json` and `touring-hooks` corrected** — capabilities that were implemented but never wired:

| Gap | Fix | Impact |
|-----|-----|--------|
| `SessionStart` / `SessionEnd` missing | Added to `settings.json` | LinUCB warm-start, cognitive engine init, cache pre-warming now active |
| `PostToolUse[*]` missing `post-tool-rl` | Added `*` matcher | RL engine receives reward signals from ALL tool uses |
| `PostToolUse[Read]` missing `post-read` | Added `Read` matcher | File metadata/symbols written from reads, not just edits |
| `PostToolUse[Bash]` missing `post-bash` | Added `Bash` matcher | Bash outcomes learned for future recall |
| `PreToolUse[Read]` missing `pre-read` | Added `Read` matcher | Gotcha injection before file reads |
| `PreToolUse[Edit]` missing `pre-edit-prevention` | Added 2nd hook | v29 prevention layer (syntax, complexity, scope shadowing) now active |
| `PreCompact` used static `echo` | Replaced with `touring-hook pre-compact` | LinUCB rkyv flush + WAL checkpoint on context compaction |
| `rust-analyzer-lsp` disabled | Enabled | Full Rust LSP intelisense for Touring workspace edits |

**Bug fixes:**
- **daemon.rs P1** (mutex poison): watchdog `.unwrap()` → `.unwrap_or_else(|e| e.into_inner())` — prevents silent daemon crash on poisoned lock
- **prompt_enhance.rs P2** (empty iterator panic): `.unwrap()` on `min_by` → `.expect("non-empty patterns")` with guard

**SubagentStop gate v4** (`subagent_stop_gate.py`): replaces text matching (`"taskupdate" in last_msg`) with transcript-based tool_use detection. Detection logic: inspect `transcript[].content[].type=="tool_use"` for `TeamCreate`/`TaskCreate` (session type) and `TaskUpdate(status="completed")` (completion signal). Pure TACO subagents (no team infra) always pass. Eliminates false positives from explanatory text. 5/5 behavioral tests passing.

## v29.1.0–v29.4.0 TACO Audit Sprint (3,983 → 4,096 tests)

**Sprint 1 — P0/P1 Bug Fixes**
- `daemon.rs`: SIGTERM/SIGINT via `tokio::signal::ctrl_c()` + `tokio::signal::unix::signal(SignalKind::terminate())` → `graceful_shutdown()` (WAL flush + LinUCB rkyv + socket remove)
- `session_hooks.rs` P0: `emit_allow()` → `return Ok(())` — prevents daemon crash when knowledge DB stats fail
- `knowledge.rs`: `file_risk_scores` table created in `ensure_schema()` (was missing despite `increment_file_risk()` trying to use it); duplicate decompose tables removed; `stats()` 6-queries → 1 compound query
- `layer7_prediction.rs`: `Vec<String>` → `VecDeque<String>` O(n)→O(1) eviction; HashMap-based dedup; sort by confidence before `.take(k)`

**Sprint 2 — Layer7 + Performance**
- `InfraRuntime.last_edited_file: RefCell<Option<String>>` — interior mutability for co-edit tracking without `&mut HookRuntime`
- `post_edit.rs` + `post_write.rs`: Full Layer7 wiring — `record_edit()`, `record_co_edit(prev, current)`, `update_file_heat()` from ACO pheromone
- `post_tool_rl.rs`: QTable in-memory cache via take/put pattern; LinUCB batch save every 10 updates (not every call)

**Sprint 3 — Quality + Sinergia**
- `shared/` module: `detect_language`, `quality` (measure_quality_snapshot + is_test_file), `reindex` (reindex_file) — 22 callsites across 5 hooks
- `shadow_v2.rs`: `speculate_v2` fast-path before external linters (2-10s → <200ms for Rust/Python/TypeScript/JavaScript); TSC byte-safe UTF-8 parsing via `.get()` slicing
- `daemon.rs`: `HookEventMetrics` in `__health__` — per-hook `{invocations, avg_latency_ms}` (zero-invocation omitted)
- `hook_registry.rs`: `all_daemon_hook_names()` with `#[cfg(feature)]` gates alongside static `ALL_DAEMON_HOOK_NAMES`
- `post_tool_rl.rs`: `quality_score = context_utility + aco_quality_bonus (×0.1) + gotcha_bonus (×0.01, capped 0.1)`
- `decomposer.rs`: `validate_order` write-lock fix (`read().await` → `write().await`); removed unused vars; safe indexing via `.get()`
- Removed stale files: `shadow.rs`, `pii.rs.bak`

## v29.5.0 Interface Excellence Sprint (3,918 tests, 0 clippy warnings)

Focused on touring-hooks ↔ Claude Code interface quality and touring-server MCP contracts.

**P2 Bug Fix — Circuit Breaker Health Pollution**
- `main.rs`: `try_daemon_health_direct()` — connects directly to daemon socket without calling `is_open()` or `record_failure()`. Health checks are diagnostic tools; must never contribute to circuit state. Fixes cascading circuit open after 4 health-check failures even when daemon was live.

**Dead Code Activation — session_id Circuit Dimension**
- `main.rs`: session_id extracted from Claude Code hook JSON input and propagated to `DaemonRequest`. Previously hardcoded `session_id: None`, making the session-level circuit breaker dimension dead code. Now activates per-session failure isolation.

**FFI Consolidation**
- `lib.rs`: `pub(crate) fn current_uid() -> u32` — single source for `unsafe { getuid() }`. Replaces 3 duplicate `extern "C"` blocks across `main.rs`, `ipc.rs`, `circuit_breaker.rs`.

**New Hook Event**
- `settings.json`: `Setup` event wired to `touring-hook setup` (lifecycle passthrough).

**touring-server MCP Contract Fixes**
- `server/mod.rs`: Triple `tokio::fs::metadata()` syscall → single call in `file_ops` exists branch (3 I/O → 1).
- `server/mod.rs`: Supported AST language list expanded 4 → 13 (added bash, html, css, markdown, json, toml, yaml, go, java).
- `server/mod.rs`: `graph_svc.inject()` added to 4 tools that skipped the contract: `mask_context`, `mcts_search`, `incremental_status`, `online_learn`.
- `server/params.rs`: Doc comment "26 MCP tool parameter types" corrected to "28".

**touring-hooks Dispatch Refactor (CC 68→51)**
- `main.rs`: 18 identical `run_lifecycle_event()` arms collapsed into 1 via Rust `|` alternation pattern. Event name normalized at runtime (`replace('-', "_")`). Reduces CC by 17; adding new lifecycle events now requires one line.

**EvolutionAnalyzer Cache (SRV-10)**
- `touring-learning/src/ranking/wilson.rs`: `WilsonRanker` and `DriftDetector` now derive `Clone`.
- `server/mod.rs`: `insights()` reuses `self.ranker` and `self.drift_detector` via `.lock().await.clone()` instead of reopening SQLite on every call. Eliminates `LearningPersistence::new()` + 2 queries per `touring_insights` invocation.

## v29.6.0 Cortex Public Exports (4,096 tests)

**touring-cortex now fully public** — all 84 handlers, traits, and types exported for reuse.

**Public API surface**:
- `touring_cortex::Handler` trait (the core contract)
- `touring_cortex::Pipeline` + `touring_cortex::register_all()`
- `touring_cortex::CortexContext`, `CortexRuntime`, `KnowledgeRef`
- All 84 handlers accessible as `touring_cortex::handlers::{H01_HookEventClassifier, ...}`
- `touring_cortex::cross_audit` module (tests)
- All fusion, call_graph, similarity, dspy modules public

**Breaking change note**: `pub(crate)` visibility removed from all handlers (102 occurrences). Duplicate `"pub"` literal in `enrichment.rs` fixed.

**4 structs required `#[derive(Default)]`** (clippy warning → error when pub):
- `PostCompactHandler` (`lifecycle.rs`)
- `CodeStandardsEnforcerHandler` (`quality.rs`)
- `DspyQualityBridgeHandler` (`quality.rs`)
- `StreamingMCTSHarness` (`streaming_mcts.rs`)

## v29.7.0 Cortex Context/Types Public API (4,096 tests)

**touring-cortex context + types constructors now public** — external handler implementations and test utilities can construct `CortexContext` and `HandlerResult` without hacky internal access.

**8 items promoted from `pub(crate)` to `pub`**:

| Item | File | Rationale |
|------|------|-----------|
| `HookEvent::parse` | types.rs:57 | String → enum parsing for external tooling |
| `HookEvent::as_str` | types.rs:96 | Serialização para string |
| `HookEvent::hook_output_name` | types.rs:133 | Hook output naming |
| `Decision::escalate` | types.rs:166 | Combina decisões (Block > Allow > Skip) |
| `HandlerResult::skip` | types.rs:192 | Construtor para resultado Skip |
| `HandlerResult::allow` | types.rs:203 | Construtor para resultado Allow |
| `HandlerResult::block` | types.rs:215 | Construtor para resultado Block |
| `CortexContext::from_input` | context.rs:58 | Criar contexto para testes/unitários |
| `CortexContext::from_input_full` | context.rs:68 | Mesmo, com todas as DB refs |
| `CortexContext::merge_result` | context.rs:138 | Accumulate handler results |

**Kept `pub(crate)`** (internal cache/filter details):
- `FilterCacheKey`, `FilterCache`, `Pipeline::execute/for_event`, `CortexRuntime::init`
- `to_cache_string`, `to_context_string` in cache_strategy.rs

**0 breaking changes** — all items were previously `pub(crate)`, now simply visible externally.

## v29.4.x C29-C37 Excellence Sprint (4,196 → 4,320 tests)

**+124 tests** across 8 modules. 0 clippy warnings maintained throughout.

### Bug Fixes

- **`is_test_file()` P1** (`shared/quality.rs`): Now correctly handles relative paths starting with `tests/` or `test/` — previously only matched absolute-style paths containing `/tests/`. All callers in `post_edit.rs`, `post_write.rs`, `pre_write.rs` benefit.
- **12 silent error discards fixed** (`post_edit.rs`, `post_write.rs`, `post_bash.rs`, `ast_bridge.rs`): `let _ = fallible_call()` → `if let Err(e) { tracing::debug!(...) }`. Every error path now has diagnostic context.

### New Capabilities

- **BM25 integration** (`pre_write.rs`, `pre_edit_prevention.rs`): `rank_gotchas_by_relevance()` from `shared/signals.rs` replaces linear gotcha lookup — relevance-ranked results surfaced to context injection.
- **CC reduction** (`pre_edit_prevention.rs`): Extracted `collect_syntax_issues()` and `collect_complexity_issues()` from `check_syntax_and_complexity()`, reducing cyclomatic complexity and improving testability.

### Test Coverage (+124 tests across 8 modules)

| Module | New Tests | Coverage Added |
|--------|-----------|----------------|
| `shared/quality.rs` | +3 | `is_test_file` edge cases (relative paths), `measure_quality_snapshot` |
| `shared/cila.rs` | +7 | Env-var override (L0/L2/L4), invalid-parse fallback, level-bucket mapping |
| `shared/antipatterns.rs` | +15 | All 8 languages (Rust/Python/TypeScript/JavaScript/Go/C/C++/Java), deduplication, empty source |
| `ann_memory.rs` | +9 | `add_batch`, EmbeddingIndex edge cases, search ordering, k-clamping |
| `inferlets.rs` | +6 | `wrap_for_dispatcher` — JSON injection, non-JSON data, all-variant kind names |
| `pre_edit.rs` helpers | +21 | `short_list`, `is_rust_builtin`, `is_code_file`, `format_caller_list` |
| `post_edit.rs` helpers | +14 | `normalize_error_fallback`, `compose_post_edit_feedback`, `collect_layer_diagnostics`, `extract_first_text_block`, error extractors, `summarize_edit_tool` |
| `post_write.rs` + `pre_write.rs` helpers | +17 | `build_response`, `collect_failed_layer_issues`, `antipattern_signals` (test/spec file detection), `count_pub_symbols` |

## v29.9.0 touring-simd Integration Sprint (4,394 → 4,570 tests)

**+176 tests** across `touring-ast` and `touring-hooks`. 0 clippy warnings maintained.

### touring-ast Integrations (6 modules)

- **quality.rs** (deduplication): Private `WilsonScore` struct removed; `touring_simd::WilsonRanker` via `#[cfg(feature="simd-search")]` — single source of truth for Wilson lower-bound formula; scalar fallback preserved
- **semantic_search.rs** (acceleration): `find_similar_symbols` O(n log n) sort → `TopKSearcher::top_k()` O(n log k); `find_similar_batch()` new method with pre-normalized embeddings + `batch_dot_products_par` (full rayon parallelism)
- **graph/pheromone.rs** (new capability): `SymbolPheromoneMap::evaporate_with_drift_check() -> Option<f64>` and `PheromoneGraph::evaporate_with_drift_check() -> Option<f64>` via `DriftDetector::ks_statistic` — enables proactive cache invalidation on distribution drift
- **speculate.rs** (new capability): `SpeculateResult.bayesian_score: Option<f64>` — `compute_bayesian_score()` with cfg-gated variants; confidence weights: syntax=0.9, symbol=0.75, structural=0.75, import=0.6; backward-compatible `#[serde(default, skip_serializing_if)]`

### touring-hooks Integration

- **ast_bridge.rs** (new capability): `pub fn fuse_quality_evidence(source, file_path, pipeline: &MetacognitivePipeline) -> Option<Resolved>` — fuses `analyze_file_quality` (complexity) + `analyze_ast_quality` (AST quality report) as `Evidence` structs into single `Resolved` via `MetacognitivePipeline::resolve`

### Dependency Changes

- `touring-hooks/Cargo.toml`: `touring-simd` now has `cortex-integration` feature enabled (required for `touring_simd::cortex` module)
- `touring-ast/Cargo.toml`: `simd-search` feature now includes `touring-simd/learning-integration` (required for `batch_dot_products_par`)

### VP-Scout False Positives Avoided

2 pre-existing integrations correctly identified as already implemented:
1. `blast_radius.rs` — `AnnIndex`/`HnswIndex` from touring-simd already active under `#[cfg(feature="ann")]`
2. `semantic_search.rs` — `CosineComputer` already active under `#[cfg(feature="simd-search")]`

## v29.8.0 DB Consolidation & U4 Quantization Sprint (4,570 → 4,805+ tests, SCHEMA_VERSION=8)

**8 legacy SQLite DBs consolidated into 3 domain DBs** + U4 quantization activated + settings.json hooks wired.

### DB Consolidation (Phases 0, 2.1-2.3)

- **3 domain DBs**: `knowledge.db` (symbols + file knowledge + wiring), `memory.db` (RLM + semantic recall + ANN embeddings), `graph.db` (GoT + RL pipeline + sessions + learning_linucb)
- **SCHEMA_VERSION 6→8**: Explicit column mappings for all 7 tables with schema divergence (file_knowledge, bash_outcomes, edit_history→file_edit_history, gotchas→file_gotchas, wiring_map, module_ecosystem, learning_wilson)
- **CLI**: `touring migrate {status|plan|run|validate|cleanup|rollback}` — full migration toolkit
- **Canonical paths**: `TouringConfig::knowledge_db_canonical()`, `memory_db_canonical()`, `graph_db_canonical()` — single source of truth
- **CortexRuntime**: migrated from hardcoded `.claude/data/` paths to canonical functions
- **TouringServer**: migrated from `config.rlm_db_path` to `memory_db_canonical`/`graph_db_canonical`
- **FTS5**: Automatic rebuild after migration via `INSERT INTO fts(fts) VALUES('rebuild')`

### ANN Memory Recall Activation (B6)

- **pre_read.rs**: `ann_recall_signal()` searches persistent ANN index for related files (path-hash embedding, <1μs)
- **post_edit.rs**: `ann_recall.add_memory()` stores edit context with path-hash embedding via `RefCell<Option<PersistedAnnMemoryRecall>>`
- **path_hash_embedding()**: FNV-1a deterministic hash of file path segments → 64-dim normalized vector. Clusters files by directory structure.
- **Interior mutability**: `ann_recall` field changed from `Option<T>` to `RefCell<Option<T>>` for mutation through `&HookRuntime`

### U4 Quantization Activation (B7, B13)

- **Feature chain**: `touring-server` → `touring-hooks/u4-quantization` → `touring-learning/u4-quantization` → `touring-simd/quantization` → `dep:half`
- **SemanticRecall**: `store_chunk()` writes both f32 + u4 when feature enabled; `ann_search()` prefers u4 path (`ann_search_u4`) with f32 fallback
- **EmbeddingU4**: Per-vector min/max 4-bit quantization, 8× compression, ~90% Recall@10 on synthetic data
- **Recall@10 test**: Clustered synthetic embeddings (500 vectors, 384 dims, 20 clusters) — verified ≥90%

### settings.json Hooks Wiring (CRITICAL)

**29 Touring hooks configured across 15 Claude Code events** (v30: +11 hooks, +3 events). Originally 0 hooks were configured (v29.7), making the entire Touring system dead code at runtime.

| Event | Hooks | Timeout |
|-------|-------|---------|
| `PreToolUse[Read]` | `touring-hook pre-read` | 10s |
| `PreToolUse[Edit]` | `touring-hook pre-edit` + `pre-edit-prevention` | 10s |
| `PreToolUse[Write]` | `touring-hook pre-write` | 10s |
| `PreToolUse[Bash]` | `touring-hook pre-bash` | 10s |
| `PostToolUse[Edit]` | `touring-hook post-edit` | 10s |
| `PostToolUse[Write]` | `touring-hook post-write` | 10s |
| `PostToolUse[Read]` | `touring-hook post-read` | 10s |
| `PostToolUse[Bash]` | `touring-hook post-bash` | 10s |
| `PostToolUse[*]` | `touring-hook post-tool-rl` + `check_context.sh` | 10s/1s |
| `SessionStart` | `touring-hook session-start` | 15s |
| `SessionEnd` | `touring-hook session-stop` | 10s |
| `FileChanged` | `touring-hook file-changed` | 5s |
| `CwdChanged` | `touring-hook cwd-changed` | 5s |
| `SubagentStart` | `touring-hook subagent-start` | 5s |
| `SubagentStop` | `touring-hook subagent-stop` | 5s |
| `PreCompact` | `touring-hook pre-compact` | 10s |
| `Setup` | `touring-hook setup` | 10s |
| `UserPromptSubmit` | `prompt_enhancer.py` | 5s |
| `PostToolUseFailure` | `touring-hook post-tool-failure` | 10s |
| `PostCompact` | `touring-hook post-compact` | 10s |
| `InstructionsLoaded` | `touring-hook instructions-loaded` | 10s |

**v30 enhancements to settings.json:**
- **`if` conditionals**: 6 hooks filtered by file extension (`*.rs|*.py|*.ts|etc`) to avoid spawning for non-code files
- **`statusMessage`**: 18 hooks show spinner messages (e.g. "Touring: analyzing blast radius...")
- **Prompt hook (S3.1)**: haiku validates API contract on Edit(`*/lib.rs|*/mod.rs|*/main.rs`)
- **Agent hook (S3.2)**: Subagent verifies pub items on PostToolUse[Edit] for `lib.rs`/`mod.rs`
- **Async telemetry (S3.3)**: `telemetry_logger.sh` appends NDJSON events, non-blocking
- **CLAUDE_ENV_FILE (S4.1)**: `session_env_setup.sh` persists TOURING_* env vars across sessions

### E2E Audit Tests (+8 tests)

- 6 migration E2E tests (table name mappings, column mappings, FTS5 integrity, happy path)
- 2 u4 search tests (`ann_search_u4_finds_stored_chunks`, `ann_search_fallback_to_f32_when_no_u4`)

## v30.0.0 Enhancement Sprint (hooks 59→68, settings.json 18→29, events 12→15)

### New Handlers (3 files)
- **post_tool_failure.rs** — PostToolUseFailure handler. Records failures in knowledge graph, auto-creates gotchas, circuit breaker (Halt after 5+ failures on same file)
- **post_compact_handler.rs** — PostCompact handler. Re-warms result cache for top accessed files after context compaction
- **instructions_loaded.rs** — InstructionsLoaded handler. Injects project knowledge stats on session init (files tracked, edits, commands, gotchas)

### HookResponse Extensions (hook_runtime.rs)
- **Deny variant** — `permissionDecision: "deny"` for PreToolUse. Wired in `pre_edit_prevention.rs` when speculate score < 0.3 AND syntax failure
- **Block variant** — `decision: "block"` for PostToolUse. Wired in `post_edit.rs` when 4+ new antipatterns detected
- **Halt variant** — `continue: false` for circuit breaker. Wired in `post_tool_failure.rs` when 5+ failures on same file
- **ContextWithUpdatedInput variant** — `updatedInput` field for modifying tool input. Wired in `pre_read.rs` for relative→absolute path normalization
- **Context truncation** — 9,500 char cap (UTF-8 safe) to stay under Claude Code's 10K limit

### settings.json Enhancements
- **3 new events**: PostToolUseFailure, PostCompact, InstructionsLoaded
- **`if` conditionals**: 6 hooks filtered by file extension to avoid spawning for non-code files
- **`statusMessage`**: 18 hooks show spinner messages
- **Prompt hook (S3.1)**: haiku validates API contract on Edit for lib.rs/mod.rs/main.rs
- **Agent hook (S3.2)**: Subagent verifies pub items on PostToolUse[Edit]
- **Async telemetry (S3.3)**: `telemetry_logger.sh` appends NDJSON events, non-blocking
- **CLAUDE_ENV_FILE (S4.1)**: `session_env_setup.sh` persists TOURING_* env vars

### New Scripts
- `~/.claude/hooks/telemetry_logger.sh` — Async NDJSON event logger
- `~/.claude/hooks/telemetry_dashboard.sh` — Dashboard (--live, --history, --summary)
- `~/.claude/hooks/session_env_setup.sh` — CLAUDE_ENV_FILE integration wrapper

## v30.1.0 Schema Rename Completion + touring-analysis Sprint (4,805+ → 4,919+ tests)

### Schema Rename FULLY PROPAGATED (SCHEMA_VERSION=8)

The v29.8.0 migration plan documented `edit_history→file_edit_history` and `gotchas→file_gotchas`
but several source files still used the old names. All usages are now reconciled:

| File | Fix |
|------|-----|
| `touring-core/src/schema/knowledge.rs` | `KNOWLEDGE_SCHEMA_V8`: table DDL + all 6 indexes renamed |
| `touring-hooks/src/knowledge.rs` | All SQL DDL/DML updated; `error_rate_history()` added |
| `touring-core/src/migration/consolidation.rs` | 5 migration bugs fixed (wrong columns in bash_outcomes, edit_history, gotchas, wiring_map, module_ecosystem) |
| `touring-analysis/src/e2e/schema_guard.rs` | Constants updated; `validate_graph_tables()` added |
| `touring-analysis/tests/e2e_integration.rs` | All INSERT statements updated to new table names |
| `touring-analysis/src/temporal/trends.rs` | Test INSERTs updated |
| `touring-server/src/knowledge_adapter.rs` | Test INSERTs updated |

### touring-analysis Crate (Layer 2.5) — SHIPPED

- **114 tests** (98 unit + 16 E2E), 0 clippy warnings
- `temporal` feature now in `default` (was gated, broke E2E compilation)
- `cognitive_complexity` added to `ComplexityMetrics` (nesting-depth scanner)
- `compute_with_start(pipeline_start: Instant)` on `BlastRadiusEngine` for shared-timer pipelines
- `QualityReport` now 8-dimensional: cognitive penalty + unwrap_risk_score penalty + test_proxy bonus
- `error_rate_history()` on `FileKnowledgeDB` — 30-day bash_outcomes series for KS drift detection in `pre_read`

## v30.2.0 touring-analysis Potencialização Sprint (4,919+ → 5,157 tests)

### touring-analysis v0.3.0 — Public API Expansion + RL Loop Closure

**Goal**: Maximise scope and integration of `touring-analysis` — zero orphan pub symbols, closed RL feedback loop, and a cognitive bridge to `touring-cognitive`.

### New / Changed Symbols

| Symbol | File | Change | Description |
|---|---|---|---|
| `BfsStrategy` | `blast_radius/bfs.rs` | `pub(crate)` → `pub` | Exact BFS strategy, now externally constructible |
| `BfsStrategy::new` | `blast_radius/bfs.rs` | `pub(crate)` → `pub` | Factory, takes `Arc<SymbolIndex>` |
| `HNSW_EMBED_DIM` | `blast_radius/hnsw.rs` | NEW | `pub const usize = 64` — FNV-1a embedding dimension |
| `path_hash_embedding` | `blast_radius/hnsw.rs` | NEW | `pub fn(path: &str) -> Vec<f32>` — 64-dim L2-normalized path vector |
| `HnswStrategy` | `blast_radius/hnsw.rs` | NEW | ANN blast radius via HNSW, `LatencyTier::Slow` |
| `HnswStrategy::new` | `blast_radius/hnsw.rs` | NEW | Builds HNSW index over all files in `SymbolIndex` |
| `BlastRadiusEngine::bfs_only` | `blast_radius/mod.rs` | NEW | Convenience factory for single-strategy BFS engine |
| `BlastRadiusEngine::hnsw_only` | `blast_radius/mod.rs` | NEW | Convenience factory for HNSW engine (feature-gated `ann-blast`) |

### New File: `crates/touring-analysis/src/blast_radius/hnsw.rs`

6 unit tests: `name`, `latency_tier`, `excludes_start_file`, `empty_index`, `embedding_normalized`, `embedding_differs`.

Feature gate: `ann-blast` requires `touring-simd/ann`.

### New File: `crates/touring-cognitive/src/analysis_bridge.rs`

Bridge between `touring-analysis` and `MetacognitivePipeline` bandit feedback loop.

- `pub fn enrich_with_analysis(engine, knowledge, learning)` — maps `KnowledgeReport` + `LearningReport` to `AdaptiveEngine::record_outcome` calls (MCTS@L2, Hybrid@L4)
- `pub fn calibration_summary(knowledge, learning) -> String` — human-readable telemetry line for `touring cognitive metrics` CLI
- Feature gate: `analysis-bridge` in `touring-cognitive/Cargo.toml`
- 4 unit tests: `enrich_does_not_panic_on_healthy_codebase`, `degrading_trend_reduces_hybrid_reward`, `hot_files_penalty_applied`, `calibration_summary_contains_key_fields`

### touring-hooks cli_e2e.rs — RL Loop Closure (T5)

`phase_learning()` now calls `update_linucb_blast_signal(rt, blast_count)`:

- Counts total reverse-dependency edges from `symbol_index`
- Normalises to `[0,1]` reward (saturating at 100 connections)
- Injects into `ArmKind::BlastRadius` LinUCB arm
- `"blast_connections": blast_count` added to `phase_learning` metrics JSON

### Test Count

| Version | Tests |
|---|---|
| v30.1.0 | 4,919+ |
| v30.2.0 | **5,157** |

0 clippy warnings maintained throughout.

## v30.3.0 Predictive Wave — Advisory-Mode Hook Layer (2026-04-20)

Três novos mecanismos preditivos integrados ao pipeline de hooks como **advisory hints** (não-bloqueantes). Cada hint é injetado via `HookResponse::ContextWithUpdatedInput` com prefixo `[TOURING-INJECT]`, preservando o output original e adicionando contexto preditivo.

**Referências cruzadas**:
- Diagrama ASCII e visão conceitual: `docs/ARCHITECTURE.md#predictive-wave` (não duplicado aqui)
- Implementação detalhada: `crates/touring-hooks/touring-hooks-ARCHITECTURE.md`
- Session report completo: `docs/2026-04-20-predictive-wave.md`

---

### D2 — Predictive Blast Injection

**Arquivo**: `crates/touring-hooks/src/pre_tool_use.rs:245`
**Trigger**: `tool_name ∈ {TaskCreate, TaskUpdate}` + `blast_modules > 3`
**Budget**: 40ms timeout

`compute_predictive_blast_injection` chama `BlastRadiusEngine::compute_with_timeout` (definido em `crates/touring-analysis/src/blast_radius/mod.rs:248`) e injeta os módulos afetados no contexto de entrada da tool antes que o agente processe o TaskCreate/TaskUpdate. O helper interno `blast_via_engine` (refactored de `accumulate_blast_modules`) abstrai a chamada ao engine e a serialização do resultado.

**Fluxo**: `pre_tool_use` recebe evento → identifica TaskCreate/TaskUpdate → chama `compute_with_timeout(40ms)` → se blast > 3 módulos → monta `[TOURING-INJECT] blast_modules=[...]` → retorna `HookResponse::ContextWithUpdatedInput`.

---

### D3 — LinUCB Routing Hints

**Arquivos**:
- `crates/touring-hooks/src/shared/task_features.rs:108` — `extract_task_features` (vetor 25-dim)
- `crates/touring-hooks/src/shared/task_features.rs:159` — `TaskRoutingDecision` (8 arms)
- `crates/touring-hooks/src/lifecycle/task_list.rs:185` — `linucb_routing_hint`

**Confidence threshold**: 0.15
**Cross-crate contract**: `touring-learning::bandit::linucb::LinUCBBandit` (FEATURE_DIM=25, NUM_ARMS=8)

`linucb_routing_hint` extrai 25 features da task (complexidade, histórico, tipo), consulta o `LinUCBBandit` do crate `touring-learning`, e injeta o arm recomendado como hint de roteamento no contexto. Arms disponíveis mapeiam para estratégias de execução (L0-L4 + variantes paralelas).

---

### D4 — MCTS Shadow Rollouts

**Arquivos**:
- `crates/touring-hooks/src/shared/shadow_rollout.rs` — `ShadowRolloutResult` + `run_shadow_rollout`
- `crates/touring-hooks/src/lifecycle/plan_mode/enter.rs:388` — `mcts_shadow_rollout_hint`

**Gate**: `cila_level >= L3`
**Budget**: 12s thread + 200ms `join_timeout` (não-bloqueante — timeout resulta em hint omitido, nunca em falha)

**Resolução de homonimia** (crítica para leitura do código):
- `CognitiveMCTS` em `cognitive_mcts.rs:170` = type alias para `GraphInformedMCTS`
- `PheromoneMCTS` em `mcts.rs:649` = struct independente (ex-`CognitiveMCTS` antes do rename)

São **dois sistemas distintos**: `CognitiveMCTS` é o MCTS guiado por grafo de dependências; `PheromoneMCTS` é o bandit de feromônio para threshold adaptativo. Não confundir ao ler o código.

`run_shadow_rollout` executa em thread separada: simula N rollouts do plano via `CognitiveMCTS`, retorna `ShadowRolloutResult {best_path, confidence, rollout_count}`. `mcts_shadow_rollout_hint` formata o resultado como hint e injeta via `ContextWithUpdatedInput` antes do `EnterPlanMode` processar.

---

### D5 — Observability Counters (9 novos)

Definidos em `crates/touring-hooks/src/shared/gate_metrics.rs` como `AtomicU64` lock-free.

| Counter | Semântica | Alert threshold |
|---|---|---|
| `blast_inject` | Blast hints injetados com sucesso | — |
| `blast_timeout` | Blast injections que excederam 40ms | > 5% de `blast_inject` |
| `blast_mutation` | Inputs mutados (conteúdo alterado pelo hint) | — |
| `linucb_route_manual` | Decisões de routing manual (confidence < 0.15) | — |
| `linucb_route_generator` | Decisões via generator strategy arm | — |
| `linucb_route_hint` | Hints de routing injetados | — |
| `mcts_shadow_run` | Shadow rollouts completados | — |
| `mcts_shadow_timeout` | Rollouts que excederam 200ms join_timeout | > 10% de `mcts_shadow_run` |
| `mcts_deadlock_detected` | Deadlocks detectados no shadow thread | deve ser 0 |

**Exposure**: `touring gate-metrics -j` (snapshot direto) | `touring status -j` (agregado no dashboard via `.gate_metrics` key)

---

### Cross-Crate Data Flow (Predictive Wave)

```
touring-analysis                touring-hooks                 touring-learning
----------------                -------------                 ----------------
BlastRadiusEngine        --D2--> pre_tool_use                       |
  compute_with_timeout           compute_predictive_blast_injection  |
  blast_via_engine               HookResponse::ContextWithUpdatedInput|
                                                                      |
                         <--D3-- task_list                           |
                                  linucb_routing_hint         LinUCBBandit
                                  extract_task_features ------> (FEATURE_DIM=25
                                  TaskRoutingDecision            NUM_ARMS=8)

touring-cognitive               touring-hooks
-----------------               -------------
CognitiveMCTS           <--D4-- plan_mode/enter
  (= GraphInformedMCTS)          mcts_shadow_rollout_hint
  run_shadow_rollout             ShadowRolloutResult
PheromoneMCTS (mcts.rs:649)     (INDEPENDENTE -- nao usado no D4)
```

**Nota**: A seta D3 é bidirecional — `task_list` lê do `LinUCBBandit` (touring-learning) e também reporta outcomes via `inject_reward` (fechando o loop RL).

---

### Test Count

| Version | Tests |
|---|---|
| v30.2.0 | 5,157 |
| v30.3.0 | **5,157+** (testes unitários D2/D3/D4 integrados nos crates existentes) |

0 clippy warnings. Advisory hints são sempre não-bloqueantes: timeout ou erro interno resulta em `HookResponse::Context` normal (sem mutação), nunca em falha da tool.
