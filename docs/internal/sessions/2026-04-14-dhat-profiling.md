# DHAT Heap Profiling — Procedure

> **Date**: 2026-04-14 | **Owner**: Touring core | **Status**: feature-wired, baseline run deferred

## Why a separate doc?

`dhat-heap` is a **mutually exclusive feature** with `prod-allocator` (both
install a global allocator — see `crates/touring-server/Cargo.toml` lines
39-42). Running a heap profile requires building a *separate* binary that
swaps the production allocator for dhat's instrumentation.

Doing this against the live daemon serving Claude Code's session is too
risky for ad-hoc validation. This doc captures the procedure so future
sessions can run the profile in a controlled window.

## When to run

Run a dhat profile when:
- A specific suspicion exists ("we think `JobRegistry` leaks under N>1k
  spawned workers") — focused workload.
- After major refactor ("we replaced `Arc<DashMap>` with moka — confirm
  steady-state RSS dropped").
- Before a release — sanity-check no allocator pathology snuck in.

Do NOT run continuously: dhat instrumentation slows the binary 2-10×
and produces files (`dhat-heap.json`) that grow with allocation count.

## Procedure (one-shot, ~10 minutes)

```bash
# 1. Build a profiling binary — drop default features, re-add the minimum
#    set you need to exercise the workload, then add dhat-heap.
cargo build --release \
    --no-default-features \
    --features dhat-heap,wasm-plugins,l7b-alpha,async-memory,scip-emit,rkyv-ipc \
    -p touring-server

# 2. Stop the production daemon ONLY in a maintenance window.
#    The daemon's lock file (/tmp/touring-daemon-1000.lock) prevents
#    accidental double-startup.
kill -TERM "$(pgrep -f touring-daemon)"

# 3. Run the profiling binary against the workload of interest.
#    Example: 1000 hook calls to exercise the hottest paths.
./target/release/touring-daemon &
DAEMON_PID=$!
for i in $(seq 1 1000); do
  ./target/release/touring index find HookRuntime >/dev/null
done
kill -TERM "$DAEMON_PID"

# 4. dhat writes dhat-heap.json into CWD on graceful shutdown.
#    Inspect with the official viewer:
#    https://nnethercote.github.io/dh_view/dh_view.html
ls -lh dhat-heap.json
```

## Reading the output

Open `dhat-heap.json` in `dh_view.html`. Look for:

| Signal | Interpretation |
|---|---|
| `t-gmax` (peak bytes) | Highest RSS during the run. Should match `top -p $DAEMON_PID` peak. |
| `t-end` (bytes at exit) | Steady-state allocation. Anything > 100 MiB after a quiet shutdown is a leak candidate. |
| Top allocation sites | Click to sort by `t-gmax`. The first 5 entries usually account for >80% of memory. |
| `Arc<DashMap<String, JobState>>` | Wave 4 D1 candidate for moka migration if it appears in top 5. |
| Tantivy IndexWriter buffers | Expected up to ~50 MiB during reindex; should drop to <5 MiB at idle. |

## Baseline (target — not yet captured)

Run the procedure above in a maintenance window and record:

| Metric | Target | Capture date | Actual |
|---|---|---|---|
| `t-gmax` post-startup | < 200 MiB | _pending_ | _pending_ |
| `t-end` post-shutdown | < 50 MiB | _pending_ | _pending_ |
| Top alloc site | (any) | _pending_ | _pending_ |
| 1000-call run delta | < 20 MiB churn | _pending_ | _pending_ |

Update this table after the first capture; keep the previous row as
historical baseline so regressions are visible.

## Companion benchmarks

The dhat run captures memory; pair with these for a complete profile:

```bash
# CPU profile via criterion (already wired)
cargo bench -p touring-rkyv --bench ipc_vs_json -- --quick

# Atomic-counter latency (Wave 4 A2)
cargo bench -p touring-hooks --bench gate_metrics_divan

# Concurrency proofs (Wave 2)
RUSTFLAGS="--cfg loom" cargo test -p touring-loom-proofs --release
```

## Rollback (always safe)

```bash
# Restore the production binary
cp ~/.claude/hooks/touring-daemon.old ~/.claude/hooks/touring-daemon
# Or rebuild standard:
cargo build --release -p touring-hooks
cp target/release/touring-daemon ~/.claude/hooks/touring-daemon
# Daemon respawns on next hook trigger with the production allocator.
```
