# Phase 2b: Performance & Scalability (F2.7–F2.13) — Touring Workspace

> Run: 2026-06-21 · Performance engineer half of the Premium-Elite review.
> **Methodology**: every finding cites `crates/<x>/src/...:line` Read directly OR live `touring gate-metrics -j` counters. No invented bottlenecks. Two parallel sub-agents profiled pre_read/hook_runtime and decompose/enrichment/moka; their evidence is integrated and re-verified.

## Verdict

**Architecturally elite caching + bounded sandbox; one systemic hot-path latency offender.** 0 Critical · 3 High · 5 Medium · 4 Low.

The headline is real and measured: **`hook_dispatch_latency` p99 = 199 ms, p99.9 = 566 ms, max = 992 ms** over 5,768 live samples — on the editor's critical path, where the design target is <50 ms. The root cause is a **time-unbounded full-project `AnalysisPipeline::run()` executed synchronously** on both the `post_edit` and `pre_read` response paths. The `budget_ms` field that should bound it is **declared but never consulted** in the sequential `run()` orchestration. Everything around it — moka caches, the CEG sandbox pool, the hdrhistogram metrics, the ANN search — is genuinely best-in-class.

### Live latency evidence (`touring gate-metrics -j`, this session)

| Counter | count | p50 | p90 | p99 | p99.9 | max | Verdict |
|---|---|---|---|---|---|---|---|
| **`hook_dispatch_latency`** (µs) | 5,768 | 1,508 | 25,167 | **199,295** | 565,759 | **992,255** | ❌ **p99=199ms on critical path** |
| `rkyv_dispatch_latency` (µs) | 27 | 6,611 | 144,127 | 322,559 | 322,559 | 322,559 | ⚠ small-n, wide tail |
| `ann_search_latency` (µs) | 0 | 0 | 0 | 0 | 0 | 0 | — not exercised (claim p99=1µs **unconfirmed-live**, but design is sound — see VF) |
| `tantivy_query_latency` (µs) | 0 | — | — | — | — | — | not exercised this session |
| `memory_rss_mb` | — | — | — | — | — | **1,302** | ⚠ 1.3 GB daemon RSS |
| `query_cache_hit_ratio` | 0.0 (0 hit / 9 miss) | — | — | — | — | — | cold-start artifact, not a bug (see F2.9) |
| `enrichment_emit_count` / mean bytes | 668 / 1,725 | — | — | — | — | — | lean emit path (VF) |
| `ceg_fast_path_count` / captured | 974 / 974 | — | — | — | — | — | 100% pure-fast-path (VF) |

The p50 (1.5 ms) is healthy — most dispatches are fast. The **p90 jump to 25 ms and p99 to 199 ms** is the signature of an occasional unbounded operation (the full-project analysis fires on code-file edits/reads, not on every dispatch), exactly matching the offender below.

---

## Findings

### P-1 [Critical-adjacent / **High**] — `AnalysisPipeline::run()` is time-unbounded on the synchronous hook response path (F2.10)

**This is the worst latency offender and the root cause of the p99=199ms / max=992ms tail.**

`post_edit.rs:317-323` runs a full-project code-health analysis **synchronously**, inline in the hook's reply:
```rust
let post_health: Option<touring_analysis::CodeHealthReport> = Some(
    touring_analysis::AnalysisPipeline::new(
        runtime.ctx.knowledge.conn_ref(),
        touring_analysis::engine::AnalysisConfig::hook_path(),
    )
    .run(runtime.project_root.to_str().unwrap_or("")),   // <-- whole-project, no time gate
);
```
The **identical pattern** is in `pre_read.rs:503-512` (the sub-agent confirmed it is the heaviest op in pre_read, and the code's own comment at `pre_read.rs:499-500` flags it as "heavier than `analyze_knowledge()`").

**Why it is unbounded** — the `hook_path()` config *declares* a budget but the sequential `run()` **never reads it**:
- `engine.rs:28-38` — `hook_path()` sets `budget_ms: Some(40)`, `quality_sample: 1`, `cross_crate: false`. The doc comment promises "<50ms total."
- `pipeline.rs:414-417` — `pub fn run()` calls `run_common_dimensions()` then measures `total_ms` **after the fact**. There is **no budget check** in `run()` or `run_common_dimensions()` (verified: `sed -n '378,455p' pipeline.rs | grep budget` → empty).
- `pipeline.rs:383` — `run_common_dimensions` unconditionally calls `run_wiring(project_root)` → `wiring/mod.rs:59 analyze_wiring(conn)` → `count_orphans(conn)` + `analyze_chains(conn)`, both **full-DB scans with no budget**.
- `budget_ms` is honored in exactly **two** leaf modules: `blast_radius/mod.rs:186` and `quality/mod.rs:275`. But `run()` does not invoke blast on the hook path (`run_wiring` is the path taken), so the cap never engages.
- There is a budgeted alternative — `analyze_wiring_incremental` (`wiring/mod.rs:136`, `LIMIT 5000` + fingerprint store) — that the hook path **does not use**.

**Impact**: each code-file edit/read pays an un-time-capped wiring + quality scan over `knowledge.db`. As the DB grows (orphan triage in Phase 1 found raw 4,823 pub symbols across the project), this scan grows, and so does the p99/max. This is the dominant contributor to the 199ms p99 and 992ms max.

**Fix** (two layers):
1. **Enforce the declared budget in `run()`** — make `run_common_dimensions` deadline-aware:
   ```rust
   pub fn run(&self, project_root: &str) -> CodeHealthReport {
       let start = std::time::Instant::now();
       let deadline = self.config.budget_ms.map(|ms| start + Duration::from_millis(ms));
       let mut dims = Vec::with_capacity(4);
       dims.push(self.run_wiring_incremental_or_capped(project_root, deadline)); // use the LIMIT 5000 path
       if deadline.is_none_or(|d| Instant::now() < d) { /* quality */ }
       // ...skip remaining dims once the deadline is passed, return partial report
   }
   ```
2. **Better: move it off the response path entirely.** `pre_read.rs:233-241` already has a fire-and-forget `handle.spawn` precedent (used for `record_access`), and `post_edit.rs:529` uses `handle.spawn(async move { adb.record_edit(...).await })` for the async knowledge write. Apply the same: compute `post_health` in a debounced background task, cache the last composite in `SessionBus`, and have the hook read the cached value (sub-µs). The editor reply no longer waits on a project-wide scan.

---

### P-2 [High] — `cargo mutants` subprocess spawned on every non-test code edit, ungoverned by any concurrency cap (F2.10 / F2.13)

`post_edit.rs:269-281` spawns a `cargo mutants` subprocess on **every** edit of a non-test file:
```rust
let job_id = crate::shared::job_registry::spawn_worker(
    "cargo-mutants", "cargo",
    &["mutants".into(), "--in-diff".into(), "--timeout-multiplier=2.0".into()],
);
```
`spawn_worker` (`job_registry.rs:187-238`) **unconditionally** `rt_handle.spawn`s a new `tokio::process::Command` — there is **no `Semaphore`, no concurrency cap, no rate-limit/debounce**. Compare to the CEG `ExecPool` (`touring-ceg/src/gateway/exec_pool.rs:39` — tokio `Semaphore` with `max_concurrent` clamped + acquire-timeout): the job_registry path has none of that governance.

The post_edit code *does* serialize per-file (lines 217-281: it polls the previous `__mutants_job__:<path>` before spawning a replacement), so editing the **same** file in a loop won't pile up. But editing **N different** files in quick succession spawns **N concurrent `cargo mutants` processes**, each of which compiles and runs the test harness. This:
- saturates CPU, directly competing with the daemon's own hook-dispatch threads → inflates the very `hook_dispatch_latency` p99 we are trying to fix;
- compounds P-3 (each spawn inserts an unbounded `JobState`).

**Fix**: route mutant spawns through a global `Semaphore` (1–2 permits — mutation testing is not latency-sensitive) the way the CEG `ExecPool` does; or gate behind a workspace-wide debounce so an editing burst coalesces into one `--in-diff` run. `cargo mutants` on a hot edit loop is heavy enough that a single-permit serialization is the right default.

---

### P-3 [High] — `JOB_REGISTRY` is unbounded-by-design with no daemon-level sweep (F2.8 — memory)

`job_registry.rs:128` — `static JOB_REGISTRY: OnceLock<Arc<DashMap<String, JobState>>>`. The module doc (`job_registry.rs:24-35`) explicitly states the map is intentionally **not** moka-evicted (a TTL/capacity eviction would drop a `JoinHandle` without aborting the task), and that "jobs remain in the registry indefinitely until `poll_worker` retrieves a terminal state AND the caller invokes `drop_job`."

A `gc` helper is documented as "provided" (`job_registry.rs:35`) — **but I could not find any daemon-level caller**. Grep across `touring-server/server/mod.rs` and `touring-dispatch/daemon.rs` for a job sweep returned empty; the only periodic sweeps are for the **staging** registry (`touring-ceg/.../staging_registry.rs:269 gc`) and the **transcript miner** (`server/mod.rs:1154/1182 miner.sweep`), neither of which touches `JOB_REGISTRY`.

**Impact**: each `Completed`/`Failed` job holds its full captured stdout `String` (`job_registry.rs:74-76`). Combined with P-2 (every code edit spawns a mutants job), a long-lived daemon accumulates one `JobState` (plus its stdout) per edit, for the whole process lifetime, unless something explicitly polls-then-drops it. This is a slow leak — plausibly a contributor to the **1.3 GB live RSS** (`memory_rss_mb=1302`), alongside legitimate mmap'd model/index state (`memory_virt_mb=16.5M`).

**Fix**: wire a periodic `JOB_REGISTRY` sweep into the daemon's existing maintenance loop (the same loop that calls `miner.sweep` at `server/mod.rs:1154`): for terminal jobs older than e.g. 5 minutes, `abort()` if still Running and `remove()`. The `started_at_secs` field (`job_registry.rs:71`) already exists for this. This preserves the "abort-able by exactly one caller" invariant (the sweep IS that caller).

---

### P-4 [Medium] — Redundant full-file `std::fs::read_to_string` (read 2-3× per Read) on the synchronous pre_read path (F2.10)

`pre_read.rs:195` reads the **entire** target file into a `String` synchronously, solely to substring-scan for a skip marker:
```rust
let skip_marker_detected = std::fs::read_to_string(file_path)
    .map(|content| content.contains("touring:skip") || content.contains("touring::skip"))
```
The same file is then read **again** at `pre_read.rs:1396` (`source_based_signals` → `read_to_string`) for `.rs`/`.ts` files, plus an `fs::metadata` at `pre_read.rs:918` — so a single code-file Read pays 2 full reads + 1 stat, unbounded by file size, all on the response path before enrichment even starts. A multi-MB file pays full read + UTF-8 validation + alloc, twice.

**Fix**: cap the skip-marker read to the first ~8 KiB (`File::take(8192)`), and thread the single `source` read from `source_based_signals` into the skip check to eliminate the double read. The warmed `global_cache()` / `FileParserCache` (already touched at `pre_read.rs:215`) can serve the content.

---

### P-5 [Medium] — Batch INSERT loop without an enclosing transaction (F2.7 — DB)

`cli_decompose_create` (`decompose.rs:486-595`) inserts subtasks one-by-one with no wrapping transaction:
- `decompose.rs:558` — `db.conn_ref().execute("INSERT OR REPLACE INTO decomposition_subtasks …")` per subtask, plus
- `decompose.rs:583 → log_event → :185` — a second `INSERT INTO decomposition_events` per subtask.

So an N-subtask DAG creation = **2N independent statements, each auto-committed** (SQLite implicit commit = one fsync per statement). The dominant cost is 2N fsyncs.

**Fix**: wrap the loop in `let tx = conn.transaction()?; … tx.commit()?` — collapses 2N fsyncs into 1 (typically 10–50× faster for batch insert). Independently, hoist the two SQLs to `conn.prepare_cached(...)` so the SQL parses once across iterations (P-9). *Note: the connection itself is correctly reused (`decompose.rs:456 &rt.ctx.knowledge`), hot `WHERE task_id=?1` queries are indexed (`decompose.rs:96-111`), and no `SELECT *` exists — this is the only real DB-perf gap.*

---

### P-6 [Medium] — `regex::Regex::new` compiled inside a loop, per call (F2.7 / D-rule A-pattern)

`PlanFactCheckerHandler::execute` (`enrichment.rs:698`, gated to `*.md` plan-doc edits):
```rust
for pattern in &path_re_patterns {              // enrichment.rs:721
    if let Ok(re) = regex::Regex::new(pattern) { // :722 — compiles 4 DFAs every call
```
Regex compilation (DFA construction) is the expensive part of the `regex` crate; doing it ×4 per call discards the compiled automaton each time. Fires on plan-doc writes (not every emit), hence Medium not High.

**Fix**: hoist to `static PATH_RES: LazyLock<[Regex; 4]> = LazyLock::new(|| [...]);` so the 4 DFAs compile once process-wide.

---

### P-7 [Medium] — `LatencyHistogram` mutex contention on the highest-frequency counter path (F2.11)

`gate_metrics.rs:36-37` — `pub struct LatencyHistogram { inner: Mutex<Histogram<u64>> }`. Every `record_us` (`:63`) takes a `std::sync::Mutex`. On the hot dispatch path this is recorded **5,768 times** (the `hook_dispatch_latency.count`). The code's own comment (`gate_metrics.rs:34`) flags the remedy: "migrate to `hdrhistogram::sync::SyncHistogram` (shardable)."

**Impact**: under concurrent multi-worker dispatch, every hook records its latency under a single global mutex — a serialization point that grows with worker count. Low absolute cost per record but it's on the literal hottest path and the metric meant to *measure* latency adds a small amount of it.

**Fix**: adopt `SyncHistogram` (sharded recorder + periodic merge) as the comment already prescribes, OR shard the histogram per worker and merge at snapshot time.

---

### P-8 [Low] — `Arc<Mutex<Option<Sender>>>` for a write-once value (F2.11)

`hook_runtime.rs:379-381` — `cmd_tx` is an `Arc<Mutex<Option<mpsc::Sender<...>>>>` set once at spawn (`set_cmd_tx`, `:402`) and read-only after (`cmd_tx()`, `:393` clones the Sender out under lock). A write-once value behind a `Mutex<Option<...>>` should be a lock-free `OnceLock<Sender>` / `ArcSwapOption`. Off the dispatch hot path (only `cli_inferlets_exec` reads it), so Low — but it carries needless poison-handling surface.

**Fix**: migrate to `OnceLock<mpsc::Sender<...>>`.

---

### P-9 [Low] — Re-prepare of identical SQL + unconditional DDL batch per `decompose create` (F2.7)

- `decompose.rs:558` / `:185` re-`prepare` the same SQL each loop iteration (subsumed by P-5's transaction fix; independently fixable via `prepare_cached`).
- `decompose.rs:457 → :65 ensure_decompose_tables` runs ~11 `CREATE … IF NOT EXISTS` statements on **every** `create` call. No-ops once tables exist, but still parse+execute. Gate behind a `OnceLock<bool>` per daemon lifetime.

---

### P-10 [Low] — `invalidate_by_path` matches by substring `contains`, not delimiter-aware (F2.9)

`query_cache.rs:269-275` (`touring-foundation`) — `invalidate_by_path` snapshot-iterates all keys and removes any where `k.contains(file_path)`. With short/common path fragments (editing `mod.rs`, or a path that is a substring of many cache keys) this can evict **more entries than the one file touched** (`foo.rs` matches a key embedding `foobar.rs`). Invisible today (60s TTL, low query volume, 0 invalidations this session) but under a busy edit loop it suppresses the hit ratio.

**Fix**: key on a normalized canonical path and match delimiter-aware (`contains("::{file_path}")` or segment equality) instead of raw `contains`.

---

### P-11 [Low / note only] — Frontend/wasm boundary (F2.12)

`touring-bindings/src/desktop/components/wiring_graph_viewer.rs:1` renders the wiring graph as SVG via the external `graphviz dot` CLI. Lower priority (desktop/web binding, not the daemon hot path), but it is the surface of the **known 50k-edge graphviz hang** (Phase 1 memory note: `dot`/`sfdp` exhibit super-linear behavior; size-capped at 2 MiB → DOT-fallback today). No additional O(n²) wasm-boundary cliff found in the binding beyond that already-mitigated one. The `is_graphviz_available()` guard (`:116`) degrades gracefully. **Note only — no action this phase.**

---

## Scalability (F2.13)

- **Daemon is a singleton (intentional SPOF, correctly guarded).** `touring-dispatch/daemon.rs:554-555` holds an `flock(2)` for the daemon lifetime; `:633 UnixListener::bind` owns the socket. This is the documented single-writer model (REGRA #19). Acceptable for a local code-intelligence daemon — but it means **all hook latency is serialized through one process**, so P-1/P-2 (CPU-heavy work stealing dispatch threads) directly degrade *every* session's editor latency. The async accept loop (`tokio::net::UnixListener`) is non-blocking, so concurrency is fine at the I/O layer; the risk is CPU contention from un-governed background work, not socket throughput.
- **O(n²) cliffs**: the graphviz one (P-11) is the only confirmed super-linear cliff, already size-capped. The `AnalysisPipeline::run()` whole-project scan (P-1) is O(symbols) per dispatch and grows with the DB — not O(n²), but unbounded-per-edit, which is the practical scalability ceiling on a large repo.
- **In-process state growth**: `JOB_REGISTRY` (P-3) is the one unbounded long-lived collection. All other long-lived caches are bounded (see VF). `record_span_layer` (`hook_runtime.rs:752`) appends per-hop timing to a per-request `SpanContext` reset each dispatch — **not** accumulating across requests (verified: created on pre_read entry, scoped to the request).

---

## Already elite (verified-fast — do NOT regress)

These were claimed elite and I **confirmed with evidence**:

| Area | Evidence | Verdict |
|---|---|---|
| **CEG `ExecPool` bounded pool** | `touring-ceg/gateway/exec_pool.rs:39` tokio `Semaphore`, `max_concurrent` clamped `MIN..MAX` (`:86`), `acquire_timeout`, evictions counted, "silently dropped and never unbounded" (`:23`) | ✅ best-in-class concurrency governance |
| **CEG `DryRunCache`** | `touring-ceg/gateway/dry_run_cache.rs:34` `moka::sync::Cache`, `max_capacity` clamped to ceiling (`:67`), TTL, eviction counter (`:114-168`) | ✅ elite |
| **Centralized moka policies** | `touring-foundation/moka_policies.rs:30-118` — every cache has explicit `max_capacity` + `time_to_live` + `time_to_idle` + weighers (knowledge 4096/120s/60s-TTI; tantivy 1024/30s hit-weighted; terminal-job 32 MiB byte-weighted) | ✅ elite — textbook bounded caching |
| **`query_cache` (the hot read cache)** | `touring-foundation/query_cache.rs` — bounded 4096 (`:41-49`), TTL 60s (`:42-50`), **single-flight stampede protection** via `get_with` (`:114`, proven by 16-thread test `:343`), event-driven `invalidate_by_path` from post_edit/post_write | ✅ elite (one substring nit → P-10) |
| **`LatencyHistogram` = real hdrhistogram** | `gate_metrics.rs:18,50` `hdrhistogram::Histogram::new_with_bounds(1, 60_000_000, 3)` — fixed memory, true p99/p99.9 (this is why the headline numbers are trustworthy) | ✅ real percentile guards (one mutex nit → P-7) |
| **ANN HNSW search** | `touring-simd/ann/hnsw.rs:22,53` bounded `ef_search` (20–50), O(ef·log N) search; latency wired at `cli/memory.rs:93 record_ann_search_latency_us` | ✅ design is sound; claimed p99=1µs **plausible but unconfirmed-live** (count=0 this session) |
| **CEG fast-path** | live `ceg_fast_path_count=974 / ceg_captured_count=974` — 100% of captured executions took the pure-skip fast path (`is_provably_pure`); 0 sandboxed, 0 blocked | ✅ pure-code shortcut working perfectly |
| **hook_runtime concurrency** | sub-agent verified **0 lock-across-await**: `cmd_tx`/`ctx_tx` clone-then-drop guard before await (`hook_runtime.rs:393`, `inferlets.rs:153`); `aco_wiring.lock()` (`:1240`) in sync fn; `cortex_rx` Mutex never `.lock()`'d; per-dispatch DB opens are `init_*`-only (idempotent guard `:1134`), pooled via deadpool after | ✅ prior "0 lock-across-await" claim **confirmed** |
| **DB connection model** | `decompose.rs` reuses `&rt.ctx.knowledge` long-lived connection; no `Connection::open` per-call; hot `WHERE` clauses indexed (`:96-111`); no `SELECT *` | ✅ correct |
| **Enrichment emit path** | sub-agent verified `enrichment.rs` sync handlers are budget-gated (`:100 <80`, `:194 <100`), `.take(8)`/`.take(3)` bounded, knowledge reads hit KNOWLEDGE_EXTENDED moka one layer down, no blocking fs, no large clones | ✅ lean (one regex-in-loop → P-6) |

---

## Severity roll-up

| Sev | # | Findings |
|---|---|---|
| Critical | 0 | — |
| **High** | 3 | P-1 (unbounded `AnalysisPipeline::run` on response path), P-2 (ungoverned `cargo mutants` spawn), P-3 (unbounded `JOB_REGISTRY`) |
| Medium | 5 | P-4 (redundant full-file reads), P-5 (no batch txn), P-6 (regex-in-loop), P-7 (histogram mutex), — |
| Low | 4 | P-8 (Mutex for write-once Sender), P-9 (re-prepare + DDL batch), P-10 (substring invalidate), P-11 (graphviz, note-only) |

## The one fix that matters most

**P-1 + P-2 + P-3 are the same story**: heavy work (project-wide analysis + mutation subprocesses) runs un-budgeted/un-governed on or alongside the synchronous hook response path, and its byproducts accumulate unbounded. Fixing P-1 alone (enforce `budget_ms` in `pipeline.rs::run` and/or move the analysis to the existing fire-and-forget `handle.spawn` pattern) should collapse the **p99 from 199ms toward the design <50ms target** — the single highest-ROI optimization in the workspace. P-2's `Semaphore` and P-3's daemon sweep remove the CPU-contention and memory-leak that amplify it.
