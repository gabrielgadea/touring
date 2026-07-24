# Wave 5 — Ecosystem Integration Plan

**Date**: 2026-04-18
**Author**: TACO (Claude Code Opus 4.7)
**Approver**: Gabriel Gadea
**CILA Level**: L4 (multi-crate, multi-capability)
**Baseline**: Wave 4 (v30.3.0, 957 tests passing in touring-ast+touring-analysis)

---

## <objective>

Integrate **9 ecosystem-proven crates** (TIER S + pyo3) into the Touring
workspace to expand testing rigor (fuzzing, CLI E2E, serialization),
observability (heap profiling, version metadata), security (CVE scanning),
API stability (public-api diffing), ergonomics (strum iteration,
derive_more boilerplate reduction, cfg_aliases), and Python interop (pyo3
expansion). Target: 100% of Wave 4 new APIs exposed to Python via
`claude_learning_kernel` plus ~110 new tests and 5 new CLI subcommands.

**Why**: workspace has 12+ feature flags, 138 manually-registered hooks,
61 CLI commands without E2E tests, no heap profiling, no CVE scanning,
enum boilerplate, and no public-API breaking-change detection. Each gap
maps to one crate in the plan.

</objective>

## <deliverables>

### TIER S — Ecosystem-Proven (Recent DL/yr ≥ 10M)

| # | Crate | Size | Target | Deliverable |
|---|---|---|---|---|
| 1 | `strum` + `strum_macros` | S | `touring-ast::Lang`, `touring-core` enums | `Lang::iter()`, `Lang::from_str()`, `Display` derive |
| 2 | `derive_more` | S | `touring-ast::TracedAstError` | Remove manual `From`/`Display` boilerplate |
| 3 | `cfg_aliases` | S | workspace root `build.rs` | `#[cfg(touring_heavy)]` aliases |
| 4 | `vergen` | S | `touring-server/build.rs` | `touring --version` enriched (git SHA, rustc, build time) |
| 5 | `serial_test` | S | touring-ast, touring-hooks tests | `#[serial(sqlite)]` for shared-state tests |
| 6 | `arbitrary` + `bolero` | M | `touring-ast/tests/fuzz_parsers.rs` | Structure-aware fuzz harness for 14 langs |
| 7 | `assert_cmd` + `predicates` | M | `touring-server/tests/cli_e2e.rs` | E2E tests for CLI commands |
| 8 | `rustsec` | M | `touring-analysis/src/security.rs` (NEW) | `scan_advisories()` API + `touring analysis security` CLI |
| 9 | `public-api` | M | `touring-ast/src/rust_semantic.rs` (EXTEND) | `RustSemanticReport::public_api_surface()` |
| 10 | `tikv-jemallocator` + `jemalloc_pprof` | L | `touring-server/src/bin/touring-daemon.rs` | Feature-gated `jemalloc` + `heap-profile` CLI cmds |
| 11 | `inventory` | L | `touring-hooks/src/hook_registry.rs` | **Feature-gated** — auto-collect 138 hooks (opt-in) |

### TIER A — Python Interop (Requested)

| # | Crate | Size | Target | Deliverable |
|---|---|---|---|---|
| 12 | `pyo3` (expansion) | L | `touring-python/src/lib.rs` | Expose Wave 4 APIs: `RustSemanticReport`, `WorkspaceInfo`, `format_rust_code`, `TracedAstError` |

**Total**: 12 atomic deliverables, 9 crates (+ 3 companion dev-deps).

</deliverables>

## <timeline>

Sequenced respecting dependencies. Each phase atomic & independently
revertible (touring memory checkpoint before L items).

```
PHASE A — Zero-risk ergonomics (parallel, independent):
  [1] strum on Lang             (S, 20min)
  [2] derive_more on errors     (S, 15min)
  [3] cfg_aliases workspace     (S, 25min)
  [4] vergen build metadata     (S, 20min)

PHASE B — Test infrastructure (parallel after A):
  [5] serial_test annotations   (S, 30min)
  [6] arbitrary + bolero fuzz   (M, 60min) — depends on [1]
  [7] assert_cmd CLI E2E        (M, 60min)

PHASE C — New library capabilities (parallel after A):
  [8] rustsec security module   (M, 60min)
  [9] public-api surface        (M, 45min)

PHASE D — Production observability (after B+C):
  [10] jemalloc heap profile    (L, 90min) — feature-gated
  [11] inventory hook registry  (L, 120min) — feature-gated, big refactor

PHASE E — Python interop:
  [12] pyo3 Wave 4 bindings     (L, 90min) — depends on [1],[2],[8],[9]
```

**Critical path**: A (4 parallel S tasks) → B+C parallel → D+E.
**Total wall-clock estimate** (single orchestrator): ~10h.
**Parallelizable via subagents**: ~4h wall-clock.

</timeline>

## <risks>

| # | Risk | Prob | Impact | Mitigation |
|---|---|---|---|---|
| R1 | `inventory` refactor breaks hook dispatch | MEDIUM | HIGH | Feature-gate `inventory`; keep manual table as default; side-by-side compat layer |
| R2 | `jemalloc` global allocator swap segfaults on musl | LOW | HIGH | Feature-gate `jemalloc`; default off; test on glibc only initially |
| R3 | `cfg_aliases` changes feature resolution silently | LOW | MEDIUM | Cargo check on all 12 feature combos after change |
| R4 | `arbitrary`/`bolero` exposes tree-sitter panics | HIGH | LOW | Expected — that's the goal. Catch + report as gotchas |
| R5 | `rustsec` offline/DB fetch failures | MEDIUM | LOW | Graceful degradation; `advisories: []` if DB unreachable |
| R6 | `public-api` requires nightly rustdoc JSON | MEDIUM | MEDIUM | Document nightly dep; fall back to syn parse if no nightly |
| R7 | `pyo3` GIL contention with async daemon | MEDIUM | MEDIUM | Use `py.allow_threads()` for long operations; document in module |
| R8 | `vergen` requires git at build time for SHA | LOW | LOW | `VERGEN_GIT_SHA=unknown` fallback |
| R9 | `strum` derive conflicts with existing impl | LOW | MEDIUM | Remove manual `FromStr` impls; add migration note |
| R10 | `serial_test` serializes too aggressively, slows tests | LOW | LOW | Use named groups `#[serial(sqlite)]`, not global |

**Self-validation gates:**
- [ ] Each deliverable is atomic (builds standalone)
- [ ] Dependencies explicit & acyclic (A→B,A→C,B∪C→D→E)
- [ ] Estimates realistic (S=15-30min, M=45-60min, L=90-120min)
- [ ] All HIGH-impact risks have MEDIUM+ mitigations

</risks>

## Success Criteria

1. `cargo check --workspace` passes after each phase
2. `cargo test --workspace` passes after each phase (no regressions)
3. `touring doctor -j` stays GREEN
4. `touring --version` shows git SHA + rustc + build time
5. New CLI commands respond: `touring analysis security`, `touring profile heap-dump`, `touring profile flamegraph`
6. `Lang::iter().count() == 14`
7. Python binding: `from claude_learning_kernel import RustSemanticReport; RustSemanticReport.from_source("fn foo() {}")`
8. `cargo bolero test fuzz_rust_parser --time=60s` runs without crash
9. `touring wiring orphans -j` shows no new orphans
10. Memory persists: `touring memory store wave5 "..."  --tier semantic`

## Non-goals (out of scope)

- `ra_ap_syntax` (TIER A #6) — deferred to Wave 6 (alpha API, lower ROI)
- `cxx`, `cbindgen`, `autocxx` — no C/C++ FFI needs
- `puffin`, `tracy-client` — CPU profiling already covered by pprof
- Workspace-wide `inventory` adoption — only touring-hooks hook registry

---

## Session Execution Report (2026-04-19)

**Executed by**: TACO (touring-scribe Phase 7)
**Outcome**: PARTIAL — 12/12 original deliverables pre-implemented; 2 net-new gaps implemented

### Pre-Implementation State Discovery

Scout (VP-Scout chains 1-6) and Architect independently verified:
**All 12 original Wave 5 deliverables were already implemented** when the session began.

| Deliverable | Status | Evidence |
|---|---|---|
| strum on Lang | PRE-IMPLEMENTED | `symbols.rs:32` has `#[derive(EnumIter)]` |
| derive_more on TracedAstError | PRE-IMPLEMENTED | `Cargo.toml` + derive macros present |
| cfg_aliases workspace | PRE-IMPLEMENTED | workspace `build.rs` already configured |
| vergen build metadata | PRE-IMPLEMENTED | `touring-server/build.rs` has vergen calls |
| serial_test annotations | PRE-IMPLEMENTED (FALSE_POSITIVE) | Tests use isolated mock daemons — no shared state |
| arbitrary + bolero fuzz | PRE-IMPLEMENTED | `fuzz_parsers.rs` test harness present |
| assert_cmd CLI E2E | PRE-IMPLEMENTED | `cli_e2e.rs` present |
| rustsec security module | PRE-IMPLEMENTED | `security.rs` + `scan_advisories()` present |
| public-api surface | PRE-IMPLEMENTED | `public_api_surface()` in rust_semantic.rs |
| jemalloc heap profile | PRE-IMPLEMENTED (PARTIAL) | jemalloc wired in daemon; CLI subcommands MISSING |
| inventory hook registry | PRE-IMPLEMENTED | auto-collect present |
| pyo3 Wave 4 bindings | PRE-IMPLEMENTED | bindings present in touring-python |

### False Positives (FASE 4.5 + Engineer verification)

| Gap ID | Description | Verdict | Evidence |
|---|---|---|---|
| S-1 | strum::EnumIter missing from SymbolKind | FALSE_POSITIVE | `symbols.rs:32` already had `#[derive(EnumIter)]` |
| S-2+S-3 | serial_test needed for shared-state tests | FALSE_POSITIVE | Tests use mock daemons with isolated SQLite — no shared resource race |

### Net-New Implementations (beyond original plan)

#### 1. `touring profile heap-dump` / `touring profile flamegraph`

**File created**: `crates/touring-server/src/cli/profile.rs` (166 lines)
**Files modified**: `crates/touring-server/src/cli/mod.rs`, `crates/touring-server/src/cli/common.rs`

```
profile heap-dump [--output <path>]   — dump jemalloc heap to .pb.gz
profile flamegraph [--output <path>]  — generate flamegraph from heap dump
```

**Implementation notes**:
- `PROF_CTL.blocking_lock()` wrapped in `tokio::task::block_in_place()` (sync mutex in async)
- Feature gate: `heap-profile` (default ON)
- **Gotcha**: Requires `MALLOC_CONF=prof:true` set before process start
- Graceful error if PROF_CTL unavailable (panic caught by daemon catch_unwind)

**Usage in production**:
```bash
MALLOC_CONF=prof:true touring profile heap-dump --output /tmp/heap.pb.gz
MALLOC_CONF=prof:true touring profile flamegraph --output /tmp/heap.svg
```

#### 2. `diff_api_surfaces` + `ApiChange` in `rust_semantic.rs`

**File modified**: `crates/touring-ast/src/rust_semantic.rs`

**Added symbols**:
- `pub struct ApiChange { kind: ApiChangeKind, item: String }` (line 467)
- `pub enum ApiChangeKind { Added, Removed }` (line 476)
- `pub fn diff_api_surfaces(old: &[String], new: &[String]) -> Vec<ApiChange>` (line 504)

**Wiring**: `public-api` crate now has a real call site in test `public_api_crate_is_accessible`.
4 tests added covering added/removed/unchanged cases.

### Gotcha Registered

**ID**: lesson:jemalloc_pprof:prof_ctl_requires_malloc_conf
**Severity**: medium
**Description**: `jemalloc_pprof::PROF_CTL` Lazy init panics without `MALLOC_CONF=prof:true`.
Panic caught by daemon catch_unwind → exit 1 (not 101). Cosmetic, not functional.
**Fix**: Always export `MALLOC_CONF=prof:true` before invoking `touring profile heap-dump`.

### Memory Persisted

```
wave5:status                                    → semantic/insight
lesson:wave5:architect-false-positives          → semantic/lesson
lesson:jemalloc_pprof:prof_ctl_requires_malloc_conf → semantic/gotcha
pattern:doc:wave5:pre-implemented-plan-detection → semantic/pattern
```
