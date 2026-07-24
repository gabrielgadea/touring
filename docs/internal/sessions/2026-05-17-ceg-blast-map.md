# CEG Pln2 — P0.1 Forensic Blast-Radius Map

> Generated: 2026-05-17 | Plan: `2026-05-17-ceg-pln2-plan.md` P0.1 | Read-only forensic measurement.
> Daemon health: `touring doctor -j` → all components `ok` (wiring_diagnostic `warning` only — non-blocking).

This document maps the blast radius, quality, and TDG grade of every existing file the Code Execution
Gateway (CEG) will touch. Every number is sourced from real `touring` CLI output (commands cited per
column). `quality_score` is the TDG `composite` field; `cognitive_score` comes from `ast meta`.
LOC is on-disk `wc -l` ground truth (note: the daemon `ast meta` `line_count` for `cli_suggester.rs`
was stale at 1163 — on-disk wc is authoritative at 2222).

## Files the CEG will modify

Commands per file:
`touring ast meta <f> --depth summary -j` (cognitive_score, fan_in_signal, fan_out_signal) ·
`touring ast blast <f>` (blast_radius) · `touring ast tdg <f>` (composite=quality_score, grade) ·
`wc -l <f>` (LOC).

| File | LOC | blast_radius | quality_score | cognitive_score | fan_in | fan_out | TDG | risk note |
|---|---|---|---|---|---|---|---|---|
| crates/touring-hooks/src/sandbox_executor.rs | 881 | 4 | 0.785 | 0.419 | 0.0 | 0.0 | C+ | Moderate. 4 consumers (incl. 2 test files). CEG reuses `execute_in_sandbox`/`SandboxConfig`/`tee_dir`/`cleanup_tee` — extend, don't rewrite. |
| crates/touring-hooks/src/cli_suggester.rs | 2222 | 2 | 0.786 | 0.624 | 0.0 | 0.0 | C+ | Large file (2222 LOC), low blast (2). High cognitive. CEG adds a CEG classifier arm — additive. |
| crates/touring-hooks/src/action_signature.rs | 743 | 0 | 0.812 | 0.790 | 0.0 | 0.0 | B | Index-stale (blast=0 but grep finds 3 consumers: lib.rs, post_tool_rl.rs, cli_suggester.rs). High cognitive 0.79. |
| crates/touring-hooks/src/pre_bash.rs | 701 | 4 | 0.798 | 0.328 | 0.0 | 0.0 | C+ | Low cognitive, on the daemon dispatch path. CEG hooks bash execution gating here. |
| crates/touring-hooks/src/pre_tool_use.rs | 746 | 1 | 0.812 | 0.970 | 0.0 | 0.0 | B | **Very high cognitive (0.97)** — edit cautiously. Single consumer (daemon dispatch). |
| crates/touring-hooks/src/pre_tool_validator.rs | 1116 | **37** | 0.775 | 0.820 | 0.0 | 0.0 | C+ | **HIGH BLAST (37)** — see Findings. Touched by nearly every cortex handler. CEG must extend, never break signature. |
| crates/touring-hooks/src/shared/ast_grep_signal.rs | 530 | 5 | 0.821 | 0.768 | 0.0 | 0.0 | B | `AstGrepRiskSignalLayer` reused by CEG analysis stage. 5 consumers. |
| crates/touring-hooks/src/shared/bash_ast_validator.rs | 561 | 2 | 0.795 | 0.720 | 0.0 | 0.0 | C+ | Source of `command_shape` + `validate_command`. Low blast — safe to extend. |
| crates/touring-hooks/src/shared/gate_metrics.rs | 2809 | **55** | 0.776 | 0.972 | 0.0 | 0.0 | C+ | **HIGHEST BLAST (55) + very high cognitive (0.97)** — see Findings. CEG only ADDS `record_*` counters (additive, low risk if append-only). |
| crates/touring-server/src/cli/gate_metrics.rs | 49 | 0 | 0.760 | 0.988 | 0.0 | 0.0 | C+ | Tiny CLI wrapper. blast=0. Very high cognitive (small dense file). |
| crates/touring-server/src/ingest/transcript_miner.rs | 1577 | 0 | 0.763 | 0.519 | 0.0 | 0.0 | C+ | Index-stale (blast=0 but grep finds 2 consumers: server/mod.rs, ingest/mod.rs). CEG may mine exec outcomes here. |
| crates/touring-server/src/server/tools_ctx_execute.rs | 55 | 0 | 0.859 | 1.000 | 0.0 | 0.0 | B+ | Tiny MCP shim. blast=0, cognitive 1.0 (dense small file). Best TDG of the set. |
| crates/touring-server/src/tools/ctx_execute_tools.rs | 280 | 1 | 0.848 | 0.552 | 0.0 | 0.0 | B | Core `ctx_execute_impl` + `detect_forbidden_calls`. Single consumer. CEG reuses these as the execution path. |

Notes:
- `fan_in_signal` / `fan_out_signal` are `0.0` for all files — `ast meta --depth summary` does not
  populate directional fan signals in this workspace (enrichment_source `on_disk_fallback` for most).
  Blast-radius consumer counts (from `ast blast`) are the load-bearing coupling metric here.
- `quality_score` column = TDG `composite`. No file in the set scores below 0.76.

## Greenfield (new) — locations the CEG creates

These do not exist yet; blast_radius = 0 by construction. Verified absent (`ast meta` on the file
paths returned `FILE_NOT_FOUND` / not in index).

| New location | Kind | blast | note |
|---|---|---|---|
| crates/touring-hooks/src/capability/ | new module dir | 0 | CEG capability model — fresh code, no dependents until wired. |
| crates/touring-hooks/src/gateway/ | new module dir | 0 | CEG gateway orchestration — fresh code. |
| crates/touring-hooks/benches/ceg_baseline.rs | new bench | 0 | Criterion baseline bench — standalone. |
| crates/touring-hooks/tests/ceg_e2e.rs | new test file | 0 | E2E test — standalone, no dependents. |
| crates/touring-server/src/cli/exec.rs | new CLI subcommand | 0 | `touring exec` CLI handler — wired into `cli/mod.rs` dispatch at creation. |

## Findings — Risk R8 inputs (blast_radius > 10 or TDG D/F)

**No file graded D or F.** Lowest grade in the set is C+ (composite ≥ 0.760). This is a positive
signal for R8 — there is no pre-existing quality debt cliff.

**Two files exceed blast_radius > 10 — R8 high-coupling risk:**

1. **`crates/touring-hooks/src/shared/gate_metrics.rs` — blast_radius 55 (HIGHEST).**
   55 consumers span touring-resource-monitor, touring-cognitive, touring-server, touring-ast, and
   nearly all of touring-hooks plus 8 integration-test files. Cognitive score 0.972 (very high).
   *R8 mitigation:* the CEG only needs to ADD new `record_*` counters. The `GateMetrics` struct
   already has ~150 `record_*` methods — appending is additive and does not alter any existing
   consumer. **Hard constraint: do not rename or change the signature of any existing `record_*`
   method or `GateMetricsSnapshot` field.** New counters only.

2. **`crates/touring-hooks/src/pre_tool_validator.rs` — blast_radius 37.**
   37 consumers, dominated by ~25 `touring-cortex` handler files plus the daemon dispatch path.
   Cognitive score 0.820. `PreToolValidator::validate` / `validate_params` are the load-bearing
   public API. *R8 mitigation:* CEG must extend validation via a new code path or additional
   validator, NOT by changing `PreToolValidator`'s public method signatures. Any signature change
   ripples to all 25 cortex handlers.

**Secondary R8 watch — high cognitive complexity (not blast, but edit-fragility):**

- `pre_tool_use.rs` (cognitive 0.970), `gate_metrics.rs` (0.972), `cli/gate_metrics.rs` (0.988),
  `tools_ctx_execute.rs` (1.000) all carry cognitive_score ≥ 0.97. `tdg.action` for `pre_tool_use.rs`
  is "Edit OK, considerar refactor leve"; for `gate_metrics.rs` it is "Edit cauteloso, planejar
  mitigação". The CEG should prefer additive edits and new modules over in-place restructuring in
  these four files.

**Index staleness note (Cadeia 7):** `action_signature.rs` and `transcript_miner.rs` report
blast_radius 0 from `ast blast`, but `grep -rln` confirms real consumers (3 and 2 respectively).
The symbol index is stale for these newer files. The blast table above reports the daemon value;
the true coupling is non-zero. This does not change R8 (both stay well under 10), but the CEG
should treat `action_signature.rs` as having ~3 dependents when editing.
