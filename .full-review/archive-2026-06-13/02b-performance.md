# Phase 2B: Performance & Scalability Review

> Touring workspace · 2026-06-13 · agent: performance-engineer (read-only)
> North star: what blocks Touring from **Premium, Elite-of-Market** performance.
> Live signal source: `touring gate-metrics` / `touring status -j` / `touring doctor -j` (real numbers, no guesses).

---

## TL;DR — the gate-metrics verdict (FACT, measured 2026-06-13)

The user-perceived hook plane has a **catastrophic tail**. From `touring gate-metrics` (1361 samples):

| Histogram | count | p50 | p90 | p99 | p999 | max |
|---|---|---|---|---|---|---|
| **`hook_dispatch_latency`** | 1361 | **239 µs** | **28.3 ms** | **488 ms** | **1.30 s** | **1.34 s** |
| **`rkyv_dispatch_latency`** | 263 | 362 µs | 253 ms | 432 ms | 571 ms | 571 ms |
| `ann_search_latency` | 28 | 1 µs | 1 µs | 1 µs | 1 µs | 1 µs |

**The headline**: p50 is elite (sub-ms), but **p50→p90 is a 118× cliff** and **p50→max is 5,600×**. Every Claude Code tool call fires hooks; a 488ms p99 / 1.3s p999 is directly user-felt latency. This is the #1 thing between Touring and elite.

**ann_search_latency tail verdict**: the earlier snapshot's p99≈4.4s (vs p50 1µs) is **GONE** — now p50=p99=max=1µs across 28 samples. This confirms the 4.4s was a **cold-index / first-call artifact** (lazy index load, ANN structure build on first query), not steady-state. It is a **cold-start problem, not a hot-path problem** — see F6.

Daemon health (`touring status -j`): composite_health 0.7139, 7/8 components healthy, `memory_rss_mb` = **1477 MB** (~1.5 GB resident).

---

## Severity counts

**2 Critical · 3 High · 4 Medium · 2 Low** · plus 3 "already elite" callouts.

---

## F1 [CRITICAL] — Inline full-workspace E2E scan on the actor thread after EVERY post_edit/post_write

**This is the root cause of the hook latency tail.**

`run_project_actor` processes hooks **serially** on a single per-project OS thread (`daemon.rs:207-307`, `cmd_rx.blocking_recv()` at line 220). After a `post_edit` or `post_write` handler returns, the actor runs **inline, on the same thread, before responding**:

```rust
// crates/touring-dispatch/src/daemon.rs:297-302
if hook_name == "post_edit" || hook_name == "post_write" {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let e2e_payload = serde_json::json!({ "depth": "quick" });
        crate::cli_e2e::cli_e2e(&mut runtime, &e2e_payload);   // <-- BLOCKS THE ACTOR
    }));
}
```

`cli_e2e` (`crates/touring-cli/src/cli_e2e.rs:1287`) at **even `quick` depth** always runs three subsystem phases (cli_e2e.rs:1346-1349):

```rust
phases.push(phase_index(rt, &target, depth));    // includes count_code_files(target)
phases.push(phase_wiring(rt));                    // wiring audit
phases.push(phase_knowledge(rt));                 // DB query
```

`phase_index` calls `count_code_files(target)` (cli_e2e.rs:165, ~line 186), which is a **synchronous, recursive `std::fs::read_dir` walk of the entire project tree**:

```rust
// crates/touring-cli/src/cli_e2e.rs:1483-1505
fn count_code_files(target: &Path) -> usize { ... collect_files_recursive(target, &mut files, limit); }
fn collect_files_recursive(dir: &Path, files: &mut Vec<String>, limit: usize) {
    let entries = match std::fs::read_dir(dir) { ... };   // recursive fs walk
}
```

**Mechanism of the tail**: the actor is strictly serial per project (daemon.rs:220, the comment at daemon.rs:14-21 claims per-project concurrency but that is *across* projects only). A `post_edit` → inline E2E → recursive fs walk of thousands of dirs takes tens-to-hundreds of ms. During that window, every other hook for that project (the burst of pre_read/post_read/pre_edit that Claude Code fires within milliseconds) **queues behind it and convoys**. When the walk finishes, the queue drains in a burst → exactly the observed p50 (fast handler, empty queue) vs p90/p99/max (handler behind a stalled E2E walk) bimodal split.

**Evidence it's the fs walk, not the DB**: the per-hook DB write IS already offloaded async (see "already elite" below). The E2E walk is the one piece of heavy work still synchronous on the actor.

- **Severity**: Critical
- **Estimated impact**: This single change should collapse `hook_dispatch_latency` p90 from 28ms toward the sub-ms p50, and eliminate the 488ms p99 / 1.3s p999 tail for post_edit/post_write-adjacent hooks. It is the dominant tail driver. On a 500k-LOC target the walk cost grows linearly — this is also the #1 scalability cliff (see F8).
- **Fix (concrete)**: Do not run E2E inline on the response path. Three options, in order of preference:
  1. **Remove from the hot path entirely** — fire-and-forget the E2E scan on a background tokio task / dedicated low-priority thread, decoupled from the hook response. The hook response should not wait for analytics.
     ```rust
     if hook_name == "post_edit" || hook_name == "post_write" {
         let pr = runtime.project_root.clone();
         // send to a bounded background scanner channel; coalesce/debounce per project
         BACKGROUND_E2E_TX.try_send(pr).ok();   // never blocks the actor
     }
     ```
  2. **Debounce** — only run the scan at most once per N seconds per project (it's analytics, not correctness; running it on every keystroke-level edit is pure waste).
  3. At minimum, **drop `count_code_files` from `phase_index`** on the inline path — use the cached `symbol_store.stats()` (already O(1), cli_e2e.rs lines ~9-12) and never walk the fs synchronously inside a hook.

---

## F2 [CRITICAL] — Heavy handlers run inline on the serial actor thread (head-of-line blocking)

Even setting F1 aside, the actor itself runs **every** handler inline (`daemon.rs:242-250`):

```rust
let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match table
    .get(hook_name.as_str()) {
    Some(handler) => handler(&mut runtime, &payload),   // SYNC, on the actor thread
    None => { ... }
}));
```

The daemon explicitly classifies "heavy" hooks (`daemon.rs:490-503`: `cli-index-rebuild`, `cli-ast-blast`, `cli-tantivy-reindex`, +9 more) and only gives them a **longer timeout** (300s vs 15s, daemon.rs:1511-1515) — it does **not offload them**. A heavy op (a blast-radius computation, a tantivy reindex, an index rebuild) blocks the actor thread, and every subsequent hook for that project queues behind it. This is classic head-of-line blocking and is the structural reason the p90 cliff exists even between E2E scans.

- **Severity**: Critical (structural; F1 is the most frequent trigger but this is the underlying defect)
- **Estimated impact**: Removes the convoy for any slow op, not just E2E. Required for the system to hold under burst load and on large workspaces.
- **Fix (concrete)**: Offload heavy/blocking handlers off the actor's response-critical thread. The complication is `rusqlite::Connection` is `!Sync` (knowledge.rs:200, owned by-value in `HookRuntime`), so naive `spawn_blocking` won't compile. Two viable paths:
  - **Per-project worker pool with an owned connection each** (a small `rayon`-style pool of N actor threads per project, each owning its own `Connection` to the same sqlite file in WAL mode — sqlite WAL supports concurrent readers + one writer). Light read-only hooks (pre_read enrichment) run on any worker; the single writer hook serializes only writes.
  - **Two-tier actor**: a fast lane (latency-budgeted, only cheap enrichment) that always responds within a few ms, and a slow lane (separate thread/channel) for heavy ops whose result is delivered out-of-band or cached. Hooks must fail-open fast (they already do — see below), so the fast lane can return the fail-open default if the slow lane misses its budget.

---

## F3 [HIGH] — No per-request latency budget that actually bounds hook *execution*

Hooks fire on every tool call and **must fail-open fast** — the architecture knows this (fail-open is implemented well, see "already elite"). But the only bound on hook *execution* is:
- a `LatencyMarker` that merely **warns** at >60s (daemon.rs:251-254) and >1s (daemon.rs:289-291) — observability, not enforcement;
- a handler **timeout** (15s/300s, daemon.rs:1511-1515) on the *client-side oneshot* — but the actor thread keeps running the handler to completion even after the client gives up (the timeout abandons the result, it does not cancel the work). So a slow handler still blocks the actor for the *full* duration, continuing to convoy others.

There is **no deadline that returns the fail-open default early and lets the actor move on**. The hooks fire on the critical path of a human's editing loop; a 1.3s p999 means the user occasionally waits >1s on a tool call for analytics they never see.

- **Severity**: High
- **Estimated impact**: Caps the tail. Combined with F1/F2, turns "p999 = 1.3s" into "p999 ≤ budget" (e.g. 50ms) by design.
- **Fix**: Give the hook dispatch a hard execution budget (e.g. 25-50ms for enrichment hooks). On overrun, return the fail-open default immediately and let any in-flight heavy work complete out-of-band (its result populates a cache for next time). This is what makes the latency *bounded* rather than *hopefully fast*.

---

## F4 [HIGH] — rkyv "zero-copy" IPC does a full JSON deserialize + 3× String copy per request

`crates/touring-rkyv/src/ipc.rs:56-62` defines `IpcRequest` with **owned** fields (`hook: String`, `payload: Vec<u8>`, `project_root: String`, `session_id: String`) — not borrowed slices. The doc comment (ipc.rs:12) claims "field access at native Rust speeds," but the daemon decode path (daemon.rs ~1014-1033) does:

```rust
let payload_value: serde_json::Value =
    serde_json::from_slice(archived.payload.as_slice()).unwrap_or(serde_json::Value::Null);  // full JSON parse + alloc
let req = DaemonRequest {
    hook: archived.hook.to_string(),          // copy
    payload: payload_value,                   // owned tree
    project_root: archived.project_root.to_string(),  // copy
    session_id,                               // archived.session_id.to_string() — copy
    priority: archived.priority,
};
```

So rkyv buys zero-copy *validation* of the envelope (`check_archived_root` with bytecheck — good for safety) but then **immediately deserializes the inner payload to a full `serde_json::Value` and copies every string field**. The `rkyv_dispatch_latency` p90=253ms tail correlates with this: a large payload (a CallGraph, a WiringAudit response) hits `serde_json::from_slice` allocating a full tree, *while* the actor is already convoyed (F1/F2). rkyv's main win was supposed to be avoiding exactly this.

- **Severity**: High (it negates the stated reason rkyv exists; the rkyv tail is real)
- **Estimated impact**: For large payloads, eliminates a full parse+alloc per request. The bigger structural win is that payloads stay as borrowed archived bytes and are only parsed by handlers that need them.
- **Fix**: Keep the payload as borrowed archived bytes (`&[u8]`) and pass it through; let each handler parse lazily only if it needs the JSON. Make `DaemonRequest<'a>` borrow from the archive (`hook: &'a str`, `payload: &'a [u8]`) instead of owning. Where the inner payload is itself structured, define an rkyv-archivable payload type instead of nesting JSON-in-rkyv (JSON-inside-rkyv is the worst of both — you pay rkyv's archive cost AND serde's parse cost).

---

## F5 [HIGH / partly contested] — Latency histogram uses a single global `Mutex<Histogram>` on the dispatch path

`crates/touring-hooks-shared/src/gate_metrics.rs:36-37`:

```rust
pub struct LatencyHistogram {
    inner: Mutex<Histogram<u64>>,
}
```

recorded synchronously on the actor response path (daemon.rs:285-287) for **every** dispatch, and again for rkyv (daemon.rs:1039). The maintainers already flagged this — gate_metrics.rs:34: *"If it becomes one, migrate to `hdrhistogram::sync::SyncHistogram` (shardable)."*

**Honest assessment (contesting the over-attribution)**: `record_us` holds the lock for an O(1) HDR record (gate_metrics.rs:62-69); `snapshot` holds it for O(log n) percentile reads (gate_metrics.rs:77-94). A contended O(1) mutex adds **microseconds**, not the 28ms p90 cliff. So this is **NOT the primary tail driver** (F1/F2 are). However, under genuine burst (the convoy drain after an E2E walk completes — dozens of hooks recording at once) it becomes a real secondary serialization point, and it is on the most latency-sensitive path in the system. It should be removed on principle: **never put a global lock on the per-request hot path**.

- **Severity**: High (defense-in-depth; do it when fixing F1/F2 so the drained convoy doesn't re-serialize)
- **Estimated impact**: Removes a global serialization point from the hot path; small absolute latency win but eliminates a contention class entirely.
- **Fix**: Migrate to `hdrhistogram::sync::SyncHistogram` (the maintainer's own suggestion) or a sharded/atomic recorder (per-thread recorder, merged at snapshot). Both make recording lock-free on the fast path.

---

## F6 [MEDIUM] — Cold-start tail: ANN/index lazy build pays a multi-second first-call cost

The earlier `ann_search_latency` p99≈4.4s (p50 1µs) — now uniformly 1µs — is a **cold-start signature**: the ANN index/structure is built or loaded lazily on the first query of a session, so the first caller eats the full build (seconds) while every subsequent call is microseconds. This is invisible in steady state but it is the **first interactive query of every fresh daemon / session** — i.e. the user's *first* edit after a daemon (re)start waits seconds.

- **Severity**: Medium (rare per-session, but maximally user-visible — it's the first impression)
- **Estimated impact**: Turns "first query after restart = 4.4s" into a background warm-up that completes before the user's first interaction.
- **Fix**: Warm the ANN/index structures at daemon/session-start in the background (the codebase already does this pattern — `warm_load_global_model` at daemon.rs:218 warm-loads the X4 model at actor spawn). Extend the same warm-load to the ANN index and tantivy reader so the first query is hot. Add a `ann_cold_start_us` counter so the warm-up is observable.

---

## F7 [MEDIUM] — Daemon RSS ~1.5 GB; memory-pressure instrumentation present but all-zero (not wired)

`memory_rss_mb` = **1477 MB** (`touring status -j`). For a long-lived daemon serving 6 projects this is large. The good news: there IS memory-pressure infrastructure — `memory_pressure_{green,yellow,red}_count`, `memory_pressure_total_tick_count`, `swap_thrashing_detected_count` — but **all read 0**, meaning the pressure monitor isn't ticking (no samples). Similarly `metadata_backpressure_dropped`, `metadata_cache_hit` = 0. The watchdog is **opt-in and off by default** (daemon.rs:627-639: "idle watchdog disabled (set TOURING_IDLE_TIMEOUT_SECS>0 to enable)") — correct for a dev workstation, but it means there is **no automatic memory ceiling**; RSS can only grow.

- **Severity**: Medium (works today; risk is unbounded growth on a long-running / many-project daemon)
- **Estimated impact**: Bounds daemon memory; prevents the OOM-on-day-30 failure mode that kills "elite-of-market" reliability claims.
- **Fix**: (1) Actually start the memory-pressure tick loop so those counters populate and back-pressure/eviction can trigger. (2) Profile the 1.5 GB — likely the per-project `HookRuntime`s each holding a `crdt_graph` / `pheromone_graph` (`Arc<RwLock<PheromoneGraph>>`, hook_runtime.rs:602) + symbol store in memory. Consider evicting cold projects' runtimes (the LRU `last_accessed`/`touch()` machinery at daemon.rs:1348 already exists — wire it to actually drop idle project runtimes under pressure).

---

## F8 [MEDIUM] — Scalability cliff at 500k LOC is the synchronous fs walk + per-edit reindex, not the index itself

The index lookups are already cheap at steady state (symbol_store.stats() is O(1); ann is µs warm). The scalability risk on a large target is concentrated in the **inline synchronous work that scales with project size**:
- `count_code_files` recursive `std::fs::read_dir` (F1) — O(files) on every post_edit. At 500k LOC ≈ tens of thousands of files this is the linear blow-up.
- per-edit index upsert + (heavy) reindex on the actor (F2).
- the wiring DB: `touring doctor` reports `rows=125269 producers=6635 consumers=118634` for *this* workspace; a 10× larger target makes wiring audits (`phase_wiring`, run inline per edit via F1) proportionally slower on the hot path.

- **Severity**: Medium (the system holds today at ~500k LOC for *this* repo, but the per-edit cost is O(project size), which is the wrong complexity for a per-keystroke-adjacent hook)
- **Estimated impact**: Fixing F1/F2 changes the per-edit hot-path cost from O(project size) to O(1), which is the actual requirement for elite scalability.
- **Fix**: Same as F1/F2 — get all O(project-size) work off the per-edit response path; make incremental indexing touch only the changed file (verify the post_edit path reindexes only the edited file, not the workspace).

---

## F9 [LOW] — Write-lock on the runtime map just to bump an LRU timestamp

`daemon.rs:1348-1351` takes `runtime.write().await` on the project-runtime map to call `pr.touch()` (update `last_accessed`) on every request to an existing project. Under burst this is a brief write-lock contention point across concurrent projects.

- **Severity**: Low (microsecond-scale, only matters under multi-project burst)
- **Fix**: Store `last_accessed` as an `AtomicU64` nanos timestamp inside the project entry and update with `Ordering::Relaxed` — requires only a read-lock on the map (or no lock if the entry is `Arc`'d).

---

## F10 [LOW] — Tokio runtime metrics not wired (observability gap)

`tokio_num_workers`, `tokio_num_idle_threads`, `tokio_num_blocking_threads`, `tokio_injection_queue_depth` all = 0 in gate-metrics; `snapshot_tokio_metrics()` (gate_metrics.rs:1302) reads atomics that nothing populates. For a tokio daemon, injection-queue depth and blocking-thread saturation are exactly the signals that would have surfaced F1/F2 from telemetry alone. Right now the convoy is invisible to the metrics until it shows up as dispatch-latency tail.

- **Severity**: Low (no functional impact; it's why the tail wasn't auto-diagnosed)
- **Fix**: Wire `tokio::runtime::Handle::current().metrics()` into the periodic snapshot (workers, blocking threads, global/local queue depth, injection queue). Add a `blocking_pool_saturated` alert. This makes future head-of-line blocking self-diagnosing.

---

## Build-time performance (F11–F13)

### F11 [HIGH] — `touring-foundation` fan-in 22 serializes incremental rebuilds; the 9× claim is aspirational (~6–7× realistic)

`touring-foundation` is depended on by **22 crates** (grep of `crates/*/Cargo.toml`), and it's a god-kernel that absorbed touring-telemetry/activity/definitions/resource-monitor (foundation/Cargo.toml). Editing one line in foundation marks 22 downstream crates dirty. Because `incremental = false` in `[profile.dev]` (root Cargo.toml, "disk-optimization wave 2026-04-26"), each of those 22 crates loses its rustc incremental MIR state and recompiles fully (sccache helps only on unchanged compilation units). The masterplan's **~9× incremental-rebuild claim is not supported by the current topology** — the dependency DAG terminates in a *serial chain* `foundation → {code, intelligence, dispatch} → hooks → server` with near-zero parallelism in the final phase. Realistic ceiling without further splits: **~6–7×**.

- **Severity**: High (developer velocity; directly limits how fast the team can iterate toward elite)
- **Fix**:
  1. Split `touring-foundation` into `foundation-core` (schema/config/error — the true kernel, the thing 22 crates actually need) vs `foundation-telemetry`/`foundation-activity`/`foundation-embedding` (the absorbed concerns). Most of the 22 only need the kernel; isolating it shrinks the dirty set per edit.
  2. **Set `incremental = true` for local dev** (keep `false` only in CI where sccache is cold). The disk-hygiene tradeoff is real but the rebuild penalty on a 22-fan-in kernel dwarfs the 25 GB disk cost; a `[profile.fast-iter]` with `incremental=true` already exists (root Cargo.toml) — make it the default local profile.

### F12 [MEDIUM] — `touring-code` pulls 15 tree-sitter grammars + full `syn` unconditionally (no feature gates), on the foundation-edit critical path

`crates/touring-code/Cargo.toml` pulls all 15 tree-sitter grammars and `syn` with `["full","extra-traits","visit","parsing"]` with **no feature gates**, and `touring-code` sits on the rebuild path of nearly every consumer. A foundation edit → touring-code dirty → recompile of 15 grammars + full syn (~120s of the rebuild) on the critical path.

- **Severity**: Medium
- **Fix**: Split `touring-code-light` (types/API, foundation-only) from `touring-code-heavy` (tree-sitter + syn). Consumers that only need symbol types depend on light; only the actual parsers pull heavy. Feature-gate individual grammars so a Go-less workspace doesn't compile tree-sitter-go. Estimated ~90–120s off the foundation-edit rebuild.

### F13 [MEDIUM] — Heavy features default-on; no feature unification (cargo-hakari absent)

`touring-dispatch` / `touring-hooks` default feature sets pull tantivy, wasmtime, rkyv-derive, prost unconditionally (their `default = [...]` arrays). And there is **no `cargo-hakari` workspace-hack** to unify the feature matrix across 1,558 deps — so feature-set divergence across the ~20 tokio/rkyv consumers can force redundant recompiles.

- **Severity**: Medium
- **Fix**: Default-off the heavy, optional subsystems (tantivy-fts, inferlets-wasm, gpu-compute) and make them explicit opt-ins for the binary that needs them. Add `cargo-hakari` to unify features and cut cold-build feature-explosion overhead (~5–10% of total build).

---

## What is ALREADY fast / elite (keep these — do not regress)

- **Fail-open hooks**: panic-guarded handlers (`catch_unwind`, daemon.rs:242-250) keep the actor alive on any handler panic; the actor never dies (daemon.rs:198-202 documents why). The accept loop has exponential backoff (daemon.rs:755-820). Excellent — hooks genuinely cannot take the daemon down.
- **CEG fast-path**: `ceg_captured_count`=166, `ceg_fast_path_count`=166, `ceg_sandboxed_count`=0 — 100% of captured executions took the provably-pure fast path; the sandbox cost is paid only when actually needed. `pre_edit_fast_ratio`=0.8 / `pre_write_fast_ratio`=0.8 — 80% of pre-edit/pre-write hooks skip full enrichment. The fast-path design is elite.
- **Async DB offload on the write path**: `post_edit.rs:555` / `post_write.rs:248-250` fire-and-forget the edit record to `AsyncFileKnowledgeDB` — the sqlite write is already OFF the actor's response path. This is exactly right; the F1 E2E scan is the one piece that escaped the same treatment.
- **Bounded caches**: `DryRunCache` is a `moka::sync::Cache` with `max_capacity` clamped to `MAX_CAPACITY_CEILING` and TTL (dry_run_cache.rs:34-67) — eviction is bounded, env-overridable, guarded against a mistyped capacity. `query_cache_hit_ratio`=0.5625 (moka, 4096/60s TTL). Bounded-cache discipline is elite.
- **Per-project connection semaphore** + handler budget timeouts + REQUEST_TIMEOUT on acquire (daemon.rs:778-790, 1434-1453, 1511-1544) — one project can't starve FDs or hang the daemon. Good isolation primitives (they just need to be paired with actual offload + execution budgets per F2/F3).

---

## The #1 performance lever toward elite

**Get all O(project-size) and blocking work OFF the serial actor's hook-response path (F1 + F2), and put a hard execution budget on hook dispatch (F3).**

Concretely, the single highest-ROI change is **F1**: stop running the full `cli_e2e` workspace scan inline after every post_edit/post_write — make it a debounced, fire-and-forget background task (the codebase already proves this pattern with `AsyncFileKnowledgeDB` and `warm_load_global_model`). That one change should collapse `hook_dispatch_latency` p90 from 28 ms toward the 239 µs p50 and erase the 488 ms p99 / 1.3 s p999 / 1.34 s max tail — the entire user-perceived latency problem. F2/F3 then make the improvement *structural and bounded* rather than incidental, which is what "Premium, Elite-of-Market" requires: not "usually fast," but **provably bounded** hook latency on every tool call, at any workspace size.
