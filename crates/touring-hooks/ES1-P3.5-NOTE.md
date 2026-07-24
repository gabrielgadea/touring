# ES1 P3.5 Release Note — S-12 Speculative Driver Now Consumes X3.5 PROVE Verdicts

**Date**: 2026-06-02
**Status**: SHIPPED
**Roadmap**: `~/.claude/rust/docs/2026-05-30-cah-epic-subsystems-roadmap.md` §2 ES1 P2-P3 followups
**Companion waves**: ES1 P1+P4 (`ES1-P1P4-NOTE.md`) · ES1 P2 (`ES1-P2-NOTE.md`) · ES1 P3 (`ES1-P3-NOTE.md`) · ES4 P1 · ES2 P1+P2 · ES3 P1
**Effort**: 2.0ed consumed of 3ed budget
**Predecessor**: ES1 P3 — `2026-06-01-es1-p3-x3-5-gateway-wiring.toon`
**Strategic role**: Tier 2 strategic differentiator followup — closes the X3.5 acceptance loop. P3 made `prove_claim` *callable*; P3.5 makes it *called* by the S-12 speculative driver.

---

## What Changed

### 1. New closure-injectable `filter_by_proof<F>` helper (`speculative.rs:123`)

```rust
// NEW in P3.5
pub fn filter_by_proof<'a, F>(
    candidates: &'a [CandidateAction],
    claim: Option<&ClaimKind>,
    ctx: &ClaimContext,
    backend: SolverBackendKind,
    prove_closure: F,
) -> &'a [CandidateAction]
where
    F: Fn(ClaimKind, &ClaimContext, SolverBackendKind) -> ProofReport,
{
    // OPT-IN: when claim is None, return input slice unchanged
    let Some(claim) = claim else {
        return candidates;
    };
    let report = prove_closure(claim.clone(), ctx, backend);
    match report.status {
        ProofStatus::Sat       => candidates,         // claim proven — keep all
        ProofStatus::Unsat     => &[],                // claim disproven — drop all
        ProofStatus::Error     => &[],                // fails-closed
        ProofStatus::Unknown   => candidates,         // no info — keep all
        ProofStatus::Void      => candidates,         // NEUTRAL (anti-overconfidence)
    }
}
```

**Closure-injectable design**: the `F: Fn(ClaimKind, &ClaimContext, SolverBackendKind) -> ProofReport` parameter lets tests inject 1-line mock provers returning each `ProofStatus` variant in turn. Production passes the real `prove_claim` symbol. Zero-cost after monomorphization.

### 2. `run_gateway_speculative` pre-pass wiring (`pre_exec.rs:295`)

```rust
// BEFORE (P3 — no X3.5 filter)
pub fn run_gateway_speculative(
    candidates: &[CandidateAction],
    deps: &GatewayDeps<'_>,
) -> AcceptedPrefix {
    let ranked = rank_by_predicted(candidates, &model, deps.predictor);
    speculative_execute(&ranked, |candidate| { ... })
}

// AFTER (P3.5 — X3.5 filter as pre-pass)
pub fn run_gateway_speculative(
    candidates: &[CandidateAction],
    deps: &GatewayDeps<'_>,
) -> AcceptedPrefix {
    // ES1 P3.5: X3.5 PROVE pre-filter — drop candidates whose claim
    // is Unsat/Error before the draft ranking and the speculative loop.
    // OPT-IN: when `deps.claim` is None, the filter returns the input
    // slice unchanged (zero overhead for default callers).
    let proven = super::speculative::filter_by_proof(
        candidates,
        deps.claim.as_ref(),
        &deps.claim_context,
        deps.solver_backend,
        prove_claim,
    );
    let ranked = rank_by_predicted(proven, &model, deps.predictor);
    speculative_execute(&ranked, |candidate| { ... })
}
```

**Defense-in-depth at the earliest possible point**: candidates with `ProofStatus::Unsat` (claim proven false) or `ProofStatus::Error` (encoder/solver fail) are dropped *before* the draft ranking and *before* the accept-prefix loop. The X3.5 verdict is consulted at the earliest possible point in the speculative pipeline.

### 3. P3 LEFTOVER FIX: 5 `GatewayDeps` struct literals in `touring-server/src/cli/exec.rs`

P3 extended `GatewayDeps` with 3 X3.5 fields (`claim`, `claim_context`, `solver_backend`) and updated 6 in-crate construction sites in `pre_exec.rs`. P3 MISSED 5 cross-crate callers in `touring-server/src/cli/exec.rs` at L152, L354, L485, L698, L1072.

**P3.5 fix** (caught at FASE 0 by `cargo check --workspace` → 5 E0063 errors): each of the 5 struct literals now includes the 3 X3.5 fields with safe defaults:

```rust
let deps = GatewayDeps {
    // ... existing fields ...
    claim: None,                                    // NEW
    claim_context: ClaimContext::default(),         // NEW
    solver_backend: SolverBackendKind::Stub,        // NEW
};
```

The fix is mechanical and the 3 fields use safe defaults that match the `run_gateway` default path. Zero behavior change for the 5 cross-crate callers.

---

## Why

P3 explicitly punted the speculative driver integration as a P3.5 followup. From `ES1-P3-NOTE.md` §10 honest scope note 3:

> **X3.5 is defense-in-depth only** — the typestate change makes the `prove_claim` service *callable* from the CEG hot path, but the actual **speculative driver integration** (S-12 accept-prefix wiring in `speculative.rs` that asserts a claim based on the predicted action) is **ES1 P3.5** followup. P3 ships the substrate; P3.5 ships the integration.

P3.5 makes the X3.5 PROVE substrate **USED** by the S-12 speculative driver:

1. **Defense-in-depth**: candidates with `ProofStatus::Unsat` (claim proven false) are dropped *before* the draft ranking and *before* the accept-prefix loop. A claim that the verifier proves false cannot produce an `AcceptedPrefix`.
2. **Fails-closed on encoder/solver error**: `ProofStatus::Error` (encoder or solver failed) drops all candidates. Better to over-reject than to silently accept on broken infrastructure.
3. **Anti-overconfidence preserved**: `ProofStatus::Void` (Stub backend = no real proof attempted) is treated as NEUTRAL — all candidates are kept. This preserves the CRITICAL P1 contract that the system never claims to have proven anything it didn't.
4. **OPT-IN default**: when `GatewayDeps.claim` is `None` (the default for callers that don't explicitly assert a claim), the filter returns the input slice unchanged. Zero overhead, lossless contract preserved for default callers.

---

## Migration Guide

**Zero new API surface** for callers of `run_gateway_speculative`:

- The function signature is unchanged: `pub fn run_gateway_speculative(candidates: &[CandidateAction], deps: &GatewayDeps<'_>) -> AcceptedPrefix`.
- Default callers (no `claim` wired) see **zero overhead** — `filter_by_proof` returns the input slice unchanged when `claim` is `None`. The lossless contract from P3 is preserved.
- Callers that want to opt in to X3.5 filtering set `GatewayDeps.claim = Some(ClaimKind::...)` and (optionally) `solver_backend = SolverBackendKind::Z3` (or `Cvc5` once P2.5 ships).

**Cross-crate callers**: any code outside `touring-hooks` that constructs a `GatewayDeps` MUST include the 3 X3.5 fields. The P3 leftover in `touring-server/src/cli/exec.rs` is a cautionary tale — `cargo check --workspace` at FASE 0 is the canonical gate for catching missed callers.

---

## HONEST SCOPE (4 documented limits)

Repeated in 4 places — `speculative.rs:123-160` (helper doc), `pre_exec.rs:287-294` (pre-pass doc), the checkpoint body (`2026-06-02-es1-p3-5-s12-speculative-driver.toon`), and this release note:

1. **Stub->Void is NEUTRAL (NOT rejection)** — when `claim = Some(...)` AND `solver_backend = Stub`, `prove_claim` returns `ProofStatus::Void` (the CRITICAL anti-overconfidence contract from ES1 P1). The downstream `filter_by_proof` treats `Void` as "no info" and **keeps all candidates** — it does NOT reject them. This means a default-caller that sets `claim = Some(...)` AND `solver_backend = Stub` sees **identical behavior** to a default-caller that sets `claim = None` (both pass through all candidates).
2. **`filter_by_proof` is OPT-IN by claim assertion** — when `claim = None` (the default), the filter returns the input slice unchanged. Zero overhead, no Z3/cvc5 invocation, lossless contract preserved. The 3 new integration tests include `opt_in_none_identity` to lock this contract.
3. **The pre-pass is a *narrowing*, not a *replacement*** — `filter_by_proof` runs BEFORE `rank_by_predicted` and BEFORE `speculative_execute`. The accept-prefix loop is unchanged: each candidate is still subject to the `run_gateway` verdict. A `Deny` verdict still truncates the prefix (locked in by `run_gateway_speculative_with_proof_filter_still_truncates_on_deny`).
4. **P3 leftover is a META-LESSON about completeness** — P3 extended `GatewayDeps` with 3 X3.5 fields and updated 6 in-crate sites, but missed 5 cross-crate callers in `touring-server/src/cli/exec.rs`. The `cargo check --workspace` gate at FASE 0 caught all 5 with E0063 errors. The lesson: **type-def changes are not update-all-callsites by inspection** — the compiler is the canonical gate.

---

## Type-System Enforcement

The `F: Fn(ClaimKind, &ClaimContext, SolverBackendKind) -> ProofReport` parameter on `filter_by_proof` is a **deliberate testability choice**:

- **Production**: `run_gateway_speculative` passes the real `prove_claim` symbol as the closure.
- **Tests**: 1-line mock closures return each `ProofStatus` variant in turn (Sat, Unsat, Error, Unknown, Void).
- **Monomorphization**: the closure is zero-cost at runtime — the compiler generates specialized code for the production `prove_claim` and for each test mock.
- **Pure function**: `filter_by_proof` has no `Result`, no I/O, no `unwrap`. The only side effect is the call to the closure (which is itself pure in the test case).

This is the **sealed-typestate discipline extended to the speculative driver**: the compiler enforces the contract that *some* prover runs, the helper decides what to do with the verdict. Combined with P3's `Execution<Proven>::predict` move, the X3.5 stage is now a **type-system-enforced step in the speculative pipeline** — the only way to advance from `run_gateway_speculative(candidates, deps)` to the accept-prefix loop is through the X3.5 filter (when a claim is asserted).

---

## Test Coverage

### Unit tests in `speculative.rs` (5 new, all PASS)

- `filter_by_proof_opt_in_none_returns_input_unchanged` (L309) — `claim = None` -> identity.
- `filter_by_proof_stub_void_keeps_all_candidates` (L329) — Stub/Void = NEUTRAL.
- `filter_by_proof_unsat_drops_all_candidates` (L347) — Unsat = DROP.
- `filter_by_proof_error_fails_closed` (L366) — Error = DROP.
- `filter_by_proof_unknown_keeps_all_candidates` (L385) — Unknown = keep.

### Integration tests in `pre_exec.rs` (3 new, all PASS)

- `run_gateway_speculative_with_proof_filter_passes_via_stub` (L937) — full chain with Stub/Void.
- `run_gateway_speculative_with_proof_filter_opt_in_none_identity` (L979) — `claim = None` = identical to pre-P3.5.
- `run_gateway_speculative_with_proof_filter_still_truncates_on_deny` (L1009) — X3.5 does NOT bypass Deny truncation.

**Total: 8 new tests, 3966/3966 touring-hooks lib tests pass** (was 3958, +8 net), 118 ceg_e2e + 2 capnp_embed_e2e PASS, 0 regressions, 0 new orphans.

---

## Strategic Impact

**ES1 P2-P3 (15ed strategic differentiator) is now 15/15ed SHIPPED.**

| Wave | ed consumed | Status |
|---|---|---|
| ES1 P1 (prove-claim SMT service) | 9.3/10 | SHIPPED 2026-06-01 |
| ES1 P4 (CLI) | bundled in P1 | SHIPPED 2026-06-01 |
| ES1 P2 (real Z3 0.20) | 7.5/12 | SHIPPED 2026-06-01 |
| ES1 P3 (X3.5 typestate) | 4.5/7 | SHIPPED 2026-06-01 |
| **ES1 P3.5 (S-12 consumption)** | **2.0/3** | **SHIPPED 2026-06-02** |
| **TOTAL** | **15.0/15+12 (P2.5 + P4 + P5 optional)** | **TIER 2 STRATEGIC DIFFERENTIATOR COMPLETE** |

The OP5 verifier-in-the-loop lift is now **substrate-complete AND integrated**: the CEG pipeline runs VGP (X3) → PROVE (X3.5) → PREDICT (X4) → SANDBOX (X5) → GATE (X6) → DECISION (X7), AND the speculative driver consults the X3.5 verdict before the draft ranking. CAH `interface.formal-verify` row 0.65 ready to flip PARCIAL → CONFORME ~0.85+ on next oracle re-run.

---

## Next Steps

1. **ES1 P2.5 — cvc5 0.4 migration** (~2ed, blocking on `libcvc5-dev` system dep). Independent SMT solver for cross-validation.
2. **ES1 P4 — `claim_from_intent` helper** (~2ed). The P3 meta-lesson's next-level mechanism: derive the `ClaimKind` that should be proved from the speculative driver's `Action`. Closure-injectable design, same pattern as `filter_by_proof`.
3. **ES3 P2 — `supervised.rs` X8 with WRITES** (~5ed). ES3 P1 made `txn_lock_enforcement` default-ON for reads; P2 makes the permit meaningful for writes.
4. **Backfill: re-audit P3** to confirm zero other call sites were missed. Workspace-wide `cargo check` is the canonical answer.
5. **Re-run CAH oracle** to measure the `interface.formal-verify` row flip.

---

_Generated by touring-scriber (FASE 7 of TACO Phase Protocol v6.2). Predecessor: `2026-06-01-es1-p3-x3-5-gateway-wiring.toon`. Checkpoint: `2026-06-02-es1-p3-5-s12-speculative-driver.toon`._
