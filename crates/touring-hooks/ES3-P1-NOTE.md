# ES3 P1 — `txn_lock_enforcement` flipped to default-ON

**Date**: 2026-06-01
**Wave**: ES3 P1 of `~/.claude/rust/docs/2026-05-30-cah-epic-subsystems-roadmap`
**Checkpoint**: `~/.claude/rust/docs/checkpoints/2026-06-01-es3-p1-txn-lock-enforcement-live.toon`

## What changed

| File | Change |
|---|---|
| `Cargo.toml:29` | `txn_lock_enforcement` added to `default = [...]` features list (was feature-gated default-OFF) |
| `src/gateway/txn.rs:83-94` | New `pub fn AccessDeclaration::from_tool_payload(tool: &str, payload: &str) -> Self` helper — pure-reader, never declares writes |
| `src/gateway/txn.rs:253-291` | 4 new unit tests covering the helper |
| `src/gateway/pre_exec.rs:199-218` + `:217` | Live call site in `run_gateway`: after `record_ceg_captured()`, acquire a `TxnPermit` via `from_tool_payload`; conflict → `tracing::warn!` + proceed read-only |
| `src/gateway/pre_exec.rs:884-967` | New `mod txn_live_e2e` with 3 E2E tests |
| `src/gateway/exec_pool.rs:215-220, 361-367` | Doc sync to "default-on since S-10 rollout, ES3 P1, 2026-06-01" |

## Why

`S-10 lost-update guard defense-in-depth`. The roadmap's ES3 diagnosis
(`2026-05-30-cah-epic-subsystems-roadmap.md` §ES3) found that:

- `txn_lock_enforcement` was enabled by **zero crates / zero tests / zero CI** — a dead feature gate.
- `run_gateway` is **synchronous** (`pre_exec.rs:177`) and never called `ExecPool::acquire_txn`.
- The two live CLI consumers (`txn-acquire`, `conflict-check`) were **single-threaded simulations**.

The flip to default-ON + a live call site in `run_gateway` closes the "feature-dead
simulation" gap. The feature is now the live path; the moment `run_gateway` is made
async (N>1 parallel agents, ES3 P2), no further wiring is needed.

## Migration guide

**No breaking change.** The `txn_lock_enforcement` feature still exists; it is now
**default-on**. Existing code that does not touch the feature is unaffected.

If you were depending on the feature being **off** in your build:

```bash
# Opt-out (no change in behavior vs. pre-ES3-P1 builds):
cargo build -p touring-hooks --no-default-features

# Note: this opt-out build is currently PRE-EXISTING broken
# (tracked separately, NOT a regression from this wave).
```

If you want to disable the `txn_lock_enforcement` feature specifically while keeping
the rest of the default features:

```toml
# In your Cargo.toml:
[dependencies]
touring-hooks = { version = "0.3.8", default-features = false }
# Or, if you need most defaults but not this one:
touring-hooks = { version = "0.3.8", default-features = false, features = ["hooks-active", "all-hooks", /* etc., omitting txn_lock_enforcement */] }
```

## Honest scope

`run_gateway` is **synchronous + single-threaded** (`pre_exec.rs:177`). The
`TxnPermit` acquired at `pre_exec.rs:217` is **defense-in-depth / future-proofing**,
not real concurrency enforcement. The actual **OP4 §5.2.4 lost-update guard for
writes** lives in `supervised.rs` X8 and is **ES3 P2**.

The conflict `tracing::warn!` is the **audit signal** — it will become load-bearing
the moment `run_gateway` is made async. Until then, the feature is live but inert on
the conflict path. **This is honest scope, not theater**: the default-ON flip
ensures the moment a real concurrent consumer arrives, no further wiring is needed.

## References

- Roadmap: `~/.claude/rust/docs/2026-05-30-cah-epic-subsystems-roadmap.md` (ES3 section, L11 progress note)
- Checkpoint (TOON v1.0): `~/.claude/rust/docs/checkpoints/2026-06-01-es3-p1-txn-lock-enforcement-live.toon`
- Memory keys: `es3-p1-shipped-2026-06-01`, `es3-p1-substrate-mapped-2026-06-01`, `es3-p1-run-gateway-defense-in-depth-2026-06-01`
- Engineer report symbols: `from_tool_payload` (txn.rs:94), `AccessDeclaration` (txn.rs:46), `TxnLockManager` (txn.rs:139), `TxnPermit` (exec_pool.rs:368), `acquire_txn` (exec_pool.rs:404), `_txn_permit` call site (pre_exec.rs:217), `mod txn_live_e2e` (pre_exec.rs:884)

## Next wave

**ES3 P2** — `supervised.rs` X8 with WRITES (real OP4 §5.2.4 deliverable). 6ed
estimated (or 4ed if reusing the call-site pattern from `pre_exec.rs:217`).
Or **ES1 P1+P4** — last Tier 1, strategic SMT service (10ed, ~70% of OP5 lift
without touching the CEG hot path).
