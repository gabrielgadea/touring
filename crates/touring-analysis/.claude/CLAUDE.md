# touring-analysis — Crate Instructions

## What this crate does

Unified Deep Code Analysis Engine — blast radius, wiring, quality, temporal, health scoring, knowledge, and learning dimensions via a composable pipeline.

## Default Features (all always-on since 2026-04-17)

All 10 features are in `default` and require no flags:

| Feature | Purpose |
|---------|---------|
| `blast-radius` | BFS exact blast radius via touring-ast SymbolIndex |
| `quality` | Antipattern, complexity, unwrap audit, error coverage |
| `wiring` | Orphan detection, functional chains, dead code |
| `temporal` | Edit velocity, churn rate, KS drift via DriftDetector |
| `simd-temporal` | Temporal analysis powered by touring-simd/learning-integration |
| `ann-blast` | HNSW approximate blast radius via touring-simd/ann |
| `deep` | Alias for blast-radius + quality + wiring + temporal |
| `simd-wiring` | Aho-corasick multi-pattern scan for dead code indicators |
| `simd-temporal-ac` | Aho-corasick pattern detection for churn (tmp/, _bak, etc.) |
| `erickson-bridge` | NLP argument mining via touring-offensive EricksonExtractor |

## Key Invariants

- **SIMD zero-cost fallback**: `scan_dead_patterns` and `detect_churn_patterns` always exported; when their feature is off they return `Vec::new()` with no perf penalty
- **Wilson confidence intervals**: use `data_points` per dimension, not dimension count
- **`Send + Sync` on OrphanResult**: rayon-powered wiring pipeline requires this
- **NUL separator** in orphan queries: `char(0)` avoids `::` collisions in symbol names
- **`erickson-bridge` now always-on**: `touring-offensive` dep is no longer optional; `erickson-bridge` feature flag remains for conditional NLP metric computation
- **`fast_content_hash` is a pre-filter**: uses `stringzilla::hash` (AES-NI) in `quick_content_changed()` before blake3 — skips blake3 for 90%+ unchanged files; `fast_hash` is NOT cryptographic, blake3 remains the authoritative hash

## How to run tests

```bash
cargo test -p touring-analysis        # 302 tests (200 + 6 + 48 + 13 + 22 + 13 + 1 ignored)
cargo clippy -p touring-analysis -- -D warnings   # must be 0
```

## File layout

| Path | Purpose |
|------|---------|
| `src/lib.rs` | Public API re-exports, feature-gated module declarations |
| `src/engine.rs` | AnalysisConfig, Depth — shared configuration |
| `src/blast_radius/` | BFS + HNSW strategy dispatch |
| `src/quality/` | Antipattern, complexity, cognitive estimate |
| `src/quality/fast_hash.rs` | `fast_content_hash` — stringzilla AES-NI pre-filter for `quick_content_changed` |
| `src/wiring/` | Orphan detection, chain analysis, dead code |
| `src/health/` | HealthStatus, CodeHealthReport, Wilson CI |
| `src/temporal/` | Trends, churn, KS drift |
| `src/pipeline.rs` | AnalysisPipeline + OtelConfig + insights |
| `src/knowledge/` | File stats, language distribution |
| `src/learning/` | RL reward computation from health report |
| `src/e2e/` | Schema guard + E2E runner |
| `tests/` | Unit + integration tests |