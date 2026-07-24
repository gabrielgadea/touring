# Touring Workspace — Architecture Reference

Version: 30.3.1 (v30.3.1: Cognitive Architecture Upgrades — Tarjan SCC, 6-layer Speculate, GoT pre-compact persist, SessionBus, emit_scored priority queue, file_digest, HNSW activated; SCHEMA_VERSION=8; 5,154+ tests, 0 failed, clippy 0; 2026-04-09)
Location: `.claude/rust/`
Previous: v4.0.0 → v8.0.0 (proactive) → v9.0.0 (cleanup) → v9.1.0 (RL) → v9.2.0 (quality) → v10.5.0 (global) → v10.8.0 (enrichment+cipher) → v11.0.0 (AST enrichment) → v12.0.0 (ACO potentialization) → v13.0.0 (RL loop closure) → v14.0.0 (semantic search + transfer + burn) → v15.0.0 (cognitive loop closure) → v16.0.0 (full hook coverage + cross-crate integration) → **v17.0.0 (daemon persistente + schema gate + touring-python)** → **v18.0.0 (touring-improvements-2026: 13 insights — diagnostic engine, evolution pkg, state machine, objective hash, file heat, RRF search, enriched blast radius, cognitive MCTS, refinement cycle, adaptive decay, GoT multi-dim, Q-value persistence)** → **v19.0.0 (10 insights: TemplateLibrary, GoalTracker 9×9, PhaseRegistry, GraphAnalytics, CQRS, Saga, ParallelEngine, ESAA-24, TimeTravel, AgentStateMachine)** → **v19.1.0 (ACO cross-audit: IC-1 Arc<Mutex<>> sharing, IC-2 GoT pheromone integration, IC-3 PredictiveFocusCache, IC-4 AcoRewardPropagator TD(λ) + 14 E2E integration tests)** → **v19.2.0 (23 ACO insights: INS-L1..L10 touring-learning + INS-A1..A5 touring-ast + INS-C1..C5 touring-cognitive; 4 novos módulos: pheromone_bus, tracker_rl_bridge, multi_objective, hybrid_engine; daemon wiring: AcoWiringState fecha Cadeia 1 file-edit + Cadeia 2 TrackerReport→RL; P4 fixes: pub(crate) + session_turn)** → **v20.0.0 (Plano H1+H2: S1 tree-sitter+shadow_v2 atomic, S2 petgraph DependencyCache+LinUCB FEATURE_DIM=25, S3 crate touring-index, S4 crate touring-cortex, S5 rkyv IndexSnapshot+MCTS MCTSConfig::for_cila_level, S6 wasmtime WasmRunner PoC)** → **v21.0.0 (N1: team_hooks gateway Agent Teams ↔ ACO — teammate-idle + task-completed wired, PheroKey::TaskId + TeammateId, 4 cadeias E2E, +16 testes → 2,671)** → **v21.1.0 (segregação+integração: touring-nlp→touring-antt, SCHEMA_VERSION pub const em touring-core::migration, cortex fork removido via pub use touring_cortex as cortex, touring-wasm feature-gated, +227 testes em touring-cortex para recuperar baseline)** → **v22.0.0 (S0–S8 Sprint: ReminderBandit 7-arm, GoT Parallel JoinSet+Arc<GotNode>, GoTSnapshot rkyv, SessionPersistence deadpool-sqlite, InferletPool Semaphore, TypedEvaluate scored 0–100, RRF fusion, CallGraph Tarjan SCC, CrdtDelta delta-sync, DriftMonitor KS-test, EbpfObserver feature-gated, BranchFs CoW TempDir, +169 testes → 2,840)** → **v24.1.0 (BIGMAS-L6: RingBuffer observability, RLE encoding for CrdtDelta, KvCacheManager + wasmtime PoolingAllocationConfig, rkyv zero-copy load_from_mmap_unchecked; +54 testes → 2,894)** → **v24.2.0 (touring-cortex optimizations: FilterCache LRU pipeline, Budget Adaptativo tiktoken cl100k_base, RRF parallel rayon+DashMap, CallGraph SCC atomic versioning+hotspots_parallel, Zstd base64 compression; +146 testes → 3,040)** → **v25.0.0 (touring-hooks Excellence Sprint: 15 strategies S1-S14+S2, daemon multi-threaded, circuit breaker IPC, dispatch table, SCHEMA_VERSION=5, HookRuntime decomposed; +200 testes → 3,040)** → **v28.0.0 (touring-cognitive Excellence 15 strategies S1-S15 + Wiring Intelligence 6 layers + cortex cross-audit: TfIdf→Nexus, AdaptiveEngine→Runtime, SqliteGraphStore wired; SCHEMA_VERSION=6, 82 handlers, +276 testes → 3,316)** → **v28.1.0 (deadpool-sqlite interact() pattern + ndarray_mlp (25→64→64→8) replacing burn; spawn_blocking handler execution; P50<5ms; +26 testes → 3,342)** → **v28.2.0 (touring-wasm async-native: AsyncKvCacheManager async trait, AsyncInferletPool InstancePre+spawn_blocking, call_evaluate_async, async_support(true); H83 TypedEvaluateHandler + PluginRegistry em touring-cortex; E2E 10/10 + 97 testes; 3,422)** → **v28.4.0 (CLI telemetry: 14 subcommands via daemon IPC, cli_handlers.rs module, hook_registry 21→35 handlers, Unix socket client in touring-server, clippy 0, 3,595 tests)** → **v28.13.0 (Inferlets E2E: 4 inferlets in 1 WASM binary, InferletService integrated, AsyncInferletPool real eval, 11 E2E tests, strip_inferlet_key manual parsing, wasm32-unknown-unknown target, 3,944 tests)** → **v29.0.0 (Pre/Post Edit/Write Excellence: 5 hooks enhanced/created — pre_edit rewrite 1500+ LOC scored signals + CILA budget + rayon parallel + 8 AST signals, pre_write new 850+ LOC, pre_edit_prevention +4 signals, post_edit feedback-capable run_returning() + multi-language anti-patterns 7 languages, post_write new 750+ LOC; hooks 57→59, +132 tests → 3,983)** → **v29.1.0 (Hook wiring sprint — 8 gaps em settings.json corrigidos: SessionStart, SessionEnd, PostToolUse[Read/Bash/*→post-tool-rl], PreToolUse[Read/pre-edit-prevention], PreCompact→touring-hook; rust-analyzer-lsp habilitado; bugs P1/P2: daemon mutex poison, prompt_enhance empty iterator; +26 tests → 4,009)** → **v29.2.0 (Layer7 PredictionLayer wired: post_edit.record_edit() + pre_read.predict_next(); settings.json: PreToolUse[Bash]→pre-bash; DomainId fix (&'static str+Copy); 0 tests change → 4,009)** → **v29.3.0 (N1+N2+N3 Expansion: 16 deliverables D1–D16; N1 ToolCatalog 28 tools InvocationType; BasicGenerator CC 67→16 pattern-matching table; cross-audit 20/20 PASS; +94 tests → 4,103)** → **v29.4.0 (TACO Audit Sprint 3 sprints: S1 daemon SIGTERM/SIGINT tokio::signal, session_hooks P0 emit_allow→Ok, knowledge file_risk_scores+dedup, layer7 VecDeque+HashMap; S2 RefCell<Option<String>> last_edited_file, Layer7 co-edit post_edit+post_write wired, QTable take/put cache, LinUCB batch; S3 shared/ detect_language+quality+reindex, speculate_v2 fast-path shadow_v2, TSC UTF-8 byte-safe, HookEventMetrics __health__, RL quality_score←ACO+gotcha, stale files removed, decomposer write-lock fix; -7 tests after cleanup → 4,096)**
→ **v29.5.0 (Interface Excellence Sprint: try_daemon_health_direct() bypasses circuit breaker for health checks; session_id propagated to DaemonRequest activating session-level circuit dimension; FFI consolidation pub(crate) current_uid() in lib.rs; Setup hook wired in settings.json; MCP: single metadata() syscall in file_ops, AST languages 4→13, graph_svc.inject() on 4 missing tools; main() CC 68→51 via lifecycle alternation pattern (-17 branches); WilsonRanker+DriftDetector derive Clone; insights() reuses self.ranker+self.drift_detector eliminating 2 SQLite opens/call; 3918 tests confirmed, clippy 0; 2026-03-29)**
→ **v30.3.0 (PLN2 P4 Meta-Optimization: P4.1 Agent Diary System with AAAK dialect + 8/8 E2E; P4.2 Palace Hierarchy (4-level wing/room/closet/drawer) with idempotent schema migration; P4.3 Evolution Drift Self-Correction with alert levels (none/degraded/structural) + inject_reward loop; P4.4 AutoSaveHook wired in post_tool_rl.rs:177; Schema isolation: memory.db (new) vs rlm_memory.db (legacy); 5,154 tests, clippy 0; 2026-04-09)**
→ **v30.3.1 (Cognitive Architecture Upgrades: Tarjan SCC cycle detection in CallGraph, 6-layer Speculate (Syntax/Symbol/Structural/Import/Complexity/CfgImpact), GoT snapshot persistence in pre-compact via GoTSnapshotStore, SessionBus typed inter-hook communication, emit_scored+merge_and_rank priority queue in CortexContext, file_digest_signal AST summary, blast_radius reclassified as precomputable, hnsw-working-memory feature activated in touring-server; 2026-04-09)**
Artifacts: `touring` binary (~25MB) + `touring-hook` binary (~11MB) + `touring-daemon` binary (~10MB) + `libclaude_learning_kernel.so` (v4.0.0)
Test suite: 5,154+ tests (v30.3.1: +Tarjan SCC, +Speculate L6 Complexity, +SessionBus, +file_digest, +emit_scored; cargo test --workspace --exclude touring-python), clippy deny all (0 warnings)
Quality: 0 production unwrap(), 0 TODO/FIXME, cross-audit PASS (v30.3.1: 5,154+ tests, 0 clippy, exit-0 invariant, composite=1.0)
Crates: 15 (14 workspace + 1 WASM target)

---

## Table of Contents

1. Executive Summary
2. Workspace Topology (diagram)
3. Crate Catalog (13 crates)
4. Dependency Graph
5. Dual-Mode Operation (preserved from v3.0.0)
6. Subsystem: touring-core (foundation)
7. Subsystem: touring-simd (performance)
8. Subsystem: touring-learning (unified brain)
9. Subsystem: touring-ast (code intelligence)
10. Subsystem: touring-antt (NLP pipeline — ex touring-antt)
11. Subsystem: touring-cognitive (predictive engine)
12. Subsystem: touring-server (MCP + Cortex + Hooks)
13. Subsystem: touring-python (PyO3 bindings)
14. Data Flows — End-to-End Traces
15. Database Schemas and Persistence
16. Concurrency Model
17. Configuration and Environment
18. Quality, Testing, and Cross-Audit
    1. Feature Flag Matrix
19. Migration from v3.0.0 (monolith → workspace)
20. touring-server Coupling Analysis and Extraction Roadmap
21. Glossary

---

## 1. Executive Summary

Touring v28.12.0 is a **Cargo workspace** containing 13 crates that replace two
previously independent Rust codebases:

- `.claude/rust-core/` (31,017 LOC, package `claude_learning_kernel`) — PyO3 extension
- `.claude/touring/` (27,781 LOC, package `touring`) — MCP server + cortex CLI

The unification eliminates 13,324 LOC of dead/duplicate code (-42%) and establishes
a **closed learning loop**: every Rust computation — whether invoked via MCP tool,
PyO3 bridge, or cortex hook — feeds into and benefits from the same QTable, Memory,
and Evolution subsystems.

### Key metrics

| Metric | v16.0.0 | **v19.2.0** | **v22.0.0** | **v25.0.0** | **v28.0.0** | **v28.2.0** | **v28.12.0** | **v29.4.0** | **v29.5.0** | **v29.8.0** | **v30.3.0** |
|--------|---------|------------|------------|------------|------------|------------|-------------|------------|------------|------------|
| Crates | 10 | **10** | **13** | **13** | **13** | **13** | **13** | **13** | **13** | **14 (+inferlets)** | **15 (+inferlets)** |
| Tests | 2,079 | **2,378** | **2,840** | **3,040** | **3,316** | **3,422** | **3,851 (+429)** | **4,096** | **3,918 (confirmed)** | **4,805+ (+887)** | **5,154 (+349)** |
| Cortex handlers | — | — | — | — | 82 | 83 | **97 (+14)** | **97** | **97** | **97** |
| AST languages | 10 | 10 | 10 | 12 | 12 | 12 | **14 (+2: Go, Java)** | **14** | **14 (MCP: 13 langs)** | **14** |
| Features active | — | — | — | — | — | 4 | **9 (+5)** | **9** | **9** | **10 (+u4-quantization)** |
| Python version | 1.0 | 1.0 | 2.0 | 2.0 | 2.0 | 3.0 | **4.0.0** | **4.0.0** | **4.0.0** | **4.0.0** |
| Clippy | deny all | deny all | deny all | deny all (0 warnings) | **deny all (0 warnings)** | **deny all (0 warnings)** | **deny all (0 warnings)** | **deny all (0 warnings)** | **deny all (0 warnings)** | **deny all (0 warnings)** |
| Hook events covered | — | — | 7/24 | 24/24 (100%) | **24/24 (100%)** | **24/24 (100%)** | **24/24 (100%)** | **24/24 (100%)** | **24/24 +Setup (100%)** | **18 hooks/12 events in settings.json** |
| touring-hook subcommands | — | — | ~15 | 36 | **36** | **36** | **36** | **36** | **37 (+setup)** | **37** |
| touring-hook binary | — | — | — | ~625KB | **~8.8MB** | **~8.8MB** | **~8.8MB** | **~8.8MB** | **~10.8MB** | **~11MB** |
| touring-daemon binary | — | — | — | — | **~8.7MB (novo)** | **~8.7MB** | **~8.7MB** | **~8.7MB** | **~10.7MB** | **~10MB** |
| libclaude_learning_kernel.so | — | 3.1MB | 3.1MB | 3.1MB | **7.6MB** | **7.6MB** | **7.6MB** | **7.6MB** | **7.6MB** | **7.6MB** |
| Hook latency (warm) | — | — | — | 10-13ms | **P50=1ms, avg=2ms** | **P50=1ms, avg=2ms** | **P50=1ms, avg=2ms** | **P50=1ms, avg=2ms** | **P50=1ms, avg=2ms** | **P50=1ms, avg=2ms** |
| Hook latency (cold start) | — | — | — | 10-13ms | **~15-20ms (uma vez/sessão)** | **~15-20ms (uma vez/sessão)** | **~15-20ms (uma vez/sessão)** | **~15-20ms (uma vez/sessão)** | **~15-20ms (uma vez/sessão)** | **~15-20ms** |
| SCHEMA_VERSION | — | — | — | — | **4** | **4** | **7** | **7** | **7** | **8 (3 consolidated DBs)** |
| Databases | — | — | — | — | 8 SQLite | 8 SQLite | 8 SQLite | 8 SQLite | 8 SQLite | **3 consolidated (knowledge+memory+graph)** |
| Cross-crate bridges | — | — | — | rules_bridge (8 fns) + nlp_bridge (7 fns) | **rules_bridge (8 fns) + nlp_bridge (7 fns)** | **rules_bridge (8 fns) + nlp_bridge (7 fns) + refinement→diagnostics (COG-2)** | **+ Wiring Intelligence 6 layers: Signal→Tracker→Cascade→RL→Cortex→Feedback** | **+ H83 TypedEvaluateHandler: touring-cortex ↔ touring-wasm (PluginRegistry, AsyncInferletPool)** | **+ insights() self.ranker+drift reuse; Clone on WilsonRanker+DriftDetector** | **+ CortexRuntime/TouringServer canonical paths; ANN recall pre_read↔post_edit** |
| Feature flags (hooks) | — | — | — | rules-engine, nlp-enrichment | **rules-engine, nlp-enrichment** | **rules-engine, nlp-enrichment** | **rules-engine, nlp-enrichment** | **rules-engine, nlp-enrichment, ebpf (stub), pooling-allocator (wasm), inferlets-wasm** | **same** | **+ u4-quantization (touring-server→hooks→learning→simd)** |
| #[non_exhaustive] enums | — | — | 1 | 4 (core, learning, ast, cognitive) | **4 (core, learning, ast, cognitive)** | **4 (core, learning, ast, cognitive)** | **4 (core, learning, ast, cognitive)** | **4** | **4** | **4** |
| #[must_use] annotations | — | — | ~20 | 71 | **71** | **71** | **73 (+1 BranchFs, +1 RingBuffer v24.1)** | **73** | **73** | **73** |
| TouringError variants | — | — | 18 | 22 (+Rules, +Nlp, +CapacityExceeded, +InvalidInput) | **22** | **22** | **22** | **22** | **22** | **22** |
| KS statistic complexity | — | — | O(n·m) | O(n+m) merge-walk | **O(n+m) merge-walk** | **O(n+m) merge-walk** | **O(n+m) merge-walk** | **O(n+m)** | **O(n+m)** | **O(n+m)** |
| MemoryTier/CILALevel | — | — | Display | + PartialOrd, Ord, TryFrom<u8>, const ALL | **+ PartialOrd, Ord, TryFrom<u8>, const ALL** | **+ PartialOrd, Ord, TryFrom<u8>, const ALL** | **+ PartialOrd, Ord, TryFrom<u8>, const ALL** | **same** | **same** | **same** |
| LinUCB RL loop | closed | closed | closed | closed + cognitive-enhanced | **closed + cognitive-enhanced** | **closed + cognitive-enhanced** | **+ Wiring RL reward from edit outcomes** | **+ RL quality_score = context_utility + ACO pheromone + gotcha prevention bonus** | **same; session_id now active in circuit dimension** | **same; post-tool-rl wired in settings.json** |
| LinUCB RL loop | closed | closed | closed | closed + cognitive-enhanced | **closed + cognitive-enhanced** | **closed + cognitive-enhanced** | **+ Wiring RL reward from edit outcomes** | **+ RL quality_score = context_utility + ACO pheromone + gotcha prevention bonus** | **same; session_id now active in circuit dimension** | **same; post-tool-rl wired in settings.json** |
| Cognitive loop | — | — | CLOSED | CLOSED + full 24-event hook coverage | **CLOSED + full 24-event hook coverage** | **CLOSED + full 24-event hook coverage** | **+ TfIdf→Nexus, AdaptiveEngine→Runtime, SqliteGraphStore wired** | **+ Layer7 co-edit prediction: record_edit+record_co_edit+update_file_heat in post_edit+post_write; predict_next in pre_read** | **same** | **+ ANN recall: pre_read search + post_edit store (path-hash embedding)** |
| ACO pheromone components | — | — | — | — | — | MctsPheromonoLayer, GotPheromoneMemory (orphan) | **+ UnifiedPheromoBus; TrackerRlBridge; MultiObjectivePheromonoLayer; HybridCognitiveEngine; AcoWiringState (daemon)** | **same** | **same** | **same** |
| Cross-audit | — | — | 5/5 PASS | PASS (2,079 tests, 0 clippy, all E2E traces verified) | **PASS (2,079 tests, 0 clippy, all E2E traces verified)** | **PASS (2,378 tests, 0 clippy, 23 insights E2E + 2 daemon cadeias + pub(crate) + session_turn)** | **PASS (3,944 tests, 0 clippy, Inferlets E2E 11/11, composite=1.0)** | **PASS (4,096 tests, 0 clippy, 17 deliverables E2E, composite=1.0)** | **PASS (3,918 tests confirmed, 0 clippy, composite=1.0)** | **PASS (4,805+ tests, 0 clippy, 18/18 hooks exit 0, E2E migration+u4+ANN)** |
| Supported languages | 11 | 13 | 13 | 13 (Go+Java, feature-gated) | **13 (Go+Java, feature-gated)** | **13 (Go+Java, feature-gated)** | **14** | **14** | **14 (MCP server: 13 lang list)** | **14** |
| Property tests | — | 3 | 3 | 3 (proptest: surgery, blast_radius, incremental) | **3 (proptest: surgery, blast_radius, incremental)** | **3** | **3** | **3** | **3** |

### Design principles

1. **Separation of concerns**: each crate owns exactly one domain
2. **DAG dependency graph**: no circular dependencies (enforced by Cargo)
3. **Shared compilation**: workspace deduplicates dependency builds
4. **Backward compatibility**: `claude_learning_kernel` Python module name preserved
5. **Never-block invariant**: all hooks exit 0, even on internal error
6. **Clippy deny all**: `[workspace.lints.clippy] all = "deny"` — every warning is a compile error
7. **Zero production unwrap**: all `.unwrap()` in non-test code replaced with `?`, `.expect()`, or `.unwrap_or_default()`
8. **Dead code elimination** (v9.0.0): every field, enum variant, and method must be exercised
9. **Correct severity mapping** (v9.0.0): ruff E5xx (style) codes are warnings, not errors
10. **HIGH-SIGNAL-ONLY** (v11.0.0): hooks inject context only when it changes Claude's behavior; silence is default
11. **Feedback loops** (v11.0.0): every post-hook feeds the next pre-hook; intelligence is emergent, not static
12. **RL loop closure** (v13.0.0): every tool execution — via PostToolUse hook — feeds reward back into QTable TD(λ) + LinUCB Sherman-Morrison; learning is continuous, not batch
13. **Transfer learning** (v14.0.0): `TransferLinUCB` blends donor-context knowledge via `export()`/`import()` with blend_weight ≤ 0.30 — new contexts bootstrap from related past experience rather than cold-start
14. **Cognitive loop closure** (v15.0.0): `CognitiveRuntime` connected to `HookRuntime` via `KnowledgeSource` trait — every hook invocation feeds `SessionPredictor` + `SemanticGraph`, and the next hook benefits from improved predictions. Pre-hooks inject risk scores, gotchas, and predicted next tools.
15. **Defensive lock recovery** (v15.0.0): All `RwLock`/`Mutex` use `unwrap_or_else(|e| e.into_inner())` uniformly — never panic on poisoned locks, always degrade gracefully
16. **SIMD-first similarity** (v15.0.0): All cosine similarity computations route through `touring_simd::CosineComputer` — no manual dot product anywhere in the workspace
17. **Full hook coverage** (v16.0.0): All 24 Claude Code hook events are wired end-to-end (settings.json → touring-hook binary → cortex handler → crate logic). No event goes unhandled.
18. **Cross-crate integration** (v16.0.0): `touring-rules` and `touring-antt` integrated into `touring-hooks` via feature-gated bridges (`rules_bridge.rs`, `nlp_bridge.rs`). No crate is isolated.
19. **Foundation hardening** (v16.0.0): `#[non_exhaustive]` on all 4 error enums, `#[must_use]` on 71 functions, `PartialOrd/Ord` on `CILALevel`/`MemoryTier`, `TryFrom<u8>` roundtrips, KS statistic O(n+m)
20. **API surface consistency** (v16.0.0): All public types implement `Display`, `Serialize/Deserialize`, and standard derives (`Clone`, `Debug`, `PartialEq`, `Eq`, `Hash`) where applicable
21. **Daemon-first hooks** (v17.0.0): `touring-hook` is a thin client that routes to the persistent daemon via Unix socket (`/tmp/touring-daemon-{uid}.sock`). The daemon keeps SQLite open and `RuntimeMap` warm per project, eliminating the 10ms floor of the process-per-invocation model. Fallback to standalone mode preserves the exit-0 invariant on daemon unavailability.
22. **Schema version gate** (v17.0.0, unified v21.1.0, v25→5, v28→6): `PRAGMA user_version` prevents redundant DDL on already-migrated databases. `pub const SCHEMA_VERSION: u32 = 6` em `touring-core::migration` é a **única fonte de verdade** — outros crates importam via `use touring_core::migration::SCHEMA_VERSION` (sem cópia local). `ensure_schema()` + `migrate_schema()` only run when `user_version < SCHEMA_VERSION`. v6 adds `wiring_map` + `module_ecosystem` tables for Wiring Intelligence.
23. **Touring improvements** (v18.0.0): 13 insights implementados — 4-layer diagnostic engine (ACO-1), evolution package population (ACO-2), auto-decompose by complexity (ACO-3), execution tracking state machine (ACO-4), objective hash invariant (ACO-5), FileHeat pheromone indexing (AST-1), RRF fused symbol search (AST-2), enriched blast radius with impact categories (AST-3), SemanticGraph-informed MCTS (COG-1), diagnostic-driven refinement cycle (COG-2 — cross-crate: touring-cognitive imports from touring-learning), adaptive temporal decay (COG-3), GoT multi-dimensional evaluation (COG-4), TransitionMatrix+QTable persistence (COG-5).
24. **ACO E2E wiring** (v19.2.0): `UnifiedPheromoBus` unifies pheromone signals from file-edits and RL rewards into a single bus (`Arc<Mutex<>>`). `TrackerRlBridge` maps `TrackerReport → (DimensionalFeatures, [f64;4], scalar)` feeding `MultiObjectivePheromonoLayer` + bus + `SessionPredictor` simultaneously. `AcoWiringState` (field `Mutex<AcoWiringState>` in `HookRuntime`) closes both chains in `post_edit::run()` via fire-and-forget — lock poison silently swallowed, exit-0 invariant preserved. `pub(crate)` on `AcoWiringInner` and `session_turn()` usage ensure correct visibility scope and real iteration granularity in `TrackerReport`.
25. **CoW safety** (v22.0.0): `BranchFs` implements copy-on-write semantics — files are snapshot to a `TempDir` before modification; `TempDir` auto-deletes on drop (implicit rollback) unless `commit()` is called explicitly. `#[must_use]` on `commit()` prevents silent rollback. Absent files are tracked as `None` in the snapshot map for accurate restore semantics.
26. **Graded plugin evaluation** (v22.0.0): `WasmModule::call_evaluate_typed()` tries `evaluate_scored` export first (returns i32 clamped 0–100) and falls back to binary `evaluate` (0→0, non-0→100). `success = score >= 50`. `InferletPool` manages concurrent WASM execution via `Arc<Mutex<VecDeque<WasmModule>>>` + `Arc<Semaphore>` — pool size controls parallelism, semaphore prevents queue exhaustion.
27. **Touring-cognitive Excellence** (v28.0.0): 15 strategies (S1-S15) implemented — TfIdf vectorizer, AdaptiveEngine for runtime optimization, Nexus integration hub, SqliteGraphStore persistent backend, confidence calibration, goal decomposition, session analytics. 7 new modules, +96 tests.
28. **Wiring Intelligence System** (v28.0.0): 6-layer architecture (Signal→Tracker→Cascade→RL→Cortex→Feedback) connecting pre-edit signals, post-edit tracking, file-changed cascade invalidation, RL reward feedback, H83 cortex handler, and closed-loop learning. SCHEMA_VERSION 5→6 adds `wiring_map` + `module_ecosystem` tables. 4 new modules, +56 tests.
29. **Cortex cross-audit** (v28.0.0): 7 integration gaps resolved between touring-cognitive and touring-cortex — TfIdf→Nexus wiring, AdaptiveEngine→Runtime connection, SqliteGraphStore persistence. +30 tests.
30. **Edit/Write Excellence** (v29.0.0): All 5 edit/write hooks (pre_edit, pre_write, pre_edit_prevention, post_edit, post_write) use scored signal ranking `Vec<(f32, String)>` sorted by priority and CILA-budget-truncated (L0-L1=1200, L2-L3=3000, L4+=6000 chars). Pre-hooks compute signals in parallel via `rayon::join`. Post-hooks return `HookResponse` with quality verification feedback enabling edit-verify-fix cycles. Multi-language anti-pattern detection covers Rust, Python, TypeScript, JavaScript, Go, C/C++, Java. Speculative validation (`speculate_v2`) runs in all 5 hooks.
31. **Hook Wiring Sprint** (v29.1.0): 8 gaps between `settings.json` hooks and daemon handlers corrected — SessionStart/SessionEnd, PostToolUse[Read/Bash/*], PreToolUse[Read/pre-edit-prevention], PreCompact. SubagentStop gate v4 uses transcript-based `tool_use` detection instead of text matching, eliminating false positives for pure TACO subagents.
32. **Layer7 Co-Edit Prediction** (v29.4.0 Sprint 2): `PredictionLayer` receives 3 input streams — (1) `record_edit()` in post_edit + post_write for session sequence, (2) `record_co_edit(prev, current)` via `RefCell<Option<String>> last_edited_file` in `InfraRuntime` for co-edit graph, (3) `update_file_heat()` from pheromone bus. `predict_next()` called in pre_read for anticipatory context injection. Interior mutability via `RefCell` avoids `&mut HookRuntime` borrow conflicts.
33. **shared/ Module** (v29.4.0 Sprint 3): `crates/touring-hooks/src/shared/` centralizes 3 utilities previously duplicated across 5 hooks — `detect_language` (unified language detection from path, 22 callsites), `quality` (measure_quality_snapshot + is_test_file), `reindex` (reindex_file). Single source of truth eliminates divergence risk. `speculate_v2` fast-path in `shadow_v2.rs` validates Rust/Python/TypeScript/JavaScript via tree-sitter before invoking slow external linters (2-10s → <200ms). TSC output parser uses byte-safe `.get()` slicing to avoid UTF-8 panic on non-ASCII filenames.
34. **RL Quality Score Composite** (v29.4.0 Sprint 3): `ImmediateReward.quality_score` in `post_tool_rl.rs` now aggregates 3 signals: `context_utility_bonus` (S7 context injection hit rate), `aco_quality_bonus` (TrackerReport ACO pheromone × 0.1), `gotcha_bonus` (prevented_errors × 0.01, capped 0.1). This closes the full RL loop: edit quality → reward → LinUCB update → better context injection.
35. **HookEventMetrics** (v29.4.0 Sprint 3): `daemon.rs` maintains a static `OnceLock<HashMap<&str, (AtomicU64, AtomicU64)>>` tracking `(invocations, total_latency_ms)` per hook. The `__health__` endpoint exposes `hook_metrics` JSON with `{invocations, avg_latency_ms}` per hook (zero-invocation hooks omitted). Enables production observability of hook performance without external monitoring.

---

## 2. Workspace Topology

```
.claude/rust/  (Cargo Workspace, resolver = "2")
│
├── Cargo.toml                    [workspace manifest + shared deps]
│
└── crates/
    │
    ├── touring-core/             [FOUNDATION — no internal deps]
    │   ├── src/error.rs          TouringError (20 variants, merged superset)
    │   ├── src/config.rs         TouringConfig (15 fields, env + file-based)
    │   ├── src/types.rs          MemoryTier (5), CILALevel (L0-L6)
    │   └── src/embedding/        EmbeddingClient (GPU:8200, feature-gated)
    │       875 LOC · 52 tests · 0 internal deps
    │
    ├── touring-simd/             [PERFORMANCE — SIMD + similarity + statistics]
    │   ├── src/simd_utils/       CPU detection, AVX2/NEON/scalar dispatch
    │   ├── src/similarity/       CosineComputer, JaccardComputer
    │   └── src/statistics/       WilsonRanker, DriftDetector, KS statistic
    │       1,335 LOC · 102 tests · deps: touring-core
    │
    ├── touring-learning/         [UNIFIED BRAIN — RL + Memory + Evolution + ACO]
    │   ├── src/rl/               QTable TD(λ) + **epsilon-greedy** (0.15→0.05 decay) + rkyv snapshot
    │   │   ├── risk_adjusted.rs  **NEW v13** RiskAdjustedQLearning — ε adapts to blast_radius
    │   │   └── burn_transformer.rs **NEW v14** ContextTransformer<B: Backend> — 19→64→64→8 Linear+ReLU
    │   │                            feature-gated (burn-transformer), backend=ndarray (CPU, CI-safe)
    │   ├── src/bandit/           LinUCB (8 arms, 19-dim) + **extract_features_rich** + **forced exploration** + rkyv snapshot
    │   │   ├── ast_features.rs   **NEW v13** extract_ast_features(file) → [f64; 16] (19→35D for internal use)
    │   │   ├── ast_enriched.rs   **NEW v13** AstEnrichedBandit — wraps LinUCB with AST context
    │   │   ├── transfer.rs       **NEW v14** TransferLinUCB — blend_weight = similarity × 0.30
    │   │   │                        transfer_from(donor, similarity) via export()/import() API
    │   │   └── reminder_bandit.rs **NEW v22** ReminderBandit — 7-arm LinUCB (6-dim: cila_level, corrections,
    │   │                            fill_ratio, success_rate, is_rust, session_turn); arms: None, VgpCheck,
    │   │                            MemoryRecall, SpeculateFirst, BlastRadius, TestValidation, CircuitBreaker;
    │   │                            REMINDER_ALPHA=1.2, NEGATIVE_REWARD=-0.3; Sherman-Morrison via insert_axis+broadcast
    │   ├── src/online_rl.rs      OnlineRLEngine: ImmediateReward.quality_score, EMA filter, forced_explore(interval=100)
    │   ├── src/online_learning/  **NEW v13** FtrlLayer (linfa-ftrl feature-gated), incremental online learning
    │   ├── src/memory/           RLM 5-tier, SemanticRecall (FTS5+cosine)
    │   │   ├── async_rlm.rs      **NEW v13** AsyncRlmMemory — Arc<RwLock> hot path + mpsc background SQLite writes
    │   │   └── crdt_graph.rs     **NEW v22** CrdtDelta — delta-CRDT sync: delta(self,other)=set difference,
    │   │                            merge_delta idempotent (tombstone semantics, LWW by updated_at),
    │   │                            full_delta for initial sync; serde on CrdtEdge+NodeWeight
    │   ├── src/evolution/        EvolutionAnalyzer, InsightEngine
    │   ├── src/ranking/          WilsonRanker + quality tiers, DriftDetector
    │   │   ├── drift_monitor.rs  **NEW v22** DriftMonitor KS two-sample test — VecDeque sliding windows,
    │   │   │                        D=max|F1-F2|, p-value via Kolmogorov series (k=20, clamped [0,1]),
    │   │   │                        None when <5 samples; DriftReport serde
    │   │   └── ebpf_observer.rs  **NEW v22** EbpfObserver — feature-gated stub (feature="ebpf", disabled by
    │   │                            default); try_load()→false, collect_metrics()→empty without feature
    │   ├── src/clustering/       Cosine threshold (default) | Leiden (feature)
    │   └── src/aco/              Models (14 structs + Pending/Running variants), DAG graph, GoalTracker 9×9, ESAA
    │   │   ├── diagnostics.rs    **NEW v18** DiagnosticLayer (Syntax/Logic/Contract/Architecture) + classify_error() + RetryStrategy
    │   │   └── evolution.rs      **NEW v18** populate_evolution_package() — patterns from Success, anti-patterns from Failed nodes
    │       ~16,300 LOC · 409 tests · deps: touring-core, touring-simd
    │
    ├── touring-ast/              [CODE INTELLIGENCE — tree-sitter]
    │   ├── src/languages.rs      Lang enum (11 languages)
    │   │                         **+ Go, Java** (feature-gated: `more-languages`) **NEW v14**
    │   ├── src/parser.rs         ParserPool (thread-safe), IncrementalParser LRU-128
    │   ├── src/symbols.rs        Symbol extraction, SymbolKind (18 variants), enriched fields
    │   ├── src/surgery.rs        Body replacement + validation
    │   ├── src/graph.rs          SymbolIndex, BlastRadius, imports
    │   │                         **+ detect_cycles()** (DFS coloring) **NEW v13**
    │   │                         **+ EnrichedBlastRadius, ImpactCategory, compute_enriched_blast_radius()** **NEW v18** severity = 0.5*d + 0.3*t + 0.2*c
    │   ├── src/document.rs       RopeDocument O(log N)
    │   │                         **+ byte_to_point_safe()** → AstResult<(usize,usize)> **NEW v13**
    │   ├── src/store.rs          SQLite SymbolStore persistence
    │   │                         **+ SymbolChangeSet, apply_change_set(), diff_symbols()** **NEW v13**
    │   ├── src/file_heat.rs      **NEW v18** FileHeat + HeatMap — digital pheromone system, heat_score = edits × decay × (1 + blast_weight)
    │   ├── src/semantic_search.rs **UPDATED v18** find_symbols_fused() — RRF over similarity + co_edit + blast_radius signals
    │   │                          index_symbol(), find_similar_symbols(threshold, limit), embed_symbol()
    │   │                          no touring-learning dep (avoids circular); standalone cosine similarity
    │   └── tests/property_tests.rs **NEW v14** 3 proptest properties:
    │                               rust_surgery_idempotent · blast_radius_monotone
    │                               incremental_parse_symbol_count_stable
    │       ~8,500 LOC · 286 tests · deps: touring-core
    │
    ├── touring-antt/              [NLP PIPELINE — regulatory text analysis]
    │   ├── src/monetary_parser   Brazilian currency extraction (R$)
    │   ├── src/keyword_matcher   Aho-Corasick multi-pattern
    │   ├── src/semantic_chunker  Boundary-aware text splitting
    │   ├── src/reranker          BM25 contextual reranking
    │   ├── src/cross_validator   Contradiction detection + confidence
    │   ├── src/rlm_integration   NLP ↔ unified Memory bridge
    │   └── src/search_index/     BM25 inverted index
    │       4,499 LOC · 80 tests · deps: touring-core, touring-simd, touring-learning
    │
    ├── touring-cognitive/        [PREDICTIVE ENGINE — semantic graph + cognitive loop]
    │   ├── src/cognitive_mcts.rs **NEW v18** CognitiveMCTS — MCTSEngine + SemanticGraph priors (neighbors=expand, relevance_score=reward)
    │   ├── src/refinement.rs     **NEW v18** RefinementCycle — diagnostic-driven retry, imports DiagnosticLayer from touring-learning
    │   ├── src/semantic_graph    StableGraph with focus tracking, **adaptive decay** (half_life = BASE*ln(1+access_count)) **UPDATED v18**
    │   ├── src/nexus             CognitiveNexus (coordinator)
    │   ├── src/session_predictor Bigram Markov-chain (2nd-order) + EMA Q-values
    │   ├── src/bridge            CognitiveRuntime + KnowledgeSource trait + EnrichedCtx
    │   ├── src/mcts              MCTS engine (UCT, discount-aware backup)
    │   ├── src/mcts_streaming    StreamingMCTS (background search, watch channel)
    │   ├── src/got               Graph of Thoughts, **multi-dimensional eval** (relevance/confidence/novelty) **UPDATED v18**
    │   │                         **+run_parallel_nodes(Vec<Arc<GotNode>>, msg) → Vec<ThoughtResult>** **NEW v22**
    │   │                         **+GoTEngine::evaluate_parallel(&self, node_ids, msg)** — JoinSet + Arc<GotNode>,
    │   │                         panic isolation per task, results sorted desc by score
    │   ├── src/snapshot.rs       **NEW v22** GoTSnapshot rkyv zero-copy — flat GotNodeSnapshot (no Arc/Box<dyn>),
    │   │                         #[archive(check_bytes)], from_engine(), to_bytes()/from_bytes(), to_json() serde fallback
    │   ├── src/session_persistence.rs **NEW v22** SessionPersistence deadpool-sqlite — async pool,
    │   │                         schema: got_sessions (session_id PK, snapshot BLOB, timestamps),
    │   │                         save_snapshot/load_snapshot/list_sessions/delete_session via conn.interact()
    │   ├── src/focus_cache       RwLock LRU-16 + predictive prefetch
    │   ├── src/persistence       GraphSnapshot **+ transition_matrix + q_values with serde(default)** **UPDATED v18**
    │   ├── src/error             CognitiveError (thiserror, #[non_exhaustive], transparent)
    │   ├── src/coedit_predictor  CoEditPredictor (RRF fusion)
    │   ├── src/rl_bridge         RlBridge trait + UCB1 bonus
    │   └── src/predictor_task    Background tasks (tokio::spawn)
    │       ~6,400 LOC · 226 tests · deps: touring-core, touring-simd, touring-learning, touring-ast
    │
    ├── touring-server/           [MCP + CORTEX + HOOKS — the runtime]
    │   ├── src/main.rs           CLI entry (12 subcommands)
    │   ├── src/server.rs         TouringServer (21 MCP tools via rmcp)
    │   ├── src/tools/            8 tool implementation modules
    │   ├── src/hooks/            12 hook handlers (classifier, PII, neural)
    │   ├── src/cortex/           CortexRuntime (81 handlers: neural+enforce+intel+life+mcp_recommend+rules+lifecycle)
    │   ├── src/index/            Incremental symbol indexer + 2-tier cache
    │   ├── src/ingest/           JSONL watcher + batch parser
    │   ├── src/reasoning/        TaskDecomposer (DAG)
    │   ├── src/session/          SessionManager + persistence
    │   ├── src/output/           JSON + TOON formatters
    │   └── src/graph_service.rs  Focus tracking, context injection
    │       19,875 LOC · 444 tests · deps: ALL crates
    │
    └── touring-python/           [PyO3 BINDINGS — backward compat]
        ├── src/lib.rs            #[pymodule] claude_learning_kernel
        ├── src/aco_bindings.rs   15 symbols for rust_bridge.py
        ├── src/rl_bindings.rs    **NEW v13** 7 RL exports + FEATURE_DIM + NUM_ARMS constants
        └── src/ast_rl_bridge.rs  **NEW v13** compute_rl_state() — AST→RL state vector
            ~3,070 LOC · 214 tests · deps: touring-core...touring-antt
            **v17.0.0**: build via `maturin develop --release` | módulo Python: `claude_learning_kernel` | bindings: RL, AST, NLP, SIMD, ACO
```

---

## 3. Crate Catalog

| Crate | Type | LOC | Tests | Internal Deps | Purpose |
|-------|------|-----|-------|---------------|---------|
| **touring-core** | lib | 1,135 | 52 | — | Error types (22 variants, `#[non_exhaustive]`), config (env+file), shared types (`MemoryTier`/`CILALevel` with `Ord`, `TryFrom<u8>`, `const ALL`), embedding client (GPU-optional) |
| **touring-simd** | lib | 2,200 | 185 | core | SIMD dispatch (AVX2→NEON→scalar via compiler auto-vectorization 8-way), CosineComputer + JaccardComputer (`Similarity` trait), **TopKSearcher** O(n log k) partial sort, **distance metrics** (Euclidean/Manhattan/Pearson), **matrix ops** (mat-vec/relu/softmax), Wilson confidence, drift detection (KS O(n+m)), `has_simd()`, `simd_backend()`, `StressResult` serializable |
| **touring-hooks** | lib+bin+bin | ~11,500 | 725 | core, ast(simd-search,more-languages), learning, cognitive, simd, [rules], [nlp] | Neural hooks, bridges (cognitive, ast, aco, rules, nlp), daemon IPC, cli_handlers (57 hooks). **v28.12**: `reranked_context.rs` (RRF reranking via touring-antt::ContextualReranker for pre_read), `callgraph_enrichment.rs` (touring-ast::CallGraph for blast radius), `semantic_classifier.rs` (TF-IDF + SIMD cosine fallback), `pattern_bandit.rs` (TD(λ) pattern learning). |
| **touring-learning** | lib | ~16,800 | 608 | core, simd | QTable TD(λ), LinUCB, ACO, RLM 5-tier, Evolution, online_rl, ESAA. **v18**: DiagnosticLayer (Syntax/Logic/Contract/Architecture) + RetryStrategy (ACO-1), populate_evolution_package() (ACO-2), auto_decompose() by complexity (ACO-3), transition_status() 9-state machine + Pending/Running variants (ACO-4), compute_objective_hash() SHA-256 (ACO-5). `LearningError` `#[non_exhaustive]`. **v22**: ReminderBandit 7-arm (S0), CrdtDelta delta-sync (S6), DriftMonitor KS-test (S7), EbpfObserver feature-gated stub (S7). **v28.1**: `ndarray_mlp.rs` (ContextMlp 25→64→64→8 ReLU) substituindo burn_transformer — resolve burn↔rusqlite 0.38+ conflict. |
| **touring-ast** | lib | ~8,500 | 286 | core, [simd], ropey | tree-sitter (**14 langs** — Python, Rust, TypeScript, JavaScript, HTML, CSS, JSON, Bash, TOML, YAML, Markdown, Go, Java). Features: `simd-search` (SIMD SemanticSymbolIndex), `more-languages` (Go+Java), `async-pipeline`. **v28.12**: both features activated in hooks+server+cortex. |
| **touring-antt** | lib | ~5,000 | 88 | core, simd, learning | Monetary parser, keywords (Aho-Corasick), semantic chunker, BM25 reranker, code tokenizer, **financial_analysis** (v28.12: NPV/IRR/stress via touring-simd::financial, ConcessionAnalysis, viability_verdict) |
| **touring-cognitive** | lib | ~7,200 | 374 | core, simd, learning, ast, antt | SemanticGraph (SIMD cosine + AnnIndex IVF-flat), SessionPredictor, StreamingMCTS, FocusCache, HybridCognitiveEngine. **v28.12**: `AnnIndex` wired into SemanticGraph (`rebuild_ann_index()` + `retrieve_by_embedding_ann()` — O(N/K*P) vs O(N) linear scan). |
| **touring-server** | bin | ~22,400 | 500 | hooks + ALL | MCP server (26 tools via rmcp), cortex (**97 handlers** H1-H97), 28 HookEvent variants, cache v2.1 (moka TinyLFU), JSON/TOON formatters, symbol indexer, graph service. **v28.12**: `wasm-plugins` enabled by default, `more-languages` (Go+Java), `simd-search`, `simd-similarity`, `smart-cache` features activated. |
| **touring-rules** | lib | 417 | 27 | — | Full rules engine (zen-engine) + 3 JDM decision tables. Integrated into touring-hooks via `rules_bridge` and touring-server via H75-H76. |
| **touring-index** | lib | ~2,800 | 120 | core, ast, [simd], [learning] | File cache, incremental indexing, file watcher. **v28.12**: `similarity.rs` (feature `simd-similarity` — FileSimilarityIndex SIMD cosine 8-dim), `smart_cache.rs` (feature `smart-cache` — SmartCachePriority LinUCB bandit). |
| **touring-cortex** | lib | ~4,500 | 836 | core, ast, learning, cognitive, **simd**, wasm, antt, rules | **97 handlers** (H1-H97). RRF fusion, CallGraph Tarjan SCC, FilterCache LRU, tiktoken budget, Zstd compression, EmbeddingIndex. **v28.12**: `signal_fusion.rs` (Bayesian fusion via touring-simd::reconciliation), `dspy/` module (DSPy-inspired prompt compilation), H99 MCTSCodeSynthesis fixed, H100 DSPyIntegration fixed. |
| **touring-wasm** | lib | ~2,180 | 183 | core, **simd** | wasmtime sandboxed plugin runner: fuel metering (10M), stack limit (1MB), import allowlist. **v22**: `InferletPool` (Arc<Mutex<VecDeque<WasmModule>>> + Arc<Semaphore>), `TypedPluginContext/Result` (evaluate_scored 0–100 + binary fallback, success=score≥50). **v24.1**: `KvCacheManager` trait (Send+Sync), `InMemoryCacheManager` (RwLock HashMap), `WasmCacheManager` delegation + pooling config, `fast_instantiation_config()` (PoolingAllocationConfig + memory_init_cow), feature `pooling-allocator`. **v28.2**: **async-native rewrite** — `async_support(true)` Config, `call_evaluate_async()` spawn_blocking, `AsyncInferletPool` (InstancePre pre-linking, Mutex<VecDeque>, spawn_blocking executor, concurrent safe), `AsyncKvCacheManager` trait + `AsyncInMemoryCacheManager` (tokio::sync::RwLock), **+80 lib tests**. **v28.3**: **simd_embedding** — `EmbeddingSearch` + `EmbeddingDoc` + `compute` utilities (cosine_similarity, squared_distance, batch_similarity, top_k_indices). **v28.13**: InferletService integrated into touring-hooks, `AsyncInferletPool` with real WASM evaluation (memory write + set_input + evaluate dispatch), 4 inferlets in 1 binary (65KB wasm32-unknown-unknown), `strip_inferlet_key` manual JSON parsing (no serde_json ~40KB overhead), 11 E2E tests passing. |
| **touring-python** | cdylib | ~3,500 | 214 | core, simd, learning, ast, antt, cognitive, rules | PyO3 bindings **v4.0.0**: ACO + AST + NLP + SIMD + RL + AST-RL bridge + **cognitive** (MCTS search) + **rules** (JDM evaluate_inline, evaluate_model, list_models) + **financial** (NPV, IRR, stress scenarios, concession analysis). 9 binding modules, 5 custom exceptions. |
| **TOTAL** | | **~110,000** | **3,851** | | |

---

## 4. Dependency Graph

```
                touring-python (cdylib)      touring-server (bin)
                    │ │ │ │ │                    │ │ │ │ │ │
                    │ │ │ │ └── touring-antt ─────┘ │ │ │ │ │
                    │ │ │ │                         │ │ │ │ │
                    │ │ │ └──── touring-ast ────────┘ │ │ │ │
                    │ │ │                              │ │ │ │
                    │ │ │       touring-cognitive ─────┘ │ │ │
                    │ │ │           │   │   │            │ │ │
                    │ │ │           │   │   └─ touring-simd (v15: SIMD cosine)
                    │ │ │           │   │                │ │ │
                    │ │ │           │   │   touring-hooks ┘ │ │  ← v15: hooks→cognitive
                    │ │ │           │   │       │   │   │   │ │
                    │ │ └────── touring-learning ┘   │   │   │ │
                    │ │              │                │   │   │ │
                    │ └────────  touring-simd ────────┘   │   │ │
                    │                │                     │   │ │
                    └──────────  touring-core ─────────────┘───┘─┘
                                  (foundation)
```

**Properties**:
- Strict DAG (enforced by Cargo — circular deps = compile error)
- `touring-core` is the sole root (no internal dependencies)
- `touring-server` and `touring-python` are leaves (no dependents)
- **v15.0.0**: `touring-hooks` now depends on `touring-cognitive` (cognitive loop closure)
- **v15.0.0**: `touring-cognitive` now depends on `touring-simd` (SIMD cosine)
- **v16.0.0**: `touring-hooks` optionally depends on `touring-rules` (feature `rules-engine`) and `touring-antt` (feature `nlp-enrichment`)
- **v16.0.0**: `touring-server` depends on `touring-rules` (H75 RulesContextRouter, H76 RulesHealthMonitor)
- **v16.0.0**: No isolated crates — every crate is connected to at least one other
- Maximum depth: 5 (core → simd → learning → cognitive → hooks → server)
- **v18.0.0**: `touring-cognitive` now imports `DiagnosticLayer` from `touring-learning` (COG-2 RefinementCycle cross-crate bridge)
- **v22.0.0**: `touring-cognitive` adds `deadpool-sqlite` dep for `SessionPersistence`; `touring-wasm` adds `tokio` Semaphore dep for `InferletPool`; no new crate-level edges added (all within existing dependency bounds)

---

## 5. Dual-Mode Operation (preserved from v3.0.0)

The touring binary operates in two mutually exclusive modes:

### MCP Server Mode (`touring serve`)
- Long-running process, tokio async runtime
- Exposes 21 tools via rmcp SDK (`#[tool_router]` macro)
- Shared state via `Arc<Mutex<...>>` (17 subsystems)
- GraphService injects `graph_ctx` into every tool response
- CognitiveNexus injects predictive context for high-complexity tools

### Neural Hook Mode (`touring <subcommand>` / `touring cortex <event>`)
- Short-lived CLI invocations (< 15ms target)
- Synchronous, no tokio runtime
- SQLite WAL mode for concurrent reads
- Never blocks the user (exit 0 even on internal error)
- 20 cortex events + 8 direct hook subcommands + 2 stateless hooks

### CLI Subcommands (12 total)

| Subcommand | Mode | Event |
|-----------|------|-------|
| `serve` | MCP Server | — |
| `cortex <event>` | Hook | All 20 Claude Code events |
| `classify-intent` | Hook (stateless) | UserPromptSubmit |
| `scan-pii` | Hook (stateless) | PreToolUse |
| `pre-read` | Hook | PreToolUse Read |
| `post-read` | Hook | PostToolUse Read |
| `pre-bash` | Hook | PreToolUse Bash |
| `post-bash` | Hook | PostToolUse Bash |
| `pre-edit` | Hook | PreToolUse Edit/Write |
| `post-edit` | Hook | PostToolUse Edit/Write |
| `session-start` | Hook | SessionStart |
| `session-stop` | Hook | Stop |

---

## 6. Subsystem: touring-core

**Purpose**: Foundation types shared by all crates. Zero business logic.

### TouringError (20 variants)
Merged superset from both original codebases:
- I/O: `Io`, `Sqlite`, `Json`, `Config`, `Embedding`
- Domain: `InvalidDimensions`, `EmptyInput`, `NumericalError`, `ClusteringError`
- Lifecycle: `MemoryLimitExceeded`, `InvalidParameter`, `StateNotFound`, `IndexOutOfBounds`
- Server: `Mcp`, `AstValidation`, `SymbolNotFound`, `NotImplemented`, `Internal`

### MemoryTier (5 variants)
Unified from rust-core's 4-tier + touring's 4-tier → 5-tier superset:

| Tier | TTL | Origin | Use case |
|------|-----|--------|----------|
| Reflexive | 1 min | touring's "ephemeral" | Instant context, auto-evict |
| Working | 1 hour | NEW | Current task context |
| Session | 8 hours | rust-core's "Session" | Session lifespan |
| Project | 30 days | both | Cross-session persistence |
| Core | ∞ | both | Permanent knowledge |

### CILALevel (L0-L6)
Intent complexity classification:
- L0 Direct, L1 PAL, L2 Tool-Augmented, L3 Pipeline, L4 Agent Loop, L5 Self-Modifying, L6 Multi-Agent

### EmbeddingClient (feature-gated: `gpu-embeddings`)
- HTTP client to `localhost:8200` (BAAI/bge-m3, 384-dim)
- Batch embedding with backpressure
- Zero-vector fallback when GPU unavailable

---

## 7. Subsystem: touring-simd

**Purpose**: CPU-optimized numerical operations. No business logic.

- **SIMD dispatch**: AVX2 → NEON → scalar (runtime detection) via compiler auto-vectorization (8-way unrolling)
- **Similarity**: CosineComputer (SIMD-accelerated), JaccardComputer, **TopKSearcher** (O(n log k) partial sort)
- **Distance**: Euclidean, Squared Euclidean, Manhattan, Pearson correlation, Dot product
- **Matrix**: mat-vec multiplication (sync + parallel), ReLU, softmax, row normalization
- **Statistics**: WilsonRanker (confidence intervals), DriftDetector (KS statistic), descriptive stats

### 7.1 New Modules (v28.3)

| Module | Purpose | Key Functions |
|--------|---------|---------------|
| `similarity/topk.rs` | Top-K nearest neighbor search | `top_k()`, `top_k_batch()` |
| `similarity/distance.rs` | Distance metrics (SIMD) | `euclidean()`, `manhattan()`, `pearson_correlation()`, `squared_euclidean()` |
| `simd_utils/matrix.rs` | Matrix operations | `mat_vec_mul()`, `mat_vec_mul_par()`, `relu()`, `softmax()`, `normalize_rows()` |
| `simd_utils/portable.rs` | Portable SIMD primitives | `portable_dot_f32()`, `portable_norm_f32()`, `portable_sqeuclidean_f32()` |

---

## 8. Subsystem: touring-learning (THE UNIFIED BRAIN)

**Purpose**: Single learning system for all interfaces (MCP, PyO3, hooks).

### 8.1 QTable (rl/)
- TD(λ) Q-learning with eligibility traces
- State: u64, Action: u64, Q-values: f64
- γ=0.99, α=0.1, λ=0.9
- Persistence: SQLite save/load

### 8.2 Memory (memory/)
- **RLM**: 5-tier storage with TTL-based eviction
- **SemanticRecall**: Hybrid FTS5 (keyword) + cosine (semantic) search
- **WorkingMemory**: In-process LRU cache (L0, fast path) with **SIMD-accelerated similarity search** (v28.3)
  - `find_topk(query, k)` — top-K via `TopKSearcher` (SIMD cosine)
  - `find_by_distance(query, k)` — Euclidean distance ranking
  - `find_similar_topk(query, k)` — trait method for WorkingMemory interface
- **rle.rs** (v24.1): RLE codecs for `CrdtDelta` field compression — `encode_u64`, `encode_u64_pair`, `encode_str` and decoders

### 8.3 Evolution (evolution/)
- **EvolutionAnalyzer**: Loads JSONL compliance records, computes tool effectiveness
- **InsightEngine**: 2D heatmaps (time × category), pattern extraction
- **Persistence**: Checkpoint/restore across sessions

### 8.4 Ranking (ranking/)
- **WilsonRanker**: 95% CI for tool effectiveness ranking
- **DriftDetector**: Temporal + categorical pattern degradation

### 8.5 Clustering (clustering/)
- **Cosine** (default): Simple threshold-based skill clustering
- **Leiden** (feature `leiden-clustering`): Modularity-optimal, requires linfa

### 8.7 NEW v13.0.0 — AST-Enriched RL State

**`bandit/ast_features.rs`**: `extract_ast_features(file_path) → [f64; 16]` — 16 AST-derived features appended to base 19D vector for internal 35D LinUCB (`FEATURE_DIM_AST=35`). Features include `symbol_density`, `avg_complexity`, `max_complexity`, `has_async`, `is_test_file`, `public_api_ratio`, `blast_radius` (normalized), `doc_coverage`, `error_handler_density`, `import_count`, plus language-specific dims.

**`bandit/ast_enriched.rs`**: `AstEnrichedBandit` wraps `LinUCBBandit` — automatically fetches AST features before arm selection when file context is available.

### 8.8 NEW v13.0.0 — Risk-Adjusted Exploration

**`rl/risk_adjusted.rs`**: `RiskAdjustedQLearning { qtable: QTable, risk_threshold: f64 }`. Method `epsilon_with_risk(state, blast_radius)` returns reduced ε when `blast_radius > risk_threshold` (exploit known good action before risky edits) and normal ε otherwise (explore freely on low-impact files).

### 8.9 NEW v13.0.0 — Online Learning (FTRL)

**`online_learning/ftrl.rs`** (feature-gated: `ftrl`): `FtrlLayer { model: Option<Ftrl>, params: FtrlParams }`. `update(features, reward) → f64` — incremental FTRL-Proximal update via `linfa-ftrl`. Feature importance learning without full model rebuild. Enabled via `[features] ftrl = ["linfa", "linfa-ftrl"]`.

### 8.10 NEW v13.0.0 — Async Persistence

**`memory/async_rlm.rs`**: `AsyncRlmMemory { inner: Arc<RwLock<RlmMemory>>, write_tx: mpsc::UnboundedSender<WriteOp> }`. `store()` writes to in-memory LRU immediately + sends `WriteOp` to background tokio task for SQLite persistence. `flush()` awaits all pending writes. Reduces hook latency by decoupling reads (sync, <1ms) from writes (async, background).

### 8.11 NEW v22.0.0 — ReminderBandit (S0)

**`bandit/reminder_bandit.rs`**: `ReminderBandit` — 7-arm LinUCB for system reminder selection. Feature vector (6-dim): `cila_level` (normalized 0–1), `recent_corrections` (bool→f64), `context_fill_ratio`, `tool_success_rate`, `is_rust_task` (bool→f64), `session_turn` (log-normalized). Arms: None (0), VgpCheck (1), MemoryRecall (2), SpeculateFirst (3), BlastRadius (4), TestValidation (5), CircuitBreaker (6). `REMINDER_ALPHA=1.2`, `NEGATIVE_REWARD=-0.3`. Sherman-Morrison A⁻¹ update via `insert_axis` + broadcast (avoids deprecated `into_shape` and `indexing_slicing` lint). All arms must be primed before preference tests (LinUCB exploration invariant: untouched arms permanently score α·‖x‖ ≈ 1.63).

### 8.12 NEW v22.0.0 — CrdtDelta (S6)

**`memory/crdt_graph.rs`** extended: `CrdtDelta { added_nodes, removed_nodes, added_edges, removed_edges, updated_weights, timestamp: u64, source_id: String }`. Methods on `CrdtSemanticGraph`: `delta(self, other) → CrdtDelta` (set difference — what self has that other doesn't), `merge_delta(delta)` (idempotent — tombstone semantics, LWW by `updated_at`), `full_delta(source_id) → CrdtDelta` (for initial sync). Enables multi-agent convergence without locking. **v24.1**: `rle_encoded_size()`, `naive_byte_size()`, `rle_compression_ratio()` — RLE compression analysis for delta fields.

### 8.13 NEW v22.0.0 — DriftMonitor + EbpfObserver (S7)

**`ranking/drift_monitor.rs`**: `DriftMonitor` — KS two-sample test over sliding `VecDeque<f64>` windows. `D = max|F1(x) - F2(x)|` over combined sorted samples. P-value: `λ = (√n_eff + 0.12 + 0.11/√n_eff)·D`; `P ≈ 2·Σ_{k=1}^{20} (-1)^{k+1}·exp(-2k²λ²)`, clamped [0,1]. `test()` returns `None` when either window has < 5 samples. `DriftReport` is serde-serializable.

**`ranking/ebpf_observer.rs`**: `EbpfObserver` — compile-time feature gate `cfg(feature = "ebpf")`. Without feature: `try_load()→false`, `collect_metrics()→vec![]`. With feature: stub for real eBPF program loading. Feature `ebpf = []` in Cargo.toml, disabled by default in CI.

### 8.14 NEW v24.1.0 — RingBuffer Observability

**`observability/ring_buffer.rs`** (NEW): `RingBuffer<T>` — fixed-capacity circular buffer with FIFO overwrite when full. `RingBufferIter<'a, T>` iterates oldest→newest via physical index `(head + index) % capacity`. NOT thread-safe — caller must ensure exclusive access. Clippy `indexing_slicing` violations resolved via `get_unchecked` with SAFETY comments. Exports: `RingBuffer`, re-exported from `touring-learning`. 7 tests.

### 8.15 NEW v24.1.0 — RLE Encoding

**`memory/rle.rs`** (NEW): Run-Length Encoding for `CrdtDelta` field compression. Codecs: `encode_u64`/`decode_u64` (12 bytes/run: 4 count + 8 value), `encode_u64_pair`/`decode_u64_pair` (20 bytes/run: 4 + 8 + 8), `encode_str`/`decode_str` (length-prefixed strings). `compression_ratio_u64()` returns uncompressed/compressed ratio. CrdtDelta methods: `rle_encoded_size()`, `naive_byte_size()`, `rle_compression_ratio()` — ratio > 1.0 means RLE saves space. 16 tests. Byte layout verified: u64 run = 12 bytes (4+8), u64 pair run = 20 bytes (4+8+8).

### 8.16 NEW v30.3.0 — PLN2 P4 Meta-Optimization (2026-04-09)

#### P4.1 — Agent Diary System
`crates/touring-server/src/cli/diary.rs` + `crates/touring-server/src/agent_diary.rs`

- **Key Hierarchy**: `wing_{agent}/diary/{meta,entries/{ts},topics/{topic}}`
- **AAAK Dialect** (Adaptive Abbreviated Agent Knowledge): `#P` phase | `#R` result/score | `#L` lesson | `#W` warning | `#E` error
- **CLI** (direct, no daemon socket — `MemoryStore::new()` bypassing daemon):
  - `touring diary write <agent> <entry> [--topic <topic>] [--aaak]`
  - `touring diary read <agent> [--last N] [--topic <topic>]`
  - `touring diary list` | `touring diary meta <agent>`
- **Schema isolation**: CLI uses `memory.db` (new, `last_accessed_at TEXT`) directly — daemon uses `rlm_memory.db` (legacy, `accessed_at INTEGER`)
- **E2E**: 8/8 PASS (write_and_read, aaak_markers, topic_filter, last_n, meta_after_write, write_exit_code, multiple_entries_ordered, no_diary_status)
- **Meta fallback**: If `wing_{agent}/diary/meta` missing → entry-based verification via `diary_read`

#### P4.2 — Palace Hierarchy Memory
`crates/touring-learning/src/memory/rlm.rs` (lines 179-204) + `crates/touring-server/src/memory_store.rs`

- **Schema migration** (idempotent):
  ```sql
  ALTER TABLE memory_entries ADD COLUMN palace_path TEXT
  CREATE INDEX idx_memory_palace_path ON memory_entries(palace_path) WHERE palace_path IS NOT NULL
  ```
- **Palace Path**: `wing_{name}/room_{name}/closet_{name}/drawer_{name}` (4-level hierarchy)
- **API**: `store_with_palace()`, `query_by_palace()`
- **Integration**: `MemoryStore::store_with_palace()` + `query_by_palace()`

#### P4.3 — Evolution Drift Self-Correction
`crates/touring-hooks/src/cli_handlers_evolution.rs`

| Alert Level | Trigger | Action |
|-------------|---------|--------|
| `none` | No degradation | — |
| `degraded` | bash_success < 0.8 OR edit spike | `inject_reward("evolution:drift_detected", severity)` |
| `structural` | 3+ metrics degrading | `tracing::warn!` + RL injection |

- **Self-correction**: `runtime.learning.inject_reward("evolution:drift_detected", drift_severity, "evolution_drift")`
- **Output schema**: `{detected, alert_level, self_correction_applied, degrading_metrics, summary{bash_success_rate, edit_trend_pct, gotchas_with_hits}}`

#### P4.4 — AutoSaveHook
`crates/touring-hooks/src/auto_save_hook.rs` + `post_tool_rl.rs:177`

- **Config**: `exchange_count` (current) + `interval` (default: 15) + `last_save_ts`
- **Wiring**: `post_tool_rl.rs:177` — `increment_exchange()` → `run_auto_save()` every `interval` exchanges
- **Checkpoint format**: `{session_id, timestamp, exchange_count, state_snapshot}`

### 8.6 ACO (aco/) — v18.0.0 Enhanced

Agentic Code Orchestrator internals — Rust acceleration layer for TACO N₂:
- **models.rs**: 6 enums + 14 structs (frozen, serializable) + **`Display` impls for all enums** (v12.0.0) + **`Pending` and `Running` variants** added to `ExecutionStatus` (v18.0.0 ACO-4)
- **graph.rs**: DAG over petgraph — Kahn topo sort, critical path, parallel levels + **execution status tracking** (`mark_executed`, `is_ready_to_execute`, `ready_nodes`) + **`iter()` iterator pattern** (v12.0.0) + **`auto_decompose()`** (Trivial=1, Moderate=2, High=4, Critical=6 sub-nodes) + **`transition_status()`** (9 legal transitions, `GraphError::InvalidTransition`) + **`execution_report()`** + **`compute_objective_hash()`** SHA-256 over BTreeMap + **`verify_invariant()`** (v18.0.0 ACO-3/4/5)
- **tracker.rs**: 9×9 GoalTracker — weighted composite, VETO/HALT/PASS + **`TrackerReport::summary()`** method (v12.0.0)
- **esaa.rs**: QueryCache (LRU+TTL, thread-safe) + EventBuffer (write-ahead batch) + rkyv zero-copy reader + **concurrent access tests** (v12.0.0)
- **diagnostics.rs** (NEW v18.0.0 ACO-1): `DiagnosticLayer` enum (Syntax/Logic/Contract/Architecture), `DiagnosticResult`, `RetryStrategy` (AutoFix/RePlan/EscalateToUser/Halt), `classify_error()`, `suggest_retry_strategy()` — 6 tests
- **evolution.rs** (NEW v18.0.0 ACO-2): `populate_evolution_package()`, `extract_learned_patterns()`, `discover_anti_patterns()`, `propose_system_upgrades()` — patterns extracted from Success nodes, anti-patterns from Failed nodes — 7 tests
- **Tests**: 409 total (+24 new v18: auto_decompose complexity levels, transition_status 9-state machine, objective hash determinism, diagnostics classification, evolution pattern extraction)

---

## 9. Subsystem: touring-ast — v13.0.0 Enhanced

**Purpose**: Language-aware code intelligence with enriched symbol metadata.

### 9.1 Core Capabilities

- **Languages**: Python, Rust, TypeScript, JavaScript, Bash, HTML, CSS, Markdown, JSON, TOML, YAML (11 languages via tree-sitter)
- **ParserPool**: Thread-local parser management (zero mutex contention)
- **Symbol extraction**: Functions, classes, methods, structs, enums, impls, type aliases, namespaces
- **Surgery**: Replace symbol body with syntax validation
- **SymbolIndex**: Dependency graph (imports, blast radius BFS)
- **SymbolStore**: SQLite persistence (symbols.db, WAL mode)
- **IncrementalPipeline**: RopeDocument + IncrementalParser + SymbolStore pipeline
- **Batch extraction** (v12.0.0): `extract_symbols_batch()` — parallel multi-file extraction via rayon
- **Filtering utilities** (v12.0.0): `filter_by_kind()`, `filter_by_complexity()`, `find_by_name()`
- **`SymbolKind::FromStr`** (v12.0.0): Parse strings into `SymbolKind` enum (`"function".parse::<SymbolKind>()`)

### 9.2 Symbol Model (v11.0.0 — Enriched)

**`SymbolKind` enum** (18 variants, type-safe):
Function, AsyncFunction, Method, Class, Struct, Enum, Trait, Impl, Interface,
TypeAlias, Namespace, Constant, Static, Variable, Module, Macro, Generator, Other.

Predicates: `is_callable()`, `is_type_definition()`, `is_container()`.
Serde: transparent string serialization (backward compat with JSON consumers).
Comparison: `PartialEq<&str>` allows `s.kind == "function"` in existing code.

**`Symbol` struct** enriched fields (all backward-compat with `#[serde(default)]`):

| Field | Type | Source | Purpose |
|-------|------|--------|---------|
| `parent_name` | `Option<String>` | Tree walk up to container | "MyClass" for methods |
| `docstring` | `Option<String>` | Python `"""`, Rust `///`, JSDoc `/**` | First line, 120 chars |
| `decorators` | `Vec<String>` | Python `@`, Rust `#[...]`, TS/JS `@` | Decorator names |
| `complexity` | `Option<u16>` | `complexity.rs` engine | Cyclomatic complexity |
| `is_async` | `bool` | `async fn`/`async def` detection | Async awareness |
| `visibility` | `Option<Visibility>` | Syntax analysis | Public/Private/Protected/Crate/Module |

**`Visibility` enum**: Public, Private, Protected, Crate, Module.

### 9.3 Complexity Engine (complexity.rs — NEW in v11.0.0)

Cyclomatic complexity via tree-sitter AST walk. Formula: `CC = 1 + Σ(decision_points)`.

| Language | Decision Points Counted |
|----------|------------------------|
| Python | if, elif, for, while, try, except, with, assert, and/or, ternary, comprehensions |
| Rust | if, else, for, while, loop, match, match_arm, &&/\|\|, ?, closure |
| TypeScript/JS | if, else, for, for_in, while, do, switch_case, catch, ternary, &&/\|\|, ?. |

Public API:
- `compute_complexity(source, lang, start_byte, end_byte)` — per-symbol
- `compute_complexity_for_source(source, lang)` — all callables
- `enrich_symbols_with_complexity(&mut symbols, source, lang)` — in-place enrichment

### 9.4 Module Structure

```
touring-ast/src/
├── lib.rs                    (exports)
├── error.rs                  (AstError, AstResult)
├── languages.rs              (Lang enum, tree-sitter language loading)
├── parser.rs                 (ParserPool [thread-local], IncrementalParser [LRU 128])
├── symbols.rs                (SymbolKind, Visibility, Symbol, extract_symbols)
├── complexity.rs             (cyclomatic complexity engine — NEW v11.0.0)
├── document.rs               (RopeDocument — O(log N) edits)
├── store.rs                  (SymbolStore — SQLite WAL)
├── graph.rs                  (SymbolIndex, BlastRadius, ImportInfo, extract_imports)
├── surgery.rs                (replace_symbol_body, validate_syntax)
├── incremental_pipeline.rs   (IncrementalPipeline, IncrementalEditResult)
└── queries/
    ├── python.scm / rust.scm / typescript.scm / javascript.scm  (symbol queries)
    ├── python_imports.scm / rust_imports.scm / ts_imports.scm   (import queries)
    └── js_imports.scm
```

### 9.5 VGP Integration (Verified Generation Protocol)

The VGP system (`scripts/vgp/`) uses touring-ast's tree-sitter parsers as its
primary extraction backend for zero-hallucination code generation.

### 9.6 NEW v13.0.0 — Safe byte_to_point

**`document.rs`**: `byte_to_point_safe(byte_idx: usize) → AstResult<(usize, usize)>` replaces the previously panicking `byte_to_point`. Returns `Err(AstError::IndexOutOfBounds)` instead of panicking on invalid byte offsets. The original `byte_to_point` kept for internal callers with pre-validated inputs.

### 9.7 NEW v13.0.0 — Change-Set Based Symbol Updates

**`store.rs`**: Three new methods on `SymbolStore`:

| Method | Signature | Description |
|--------|-----------|-------------|
| `diff_symbols` | `(old: &[Symbol], new: &[Symbol]) → SymbolChangeSet` | Computes minimal diff |
| `apply_change_set` | `(&self, changes: &SymbolChangeSet) → Result<()>` | Atomic apply with explicit ROLLBACK on error |
| `SymbolChangeSet` | `{ added, removed, modified: Vec<Symbol> }` | Minimal diff struct |

The `apply_change_set` uses `BEGIN IMMEDIATE` + closure pattern + explicit `ROLLBACK` on error (fixes previous missing ROLLBACK gap). Up to 50-100x faster than full file re-index for incremental edits.

### 9.8 NEW v13.0.0 — Cycle Detection

**`graph.rs`**: `SymbolIndex::detect_cycles() → Vec<Vec<String>>` — DFS with 3-color marking (White→Gray→Black). Returns each cycle as a `Vec<String>` of file paths. Used by touring-server to warn on circular import graphs before AST surgery.

| VGP Step | touring-ast Feature Used | Purpose |
|----------|------------------------|---------|
| V1 EXTRACT | `extract_symbols()` via tree-sitter Python bindings | Extract struct schemas from .rs/.py/.ts source |
| V2 VERIFY | `SymbolIndex` field lookup | Verify each referenced field exists |
| V3 IMPACT | `BlastRadius` (graph.rs) | Assess change impact (file_count, affected_files) |

See: `scripts/vgp/extractor.py`, `.claude/rules/vgp-protocol.md` (P97)

### 9.9 NEW v18.0.0 — FileHeat Pheromone System (AST-1)

**`file_heat.rs`**: `FileHeat { edits: u32, last_edit_epoch: u64, access_count: u32, blast_radius_weight: f32 }` + `HeatMap` (LRU-style with `evict_if_needed`, `decay_all`).

Formula: `heat_score = edits × recency_decay × (1 + blast_radius_weight)`

`DEFAULT_HALF_LIFE_SECS = 86400.0` (24h). Files with high edit frequency + large blast radius surface as "hot zones" — hooks inject heat context to prioritize risky files. 5 tests.

### 9.10 NEW v18.0.0 — RRF Fused Symbol Search (AST-2)

**`semantic_search.rs`**: `find_symbols_fused()` with Reciprocal Rank Fusion (RRF) over 3 signals:
1. `similarity` — cosine similarity (existing)
2. `co_edit_scores` — co-edit frequency from knowledge graph
3. `blast_radius_scores` — normalized blast radius weight

Formula: `score = Σ 1/(k + rank + 1)` where k=60 (standard RRF constant). Provides more robust symbol ranking than pure cosine similarity. 5 tests.

### 9.11 NEW v18.0.0 — Enriched Blast Radius (AST-3)

**`graph.rs`**: `ImpactCategory` enum (DirectDependents, TransitiveDependents, CoEdited), `EnrichedBlastRadius { base: BlastRadius, direct: Vec<String>, transitive: Vec<String>, co_edited: Vec<String> }`, `compute_enriched_blast_radius()`.

Severity formula: `severity = (0.5*d + 0.3*t + 0.2*c) / total` where d=direct, t=transitive, c=co_edited counts. Enables hooks to distinguish high-severity (many direct dependents) from low-severity (only co-edited) changes. 5 tests.

---

## 10. Subsystem: touring-antt

**Purpose**: Regulatory document text processing (ANTT — Agência Nacional de Transportes Terrestres). Renomeado de `touring-antt` em v21.1.0 para refletir o domínio real de aplicação.

| Module | Function |
|--------|----------|
| monetary_parser | Brazilian currency extraction (R$/USD/EUR) |
| keyword_matcher | Aho-Corasick multi-pattern (10.7x baseline) |
| semantic_chunker | Boundary-aware text splitting |
| reranker | BM25 contextual reranking |
| cross_validator | Contradiction detection + confidence propagation |
| rlm_integration | NLP ↔ touring-learning Memory bridge |
| search_index | BM25 inverted index for .claude/ docs |

---

## 11. Subsystem: touring-cognitive

**Purpose**: Predictive context engine — closed cognitive loop with touring-hooks.

### Core Components

| Module | Purpose | v15 Changes |
|--------|---------|-------------|
| **semantic_graph.rs** | StableGraph with DashMap index, temporal decay, attention-weighted retrieval | **SIMD TopKSearcher** (retrieve_by_embedding, retrieve_attention_weighted), **compact(max_nodes)** with scored removal |
| **ann_index.rs** | IVF-flat ANN index for approximate nearest neighbor search | **SIMD TopKSearcher** for centroid finding, **SIMD cosine** for re-scanning partitions |
| **tfidf.rs** | TF-IDF bag-of-words embedding vectorizer | **SIMD CosineComputer** for cosine similarity (replaces 14-line pure Rust) |
| **session_predictor.rs** | Markov-chain tool prediction + EMA Q-values | **Bigram transitions** (2nd-order: 0.4 unigram + 0.6 bigram blend) |
| **bridge.rs** | CognitiveRuntime + KnowledgeSource trait + EnrichedCtx | **prefetch_predicted()**, **knowledge_ref()** accessor |
| **focus_cache.rs** | LRU-16 memoization for graph context lookups | **Mutex → RwLock** for read concurrency, **prefetch()** for proactive warming |
| **mcts.rs** | Monte Carlo Tree Search (UCT, discount-aware backup) | — |
| **mcts_streaming.rs** | Background continuous MCTS search | **NEW v15**: watch channel, shutdown-on-drop |
| **got.rs** | Graph of Thoughts (concurrent heuristic eval) | — |
| **nexus.rs** | CognitiveNexus (coordinator: graph + predictor via tokio::join!) | — |
| **persistence.rs** | GraphPersistence (serde_json snapshots) | **CognitiveResult** return types (was String) |
| **error.rs** | CognitiveError enum | **NEW v15**: thiserror, #[non_exhaustive], #[error(transparent)] |
| **coedit_predictor.rs** | Reciprocal Rank Fusion for co-edit prediction | — |
| **rl_bridge.rs** | RlBridge trait + UCB1 bonus for MCTS | — |
| **predictor_task.rs** | Background tasks (tokio::spawn) | — |

### Cognitive Loop (v15.0.0)

```
Pre-hook (pre_read.rs)
  → CognitiveRuntime.knowledge_ref().file_risk() → risk score
  → CognitiveRuntime.knowledge_ref().gotchas_for_file() → gotchas
  → SessionPredictor.predict_top_k("Read", 2) → predicted next tool
  → Inject enriched context (risk + gotchas + predictions)
  → Claude executes tool
Post-hook (post_tool_rl.rs)
  → SessionPredictor.record(ToolInvocation) → unigram + bigram transitions
  → SemanticGraph.touch(file_path) → access_count + last_accessed
  → QTable + LinUCB update (existing RL)
Next pre-hook → predictions improved by accumulated data ↻
```

### KnowledgeSource Trait (bridge.rs:80-107)

| Method | Returns | Data Source |
|--------|---------|-------------|
| `file_relations()` | `Vec<FileRelation>` | SQLite file_relations table |
| `recent_bash_outcomes(limit)` | `Vec<BashOutcomeRecord>` | SQLite bash_outcomes table |
| `coedit_pairs()` | `Vec<CoEditPair>` | SQLite coedit_pairs table |
| `gotchas_for_file(path)` | `Vec<GotchaRecord>` | SQLite gotchas table |
| `recent_edits(limit)` | `Vec<EditRecord>` | SQLite edit_history table |
| `file_risk(path)` | `FileRisk` | Computed from failures + gotchas + deps |
| `dependents_of(path)` | `Vec<String>` | Reverse file_relations lookup |
| `file_count()` | `usize` | COUNT(*) from file_knowledge |
| `relation_count()` | `usize` | COUNT(*) from file_relations |

Implemented by `ThreadSafeKnowledgeDB` in `touring-hooks/src/cognitive_bridge.rs`.
All methods return empty/default on error (graceful degradation, no panics).

### NEW v18.0.0 — Cognitive MCTS (COG-1)

**`cognitive_mcts.rs`**: `CognitiveMCTSConfig { mcts_config: MCTSConfig, relevance_weight: f32 }` (default `relevance_weight=0.5`), `CognitiveMCTS` struct, `cognitive_search()` method.

- **expand_fn**: uses `graph.neighbors()` — SemanticGraph topology drives tree expansion
- **reward_fn**: `0.5 + relevance_weight * relevance_score(now)` — nodes with higher temporal relevance get higher reward signal
- Result: MCTS search is grounded in the semantic graph's accumulated attention data, not random rollouts. 6 tests.

### NEW v18.0.0 — Diagnostic-Driven Refinement Cycle (COG-2)

**`refinement.rs`**: `RefinementConfig { max_iterations: usize }` (default 3), `RefinementOutcome` (Resolved/Exhausted/Escalated), `RefinementCycle.run_refinement<F>()`.

Cross-crate import: `use touring_learning::aco::diagnostics::DiagnosticLayer` — cognitive crate now imports from learning crate. Each iteration classifies the error via `DiagnosticLayer`, applies the suggested `RetryStrategy`, and loops until resolved or max_iterations reached. 6 tests.

### NEW v18.0.0 — Adaptive Temporal Decay (COG-3)

**`semantic_graph.rs`**: Renamed `DECAY_HALF_LIFE_SECS` → `BASE_DECAY_HALF_LIFE_SECS`. New `adaptive_half_life(access_count)` function:

```
adaptive_half_life = BASE_DECAY_HALF_LIFE_SECS * ln(1.0 + access_count).max(1.0)
```

Frequently accessed nodes decay slower — hot files remain relevant longer in the semantic graph. `relevance_score()` uses `adaptive_half_life()`. 4 tests.

### NEW v18.0.0 — GoT Multi-Dimensional Evaluation (COG-4)

**`got.rs`**: `ThoughtResult` gains fields `relevance: f32`, `confidence: f32`, `novelty: f32`. New `evaluate_multidimensional()` method:

```
score = 0.4 * relevance + 0.3 * confidence + 0.3 * novelty
```

Novelty computed via Jaccard word-level similarity (low overlap = high novelty). Replaces single-dimensional heuristic eval. 5 tests.

### NEW v18.0.0 — TransitionMatrix + QTable Persistence (COG-5)

**`persistence.rs`**: `GraphSnapshot` gains two new fields with `#[serde(default)]` for backward compatibility:
- `transition_matrix: HashMap<String, HashMap<String, f32>>` — captures state-transition probabilities
- `q_values: HashMap<String, f32>` — persists Q-table values across sessions

`test_backward_compatible_load` verifies old snapshots without these fields still deserialize correctly. 5 tests.

### NEW v22.0.0 — GoT Parallel Actors (S2)

**`got.rs`**: `run_parallel_nodes(nodes: Vec<Arc<GotNode>>, msg: &ThoughtMessage) -> Vec<ThoughtResult>` — spawns one `tokio::task::JoinSet` task per node. `GotNode` does not implement `Clone`; `Arc<GotNode>` enables shared ownership. Each task is isolated: panics are caught and logged as `tracing::warn!`, not propagated. Results are sorted descending by score. Method `GoTEngine::evaluate_parallel(&self, node_ids: &[NodeId], msg)` wraps the free function.

### NEW v22.0.0 — GoTSnapshot rkyv (S3)

**`snapshot.rs`**: `GoTSnapshot { version: u32, session_id: String, node_snapshots: Vec<GotNodeSnapshot>, root_id: Option<String>, created_at: u64 }`. Flat struct — no `Arc`, no `Box<dyn>`. `#[archive(check_bytes)]` for rkyv 0.7 safe deserialization. `to_bytes()` → `rkyv::to_bytes::<_, 256>()`, `from_bytes()` → `rkyv::from_bytes()`, `to_json()` → serde_json fallback for debug/storage. `from_engine(engine, session_id)` extracts a read lock, collects node IDs as strings.

### NEW v22.0.0 — SessionPersistence (S3)

**`session_persistence.rs`**: `SessionPersistence { pool: deadpool_sqlite::Pool }`. Schema: `got_sessions (session_id TEXT PRIMARY KEY, snapshot BLOB NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)`. All methods async, use `conn.interact(move |conn| { ... }).await` — all variables must be moved into the closure (synchronous SQLite operations on a thread pool). `save_snapshot` uses `INSERT OR REPLACE`. `load_snapshot` returns `Ok(None)` for missing sessions. `list_sessions` returns `Vec<String>` of session IDs.

---

## 12. Subsystem: touring-server

**Purpose**: Runtime — MCP server, cortex handlers, hooks, indexer.

### 12.1 MCP Server (26 tools)

| Tool | Category |
|------|----------|
| touring_ast_overview, touring_ast_find, touring_ast_edit | Code Intelligence |
| touring_classify_intent, touring_scan_pii | Classification |
| touring_memory_store, touring_memory_recall | Memory |
| touring_learn_pattern, touring_cluster_skills, touring_suggest | Learning |
| touring_index_status, touring_file_ops, touring_project | Project |
| touring_graph, touring_decompose, touring_session | Structure |
| touring_evolve, touring_insights, touring_evolution_status | Evolution |
| touring_checkpoint, touring_refactor | Utility |
| touring_mask_context, touring_mcts_search | v9.0 Context/Planning |
| touring_speculate, touring_incremental_status, touring_online_learn | v9.0 Shadow/RL |

**VGP Parameter Structs**: 26 `*Params` structs in server.rs (L42-L440) define
the MCP tool input schemas. VGP (`scripts/vgp/extractor.py`) extracts these
via tree-sitter to generate verified tool call snippets. Key params:
AstOverviewParams, AstFindParams, GraphParams, MemoryStoreParams,
MemoryRecallParams, DecomposeParams, SuggestParams, MctsSearchParams,
SpeculateParams. See `.claude/rules/vgp-protocol.md` for full registry.

### 12.2 Cortex Runtime (73 handlers — v10.8)

| Group | File | Count | Handlers |
|-------|------|-------|----------|
| Neural | neural.rs | 10 | pre/post-read, pre/post-bash, pre/post-edit, session-start/stop, classify-intent, scan-pii |
| Enforcement | enforcement.rs | 8 | strategy-enforcer, data-structure + 6 sub-validators |
| Intelligence | intelligence.rs | 5 | aco-phase0, prompt-recall, touring-memory-injector, crystallizer, mcp-recommend |
| Lifecycle | lifecycle.rs | 15 | pre/post-compact, subagent-start/stop, teammate-idle, task-completed, post-tool-failure, stop-failure, session-end, config-change, worktree-enter/exit, instructions-enricher, permission-request, **vgp-learning (H59)** |
| Learning | learning.rs | 2 | rl-reward-emitter, td-learning-loop |
| Session | session.rs | 3 | session-finalizer, session-state-manager, cost-tracker |
| Tools | tools.rs | 6 | tool-search-advisor, failure-recorder, subagent-context-injector-v2, subagent-outcome-recorder-v2, **aco-goal-tracker (H56)**, **failure-recovery-orchestrator (H57)** |
| Evolution | evolution.rs | 4 | aco-evolution, generator-learning, session-optimizer, retrospective |
| **Quality** | **quality.rs** | **5** | **code-standards-enforcer (H51)**, **post-quality-gate (H52)**, **compliance-collector (H53)**, **dspy-quality-bridge (H54)**, **dspy-session-optimizer (H55)** |
| **Enrichment** | **enrichment.rs** | **15** | **permission-auto-approver (H60)**, symbol-enricher (H61), semantic-read-enricher (H62), vgp-advisory (H63), blast-radius-guard (H64), rlm-execution-capture (H65), pgcc-drift-validator (H66), rust-accelerator-check (H67), plan-fact-checker (H68), learning-session-startup (H69), **code-first-pipeline-guard (H70)**, **cipher-knowledge-retrieval (H71)**, **cipher-knowledge-capture (H72)**, **cipher-agent-output-capture (H73)**, **observer-learning-loop (H74)** |

#### v10.5 Handler Changes
- **Added**: quality.rs (NEW module) with 5 handlers (H51-H55)
- **Added**: H56 AcoGoalTracker, H57 FailureRecoveryOrchestrator (tools.rs), H59 VgpLearning (lifecycle.rs)
- **Removed**: H39 ShadowLintGate (superseded by H51 CodeStandardsEnforcer — diff-based + cached)
- **Net**: +8 added, -1 removed = +7 (51 → 58)

#### v10.6-v10.8 Handler Changes
- **v10.6**: enrichment.rs (NEW module) with 10 handlers (H60-H69) — migrated from Python project hooks
- **v10.7**: H70 CodeFirstPipelineGuard — TACO CODE-FIRST principle as native Rust handler
- **v10.8**: H71-H73 Cipher RL-connected handlers (knowledge retrieval, capture, agent output) + H74 ObserverLearningLoop
- **H51 matcher narrowed**: Write only (not Write|Edit|MultiEdit) — Edit `new_string` is fragment, ruff false positives
- **H55 event changed**: SessionEnd → Stop (SessionEnd never fires in Claude Code)
- **Net v10.5→v10.8**: +15 added = 58 → 73

#### Quality Module (quality.rs) — NEW in v10.5
| Handler | Event | Matcher | Async | Can Block | Purpose |
|---------|-------|---------|-------|-----------|---------|
| H51 CodeStandardsEnforcer | PreToolUse | Write\|Edit\|MultiEdit | No | **YES** | Diff-based ruff lint — blocks only on NEW violations. DashMap content-hash cache. |
| H52 PostQualityGate | PostToolUse | Write\|Edit\|MultiEdit | Yes | No | Format check (ruff format) + complexity heuristic + summary injection. |
| H53 ComplianceCollector | PostToolUse | (all) | Yes | No | Appends tool metrics to `~/.claude/metrics/compliance.jsonl`. Wilson score tracking. |
| H54 DspyQualityBridge | PostToolUse | Write\|Edit\|MultiEdit | Yes | No | Calls `~/.claude/scripts/dspy_quality_bridge.py` via subprocess. Fail-open (OnceLock DSPy check). |
| H55 DspySessionOptimizer | Stop | (all) | Yes | No | Calls `~/.claude/scripts/dspy_session_optimizer.py`. Stores suggestions in RlmMemory. |


#### Enrichment Module (enrichment.rs) — NEW in v10.6-v10.8
| Handler | Event | Matcher | Async | Purpose |
|---------|-------|---------|-------|---------|
| H60 PermissionAutoApprover | PermissionRequest | * | No | Auto-approve `mcp__touring__*` tools |
| H61 SymbolEnricher | PreToolUse | Read\|Edit | No | Inject file knowledge + symbol relations |
| H62 SemanticReadEnricher | PreToolUse | Read | No | FTS5 semantic memory search for context |
| H63 VgpAdvisory | PreToolUse | Write | No | Warn on unverified Touring struct references |
| H64 BlastRadiusGuard | PreToolUse | Edit | No | Dependency reverse lookup via `get_dependents()` |
| H65 RlmExecutionCapture | PostToolUse | Task | Yes | Captures ANTT task results to RLM |
| H66 PgccDriftValidator | PreToolUse | Write\|Edit | No | Symbol drift detection via AST heuristic |
| H67 RustAcceleratorCheck | SessionStart | * | Yes | Verifies touring/cargo/ruff availability |
| H68 PlanFactChecker | PostToolUse | Write\|Edit | Yes | Verifies path claims in plan .md files |
| H69 LearningSessionStartup | SessionStart | * | Yes | Loads Wilson patterns + recent RLM executions |
| H70 CodeFirstPipelineGuard | PreToolUse | Skill\|Task | No | Blocks ANTT skills without pipeline_state |
| H71 CipherKnowledgeRetrieval | PreToolUse | Read\|Write\|Edit\|Grep\|Glob | Yes | Scans .cipher/patterns/, feeds Wilson |
| H72 CipherKnowledgeCapture | PostToolUse | Write\|Edit | Yes | Stores in RLM + Wilson `cipher:capture` |
| H73 CipherAgentOutputCapture | SubagentStop | * | Yes | Stores in RLM `cipher:agent:*` + Wilson |
| H74 ObserverLearningLoop | Stop | * | Yes | Records session patterns to Wilson + drift + RLM |

#### Global Scripts (v10.8) — `~/.claude/scripts/` and `~/.claude/hooks/`
| Script | Called By | Purpose |
|--------|----------|---------|
| `start_cipher_mcp.sh` | MCP server (global) | Cipher wrapper with `$PWD` fallback, graceful degradation without API keys |
| `dspy_quality_bridge.py` | H54 | AST-based DSPy pattern validation (signatures, forward(), f-string prompts, API keys) |
| `dspy_session_optimizer.py` | H55 | Session pattern analysis, optimization suggestions from compliance.jsonl |

### 12.3 Hooks (touring-hooks crate, 20+ handlers)
- IntentClassifier: 58-pattern RegexSet for CILA L0-L6
- PIIScanner: CPF/CNPJ/SEI/email/phone with whitelist (13 patterns, 13 filters)
- Neural hooks: pre/post for Read/Bash/Edit + session start/stop
- shadow_v2: Multi-branch speculative execution with ruff validation
- **AST Bridge** (v11.0.0): Intelligence layer connecting touring-ast to hooks
  - `extract_enriched_symbols()`: symbols + complexity in single pass
  - `analyze_file_quality()`: FileQualityMetrics (max/avg CC, async ratio)
  - `validate_edit_impact()`: EditImpactResult (syntax + blast radius + CC delta)
  - `check_symbol_complexity()`: per-symbol CC check with threshold
  - `build_enriched_knowledge_with_quality()`: FileKnowledge + quality notes
- **ACO Bridge** (v12.0.0): 9D quality tracking integrated into HookRuntime
  - `HookQualityAssessment`: Maps 9 dimensions (Precision/Coverage/Latency/Knowledge/Context/Reliability/Integration/Security/Evolution)
  - `HookResultCache`: LRU+TTL query result caching for hooks
  - `HookEventBuffer`: Write-ahead batch buffering for hook events
  - `HookRuntime::quality_report()`: Returns `TrackerReport` with composite score
  - `HookRuntime::reset_quality_tracking()`: Session lifecycle integration
- **OutputCapture** (v12.0.0): Intelligent output summarization for large command outputs
  - 4 extractors: `PytestExtractor`, `CargoTestExtractor`, `RuffExtractor`, `GenericExtractor`
  - Auto-selects extractor based on command content
  - Extracts structured metrics (passed/failed/errors/warnings/coverage)
  - UTF-8 safe truncation for multi-byte characters
  - Percentage extraction with `%` suffix priority
- **Integration Tests** (v12.0.0): E2E validation of ACO bridge + HookRuntime lifecycle
- **post_read.rs** (v11.0.0): Migrated from regex-only to AST+regex dual path
  - Python/Rust/TS/JS → tree-sitter via `ast_bridge::build_enriched_knowledge_with_quality()`
  - Other languages → regex fallback (extract_imports_fast, extract_symbols_fast)

### 12.3.1 Neural Hooks — Claude Code Integration (v29.0.0)

**13 hooks registered in `~/.claude/settings.json`** (v29: +pre-write, +post-write):

| Hook | Event | Matcher | Function |
|------|-------|---------|----------|
| `touring session-start` | SessionStart | startup\|resume | Injects knowledge DB stats |
| `touring pre-read` | PreToolUse | Read | Injects notes, gotchas, dependents |
| `touring post-read` | PostToolUse | Read | Learns imports, symbols, hash (AST+regex) |
| `touring pre-edit` | PreToolUse | Edit | Scored signals (v29): blast radius, external callers, complexity delta, call graph impact, scope shadowing, ModuleTree re-exports, cognitive enrichment. CILA budgets (L0-L1=1200, L2-L3=3000, L4+=6000). Rayon parallel AST. |
| `touring pre-write` | PreToolUse | Write | Speculative validation, anti-pattern detection, import completeness, wiring prediction, quality baseline. CILA budget (v29). |
| `touring post-edit` | PostToolUse | Edit | Feedback-capable (v29): run_returning() returns HookResponse with 5 verification signals (speculative, anti-patterns, complexity, wiring). Multi-language (Rust/Python/TS/JS/Go/C/C++/Java). |
| `touring post-write` | PostToolUse | Write | Quality verification + feedback. Multi-language anti-patterns. Wiring registration for new files (v29). |
| `touring pre-bash` | PreToolUse | Bash | Alerts about prior command failures |
| `touring post-bash` | PostToolUse | Bash | Records outcomes, error patterns |
| `touring session-stop` | Stop | * | Persists session insights to JSON |

**Feedback loops** (each Post-hook feeds the corresponding Pre-hook):
```
post-read → knowledge DB → pre-read (next read of same file)
post-bash → outcomes DB → pre-bash (next run of same command)
post-edit → edit history + quality feedback → pre-edit (next edit of same file) + auto-gotcha
post-write → wiring registration + quality feedback → pre-write (next write of same file) [v29]
post-bash(error on file) → pre-read (cross-cutting file note)
session-stop → insights JSON → session-start (next session warm-start)
```

### 12.4 Index
- Incremental symbol indexing (WAL journal tracking)
- 2-tier cache: TTL L1 (60s, 256 entries) + SQLite L2
- File watcher (notify 7.0, debounced 100ms)

### 12.5 Daemon Persistente — v17.0.0

**Problema resolvido**: cada invocação de hook criava um processo efêmero que abria 3-4 conexões SQLite + executava DDL completo → floor de 10-13ms irredutível.

**Solução**: `touring-hook` é agora um thin client. O binário `touring-daemon` mantém `HookRuntime` aquecido em memória.

#### Arquivos novos em `crates/touring-hooks/src/`

| Arquivo | Responsabilidade |
|---------|-----------------|
| `ipc.rs` | `DaemonRequest` / `DaemonResponse` (serde JSON), `daemon_socket_path()` → `/tmp/touring-daemon-{uid}.sock`, `daemon_lock_path()` |
| `daemon.rs` | Servidor Unix socket, `RuntimeMap = Arc<Mutex<HashMap<PathBuf, HookRuntime>>>` (multi-projeto sem thrashing), idle watchdog 5min com `AtomicBool request_in_progress` (evita matar mid-request), lock file atômico via `create_new(true)` (elimina TOCTOU race) |
| `daemon_main.rs` | Entrypoint do binário `touring-daemon` com handlers SIGTERM/SIGINT |

#### Fluxo de execução (thin client em `main.rs`)

```
touring-hook <subcommand>
  1. Tenta conectar ao Unix socket
  2. Se socket existe → serializa request → lê response (timeout 3000ms)
  3. Se socket não existe → auto-start `touring-daemon` → retry
  4. Se daemon falhar → fallback standalone (exit 0 sempre)
```

#### Métricas de latência

| Cenário | Latência |
|---------|---------|
| Daemon warm (socket aberto) | P50=1ms, avg=2ms |
| Cold start (primeira invocação) | ~15-20ms (daemon inicializa, acontece uma vez por sessão) |
| Fallback standalone | 10-13ms (comportamento anterior) |

#### Design decisions (code review — 7 issues resolvidos)

| ID | Fix |
|----|-----|
| C1 | `acquire_lock` atômico via `create_new(true)` — elimina TOCTOU race |
| C2 | `AtomicBool request_in_progress` — watchdog não mata daemon mid-request |
| C3 | Watchdog usa `process::exit` (não `abort`) — comentário corrigido |
| I1 | Bloco de histórico de versões para `SCHEMA_VERSION` em `knowledge.rs` |
| I2 | `RuntimeMap = HashMap<PathBuf, HookRuntime>` — suporte a múltiplos projetos simultâneos |
| I3 | `read_timeout` aumentado de 100ms para 3000ms |
| I4 | `touring-daemon` adicionado ao `settings.json` em `SessionStart` para pre-warm |

---

## 13. Subsystem: touring-python — v17.0.0 Updated

**Purpose**: PyO3 bindings preserving backward compatibility with `scripts/aco/rust_bridge.py`.
**Module name**: `claude_learning_kernel` (definido em `[lib] name` do `Cargo.toml`)
**5 custom exceptions**: `AcoGraphError`, `AcoValidationError`, `AstParseError`, `AstSurgeryError`, `SerializationError`

### 13.0 Build — maturin (v17.0.0)

| Item | Valor |
|------|-------|
| Build tool | `maturin` v1.12.6 |
| Comando | `maturin develop --release` (dentro de `crates/touring-python/`) |
| Venv | `.venv` em `crates/touring-python/` |
| Output | `libclaude_learning_kernel.so` (7.6MB) |
| Bindings verificados | RL, AST, NLP, SIMD, ACO |

```bash
cd crates/touring-python
maturin develop --release   # compila + instala no .venv local
python -c "import claude_learning_kernel; print('OK')"
```

### 13.1 ACO Bindings (15+ symbols)

| Symbol | Type | Purpose | v12.0.0 |
|--------|------|---------|---------|
| `CRITICAL_WEIGHT` | f64 constant (1.5) | GoalTracker weight | |
| `HALT_ITERATIONS` | i64 constant (3) | Consecutive failure limit | |
| `HALT_THRESHOLD` | f64 constant (0.5) | Minimum composite | |
| `NORMAL_WEIGHT` | f64 constant (1.0) | Non-critical weight | |
| `VETO_THRESHOLD` | f64 constant (0.8) | Per-dimension minimum | |
| `AcoGraph` | #[pyclass] | DAG graph operations | **+ `__repr__`** |
| `DimResult` | #[pyclass] | 9D dimension result | **+ `__repr__` + `to_dict` (Python dict)** |
| `TrackerReport` | #[pyclass] | GoalTracker report | **+ `__repr__`** |
| `EventProjector` | #[pyclass] | ESAA event projection | |
| `QueryCache` | #[pyclass] | LRU+TTL cache | **+ `__repr__`** |
| `EventBuffer` | #[pyclass] | Write-ahead batch buffer | **+ `__repr__`** |
| `py_compute_composite` | #[pyfunction] | Weighted 9D composite | |
| `py_determine_status` | #[pyfunction] | PASS/HALT/VETO | |
| `py_build_report` | #[pyfunction] | Full report builder | |
| `verify_chain_parallel` | #[pyfunction] | SHA-256 chain verification (GIL-released) | |

### 13.2 AST Bindings (6 symbols)

| Symbol | Type | Purpose | v12.0.0 |
|--------|------|---------|---------|
| `AstSymbol` | #[pyclass] (frozen) | 15-field symbol with `start_byte`/`end_byte` | **Bug fix: test constructors** |
| `py_extract_symbols` | #[pyfunction] | Extract symbols from source (GIL-released) | |
| `py_extract_symbols_from_file` | #[pyfunction] | Extract from file path | |
| `py_compute_complexity` | #[pyfunction] | Cyclomatic complexity map | |
| `py_validate_syntax` | #[pyfunction] | Syntax validation | |
| `py_extract_imports` | #[pyfunction] | Import extraction | |
| `py_supported_languages` | #[pyfunction] | List 11 supported languages | |

### 13.3 NLP Bindings (11 symbols) + SIMD Bindings (6 symbols)

NLP: `PyMonetaryValue`, `KeywordMatcher`, `py_chunk_document`, `py_levenshtein_distance`, etc.
SIMD: `CosineComputer`, `WilsonRanker`, `DriftDetector`, `py_npv`, `py_irr`, `py_stress_scenarios`, etc.

### 13.4 RL Bindings (NEW v13.0.0) — `rl_bindings.rs`

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `process_reward` | `(state: u64, action: u64, reward: f64, next_state: u64, terminal: bool) → f64` | QTable TD(λ) update, returns TD error |
| `select_arm` | `(features: Vec<f64>) → usize` | LinUCB arm selection (FEATURE_DIM=19) |
| `update_arm` | `(arm_index: usize, features: Vec<f64>, reward: f64) → ()` | LinUCB Sherman-Morrison rank-1 update — closes the bandit loop |
| `get_q_value` | `(state: u64, action: u64) → f64` | Read Q(state, action), 0.0 for unseen |
| `get_best_action` | `(state: u64) → Option<u64>` | argmax_a Q(state, a) |
| `get_linucb_arm_stats` | `() → Vec<PyDict>` | Per-arm: arm_index, pulls, avg_reward, cumulative_reward |
| `FEATURE_DIM` | constant `19` | Feature vector dimensionality |
| `NUM_ARMS` | constant `8` | LinUCB arm count |

**Global singletons** (`OnceLock<Mutex<T>>`): QTable and LinUCBBandit initialized once per process, shared across all Python calls. Sherman-Morrison update is O(d²) ≈ 3,600 FLOPs for d=19 — sub-microsecond.

**Critical invariant**: `update_arm` MUST be called after `select_arm` once reward is known. Without this call, `select_arm` explores but LinUCB covariance matrices never update (pulls stay at 0, avg_reward stays 0.0).

### 13.5 AST-RL Bridge (NEW v13.0.0) — `ast_rl_bridge.rs`

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `compute_rl_state` | `(file_path: &str) → u64` | DJB2 hash of (file_path + symbol context) → state ID for QTable |

### Integration pattern
```python
# scripts/aco/rust_bridge.py — ACO bindings (existing)
try:
    from claude_learning_kernel.claude_learning_kernel import AcoGraph, ...
    RUST_AVAILABLE = True
except ImportError:
    RUST_AVAILABLE = False  # fallback to pure Python

# ~/.claude/hooks/post_tool_use_rl.py — RL bindings (NEW v13)
try:
    import claude_learning_kernel as k  # k.select_arm, k.process_reward, k.update_arm
    # k.FEATURE_DIM == 19, k.NUM_ARMS == 8
except ImportError:
    pass  # fallback to file-based queue
```

---

## 14. Data Flows — End-to-End Traces

### Trace 1: User prompt → CILA classification → hook context

```
User types prompt
  → Claude Code fires UserPromptSubmit hook
  → settings.local.json routes to: touring cortex prompt
  → CortexRuntime dispatches to classify-intent handler
  → IntentClassifier (48-pattern RegexSet) → CILALevel
  → Hook output: { additionalContext: "↳ [perception] action=... | arch=..." }
  → Claude Code receives context injection
```

### Trace 1b: Neural Hook feedback loop (v11.0.0)

```
Claude reads file (e.g., symbols.rs)
  → Claude Code fires PreToolUse (Read) hook
  → touring pre-read: checks FileKnowledgeDB for notes, gotchas, dependents
  → If HIGH-SIGNAL exists: injects additionalContext ("`ruff` failed on this file")
  → If no signal: silence (exit 0, no output)
  → Claude reads the file
  → Claude Code fires PostToolUse (Read) hook
  → touring post-read:
    → AST path (Python/Rust/TS/JS): ast_bridge::build_enriched_knowledge_with_quality()
      → tree-sitter parsing → extract symbols (with kind, parent, async, decorators)
      → compute complexity per callable
      → extract imports via tree-sitter queries
      → compute SHA-256 content hash
    → Regex fallback (other languages): extract_imports_fast() + extract_symbols_fast()
    → Upserts FileKnowledge + FileRelation + access_log
    → NEXT pre-read of this file will have accumulated knowledge
```

### Trace 2: MCP tool call → graph context → response

```
Claude Code invokes touring_memory_recall(query="pipeline optimization")
  → TouringServer receives via rmcp
  → MemoryStore searches unified SQLite (FTS5 + cosine)
  → GraphService resolves focus context (neighbors, imports)
  → CognitiveNexus resolves predictive context
  → Response enriched with graph_ctx + cognitive_ctx
  → Claude Code receives structured JSON
```

### Trace 3: Python → PyO3 → ACO computation

```
scripts/aco/orchestrator.py calls get_compute_composite()
  → rust_bridge.py imports from claude_learning_kernel
  → touring-python delegates to touring-learning/aco/tracker
  → Weighted 9D composite computed in Rust (f64 precision)
  → Result returned to Python as float
```

### Trace 4: Tool execution → RL feedback loop (NEW v13.0.0)

```
Claude executes any tool (Edit, Bash, Write, Read, ...)
  → Claude Code fires PostToolUse hook
  → settings.json routes to: ~/.claude/hooks/post_tool_use_rl.py
  → _parse_payload(): extract tool_name, output, error, elapsed_ms
  → compute_reward(): multi-dimensional reward ∈ [-1.0, 1.0]
      ├─ Silent tools (Read, Glob, Grep): reward = 0.0
      ├─ Hard error: reward = -1.0 (or -0.5 if output has useful content)
      ├─ Base success: +0.5
      ├─ Latency penalty: -0.2 if >5s, -0.1 if >1s
      └─ Code quality (Edit/Write/MultiEdit): ±0.3
  → _build_feature_vector(): 19D vector [accepted, latency_norm, errors_norm,
      cila_norm, file_type_oh[4], quality, reward_norm, zeros×9]
  → import claude_learning_kernel (PyO3, <1ms)
  → select_arm(features): LinUCB selects best arm for this context
  → state = _djb2_hash(tool_name): stable u64 state ID
  → process_reward(state, arm, reward, state, False): QTable TD(λ) update
  → update_arm(arm, features, reward): LinUCB Sherman-Morrison A⁻¹ update
  → On ImportError: _enqueue_to_file() → ~/.claude/data/rl_reward_queue.jsonl
  → Always exits 0 (never blocks Claude Code)
```

**Runtime proof** (120 cycles post-deployment):
```
QTable top states:    Write(6.195), Read(3.782), Bash(2.566)
LinUCB arm 2:         avg_reward=0.64 (highest), pulls=18
All 8 arms:           pulls > 0 (convergence confirmed)
```

---

## 15. Database Schemas and Persistence

### v29.8.0: 3 Consolidated Domain Databases (SCHEMA_VERSION=8)

Previously 8 separate SQLite files; now consolidated into 3 domain DBs.
Path resolution via `TouringConfig::*_canonical(&project_root)`.

| Database | Path | Content | DDL Source |
|----------|------|---------|------------|
| **knowledge.db** | `.claude/touring/knowledge.db` | Symbols, file knowledge, wiring map, gotchas, edit history, bash outcomes | `touring_core::schema::knowledge::KNOWLEDGE_SCHEMA_V8` |
| **memory.db** | `.claude/touring/memory.db` | RLM entries (FTS5), semantic recall embeddings (f32+u4), ANN embeddings | `touring_core::schema::memory::MEMORY_SCHEMA_V8` |
| **graph.db** | `.claude/touring/graph.db` | GoT snapshots, learning (Wilson/QTable/LinUCB/Drift), sessions, hook events | `touring_core::schema::graph::GRAPH_SCHEMA_V8` |

#### knowledge.db tables
```
symbols, dependencies, symbols_fts (FTS5), file_knowledge, bash_outcomes,
file_edit_history, file_gotchas, file_risk_scores, wiring_map, module_ecosystem
```

#### memory.db tables
```
rlm_entries, rlm_fts (FTS5), recall_embeddings (f32 + u4 columns),
ann_embeddings, ann_meta
```

#### graph.db tables
```
got_snapshots, learning_wilson, learning_qtable, learning_linucb,
learning_drift, learning_tool_outcomes, touring_hook_events,
sessions, session_checkpoints
```

### Migration Tool

CLI: `touring migrate {status|plan|run|validate|cleanup|rollback}`
API: `touring_core::migration::consolidation::ConsolidationMigration`

Key design decisions:
- Explicit column mappings for 7 tables where legacy and v8 schemas diverge
- `INSERT OR IGNORE` for first source, `INSERT OR REPLACE` for merge (newest wins)
- FTS5 rebuilt via `INSERT INTO fts(fts) VALUES('rebuild')` after data migration
- `_migration_state` table in each DB for resume support
- Legacy DBs renamed to `.db.migrated` (not deleted) for rollback safety

### Schema Migration Engine (touring-core::migration)

`SCHEMA_VERSION = 8` (pub const in `touring_core::migration`).
All 3 domain DBs use `PRAGMA journal_mode = WAL` and `schema_meta` table for version tracking.

---

## 16. Concurrency Model

| Component | Pattern | Rationale |
|-----------|---------|-----------|
| MCP Server | `tokio::sync::Mutex` | Async runtime, sequential tool access |
| Hooks | No concurrency (synchronous) | < 15ms budget, no contention |
| NLP Pipeline | `std::sync::Mutex` | Thread-safe with Rayon parallelism |
| Symbol Index | `rusqlite` WAL mode | Concurrent readers, single writer |
| Embedding Cache | `DashMap` (6.1) | Lock-free concurrent reads |
| LRU caches | `Mutex<LruCache>` | Microsecond-duration locks |

---

## 17. Configuration and Environment

### Environment Variables
| Variable | Default | Purpose |
|----------|---------|---------|
| `CLAUDE_PROJECT_DIR` | `.` | Project root for path resolution |
| `TOURING_DB_PATH` | `.claude/rust/symbols.db` | Symbol database path |
| `TOURING_MEMORY_PATH` | `.claude/data` | Memory database directory |
| `TOURING_BIN` | (auto-detected) | Override binary path for cli-anything |

### Hook Registrations (v29.8.0)

**Global** (`~/.claude/settings.json`): 18 Touring hooks + 3 third-party across 12 events.
All Touring hooks use `$HOME/.claude/hooks/touring-hook <subcommand>` (symlink → release binary).

| Event | Hooks | Timeout |
|-------|-------|---------|
| `PreToolUse[Read]` | `touring-hook pre-read` | 10s |
| `PreToolUse[Edit]` | `touring-hook pre-edit` + `pre-edit-prevention` | 10s |
| `PreToolUse[Write]` | `touring-hook pre-write` | 10s |
| `PreToolUse[Bash]` | `touring-hook pre-bash` | 10s |
| `PreToolUse[Grep\|Glob\|Bash]` | `gitnexus-hook.cjs` (third-party) | 8s |
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
| `Stop` | `auto_compact.sh` (third-party) | 2s |
| `UserPromptSubmit` | `prompt_enhancer.py` (third-party) | 5s |

**Note (v29.8.0)**: Prior to this version, 0 Touring hooks were configured in settings.json — the entire Touring intelligence layer was dead code at runtime. All 18 hooks verified exit 0.

---

## 18. Quality, Testing, and Cross-Audit

### 18.1 Feature Flag Matrix

| Crate | Flag | Default | Dependencies Added | Behavior When ON | Behavior When OFF |
|-------|------|---------|-------------------|------------------|-------------------|
| touring-core | `gpu-embeddings` | off | `reqwest` | `embedding` module compiled; `EmbeddingClient` sends HTTP to GPU service (localhost:8200); semantic recall uses cosine similarity | `embedding` module excluded; semantic recall degrades to FTS5-only keyword matching |
| touring-learning | `simple-clustering` | **on** | — | Threshold-based cosine skill clustering | (not meaningful alone — one of simple/leiden should be on) |
| touring-learning | `leiden-clustering` | off | `linfa`, `linfa-clustering` | Modularity-optimal Leiden algorithm for skill clustering | Falls back to simple cosine threshold clustering |
| touring-learning | `ftrl` | off | `linfa-ftrl` | FtrlLayer online learning compiled | FtrlLayer excluded from build |
| touring-learning | `ebpf` | off | (system eBPF) | `EbpfObserver::try_load()` returns true, `collect_metrics()` returns real data | Stub: try_load()→false, collect_metrics()→empty vec |
| touring-learning | `u4-quantization` | **on** (v29.8.0) | `touring-simd/quantization` → `dep:half` | `store_chunk()` writes f32+u4; `ann_search_u4()` reads u4 with f32 fallback; `migrate_embeddings_to_u4()` batch migration | u4 columns not written; search uses f32 only |
| touring-hooks | `u4-quantization` | **on** (v29.8.0) | `touring-learning/u4-quantization` | Propagates feature to touring-learning | No effect |
| touring-simd | `quantization` | off (enabled transitively) | `dep:half` | `EmbeddingU4`, `BlockQuantizer`, f16 conversion | Quantization types excluded |

#### Tested Configurations

| Configuration | CI Tested | Notes |
|--------------|-----------|-------|
| default (no flags) | ✅ | FTS5-only recall, simple clustering |
| `gpu-embeddings` | ✅ | Requires GPU service at :8200 — tests use offline fallback |
| `leiden-clustering` | ⚠️ | Adds ~15s compile time (linfa); test with `cargo test --features leiden-clustering` |
| `gpu-embeddings` + `leiden-clustering` | ⚠️ | Full-featured; production configuration when GPU available |

#### Degradation Behavior

When `gpu-embeddings` is OFF (default):
- `SemanticRecall` operates in **FTS5-only mode** — keyword matching without cosine similarity
- `EmbeddingClient` is not compiled — no HTTP calls to GPU service
- Memory entries stored WITHOUT embeddings (BLOB column is NULL)
- **User impact**: recall quality is lower (keyword-only vs hybrid keyword+semantic)

When `leiden-clustering` is OFF (default):
- `ClusterEngine` uses simple cosine threshold (similarity > 0.7 = same cluster)
- Adequate for < 100 skills; Leiden recommended for larger skill catalogs

### VGP Cross-Audit (Python-side consumer)

VGP validates that Python code correctly references Touring Rust structs.
Audit: `python scripts/vgp/tests/audit_vgp_cross_functional.py` — 66/66 checks, composite 1.0000.
Tests: `pytest scripts/vgp/tests/test_vgp_core.py` — 35 tests, 2s.

### Build verification
```bash
cargo check --workspace              # type check (< 15s)
cargo test --workspace --exclude touring-python  # 2,840 tests (< 90s)
cargo clippy --workspace -D warnings # 0 warnings
cargo build --workspace --release    # ~58s
```

### Per-crate test coverage (v28.3)
| Crate | Tests | Coverage focus |
|-------|-------|----------------|
| touring-core | 28 | MemoryTier, CILALevel, TouringConfig, migration engine |
| touring-simd | 185 | SIMD parity (AVX2 vs scalar), Wilson, Drift, **TopKSearcher, distance metrics, matrix ops (v28.3)** |
| touring-learning | 502 | QTable TD(λ), RLM tiers, ACO graph/tracker, LinUCB, ReminderBandit 7-arm (S0), CrdtDelta delta/merge (S6), DriftMonitor KS-test (S7), EbpfObserver (S7), RingBuffer (v24.1), RLE encoding (v24.1), **SIMD find_topk/find_by_distance (v28.3)** |
| touring-ast | 286 | Symbol extraction per language, surgery, store, FileHeat, RRF, EnrichedBlastRadius |
| touring-antt | 80 | Monetary parsing, keyword matching, chunking (ex touring-nlp, renomeado v21.1) |
| touring-cognitive | 266 | SemanticGraph, SessionPredictor, GoT parallel JoinSet (S2), GoTSnapshot rkyv (S3), SessionPersistence deadpool-sqlite (S3), **SIMD TopKSearcher (semantic_graph, ann_index), SIMD CosineComputer (tfidf) (v28.3.1)** |
| touring-hooks | 572 | Hooks E2E, DependencyCache petgraph, rkyv IndexSnapshot, BranchFs CoW+restore+drift (S8) |
| touring-server | 444 | Tools, cortex handlers (H1-H83), server integration |
| touring-index | 110 | Symbol index, incremental re-indexing, cross-file references, IndexSnapshot |
| touring-cortex | 519 | Cortex handlers + FilterCache + tiktoken + RRF parallel + SCC caching + Zstd compression + **EmbeddingIndex (v28.3)** |
| touring-wasm | 183 | WasmRunner (async_support=true), WAT modules, sandbox, fuel metering, InferletPool, AsyncInferletPool InstancePre+spawn_blocking (v28.2), TypedEvaluate (v22), call_evaluate_async (v28.2), KvCacheManager (v24.1), AsyncKvCacheManager trait + AsyncInMemoryCacheManager (v28.2), fast_instantiation_config (v24.1), H83 TypedEvaluateHandler (cortex), **simd_embedding (v28.3)**, InferletService (v28.13, touring-hooks integration), set_input+evaluate WASM ABI, strip_inferlet_key dispatch |
| touring-python | 6 (excl.) | Backward compat constants, ACO operations |
| **Total** | **3,471** | **cargo test --workspace --exclude touring-python** |

### Cross-audit (9 dimensions)
```bash
python scripts/touring_unification/cross_audit.py
# D1: Struct uniqueness    D2: Clippy    D3: Tests
# D4: MCP tools (21)       D5: Binary    D6: Python compat
# D7: DB schema            D8: Artifacts D9: Settings refs
# Composite ≥ 0.9 + no dim < 0.5 → PASS
```

---

## 19. Migration from v3.0.0 (monolith → workspace)

### What moved where

| v3.0.0 Source | v4.0.0 Target | LOC |
|---------------|---------------|-----|
| touring/src/memory/ | touring-learning/memory/ | 2,014 |
| touring/src/learning/ | touring-learning/rl+ranking+clustering/ | 1,372 |
| touring/src/evolution/ | touring-learning/evolution/ | 1,093 |
| touring/src/ast/ | touring-ast/ | 2,382 |
| touring/src/cognitive/ | touring-cognitive/ | 506 |
| touring/src/server.rs + tools/ + hooks/ + cortex/ + index/ + ... | touring-server/ | 19,875 |
| touring/src/config.rs + error.rs + embedding/ | touring-core/ | 875 |
| rust-core/src/aco/ | touring-learning/aco/ | 3,504 |
| rust-core/src/nlp/ + search_index/ | touring-antt/ | 4,499 |
| rust-core/src/simd_utils/ + similarity/ + statistics/ | touring-simd/ | 1,335 |
| rust-core/src/lib.rs (PyO3) | touring-python/ | 758 |

### What was retired (not migrated)

| Module | LOC | Reason |
|--------|-----|--------|
| rust-core/src/touring/ | 6,474 | Dead predecessor of touring MCP server |
| rust-core/src/bin/ | 2,076 | 9 dead binaries (0 refs in settings) |
| rust-core/src/validation_daemon/ | 1,557 | Superseded by hook quality gates |
| rust-core/src/checkpoint/ | 1,530 | Reimplemented in touring-server/tools/ |
| rust-core/src/hooks_accel/ | 1,174 | Superseded by touring-server/hooks/ |
| rust-core/src/dspy_compat/ | 390 | DSPy 3.x removed Assert/Suggest APIs |
| rust-core/src/integration/ | 847 | NumPy bridge — 0 active Python imports |
| rust-core/hook_code_standards.rs | 123 | Python hook handles this |

---

## 20. touring-server Coupling Analysis and Extraction Roadmap

### Module-level LOC distribution (19,875 total)

| Module | LOC | Files | % of crate | Extractable? |
|--------|-----|-------|-----------|-------------|
| cortex/ | 5,310 | 11 | 26.7% | Phase 2 (deps on hooks/) |
| hooks/ | 3,959 | 12 | 19.9% | **Phase 1** (0 internal deps) |
| tools/ | 3,244 | 8 | 16.3% | Not recommended (tight coupling to server.rs) |
| server.rs | 2,968 | 1 | 14.9% | No (entry point) |
| index/ | 1,551 | 4 | 7.8% | Possible (standalone indexer) |
| ingest/ | 617 | 3 | 3.1% | Possible (standalone ingester) |
| memory_store.rs | 554 | 1 | 2.8% | No (server-specific) |
| output/ | 504 | 3 | 2.5% | No (formatters for server) |
| reasoning/ | 342 | 2 | 1.7% | No (small, tightly coupled) |
| main.rs | 308 | 1 | 1.5% | No (entry point) |
| session/ | 273 | 2 | 1.4% | No (server-specific) |
| graph_service.rs | 223 | 1 | 1.1% | No (server-specific) |

### Cross-module dependency graph

```
                    ┌──────────────────────────────────────┐
                    │          touring-server               │
                    │                                      │
                    │  server.rs ──→ hooks (3 imports)      │
                    │      │                               │
                    │      └──→ tools ──→ hooks (2 imports) │
                    │      │                               │
                    │      └──→ cortex ──→ hooks (9 imports)│
                    │             │                         │
                    │             └──→ tools (0)            │
                    │                                      │
                    │  hooks ──→ cortex (0)  ← CLEAN!      │
                    │  hooks ──→ tools  (0)  ← CLEAN!      │
                    └──────────────────────────────────────┘
```

**Key finding**: `hooks/` has **zero imports** from cortex/ or tools/, making it the
cleanest extraction candidate. All other modules depend on hooks, but hooks depends
on nothing within touring-server.

### Extraction roadmap

**Phase 1**: Extract `touring-hooks` crate (3,959 LOC, 12 files)
- Move `hooks/` → `crates/touring-hooks/src/`
- Add `touring-hooks` as dependency of `touring-server`
- Update 14 `use crate::hooks` → `use touring_hooks` in cortex/, tools/, server.rs
- Estimated reduction: touring-server 19,875 → 15,916 LOC (-20%)
- Risk: LOW (no circular deps, clean boundary)

**Phase 2**: Extract `touring-cortex` crate — ✅ CONCLUÍDO (S4, 2026-03-26)
- `crates/touring-cortex/` criado: types, handler, context, pipeline, metrics, cache_strategy, enrichment, runtime, rl_mapping + 81 handlers (H1-H82)
- `pub use touring_cortex as cortex;` em touring-server para backward compat
- +231 testes (S4) + +227 testes (v21.1 recovery) = 458 testes total, zero circular deps

**Phase 3**: Extract `touring-index` — ✅ CONCLUÍDO (S3, 2026-03-26)
- `crates/touring-index/` criado: cache.rs, incremental.rs, watcher.rs (1,525 LOC total)
- `pub use touring_index as index;` em touring-server para backward compat
- 16 testes, zero deps circulares

**Phase 4**: Deduplicar fork do cortex em touring-server — ✅ CONCLUÍDO (v21.1, 2026-03-26)
- `touring-server/src/cortex/` (fork ~12.800 linhas) removido após audit confirmar touring-cortex completo
- `lib.rs` simplificado: `pub use touring_cortex as cortex;` — zero código duplicado
- Recuperação de 227 testes em touring-cortex (9 arquivos) para manter baseline 2.671

**Status**: ✅ COMPLETO — 4 fases concluídas. touring-server é thin orchestration layer: tools + MCP server + `pub use` aliases para cortex/index. Zero duplicação.

---

## 21. v9.0.0 Changelog — Exponential Intelligence (2026-03-20)

Based on cross-source research (Cursor Engineering Blog, Context7, 30+ arxiv papers).
Plan: `.claude/plans/touring-v9-exponential-strategy.md`
Generator: `scripts/aco/generators/gen_touring_v9_exponential.py` (N1, 2046 LOC)

### New Modules (5)

| Module | Crate | LOC | Tests | Purpose |
|--------|-------|-----|-------|---------|
| `observation_masker.rs` | touring-server | 877 | 22 | Context token reduction via tool-result masking (ContextRole classification, MaskingStrategy, activation threshold). Based on "The Complexity Trap" (NeurIPS 2025) |
| `mcts.rs` | touring-cognitive | 724 | 18 | Monte Carlo Tree Search with UCT selection, generic closures for expand/reward, multi-depth planning. Based on RPM-MCTS (arXiv:2511.19895) |
| `incremental_pipeline.rs` | touring-ast | 742 | 14 | Wires IncrementalParser + RopeDocument + SymbolStore into single edit pipeline. tree-sitter queries replace regex for symbol extraction |
| `shadow_v2.rs` | touring-hooks | 981 | 24 | Multi-branch speculative execution with HashMap overlays, ruff validation, scoring, branch commit. No FUSE dependency |
| `online_rl.rs` | touring-learning | 525 | 17 | Immediate per-tool reward (complementary to batch auto_learn), EMA noise filter, warm-start LinUCB priors |

### Infrastructure Changes

| Change | Scope | Impact |
|--------|-------|--------|
| 12 criterion benchmarks (9 new) | 6 crates | 138 benchmark functions, performance baselines |
| moka TinyLFU in context_compiler | touring-server | Thread-safe cache without Mutex wrapper |
| DashMap + AtomicUsize in keyword_matcher | touring-antt | Eliminated RwLock, lock-free reads |
| rkyv QTableSnapshot + LinUCBSnapshot | touring-learning | Zero-copy state persistence for RL engine |
| Dead code elimination | 4 modules | MaskingStrategy wired, discount applied, clear_cache fixed, E5xx severity corrected |

### Metrics Comparison

| Metric | v8.0.0 | v9.0.0 | Delta |
|--------|--------|--------|-------|
| LOC | 47,700 | 56,500 | +8,800 |
| Tests | 1,106 | 1,209 | +103 |
| Benchmarks | 3 | 12 | +9 |
| Lock operations | 189 | ~126 | -33% |
| Clippy warnings | 0 | 0 | -- |
| New Rust modules | -- | 5 | +5 |

### Cross-Audit

32/32 checks PASS, composite 1.0000. Checkpoint: `.claude/checkpoints/cross_audit_v9_20260320.toon`

---

## 22. Glossary

| Term | Definition |
|------|------------|
| **CILA** | Classification of Intent and Linguistic Analysis (L0-L6) |
| **Cortex** | Unified hook event pipeline (51 Rust handlers, 20 event types) |
| **ESAA** | Event Store + State Analytics (append-only JSONL + hash chain) |
| **GoalTracker** | 9×9 matrix scoring system (9 dimensions × weighted composite) |
| **GraphService** | Focus tracker that injects `graph_ctx` into every MCP tool response |
| **MCP** | Model Context Protocol (Anthropic standard for tool integration) |
| **RLM** | Recency-Frequency-Magnitude memory tiering system (5 tiers) |
| **TD(λ)** | Temporal Difference learning with eligibility traces (Q-learning variant) |
| **TOON** | Touring's checkpoint format (.toon extension, JSON content) |
| **Touring** | The unified Rust intelligence layer for Claude Code |


---

## v9.2.0 Changelog — RL Quality Enhancement (2026-03-21)

### New Capabilities

#### Rich Reward Signal (S1)
- `_compute_rich_quality_reward()`: 5-dimensional quality reward [0.0, 1.0]
  - lint_score (0.30): ruff/eslint violations → continuous penalty
  - test_score (0.30): pytest/cargo test pass/fail
  - type_score (0.20): pyright/mypy errors
  - complexity_score (0.10): C901 violations
  - regression_score (0.10): new violations vs baseline
- Replaces flat reward {0.52-0.88} with continuous [0.0, 1.0]

#### File Risk Memory (S2)
- `file_risk_scorer.py`: correlates edit_history × bash_outcomes → 580 file risk scores
- `knowledge.rs:file_risk_score()`: query risk for file
- `knowledge.rs:increment_file_risk()`: auto-increment on PostToolUseFailure
- `pre_edit.rs` Signal 5: injects `⚠ file_risk: HIGH (62%)` when risk ≥ 0.3

#### Feature Enrichment (S3)
- `extract_features_rich()`: slots [10..11] now continuous quality context
  - [10] = quality_score (0.0-1.0, fallback: 0.8 if no errors, 0.2 if errors)
  - [11] = file_risk (0.0-1.0, default 0.0)
- Replaces binary error one-hot with richer signals for LinUCB

#### Anti-Regression (S4)
- Pattern 5 in `touring_learning_loop.py`: stores `lesson:regression:{file}` after edit→failure
- `_store_regression_note()`: writes to `file_knowledge.notes` for Signal 3 pickup
- Data flow fix: `_fetch_edit_outcomes` + `_fetch_bash_outcomes` now populate file_path+timestamp

#### Reward Calibration (S5)
- `ImmediateReward.quality_score: Option<f64>`: when Some, maps [0,1]→[-1,1]
- `OnlineRLConfig.forced_explore_interval: u64 = 100`: picks coldest arm periodically
- Prevents LinUCB arm starvation (5/8 arms were cold with 5 pulls each)

### Meta-Improvement

#### Completion Gate (E1)
- `SessionStopHandler`: verifies edited files were E2E tested (VERIFIED/COMPILED_ONLY/UNTESTED)
- `TaskCompletedHandler`: mini completion gate per task
- Prevents false "everything complete" claims

#### Gotcha System (E2+E3)
- `record_audit_finding()`: stores audit-learned anti-patterns as gotchas
- `pre_edit.rs` Signal 6: injects matching gotchas via `get_gotchas_for_file()`
- 29 gotchas in DB (24 original + 5 from audits)

#### PromptRecallHandler (4-source)
- Source 1: file_knowledge notes (knowledge.db — consolidated v29.8.0)
- Source 2: rlm_memory insights/lessons/regression patterns (memory.db — consolidated v29.8.0)
- Source 3: QTable top action (RL suggest)
- Source 4: gotcha scan by prompt keywords

#### McpRecommendHandler (8-trigger)
- Grep/Glob → touring_ast_find (symbol search)
- Edit (function) → touring_ast_edit (replace_body)
- Edit (rename) → touring_refactor (rename + validate)
- Edit (high-risk) → touring_speculate (shadow validate)
- TaskCreate → touring_decompose (formal DAG)
- EnterPlanMode → touring_mcts_search (MCTS planning)
- Agent → touring_session (check state before subagent)

### Lifecycle Enhancements
- `PostToolUseFailureHandler`: auto-increments file_risk
- `PreCompactHandler`: saves edit snapshot + MCP hints
- Pattern 6 (skill clustering): groups tools by performance
- `record_insight()`: stores ★ Insight blocks + RL reward
- G8 rule: insights must be auto-stored

### Tool Coverage
- Before: 0/26 MCP tools actively called by LLM
- After: 18 AUTO + 8 RECOMMEND = **26/26 (100%)** integrated

### Bugs Found by 4 Audits
1. `lstrip("./")` → `startswith("./")` (Python character set vs substring)
2. `_fetch_edit_outcomes` missing file_path/timestamp (data flow gap)
3. `record_insight` argument order swap (API contract mismatch)
4. Hook format: `{hooks:[{type:"command",...}]}` not `{command:...,args:[...]}`

### Stats (v11.0.0)
- **1194 Rust tests**, 0 failures
- **31 cortex handlers** (was 30)
- **6 pre-edit signals** (was 4)
- **580 file risk scores** computed
- **29 gotchas** in knowledge DB
- **Binary**: 11MB, clippy 0 warnings

---

## v12.0.0 Changelog — ACO Potentialization + Hook Quality Tracking + Cross-Audit

**Date**: 2026-03-22
**Theme**: Potentialização exponencial do ecossistema ACO→AST→Python→Hooks

### New Features

#### touring-learning/aco
- `Display` trait implementations for ALL enums: `OperationMode`, `Complexity`, `ValidationStatus`, `DriftLevel`, `ExecutionStatus`, `TrackerStatus`
- `MutableGeneratorGraph::iter()`: idiomatic Rust iterator over `(&str, &GeneratorNode)` pairs in BTreeMap order
- **Execution status tracking**: `mark_executed()`, `execution_status()`, `is_ready_to_execute()`, `ready_nodes()` — enables runtime DAG execution orchestration
- `TrackerReport::summary()`: single-line summary string (e.g., "PASS composite=0.95 iter=3 (9/9 dims passing)")
- 34 new tests: cycle detection, diamond dependencies, self-dependency, concurrent cache access, all-fail/mixed-critical scenarios, execution tracking, iterator patterns

#### touring-ast
- `SymbolKind::FromStr` implementation: parse strings like `"function"` into `SymbolKind::Function`
- `extract_symbols_batch()`: parallel multi-file symbol extraction via rayon
- Symbol filtering utilities: `filter_by_kind()`, `filter_by_complexity()`, `find_by_name()`
- Symbol builder improvements: `with_complexity()`, `with_parent()` methods
- 17 new tests for all new functionality

#### touring-python
- Bug fix: `PyAstSymbol` test constructors missing `start_byte`/`end_byte` fields
- `DimResult.__repr__()`: shows `DimResult(id='D1', name='Precision', score=0.9000)`
- `DimResult.to_dict()`: returns Python `dict` (not JSON string) with computed fields
- `TrackerReport.__repr__()`: shows status, composite, dim count
- `PyAcoGraph.__repr__()`: shows node count
- `PyQueryCache.__repr__()`: shows len and hit_rate
- `PyEventBuffer.__repr__()`: shows len and ready status

#### touring-hooks
- **ACO bridge integrated into HookRuntime**: `quality_assessment` field, `quality_report()` method, `reset_quality_tracking()` for session lifecycle
- **HookResultCache integrated into HookRuntime**: cache hit/miss for hook computations
- **OutputCapture module** (output_capture.rs): 4 specialized extractors (pytest, cargo test, ruff, generic) with metrics extraction
  - UTF-8 safe truncation (`is_char_boundary` loop)
  - Percentage extraction with `%` suffix priority (2-pass algorithm)
  - Auto-selects extractor based on output content heuristics
- **Integration tests module** (integration_tests.rs): E2E validation of full hook lifecycle
- Session lifecycle: `run_session_start()` now initializes quality tracking, `run_session_stop()` generates final quality report
- 36 new tests (21 output_capture + 11 integration + 4 session quality)

### Bugs Fixed (7 total)
1. **touring-python**: `PyAstSymbol` missing `start_byte`/`end_byte` in 2 test constructors (compilation error)
2. **touring-hooks/output_capture**: `extract_leading_number` used first word instead of first numeric word
3. **touring-hooks/output_capture**: `extract_percentage` dead code in fallback branch (linter corrupted logic)
4. **touring-hooks/output_capture**: UTF-8 panic on multi-byte character truncation (`&summary[..497]`)
5. **touring-hooks/post_bash**: `tracing::debug!` referencing undeclared crate dependency
6. **touring-hooks/session_hooks**: `run_session_start` signature changed to `&mut HookRuntime` without updating call site
7. **touring-hooks/integration_tests**: unused variable `report1` warning

### Cross-Audit Results
- Contract verification: PASS (DimResult, TrackerStatus, backward-compat symbols all matching)
- Invariant verification: PASS (exit 0, cycle detection, TTL expiry, shutdown rejection)
- Edge cases: PASS (empty graph, diamond deps, unsupported language)
- Integration chain: PASS (aco_bridge→tracker→build_report→Python, ast_bridge→ast→Python)
- E2E validation: 8/8 tests PASS (intent classification, PII scanning, pre/post-read, session lifecycle, prompt enhancement, knowledge accumulation, MCP server init)

### Metrics
- **1,497 Rust tests**, 0 failures (was 1,194 in v11.0.0, +303)
- **~63,200 LOC** (was ~61,500, +1,700)
- **Binary**: ~12MB release, clippy deny all, 0 warnings
- **4 crates modified**: touring-learning, touring-ast, touring-python, touring-hooks
- **1 crate impacted**: touring-server (call site fix for `&mut HookRuntime`)

## v13.0.0 Changelog — RL Loop Closure + AST Integration (2026-03-23)

### New Features

**touring-learning** — 5 new modules:
- `bandit/ast_features.rs`: `extract_ast_features(file_path) → [f64; 16]` — 16 AST-derived features (symbol_density, avg_complexity, max_complexity, has_async, is_test_file, public_api_ratio, blast_radius_norm, doc_coverage, error_handler_density, import_count, 6 language-specific dims). Internal `FEATURE_DIM_AST=35` (19+16).
- `bandit/ast_enriched.rs`: `AstEnrichedBandit` — wraps `LinUCBBandit`, auto-fetches AST features before arm selection.
- `rl/risk_adjusted.rs`: `RiskAdjustedQLearning` — ε adapts to blast_radius: high impact → exploit known good action; low impact → explore freely.
- `online_learning/ftrl.rs` (feature-gated: `ftrl`): `FtrlLayer` — incremental FTRL-Proximal via `linfa-ftrl`. Feature importance learning without model rebuild.
- `memory/async_rlm.rs`: `AsyncRlmMemory` — `Arc<RwLock>` hot-path reads + mpsc unbounded channel for background SQLite writes. `store()` returns instantly; `flush()` awaits pending writes.

**touring-ast** — 3 new capabilities:
- `document.rs`: `byte_to_point_safe(idx) → AstResult<(usize, usize)>` — Result-returning variant, no more panic on invalid byte offset.
- `store.rs`: `SymbolChangeSet { added, removed, modified }` + `diff_symbols(old, new) → SymbolChangeSet` + `apply_change_set(&self, changes) → Result<()>` with explicit `ROLLBACK` on error path. 50-100x faster than full re-index.
- `graph.rs`: `SymbolIndex::detect_cycles() → Vec<Vec<String>>` — DFS 3-color algorithm (White→Gray→Black).

**touring-python** — 2 new modules:
- `rl_bindings.rs`: 7 PyO3 functions (`process_reward`, `select_arm`, `update_arm`, `get_q_value`, `get_best_action`, `get_linucb_arm_stats`) + 2 constants (`FEATURE_DIM=19`, `NUM_ARMS=8`). Global singletons via `OnceLock<Mutex<T>>`. Sherman-Morrison update O(d²) ≈ sub-μs.
- `ast_rl_bridge.rs`: `compute_rl_state(file_path) → u64` — DJB2 hash for stable QTable state IDs from file context.

**New hook** — `post_tool_use_rl.py`:
- PostToolUse hook that closes the RL learning loop on every Claude Code tool execution
- 19D feature vector (accepted, latency_norm, errors_norm, cila_norm, file_type_oh[4], quality, reward_norm, zeros×9)
- Calls `select_arm → process_reward → update_arm` atomically
- JSONL fallback queue (`~/.claude/data/rl_reward_queue.jsonl`) when PyO3 unavailable
- Exit 0 invariant: never blocks Claude Code

### Bugs Fixed (4 total)
1. **LinUCB RL loop gap**: `select_arm` was called but `update_arm` was never called — LinUCB explored but never learned (pulls stayed at 0, avg_reward 0.0 for all arms). Fixed by adding `update_arm` binding + calling it in hook.
2. **touring-ast/store.rs**: `apply_change_set` missing explicit `ROLLBACK` — error between `BEGIN IMMEDIATE` and `COMMIT` left connection in pending transaction state. Fixed via closure pattern + `let _ = conn.execute_batch("ROLLBACK")` in `Err` arm.
3. **touring-python/rl_bindings.rs**: `get_linucb_arm_stats` used `.expect()` inside `map()` closure — panics abort Python process. Fixed via `collect::<PyResult<Vec<_>>>()?`.
4. **post_tool_use_rl.py Pyright**: `OnlineRLEngine` was referenced but is not a known attribute of PyO3 module (functions are free, not methods). Fixed: replaced with `claude_learning_kernel.select_arm(features)` + `# type: ignore[attr-defined]`.

### E2E Validation Results
- 5 hook scenarios tested (Edit OK, Bash error, Write OK, empty payload, garbage input): all exit=0 ✓
- Feature vector invariant: 19D for all 8 tested scenarios ✓
- Runtime proof after 120 cycles: Write(6.195) > Read(3.782) > Bash(2.566) in QTable ✓
- All 8 LinUCB arms: pulls > 0 (convergence confirmed) ✓

### Metrics
- **1,874 Rust tests**, 0 failures (was 1,497 in v12.0.0, +377)
- **~71,700 LOC** (was ~63,200, +8,500 | +13%)
- **libclaude_learning_kernel.so**: ~3.1MB (was 2.6MB)
- **3 crates modified**: touring-learning (+4,193 LOC), touring-ast (+3,308 LOC), touring-python (+970 LOC)
- **1 new hook**: post_tool_use_rl.py

---

## v14.0.0 Changelog — Semantic AST Search + Transfer Learning + Burn Transformer + Go/Java + PropTests (2026-03-23)

### New Features

**touring-ast** — 3 additions:
- `src/semantic_search.rs`: `SemanticSymbolIndex` — local 16D embedding store (IndexMap LRU). `embed_symbol(sym) → Vec<f32>`: encodes name_len, SymbolKind discriminant, complexity, is_async, visibility, line, col, parent presence, docstring presence, decorator count, body_len. `find_similar_symbols(query, threshold, limit) → Vec<(String, f64)>`: cosine similarity scan; no touring-learning dependency (avoids circular). 8 inline unit tests.
- `src/languages.rs`: `Lang::Go` and `Lang::Java` added under `#[cfg(feature = "more-languages")]`. Dependencies `tree-sitter-go = "0.25"` and `tree-sitter-java = "0.23"` declared as optional in Cargo.toml.
- `tests/property_tests.rs`: 3 property-based tests via `proptest`: `rust_surgery_idempotent` (body replacement → re-parse recovers same symbols), `blast_radius_monotone` (more imports → blast_radius.file_count ≥ original), `incremental_parse_symbol_count_stable` (re-parsing same content gives same symbol count).

**touring-learning** — 2 additions:
- `src/bandit/transfer.rs`: `TransferLinUCB { bandit: LinUCBBandit, context_similarity: f64 }`. `transfer_from(&mut self, donor: &LinUCBBandit, similarity: f64)`: computes `blend_weight = (similarity * 0.30).min(0.30)`, blends theta via `donor.export()` + `self.bandit.import()`. Cap at 30% ensures donor knowledge never dominates fresh experience. 5 unit tests (including blend_weight cap and export/import roundtrip).
- `src/rl/burn_transformer.rs`: `ContextTransformer<B: Backend>` — three-layer MLP (19→64→64→8) with ReLU activations. Feature-gated: `burn-transformer` feature activates `burn = { version = "0.16", features = ["ndarray"] }`. `ndarray` backend chosen for CI safety (no GPU required). `forward(features: Vec<f32>) → Vec<f32>` returns per-arm utility estimates.

### New Feature Flags
- `touring-ast`: `more-languages = ["dep:tree-sitter-go", "dep:tree-sitter-java"]` — opt-in Go + Java parsing
- `touring-learning`: `burn-transformer = ["dep:burn"]` — opt-in neural arm utility estimation

### Architecture Decisions
- **No dep-cycle**: `SemanticSymbolIndex` uses its own `IndexMap` + cosine (not `LruWorkingMemory` from touring-learning). touring-ast must never depend on touring-learning (circular).
- **burn ndarray backend**: `wgpu` requires GPU driver — breaks CI. `ndarray` is CPU-pure, deterministic, zero-dep.
- **TransferLinUCB blend cap**: 30% max prevents donor arm stats from dominating; 70% remains from fresh exploration. Similarity=1.0 still capped at 30%.

### Metrics
- **1,898 Rust tests**, 0 failures (was 1,874 in v13.0.0, +24)
- **~72,900 LOC** (was ~71,700, +1,200 | +1.7%)
- **2 crates modified**: touring-ast (+242 LOC), touring-learning (+207 LOC)
- **Total since v12.0.0**: 1,898 tests (+27%), ~72,900 LOC (+15%)

---

## v22.0.0 Changelog — S0–S8 Sprint (2026-03-26)

Sprint of 9 features (S0–S8) across 5 crates, completing the intelligence layer capabilities.

### New Modules (12 files)

| Module | Crate | Sprint | Purpose |
|--------|-------|--------|---------|
| `bandit/reminder_bandit.rs` | touring-learning | S0 | ReminderBandit 7-arm LinUCB for adaptive system reminder selection |
| `got.rs` (extended) | touring-cognitive | S2 | `run_parallel_nodes` JoinSet + `GoTEngine::evaluate_parallel` |
| `snapshot.rs` | touring-cognitive | S3 | GoTSnapshot rkyv zero-copy (flat struct, #[archive(check_bytes)]) |
| `session_persistence.rs` | touring-cognitive | S3 | SessionPersistence deadpool-sqlite async pool |
| `pool.rs` | touring-wasm | S4/v28.2 | InferletPool sync + AsyncInferletPool async-native (InstancePre + spawn_blocking), memory write + set_input + evaluate dispatch for WASM ABI, 12 tests |
| `typed.rs` | touring-wasm | S4 | TypedPluginContext/Result, call_evaluate_typed() graded 0–100 |
| `fusion.rs` | touring-cortex | S5 | reciprocal_rank_fusion weighted + rrf convenience |
| `call_graph.rs` | touring-cortex | S5 | CallGraph StableGraph + Tarjan SCC + toposort + hotspots |
| `memory/crdt_graph.rs` (extended) | touring-learning | S6 | CrdtDelta: delta(), merge_delta(), full_delta() |
| `ranking/drift_monitor.rs` | touring-learning | S7 | DriftMonitor KS two-sample test, VecDeque sliding windows |
| `ranking/ebpf_observer.rs` | touring-learning | S7 | EbpfObserver feature-gated stub (feature="ebpf") |
| `branch_fs.rs` | touring-hooks | S8 | BranchFs CoW snapshots — TempDir auto-rollback, SHA-256 drift |
| `observability/ring_buffer.rs` | touring-learning | v24.1 | RingBuffer fixed-capacity circular buffer, FIFO overwrite, 7 tests |
| `memory/rle.rs` | touring-learning | v24.1 | RLE codecs for CrdtDelta fields: u64, u64_pair, str; compression_ratio_u64, 16 tests |
| `cache_manager.rs` | touring-wasm | v24.1/v28.2 | KvCacheManager + AsyncKvCacheManager traits; InMemoryCacheManager + AsyncInMemoryCacheManager + WasmCacheManager + fast_instantiation_config, 7 tests |
| `handlers/wasm.rs` | touring-cortex | v28.2 | H83 TypedEvaluateHandler + PluginRegistry; typed WASM evaluation with scored results (0–100), 11 tests |

| Algorithm | Location | Complexity |
|-----------|----------|------------|
| LinUCB Sherman-Morrison (ReminderBandit) | bandit/reminder_bandit.rs | O(d²) per update |
| KS two-sample test (DriftMonitor) | ranking/drift_monitor.rs | O(n log n) sort + O(n+m) scan |
| Kolmogorov p-value series | ranking/drift_monitor.rs | 20-term series, clamped [0,1] |
| Tarjan SCC (CallGraph) | touring-cortex/call_graph.rs | O(V+E) |
| Reciprocal Rank Fusion (sequential) | touring-cortex/fusion.rs | O(Σ|list_i|) |
| RRF Parallel (rayon+DashMap) | touring-cortex/fusion.rs | O(Σ|list_i|/P) wall-clock |
| tiktoken cl100k_base tokenization | touring-cortex/enrichment.rs | O(n) |
| Zstd encode/decode + base64 | touring-cortex/cache_strategy.rs | O(n) |
| LRU cache peek (read-only) | touring-cortex/pipeline.rs | O(1) expected |
| CRDT set-difference delta | memory/crdt_graph.rs | O(|nodes|+|edges|) |
| GoT parallel evaluation | cognitive/got.rs | O(N/P) wall-clock with P workers |
| SHA-256 file drift (BranchFs) | hooks/branch_fs.rs | O(file_size) |

### Design Decisions

1. **Arc<GotNode> over Clone**: `GotNode` intentionally doesn't implement `Clone` (contains `Box<dyn Fn>` closures); `Arc` enables sharing without cloning.
2. **rkyv flat snapshot**: Archive derivation requires all fields to implement `Archive`; `Arc`/`Box<dyn>` don't. Flat `GotNodeSnapshot` with String IDs is the correct design.
3. **insert_axis + broadcast over into_shape**: ndarray 0.16 deprecated `into_shape()`. Outer product `col × row` via `insert_axis(Axis(1)) × insert_axis(Axis(0))` also avoids `indexing_slicing` clippy lint.
4. **into_raw_vec_and_offset().0**: Replacement for deprecated `into_raw_vec()` in ndarray 0.16.
5. **LinUCB exploration priming**: All arms must be touched before testing arm preference in tests — untouched arms have permanent exploration bonus α·‖x‖ ≈ 1.63 that exceeds exploitation of any single rewarded arm.
6. **deadpool-sqlite interact pattern**: `conn.interact(|conn| { ... }).await` — synchronous closure runs on thread pool; all captured variables must be moved (`.to_string()` clones before entering closure).
7. **BranchFs #[must_use] on commit()**: Prevents silent rollback from implicit TempDir drop — defensive API design.
8. **EbpfObserver as compile-time stub**: `cfg(feature = "ebpf")` ensures zero runtime overhead when disabled; CI always runs without the feature.
9. **DashMap `and_modify` over `or_insert`**: Compound assignment (`or_insert(0.0) += v`) fails on `RefMut`; correct pattern: `.and_modify(|e| *e += contribution).or_insert(contribution)`.
10. **RwLock peek for fast-path cache hits**: `cache.peek(&key)` (read-only) avoids LRU eviction on hot hits; only `write()` on miss triggers eviction.
11. **SCC cache returns owned `Vec`**: `&cached.sccs` with live `RwLock` guard causes borrow lifetime issues; return `sccs.clone()` for cache hits.
12. **Zstd binary → base64 → UTF-8 string**: `encode_all()` produces raw binary bytes; `String::from_utf8()` panics; wrap with base64 encoding (`!:b64:` prefix) for safe storage.
13. **tiktoken-rs over tiktoken crate**: tiktoken 3.x requires Rust 1.94; tiktoken-rs 0.9 is compatible with Rust 1.75 and exposes `cl100k_base()` directly.
14. **AsyncInferletPool uses InstancePre + spawn_blocking**: WASM execution is CPU-bound; `spawn_blocking` moves it to a thread pool without blocking the async executor. `InstancePre<()>` enables pre-linked instances with near-zero instantiation cost.
15. **#[async_trait] for AsyncKvCacheManager**: Provides drop-in async replacement for sync `KvCacheManager` trait with `async_trait` crate, enabling seamless integration with existing caching infrastructure.

### Metrics

| Metric | v21.1.0 | v22.0.0 | v24.1.0 | v24.2.0 | **v28.0.0** | **v28.2.0** | Delta |
|--------|---------|---------|---------|---------|------------|------------|-------|
| Tests | 2,671 | 2,840 | 2,894 | 3,040 | **3,316** | **3,422** | **+522 (3,944 total)** |
| LOC | ~99,900 | ~101,500 | ~102,500 | ~103,100 | **~106,500** | **~107,200** | **+700** |
| New public types | — | 26 | 32 | 32 | **32** | **38** | **+6** |
| Feature flags | 4 | 6 | 8 | 8 | **8** | **8** | 0 |
| Clippy warnings | 0 | 0 | 0 | 0 | **0** | **0** | 0 |
| SCHEMA_VERSION | 4 | 4 | 4 | 5 | **6** | **6** | 0 |
| Cortex handlers | 81 | 81 | 81 | 81 | **82** | **82** | 0 |
| Cross-audit gates | 10/10 | 9/9 | 9/9 | 9/9 | **9/9** | **9/9** | PASS |

### Cross-Audit Results (v28.2.0)

| Gate | Result | Evidence |
|------|--------|----------|
| Functional (tests pass) | PASS | 3,422 passed, 0 failed |
| Clippy (0 warnings) | PASS | deny all — 0 warnings |
| Contract (APIs match docs) | PASS | All types match documented signatures |
| Invariants (exit 0) | PASS | Wiring hooks exit 0 on error; cascade invalidation never panics |
| Edge cases | PASS | Wiring audit: 5 bugs fixed (2 P0, 3 P1); cortex: 7 gaps resolved; async pool: 7 new tests |
| Integration (cross-crate) | PASS | touring-cortex↔touring-wasm (H83), touring-hooks↔touring-wasm (InferletService), TfIdf→Nexus, AdaptiveEngine→Runtime wired E2E |
| No regressions | PASS | All 3,316 pre-existing v28.0.0 tests still pass |
| Memory safety | PASS | 0 unwrap() in production code; all Arc/Mutex patterns verified |
| Documentation drift | PASS | ARCHITECTURE.md, CLAUDE.md, touring-system.md, MEMORY.md all updated to v28.2.0 |
