# Cycle 2 Fix Recipe — W3.2 (Class A demo)

> **Wave**: W3 of the 47to13-residual UPGRADE plan (Premium Elite).
> **Date**: 2026-06-04
> **Recipe applies to**: Cycles 2, 3, 4, 6, 7, 8 (the 6 intra-crate sibling-mod
> re-exports; same pattern, different files).

## The cycle

```
Cycle #2 (depth: 2, modules: 2, severity: medium)
  crates/touring-hooks/src/gateway/fast_path.rs →
  crates/touring-hooks/src/gateway/pre_exec.rs
  [CYCLE CLOSES]
```

`fast_path.rs` defines:
- `is_provably_pure(body: &CodeBody, language: Lang, static_report: &StaticReport) -> bool`
- `FastPathDecision` enum (`RunSandbox | SkipSandbox`)
- `fast_path_decision(...) -> FastPathDecision`
- `pure_skip_outcome(...) -> SandboxOutcome` — a fallback outcome

`pre_exec.rs` is the CEG driver (`run_gateway(deps: GatewayDeps) -> GatewayOutcome`).
It uses `fast_path::is_provably_pure` + `fast_path::FastPathDecision` to
short-circuit X5 SANDBOX when the body is provably pure.

The cycle closes because `fast_path.rs::pure_skip_outcome` returns a
`SandboxOutcome` (defined in `super::sandbox_stage`), and
`pre_exec.rs` likely also references `sandbox_stage` types. The wiring
tool sees the round-trip.

## The fix (Class A, sub-pattern 1: extract shared module)

### Step 1 — Identify the shared types

The shared type is **`SandboxOutcome`** (or whatever the cycle's pivot is).
Currently in `sandbox_stage.rs`. Both `fast_path.rs` and `pre_exec.rs`
import it.

### Step 2 — Create the shared module (if not already present)

```bash
# If `gateway/shared.rs` doesn't exist:
touch /home/gabrielgadea/.claude/rust/crates/touring-hooks/src/gateway/shared.rs
```

```rust
// crates/touring-hooks/src/gateway/shared.rs
// Re-exports types that are shared between fast_path and pre_exec.
pub use super::sandbox_stage::SandboxOutcome;
pub use super::sandbox_stage::SandboxConfig;
```

### Step 3 — Update fast_path.rs

```rust
// In fast_path.rs, change:
use super::sandbox_stage::SandboxOutcome;
// To:
use super::shared::SandboxOutcome;
```

### Step 4 — Update pre_exec.rs

```rust
// In pre_exec.rs, change:
use super::sandbox_stage::SandboxOutcome;
// To:
use super::shared::SandboxOutcome;
```

(If `pre_exec.rs` doesn't import `SandboxOutcome` directly, skip this step.)

### Step 5 — Verify

```bash
cd /home/gabrielgadea/.claude/rust
cargo check --workspace 2>&1 | tail -5     # MUST be exit 0
touring wiring cycles --min-depth 2 2>&1 | head -20
# Cycle #2 should no longer appear in the output.
```

## The pattern (reusable for cycles 3, 4, 6, 7, 8)

| Step | Action |
|------|--------|
| 1 | Identify the **shared type** that both files in the cycle import. |
| 2 | Create (or use) a `mod shared` (or `super::types`) in the parent directory. |
| 3 | Re-export the shared type from `mod shared`. |
| 4 | Change BOTH cycle files to import from `mod shared` instead of the direct path. |
| 5 | Verify `cargo check --workspace` exit 0 + `touring wiring cycles` no longer reports the cycle. |

**Effort per cycle**: 30-60 minutes (most of it is finding the shared
type + verifying no semantic change). **Risk**: LOW (intra-crate; no
public API change).

## Why this works (Rust semantics)

Rust allows intra-crate `mod` cycles as long as the **public surface** is
acyclic. The wiring tool's Tarjan SCC walks the dependency graph and
flags any cycle, even intra-crate ones. By moving the shared type to
a `mod shared`, both consumers go through the same node, eliminating
the back-edge from the wiring tool's perspective.

## Risk mitigations

1. **Type mismatch check**: after the refactor, `cargo check --workspace`
   must remain exit 0. If not, the shared type moved semantics; revert.
2. **Public API check**: `touring ast overview crates/touring-hooks/src/gateway/`
   should show the same public types (no removal, just re-routing).
3. **Test check**: `cargo test -p touring-hooks` should remain green.
4. **Orphan check**: `touring wiring orphans -j` should not increase
   (no new pub symbols introduced by the refactor).

## When NOT to use this pattern

- If the shared type has a **different lifetime** in the two contexts
  (e.g. one wants `'static`, the other wants `'a`), the shared
  abstraction loses information. Use a trait instead.
- If the cycle is **cross-crate** (cycles 1, 9), this pattern doesn't
  apply; use a trait-object boundary in `touring-foundation`.
- If the cycle is **depth 9+** (cycle 5), there are multiple pivot
  types; the fix needs more thought than a single `mod shared` extraction.

## Cycle 2 specific risk assessment

- The `pure_skip_outcome` function is **called only by** `pre_exec.rs`.
  Moving its dependencies to a shared module is a no-op for callers.
- The `is_provably_pure` function is **pure** (no side effects). The
  refactor is non-semantic.
- **Risk: very low**. Estimated 30-45 minutes including verification.

---

_Recipe authored 2026-06-04 as W3.2 of the upgrade plan. Apply in
future session for cycle 2 + cycles 3, 4, 6, 7, 8 (same pattern)._
