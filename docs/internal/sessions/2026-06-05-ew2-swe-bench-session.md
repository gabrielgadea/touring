# E-W2 SWE-bench — Wave Session Report

> **Date**: 2026-06-05 | **Wave**: E.W2.P1.T5 (`touring-eval` SWE-bench-lite)
> **Task**: task_1780622111986800900 (plan, CILA L4) | **Mode**: TACO solo (L2), fully cargo/python-verified

## Outcome: harness infrastructure delivered + proven; full benchmark gated

E.W2.P1.T5 is XL. Its expensive half — 50 real GitHub issues + a multi-model
leaderboard — is genuinely gated on (a) E.W3 multi-provider (not yet built) and
(b) explicit credit authorization (memory: never run LLM batch loops). The
responsible interpretation of "prossiga com SWE-bench" was therefore: deliver the
**measurement engine** — verifiable now, zero credit burn, zero git on the
workspace — with a self-test proving it end to end.

## Delivered

- `eval/swe_bench/harness.py` (693 LOC, Python stdlib, via `taco-forge
  perfect-create-script`, 12 stages): SWE-bench-lite harness.
  - Schema: `EvalInstance` (inline + git modes), `SolverReport`, `InstanceResult`, `Report`.
  - Solvers: `Solver(ABC)` + `GoldSolver` + `FilePatchSolver` (the credit-safe
    out-of-band bridge to any external solver). **No LLM inside the harness.**
  - Metrics (exactly E.W2.P1.T5's list): `resolved_pct`, `vgp_false_positive_rate`,
    `mean_tokens`, `comparison.delta_vs_aider`.
  - Subcommands: `selftest`, `validate`, `run`, `emit-dataset`; `--check` CI gate.
- `eval/swe_bench/datasets/touring-rust-selftest.jsonl` (emitted by the harness).
- `eval/swe_bench/README.md` + `docs/how-to/run-swe-bench.md` (Diataxis).

## Validation (Hard Rule #9 — every claim cargo/python-verified)

- `selftest`: 2/2 resolved (a real Rust `cargo test` instance: `safe_add` overflow
  → `checked_add`; plus a Python instance), 0 VGP false positives.
- **Discrimination proof**: a wrong-but-claiming solver (`wrapping_add`, which
  compiles but still wraps) scored `resolved=0`, `vgp_false_positive_rate=1.0`,
  `fail_to_pass.test_no_overflow=False`. That `False` is a runtime test result —
  it proves cargo actually compiled the wrong code and ran the test. The 0.28s
  wall time is explained by the global sccache rustc-wrapper.
- `validate`: 2/2 gold-resolved, 0 problems (the pre-patch red-test correctness gate works).
- py_compile OK; ruff "All checks passed"; pyright 0 errors; file_size_gate PASS.

## Constraints honored

- REGRA #11: never git on the workspace — confirmed no `.git` at root; harness uses
  system temp (inline) or a temp clone (git mode).
- Memory [Code Analyses, LLM Synthesises]: harness never invokes a model; running
  it cannot burn credits. Model inference is an out-of-band, authorized step.
- REGRA #14: created via taco-forge; edits re-validated.

## Bugs fixed mid-wave

- argparse `help="...100%"` → `ValueError: incomplete format` (literal `%`); reworded.
- F541 (f-string without placeholder); pyright unused abstract param → `Solver(ABC)`
  + `@abstractmethod`; `main` CC 25→~2 by extracting `_cmd_*` handlers (composite 0.94→0.99).

## Remaining (gated — Gabriel's call)

- 50-issue curated Rust dataset from real GitHub issues (needs network + git in temp).
- Model-backed solver run (out of band, explicit credit authorization) → first baseline.
- Multi-model leaderboard + `touring.dev/eval` submission (E.W3 multi-provider first).
- Weekly CI scheduling of `--check` (distribution/CI track deferred by Gabriel).
