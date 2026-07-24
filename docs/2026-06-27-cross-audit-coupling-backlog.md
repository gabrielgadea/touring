# Cross-Audit — Coupling Backlog (purpose-fidelity, executed evidence)

> **Date**: 2026-06-27 | **Skill**: TACO-cross-audit | **Scope**: all 17 implemented items
> (C1–C14 + MT-1 `touring_audit`, Gap2 annotations, Gap4 benchmark)
> **Question answered**: *does each item fulfil its documented purpose, proven in practice?*

## Verdict

**17/17 fulfil their documented purpose, proven by executed evidence.** The audit found
**4 pure engines that were orphaned of a production consumer** (C11, C12, C13, C14 — green
unit tests but no call-site outside tests) and **potentialized all four** (REGRA #0): C13
wired intra-flow, C11/C12/C14 exposed as real CLI surfaces the orchestrator invokes. Every
touched file clears the 50-dim 6 P0 BLOCK gate at Diamond.

## Phase 1–4 — MAP / PURPOSE / DEBT / HARMONY (read-only)

| Finding | Evidence | Status |
|---|---|---|
| Baseline compiles | `cargo check --workspace` → **exit 0** (29s) | ✅ |
| `run_audit` homonímia (MT-1 vs C1) | `tools_workflow.rs:200 (path,layers)` vs `cli/wiring.rs:284 (use_tree)` — distinct sig/module/purpose | ✅ benign |
| **C11/C12/C13/C14 orphaned of production** | grep: only `pub use` re-exports + test call-sites; zero production consumers | ⚠️ → fixed |
| Wired engines | `summarize_output`→`sandbox_executor.rs:401`; `detect_silent_failure`→`ceg_adapter.rs:238` | ✅ |
| MT-1 registered | CURATED_TOOLS:100 + router merge:541 + mod:584 | ✅ |

## Phase 5 — FIX & POTENTIALIZE (REGRA #0: orphan → wire, never delete)

| Item | Potentialization | Footprint |
|---|---|---|
| **C13** `decide_checkpoint` | wired in `ceg_adapter` — the X6 `gate_report.gated` capabilities feed `decide_checkpoint`; `needs_compensation` emits a structured tracing signal the X8 supervised path reads. Fail-open, intra-crate. | +13 lines |
| **C11** `verify_conservation` | `touring budget-verify --root … --node …` (the orchestrator supplies the root from the Workflow-tool; `Task` has no internal root budget — forcing one would be fake). | new CLI surface |
| **C12** `plan_tool_chain` | `touring plan-chain --edge from,to,cost --start --goal` (MCTS geodesic over the tool graph). | new CLI surface |
| **C14** `consistency_gate` | `touring consistency --a-nodes/--a-edges --b-nodes/--b-edges` (GED+cosine merge gate). | new CLI surface |

All three surfaces share `crates/touring-server/src/cli/reason_tools.rs` (1 new module,
3 handlers, registered in `command_table.rs` + `cli/mod.rs`).

## Phase 6 — E2E PROOF (executed, output shown)

```
C11 over-commit : {"conserved":false,"nodes":2,"violations":[{"allocated":6,"dimension":"subtasks","root_budget":5}]}  exit 0
C11 conserved   : conserved: Σ 2 node budgets ≤ root on all 6 dimensions
C12 geodesic    : {"chain":[1,2,3,99],"reached_goal":true,"total_cost":3}                                              exit 0
C14 divergent   : {"consistent":false,"cosine_sim":1.0,"distance":1.0,"ged":8}                                         exit 0
C14 identical   : {"consistent":true,"cosine_sim":1.0,"distance":0.0,"ged":0}
C7  route       : {"level":"L5","routing_mode":"FullTaco","max_parallelism":8,"composite":0.795}                       exit 0
C1  --brief     : 485 bytes (was 43.7 MB raw) — "_elided_array_len":4823 (count preserved, array elided)
C3  search-tools: top hit "touring_audit (MCP)" score 22.18 for a security intent
Gap4 benchmark  : 16 passed (pytest)
```

| Suite | Result |
|---|---|
| `cargo test` ceg + intelligence + server-reasoning + offensive | **0 failed** (ceg 528, …) |
| `cargo test` touring-server + hook-runtime | **1834 passed, 0 failed** |
| MT-1 `touring_audit` (9 tests) | ok — incl. `vuln_layer_flags_sql_injection_as_block`, `run_audit_vuln_only_blocks_on_xss` |
| C11/C12/C14 `reason_tools` (6 tests) | ok — incl. `plan_chain_reaches_goal`, `consistency_identical_…_divergent` |
| C2 curation | `curated_names_are_unique` + `curated_surface_is_lean` ok |
| `cargo clippy` touched crates | **exit 0, 0 warnings** |
| 50-dim 6 P0 BLOCK | `reason_tools.rs` Diamond 0.976, `ceg_adapter.rs` Diamond 1.0 — 0 blockers |

## Honest residuals (not masked)

- **C13** computes the checkpoint decision in production now (no longer orphan) and emits an
  observable signal; the *effective* `DistributedSagaCoordinator::compensate` call is the
  cross-crate X8 follow-up (touring-ceg → touring-hooks-saga). The decision is live; the
  rollback wiring is the deliberate next step.
- **Workspace pre-existing debt**: `wiring audit` reports 4823 orphan diagnostics + 1214
  low-score modules across the whole workspace (mostly `.cargo/registry/` noise). This is
  historic debt unrelated to the 17 audited items — a separate audit surface.

## Effectivation

`update-touring` (build release --workspace + daemon restart) — see session report.
The 3 new commands (`budget-verify`/`plan-chain`/`consistency`) ship in the canonical binary.
