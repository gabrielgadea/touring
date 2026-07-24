# CEG Phase P3 — TACO Cross-Audit (purpose-fidelity)

> Audit date: 2026-05-18. Target: the **CEG `X0..X9` gateway** — Phase P3 of CEG
> Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`). 12 files in
> `crates/touring-hooks/src/gateway/` + `crates/touring-server/src/cli/exec.rs`
> + the touring-hooks `main.rs` dispatch arm — **4 813 LOC**.
>
> The question this audit answers is not "does it crash?" — that is a unit
> test's job. It answers **"does the code fulfill its documented purpose?"**,
> proving it with executed evidence.

## Verdict

**Phase P3 is sound — and the audit found and fixed one real integration bug**
that every unit test had missed. The bug was caught only by Phase 6's executed
proof (running the actual binary), exactly as a cross-audit is designed to do.

## FASE 1 — MAP

| Metric | Value |
|--------|-------|
| Gateway files | 12 (`mod.rs` + 11 stage / support modules) |
| `cli/exec.rs` | 318 → 339 LOC (post-fix) |
| Total target LOC | 4 813 |
| Module declarations / re-exports | 11 / 11 (`gateway/mod.rs`) |
| FASE 0 health | `cargo check --workspace` exit 0 ✓ |

The gateway is a **linear pipeline** — `X0 capture → X1 classify → X2 static →
X3 vgp → X4 predict → X5 sandbox → X6 gate → X7 decision`, plus `error` (leaf)
and `pre_exec` (the driver, importing the earlier modules). Acyclic by
construction.

## FASE 2 — PURPOSE AUDIT

Every documented purpose claim was verified against real behaviour:

| Claim | Verification |
|-------|--------------|
| "X3 and X5 cannot be bypassed — compile error" | The 4 `compile_fail` doctests + the linear `advance_chain!` — every path traverses X3 / X5. ✓ |
| "the X0..X7 pipeline cannot fail once entered" | No X1–X7 transition returns `Result`; closures fold failure into evidence. `GatewayError` covers only the entry layer. ✓ |
| "the hook keeps the exit-0 invariant" | `run` → `run_returning().emit()`; `emit()` ends every arm with `process::exit(0)`. Proven executed in FASE 6. ✓ |
| "deny-wins — a high composite never overrides a hard block" | `from_evidence` checks `static_block \|\| gate_block` before any Allow. ✓ |
| "guarded_dry_run refuses the destructive catalogue, never spawns it" | `guarded_dry_run` runs `validate_command` before any spawn; the refused path returns the `<X5-refused>` marker. ✓ |

## FASE 3 — DEBT SCAN

`scan_debt.py` on the gateway tree → **"no debt markers found — the tree is
clean."** Zero `TODO` / `FIXME` / `unimplemented!` / `todo!()` /
`allow(dead_code)` / `allow(unused)`.

One `.expect()` in production code — `pre_exec.rs:187`, in `run_gateway`:
`decided.evidence().decision.clone().expect("decide() attaches a GateDecision —
P3.6 typestate invariant")`. **Audited and classified as correct, not debt**:
`run_gateway` calls `.decide()` two lines above, and P3.6's `decide()`
unconditionally attaches the decision — the invariant provably holds in this
context, and `.expect()` with a precise message is the CLAUDE.md-sanctioned
form. (A `decision()` accessor on `Execution<Decided>` was considered and
rejected: it would break the uniform `.evidence().<field>` access pattern that
every stage's result shares — the distributed-evidence design from P3.1.)

The X5-deferred design (`deferred_dry_run` as the default runner) is **not
debt** — it is a documented, intentional safety decision: the sandbox has no
filesystem isolation until P4.2's landlock, so running untrusted code would be
the harm the gateway prevents. The module docs explain it; `guarded_dry_run`
and P3.5's `dry_run_in_sandbox` are the built, working real-run path.

## FASE 4 — HARMONY CHECK

| Check | Result |
|-------|--------|
| New P3.7 pub symbols with a consumer | **10 / 10** — grep-verified (Cadeia 7: the index is stale for code written today) |
| Unused imports / dead code in the gateway | **0** — `cargo check` clean |
| Gateway files in a dependency cycle | **0** — `touring wiring cycles` names no gateway file |
| `pre-exec` hook + `exec` CLI wired | ✓ — `main.rs` dispatch arm + `command_table` entry |

`harmony_map.py` reported "3 597 orphan pub symbols / 3 cycles" — these are the
**workspace-wide pre-existing** figures (the same global counter the hooks
report as 169 626), not gateway findings. The gateway-scoped checks above are
the authoritative result.

## FASE 5 — FIX & POTENTIALIZE

**One real bug, found by FASE 6's executed proof, fixed here.**

`SYMPTOM` — `touring exec --profile sandboxed echo hi` reported
*"X6 denied the subprocess capability **'touring'**"* — the gated command was
`touring exec echo hi`, not `echo hi`.

`ROOT CAUSE` — `main.rs:159` builds `args` from `std::env::args().collect()` —
the **full process argv** — and dispatches it whole: `(cmd.handler)(&args)`. So
`cli::exec::run` received `["touring", "exec", "--profile", "sandboxed",
"echo", "hi"]`. `parse_global_flags` strips only `-j` / `-v` / `--timeout`, not
leading positionals — so `touring` and `exec` became words of the gated
command. The unit tests passed because they called `parse_exec_args` directly
with already-clean arrays (`["echo", "hello"]`); the E2E `run` tests encoded
the same wrong convention.

`FIX` — a new testable helper `sub_command_args(argv) -> &[String]` that skips
`argv[2..]` (binary + subcommand — the convention `ssr.rs:42` confirms);
`run` parses from it. The E2E `run` tests were rewired to a realistic argv
(`["touring", "exec", …]`), and a **regression guard**
`gate_command_gates_only_the_command_not_the_argv` asserts the gated capability
is `ls`, never `touring`.

`POTENTIALIZE` — the fix *adds* `sub_command_args` (a reusable, tested helper)
and a regression test; it shrinks nothing.

The fix was applied via `taco-forge perfect-create --force` (REGRA #14).

## FASE 6 — E2E PROOF (executed evidence)

```text
# full gateway suite
cargo test gateway::        → 157 / 157 PASS
cargo test --doc gateway::  →   4 /   4 PASS
cargo test cli::exec        →  17 /  17 PASS  (13 → 17: +4 for the fix)
cargo check --workspace     → exit 0 — zero regression

# entry-point exit codes — the touring binary, executed
touring exec echo hello                  → Allow, composite 0.93   exit 0
touring exec --profile sandboxed echo hi → Deny  (X6 denies Run)   exit 1
touring exec -j echo hi                  → JSON, 8 stages logged

# the pre-exec hook EXIT-0 INVARIANT — touring-hook pre-exec, executed
clean command   → exit 0
malformed input → exit 0   (fail-open)
non-code tool   → exit 0

# the fix, re-proven post-rebuild
touring exec --profile sandboxed echo hi → "denied the subprocess
  capability 'echo'"   ← was 'touring' (the bug); now correct.
```

The destructive-command path (`rm -rf /` → Deny) is proven by the executed
Rust E2E tests (`e2e_run_fails_for_a_destructive_command`,
`run_gateway_denies_a_destructive_command`, `e2e_gate_hook_input_destructive`) —
the binary cannot be driven with a literal destructive command because the
bash-safety hook (correctly) blocks the audit's own invocation.

## FASE 7 — outcome

| Dimension | Result |
|-----------|--------|
| Purpose fidelity | ✓ — every documented claim verified |
| Debt | ✓ — zero markers; the lone `.expect()` audited as correct |
| Harmony | ✓ — zero gateway orphans / cycles / unused |
| Bugs found | **1** — argv leak in `cli::exec::run` |
| Bugs fixed | **1** — `sub_command_args`, proven post-fix |
| Tests after audit | gateway 157 + doctests 4 + cli::exec 17 = **178**, all green |
| Regression | 0 — `cargo check --workspace` exit 0 |

**Phase P3 is sound.** The cross-audit proved purpose-fidelity with executed
evidence and corrected the one integration defect that isolated unit testing
structurally could not catch — a CLI handler's contract with `main.rs`'s
full-argv dispatch. Lesson persisted: `gotcha:cli-handler-full-argv`.

---
_Cross-audit complete. 7 phases. 1 file fixed (`cli/exec.rs`), +4 tests
(incl. a regression guard). Evidence executed and shown. 0 pending, 0 debt._
