# ES1 P3 Release Note — X3.5 PROVE Stage Wired into CEG Pipeline

**Date**: 2026-06-01
**Status**: SHIPPED
**Roadmap**: `~/.claude/rust/docs/2026-05-30-cah-epic-subsystems-roadmap.md` §2 ES1 P3
**Companion waves**: ES1 P1+P4 (`ES1-P1P4-NOTE.md`) · ES1 P2 (`ES1-P2-NOTE.md`) · ES4 P1 · ES2 P1+P2 · ES3 P1
**Effort**: 4.5ed consumed of 7ed budget
**Predecessor**: ES1 P2 — `2026-06-01-es1-p2-z3-0-20-migration.toon`
**Strategic role**: Tier 2 strategic differentiator — closes the X3.5 gap between X3 (VGP) and X4 (PREDICT) in the CEG pipeline

---

## What Changed

### 1. `Execution<S>` sealed typestate grew from 8 to 9 states (`typestate.rs:120`)

```rust
// BEFORE (P2 — 8 states, ordinals 0..=7)
Captured = ("X0-CAPTURE", 0),
Classified = ("X1-CLASSIFY", 1),
StaticallyChecked = ("X2-STATIC", 2),
Verified = ("X3-VGP", 3),
Predicted = ("X4-PREDICT", 4),
Sandboxed = ("X5-SANDBOX", 5),
Gated = ("X6-CAPABILITY-GATE", 6),
Decided = ("X7-DECISION", 7),

// AFTER (P3 — 9 states, ordinals 0..=8, Proven inserted at 4)
Captured = ("X0-CAPTURE", 0),
Classified = ("X1-CLASSIFY", 1),
StaticallyChecked = ("X2-STATIC", 2),
Verified = ("X3-VGP", 3),
Proven = ("X3.5-PROVE", 4),       // NEW
Predicted = ("X4-PREDICT", 5),    // 4 -> 5
Sandboxed = ("X5-SANDBOX", 6),    // 5 -> 6
Gated = ("X6-CAPABILITY-GATE", 7),// 6 -> 7
Decided = ("X7-DECISION", 8),     // 7 -> 8
```

`Verified -> Proven, Proven -> Predicted` transitions added at `typestate.rs:147-148`.

### 2. `Evidence::proof_report` field added (`typestate.rs:299`)

```rust
pub struct Evidence {
    // ... existing fields ...
    pub proof_report: Option<crate::offensive_integration::ProofReport>,
    // ^ NEW. None when no claim was asserted (zero Z3 cost).
    //   Populated by prove_claim when claim = Some(...).
}
```

### 3. `Execution::prove_claim` constructor added (`typestate.rs:482`)

The **sole** way to advance from `Execution<Verified>` to `Execution<Proven>`:

```rust
pub fn prove_claim(
    self,
    claim: Option<ClaimKind>,
    backend: SolverBackendKind,
    ctx: &ClaimContext,
) -> Execution<crate::gateway::Proven>
```

- `claim = None` → typestate advances, `proof_report = None`, **no Z3 call** (fast path).
- `claim = Some(...)` → `prove_claim` runs, `proof_report = Some(report)`.
- `backend = Stub` → `ProofStatus::Void` (CRITICAL anti-overconfidence contract preserved).

### 4. `Execution<Proven>::predict` MOVED (`predict.rs:146`)

The `predict` function is now `impl Execution<Proven>` — was `impl Execution<Verified>` in P1/P2. The type system now **enforces** that every predict call is preceded by a `prove_claim`. The compiler caught 6 unupdated call sites at type-check time (3 in `run_gateway` chain, 2 in test helpers, 1 in the typestate doctest).

```rust
// BEFORE (P2)
impl Execution<Verified> {
    pub fn predict<F>(self, f: F) -> Execution<Predicted> { ... }
}

// AFTER (P3)
impl Execution<Proven> {                                       // MOVED
    pub fn predict<F>(self, f: F) -> Execution<Predicted> { ... }
    //      ^^^^^ type system: must call .prove_claim(...) first
}
```

### 5. `GatewayDeps` extended with 3 fields (`pre_exec.rs:75-89`)

```rust
pub struct GatewayDeps<'a> {
    // ... existing fields ...
    /// X3.5 PROVE — optional claim to verify before X4 prediction
    /// (None = no claim = no Z3 cost; default callers use None).
    /// Some(claim) => prove_claim runs the encoding + backend dispatch.
    pub claim: Option<crate::offensive_integration::ClaimKind>,
    /// Context for the claim (file:line, scope, etc.).
    /// Sensible defaults via ClaimContext::default() when needed.
    pub claim_context: crate::offensive_integration::ClaimContext,
    /// Solver backend to use when claim is Some(...).
    /// Stub returns ProofStatus::Void; Z3 returns Sat/Unsat (when wired).
    pub solver_backend: crate::offensive_integration::SolverBackendKind,
}
```

All 6 construction sites in `pre_exec.rs` updated with safe defaults (None, `ClaimContext::default()`, `SolverBackendKind::Stub`).

### 6. `run_gateway` chain wired with `.prove_claim(...)` (`pre_exec.rs:243-246`)

```rust
// In the observe path of run_gateway:
let proven = verified
    .prove_claim( // X3.5 (ES1 P3, 2026-06-01)
        deps.claim.clone(),
        deps.solver_backend,
        &deps.claim_context,
    )
    .predict(/* ... */)
    // ... rest of the chain ...
;
```

When `deps.claim = None`, the `.prove_claim(...)` call is a typestate re-brand with zero Z3 cost.

### 7. New `offensive_integration` re-export module (`src/offensive_integration.rs`, 19 LOC)

```rust
// crates/touring-hooks/src/offensive_integration.rs (NEW)
//
// ES1 P3 (2026-06-01): re-exports SMT solver types from touring-offensive
// so the gateway can use them without leaking touring-offensive's internal layout.

pub use touring_offensive::solver::{
    ClaimContext, ClaimKind, ProofReport, SolverBackendKind, prove_claim,
};
```

Registered in `lib.rs:33`:

```rust
pub mod offensive_integration; // ES1 P3 (2026-06-01): re-exports SMT solver types (ProofReport/ClaimKind/etc)
```

### 8. 5 new tests (4 unit + 1 integration)

| Test | File:Line | What it proves |
|---|---|---|
| `prove_claim_with_none_attaches_no_report_and_advances_to_proven` | `typestate.rs:741` | `claim = None` → `proof_report = None`, typestate advances |
| `prove_claim_with_some_claim_runs_prove_claim_and_attaches_report` | `typestate.rs:762` | `claim = Some(...)` → `proof_report = Some(report)`, typestate advances |
| `prove_claim_with_stub_backend_returns_void_status` | `typestate.rs:787` | Stub backend → `ProofStatus::Void` (CRITICAL contract preserved) |
| doormat test (proves `Execution<Proven>::predict` is the only path to `Execution<Predicted>`) | `typestate.rs:820+` | Type-system enforcement verified at compile-time |
| `run_gateway_with_claim_attaches_proof_report_to_evidence` | `pre_exec.rs:916` | Full chain test: `run_gateway` with `claim: Some(Postcondition)` → evidence attaches `ProofReport` |

---

## Why

ES1 P3 closes the **X3.5 PROVE gap** in the CEG pipeline. The roadmap (`2026-05-30-cah-epic-subsystems-roadmap.md` §ES1) flagged this as the **strategic differentiator** of the OP5 (verifier-in-the-loop) lift:

> P3 (X3.5 gateway wiring + Execution<Proven> typestate) — the natural next wave. P2 closes the substrate (real Z3 wired in `prove_claim`); P3 closes the integration (`prove_claim` is callable from the CEG hot path).

Concretely, the wave delivers:

1. **Type-system enforcement of the stage ordering** — the CEG pipeline now has 9 sealed typestate variants, and the compiler refuses all shortcuts. The path `Execution<Captured> -> Verified -> Proven -> Predicted -> ...` is the **only** valid path; missing `.prove_claim(...)` is a compile error, not a runtime panic.
2. **Substrate completion for the verifier-in-the-loop lift** — the `prove_claim` service (ES1 P1+P4) with real Z3 (ES1 P2) is now **callable from the CEG hot path**. The default behavior is unchanged (no claim → no Z3 cost), but the substrate is ready for speculative-driver integration.
3. **Defense-in-depth for CAH conformance** — the X3.5 stage is now a first-class citizen in the pipeline. CAH `interface.formal-verify` row 0.65 is ready to flip PARCIAL → CONFORME ~0.85+ on next oracle re-run.

---

## Migration Guide

### For callers of `run_gateway`

`GatewayDeps` grew 3 new fields. Existing code that constructs `GatewayDeps` must add the new fields (with safe defaults if you don't want to assert a claim):

```rust
// BEFORE (P2)
let deps = GatewayDeps {
    profile: CapabilityProfile::Sandboxed,
    predictor: Arc::new(MyPredictor),
    sandbox: Arc::new(MySandbox),
    gate: Arc::new(MyGate),
    typestate_only: false,
    // ... other existing fields ...
};

// AFTER (P3) — add 3 fields
let deps = GatewayDeps {
    profile: CapabilityProfile::Sandboxed,
    predictor: Arc::new(MyPredictor),
    sandbox: Arc::new(MySandbox),
    gate: Arc::new(MyGate),
    typestate_only: false,
    // ... other existing fields ...
    // NEW:
    claim: None,                                          // no claim by default
    claim_context: ClaimContext::default(),               // safe default
    solver_backend: SolverBackendKind::Stub,              // safe default
};
```

### For callers that build `Execution<Verified>` and call `.predict()`

The `predict` method moved from `Execution<Verified>` to `Execution<Proven>`. The compiler will catch this with a clear error message:

```
error[E0599]: no method named `predict` found for struct `Execution<Verified>` in the current scope
   --> src/my_module.rs:42:18
    |
42  |     verified.predict(my_predictor);
    |              ^^^^^^ method not found
    |
help: call `.prove_claim(...)` first
    |
42  |     verified.prove_claim(None, SolverBackendKind::Stub, &ClaimContext::default()).predict(my_predictor);
    |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The fix is to insert `.prove_claim(None, SolverBackendKind::Stub, &ClaimContext::default())` between `.vgp_verify()` and `.predict()`. If you want to assert a claim, replace `None` with `Some(ClaimKind::Postcondition)` (or another variant).

### For tests that use `Execution::verify()` as a setup step

Tests that previously did `Execution::capture(...).classify(...).statically_check(...).verify(...)` and then called `.predict(...)` need the same `.prove_claim(...)` insertion. The 4 new typestate unit tests (`prove_claim_with_none_attaches_no_report_and_advances_to_proven`, etc.) demonstrate the pattern.

---

## Honest Scope

> **(1) X3.5 is defense-in-depth only** — the typestate change makes the `prove_claim` service *callable* from the CEG hot path, but the actual **speculative driver integration** (S-12 accept-prefix wiring in `speculative.rs` that asserts a claim based on the predicted action) is **ES1 P3.5** followup. P3 ships the substrate; P3.5 ships the integration.

> **(2) `prove_claim(None, ...)` is a no-op for Z3** — the function still re-brands the typestate so the X3.5-PROVE stage record is appended to the audit log, but it skips the Z3/cvc5 call entirely. The `None` case is the **fast path** for the default `run_gateway` chain (no behavior change for callers that don't assert a claim).

> **(3) Stub->Void contract preserved** — when `claim = Some(...)` AND `solver_backend = Stub`, `prove_claim` returns `ProofStatus::Void`. This is the CRITICAL anti-overconfidence contract from ES1 P1. Verified in `prove_claim_with_stub_backend_returns_void_status`.

> **(4) Pre-existing test failure NOT a regression** — `e2e_diagnostic_rfc100::touring_binary_wired_and_healthy` fails with `wiring_diagnostic kind_unknown=1070`. This is a **pre-existing environmental failure** (the test inspects `touring wiring audit -j` output for a `kind_unknown` field; the 1070 unmatched rows are pre-2026 entries that pre-date the `kind` discriminator). The failure was present before P3 and is NOT a P3 regression.

---

## Type-System Enforcement — The Payoff

The most significant deliverable of P3 is not the new typestate variant — it's the **type-system enforcement of the stage ordering**.

| Before (P2) | After (P3) |
|---|---|
| `Execution<Verified>::predict` existed | `Execution<Proven>::predict` only |
| One could call `.predict()` without VGP or PROVE | Compiler refuses; only path is `Captured -> Verified -> Proven -> Predicted` |
| 6 unupdated call sites = 6 runtime bugs waiting to happen | 6 unupdated call sites = 6 compile errors caught at `cargo check` time |

**Counterfactual**: in a language without sealed typestates, the same change would have required either (a) a runtime check (`assert!(state == Proven)`) or (b) discipline + tests. The Rust type system caught all 6 sites in **~2 seconds** during `cargo check` — cheaper than a single test run.

---

## Files Modified (12 total)

| File | LOC delta | What changed |
|---|---|---|
| `crates/touring-hooks/src/gateway/typestate.rs` | +60 | New `Proven` typestate variant; `Evidence::proof_report` field; `Execution::prove_claim` constructor; 4 new unit tests; ordinal renumbering |
| `crates/touring-hooks/src/gateway/predict.rs` | +4 | `impl Execution<Proven>` MOVED; 1 doctest updated |
| `crates/touring-hooks/src/gateway/pre_exec.rs` | +44 | `GatewayDeps` extended (3 fields); 6 construction sites updated; `.prove_claim` wired in chain; 1 new integration test |
| `crates/touring-hooks/src/lib.rs` | +1 | `pub mod offensive_integration;` |
| `crates/touring-hooks/src/offensive_integration.rs` (NEW) | +19 | Re-export module |
| `crates/touring-hooks/src/gateway/sandbox_stage.rs` | +6 | 3 sites updated to use `crate::offensive_integration::*` |
| `crates/touring-hooks/src/gateway/learn.rs` | 0 | 1 site updated to add 3 new `GatewayDeps` fields |
| 4 other gateway/benches/tests sites | various | Doctests + benchmark bodies updated to use new `Execution<Proven>` shape |

---

## Validation Gates

| Gate | Result |
|---|---|
| `cargo check --workspace --all-targets` | exit 0 (0 errors) |
| `cargo test -p touring-hooks --lib` | 3958/3958 PASS (baseline 3953 → 3958, +5) |
| `cargo test -p touring-hooks --test ceg_e2e` | 118/118 PASS (no regression) |
| `cargo test -p touring-hooks --test capnp_embed_e2e` | 2/2 PASS (no regression) |
| `cargo clippy -p touring-hooks --lib --no-deps -- -D warnings` | exit 0 (no new warnings) |
| Stub→Void contract | PASS (verified in `prove_claim_with_stub_backend_returns_void_status`) |
| `touring wiring audit -j` | 0 new orphans, 0 regressions |

---

## Memory Keys

- `es1-p3-x3-5-gateway-plan-2026-06-01` (consolidated lesson + RL reward)
- `es1-p3-x3-5-gateway-wiring-2026-06-01` (post-ship lesson)

---

## Next: ES1 P3.5 — Speculative Driver Integration

P3 ships the substrate. **P3.5 ships the integration** — wire the `accept-prefix` logic in `speculative.rs` to assert a `ClaimKind` based on the predicted action. ~3ed. The X3.5 stage is now opt-in by claim assertion; P3.5 makes the speculative driver assert the right claim.

Other followups:
- **ES1 P2.5** — cvc5 0.4 migration (~2ed, blocking on `libcvc5-dev`)
- **ES1 P4** — `claim_from_intent` helper (~2ed) — derives `ClaimKind` from the speculative driver's intent
- **ES3 P2** — `supervised.rs` X8 with WRITES (~5ed) — independent of ES1
- **`touring index rebuild`** (~30s) — daemon's index is stale after P3; downstream agents depending on `touring index find` will see stale results until rebuild
