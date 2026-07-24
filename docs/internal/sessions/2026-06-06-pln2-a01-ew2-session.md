# Pln2 Execution — A01 crate extraction + E-W2 real Multi-SWE-bench

> **Date**: 2026-06-06 | **Plan**: `~/.claude/plans/recursive-cuddling-blossom.md` (Pln2 = (Pln1)²)
> **Mode**: TACO autonomous, plan-approved | **All claims verified in loco** (cargo/touring, never trust pipe-masked exit)

## Outcome

Pln2 executed across A01 (crate extraction, Gabriel chose "include now"), E-W2 (real
Multi-SWE-bench, Gabriel chose "multi-repo"), and the DoD-gap workstream. A01 + the
real-SWE-bench milestone delivered and validated; pre-existing cross-crate
--all-features debt documented (out of scope).

## A01 — touring-hooks crate extraction (FACT, validated)

`touring-hooks` was 172,448 LOC (36% of workspace). Extracted **two genuine leaf crates**
via the W8 façade pattern (`pub use … as <mod>`; hook_registry.rs untouched):

| New crate | Payload | Outbound deps | Validation |
|-----------|---------|---------------|------------|
| `touring-hooks-saga` | `saga/` (558 LOC) | dashmap, tokio, touring-rkyv, uuid (zero `crate::`) | `cargo check --workspace` exit 0; 13 tests `--features saga` |
| `touring-hooks-rl` | `agentic_rl.rs` (1,240 LOC) | touring-hooks-shared (pattern_bandit), touring-learning, tokio, serde | `cargo check --workspace` exit 0 (real, file-captured) |

- **−1,798 LOC** out of the monolith into acyclic leaves. `touring wiring cycles --min-depth 2` = **0** (no back-edge introduced).
- **CEG NOT extracted** — proven (grep) to be a tight SCC (mutual recursion with
  sandbox_executor/staging/drift_corrector/hook_runtime). Forcing it would cycle;
  recommended internal trait-seam instead (S-13, deferred P3).
- **Cycle averted in S-12**: `agentic_rl` had one hidden back-dep — `update_learning_phase_with_bus(&mut crate::HookRuntime)`. It was **dead (0 call-sites, grep-verified)**, so removed (REGRA #0: wire or remove); the pure `update_learning_phase` stays. Re-wiring instructions left in a comment.
- Façade preserves the external API: `touring_hooks::{saga,agentic_rl}::*` resolve unchanged for all 6 consumer crates.

### Gotchas captured (memory tier=semantic)
1. `grep 'crate::[a-z_]+'` misses CamelCase `crate::HookRuntime` — nearly missed the back-dep.
2. `uuid::Uuid` used inline path-qualified (no `use`) — missed in the first dep sweep (E0433).
3. `perfect-create-script` rejects short names (`gc.sh`) → use `perfect-create` for `.sh`.
4. **Pipe-masking**: `cargo … | tail` returns tail's exit (0), masking cargo failure → capture exit to a file.

## E-W2 — real Multi-SWE-bench (git mode)

Built `import_multi_swe.py` + extended the harness git-mode to apply `test_patch`.
**Milestone (FACT)**: MiniMax-M3 resolved a real GitHub issue end-to-end —
`tokio-rs/bytes#732` (sign-extension bug): SEARCH/REPLACE patch → `cargo test` →
`test_get_int` red→green + 9 pass_to_pass green, vgp_fp=0
(`runs/minimax-m3-bytes732.report.json`).

Multi-instance honest findings (`runs/minimax-m3-multiswe-rust.report.json`):
- 3/5 bytes instances (543/643/547) have **empty `f2p_tests`** upstream → unusable;
  importer now skips them loudly (no silent caps).
- bytes-721: MiniMax produced a wrong patch → harness scored unresolved, **vgp_fp=1.0**
  (the metric catching a real model limitation; no false pass).
- Solver flakiness: MiniMax-M3 reply format varies; `apply_search_replace` parsed
  bytes-732 on its dedicated run, missed it on the batch — documented follow-up
  (tolerant SR matching + retry).
- context7 `/swe-bench/swe-bench` confirmed the harness grading semantics match upstream
  (resolved = all FAIL_TO_PASS pass AND PASS_TO_PASS stable).

## DoD gaps closed
- **A.W1**: ARCHITECTURE.md:833 stale cell (45/429,255 → 38/476,728 via sync_metrics);
  `2026-06-04-touring-server-test-baseline.md` created.
- **A04**: README footer refreshed (workspace index figures + provenance note).
- **C.W3**: `fuzz/gc.sh` (corpus/artifact GC, REGRA #12) + health-delta gate row in `quality-gates.md`.
- **D.W2**: `#![warn(rustdoc::broken_intra_doc_links)]` on touring-hooks + touring-generator
  (warn not deny — deny(missing_docs) would break the build on legacy items).

## Deferred (documented, with rationale)
- **S-13 CEG trait-seam** (P3): the CEG is an SCC; an internal dependency-inversion seam
  is the right next step but is a separate refactor.
- **S-36 N01 satellite fold** (11 `cli_handlers_*.rs` into `cli/`): the core N01 monolith
  was already decomposed (A.W2); folding the 7,811-LOC satellites is organizational polish,
  high call-site churn, deferred to avoid risk late in the wave.
- **--all-features debt**: `cargo check --workspace --all-features` fails (3 pre-existing
  warnings-as-errors in `touring-offensive` z3_backend + `touring-intelligence` linucb.rs —
  unused mut/imports/const under non-default solver features). **Not A01-introduced**
  (those crates untouched); no impact on the default/deployed build (which passes).
  A separate cross-crate --all-features cleanup is the follow-up.

## Validation summary (in loco)
- `cargo check --workspace` (default) exit 0 (real, file-captured) after both extractions.
- `touring wiring cycles --min-depth 2` = 0.
- saga: 13 tests pass (`--features saga`); rl: compiles; saga/rl: 0 own clippy issues.
- bytes-732 real SWE-bench: resolved 1/1.
- index rebuilt (52,824 symbols); doctor 5/6 (only wiring_diagnostic warning).

## S-13 — CEG trait-seam (DONE 2026-06-06, follow-up wave)

The deferred S-13 was executed in-place (NOT a crate extraction — consistent with
the plan's "NÃO crate"). Ground truth corrected the plan's SPECULATION about which
edges matter:

- The dominant CEG outbound edge is **`offensive_integration` (116 refs)** — a
  re-export *facade* over the external `touring-offensive` crate, not deep
  coupling. Its treatment is **relocation**, not trait inversion.
- The genuine *mutual* module cycles are `sandbox_executor`, `hook_runtime`,
  `drift_corrector`, `workflow`, `staging` (root). `cli_handlers` and `runtime`
  are *one-way* edges (still extraction-blockers, but not cycles).
- `touring wiring cycles` = 0 because the SCC is intra-crate *module* cycles
  (Rust permits these); they only become forbidden Cargo cycles at extraction.

**Delivered (the cleanest, fully-verifiable increment):** the X9 LEARN
dependency-inversion seam.

- New module `crates/touring-hooks/src/gateway/deps.rs` — `trait LearnRuntime`
  (`learning_reward` / `gotcha_add` / `memory_store`) + the full CEG→parent
  **extraction map** (every remaining edge, ref count, cycle flag, and treatment)
  in its module docs. Created via `taco-forge perfect-create --content-from`
  (REGRA #14; VGP verified=7, atomic write, orphan_delta=0).
- `gateway/learn.rs` routes the RL/gotcha/memory side-effects via the trait
  methods on `rt` instead of `use crate::cli_handlers::{…}`. The cross-agent
  ledger block (HookRuntime *fields*, not cli_handlers) stays as-is. Public
  signatures unchanged → **zero caller blast**.
- `HookRuntime` implements `LearnRuntime` in `cli_handlers.rs`, delegating to the
  same `cli_*` free functions — no behavioural change, no logic duplication. The
  edge direction flips to `cli_handlers → gateway::deps` (parent → leaf, the
  correct direction for a future extraction).

**Result:** the `gateway → cli_handlers` edge (one-way, into a 7.8k-LOC module)
is **eliminated** (`grep crate::cli_handlers` over `gateway/` = 0). Residuals in
`learn.rs` (`drift_corrector` cycle + `runtime::HookRuntime` one-way, both used by
`reconcile_drift`) are documented in `deps.rs` as the next steps.

**Validation (real, file-captured exits):**
- `cargo check --workspace` exit 0.
- gateway lib tests **375 passed / 0 failed** (incl. 2 new `gateway::deps::tests`
  mock-runtime tests); `ceg_e2e` **118 passed / 0 failed**.
- `touring wiring cycles --min-depth 2` = 0 (no new crate cycle).
- Touched files clippy-clean (`cargo clippy -p touring-hooks --lib` exit 0). The
  `-D warnings` failure is pre-existing `touring-offensive` z3/cvc5 backend debt
  (cvc5_backend.rs:109/115, z3_backend.rs:82) — unrelated to S-13.

**Deploy:** zero behavioural change (the seam delegates to the same handlers), so
the running daemon behaves identically; the new binary lands on the next routine
`update-touring` rebuild.

**S-13 follow-up edge DONE (2026-06-06):** `offensive_integration` (116 refs, the
largest CEG→parent edge, zero non-gateway consumers) **relocated into the gateway**
(`gateway/offensive_integration.rs`) via physical move + `lib.rs` re-export
(`pub use crate::gateway::offensive_integration`) — public API + all 116 call sites
unchanged, `cargo check --workspace` exit 0, gateway lib **379 passed / 0 failed**
(incl. the 4 relocated `tests_p4`), `ceg_e2e` 118/0, clippy-clean, cycles 0. The
`crate::`→`super::` cosmetic flip is a trivial deferred follow-up.

**S-13 follow-up edge DONE (2026-06-06):** `action_signature` (19 gateway refs +
8 non-gateway consumers) **relocated to the `touring-hooks-shared` LEAF crate** —
VGP confirmed zero `crate::` deps (leaf-safe, no Cargo cycle). Move + `lib.rs`
re-export (`pub use touring_hooks_shared::action_signature`): public API + all call
sites unchanged. Breaks the edge for the gateway **and** for `offensive_integration`
(which also used it). `cargo check --workspace` exit 0; the 42 relocated tests run in
the leaf (`-p touring-hooks-shared` 42 passed/0 failed); clippy-clean; cycles 0.

**S-13 follow-up edge DONE (2026-06-06):** `sandbox_executor` (1361 LOC, the CEG
sandbox runner — the largest remaining edge and a **true bidirectional cycle**:
gateway ↔ `sandbox_executor::…crate::gateway::exec_pool`). VGP-decided to move the
**whole module into the gateway** (not the plan's enum-split) because it uses
`exec_pool` + `capability` and is used by `sandbox_stage`/`supervised` — it *is* the
CEG sandbox. This collapses the cycle to intra-gateway. Move + `lib.rs` re-export
(`pub use crate::gateway::sandbox_executor`): public API + all call sites unchanged
(13 gateway + 5 non-gateway, all `crate::` absolute). VGP confirmed **0 production
unwraps** → safe under the gateway `deny(clippy::unwrap_used)`. `cargo check
--workspace` exit 0; 34 tests run under `gateway::sandbox_executor`; `ceg_e2e` 118/0;
clippy-clean; cycles 0.

**S-13 follow-up edges DONE (2026-06-06) — 3 more in one pass:**
- `drift_corrector` (284 LOC, cycle) — **whole module moved into the gateway**
  (0 non-gateway consumers, uses only gateway types). Cycle collapses intra-gateway.
- `IsolationMode` (enum, cycle) — **relocated to `touring-hooks-shared::isolation_mode`**
  (leaf-safe, std-only); `hook_runtime` re-exports it, `txn.rs` names it from the leaf
  → **gateway → hook_runtime edge eliminated** (grep 0).
- `staging` (root, 12 KB) — VGP showed it is NOT a cycle (leaf-safe; the back-edge was a
  doc-link) and is the temporal-split *classification* (disjoint symbols from gateway's
  `staging` area/GC). **Moved into the gateway as `staging_classify`**; `lib.rs` aliases
  it to `crate::staging` → zero consumer edits.

All validated: `cargo check --workspace` exit 0; `gateway::drift_corrector` / `gateway::staging_classify`
(12) / shared `isolation_mode` (2) tests pass; `ceg_e2e` 118/0; clippy-clean; cycles 0.

**S-13 edge tally: 7 of 9 done** (cli_handlers · offensive_integration · action_signature ·
sandbox_executor · drift_corrector · IsolationMode · staging).

**S-13 follow-up edge DONE (2026-06-06) — the hardest one (`workflow`, true
bidirectional cycle):** VGP corrected the plan — the real back-edge coupling is
`StaticSeverity` (returned by `antipattern_severity` via a **relative**
`super::super::gateway::static_stage` path that `crate::` greps missed), NOT `Verdict`
(which lives only in `convert.rs`, unused by the gateway = parent→child, left as-is).
**2-part fix:** (1) relocated `StaticSeverity` (Clear/Warn/Block, leaf-safe) →
`shared::severity` (re-exported at `gateway::static_stage`, 0 of 7 consumers edited);
(2) relocated the leaf-safe core `{baseline, stage, antipattern}` → a nested
`shared::workflow` module (preserves the moved files' `crate::workflow::*` paths;
only edit = antipattern's `StaticSeverity` refs → `crate::severity`). `workflow/mod.rs`
re-exports the three for `advise`/`convert`/`cli_suggester`; `gateway/static_stage`
imports them from the leaf → **forward edge `gateway → workflow` eliminated** (grep 0).
Validated: `cargo check --workspace` exit 0; shared `workflow` 47/0, gateway
`static_stage` 10/0, `ceg_e2e` 118/0; clippy-clean (both crates); cycles 0.
**Lesson (gotcha):** `grep crate::` misses `super::`/`super::super::` relative paths —
always grep those too when mapping cross-module/extraction deps.

**S-13 final edge DONE (2026-06-06) — `runtime::HookRuntime` via `CegRuntime`:**
Built `CegRuntime` (supertrait of `LearnRuntime` adding `record_tool_outcome` [ledger],
`drift_cache_get`/`drift_cache_put` [result-cache], `contract_attestation` [a gateway
type — no external coupling]). `HookRuntime` implements it in `cli_handlers.rs`; the four
`learn.rs` X9 functions are now generic over `&mut impl CegRuntime` → **`learn.rs` names
zero `crate::runtime`/`HookRuntime` symbols** (grep 0). Validated: `cargo check
--workspace` exit 0; `gateway::learn` 17/0 + `gateway::deps` 2/0 + `ceg_e2e` 118/0;
clippy-clean; cycles 0.
**Honest scope:** the runtime edge had two faces — (1) the X9 LEARN **logic** (`learn.rs`)
is now runtime-free via `CegRuntime`; (2) the `pre_exec.rs` hook **driver** (`run_returning`/
`run` + `HookResponse`) is the legitimate runtime↔gateway **boundary** (`run_returning` is
called by 10+ parent hooks and returns the parent's `HookResponse` protocol). The boundary
is *not* a trait-abstraction target: at crate extraction it stays in the parent calling the
gateway's `run_gateway`. Genericizing the driver would contort the boundary without removing
`HookResponse` (a workspace-wide relocation, out of scope).

## S-13 COMPLETE — CEG extraction map: 9/9 addressed

| # | Edge | Type | Resolution |
|---|------|------|------------|
| 1 | `cli_handlers` (X9 LEARN) | one-way | `LearnRuntime` seam |
| 2 | `offensive_integration` (116) | one-way | moved into gateway |
| 3 | `action_signature` (19) | one-way | moved to leaf `shared` |
| 4 | `sandbox_executor` (1361 LOC) | **cycle** | moved into gateway |
| 5 | `drift_corrector` (284) | **cycle** | moved into gateway |
| 6 | `IsolationMode` | **cycle** | moved to leaf `shared::isolation_mode` |
| 7 | `staging` | one-way | moved into gateway as `staging_classify` |
| 8 | `workflow` | **cycle (bidirectional)** | `StaticSeverity`→`shared::severity` + `{baseline,stage,antipattern}`→`shared::workflow` |
| 9 | `runtime::HookRuntime` | one-way | `CegRuntime` (logic) + hook-driver boundary documented |

**All 4 true module cycles broken**; all one-way edges eliminated except the deliberate
hook-driver boundary (face 2 of #9). Across S-13: zero regression, zero behavioural change,
public API preserved at every step; `touring wiring cycles` = 0 throughout; `cargo check
--workspace` exit 0 after every edge. The CEG is now materially extraction-ready — the only
remaining touring-hooks coupling is the hook-driver boundary, which by design lives in the
parent at extraction. New leaf modules created in `touring-hooks-shared`: `action_signature`,
`isolation_mode`, `severity`, `workflow::{baseline,stage,antipattern}`. New gateway-owned
modules: `deps` (LearnRuntime + CegRuntime), `offensive_integration`, `sandbox_executor`,
`drift_corrector`, `staging_classify`.

**Deploy:** all S-13 changes are zero-behavioural (relocations + re-exports + trait
delegation) → the running daemon behaves identically; the new layout lands on the next
routine `update-touring` rebuild.
