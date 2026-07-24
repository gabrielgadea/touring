# touring-eval — SWE-bench-lite harness

> Master Plan **E.W2.P1.T5**. A deterministic, **credit-safe** harness that measures
> whether a *solver* actually resolves real software-engineering issues — tests go
> from red to green with no regressions. Rust-first, Python-capable.

## Why this exists

E.W2's DoD is a *SWE-bench baseline*: proof that the Touring/TACO stack resolves
issues, with `resolved%`, token cost, and a **VGP false-positive rate** (the
SWE-bench-for-Touring signal — a solver claiming a fix that doesn't hold). This
harness is the measurement engine. It is intentionally split from any model
inference so that running it can **never burn API credits on its own**.

## Two hard constraints it honors

1. **No git on the workspace** (REGRA #11). Inline instances write a self-contained
   mini-repo to a system temp dir; git mode (for external SWE-bench repos) confines
   every git call to a throwaway temp clone. The Touring workspace is never touched.
2. **No LLM inside the harness.** Solvers are a pluggable interface. The built-ins
   (`gold`, `file:<dir>`) are 100% deterministic. A model-backed solver runs
   *out of band* (under explicit credit authorization) and dumps its patches for
   deterministic scoring — see "Plugging a real solver" below.

## Quick start

```bash
# 1. Prove the harness end-to-end (compiles a tiny Rust crate + a Python fixture)
eval/swe_bench/harness.py selftest

# 2. Materialize the bundled instances as a JSONL example
eval/swe_bench/harness.py emit-dataset

# 3. Validate a dataset is well-formed (gold patch turns red tests green,
#    and fail_to_pass tests are genuinely red pre-patch)
eval/swe_bench/harness.py validate --dataset eval/swe_bench/datasets/touring-rust-selftest.jsonl

# 4. Score a solver's output and emit a JSON report
eval/swe_bench/harness.py run --dataset <dataset.jsonl> --solver file:/path/to/patches --out report.json

# 5. CI gate: fail if resolved_pct drops below a baseline
eval/swe_bench/harness.py run --dataset <dataset.jsonl> --solver gold --check --threshold 1.0
```

## Instance format (JSONL, one object per line — SWE-bench-lite compatible)

| field | meaning |
|-------|---------|
| `instance_id` | unique id (e.g. `repo__short-slug`) |
| `problem_statement` | the issue text given to a solver |
| `mode` | `inline` (self-contained tree) or `git` (clone + checkout) |
| `files` | *inline*: initial mini-repo tree `{path: content}` (starts red) |
| `gold_files` | *inline*: reference fix as full-file replacement `{path: content}` |
| `gold_patch` | *git*: reference fix as a unified diff (applied with `git apply`) |
| `repo` / `base_commit` | *git*: clone URL + commit to check out |
| `test_cmd` | runner; the harness appends each test name (e.g. `cargo test --quiet`) |
| `fail_to_pass` | tests that must go red→green (the "fix") |
| `pass_to_pass` | tests that must stay green (no regression) |
| `aider_resolved` | optional: whether Aider solved it (for the comparison column) |

## Solvers

- `gold` — returns the reference fix. Used by `selftest`/`validate` and as the
  CI upper-bound baseline (proves the dataset + harness are well-formed).
- `file:<dir>` — reads pre-computed solver output, one set of files per instance:
  - `<dir>/<id>.files.json` → `{path: new content}` (inline mode)
  - `<dir>/<id>.patch` → unified diff (git mode)
  - `<dir>/<id>.meta.json` → `{"tokens": int, "claims_resolved": bool}`
  - missing output ⇒ empty patch, `claims_resolved=false` (an honest non-answer)

### Plugging a real solver (credit-safe)

1. Run TACO / Aider / a model agent against the dataset **out of band** (this is
   the step that costs tokens — it is your explicit, authorized decision).
2. Dump each result into a directory in the `file:<dir>` layout above.
3. Score deterministically: `harness.py run --solver file:<dir> --dataset <d> --out report.json`.

The harness then tells you not just *did the tests pass* but *did the solver lie*
(`claims_resolved=true` but `resolved=false` ⇒ a VGP false positive).

### Bundled model solver: MiniMax-M3

`solvers/minimax_solver.py` is a ready-to-use step 1. It calls MiniMax-M3 over the
MiniMax Anthropic-compatible endpoint (key from `$MINIMAX_API_KEY`, never
hardcoded) and writes the `file:<dir>` layout for you:

```bash
eval/swe_bench/solvers/minimax_solver.py \
  --dataset eval/swe_bench/datasets/touring-rust-selftest.jsonl --out-dir /tmp/minimax_patches
eval/swe_bench/harness.py run \
  --dataset eval/swe_bench/datasets/touring-rust-selftest.jsonl \
  --solver file:/tmp/minimax_patches --out eval/swe_bench/runs/minimax-m3-selftest.report.json
```

It is the only credit-costing component and lives outside the harness by design.

### First baseline (selftest, 2026-06-05)

`runs/minimax-m3-selftest.report.json` — MiniMax-M3: **2/2 resolved (100%)**,
VGP false-positive rate **0.0**, mean **523 tokens**, **+50 pp vs Aider** over the
2 shared instances. End-to-end: real model → patches → real `cargo test` + Python
in temp dirs → deterministic scoring.

### Scaled baseline — touring-lite-v1, 20 instances (2026-06-06)

`runs/minimax-m3-lite-v1.report.json` — MiniMax-M3 over the curated
`datasets/touring-lite-v1.jsonl` (14 Rust + 6 Python, built by
`datasets/build_lite_bench.py`, all `validate`-clean):

| group | resolved | tokens min/mean/max |
|-------|----------|---------------------|
| Rust  | 14/14    | 458 / 619 / 864 |
| Python| 6/6      | 387 / 489 / 765 |
| **All** | **20/20 (100%)** | 387 / **580** / 864 |

VGP false-positive rate **0.0**; 20 model calls in ~2m42s; scoring in ~3s.

**Honest reading**: 100% means the pipeline works at scale, but the lite set
(single-function partial bugs) is too easy to *discriminate* solver quality — the
VGP false-positive metric only becomes informative on harder instances where a
model bluffs a fix that doesn't hold. The next lever for a credible benchmark is
**difficulty**, not just count: multi-file bugs, subtle logic, and raw GitHub
issues (Multi-SWE-bench Rust — needs per-repo Docker envs + larger model context,
the documented heavy escalation).

### Real Multi-SWE-bench path (git mode, 2026-06-06)

The harness now runs **real GitHub issues** end-to-end via git mode +
`datasets/import_multi_swe.py` (clone @ `base.sha` → apply `test_patch` → apply the
solver's patch → `cargo test`). Proven in loco on **`tokio-rs/bytes`**:

- **`bytes-732`** (real bug "Buf::get_int sign extension"): MiniMax-M3 produced a
  SEARCH/REPLACE patch that the harness scored **resolved 1/1** — `test_get_int`
  (fail_to_pass) red→green + 9 pass_to_pass green. Report:
  `runs/minimax-m3-bytes732.report.json`. This is the credibility milestone: a real
  model fixing a real GitHub issue, verified by the real test suite, NOT a fixture.
- **Multi-instance bytes** (`runs/minimax-m3-multiswe-rust.report.json`): of the 5
  bytes instances, **3 (543/643/547) have empty `f2p_tests`** in the upstream dataset
  (the fix added no failing test) and are now **skipped loudly** by the importer — not
  usable for red→green scoring. Of the 2 usable instances, MiniMax-M3 resolved
  `bytes-732` and **failed `bytes-721`** (its patch left the fail_to_pass test red →
  the harness scored it unresolved, `vgp_false_positive_rate=1.0` since the solver
  claimed a fix). That is the metric doing its job: a real model limitation caught,
  no false pass.

**Known limitations / follow-ups** (honest):
1. *Solver output variance* — MiniMax-M3 is a reasoning model; its reply format varies
   run-to-run, so `apply_search_replace` occasionally misses (bytes-732 parsed on its
   dedicated run, missed on the batch run). Hardening: tolerant SR matching + a retry.
2. *Native build only* — bytes builds natively; heavier repos (fd/clap/ripgrep) may
   need their Docker envs (per the upstream harness). Not attempted here.
3. *Oracle file context* — the solver sees the files the gold patch touches (standard
   SWE-bench "oracle retrieval"), not a full-repo retriever.

## Metrics (in the JSON report)

| field | meaning |
|-------|---------|
| `resolved_pct` | fraction of instances actually resolved |
| `vgp_false_positive_rate` | of instances the solver *claimed* it fixed, how many it didn't |
| `mean_tokens` | mean solver-reported token cost |
| `comparison.delta_vs_aider` | percentage-point delta vs Aider over shared instances |

## Status (honest scope)

- **Delivered & verified in-session**: the harness, schema, solver interface,
  all four metrics, the `--check` CI gate, and a runnable self-test (a real Rust
  cargo instance + a Python instance). Discrimination proven: a wrong-but-claiming
  solver scores `resolved=0`, `vgp_false_positive_rate=1.0`.
- **Gated (requires explicit credit authorization + E.W3 multi-provider)**: the
  full 50-issue curated Rust dataset from real GitHub issues, the multi-model
  leaderboard, and the public `touring.dev/eval` submission. Curate real instances
  into `datasets/` and run a model-backed solver out of band to produce a baseline.

See `docs/how-to/run-swe-bench.md` for the task-oriented walkthrough.
