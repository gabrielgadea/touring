# How to run a SWE-bench-lite evaluation

Task-oriented guide for `touring-eval` (Master Plan E.W2). For the full reference
see `eval/swe_bench/README.md`.

## Goal

Measure whether a solver resolves a set of issues, and get `resolved%`, the
**VGP false-positive rate**, token cost, and an optional Aider comparison.

## Prerequisites

- `python3` (stdlib only — no pip install).
- `cargo` if your dataset has Rust (`cargo test`) instances.
- For `git` mode instances: `git` and `patch` (used only inside temp dirs).

## 1. Confirm the harness works

```bash
eval/swe_bench/harness.py selftest
```

Expected: `SELFTEST PASS`, `2/2 (100.0%)`. This compiles a tiny Rust crate and a
Python fixture in a temp dir — nothing in your workspace is touched.

## 2. Build (or validate) a dataset

Each line of the JSONL is one issue. Start from the bundled example:

```bash
eval/swe_bench/harness.py emit-dataset --out eval/swe_bench/datasets/my-bench.jsonl
# edit my-bench.jsonl to add real instances, then:
eval/swe_bench/harness.py validate --dataset eval/swe_bench/datasets/my-bench.jsonl
```

`validate` fails loudly if an instance is malformed — including the crucial
correctness gate that every `fail_to_pass` test must be **red before the patch**
(a "fix" for an already-green test is bogus).

## 3. Produce solver output (the only step that can cost credits)

Run your solver **out of band** and dump its patches:

```text
patches/<instance_id>.files.json   # {"src/lib.rs": "...new content..."}  (inline)
patches/<instance_id>.patch        # unified diff                          (git)
patches/<instance_id>.meta.json    # {"tokens": 5123, "claims_resolved": true}
```

The harness never calls a model itself — this keeps `run` deterministic and free.

## 4. Score it

```bash
eval/swe_bench/harness.py run \
  --dataset eval/swe_bench/datasets/my-bench.jsonl \
  --solver file:patches \
  --out report.json --json
```

Read `report.json`: `resolved_pct`, `vgp_false_positive_rate`, `mean_tokens`,
and (if instances carry `aider_resolved`) `comparison.delta_vs_aider`.

## 5. Wire it into CI (weekly)

The `--check` flag turns the harness into a gate (exit 1 if below threshold):

```bash
# regression guard — fails if the gold baseline ever stops resolving
eval/swe_bench/harness.py run --dataset eval/swe_bench/datasets/touring-rust-selftest.jsonl \
  --solver gold --check --threshold 1.0
```

Schedule this in whatever runner you use (the distribution/CI track is deferred —
the gate primitive is ready; wiring it to a weekly job is the remaining step).

## Reading the VGP false-positive rate

This is the signal the plan calls out (`medir false-positive rate além de
resolved%`). A high `resolved_pct` with a high `vgp_false_positive_rate` means the
solver *looks* productive but ships fixes that don't hold — worse than a lower
honest score. Track both.
