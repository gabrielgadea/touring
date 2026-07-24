# Changelog — touring-analysis

All notable changes to this crate will be documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) | Versioning: [SemVer](https://semver.org/)

---

## [0.3.5] — 2026-04-07

### Changed

- **`simd-wiring` feature now activated by `touring-hooks`**: `touring-hooks/Cargo.toml` adds
  `"simd-wiring"` to its `touring-analysis` feature list. When enabled, `scan_dead_patterns()`
  in `src/wiring/orphan.rs` uses `aho_corasick::AhoCorasick` for multi-pattern matching across
  `["_unused", "_dead", "_old", "_deprecated", "_legacy", "_stub"]`. Without the flag the function
  returns an empty vec (soft-disable). No API change — caller behaviour identical.

- **`simd-temporal-ac` feature now activated by `touring-hooks`**: `touring-hooks/Cargo.toml` adds
  `"simd-temporal-ac"` to its `touring-analysis` feature list. When enabled,
  `detect_churn_patterns()` in `src/temporal/trends.rs` uses `aho_corasick::AhoCorasick` for
  multi-pattern matching across `["tmp/", "_bak", ".bak", "_old", "_backup"]`. Without the flag
  returns an empty vec. No API change.

### Quality

- Feature activation verified: `cargo check -p touring-hooks --all-features` — 0 errors
- Both `scan_dead_patterns` and `detect_churn_patterns` have doc comments describing feature-gate
  behaviour and the exact pattern sets they match (see `src/wiring/orphan.rs:126-138` and
  `src/temporal/trends.rs:346-349`)
- Integration test coverage: `touring-integration-tests/tests/pln2_e2e.rs` tests G1 (`scan_dead_patterns`)
  and G2 (`detect_churn_patterns`) with concrete inputs

---

## [0.3.4] — 2026-04-06

### Added

- **S-1 pre_edit Signal 15b** — Per-dimension alerts via `MetricsDashboard::alerts_below(0.8)`
  injected at score 1.5 (above Signal 15 composite at 1.3) so degraded dimensions surface before
  the aggregate summary; budget-guarded inside the existing `signals.len() < 10` gate that already
  owns `health_report`, adding zero pipeline overhead
  (ref: `crates/touring-hooks/src/pre_edit.rs`, Signal 15b block)

- **S-2 pre_write `quality_depth_signals()`** — New private function that injects 0–2 quality
  signals before every Write:
  - `max_CC > 15` → score 1.4 `"quality_depth: max_CC=N (>15 threshold) — refactor before writing"`
  - `avg_CC > 10.0` → score 1.2 `"quality_depth: avg_CC=N.N — consider splitting complex functions"`
  - `risk_score > 0.3` → score 1.6 `"quality_depth: N .unwrap() call(s) (risk=N.NN) at lines [L1, L2, L3] — use ? or .expect()"`
  - Test files skipped via `is_test_file()` guard; `signals.len() < 8` prevents overflow
  - APIs used: `touring_analysis::analyze_complexity(content, lang)`, `touring_analysis::analyze_unwraps(content)`
  - `analyze_antipatterns` intentionally excluded — homonym conflict with
    `crate::shared::antipatterns::detect_antipatterns` already active in hook pipeline (VP-Scout Homonimia Chain)
  (ref: `crates/touring-hooks/src/pre_write.rs`, `quality_depth_signals()` + `collect_upfront_signals()`)

- **S-3 post_edit phase2 dimensional health** — Inside `phase2_verification()`, after existing
  issue collection, runs `AnalysisPipeline::new(conn_ref(), hook_path()).run(rel_path)` and emits:
  - Non-empty `alerts_below(0.8)` → `"HEALTH {alerts.join(", ")}"`
  - No alerts but `composite_score < 1.0` → `"HEALTH {one_liner}"` fallback
  - Budget gate: `issues.len() < 8` prevents noise accumulation
  - `issue_priority()` gains new `HEALTH → 1.2` arm (between WIRING=1.0 and COMPLEXITY=1.5)
  - Correct API: `conn_ref()` — `FileKnowledgeDB` does not expose `conn_opt()`
  (ref: `crates/touring-hooks/src/post_edit.rs`, `phase2_verification()` + `issue_priority()`)

- **S-4 post_write HEALTH signal** — Inside `collect_quality_issues()`, after V4 wiring check,
  runs `AnalysisPipelineBuilder::new(conn_ref()).config(hook_path()).with_files(files).build().run(rel_path)`
  and emits `"HEALTH {one_liner}"` when `!summary.passes` (passes = composite_score ≥ 0.8)
  - Budget gate: `all_issues.len() < 6`; uses `content` already in scope — no redundant disk read
  (ref: `crates/touring-hooks/src/post_write.rs`, `collect_quality_issues()`)

- **5 new regression tests** in `crates/touring-hooks/src/post_edit.rs` and `pre_write.rs`:
  - `test_issue_priority_health_above_wiring` — HEALTH (1.2) > WIRING (1.0)
  - `test_issue_priority_health_below_complexity` — COMPLEXITY (1.5) > HEALTH (1.2)
  - `test_quality_depth_test_file_skipped` — test files return `Vec::new()`
  - `test_quality_depth_high_cc_triggers_signal` — CC > 15 emits signal with score ≥ 1.2
  - `test_quality_depth_unwrap_density_triggers_signal` — 5+ unwraps emit signal with score ≥ 1.4

### Changed

- **`issue_priority()` in `post_edit.rs`** — Added `HEALTH → 1.2` arm, placing dimensional health
  feedback above wiring notices (1.0) and below complexity warnings (1.5); full priority table:
  SYNTAX/SYMBOL/STRUCTURAL/IMPORT/CFG=2.5 > ANTIPATTERN/API=2.0 > COMPLEXITY=1.5 > HEALTH=1.2 > WIRING=1.0 > feature-gated=0.8 > default=0.5

### Quality

- `cargo check -p touring-hooks -p touring-analysis -p touring-server --all-features`: 0 errors
- `cargo clippy -p touring-hooks --all-features -- -D warnings`: 0 warnings
- `cargo test -p touring-hooks --all-features`: 1354 passed, 0 failed
- `cargo test -p touring-analysis --all-features`: 229 passed, 0 failed (unchanged — analysis APIs consumed, not modified)
- Hook chain coverage: all 4 hook sites (pre_edit, pre_write, post_edit, post_write) now emit touring-analysis quality signals
- Integration points: `analyze_complexity`, `analyze_unwraps`, `AnalysisPipeline`, `AnalysisPipelineBuilder`, `MetricsDashboard::alerts_below`, `to_analysis_summary`, `one_liner`, `passes`, `AnalysisConfig::hook_path`

---

## [0.3.3] — 2026-04-06

### Added

- `compute_blast_reward(edge_count: usize) -> f64` — LinUCB blast radius RL reward signal,
  normalized to `[0.1, 1.0]` via saturation constant `10_000`; maps edge_count to a smooth
  reward curve suitable for LinUCB arm feedback (ref: `src/lib.rs`)
- `AnalysisSummary` struct with `one_liner() -> String` and `passes: bool` gate
  (`score >= 0.8`); format: `"STATUS score [dims] ms — issue"` — compact single-line
  representation of a full analysis run (ref: `src/report.rs`)
- `CodeHealthReport::to_analysis_summary()` — lightweight conversion from full health report
  to `AnalysisSummary` for use in pre_edit signal injection and RL feedback loops
- `MetricsDashboard::to_json_line() -> String` — JSON-serialized single-line metrics snapshot
  for structured logging and alerting pipelines
- `MetricsDashboard::alerts_below(threshold: f64) -> Vec<String>` — returns dimension names
  scoring below threshold; enables alerting ergonomics without full report parsing
- `impl Display for HealthDiff` — delegates to `DimensionDiff::fmt`, enabling direct
  `format!("{}", diff)` and `tracing` log output without manual field extraction
- `AnalysisConfig::standard_with_learning()` — preset combining standard depth with RL learning
  enabled; convenience constructor for hook integration sites
- `AnalysisConfig::with_budget(budget: usize)` — builder method to set signal budget ceiling
- `AnalysisConfig::with_temporal(enabled: bool)` — builder method to toggle temporal analysis
- `AnalysisConfig::with_learning(enabled: bool)` — builder method to toggle RL integration
- `AnalysisPipelineBuilder::depth(depth: Depth)` — builder method to set analysis depth preset
  (Quick/Standard/Deep), replacing magic config construction at call sites
- `AnalysisPipelineBuilder::with_symbol_index(index: Arc<SymbolIndex>)` — wires a real
  `SymbolIndex` into the pipeline so `run_blast()` uses BFS over actual codebase graph instead
  of an empty stub (critical for `touring-hooks/src/cli_e2e.rs` integration)
- `CachedAnalysisPipeline::run_cached_with_config(config: AnalysisConfig)` — cached analysis
  variant accepting an explicit config, enabling callers to override defaults without
  reconstructing the full pipeline
- 10 new E2E integration tests in `tests/e2e_integration.rs` covering all v0.3.3 public APIs:
  `compute_blast_reward` normalization bounds, `AnalysisSummary` one_liner format,
  `to_analysis_summary()` conversion, `to_json_line()` serialization, `alerts_below()`,
  `Display` for `HealthDiff`, all 4 `AnalysisConfig` builder methods,
  `AnalysisPipelineBuilder::depth()` + `with_symbol_index()`, and
  `run_cached_with_config()` roundtrip
- `E2eConfig { project_root, depth }` + `run_e2e(config, knowledge_conn, graph_conn)`
  in `src/e2e/mod.rs` — clean orchestrated entry point that builds depth-appropriate
  pipeline without manual builder construction (re-exported from crate root)
- 2 additional E2E tests: `test_e2e_run_e2e_convenience_fn` and
  `test_e2e_run_quality_included_in_run`

### Fixed

- `run_blast()` was constructing `BlastRadiusEngine::new(vec![])` unconditionally, which
  always selected the `"none"` fallback strategy and produced empty blast radius results;
  fixed to use `pipeline.symbol_index` when `Some`, falling back to a fresh `BfsStrategy`
  with an empty `SymbolIndex` only when no index is provided (ref: `src/pipeline.rs`)
- `run_knowledge()` now correctly exposes `language_distribution`, `import_graph_health`, and
  `active_gotchas` in both metrics output and issues list; previously these fields were
  computed but never surfaced to callers
- `AnalysisPipeline::run()` was missing the quality dimension despite the `quality` feature
  being active; fixed by adding the same quality branch as `run_parallel()`, guarded by
  `!self.files.is_empty() && self.config.quality_sample > 0`

### Quality

- 229 tests passing (163 unit + 48 integration + 18 doc) in `touring-analysis` crate
- `cargo clippy -- -D warnings`: 0 warnings
- All v0.3.3 public symbols covered by at least one E2E test
- `touring-server` cross-compilation: 0 errors after API additions

---

## [0.3.2] — 2026-04-06

### Added
- **Signal 17 (Temporal Velocity)**: New pre_edit signal (score 0.9) powered by `analyze_trends()` — reports edit velocity (edits/day), reliability (bash success %), quality drift detection, and churn rate
- **Export API**: 6 orphan pub symbols wired to crate root: `TestProxy`, `estimate_cognitive_complexity`, `OrphanResult`, `count_orphans`, `ChainResult`, `analyze_chains`
- **E2E tests**: 3 new integration tests for `analyze_chains`, `count_orphans`, `estimate_cognitive_complexity`

### Changed
- **Signal 15 (Health Score)**: Extended to show worst-performing dimension name and score when `composite_score < 0.8` (e.g. `"— weak: quality:0.42"`)

### Quality
- 5167 tests passing (+10 vs v0.3.1)
- `cargo clippy -- -D warnings`: 0 warnings
- Touring index: 470 symbols, 189 wiring entries

---

## [0.3.1] — 2026-04-06

### Added

- `TrendReport.quality_drift` changed from `f64` to `Option<f64>`: `Some(KS statistic)` when
  `simd-temporal` feature is enabled, `None` otherwise — eliminates the ambiguous `0.0` sentinel
  value that could mask genuine zero-drift results (ref: `temporal/trends.rs`)
- E2E test `test_e2e_quality_drift_option_type` in `tests/e2e_integration.rs` — proves
  `Option<f64>` behavior under both `#[cfg(feature = "simd-temporal")]` and
  `#[cfg(not(feature = "simd-temporal"))]` configurations, using helper
  `setup_knowledge_db_with_drift()` with 4:1 week-over-week asymmetry
- `update_linucb_health_signal()` in `touring-hooks/src/cli_e2e.rs` — closes the RL reward loop
  for `ArmKind::FullEnrichment` (arm 7) using avg Wilson composite health score; cold-start
  guard skips injection when `total_pulls() == 0` to avoid poisoning the bandit on first run
- pre_edit Signal 15 (score 1.3): `CodeHealthReport` injected via
  `AnalysisPipeline::hook_path()` before each edit; budget guard `signals.len() < 10`
  (ref: `touring-hooks/src/pre_edit.rs`)
- pre_edit Signal 16 (score 1.1): `KnowledgeReport` injected via `analyze_knowledge()` before
  each edit; budget guard `signals.len() < 12` (ref: `touring-hooks/src/pre_edit.rs`)
- Knowledge health signal in `pre_read::build_db_context` — appends
  `"knowledge: {n}/{n} files indexed (score {s:.2})"` after cognitive enrichment when
  `remaining_budget > 80` (ref: `touring-hooks/src/pre_read.rs`)

### Changed

- `phase_wiring()` in `cli_e2e.rs` now calls `analyze_wiring()` (full `WiringReport`) instead
  of `count_orphans` + manual ad-hoc SQL — exposes additional metrics in E2E output:
  `avg_integration_score`, `modules_below_threshold`, `wiring_score`
- `E2eReport` gains `cache_stats` field with cache advisory information
- `pipeline.rs` call sites emit `quality_drift_available: bool` alongside `quality_drift`
  value at all consumers of `TrendReport`, using `.unwrap_or_default()` for backward compat

### Fixed

- `hook_decompose_bridge.rs` E0282: added missing type annotation on `task_id` binding
- `hook_decompose_bridge.rs` E0599: replaced non-existent `.flush()` call with
  `PRAGMA wal_checkpoint(TRUNCATE)` for correct WAL flush semantics
- `post_edit::run` and `post_edit::run_returning` corrected to accept `&mut HookRuntime`
  (pre-existing signature mismatch between declaration and call sites)
- `touring-cortex` and `touring-server` call sites updated for `&mut HookRuntime` in
  `post_edit` and `pre_edit` dispatch paths
- Unused variable warnings in `hook_decompose_bridge.rs` suppressed via `_` prefix on
  bindings that are intentionally consumed for side effects only

---

## [0.3.0] — 2026-04-05

### Added

- `BfsStrategy` made `pub` — blast-radius BFS traversal now accessible to consumers
- `HnswStrategy` in `hnsw.rs` (NEW file) — ANN-accelerated blast radius via
  `ann-blast` feature flag
- `bfs_only()` and `hnsw_only()` factory functions for strategy selection
- `analysis_bridge` module in `touring-cognitive` (NEW crate) — bridges analysis pipeline
  into cognitive runtime
- RL loop closure: `update_linucb_blast_signal()` in `cli_e2e.rs` feeds blast-radius results
  back into LinUCB bandit

### Changed

- Symbol index schema renamed to SCHEMA_VERSION=8
- `compute_with_start` API added for temporal analysis entry point
- `validate_graph_tables` added for graph DB integrity checks
- Default feature set expanded: `["blast-radius", "quality", "wiring", "temporal", "simd-temporal"]`

---

## [0.1.0] — initial

- Unified deep code analysis engine: blast radius, wiring, quality, knowledge, learning, health
- `AnalysisPipeline` with `hook_path()` entry point
- `WiringReport`, `QualityReport`, `KnowledgeReport`, `TrendReport` output types
- SQLite-backed symbol store integration
- Rayon parallel analysis for multi-file workloads
- 98 unit tests + 16 E2E integration tests
