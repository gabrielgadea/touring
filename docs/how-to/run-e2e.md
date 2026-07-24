# How to run the E2E health checks

> A **how-to** (Diátaxis): task-oriented. You have a workspace and want to know
> if the system is healthy end-to-end. Master Plan D.W4.P3.

## Goal

Get a single composite health signal (0–1) plus the per-subsystem breakdown,
and know what to do when it is low.

## Steps

```bash
cd ~/.claude/rust

# 1. Composite end-to-end score (0–1) — the one-number gate
touring e2e -j

# 2. Daemon component health (knowledge DB, bandit, symbol store, predictor, …)
touring doctor -j

# 3. Dashboard: symbol count, orphan count, RL reward, composite_health_score
touring status -j

# 4. Live counters (CEG activity, tantivy, health-delta, query cache)
touring gate-metrics -j
```

A healthy system reports `composite_health_score` toward 1.0 and all daemon
components `healthy`. The bundled script composes the first three with a
traffic-light verdict and a non-zero exit on failure (CI-friendly):

```bash
python3 ~/.claude/skills/Touring/scripts/diagnose_health.py   # exit 0/1/2
```

## Run the Rust test suite

```bash
cargo test --workspace                       # full suite
cargo test -p touring-hooks --lib            # one crate, lib tests only
cargo test -p touring-hooks --lib <pattern>  # filter by name
```

## Interpreting a low score

| Symptom | Likely cause | Action |
|---|---|---|
| `daemon_socket: error` right after session start | spurious startup race | wait 2–3s, re-run `touring daemon-ctl status` (REGRA #19 — never `pkill`) |
| `wiring` low / many "orphans" | stale wiring cache | `grep` the symbol to confirm a real consumer (VP-Scout Chain 7) before trusting |
| `cargo test` fails to compile | a real regression | `cargo check --workspace` and read the first error (never assert state without it) |
| composite drops after an edit | check the edited file | `touring health-delta status <file>` |

## Verify

`touring e2e -j` returns a composite ≥ your baseline and `cargo test` is green.
If the daemon is genuinely down, every command auto-spawns it on next call; you
do not start it by hand.
