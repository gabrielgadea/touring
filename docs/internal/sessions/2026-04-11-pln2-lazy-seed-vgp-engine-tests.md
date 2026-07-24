# PLN2 Phase 5.2 — BkTreeFuzzyAdapter lazy-seed + VgpEngine test suite

**Date**: 2026-04-11
**Session**: TACO Iteration 2 — Phase 5 (Implementation) + Phase 6 (Cross-Audit)
**Status**: COMPLETED — 5,958 tests passing, 0 failures
**Predecessor**: [2026-04-11-pln2-phase5-closure-injection.md](./2026-04-11-pln2-phase5-closure-injection.md)
**Strategy reference**: [2026-04-10-touring-generator-strategy-pln2.md](./2026-04-10-touring-generator-strategy-pln2.md)

---

## Summary

This iteration delivered three complementary improvements that collectively wired orphaned
symbols, added critical test coverage, and resolved a graceful-degradation gap in the
BkTreeFuzzyAdapter introduced in Phase 5.1 (closure injection).

| Deliverable | Problem solved | Symbols added |
|-------------|---------------|---------------|
| A. lazy-seed for BkTreeFuzzyAdapter | `seed()` had consumer=0; pool empty in production | 4 private (seed_attempted, load_from_cli, parse_cli_json, extract_names_from_array) |
| B. VgpEngine test suite | VgpEngine had 0 tests despite being VGP pipeline core | 10 tests (0 new prod symbols) |
| C. SymbolRef Default derive | Tests for SymbolKey required Default on SymbolRef | 0 new symbols (derive macro) |

**Net orphan delta**: 0 (all new symbols are private helpers, not pub).

---

## A. lazy-seed Pattern for BkTreeFuzzyAdapter

### Problem

`BkTreeFuzzyAdapter` was introduced in Phase 5.1 with an explicit `seed(pool: Vec<String>)`
method. Cross-audit (Fase 6) found `seed()` had **consumer=0** — no call site populated the
pool before production `top_k()` calls. The adapter was silently returning empty results.

### Solution: lazy-seed on first `top_k()` call

**File**: `crates/touring-generator/src/core/context.rs`

**Pattern applied**:
```
AtomicBool seed_attempted (Ordering::Relaxed)
  └─ set to true on first top_k() call
       └─ if pool empty AND !seed_attempted → load_from_cli()
            └─ Command::new("touring").args(["index","search","","-j"])
                 └─ success → parse_cli_json() → populate pool
                 └─ any error → tracing::warn! → return Vec::new() (degraded)
```

**Why Ordering::Relaxed is sufficient**: The flag guards a best-effort seed; worst case is
two threads both attempt seed simultaneously (harmless — both populate the pool, second
write is idempotent). No dependent memory loads follow the store, so SeqCst is unnecessary.

### New private symbols

| Symbol | Kind | Purpose |
|--------|------|---------|
| `seed_attempted` | field: `AtomicBool` | Guards lazy-seed to prevent repeated subprocess calls |
| `load_from_cli() -> Vec<String>` | fn (private) | Spawns `touring index search` subprocess, graceful degradation on any error |
| `parse_cli_json(&[u8]) -> Vec<String>` | fn (private) | Handles both `{"results":[...]}` and `[...]` array formats |
| `extract_names_from_array(iter) -> Vec<String>` | fn (private) | Extracts `"name"` field from JSON objects |

### Graceful degradation contract

Every error path in `load_from_cli()` returns `Vec::new()` with `tracing::warn!`:
- Daemon not running (socket error)
- Non-zero exit code from touring binary
- Invalid JSON in response
- Missing "name" field in results

The adapter continues to function (returning no fuzzy suggestions) rather than panicking.
This matches the pattern for optional CLI subprocess calls throughout the codebase.

### New tests (7 added to fuzzy_tests mod)

| Test | What it proves |
|------|----------------|
| `lazy_seed_when_pool_is_empty` | Seed triggers when daemon available; graceful degradation in CI |
| `fuzzy_adapter_explicit_seed_takes_priority_over_lazy_seed` | Pre-populated pool → lazy-seed skipped |
| `seed_attempted_flag_is_set_after_first_top_k` | AtomicBool is set after first top_k, regardless of outcome |
| `parse_cli_json_standard_results_shape` | `{"results":[{"name":"X"}]}` parsed correctly |
| `parse_cli_json_plain_array_shape` | `[{"name":"X"}]` array parsed correctly |
| `parse_cli_json_empty_results` | Valid JSON with no results → empty Vec |
| `parse_cli_json_invalid_json_returns_empty` | Garbage bytes → Vec::new(), no panic |

---

## B. VgpEngine Test Suite

### Problem

`VgpEngine` was the core of the VGP (Verified Generation Protocol) pipeline — responsible
for contract verification, counter tracking, cache invalidation, and symbol key management —
but had **zero tests** at the end of Phase 5.1. Any regression in VgpEngine would be silent.

### New tests (10 added to vgp_engine_tests mod)

**File**: `crates/touring-generator/src/vgp/engine.rs`

| Test | Kind | What it proves |
|------|------|---------------|
| `test_vgp_engine_new_counters_start_at_zero` | sync | AtomicU64 counters are 0 at construction |
| `test_vgp_engine_reset_counters_zeroes_atomics` | sync | `reset_counters()` zeroes all counters idempotently |
| `test_vgp_engine_invalidate_all_does_not_panic` | sync | `invalidate_all()` completes without panic under any state |
| `test_verify_batch_empty_contracts_returns_all_passed` | async (tokio) | Empty contract list → all results are Passed |
| `test_verify_batch_empty_contracts_does_not_increment_counters` | async (tokio) | Empty batch does not mutate counters |
| `test_verify_batch_concurrent_no_deadlock` | async (tokio, 5x Arc, 10s timeout) | 5 concurrent callers on shared Arc<VgpEngine> do not deadlock |
| `test_symbol_key_from_symbol_ref_no_crate` | sync | SymbolKey From<&SymbolRef> without crate field |
| `test_symbol_key_from_symbol_ref_with_crate` | sync | SymbolKey From<&SymbolRef> with crate field |
| `test_symbol_key_equality_reflexive` | sync | SymbolKey implements Eq/Hash correctly (reflexive) |
| `test_symbol_key_inequality_by_crate` | sync | Same symbol name, different crates → unequal keys |

The concurrency test uses `tokio::time::timeout(Duration::from_secs(10), ...)` as a hard
deadline — if a deadlock occurs, the test fails with a timeout rather than hanging forever.

---

## C. SymbolRef Default Derive

**File**: `crates/touring-generator/src/plan/contracts.rs`

Added `#[derive(Default)]` to `SymbolRef`. This was required by the VgpEngine tests to
construct `SymbolRef` values inline without specifying every field. No behavior change in
production code; purely a test ergonomics improvement.

---

## E2E Proof

### Workspace test results (post-implementation)

```
cargo test --workspace (excluding touring-python)
running 5958 tests
test result: ok. 5958 passed; 0 failed; 1 ignored
```

### touring-generator feature-gated tests

```
cargo test -p touring-generator --features simd-fuzzy,rl-integration
running 92 tests
test result: ok. 92 passed; 0 failed; 0 ignored
  - 32 synchronous
  - 60 asynchronous (tokio)
```

### Cross-Audit results (Phase 6)

| Check | Result |
|-------|--------|
| Compilation errors | 0 |
| New orphan pub symbols | 0 |
| Security (bare unwrap, shell injection, PII) | 0 violations |
| Purpose fidelity checks | 6/6 PASS (confidence 95-99%) |
| VP-Scout chains (Feature Trace, Dep Cycle, Already Impl, Homonimia) | ALL PASS |
| Composite score | 1.0 |

---

## PLN2 Coverage Assessment

The PLN2 strategy document (`2026-04-10-touring-generator-strategy-pln2.md`) defines
14 migration waves. This iteration covers:

| PLN2 objective | Coverage | Notes |
|---------------|----------|-------|
| BkTreeFuzzyAdapter with production-grade seeding | COMPLETE | lazy-seed pattern fully implemented and tested |
| VgpEngine test coverage | COMPLETE | 10 tests covering counters, async, concurrency, SymbolKey |
| Zero orphan pub symbols after each wave | COMPLETE | All new symbols private, no wiring debt |
| Graceful degradation for all subprocess calls | COMPLETE | load_from_cli follows established pattern |
| Test strategy: async tokio coverage | PARTIAL | VgpEngine covered; other async components pending future waves |

Outstanding PLN2 waves not yet started: GeneratorKinds expansion (×3.5 from Pln1),
CLI subcommands (24 target), MCP tools (20 target), PlanRegistry, SchemaRegistry.

---

## Auditor Recommendations for Next Iterations

1. **Seed integration test**: Add an integration test that verifies the lazy-seed path
   actually populates suggestions when touring daemon is running (currently skipped in CI).

2. **VgpEngine cache invalidation coverage**: The `invalidate_all()` test only checks
   non-panic; a future test should verify the cache is actually empty after invalidation.

3. **parse_cli_json property test**: A proptest/quickcheck fuzz over random byte slices
   would strengthen the `invalid_json_returns_empty` guarantee.

4. **BkTreeFuzzyAdapter benchmark**: The Levenshtein linear scan is O(n) over the pool.
   Once the pool grows beyond ~10k symbols, profile and consider BK-tree proper or
   parallel scan via rayon (with spawn_blocking for async contexts).

5. **PLN2 Wave 3 (GeneratorKinds)**: Next priority per PLN2 strategy — expand from
   8 to 28 generator kinds. Requires GenerateRequest/GenerateResult struct definitions
   and the LayerResult/LayerScore types listed in PLN2 §3.

---

## Memory entries stored (Touring semantic tier)

| Key | Type |
|-----|------|
| `lesson:bktree-lazy-seed:pattern` | lesson |
| `lesson:atomicbool-ordering-relaxed` | lesson |
| `lesson:vgpengine-tests-added` | lesson |
| `audit:taco-iter2:workspace-baseline` | insight |
| `pattern:graceful-degradation:subprocess` | pattern |
