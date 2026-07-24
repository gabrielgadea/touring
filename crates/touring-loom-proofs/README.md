# touring-loom-proofs

Isolated crate for [loom](https://docs.rs/loom) concurrency-model proofs of
invariants the `touring-daemon` actor pattern relies on.

## Why a separate crate?

`RUSTFLAGS="--cfg loom"` is a **global** compiler flag — Cargo rebuilds
every crate in the dependency graph with loom's shadow primitives.
The main `touring-hooks` crate pulls `reqwest → hyper-util` transitively
(via `touring-core`'s `gpu-embeddings` feature); `hyper-util` uses
`tokio::net::UnixStream` which has **no loom shim** and fails to compile
under the flag.

This crate carries **zero touring dependencies**. Its only dev-dep is
`loom` itself. Running `cargo test -p touring-loom-proofs --release` with
the cfg flag compiles only the loom scaffolding — the broken transitive
chain never enters the build graph.

## Running

```sh
RUSTFLAGS="--cfg loom" cargo test -p touring-loom-proofs --release
```

Without the flag the tests are gated by `#![cfg(loom)]` and the file
compiles as an empty binary — zero cost for a normal `cargo test --workspace`.

## What we prove

| Test | Invariant |
|---|---|
| `invariant_a_concurrent_fetch_add_converges` | `AtomicUsize::fetch_add(SeqCst)` never loses updates under multi-producer contention. Models the `handled` counter in the actor loop. |
| `invariant_b_release_store_publishes_prior_writes` | Release/Acquire ordering correctly publishes a prior `Relaxed` write to any observer that sees the flag. Models the `acked` shutdown signal. |
| `invariant_c_mutex_protected_map_has_no_lost_update` | `Arc<Mutex<Vec<_>>>` never loses updates under two-writer contention. Models the `DashMap<String, JobState>` shard-locked pattern. |

## What we intentionally do NOT model

1. **Real `ProjectCommand` mpsc flow** — loom 0.7's `mpsc::channel` has
   known destructor panics during model exploration ("panic in a destructor
   during cleanup"). Since the real daemon uses `tokio::sync::mpsc` (outside
   loom's shadow universe anyway), we prove the *atomic backbone* that
   tokio's channel internally relies on rather than the channel itself.
2. **Tokio / rusqlite internals** — out of loom's scope.
3. **Shutdown acknowledgement via `oneshot`** — same reason as (1).

## Extending the proof set

When adding a new invariant:

1. Keep the model **small** — loom's permutation exploration is
   exponential in the number of threads and atomic ops. Two to three
   threads is the sweet spot.
2. Prefer **atomics + `Mutex`** over loom's channel primitives.
3. Gate every test with `#![cfg(loom)]` at the top of the test file.
4. Document the **invariant** and the **daemon code path** it models in
   the doc-comment above each `#[test]`.

## References

- [loom — Permutation testing for concurrent code](https://docs.rs/loom)
- `crates/touring-hooks/src/daemon.rs` — the real actor the proofs model
- `crates/touring-hooks/src/shared/job_registry.rs` — `JobState` + DashMap pattern
