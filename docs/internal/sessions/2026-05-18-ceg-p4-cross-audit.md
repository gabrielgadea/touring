# CEG Phase P4 — cross-audit report (purpose-fidelity)

> `/TACO-cross-audit` over the CEG **Phase P4** body (P4.1–P4.6), 2026-05-18.
> Target tree: `gateway/{exec_pool,dry_run_cache,fast_path,supervised}.rs`,
> `capability/{limits,enforce_linux}.rs`, `sandbox_executor.rs` (P4.1 additions),
> `benches/{dry_run_cache,fast_path}.rs`, and the `gateway/mod.rs` / `pre_exec.rs`
> wiring. The question is not "does it crash?" — it is **"does the code fulfill
> its documented purpose?"**, proven by execution.

## Phase 1 — MAP

2 334 LOC across 8 P4 files. The X5/X8 sandbox surface is the audited flow:
every async subprocess spawn routes through `sandbox_executor::spawn_and_capture`,
and `apply_resource_caps_to` is the resource-cap chokepoint that precedes it.
Daemon was degraded — discovery ran in `cargo` / `grep` fallback (authoritative).

## Phase 2 — PURPOSE AUDIT

Every P4 module carries a `//!` header stating its purpose, and behaviour was
faithful to it **except two functions**:

| Symbol | Documented purpose | Real behaviour |
|--------|--------------------|----------------|
| `cgroup_v2_status` (P4.3) | "probe the host … reports that precondition honestly" | Probe is correct — but **never called** |
| `apply_landlock` (P4.2 / P2.4-A) | "the advisory availability report (used for the X9 LEARN record)" | Report is correct — but **never consumed** |

A report function whose report nobody reads does not fulfil its purpose: the
information never reaches any decision or any operator. A unit test cannot catch
this — `cgroup_v2_status_returns_a_definite_status` and
`apply_landlock_reports_kernel_enforcement` both passed, because they exercise
the function *in isolation*. The cross-audit asks the integration question.

## Phase 3 — DEBT SCAN

**Zero** debt markers across 2 334 LOC of P4 code — no `TODO` / `FIXME` /
`HACK` / `XXX` / `unimplemented!` / `todo!()` / `allow(dead_code)` /
`allow(unused)`.

## Phase 4 — HARMONY CHECK

Consumer-grep over every headline P4 symbol. All wired **except** the two
Phase-2 findings: `cgroup_v2_status`, `apply_landlock` (and `apply_landlock`'s
return type `EnforcementReport`) had **zero production consumers** workspace-wide
— referenced only by their own definition, the `capability/mod.rs` re-export,
and their own tests. Confirmed orphan.

## Phase 5 — FIX & POTENTIALIZE (REGRA #0)

The fix **wires** the orphans into a real flow — it does not delete them. The
two advisories are exactly the half-built feature REGRA #0 says to *complete*.

A consolidated advisory was added to `capability/limits.rs`:

| New symbol | Role |
|------------|------|
| `sandbox_enforcement_advisory()` | Consolidates the three protection layers — rlimit (always-on), cgroup v2 (`cgroup_v2_status`), landlock (`apply_landlock`) — into one human-readable line |
| `log_enforcement_advisory_once()` | Emits the advisory exactly once per process (`std::sync::Once` + `tracing::info!` `target: "ceg::enforcement"`) |

`apply_resource_caps_to` — its two `#[cfg]` definitions (Linux / non-Linux)
were merged into one function that calls `log_enforcement_advisory_once()`
**first**, then does the platform-specific rlimit wiring. Because every X5
dry-run and X8 SUPERVISED-EXEC spawn routes through `apply_resource_caps_to`,
the advisory is guaranteed to be emitted once, before any sandboxed subprocess,
on every platform.

Result: `cgroup_v2_status`, `apply_landlock` and `EnforcementReport` now have a
production consumer; the advisory their docs promised is genuinely produced.
`sandbox_enforcement_advisory` is re-exported from `capability/mod.rs`.

## Phase 6 — E2E PROOF (executed)

| Gate | Result |
|------|--------|
| `cargo check -p touring-hooks --tests --benches` | exit 0 |
| `cargo test capability` | **84 / 84 PASS** (P4.3 limits + 2 new advisory tests) |
| `cargo test gateway` | **199 / 199 PASS** (P4.2/4.4/4.5/4.6 + X5 + pre_exec) |
| `cargo test sandbox_executor` | **32 / 32 PASS** (P4.1 + P4.3 E2E) |
| `cargo clippy` — `limits.rs` | 0 warnings / 0 errors |
| `cargo check --workspace` | exit 0 — zero regression |

**315 P4 tests pass.** Two tests were added for the wiring:
`sandbox_enforcement_advisory_reports_all_three_layers` (the advisory names
rlimit + cgroup + landlock) and `log_enforcement_advisory_once_is_idempotent`
(the `Once` gate makes repeated calls safe).

## Phase 7 — finding outside the audit tree

`cargo clippy -p touring-hooks --lib --tests` surfaces ~4 clippy errors in
**integration test files unrelated to CEG** — `tests/post_wave_orchestration_e2e.rs`
(manual `str::repeat`), `tests/cli_handlers_e2e.rs`,
`tests/compression_profiles_e2e.rs`, `tests/post_edit_rule_engine_e2e.rs`
(unnecessary `collect()`). They predate the entire P4 phase and are not part of
the audited CEG tree — a change to the gateway library cannot introduce a clippy
error in an independently-compiled `tests/*.rs` file. Reported here for a
dedicated touring-hooks test-clippy cleanup; **not fixed in this P4-scoped audit**
(fixing 4 unrelated test files is scope expansion beyond "audit CEG P4"). The
P4 code itself is clippy-clean.

## Verdict

CEG Phase P4 **fulfils its documented purpose**, proven in practice (315 tests).
The cross-audit found and closed one real purpose-fidelity defect — two orphan
advisory functions — by completing the advisory they were built for and wiring
it into the live X5/X8 path. Zero debt, zero regression.

---
_Cross-audit complete. Phase 5 changed `capability/limits.rs` (+`sandbox_enforcement_advisory`,
+`log_enforcement_advisory_once`, merged `apply_resource_caps_to`, +2 tests) and
`capability/mod.rs` (+re-export). Lessons persisted: `ceg-p4-cross-audit-2026-05-18`,
`gotcha:advisory-functions-orphan-prone`._
