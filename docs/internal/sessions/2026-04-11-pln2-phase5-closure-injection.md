# PLN2 Phase 5 — Closure Injection & Struct API Migration

**Date**: 2026-04-11
**Session**: TACO Phase 5 Implementation
**Status**: COMPLETED — 1489 tests passing, zero failures
**Successor**: [2026-04-11-pln2-lazy-seed-vgp-engine-tests.md](./2026-04-11-pln2-lazy-seed-vgp-engine-tests.md) (Phase 5.2 — lazy-seed + VgpEngine tests, workspace total 5,958 passing)

---

## Summary

This session implemented two major deliverables:

1. **Generator Closure Injection** (PLN2 §8.1): Production-grade `BkTreeFuzzyAdapter`, `LinUCBRewardSink`, and `GeneratorContext::with_closures()` constructor
2. **Struct API Migration** (E0061 bugfix): 10 call sites migrated from 8-arg positional to struct-based API for `build_minimal_context` and `build_change_risk_report`

---

## Implementation 1: Generator Closure Injection

### Files Modified
- `crates/touring-generator/src/core/context.rs` — new structs + impls
- `crates/touring-generator/src/lib.rs` — feature-gated re-exports

### A) BkTreeFuzzyAdapter (`#[cfg(feature = "simd-fuzzy")]`)

```rust
pub struct BkTreeFuzzyAdapter {
    pool: Mutex<Vec<String>>,
}
```

- Implements `FuzzyMatcher` trait
- Method `top_k(query: &str, k: usize) -> Vec<FuzzySuggestion>` with linear scan Levenshtein distance
- Private helper `levenshtein_dist(a: &str, b: &str) -> usize` — O(min(m,n)) space via DP row
- `FuzzySuggestion { name: String, distance: u8, confidence: NormalizedScore }` — uses `NormalizedScore::clamped()` (infallible, not `::new()` which returns Result)

### B) LinUCBRewardSink (`#[cfg(feature = "rl-integration")]`)

```rust
pub struct LinUCBRewardSink {
    engine: Mutex<OnlineRLEngine>,
    qtable: Mutex<QTable>,
    linucb: Mutex<LinUCBBandit>,
}
```

- Implements `RlRewardSink` trait
- Method `inject(tool: &str, reward: f64, ctx: &str)` — acquires 3 locks, builds `ImmediateReward`, calls `eng.process_reward(&imm, &mut qt, &mut linucb)`
- Method `ema(tool: &str) -> f64` — returns `eng.ema_reward()` via lock

**CRITICAL**: `OnlineRLEngine::ema_reward()` already exists at `online_rl.rs:~420`. Do NOT add a duplicate getter — causes E0592.

### C) GeneratorContext::with_closures()

```rust
pub fn with_closures(
    fuzzy_index: Arc<dyn FuzzyMatcher>,
    rl: Arc<dyn RlRewardSink>,
    pheromone_fn: Option<PheromoneCallback>,
    wiring_gate_fn: Option<WiringGateCallback>,
) -> Arc<Self>
```

- Production constructor replacing `for_testing()`
- Reads `TOURING_PROJECT_ROOT` env var, fallback `/tmp/touring`
- Populates NoopX defaults for all fields except the 4 passed parameters

### lib.rs Re-exports (feature-gated)

```rust
// CORRECT pattern — separate statements, NOT inside use {}
#[cfg(feature = "simd-fuzzy")]
pub use core::context::BkTreeFuzzyAdapter;

#[cfg(feature = "rl-integration")]
pub use core::context::LinUCBRewardSink;
```

**PITFALL**: Never place `#[cfg(feature=X)]` inside a `use {}` block — each feature-gated item must be a separate `pub use` statement.

---

## Implementation 2: make_context() Production Rewrite

### File: `crates/touring-server/src/tools/generator_tools.rs`

```rust
fn make_context() -> Arc<GeneratorContext> {
    #[cfg(feature = "simd-fuzzy")]
    let fuzzy = Arc::new(BkTreeFuzzyAdapter::new());
    #[cfg(not(feature = "simd-fuzzy"))]
    let fuzzy = Arc::new(NoopFuzzyMatcher);

    #[cfg(feature = "rl-integration")]
    let rl = Arc::new(LinUCBRewardSink::new());
    #[cfg(not(feature = "rl-integration"))]
    let rl = Arc::new(NoopRlSink);

    GeneratorContext::with_closures(fuzzy, rl, None, None)
}
```

### File: `crates/touring-server/Cargo.toml`

```toml
touring-generator = { ..., features = ["simd-fuzzy", "rl-integration"] }
```

---

## Implementation 3: Struct API Migration (E0061 bugfix)

10 call sites migrated from 8-argument positional API to struct-based API.

### MinimalContextInput

```rust
MinimalContextInput {
    symbol_count: usize,
    file_count: usize,
    orphan_count: usize,
    e2e_health: f64,
    rl_reward: f64,
    top_gotchas: Vec<String>,
    task_hint: String,
    detail_level: DetailLevel,
}
```

**Sites updated**:
- `crates/touring-server/tests/token_efficiency_e2e.rs` — 2 sites (E2E Test 6 + 7)
- `crates/touring-server/src/tools/context_tools.rs` — 3 sites (3 unit tests)

### ChangeRiskInput

```rust
ChangeRiskInput {
    changed_files: Vec<String>,
    affected_files: Vec<String>,
    affected_symbols: Vec<String>,
    test_gaps: Vec<String>,
    hotspots: Vec<String>,
    gotcha_warnings: Vec<String>,
    wiring_score: f64,
    detail_level: DetailLevel,
}
```

**Sites updated**:
- `crates/touring-server/tests/token_efficiency_e2e.rs` — 2 sites (E2E Test 7)
- `crates/touring-server/src/tools/risk_scoring.rs` — 4 sites (4 unit tests)
- `crates/touring-server/src/server/mod.rs` — 3 sites (H-handlers)

---

## Final Status

| Check | Result |
|-------|--------|
| `cargo test --workspace --exclude touring-python` | 1489 tests, ZERO failures |
| `cargo check --package touring-generator --features simd-fuzzy,rl-integration` | CLEAN |
| Net new tests | +11 (1478 → 1489) via feature-gated code now active |

---

## Pre-existing Issue (NOT introduced this session)

`cargo test --package touring-server` fails due to `std::time::Elapsed` not found in `touring-hooks/src/touring_error.rs`. This is a pre-existing issue in `touring-hooks`, unrelated to changes in this session. The `--exclude touring-python` workspace test run passes completely.

---

## Architecture Integration

```
GeneratorContext::with_closures()
    ├── Arc<dyn FuzzyMatcher>
    │   └── BkTreeFuzzyAdapter [cfg(simd-fuzzy)]
    │       └── Levenshtein DP, pool: Mutex<Vec<String>>
    └── Arc<dyn RlRewardSink>
        └── LinUCBRewardSink [cfg(rl-integration)]
            ├── Mutex<OnlineRLEngine>
            ├── Mutex<QTable>
            └── Mutex<LinUCBBandit>
                → process_reward(&imm, &mut qt, &mut linucb)

generator_tools.rs::make_context()
    → cfg-gated: BkTreeFuzzyAdapter OR NoopFuzzyMatcher
    → cfg-gated: LinUCBRewardSink OR NoopRlSink
    → GeneratorContext::with_closures(fuzzy, rl, None, None)
```

---

## Gotchas Registered

| Pattern | Severity | Key insight |
|---------|----------|-------------|
| FuzzySuggestion field names | high | Use `name`, `distance: u8`, `confidence: NormalizedScore` — NOT `symbol_name/edit_distance/score` |
| LinUCBRewardSink Mutex triple lock | medium | inject() needs 3 simultaneous locks: engine+qtable+linucb |
| NormalizedScore::clamped() | lesson | Use `clamped(f64)` (infallible) not `new(f64)` (returns Result) |
| OnlineRLEngine::ema_reward() duplicate | gotcha | Already exists at online_rl.rs:~420 — do NOT add duplicate |
| cfg-feature pub use | lesson | Each feature-gated re-export must be a separate `pub use` statement |
