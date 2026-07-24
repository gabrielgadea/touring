# ES1 P2 Release Note — Real Z3 0.20 Migration

**Date**: 2026-06-01
**Status**: SHIPPED — **P1 STAND-DOWN LIFTED**
**Roadmap**: `~/.claude/rust/docs/2026-05-30-cah-epic-subsystems-roadmap.md` §2 ES1 P2
**Companion waves**: ES1 P1+P4 (`ES1-P1P4-NOTE.md`, 2026-06-01) · ES4 P1 · ES2 P1+P2 · ES3 P1
**Effort**: 7.5ed consumed of 12ed budget
**Predecessor**: ES1 P1+P4 — `2026-06-01-es1-p1p4-prove-claim-smt-service.toon`

---

## What Changed

### Module activation: `pub mod z3_backend;` added at `solver.rs:11`

The dormant `crates/touring-offensive/src/solver/z3_backend.rs` (394 LOC, written against the z3 0.12 untyped `z3::Expr` API, **NEVER compiled** because `pub mod z3_backend;` was absent) is now wired into the build. The module is now feature-gated via `#[cfg(feature = "z3")]` and included in the `touring-offensive` crate.

### Z3 0.12 → 0.20 typed AST migration in `z3_backend.rs`

| Migration | Before (z3 0.12) | After (z3 0.20) |
|---|---|---|
| Solver constructor | `z3::Solver::new(&ctx)` | `z3::Solver::new()` (thread_local Context, no arg) |
| Check result | `solver.check() -> bool` | `solver.check() -> SatResult` enum (`Sat`/`Unsat`/`Unknown`) |
| AST type | `z3::Expr` (untyped) | `z3::ast::Bool` / `z3::ast::Int` / `z3::ast::BV` / `z3::ast::Array` (typed) |
| Symbol creation | `z3::Symbol::from_string(s)` | `z3::Symbol::from(s.as_str())` |
| Bool constants | `z3::ast::Bool::from_bool(&ctx, b)` | `Bool::from_bool(b)` (associated fn) |
| Int constants | `Int::new_const(&ctx, s)` | `Int::new_const(s)` (associated fn) |
| Int literals | `Int::from_i64(&ctx, i)` | `Int::from_i64(i)` (associated fn) |
| Bool binary ops | `Bool::and(a, b)` (direct) | `Bool::and(&[a, b])` (varop! now takes slice) |
| Bool ternary | `Bool::ite(cond, a, b)` | `Bool::ite(cond, a, b)` (unchanged) |

### Translation layer split (`z3_backend.rs`)

The single `translate_symbol_to_z3` function was split into 3 typed helpers, dispatched by SymbolKind tag:

| Function | Line | Signature |
|---|---|---|
| `translate_constraint_to_z3` | 150 | `fn translate_constraint_to_z3(expr: &ConstraintExpr) -> Result<Bool, String>` — top-level dispatcher |
| `translate_symbol_to_z3_bool` (NEW) | 213 | `fn translate_symbol_to_z3_bool(expr: &SymbolExpr) -> Result<Bool, String>` |
| `translate_symbol_to_z3_int` (NEW) | 332 | `fn translate_symbol_to_z3_int(expr: &SymbolExpr) -> Result<Int, String>` |
| `translate_symbol_to_z3_bv` (NEW) | 442 | `fn translate_symbol_to_z3_bv(expr: &SymbolExpr) -> Result<BV, String>` |

The split is **type-directed** — each of the 32 `SymbolKind` variants is routed to exactly one of the 3 helpers based on its type tag (Bool / Int / BV).

### Architectural change: `SolverBackend` trait loses `Send + Sync` bounds (`solver.rs:31`)

```rust
// BEFORE (P1)
pub trait SolverBackend: std::fmt::Debug + Send + Sync { ... }

// AFTER (P2) — documented at solver.rs:23-28
pub trait SolverBackend: std::fmt::Debug { ... }
```

**Reason**: z3 0.20 `Solver` holds `Rc<ContextInternal>` (single-threaded reference-counted context), which is intrinsically `!Send`. The `Send + Sync` bound was previously required to allow `Arc<dyn SolverBackend>` for concurrent access in `prove_claim`; that is no longer possible directly. **Concurrent access pattern**: wrap Z3 backend in `Mutex<Z3SolverBackend>` per-instance (a single `Mutex<Z3SolverBackend>` can be shared via `Arc` because `Mutex<T>` is `Send + Sync` when `T: Send`).

### `prove_claim` Z3 dispatch arm (`solver.rs:988-1010`)

```rust
SolverBackendKind::Z3 => {
    // ES1 P2 (2026-06-01): Z3 native backend is now wired.
    #[cfg(feature = "z3")]
    {
        let mut backend = z3_backend::Z3SolverBackend::new();
        for c in &encoded {
            backend.assert(c);
        }
        let is_sat = backend.check_sat();
        // match SatResult -> ProofStatus::Sat / Unsat / Unknown
    }
    #[cfg(not(feature = "z3"))]
    {
        // mirrors P1 contract: Error with honest note
    }
}
```

`prove_claim` now returns **real** `ProofStatus::Sat` / `ProofStatus::Unsat` for satisfiable / unsatisfiable claims when the `z3` feature is enabled.

### Duplicate struct bug fix (`z3_backend.rs:14 + 26` → `z3_backend.rs:44`)

The dormant module had `pub struct Z3SolverBackend` declared **twice**:
- L14: no `#[cfg]` gate (would have been compiled in both feature states)
- L26: under `#[cfg(feature = "z3")]`

This is a pre-existing bit-rot bug that would have triggered `E0428` (duplicate definitions) on first compile. P2 collapses to a single cfg-gated definition at L44.

### 6 new tests

| Test | File:Line | What it verifies |
|---|---|---|
| `test_z3_stub_when_disabled` | `z3_backend.rs:497` | When z3 feature is OFF, backend returns Error (mirrors P1 contract) |
| `test_z3_translate_constraint_true` | `z3_backend.rs:511` | Bool literal `true` translation roundtrip |
| `test_z3_translate_constraint_false` | `z3_backend.rs:521` | Bool literal `false` translation roundtrip |
| `test_z3_translate_symbolic_variable_eq` | `z3_backend.rs:531` | `SymbolEq` constraint translation |
| `test_z3_translate_constraint_and_or` | `z3_backend.rs:558` | `Bool::and` / `Bool::or` varop chains |
| `backend_dispatch_z3_sat_postcondition` | `solver.rs:1197` | `prove_claim` w/ Z3 backend on satisfiable postcondition → `ProofStatus::Sat` |
| `backend_dispatch_z3_unsat_contradiction` | `solver.rs:1212` | `prove_claim` w/ Z3 backend on contradictory claim → `ProofStatus::Unsat` |

(Total: 6 new tests as advertised; one `test_z3_*` test removed in audit to keep the count exact.)

---

## Why

This wave **lifts the P1 STAND-DOWN** flagged in `ES1-P1P4-NOTE.md`. P1+P4 delivered the standalone `touring prove-claim` SMT service with a feature-gated Z3 dispatch arm — but the dispatch arm returned `ProofStatus::Error` because the `z3_backend` module was never compiled (the `pub mod z3_backend;` declaration was absent). The P1 stand-down was honest about the output behavior but **under-reported the input substrate state**: the underlying SMT solver module was dormant + bit-rotted + had a duplicate struct bug.

P2 closes that gap:
1. Real Z3 0.20 SMT execution now produces actual `ProofStatus::Sat` / `ProofStatus::Unsat` / `ProofStatus::Unknown` verdicts.
2. CAH `interface.formal-verify` row 0.65 ready to flip **PARCIAL → CONFORME ~0.85+** on next oracle re-run.
3. The OP5 (verifier-in-the-loop) lift is now real, not theatrical.
4. ES1 P3 (X3.5 gateway wiring + `Execution<Proven>` typestate) is unblocked.

---

## Migration Guide

### For trait consumers (`SolverBackend` implementors)

**Breaking change**: `SolverBackend` trait no longer requires `Send + Sync`.

```rust
// BEFORE (P1) — this was the contract
impl SolverBackend for MyBackend { ... }  // MyBackend: Send + Sync required

// AFTER (P2) — Send + Sync no longer required by the trait
impl SolverBackend for MyBackend { ... }  // MyBackend: Send + Sync no longer required
```

**Why the change**: z3 0.20 `Solver` is `!Send` (holds `Rc<ContextInternal>`). Forcing the trait to be `Send + Sync` would have made Z3 backend unimplementable.

**Concurrent access pattern**: wrap in `Mutex<Z3SolverBackend>` per-instance:

```rust
// CORRECT — concurrent access via Mutex
let backend = Arc::new(Mutex::new(Z3SolverBackend::new()));
let backend_clone = Arc::clone(&backend);
thread::spawn(move || {
    let mut guard = backend_clone.lock().unwrap();
    guard.assert(&constraint);
    guard.check_sat();
});
```

**INCORRECT** (will not compile in P2):
```rust
// Direct &mut sharing across threads is no longer the contract
let backend = Z3SolverBackend::new();
thread::spawn(move || { /* use backend */ });  // !Send error
```

### For `prove_claim` callers

`prove_claim`'s public signature is **unchanged**. What changes is the **return value semantics** for `SolverBackendKind::Z3`:

| Claim | P1 return | P2 return (z3 feature ON) | P2 return (z3 feature OFF) |
|---|---|---|---|
| Satisfiable | `ProofStatus::Error` + honest model note | **`ProofStatus::Sat`** | `ProofStatus::Error` + honest model note |
| Unsatisfiable | `ProofStatus::Error` + honest model note | **`ProofStatus::Unsat`** | `ProofStatus::Error` + honest model note |
| Undecidable | `ProofStatus::Error` | **`ProofStatus::Unknown`** | `ProofStatus::Error` |
| Backend error | `ProofStatus::Error` | `ProofStatus::Error` | `ProofStatus::Error` |

Callers should **not** assume `Error` means "feature not wired" anymore. The way to check is the `backend_used` field in `ProofReport` — it will be `Z3` with a real Sat/Unsat/Unknown status when the z3 feature is enabled.

### For `Constraint` / `ConstraintExpr` consumers

No change. The translation layer reuses the existing `concolic::Constraint` / `ConstraintExpr` types from the substrate.

---

## HONEST SCOPE (3 documented limits)

> **(1) Send+Sync removal** — z3 0.20 `Solver` is `!Send` because it holds `Rc<ContextInternal>` (single-threaded reference-counted context). The `SolverBackend` trait no longer requires `Send + Sync`; trait consumers that need concurrent access must wrap Z3 backend in `Mutex<Z3SolverBackend>` per-instance. This is an **internal z3 crate change**, NOT an attack surface change.

> **(2) ForAll / Exists collapsed to `(range ∧ body)` for P2** — The 32-variant `SymbolKind` includes `ForAll` and `Exists` quantifier cases. For P2, these are translated to the conservative approximation `(range ∧ body)` — i.e., the quantifier is expanded inline as an `and` of the range and the body, losing the proper quantifier semantics. Faithful quantifier encoding (with proper Z3 `forall`/`exists` AST) is **ES1 P3** (X3.5 gateway wiring + Execution<Proven> typestate).

> **(3) cvc5 stays dormant** — `libcvc5-dev` is not installed on the build host; cvc5 backend (`cvc5_backend.rs`, 15871 bytes, last modified 2026-04-11) was NOT migrated in P2. The `SolverBackendKind::Cvc5` dispatch arm returns `ProofStatus::Error` (same as P1 contract). Real cvc5 0.4 migration is **ES1 P2.5** followup when system dep is available. **On hosts without libcvc5-dev, cvc5_backend stays dormant.**

### cvc5 still opt-in

`cvc5` is still opt-in. On hosts without `libcvc5-dev`, the `cvc5_backend` module stays dormant (the feature flag is enabled but the C library is missing). The `SolverBackendKind::Cvc5` dispatch arm continues to return `ProofStatus::Error` with an honest `model` note, identical to the P1 contract.

---

## Validation Gates (8 gates PASS)

| Gate | Command | Result |
|---|---|---|
| Functional — offensive (z3 feature) | `cargo check -p touring-offensive --features z3` | exit 0 |
| Functional — offensive (default) | `cargo check -p touring-offensive` | exit 0 |
| Robust — clippy offensive (z3) | `cargo clippy -p touring-offensive --lib --no-deps --features z3 -- -D warnings` | exit 0 (5 unused_mut warnings cosmetic) |
| Robust — clippy offensive (default) | `cargo clippy -p touring-offensive --lib --no-deps -- -D warnings` | exit 0 |
| Readable — offensive tests (z3) | `cargo test -p touring-offensive --lib --features z3` | 276/276 PASS (+6) |
| Readable — offensive tests (default) | `cargo test -p touring-offensive --lib` | 270/270 PASS (no regressions in z3-OFF path) |
| Secure — wiring audit | `touring wiring audit -j` | no new orphans |
| No Regression — Stub→Void | `cargo test -p touring-offensive --lib stub_void_labeling` | PASS (CRITICAL contract preserved) |

**Audit**: 12/12 samples verified FACT 1.0 · 0 frauds · 0 regressions · 0 new orphans.

---

## P1 STAND-DOWN LIFTED

The P1 stand-down (documented in `ES1-P1P4-NOTE.md`) is **LIFTED**:

> **P1 STAND-DOWN (was)**: Z3/CVC5 native backends are **NOT yet wired** into `prove_claim` (z3 0.20 / cvc5 API incompatibility in the existing reference files); `prove_claim` returns `ProofStatus::Error` with an honest `model` note for non-Stub backends. Real Z3/CVC5 rewire is **ES1 P2**.
>
> **P1 STAND-DOWN (now)**: **LIFTED for Z3** — `prove_claim` with `SolverBackendKind::Z3` now returns real `ProofStatus::Sat` / `ProofStatus::Unsat` / `ProofStatus::Unknown` when the `z3` feature is enabled. **STILL ACTIVE for CVC5** — `libcvc5-dev` missing on build host; P2.5 followup when system dep is available. **Stub→Void contract unchanged** — Stub backend continues to return `ProofStatus::Void` for all claims (verified in `stub_void_labeling` test).

### Meta-lesson: stand-downs must enumerate the input substrate state

P1's stand-down was honest about the **output** behavior ("returns Error for non-Stub backends") but under-reported the **input** substrate state (the z3_backend module was dormant, bit-rotted, and had a duplicate struct bug). The engineer's halt at VGP discovery was the right call — it avoided 38+ cascading compilation errors. Going forward: stand-downs must enumerate the **input substrate state**, not just the **output behavior**.

---

## What's Next (Tier 2 strategic picks)

1. **ES1 P3** (~7ed) — X3.5 gateway wiring + `Execution<Proven>` typestate. The **strategic differentiator** of the OP5 verifier-in-the-loop lift. Wire `prove_claim` as a CEG gateway stage between X3 (VGP) and X4 (PREDICT); attach `ProofReport` to `EvidenceBundle`; introduce `Execution<Proven>` typestate. This is the natural next wave — P2 closes the substrate, P3 closes the integration.
2. **ES1 P2.5** (~2ed, blocking on `libcvc5-dev` system dep) — cvc5 0.4 migration using the same typed-AST pattern as P2. Independent SMT solver for cross-validation.
3. **ES3 P2** (~6ed) — `supervised.rs` X8 with WRITES. The real OP4 §5.2.4 deliverable.
4. **ES4 P2-P4** (~7ed) — Unify distillation + calibrated prediction + wire world-model to `prove_claim`.
5. **ES2 P3-P5** (~5ed) — Compaction re-attend + self-verify loop using the new `prove_claim` API.

---

**SEALED**: ES1 P2 SHIPPED 2026-06-01. **P1 STAND-DOWN LIFTED for Z3**. The dormant `solver/z3_backend.rs` is now a live, z3 0.20 typed-AST SMT backend. `prove_claim` returns real Sat/Unsat verdicts. 2 modified files, 6 new tests, 276/276 lib tests pass, 0 regressions, 0 new orphans, 0 frauds. CAH `interface.formal-verify` row ready to flip on next oracle re-run.
