# Changelog — touring-hooks

All notable changes to this crate will be documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) | Versioning: [SemVer](https://semver.org/)

---

## [Unreleased / v0.3.8-hooks] — 2026-04-12 (evening)

### Changed — Daemon Actor Refactor (Session f0e87f2d)

- **`daemon.rs` — Actor pattern replaces `Arc<Mutex<HookRuntime>>`**:
  - New `ProjectCommand` enum with `RunHook { hook_name, payload, response: oneshot::Sender<String> }` and `Shutdown { done: oneshot::Sender<()> }` variants.
  - New `PROJECT_CHANNEL_DEPTH = 128` bounded mpsc per project; producers block via `send().await` under backpressure rather than leaking FDs.
  - New `run_project_actor(runtime, cmd_rx)` function — dedicated OS thread (`std::thread::Builder` named `touring-project-actor`) owns the `HookRuntime` and processes commands serially. `!Sync` rusqlite constraint preserved without kernel Mutex contention.
  - Panic-safe: every handler invocation and E2E scan wrapped in `std::panic::catch_unwind(AssertUnwindSafe(...))`. Handler panic logs `tracing::error!` and continues to next command instead of dropping the channel receiver and killing the actor.
  - Shutdown lifecycle: `ProjectCommand::Shutdown { done }` runs WAL checkpoint + LinUCB save + CRDT save inside the actor (panic-guarded per step), acks via oneshot. `graceful_shutdown` awaits each actor with 5s timeout.
  - `ProjectRuntime` struct now holds `cmd_tx: mpsc::Sender<ProjectCommand>` + `last_accessed` + per-project `connection_semaphore`. `rt: Arc<Mutex<HookRuntime>>` removed.

- **Accept loop resilience**:
  - Exponential backoff on `listener.accept()` errors: `100ms × 2^streak` capped at 2s. Previous tight retry spun on EMFILE/ENOBUFS and made resource exhaustion worse.
  - `accept_error_streak` counter resets on success, so transient errors don't accumulate budget.

- **Handler budgets**:
  - Light hooks: 15s (detect stuck handlers quickly).
  - Heavy hooks: **300s** (raised from 60s). Expanded `is_heavy_hook()` to include `cli-tantivy-reindex`, `cli-wiring-chains`, `cli-wiring-audit`, `cli-e2e`, `cli-ast-blast-cross-feature`.

- **Per-project semaphore timeout**: `project_sem.acquire_owned()` now wrapped in `timeout(REQUEST_TIMEOUT, ...)`. Prevents indefinite FD accumulation when hook storm saturates the 56-slot per-project budget.

- **Per-project connection budget** raised from 16 to **56** slots to absorb Claude Code hook bursts.

### Added — Tantivy Batched Reindex + Composite Dedup

- **`cli_handlers.rs::cli_tantivy_reindex`**: payload now accepts `{mode: "batch"|"full", offset: u64, limit: u64, clear: bool}`. Batch mode returns `{reindexed, done, mode, upserted, next_offset, stats}` enabling client-side looping. `full` mode preserves legacy single-call semantics but may exceed handler budget on >500k-symbol workspaces.

- **`cli/tantivy.rs::run` (reindex branch)**: CLI-side batching loop with flags `--batch-size N` (default 25000), `--resume N`, `--full`. Emits per-batch progress to stderr and final JSON summary to stdout with `{batches, total_upserted, final_offset, elapsed_secs, final_stats}`.

- **`tantivy_index.rs::upsert_symbol`**: dedup key changed from `symbol_name` (which clobbered all homonymous siblings — e.g., `description` appears 3,932× in the workspace) to composite `blake3(symbol_name | file_path | line)` stored in `blake3_hash` field (STRING|STORED). Auto-derives hash when caller passes `None`. Eliminates 93% data loss observed in prior schema: 1.1M upserts previously produced 75,696 `total_docs`; now produces 1,097,892.

### Validated

- Full reindex from `/home/gabrielgadea` (1,097,890 symbols in `.claude/touring/symbols.db`) completed in 2m14s across 44 batches of 25k rows. Final Tantivy index: 190MB, 1,097,892 `total_docs`, 2,240 `total_commits`.
- BM25 search returns real hits: `HookRuntime` → 2 definitions with score 9.53; `description` / `symbols_page` return correct file:line coordinates.
- Daemon under hook load: 2 ESTAB connections, 454MB RSS, no EAGAIN, no stuck handlers. Previous state: 50+ ESTAB connections blocked in `futex_wait` on shared Mutex.

### Fixed

- **Broken symlink**: `~/.claude/hooks/touring-hook` → `~/.claude/rust/target/release/touring-hook` target was missing (build artifacts cleaned). Rebuilt restores the target; Claude Code hooks no longer fail with "file not found".
- **Socket EAGAIN under hook storm**: root cause was unbounded `sem.acquire_owned().await` on both global (64) and per-project (16→56) semaphores; tasks accumulated holding socket FDs until kernel listen backlog filled. Fixed by wrapping both acquires in `tokio::time::timeout(REQUEST_TIMEOUT, ...)` and returning fail-fast rejections.

---

## [v0.3.7-hooks] — 2026-04-12 (morning)

### Added — Pre-Hooks Potentialization (Session ff228469)

- **`InfraRuntime::pheromone_graph` (hook_runtime.rs:629)**: New `Arc<RwLock<PheromoneGraph>>`
  field on `InfraRuntime`. Initialized in `HookRuntime::new()` via
  `Arc::new(RwLock::new(PheromoneGraph::new(0.1)))`. Shared with `PheromoneGraphSignalLayer` in
  pre_read without additional parameter passing — single owner in InfraRuntime, cheap Arc::clone
  at call site. Must use `std::sync::RwLock` (not tokio) — `PheromoneGraphSignalLayer::new()`
  requires the stdlib variant.

- **4 orphaned SignalLayers wired into pre_read pipeline (pre_read.rs:636-655)**:
  - `EnrichedBlastRadiusSignalLayer` → `CilaGatedLayer(threshold=3)` — blast radius with enrichment
  - `WeightedBlastSignalLayer` → `CilaGatedLayer(threshold=4)` — weighted blast scoring
  - `PheromoneGraphSignalLayer` → `CilaGatedLayer(threshold=2)` — ACO pheromone trails
  - `HnswSignalLayer` → `CilaGatedLayer(threshold=5)` — HNSW ANN semantic neighbors
  All 4 use shared `Arc<SymbolIndex>` and `Arc<RwLock<PheromoneGraph>>` from InfraRuntime.

- **5 Tantivy BM25 signals wired into pre_write pipeline (pre_write.rs:331-351)**:
  All feature-gated `#[cfg(feature = "tantivy-fts")]`. Mirror of pre_read's
  `collect_index_signals()` pattern applied to `collect_upfront_signals()`:
  - `tantivy_related_docs_signal` (weight 0.70)
  - `tantivy_fuzzy_file_signal` (weight 0.65)
  - `tantivy_kind_context_signal` (weight 0.73)
  - `tantivy_crate_origin_signal` (weight 0.68)
  - `tantivy_fuzzy_symbol_signal` (weight 0.62)

- **SignalPipeline budget-aware truncation in pre_edit (pre_edit.rs:283-286)**: Assembled
  context now wrapped in `SignalPipeline::new(budget).add_layer(StaticSignalLayer::new(...))`.
  Budget computed via `cila_budget_edit(cila_level)`: L0=1200, L2=3000, L4=6000 chars.
  Ensures consistent truncation at CILA budget boundary across all pre-hooks.

### Fixed — Pre-Hooks Potentialization

- **CILA hardcode bug in pre_read::build_parallel_signal_pipeline (pre_read.rs:602)**: 
  Was `fn build_parallel_signal_pipeline(runtime, file_path, rel_path, remaining)` with
  internal `.with_cila(3)` hardcoded, disabling dynamic CILA escalation for graph layers.
  Fixed by threading `cila_level: usize` parameter and passing to `SignalContext::with_cila()`.

- **`circuit_breaker::load_cached_state` idempotency (circuit_breaker.rs:399)**: Added early
  return `if CIRCUIT_CACHE.get().is_some() { return; }`. Root cause: race condition in parallel
  tests — concurrent `init()` calls would overwrite live in-memory cache state with stale disk
  data after `reset()`. Fixed the flaky `catastrophic_resets_on_success` test. Result: 1554
  passed, 0 failed consistently (was intermittent failure with concurrent test threads).

### Quality — Pre-Hooks Potentialization

- `cargo check --workspace`: **EXIT:0** — 0 errors
- `cargo test -p touring-hooks`: **1554 passed, 0 failed, 1 ignored**
- RL rewards: `learning reward edit 1.0` + `learning reward orchestrate 1.0` injected

---

## [Unreleased / v0.3.6-hooks] — 2026-04-12

### Added — Wave 2-4 Tantivy FTS Integration

- **`tantivy_related_docs_signal` (signals.rs:396)**: BM25 search for related symbols by module
  context. Returns up to 3 related file:symbol pairs at weight 0.7. Feature-gated `tantivy-fts`.

- **`tantivy_fuzzy_file_signal` (signals.rs:483)**: Levenshtein fuzzy search for files with similar
  path basenames. Helps discover test/impl counterparts. Weight 0.65. Min 3-char basename.

- **`tantivy_kind_context_signal` (signals.rs:531)**: Extracts symbol kind distribution
  (struct/function/trait/module) from related files in the index. Weight 0.73. Returns top 3 kind
  categories with hit counts.

- **`tantivy_crate_origin_signal` (signals.rs:580)**: Finds other files in the same crate via
  Tantivy crate-scoping. Weight 0.68. Requires path under `crates/<name>/`.

- **`tantivy_fuzzy_symbol_signal` (signals.rs:633)**: Fuzzy search for symbols with similar names
  across the codebase. Weight 0.62. Min 4-char basename, 2+ similar symbols required.

- **`SearchHit::cognitive_score` (tantivy_index.rs:73)**: Extended `SearchHit` struct with
  `cognitive_score: Option<f64>` field. Populated via `extract_cognitive_score()` helper that
  converts `cognitive_score_x1000` u64 field (stored as x1000 integer) to f64/1000. Enables
  cognitive enrichment queries on search hits.

- **`FastMetadata::with_language` (metadata_collector.rs:59)**: Builder-style enrichment using
  `extension_to_language()` from `tantivy_index`. Maps file extension → language string.
  Idempotent — no-op if already enriched.

- **`FastMetadata::with_feature_flags` (metadata_collector.rs:74)**: Walks up from file path to
  find `crates/<name>/Cargo.toml`, parses `[features]` section. Falls back to empty vec on any
  error. Enables feature-flag awareness in pre_read context.

- **`FastMetadata::with_cognitive_from_index` (metadata_collector.rs:82)**: Queries Tantivy index
  for all symbols in this file, computes average `cognitive_score` across hits. Stores result as
  `language = Some(format!("cognitive:{:.2}", avg))` for downstream consumption.

- **All 5 Tantivy signals wired in `collect_index_signals()` (pre_read.rs:560-590)**: Each signal
  conditionally pushed when `#[cfg(feature = "tantivy-fts")]` and returning `Some`. Signal tower
  fully orthogonal — no pipeline changes needed for new signals.

### Added — E2E Test Suite

- **`tantivy_signals_e2e.rs` (12 tests)**: Smoke tests for all 5 Tantivy signal functions plus
  FastMetadata enrichment chain. Covers: never-panic on valid/null input, valid weight range
  [0.0, 1.0], edge cases (short path < 3 chars, empty path), full enrichment chain
  `with_language + with_feature_flags + with_cognitive_from_index`.

### Changed — SCHEMA_VERSION

- **`SCHEMA_VERSION = 8`**: Auto-creates enrichment tables (`cognitive_enrichment`,
  `module_ecosystem`, `file_blake3_registry`, `file_test_coverage`, `file_communities`) via
  `CREATE TABLE IF NOT EXISTS` idempotent migration.

### Quality

- `cargo test -p touring-hooks --features tantivy-fts`: **1.731 tests passed** (1554 unit + 177 integration E2E)
- `cargo test -p touring-hooks --features tantivy-fts --test tantivy_signals_e2e`: **12/12 PASS**
- `cargo test -p touring-hooks --features tantivy-fts --test wave2_4_e2e`: **20/20 PASS**
- `cargo check --workspace`: **EXIT:0** (101 warnings, all non-touring-hooks crates)
- Warning fixed: removed `unused import: std::io::Write` in `tantivy_signals_e2e.rs:89`

---

## [Unreleased / v0.3.5-hooks] — 2026-04-11

### Added

- **`RL-WARM (neural.rs:37)**: Added post-tool-rl match arm to `neural.rs` HookRuntime dispatch. Closes RL cold-start gap: thin-client HookRuntime now forwards reward signals to daemon RL engine. Daemon restart required after this change — G1 iter5 TACO.

- **`WIRE-TOP100 (hook_runtime.rs:699)**: `KnowledgeSymbolBridge` wired to `SymbolStore::subscribe`. Confirmed real orphan via VP-Scout chain: consumer=1 after wiring audit. Pattern: use `touring index find` to locate consumers before declaring orphan — G2 iter5 TACO.

- **`AST-CC1 (touring_cli.py)**: Refactored `touring_cli.py` — 3 pure helper functions extracted. Cyclomatic complexity reduced from 16 to 10. Pattern: extract pure/transform helpers, keep orchestrator for control flow — G3 iter5 TACO.

- **`AST-CC2 (inferlets lib.rs)**: Refactored `inferlets lib.rs` — 2 pure helper functions extracted. Cyclomatic complexity reduced from 17 to 10. Same CC-reduction pattern as AST-CC1 — G4 iter5 TACO.

### Changed

- **`Cargo.toml` (line 52)**: `touring-analysis` features expanded from `["ann-blast"]` to
  `["ann-blast", "simd-wiring", "simd-temporal-ac"]`. Activates the aho-corasick SIMD paths inside
  `scan_dead_patterns` and `detect_churn_patterns`; without these flags both functions return empty
  vecs (soft-disable, no API change for callers).

### Changed

- **`truncate_context` (hook_runtime.rs:250, hook_response.rs:250)**: Deduplicated — single canonical source now at `hook_response.rs:250`. Updated 5 call sites in `hook_runtime.rs` to use `super::truncate_context`. Implementation upgraded to robust `is_char_boundary` UTF-8 safe check (was naive `context.len() <= MAX`). Cycle 7 G1 TACO.

- **`antipattern_signals` / `verify_antipatterns` (pre_write.rs:652, post_write.rs:611)**: Verified already wired — both functions call `crate::shared::antipatterns::detect_antipatterns` directly. No code changes needed. Cycle 7 G2 TACO.

### Quality

- `cargo test -p touring-hooks --all-features`: 1,354 passed, 0 failed (unchanged from v0.3.4)
- `cargo clippy -p touring-hooks --all-features -- -D warnings`: 0 warnings
- New integration coverage: `touring-integration-tests/tests/pln2_e2e.rs` — 27 tests cover all
  pln2 hook-facing APIs (`scan_dead_patterns`, `detect_churn_patterns`, `WiringFingerprintStore`,
  `analyze_wiring_incremental`, `OtelConfig`, `analysis_reward_from_report`, `AnalysisInsights`)

### Cycle 8 (2026-04-12) — touring-hooks deep analysis

- **`ENG-1 (errors.rs:81)**: Added `From<rusqlite::Error> for TouringError` impl. Enables clean `?`
  propagation across 18 functions in `hook_memory.rs`, `ann_memory/persistence.rs`, and
  `async_knowledge.rs`. Removes 18x `#[allow(clippy::unwrap_used)]` suppressions.

- **`ENG-3 (hook_runtime.rs:450,515-535)**: `PatternBandit` consolidated to
  `LearningRuntime.pattern_bandit` shared field. Fixes double-init issue. Single source of truth.

- **`ENG-4 (hook_runtime.rs:655,813)**: Added `triad_state: RefCell<Option<TriadState>>` field.
  Enables TRIAD write protection state persistence across pre_write/post_write cycle.

- **`ENG-5 (pre_write.rs:89)**: `run_pre_write` wired — TRIAD pre-write hook now active.

- **`ENG-6 (post_write.rs:245,249)**: `run_post_write` wired with `take()` pattern — TRIAD
  post-write rollback protection active.

- **`ENG-7 (hook_runtime.rs:614,805,1107-1113)**: Added `n1_bridge: Option<N1Bridge>` field.
  Lazy initialization via `init_n1_bridge()`. Connects HookRuntime (N0) to N1 layer.

- **`ENG-8 (pre_tool_use.rs:63,67-81)**: `compute_n1_delegation()` integrated with scout_context
  merge. CILA L4+ tasks now delegated to N1 sequence generator.

- **`Bonus fixes`**: Removed unused import `crate::triad_hook::TriadState` (hook_runtime.rs:51).
  Fixed `n1_sequence` unused variable by merging into scout_context (pre_tool_use.rs).

---

## [Unreleased / v0.3.5-hooks] — 2026-04-07

- **`cli_wiring_orphans` in `src/cli_handlers.rs`**: After the orphan DB query, calls
  `touring_analysis::scan_dead_patterns(&symbol_names)` (aho-corasick, `simd-wiring` feature) and
  returns enriched JSON `{"orphans":[...], "dead_patterns":[...], "orphan_count": N}`.
  Consumers get dead-code pattern matches alongside orphan list in one response — G1 pln2.

- **`phase_wiring` in `src/cli_e2e.rs`**: Replaced ad-hoc orphan SQL with
  `analyze_wiring_incremental(conn, &mut fp_store)` using a fresh `WiringFingerprintStore` per
  call. Fingerprint store skips unchanged modules, reducing analysis time on large codebases — B7 pln2.

- **`phase_wiring` in `src/cli_e2e.rs`**: Calls `detect_churn_patterns(&module_files)` on the
  list of module file paths. `churn_pattern_count` (usize) emitted in phase metrics JSON alongside
  existing orphan/wiring metrics — G2 pln2.

- **Deep path in `src/cli_e2e.rs`**: Initialises `OtelConfig::from_env()` + `init_otel_subscriber`
  in the deep analysis path only (no-op in quick/standard) — F1 pln2.

- **Cross-validation in `src/cli_e2e.rs`**: Calls `analysis_reward_from_report(&cross_report)` and
  emits `rl_reward: f64` in phase metrics — RL reward loop closed for cross-analysis phase — G3 pln2.

### Changed

- **`Cargo.toml` (line 52)**: `touring-analysis` features expanded from `["ann-blast"]` to
  `["ann-blast", "simd-wiring", "simd-temporal-ac"]`. Activates the aho-corasick SIMD paths inside
  `scan_dead_patterns` and `detect_churn_patterns`; without these flags both functions return empty
  vecs (soft-disable, no API change for callers).

### Quality

- `cargo test -p touring-hooks --all-features`: 1,354 passed, 0 failed (unchanged from v0.3.4)
- `cargo clippy -p touring-hooks --all-features -- -D warnings`: 0 warnings
- New integration coverage: `touring-integration-tests/tests/pln2_e2e.rs` — 27 tests cover all
  pln2 hook-facing APIs (`scan_dead_patterns`, `detect_churn_patterns`, `WiringFingerprintStore`,
  `analyze_wiring_incremental`, `OtelConfig`, `analysis_reward_from_report`, `AnalysisInsights`)

---

## [Unreleased / v0.3.4-hooks] — 2026-04-07

### Refactored

- **src/cli_handlers_wiring.rs**: Entire file removed (153 lines) — all functions were dead code: `DriftMetricOld`, `populate_drift_from_sql_old`, `populate_ranker_from_sql_old`, `cli_evolution_drift_alias`, `cli_evolution_insights_alias`, `cli_evolution_tools_alias`. Dispatch table never referenced any of them.
- **src/cli_handlers.rs**: ~313 lines removed — `DriftMetricOld` struct (8 lines), 3 evolution alias functions (~210 lines), 2 `populate_*_old` helper functions (~90 lines). Verified dead via dispatch table inspection.
- **src/cli_handlers.rs**: ~350 lines removed — 5 duplicate `cli_index_*` functions and 3 duplicate `cli_ast_*` functions; dispatch correctly routes to `cli_handlers_index` handlers instead.
- **src/cli_handlers_index.rs:502,507**: `&symbol` corrected to `symbol` — needless borrow clippy warning.

### Added

- **Signal 15b in `pre_edit.rs`** — Per-dimension quality alerts from `MetricsDashboard::alerts_below(0.8)`
  injected at score 1.5, ranking above the composite Signal 15 (score 1.3) so actionable dimension
  degradations (e.g. `"ALERT [wiring] score=0.55"`) surface before the aggregate summary line.
  Budget-reuses `health_report` already computed in Signal 15 — zero additional pipeline overhead.
  Budget gate: `signals.len() < 10`.

- **`quality_depth_signals(content, file_path)` in `pre_write.rs`** — New private function wired
  into `collect_upfront_signals()` that gates writes on complexity and unwrap density:
  - `analyze_complexity(content, lang).max_complexity > 15` → score 1.4 refactor warning
  - `analyze_complexity(content, lang).avg_complexity > 10.0` → score 1.2 splitting suggestion
  - `analyze_unwraps(content).risk_score > 0.3` → score 1.6 with line-level callout (first 3 lines)
  - Test files bypassed via `is_test_file()` guard; `signals.len() < 8` prevents overflow.
  - `analyze_antipatterns` excluded: homonym with `crate::shared::antipatterns::detect_antipatterns`
    already active in the hook pipeline (VP-Scout Homonimia Chain applied).

- **HEALTH issue in `post_edit.rs` `phase2_verification()`** — After existing issue collection,
  runs `AnalysisPipeline::new(conn_ref(), AnalysisConfig::hook_path()).run(rel_path)`:
  - Non-empty `alerts_below(0.8)` → `"HEALTH {alerts joined by ', '}"` issue
  - No alerts but `composite_score < 1.0` → `"HEALTH {one_liner}"` fallback
  - Budget gate: `issues.len() < 8`.

- **`HEALTH` arm in `issue_priority()` in `post_edit.rs`** — Score 1.2, placed between
  WIRING (1.0) and COMPLEXITY (1.5). Full priority ladder:
  `SYNTAX/SYMBOL/STRUCTURAL/IMPORT/CFG=2.5 > ANTIPATTERN/API=2.0 > COMPLEXITY=1.5 > HEALTH=1.2 > WIRING=1.0 > feature-gated=0.8 > default=0.5`

- **HEALTH signal in `post_write.rs` `collect_quality_issues()`** — After V4 wiring check,
  builds `AnalysisPipelineBuilder::new(conn_ref()).config(hook_path()).with_files(files).build()`
  and emits `"HEALTH {one_liner}"` when `!summary.passes` (passes = composite_score >= 0.8).
  Budget gate: `all_issues.len() < 6`. Uses `content` already in scope — no extra disk read.

- **5 regression tests** covering the new integration points:
  - `post_edit::tests::test_issue_priority_health_above_wiring`
  - `post_edit::tests::test_issue_priority_health_below_complexity`
  - `pre_write::tests::test_quality_depth_test_file_skipped`
  - `pre_write::tests::test_quality_depth_high_cc_triggers_signal`
  - `pre_write::tests::test_quality_depth_unwrap_density_triggers_signal`

### Changed

- `pre_write.rs` `collect_upfront_signals()` — Calls `quality_depth_signals()` as additional
  signal source alongside existing antipattern and speculative validation signals.
- `post_edit.rs` `phase2_verification()` — Extended with HEALTH dimensional block after E13
  API surface diff check.
- `post_edit.rs` `issue_priority()` — New HEALTH arm at 1.2 between WIRING and COMPLEXITY.
- `post_write.rs` `collect_quality_issues()` — Extended with HEALTH block after V4 wiring check.

### Quality

- `cargo check -p touring-hooks --all-features`: 0 errors
- `cargo clippy -p touring-hooks --all-features -- -D warnings`: 0 warnings
- `cargo test -p touring-hooks --all-features`: 1354 passed, 0 failed
- touring-analysis APIs consumed: `analyze_complexity`, `analyze_unwraps`, `AnalysisPipeline`,
  `AnalysisPipelineBuilder`, `MetricsDashboard::alerts_below`, `to_analysis_summary`,
  `one_liner`, `passes`, `AnalysisConfig::hook_path`, `conn_ref()`

---

## [Prior to v0.3.4] — historical

See `crates/touring-analysis/CHANGELOG.md` for the analysis engine changes that enabled this
hook chain integration (v0.3.1 through v0.3.3 added the APIs consumed here).
