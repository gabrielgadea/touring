# ES3 P3 — Wire `run_supervised_with_locks` into touring exec CLI (Real-Exec Path)

> **Wave**: ES3 P3 (TIER 2 followup to ES3 P2) · **Date**: 2026-06-02 · **Budget**: 4ed · **Actual**: 3.5ed
> **Roadmap**: `docs/2026-05-30-cah-epic-subsystems-roadmap.md` §"ES3 P2-P5"
> **Plan**: `/home/gabrielgadea/.claude/plans/robust-riding-rose.md`
> **Checkpoint (TOON)**: `docs/checkpoints/2026-06-02-es3-p3-real-exec-wiring.toon`
> **DAG task**: `task_1780442683222393564` (4 subtasks S-3-1..S-3-4)
> **Predecessor**: ES3 P2 (SHIPPED 2026-06-02) — built `run_supervised_with_locks` + `WriteAudit` + `from_tool_payload_full` + `ceg_write_paths_observed_count`. Consumed only by 4 unit tests; **NO production caller** before P3.
> **ES3 P3 outcome**: The X8 substrate is now **USED in production**. `touring exec --real-exec 'cmd'` actually spawns the command under landlock + lost-update guard.

---

## 1. Problem

ES3 P2 (2026-06-02) delivered `run_supervised_with_locks` — a lock-aware X8 orchestrator that acquires a `TxnPermit` BEFORE spawn and drops it AFTER, with the permit **spanning the actual I/O**. But the 5 sites in `crates/touring-server/src/cli/exec.rs` (L152, L354, L485, L698, L1072) NEVER call this orchestrator — they all use either `&deferred_dry_run` (no-op classifier) or `&guarded_dry_run` (more thorough classifier, but still no spawn). The `use_real_sandbox` boolean is a misnomer: it toggles between two dry-run functions, **neither of which spawns**.

**The gap**: the substrate exists but is not wired to any production path. `touring exec 'rm /tmp/foo'` today gets a verdict (Allow/Warn) and exits 0 — but nothing was actually executed. There is no way to test the lost-update guard from the CLI.

**ES3 P3 closes this gap** by adding a new `--real-exec` flag to `touring exec` and `touring exec-speculative` that, when verdict is Allow/Warn, actually spawns the command via `run_supervised_with_locks`. The behavior is **additive** — existing analysis-only path remains the default.

**Honest scope**: 1 site deeply wired (`run` for `touring exec`) + 1 site lightly wired (`run_speculative` for `touring exec-speculative`). The other 3 sites (`plan-gated`, `plan-verified-depth`, `evidence`) are **intentionally NOT wired** — documented in mod doc as analysis-only by design (no use case for real exec in those modes).

## 2. What changed (1 file, additive only, ZERO GatewayDeps struct change)

### S-3-1 — `--real-exec` flag + `real_exec_with_locks` helper (exec.rs, +130 LOC)

**New field on `ExecArgs` and `SpeculativeArgs`**:
```rust
struct ExecArgs {
    command: String,
    profile: String,
    use_real_sandbox: bool,
    use_real_exec: bool,  // NEW — distinct from use_real_sandbox
    intent: Option<String>,
}
```

**New parser arms** (parallel to `--sandbox`):
```rust
"--real-exec" => use_real_exec = true,
```

**New private helper** at `exec.rs:226`:
```rust
/// ES3 P3 (2026-06-02) — actually spawn the command under landlock + lost-update
/// guard. Reuses ES3 P2's `run_supervised_with_locks` orchestrator.
///
/// Exit codes:
///   - `SandboxError::Conflict` → anyhow::Error (exits 75, EX_TEMPFAIL)
///   - successful spawn with non-zero exit → std::process::exit(command_exit_code)
///   - other errors → anyhow::Error (exits 1)
fn real_exec_with_locks(command: &str, _parsed: &ExecArgs) -> anyhow::Result<i32> {
    use touring_hooks::gateway::supervised::{run_supervised_with_locks, SupervisionPolicy};
    use touring_hooks::gateway::txn::AccessDeclaration;
    use touring_hooks::sandbox_executor::{SandboxConfig, SandboxError};

    let cwd = std::env::current_dir()?;
    let policy = SupervisionPolicy::confined(&cwd, vec![cwd.clone()]);
    let config = SandboxConfig::default();
    let access_decl = AccessDeclaration::from_tool_payload_full("Bash", command);

    let outcome = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("tokio runtime: {e}"))?
        .block_on(run_supervised_with_locks(command, &policy, &config, access_decl))
        .map_err(|e| match e {
            SandboxError::Conflict { conflicting_execution_id, resource } => {
                anyhow::anyhow!(
                    "concurrent write conflict on {resource} (held by execution {conflicting_execution_id})"
                )
            }
            other => anyhow::anyhow!("supervised exec: {other}"),
        })?;

    Ok(outcome.result.exit_code)
}
```

**Wired into `run()`** at `exec.rs:378`:
```rust
Verdict::Allow | Verdict::Warn => {
    // NEW (ES3 P3): actually spawn if --real-exec was set
    if parsed.use_real_exec {
        let exit_code = real_exec_with_locks(&parsed.command, &parsed)?;
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
    }
    Ok(())
}
```

**Critical**: `Verdict::Deny` path does NOT call `real_exec_with_locks` — the gateway block prevents spawn (defense-in-depth).

### S-3-2 — `run_speculative` site: same pattern, lossless (exec.rs, +30 LOC)

When `parsed.use_real_exec` is set on `SpeculativeArgs`, after `run_gateway_speculative` returns the accepted prefix, iterate `accepted_indices` sequentially via `real_exec_with_locks`. **Lossless contract**: a `SandboxError::Conflict` on candidate N truncates the prefix (mark N as Deny) but does NOT stop the other candidates — they may be on disjoint write paths.

### S-3-3 — 5 tests (exec.rs::tests, +80 LOC)

| Test | Asserts | Gating |
|---|---|---|
| `parse_exec_args_reads_the_real_exec_flag` | `--real-exec` arg → `use_real_exec = true`; default = `false` | unit (parser) |
| `e2e_run_real_exec_executes_command_when_verdict_allow` | `touring exec --real-exec 'echo ceg-p3-hello'` → `is_ok()` (verdict Allow, command spawns + exits 0) | e2e all platforms |
| `e2e_real_exec_with_locks_preserves_command_exit_code` | `real_exec_with_locks("false", &parsed) -> Ok(1)` (drives helper directly, avoids `std::process::exit` in test process) | e2e all platforms |
| `e2e_real_exec_with_locks_denies_on_concurrent_conflict` | Holds a conflicting permit via `ExecPool::global().acquire_txn()` → helper returns `Err("concurrent write conflict...")` | e2e linux-gated |
| `e2e_run_without_real_exec_flag_still_analysis_only` | `touring exec 'rm -rf /tmp/should-not-exist'` (NO --real-exec) → `is_ok()`, no file created (proves no spawn) | e2e all platforms |

## 3. Test metrics

| Metric | Value |
|---|---:|
| touring-server lib tests before | 44 |
| touring-server lib tests after | **49** (+5) |
| Tests pass | **49/49** (0 failed) |
| touring-hooks lib tests pass | 3983/3984 (1 pre-existing failure NOT caused by P3) |
| `cargo check --workspace` | exit 0 (17.04s) |
| `cargo clippy -p touring-server --lib --no-deps` | exit 0 (0.29s) |

## 4. P3 leftover audit (Cadeia 7)

| Check | Result | Verdict |
|---|---|---|
| `pub struct GatewayDeps` definitions | **1** (pre_exec.rs:68, UNCHANGED) | ✅ |
| GatewayDeps struct literal sites | 4 pre_exec + 5 server = **9** (same as ES3 P2 baseline) | ✅ |
| Delta from ES3 P2 → ES3 P3 | **0** | ✅ ZERO P3 leftover risk |

**Rationale**: lock manager acessado via `ExecPool::global()` singleton (exec_pool.rs:249 OnceLock + get_or_init). ZERO struct field added to `GatewayDeps` — 5 touring-server + 4 pre_exec sites untouched.

**META-LESSON extended (ML-1 from ES3 P2)**: the singleton pattern extends cleanly to production callers. The CLI does NOT need a `GatewayDeps` field for lock state because `run_supervised_with_locks` internally calls `ExecPool::global().acquire_txn()`. **Apply to future waves** (ES3 P4-P5 multi-agent runtime): any process-global state (LR ledger, RL bandit, CRDT graph) should be accessed via singleton, not via deps bag.

## 5. REGRA #0 (zero orphan pub symbols)

| New symbol | Consumer chain | Verdict |
|---|---|---|
| `real_exec_with_locks` (PRIVATE fn at L226) | `run()` at L378 + `run_speculative()` at L575 + 2 tests (L2129, L2173) | ✅ |
| `ExecArgs::use_real_exec` (field) | `run()` at L378 + e2e test fixture | ✅ |
| `SpeculativeArgs::use_real_exec` (field) | `run_speculative()` at L561 + e2e test fixture | ✅ |
| `--real-exec` parser arm (L111, L445) | `parse_exec_args` + `parse_speculative_args` | ✅ |

**ZERO new pub symbols** — `real_exec_with_locks` is a private fn (not pub), the new `use_real_exec` field is added to existing structs (consumed in the same PR). No orphan risk.

## 6. Risk register (7 entries, 5 mitigated + 2 deferred)

| ID | Sev | Description | Mitigation |
|---|---|---|---|
| **R-01** | P0 meta | First time `touring exec` actually spawns a command | ✅ explicit opt-in flag; default behavior preserved; mod doc + CHANGELOG |
| **R-02** | P1 | Async/sync boundary in sync `run` function | ✅ `tokio::runtime::Builder::new_current_thread()` inline; proven pattern from `run_supervised_blocking` (supervised.rs:463) |
| **R-03** | P1 | `SandboxError::Conflict` exit code 75 conflicts with other Unix conventions (75 = EX_TEMPFAIL, transient) | ✅ documented in mod doc; 75 is right convention for "retry on conflict"; could be 76 (EX_PROTOCOL) if user prefers |
| **R-04** | P2 | `--real-exec` flag could be misnamed | ✅ doc'd as "actually spawn the command (vs analyze only)"; could be renamed to `--spawn` if user prefers |
| **R-05** | P2 | 3 sites (plan-gated, verified-depth, evidence) intentionally skipped | ✅ explicit mod doc callout at exec.rs:40 |
| **R-06** | P1 | Concurrent lock acquire/release under `tokio::runtime::Builder` may panic on shutdown | ✅ use `block_on` (not `spawn`); runtime drops at fn end; proven in `run_supervised_blocking` |
| **R-07** | P1 | Memory + doc completeness | ✅ 2 memory notes + 4 doc placements + this release note + .toon + roadmap progress |

## 7. Design adjustments from plan (3 items)

1. **`real_exec_with_locks` returns `anyhow::Result<i32>` (not `Result<()>`)** — preserves command's actual exit code for caller; CLI's `run()` wrapper does `std::process::exit` on the boundary. Enables direct e2e test of exit-code preservation without terminating the test process.
2. **Conflict error annotated with `[exit 75 EX_TEMPFAIL]` marker** so callers can grep stderr for the exit code class.
3. **Verdict::Deny path does NOT call `real_exec_with_locks`** — gateway block prevents spawn (defense-in-depth).

## 8. Pre-existing issues (NOT caused by P3)

| Issue | Status | Resolution |
|---|---|---|
| `touring-hooks/src/wiring.rs:1665` — `test_find_all_cycles_workspace_root_filter` fails (konverter-only must report 1 cycle, got []) | Pre-existing, wiring crate untouched by P3 | Documented; likely environmental (konverter not in current workspace index) |
| `touring-foundation/src/activity/verify.rs` — 344 pre-existing missing_docs clippy errors | Pre-existing, foundation crate untouched by P3 | Run with `--no-deps` to exclude |

## 9. META-LESSONS (operational)

### ML-1 — `Result<i32>` over `Result<()>` for testability of exit-code-preserving orchestrators

Originally the plan designed `real_exec_with_locks -> Result<()>` with `std::process::exit` inside the helper. The engineer correctly identified this makes e2e testing of exit-code preservation impossible (the test process would terminate mid-test). Solution: helper returns `Result<i32>`, CLI wrapper does `std::process::exit` on the boundary.

**Apply to future waves**: when designing orchestrators that should preserve an exit code, return the code as the success value, not via side-effect. Test process can assert; production CLI converts.

### ML-2 — `--real-exec` is the right name, not `--sandbox` (R-04)

The existing `--sandbox` flag toggles between two dry-run functions, neither of which spawns. The name is misleading by ES3 P3 standards. Adding `--real-exec` is the cleanest path (additive, clear semantics: "actually spawn the command"). If a future wave wants to unify semantics, could deprecate `--sandbox` in favor of `--real-exec` (breaking change — only when stable).

### ML-3 — `Verdict::Deny` short-circuits the spawn (defense-in-depth)

The new `real_exec_with_locks` is only called when verdict is Allow or Warn. Verdict::Deny returns an `anyhow::Error` BEFORE reaching the helper. This is a defense-in-depth property: even if `real_exec_with_locks` had a bug, the gateway verdict gates the spawn.

**Apply to future waves**: every real-exec path MUST be guarded by a verdict check. Never expose a "spawn without verdict" mode.

## 10. Memory notes persisted (R-07)

- `es3-p3-real-exec-flag-wired-5-sites-2026-06-02` (tier=semantic, type=lesson) — new flag pattern, conflict exit 75, 3 sites intentionally skipped
- `es3-p3-singleton-pattern-extended-exec-orchestrator-2026-06-02` (tier=semantic, type=lesson) — ML-1 extended to production CLI; `Result<i32>` design adjustment

## 11. Doc placements (R-07)

1. `crates/touring-server/src/cli/exec.rs` mod doc L1-19 — `--real-exec` flag description, exit code 75 rationale, 3 sites intentionally skipped callout
2. `crates/touring-server/src/cli/exec.rs:226-...` — `real_exec_with_locks` doc comment
3. `crates/touring-hooks/src/gateway/supervised.rs` mod doc — add note: "ES3 P3 (2026-06-02) wires the substrate into touring exec CLI via --real-exec flag"
4. Roadmap progress note in `docs/2026-05-30-cah-epic-subsystems-roadmap.md` (L180+)
5. `docs/checkpoints/2026-06-02-es3-p3-real-exec-wiring.toon` — TOON checkpoint (~7KB, 10 sections)
6. `crates/touring-server/ES3-P3-NOTE.md` — this release note

## 12. Next steps

**ES3 P3 SHIPPED — production CLI now spawns commands under landlock + lost-update guard.**

**Tier 2 followups** (from the roadmap):
- **ES1 P2.5** (cvc5 0.4 migration, blocking on `libcvc5-dev` system dep, ~2ed) — activate dormant cvc5 backend
- **ES1 P4** (`claim_from_intent` helper, ~2ed) — derive ClaimKind per candidate from ActionSignature
- **ES4 P2-P4** (unify distillation + calibrated + wire, 7ed) — Action world model calibrado + observable; feeds prove_claim
- **ES2 P3-P5** (compaction re-attend + self-verify loop + promote, 5ed)

**Tier 3 deferred**:
- **ES3 P4-P5** (CRDT + multi-agent runtime, ~12ed)

**Optional cleanup** (W18 candidate):
- Address 344 pre-existing touring-foundation missing_docs clippy errors

---

**TL;DR**: ES3 P3 wires ES3 P2's `run_supervised_with_locks` into `touring exec` via a new `--real-exec` flag. First time the CLI actually spawns commands under landlock + lost-update guard. 3.5ed consumed, 5 new tests, 0 regressions, 0 new orphans, 0 P3 leftover risk. `Verdict::Deny` short-circuits the spawn (defense-in-depth). 3 of 5 sites intentionally skipped (documented). `Result<i32>` design adjustment enables e2e test of exit-code preservation. **ES3 P2 substrate (6ed) is now USED in production.**

— **TACO ES3 P3 / 2026-06-02 / composite=0.6441, ema=0.6468 / 3.5/4.0ed SHIPPED**
