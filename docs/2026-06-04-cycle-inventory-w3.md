# Cycle Inventory — Touring (W3 of the upgrade plan)

> **Date**: 2026-06-04
> **Measurement**: `touring wiring cycles --min-depth 2` (Tarjan SCC, O(V+E))
> **Total cycles detected**: 9 (1 catastrophic, 7 medium, 1 high)

## The 9 cycles (canonical, post-re-measurement)

| # | Depth | Modules | Severity | Path | Classification |
|--:|------:|--------:|----------|------|----------------|
| 1 | 3 | 3 | medium | `touring-hooks/src/post_edit.rs` → `touring-orchestration/src/flow/flow_pipeline.rs` → `touring-orchestration/src/flow/new_flow_builder.rs` | **CROSS-CRATE** (touring-hooks → touring-orchestration) |
| 2 | 2 | 2 | medium | `touring-hooks/src/gateway/fast_path.rs` → `touring-hooks/src/gateway/pre_exec.rs` | intra-touring-hooks/gateway |
| 3 | 3 | 3 | medium | `touring-code/src/ast/call_graph.rs` → `touring-code/src/ast/mod.rs` → `touring-code/src/ast/module_tree.rs` | intra-touring-code/ast |
| 4 | 2 | 2 | medium | `touring-intelligence/src/reasoning/persistence.rs` → `touring-intelligence/src/reasoning/semantic_graph.rs` | intra-touring-intelligence/reasoning |
| 5 | 9 | 9 | high | `touring-code/src/ast/file_heat.rs` → ... (8 more nodes) → `touring-code/src/ast/symbols.rs` | intra-touring-code/ast (long chain) |
| 6 | 2 | 2 | medium | `touring-server/src/tools/file_tools.rs` → `touring-server/src/tools/project_tools.rs` | intra-touring-server/tools |
| 7 | 3 | 3 | medium | `touring-intelligence/src/rl/n3/n3_pipeline.rs` → `touring-intelligence/src/rl/n3/rust_meta_generator.rs` → `touring-intelligence/src/rl/n3/domain_spec.rs` | intra-touring-intelligence/rl/n3 |
| 8 | 2 | 2 | medium | `touring-bindings/src/capnp/server.rs` → `touring-bindings/src/capnp/holon_impl.rs` | intra-touring-bindings/capnp |
| 9 | **391** | **391** | **high** | `touring-simd/src/similarity/cosine.rs` → `touring-hooks-prediction/src/ann_memory/mod.rs` → ... (388 more) → ... | **CROSS-CRATE MASSIVE** |

## Classification by root cause

### Class A — Intra-crate sibling module re-exports (cycles 2, 3, 4, 6, 7, 8; 6 cycles)

These are **Rust-allowed cycles at the module level** (intra-crate) but
the wiring tool flags them because it cannot distinguish `pub(crate)` from
`pub`. The fix is one of:

1. **Move shared types to a `mod shared;`** and re-export from both sides
   (the canonical Rust pattern).
2. **Use `pub(crate)` visibility** for the shared type so it's not exposed
   publicly.
3. **Extract a trait** to a separate file that both sides import.

**Effort per cycle**: 30-60 minutes. **6 cycles × 1h = 6h**.
**Risk**: LOW (intra-crate; no API change).

### Class B — Intra-crate long chain (cycle 5; 1 cycle, depth 9)

A long chain of `mod.rs` re-exports forming a 9-node cycle within
`touring-code/src/ast/`. The fix is to **extract the `ast/graph/` subtree
as a sibling module** that all `ast/*` modules depend on, rather than
through `ast/mod.rs`.

**Effort**: 2-4 hours. **Risk**: MEDIUM (touches 9 files; needs
regression test on AST visitors).

### Class C — Cross-crate cycles (cycles 1, 9; 2 cycles)

- **Cycle 1** (depth 3): `touring-hooks` → `touring-orchestration` → back
  to `touring-hooks`. The fix is a **trait-object boundary**: define
  `trait Orchestrator` in `touring-foundation`, have `touring-orchestration`
  implement it, have `touring-hooks` take `&dyn Orchestrator`. The
  `compiler` driver in `touring-orchestration::tasks` is the re-entry
  point; it should be a method on the trait, not a hard dep.

- **Cycle 9** (depth 391, catastrophic): the 391-node cycle starts at
  `touring-simd/src/similarity/cosine.rs` and threads through
  `touring-hooks-prediction`, `touring-hooks` (extensive), `touring-server`,
  `touring-storage`, `touring-foundation`, `touring-cortex`, etc. **This
  is not a true cycle** in the Rust sense — most of the edges are
  `pub use` re-exports and `use crate::` self-references. The wiring
  tool is over-reporting because it doesn't distinguish `pub use` from
  `use` or `pub(crate) use`. **The actual architectural risk is low**
  but the signal is noisy.

  The fix for the noise is **2-pronged**:
  1. **Update the wiring tool** to filter `pub use` re-exports from cycle
     detection (1-2 days; tool work, not architectural).
  2. **Identify the 5-10 real cross-crate re-entry points** within the
     391-node graph (audit step; half-day).
  3. **Apply trait-object boundaries** to each re-entry point
     (architectural; multi-day).

**Effort cycle 1**: 1-2 days. **Effort cycle 9 (after tool update)**: 3-5 days.

## W3 execution plan (this session + next)

### This session (W3.1 — INVENTORY + DEMO)

- ✅ Document the 9 cycles (this file).
- ⏳ **Apply 1 fix as a demo** (cycle 2, the smallest: 2 nodes,
  intra-touring-hooks/gateway). This demonstrates the pattern; the
  remaining 5 intra-crate cycles can be applied by following the same
  pattern in future waves.
- ⏳ **Document cycle 9 as multi-day follow-up** with the tool-update
  prerequisite.

### Future sessions (W3.2-W3.8)

- Apply the 5 remaining Class A cycles (one per future session, low risk).
- Apply the Class B cycle (cycle 5) — 1 session, 2-4 hours.
- Apply the Class C cycle 1 — 1-2 sessions, 1-2 days.
- Defer cycle 9 until the wiring tool's `pub use` filter is in place.

## Target

- **Cycles 2, 3, 4, 6, 7, 8** (6 cycles): apply Class A fix → 0 cycles in
  the medium-severity bucket.
- **Cycle 5** (1 cycle, high-severity, intra-crate): apply Class B fix →
  0 cycles in the high-severity bucket.
- **Cycle 1** (1 cycle, medium-severity, cross-crate): apply Class C fix
  (trait-object boundary) → 0 cross-crate cycles.
- **Cycle 9** (1 cycle, depth 391): deferred until wiring tool is fixed
  (the cycle is mostly `pub use` noise, not real architectural risk).

**End-state target after all W3 work**: 0 cycles of severity medium+;
cycle 9 deferred to a tooling fix.

## Cycle 2 demo (the smallest, lowest-risk fix)

**Before**:
- `touring-hooks/src/gateway/fast_path.rs` references
  `touring-hooks/src/gateway/pre_exec.rs` (or vice versa).
- The wiring tool sees the edge and reports a 2-node cycle.

**Fix pattern** (Class A, sub-pattern 1: extract shared type):
1. Identify the shared type or function that both modules import.
2. Create `touring-hooks/src/gateway/shared.rs` (or use an existing
   shared module).
3. Move the shared item there; have both `fast_path.rs` and `pre_exec.rs`
   import from `super::shared::*` (or `crate::gateway::shared::*`).
4. Verify: `cargo check --workspace` exit 0; `touring wiring cycles
   --min-depth 2` no longer reports the cycle.

**Effort**: 30-60 minutes.

---

_Wave W3.1 (inventory) delivered 2026-06-04. W3.2 (apply 1 fix) in progress.
Cycle 9 deferred to multi-session follow-up after wiring-tool fix._
