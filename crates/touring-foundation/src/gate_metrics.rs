//! L7-B Gamma: Enrichment gate metrics — counts fast-path vs full-enrichment
//! invocations of pre_edit and pre_write hooks.
//!
//! Measures how often the CILA-gated `should_enrich` policy actually skips
//! expensive enrichment (DB queries, AST analysis, signal pipeline). Enables
//! empirical tuning of the CILA threshold and proves the ROI of the gate.
//!
//! # Overhead
//!
//! - `AtomicU64::fetch_add(Ordering::Relaxed)` — ~1ns per increment
//! - No locks, no allocation, lock-free per-thread scaling
//!
//! # Exposure
//!
//! Snapshot exposed via CLI `touring gate-metrics` and via `status -j` under
//! `gate_metrics` key.

use hdrhistogram::Histogram;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Latency histogram wrapped in a `Mutex` for cross-thread recording.
///
/// Upgrades raw `AtomicU64` counters (sum + count → mean) to proper
/// distribution tracking (P50 / P90 / P99 / P999 / max). Records in
/// microseconds; configured for a 1μs–60s range at 3 significant digits
/// (~2KB memory per histogram).
///
/// # Contention
///
/// The mutex is held only for the duration of `record` (O(1)) or
/// `snapshot` (O(log n) for percentiles). Under steady daemon load
/// (~1M records/s worst case) this is not a bottleneck. If it becomes
/// one, migrate to `hdrhistogram::sync::SyncHistogram` (shardable).
#[derive(Debug)]
pub struct LatencyHistogram {
    inner: Mutex<Histogram<u64>>,
}

impl LatencyHistogram {
    /// Create a new histogram covering 1μs–60s at 3 sigfigs.
    ///
    /// Panics only if the `hdrhistogram` crate rejects the range — which
    /// it cannot for these hard-coded constants. Marked `must_use` so
    /// accidental discards surface in code review.
    #[must_use]
    pub fn new() -> Self {
        // 1..=60_000_000 covers 1μs to 60s — wide enough for any hook
        // that should NOT be timed out by the daemon's 15s/300s budgets.
        let h = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)
            .expect("hdrhistogram bounds are always valid");
        Self {
            inner: Mutex::new(h),
        }
    }

    /// Record a single observation in microseconds.
    ///
    /// Values outside the `[1, 60_000_000]` range are clamped to the
    /// nearest bound — preserves monotonicity of `count` across any
    /// caller, even those recording garbage.
    #[inline]
    pub fn record_us(&self, micros: u64) {
        let clamped = micros.clamp(1, 60_000_000);
        if let Ok(mut h) = self.inner.lock() {
            // Infallible once clamped; use saturating record for safety
            // against a poisoned mutex recovery path.
            let _ = h.record(clamped);
        }
    }

    /// Capture a snapshot of percentiles and total count.
    ///
    /// Returns an empty `LatencySnapshot` (all zeros) when the histogram
    /// has no recorded values — matching the `AtomicU64` convention.
    #[must_use]
    pub fn snapshot(&self) -> LatencySnapshot {
        let h = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let count = h.len();
        if count == 0 {
            return LatencySnapshot::default();
        }
        LatencySnapshot {
            count,
            p50_us: h.value_at_quantile(0.50),
            p90_us: h.value_at_quantile(0.90),
            p99_us: h.value_at_quantile(0.99),
            p999_us: h.value_at_quantile(0.999),
            max_us: h.max(),
        }
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Immutable latency distribution snapshot.
///
/// All percentiles are in microseconds. `count` reflects total observations;
/// `max_us` is the largest single recorded value (useful for spike detection
/// that percentiles smooth over).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LatencySnapshot {
    /// Total number of observations recorded.
    pub count: u64,
    /// 50th-percentile (median) latency in microseconds.
    pub p50_us: u64,
    /// 90th-percentile latency in microseconds.
    pub p90_us: u64,
    /// 99th-percentile latency in microseconds.
    pub p99_us: u64,
    /// 99.9th-percentile latency in microseconds.
    pub p999_us: u64,
    /// Largest single recorded latency in microseconds.
    pub max_us: u64,
}

/// Global gate metrics singleton — lazy-initialized on first access.
///
/// Lifetime = daemon lifetime. Counters are monotonically increasing.
/// Reset only on daemon restart.
static GATE_METRICS: OnceLock<GateMetrics> = OnceLock::new();

/// Per-hook per-branch counters for enrichment gate observability.
///
/// Each counter uses `AtomicU64` with `Ordering::Relaxed` — safe for
/// monotonic counters where exact ordering between threads is irrelevant.
#[derive(Debug)]
pub struct GateMetrics {
    /// pre_edit hook invocations that took the fast-path (gate returned `false`).
    /// Incremented when cila_level < 2 OR enrichment_active == false OR tool not in mutation set.
    pub pre_edit_fast_path_count: AtomicU64,

    /// pre_edit hook invocations that ran the full enrichment (gate returned `true`).
    /// Incremented when `compose_edit_context` was actually called.
    pub pre_edit_full_enrichment_count: AtomicU64,

    /// pre_write hook invocations that took the fast-path.
    pub pre_write_fast_path_count: AtomicU64,

    /// pre_write hook invocations that ran the full enrichment pipeline.
    pub pre_write_full_enrichment_count: AtomicU64,

    /// post_tool_use hook invocations where L4+ mandatory enrichment triggered.
    /// Counts workflows that crossed the self-modifying threshold.
    pub post_tool_l4_mandatory_count: AtomicU64,

    /// Metadata cache hits (MetadataDedup returned a cached result).
    pub metadata_cache_hit_count: AtomicU64,

    /// Metadata backpressure drops (TokenBudget dropped a metadata enrichment).
    pub metadata_backpressure_dropped_count: AtomicU64,

    /// pre_grep / pre_glob hook invocations that emitted an enrichment block
    /// (D43, master plan W2 — Grep/Glob symbol enrichment).
    /// Incremented when the symbol_store returned ≥1 location for a symbol-like pattern.
    pub pre_grep_enrichment_count: AtomicU64,

    /// pre_grep / pre_glob hook invocations that fell through silently because
    /// the symbol_store returned 0 locations for a symbol-like pattern.
    /// Watchdog metric — high values mean the whitelist regex is too permissive
    /// (Gabriel can disable via `TOURING_DISABLE_PREGREP=1`).
    pub pre_grep_zero_results_count: AtomicU64,

    // ── Tantivy FTS counters (U21) ────────────────────────────────────────────
    /// Total upsert operations forwarded to the Tantivy FTS index.
    /// Monotonically increasing; incremented by `record_tantivy_upsert()` which
    /// is called from `TantivyIndex::upsert_symbol()` when the feature is active.
    pub tantivy_upsert_count: AtomicU64,

    /// Cumulative query latency in microseconds across all Tantivy search calls.
    /// Summed by `record_tantivy_query_latency(us)`. Divide by query call count
    /// (not exposed separately) to get average latency. Overflow-safe for days
    /// of operation at sub-millisecond query latency.
    pub tantivy_query_latency_us: AtomicU64,

    /// Total explicit commit operations on the Tantivy index.
    /// Incremented by `record_tantivy_commit()`. Includes both auto-commits
    /// (batch_size threshold) and explicit `commit()` calls.
    pub tantivy_commit_count: AtomicU64,

    // ── Incremental indexing failures (B2, 2026-05-10) ────────────────────────
    /// Number of `reindex_file` invocations from `post_edit`/`post_write` that
    /// returned `Err` instead of successfully updating the symbol index.
    ///
    /// Incremented by [`record_reindex_failure`] at the failure site in
    /// `post_edit.rs`. Each increment also emits a `tracing::warn!` (promoted
    /// from `debug!` in B2) so the failure is visible without `RUST_LOG=debug`.
    ///
    /// **Watchdog semantics**: a non-zero value here while the editor session
    /// is actively writing files indicates the incremental index is drifting
    /// behind reality — callers should treat `touring index find` results as
    /// stale until a `touring index rebuild` or `touring index ingest <file>`
    /// completes successfully.
    pub reindex_failure_count: AtomicU64,

    // ── rkyv IPC counters (Wave 3 D1) ─────────────────────────────────────────
    /// Number of inbound daemon requests dispatched through the rkyv
    /// zero-copy parse path (feature `rkyv-ipc`). Incremented *after*
    /// `check_archived_root` succeeds — failures are counted separately
    /// in `rkyv_parse_error_count`.
    pub rkyv_dispatch_count: AtomicU64,

    /// Cumulative body byte count of successful rkyv dispatches (excluding
    /// the 8-byte frame header). Divide by `rkyv_dispatch_count` for the
    /// mean payload size — useful for sizing buffer pools.
    pub rkyv_dispatch_bytes: AtomicU64,

    /// Number of rkyv frames rejected: bad magic, truncated, length
    /// mismatch, or bytecheck failure. A non-zero value under steady load
    /// is a wire-level bug signal.
    pub rkyv_parse_error_count: AtomicU64,

    /// Number of outbound responses emitted in rkyv framed form (Wave 3 D4).
    /// Matches `rkyv_dispatch_count` 1:1 when response migration is active.
    pub rkyv_response_count: AtomicU64,

    // ── Latency histograms (2026-04-17) ───────────────────────────────────
    //
    // Upgrade from `AtomicU64` sum/count (which only exposes mean) to full
    // distributions (P50 / P90 / P99 / P999 / max) for the three hottest
    // paths in the daemon. Enables tail-latency alerts and empirical
    // validation of the "P50 = 1ms" invariant.
    /// Per-dispatch latency of rkyv IPC parse + hook execution + response
    /// framing (microseconds). Complements `rkyv_dispatch_count` with a
    /// distribution — same counter, richer signal.
    pub rkyv_dispatch_latency: LatencyHistogram,

    /// Per-request latency of the actor-thread hook dispatch, measured
    /// from `ProjectCommand::RunHook` receipt to oneshot response. Caps
    /// the 15s light / 300s heavy budgets in `daemon.rs`.
    pub hook_dispatch_latency: LatencyHistogram,

    /// Per-query latency of Tantivy search / fuzzy / suggest calls.
    /// Complements the cumulative `tantivy_query_latency_us` sum with the
    /// distribution — detects outlier queries that cumulative hides.
    pub tantivy_query_latency_hist: LatencyHistogram,

    // ── Actor queue observability (2026-07-01) ────────────────────────────
    /// Time a `ProjectCommand::RunHook` spent parked in the per-project
    /// actor's bounded mpsc (enqueue → dequeue). `hook_dispatch_latency`
    /// only starts at dequeue, so serialized-actor backpressure — a heavy
    /// hook from one session stalling another session's hooks in the same
    /// project — was invisible before this histogram. Values above the 60s
    /// histogram bound are clamped (`max_us == 60_000_000` ⇒ saturated).
    pub actor_queue_wait: LatencyHistogram,

    /// Dispatches that gave up waiting for the actor's oneshot reply —
    /// the handler exceeded its budget (15s light / 300s heavy).
    pub actor_budget_timeout_count: AtomicU64,

    /// Dispatches dropped on the send side — the actor's bounded queue
    /// stayed full past `REQUEST_TIMEOUT` (queue-full backpressure).
    pub actor_send_timeout_count: AtomicU64,

    // ── Wave 12 (2026-04-18) — Health Delta observability ─────────────────
    //
    // Counts the four meaningful events of the cross-hook health_delta
    // bridge introduced in Wave 9 and wired in Waves 10/11:
    //
    // - `health_delta_record_count`: pre_edit/pre_write recorded an old
    //   health score into the cache. High values indicate the bridge is
    //   actively populated.
    // - `health_delta_compute_count`: post_edit consumed a cache entry
    //   and emitted a `HealthDelta`. Should track `record_count`
    //   asymptotically; large divergence signals leaks (record without
    //   compute) or daemon restarts.
    // - `health_delta_regression_count`: number of computes that flagged
    //   `is_regression()` (delta ≤ -0.05). Drift detection input.
    // - `health_delta_improvement_count`: number of computes that flagged
    //   `is_improvement()` (delta ≥ +0.05). Positive feedback signal.
    /// Pre-record events into `HealthDeltaCache` (Wave 9 bridge).
    pub health_delta_record_count: AtomicU64,

    /// Post-compute events that produced a `HealthDelta`.
    pub health_delta_compute_count: AtomicU64,

    /// Compute events whose delta was a regression (≤ -0.05).
    pub health_delta_regression_count: AtomicU64,

    /// Compute events whose delta was an improvement (≥ +0.05).
    pub health_delta_improvement_count: AtomicU64,

    /// Wave 13 — Regression streak alerts (consecutive regression
    /// transitions that crossed the alert threshold of 3).
    /// Each crossing increments once; the streak does not re-arm
    /// until an improvement (or non-regression delta) breaks it.
    pub health_delta_streak_alert_count: AtomicU64,

    /// Wave 13 — Recovery events (improvement after a regression
    /// streak >= 1). Tracks how often CC's edits actually undo prior
    /// degradations, complementing `improvement_count` with directionality.
    pub health_delta_recovery_count: AtomicU64,

    /// Wave 17 — Query result cache hits (cli_index_find, cli_tantivy_search,
    /// etc.). Drives the cache hit ratio metric: hits / (hits + misses).
    pub query_cache_hit_count: AtomicU64,

    /// Wave 17 — Query result cache misses (compute path executed).
    pub query_cache_miss_count: AtomicU64,

    /// Wave 18 — Cumulative entries invalidated by path-scoped invalidation
    /// (post_edit / post_write removing stale entries for an edited file).
    pub query_cache_invalidate_count: AtomicU64,

    // ── Predictive Wave (2026-04-20) — D2/D3/D4 observability ─────────────
    //
    // Counts the three predictive vectors introduced by the predictive wave:
    //   D2 (Blast): HNSW blast radius injection at PreToolUse[Task*].
    //   D3 (LinUCB): contextual bandit routing hints at TaskList.
    //   D4 (MCTS):   shadow rollout + Tarjan SCC at EnterPlanMode.
    //
    // All are advisory (never block exit 0). Counters measure how often
    // each path fires, how often it times out, and how often it produces
    // an actionable mutation / hint / deadlock signal.
    /// D2 — blast radius compute invocations at `PreToolUse[Task*]`.
    pub blast_inject_count: AtomicU64,

    /// D2 — blast radius computes that exceeded their timeout budget.
    pub blast_timeout_count: AtomicU64,

    /// D2 — PreToolUse responses that mutated the tool input with an
    /// injected `[TOURING-INJECT]` subject prefix.
    pub blast_mutation_count: AtomicU64,

    /// D3 — LinUCB routing decisions where the winning arm was ManualEdit.
    pub linucb_route_manual_count: AtomicU64,

    /// D3 — LinUCB routing decisions where a Generator* arm won.
    pub linucb_route_generator_count: AtomicU64,

    /// D3 — TaskList responses that emitted a `[TOURING RL-ROUTER]` hint.
    pub linucb_route_hint_count: AtomicU64,

    /// D4 — MCTS shadow rollouts actually executed (CilaLevel gate passed).
    pub mcts_shadow_run_count: AtomicU64,

    /// D4 — MCTS shadow rollouts that exceeded their budget (returned None).
    pub mcts_shadow_timeout_count: AtomicU64,

    /// D4 — MCTS shadow rollouts that detected a deadlock cycle via Tarjan SCC.
    pub mcts_shadow_deadlock_detected_count: AtomicU64,

    // ── Wave 22 — ANN search latency histogram ────────────────────────────
    //
    // Records per-search latency for the SQL+ANN hybrid recall path in
    // `cli_memory_recall`. Enables tail-latency visibility for the ANN
    // search component specifically — distinct from the full handler latency
    // tracked by `hook_dispatch_latency`.
    /// Per-query latency of ANN (HNSW) vector search in microseconds.
    ///
    /// Recorded in `cli_memory_recall` when `ann_recall` is `Some`.
    /// Provides a distribution over the HNSW k-NN search time — expected
    /// P50 < 500μs for the 64-dim U4-quantized index at typical sizes.
    pub ann_search_latency: LatencyHistogram,

    // ── Tantivy Stream Actor (Suggestion 2 — 2026-04-20) ──────────────────
    //
    // Observability for the async streaming upsert pipeline introduced by
    // `shared::tantivy_stream`. The actor drains a bounded mpsc channel and
    // batches commits (≥5,000 docs or every 2s).
    /// Docs successfully enqueued to the stream actor channel.
    pub tantivy_stream_enqueued_count: AtomicU64,
    /// Docs dropped due to full channel (actor is falling behind — backpressure).
    pub tantivy_stream_backpressure_drop_count: AtomicU64,
    /// Flush (commit) operations executed by the stream actor.
    pub tantivy_stream_flush_count: AtomicU64,
    /// Total docs flushed across all commits. Divide by flush_count for mean batch size.
    pub tantivy_stream_flush_docs_count: AtomicU64,

    // ── MCTS Prefetch Actor (Suggestion 3 — 2026-04-20) ───────────────────────
    //
    // Observability for the predictive file-cache warming pipeline introduced by
    // `shared::file_prefetch`. After each MCTS shadow rollout, predicted file paths
    // are enqueued; the actor warms the global parser cache via spawn_blocking.
    /// File paths enqueued to the prefetch actor after MCTS shadow rollout.
    pub prefetch_enqueued_count: AtomicU64,
    /// Files successfully warmed into the global parser cache by the prefetch actor.
    pub prefetch_warmed_count: AtomicU64,

    // ── JDM Routing (Grupo 5 §1 — 2026-04-21) ────────────────────────────
    //
    // Observability for the Joint Domain Mapper routing hint emitted on
    // TaskCreate. Classifies task subjects into execution classes A–D and
    // tracks which class was selected most frequently.
    /// TaskCreate subjects classified as class-A (code implementation).
    pub jdm_class_a_count: AtomicU64,
    /// TaskCreate subjects classified as class-B (infra/devops).
    pub jdm_class_b_count: AtomicU64,
    /// TaskCreate subjects classified as class-C (cognitive/architecture).
    pub jdm_class_c_count: AtomicU64,
    /// TaskCreate subjects classified as class-D (multi-agent/orchestration).
    pub jdm_class_d_count: AtomicU64,

    // ── Tokio Runtime Metrics (Wave 26 — 2026-04-21) ────────────────────
    //
    // Observability for the Tokio multi-threaded work-stealing scheduler.
    // Exposed via `touring metrics -j` and `touring status -j`. Enables
    // auto-diagnosis of backpressure, worker undersizing, and blocking
    // pool saturation without external instrumentation.
    //
    // Diagnostic rules:
    // - num_idle_threads > 0 AND injection_queue_depth > 0 → backpressure downstream
    // - num_idle_threads == 0 sustained → workers undersized
    // - num_blocking_threads == max → blocking pool saturated
    /// Tokio scheduler worker threads (all Tokio multi-thread runtimes have at least 1).
    pub tokio_num_workers: AtomicU64,

    /// Tokio worker threads currently idle (parked, no work available).
    /// Non-zero with injection_queue_depth > 0 indicates downstream backpressure.
    pub tokio_num_idle_threads: AtomicU64,

    /// Tokio blocking threads (spawn_blocking pool). Saturates when
    /// many long-running CPU-bound tasks are queued.
    pub tokio_num_blocking_threads: AtomicU64,

    /// Requests pending injection onto worker queues across all threads.
    /// Non-zero with idle threads > 0 → workers blocked on a slow handler.
    pub tokio_injection_queue_depth: AtomicU64,

    // ── RFC-100 Diagnostic Code Counters (2026-04-25) ─────────────────────
    //
    // Track how often specific RFC-100 diagnostic codes are emitted by the
    // daemon. Enables empirical measurement of code-quality issue prevalence
    // across sessions without requiring log scraping.
    //
    // - W-1xx: wiring findings (orphan, unused pub, dead chain)
    // - Q-201: TDG grade F (composite < 0.40) — emitted as Error severity
    // - Q-202: TDG grade D (composite 0.40–0.55) — emitted as Warning severity
    /// RFC-100 diagnostic code emitted — wiring finding (W-1xx codes).
    ///
    /// Incremented each time `to_diagnostic_opt()` fires for a wiring orphan,
    /// unused pub symbol, or dead functional chain. Drives W-1xx prevalence
    /// metrics in `touring gate-metrics -j`.
    pub diagnostic_wiring_finding_emitted_count: AtomicU64,

    /// RFC-100 diagnostic code emitted — TDG grade warning/error (Q-201/202).
    ///
    /// Incremented each time `cli_ast_tdg` computes a TDG grade of D or F,
    /// triggering a diagnostic. Drives Q-2xx prevalence metrics in the
    /// `touring gate-metrics -j` snapshot.
    pub diagnostic_tdg_emitted_count: AtomicU64,

    /// RFC-100 diagnostic code emitted — B-302 PatchExpansion warning.
    ///
    /// Incremented each time `cli_mpatch_preview` (or any caller of
    /// `pre_write::emit_b302_if_low_confidence_expansion`) detects an
    /// mpatch fuzzy expansion with confidence below the 0.7 threshold and
    /// emits a B-302 RFC-100 diagnostic. Drives B-3xx prevalence metrics
    /// in the `touring gate-metrics -j` snapshot. Wave 12 (2026-04-27).
    pub diagnostic_b302_emitted_count: AtomicU64,

    /// CC→Touring task-sync: mirror container created by `bridge_task_created`.
    /// F0-pre (2026-07-20) — the sync chain previously had ZERO observability,
    /// which let double-create + create-only sync rot silently for months.
    pub task_sync_create_count: AtomicU64,

    /// CC→Touring task-sync: `bridge_task_created` reached with an existing
    /// mirror (the second sync path arriving) — deduped, no container minted.
    pub task_sync_deduped_count: AtomicU64,

    /// CC→Touring task-sync: a TaskUpdate status change was propagated onto the
    /// mirror container by `handle_task_sync_post_update`.
    pub task_sync_update_propagated_count: AtomicU64,

    /// RFC-100 diagnostic code emitted — Q-220 NonIdempotentFormat.
    ///
    /// Incremented each time `idempotency::check_idempotency` detects that
    /// `format(format(x)) != format(x)` for a proposed Rust edit in
    /// `pre_edit`. The edit is downgraded by 0.3 and Q-220 is emitted.
    /// Drives Q-220 prevalence metrics in the `touring gate-metrics -j`
    /// snapshot. Wave A.3 (2026-04-29).
    pub diagnostic_q220_nonidempotent_emitted_count: AtomicU64,

    /// RFC-100 diagnostic code emitted — W-115 SkippedRegionWritten.
    ///
    /// Incremented each time `post_edit` detects that an Edit tool write
    /// overlapped a `// touring:skip-region` … `// touring:skip-end` frozen
    /// region. Non-blocking warning — edit is allowed to proceed.
    /// Drives W-115 prevalence metrics in the `touring gate-metrics -j`
    /// snapshot. Wave A.2.4 (2026-04-29).
    pub diagnostic_w115_skipped_region_written_count: AtomicU64,

    // ── ESAA activity hook counters (v8 D1.6) ────────────────────────────────
    //
    // Observability for ESAA event-sourcing activity events fired by 5 hooks:
    // pre_edit, post_edit, post_write, instructions_loaded, pre_compact.
    // Each counter tracks how many times the corresponding activity event was
    // emitted via EventStore append (fire-and-forget — errors logged, not counted).
    /// pre_edit activity event emitted to activity.jsonl via EventStore.
    pub activity_pre_edit_count: AtomicU64,

    /// post_edit activity event emitted to activity.jsonl via EventStore.
    pub activity_post_edit_count: AtomicU64,

    /// post_write activity event emitted to activity.jsonl via EventStore.
    pub activity_post_write_count: AtomicU64,

    /// instructions_loaded activity event emitted at session_start.
    pub activity_instructions_loaded_count: AtomicU64,

    /// pre_compact activity event emitted before context compaction.
    pub activity_pre_compact_count: AtomicU64,

    // ── resource-monitor counters (W-540..W-543, 2026-05-02) ──────────────
    //
    // Observability for the PSI memory pressure sentinel (touring-foundation (sentinel)).
    // All counters are monotonically increasing; `_count` suffix follows the
    // convention established by health_delta_* and predictive wave counters.
    //
    // These fields are always present in `GateMetrics` regardless of whether
    // the `resource-monitor` feature is compiled in — the counters simply
    // remain at 0 when the feature is absent, which keeps the JSON shape
    // stable for downstream consumers (e.g. `touring gate-metrics -j`).
    /// Pressure ticks classified as Green (normal — no action needed).
    pub memory_pressure_green_count: AtomicU64,

    /// Pressure ticks classified as Yellow (elevated — warn only).
    pub memory_pressure_yellow_count: AtomicU64,

    /// Pressure ticks classified as Red (critical — heavy spawns paused).
    pub memory_pressure_red_count: AtomicU64,

    /// Total pressure ticks recorded (sum of green + yellow + red).
    /// Useful as a denominator for computing pressure distribution ratios.
    pub memory_pressure_total_tick_count: AtomicU64,

    /// Number of times swap thrashing was detected (W-542).
    /// Incremented by `record_pressure_tick` when pgmajfault exceeds threshold.
    pub swap_thrashing_detected_count: AtomicU64,

    /// Number of heavy spawns (cargo test/build, npm install, pip install)
    /// paused by the W-540 gate because pressure was Red.
    pub cargo_test_paused_count: AtomicU64,

    /// Number of successful P-core (performance core) affinity pinnings
    /// performed by CoreScheduler. Non-zero only when `resource-monitor`
    /// feature is active and the host CPU has a detected P/E topology.
    pub core_pinning_p_count: AtomicU64,

    /// Number of successful E-core (efficiency core) affinity pinnings
    /// performed by CoreScheduler.
    pub core_pinning_e_count: AtomicU64,

    // ── D2.4: PreToolUse output routing counters ─────────────────────────
    //
    // Observability for the D2 PreToolUse sandbox routing introduced in
    // context-mode integration (2026-05-08). Tracks how many tool invocations
    // were routed to sandbox execution and how many times sandbox fallback
    // was triggered due to timeout.
    /// Total tool invocations routed to sandbox execution (D2.4).
    ///
    /// Incremented when `check_tool_output_routing()` returns
    /// `RouteToSandbox` and the tool is actually executed in the sandbox
    /// subprocess (D2.2 sandbox_executor). Drives the `tool_output_routed_count`
    /// metric in `touring gate-metrics -j`.
    pub tool_output_routed_count: AtomicU64,

    /// Total sandbox executions that fell back to original tool execution
    /// due to timeout.
    ///
    /// Incremented when a sandbox execution exceeds its configured timeout
    /// (`TOURING_SANDBOX_TIMEOUT_MS`) and `TOURING_SANDBOX_FALLBACK_ON_TIMEOUT=true`.
    /// High values indicate sandbox performance issues or undersized timeout.
    pub sandbox_timeout_fallback_count: AtomicU64,

    // ── Wave 2026-05-08 Master Plan — Tantivy native upgrades ────────────
    /// I-02 — multi-term queries onde PhraseQuery casou no symbol_name field.
    /// Indica quão frequente o proximity boost contribui ao ranking.
    pub phrase_query_match_count: AtomicU64,
    /// I-01 — substring searches via NgramTokenizer trigram field.
    pub tantivy_trigram_query_count: AtomicU64,
    /// I-05 — ToolOutputsIndex stores skipped por content_hash fresh dentro do TTL.
    pub tool_outputs_ttl_skip_count: AtomicU64,
    /// I-05 — ToolOutputsIndex docs deletados pelo cleanup actor de retenção.
    pub tool_outputs_cleanup_deleted_count: AtomicU64,
    /// NEW-2 — Sandbox failures persisted via tee mode (exit_code != 0).
    pub sandbox_tee_persisted_count: AtomicU64,
    /// NEW-1 — Per-command compression profile applications (any profile).
    pub compression_profile_applied_count: AtomicU64,
    // ── Wave 3 INTELLIGENCE — 16 T1 counters ───────────────────────────────
    /// T1-01 ctx_replay invocations.
    pub ctx_replay_count: AtomicU64,
    /// T1-02 ctx_purge invocations.
    pub ctx_purge_count: AtomicU64,
    /// T1-03 ctx_doctor invocations.
    pub ctx_doctor_call_count: AtomicU64,
    /// T1-04 daily flush write count.
    pub gate_metrics_daily_flush_count: AtomicU64,
    /// T1-05 ctx_gain_graph invocations.
    pub ctx_gain_graph_count: AtomicU64,
    /// T1-06 ctx_session_adoption queries.
    pub ctx_session_adoption_query_count: AtomicU64,
    /// T1-07 touring init invocations.
    pub touring_init_invocation_count: AtomicU64,
    /// T1-08 ctx_smart invocations.
    pub ctx_smart_count: AtomicU64,
    /// T1-09 read aggressive chunking applied.
    pub read_aggressive_chunked_count: AtomicU64,
    /// T1-09 read passthrough (under threshold).
    pub read_aggressive_passthrough_count: AtomicU64,
    /// T1-10 ctx_explain invocations.
    pub ctx_explain_count: AtomicU64,
    /// T1-11 ctx_budget warnings (75%).
    pub ctx_budget_warning_emitted_count: AtomicU64,
    /// T1-11 ctx_budget alerts (90%).
    pub ctx_budget_alert_emitted_count: AtomicU64,
    /// T1-12 ctx_batch_execute invocations.
    pub ctx_batch_execute_count: AtomicU64,
    /// T1-12 ctx_batch_execute total items processed.
    pub ctx_batch_item_count: AtomicU64,
    /// T1-13 ctx_execute_file invocations.
    pub ctx_execute_file_count: AtomicU64,
    /// T1-14 ctx_upgrade invocations.
    pub ctx_upgrade_count: AtomicU64,
    /// T1-15 ctx_discover_session invocations.
    pub ctx_discover_session_count: AtomicU64,
    /// T1-15 missed opportunities surfaced.
    pub ctx_discover_opportunities_found: AtomicU64,
    // ── Wave 3 Extended (T2 + T3) — 25 unified counters ────────────────────
    /// Wave 3 Tier-2 feature T201 invocation counter.
    pub wave3_t201_count: AtomicU64,
    /// Wave 3 Tier-2 feature T202 invocation counter.
    pub wave3_t202_count: AtomicU64,
    /// Wave 3 Tier-2 feature T203 invocation counter.
    pub wave3_t203_count: AtomicU64,
    /// Wave 3 Tier-2 feature T204 invocation counter.
    pub wave3_t204_count: AtomicU64,
    /// Wave 3 Tier-2 feature T205 invocation counter.
    pub wave3_t205_count: AtomicU64,
    /// Wave 3 Tier-2 feature T206 invocation counter.
    pub wave3_t206_count: AtomicU64,
    /// Wave 3 Tier-2 feature T207 invocation counter.
    pub wave3_t207_count: AtomicU64,
    /// Wave 3 Tier-2 feature T208 invocation counter.
    pub wave3_t208_count: AtomicU64,
    /// Wave 3 Tier-2 feature T209 invocation counter.
    pub wave3_t209_count: AtomicU64,
    /// Wave 3 Tier-2 feature T210 invocation counter.
    pub wave3_t210_count: AtomicU64,
    /// Wave 3 Tier-2 feature T211 invocation counter.
    pub wave3_t211_count: AtomicU64,
    /// Wave 3 Tier-2 feature T212 invocation counter.
    pub wave3_t212_count: AtomicU64,
    /// Wave 3 Tier-2 feature T213 invocation counter.
    pub wave3_t213_count: AtomicU64,
    /// Wave 3 Tier-2 feature T214 invocation counter.
    pub wave3_t214_count: AtomicU64,
    /// Wave 3 Tier-2 feature T215 invocation counter.
    pub wave3_t215_count: AtomicU64,
    /// Wave 3 Tier-3 feature T301 invocation counter.
    pub wave3_t301_count: AtomicU64,
    /// Wave 3 Tier-3 feature T302 invocation counter.
    pub wave3_t302_count: AtomicU64,
    /// Wave 3 Tier-3 feature T303 invocation counter.
    pub wave3_t303_count: AtomicU64,
    /// Wave 3 Tier-3 feature T304 invocation counter.
    pub wave3_t304_count: AtomicU64,
    /// Wave 3 Tier-3 feature T305 invocation counter.
    pub wave3_t305_count: AtomicU64,
    /// Wave 3 Tier-3 feature T306 invocation counter.
    pub wave3_t306_count: AtomicU64,
    /// Wave 3 Tier-3 feature T307 invocation counter.
    pub wave3_t307_count: AtomicU64,
    /// Wave 3 Tier-3 feature T308 invocation counter.
    pub wave3_t308_count: AtomicU64,
    /// Wave 3 Tier-3 feature T309 invocation counter.
    pub wave3_t309_count: AtomicU64,
    /// Wave 3 Tier-3 feature T310 invocation counter.
    pub wave3_t310_count: AtomicU64,

    // ── CEG Pln2 FASE 5a — P7.1 counters ─────────────────────────────────────
    //
    // Observability for the Code Execution Gateway (CEG) X0..X7 pipeline.
    // All counters are monotonically increasing; `_count` suffix follows the
    // established convention.  They remain at 0 when the gateway is not used,
    // keeping the JSON shape stable.
    /// X0 CAPTURE — tool calls that entered the CEG pipeline.
    pub ceg_captured_count: AtomicU64,

    /// X7 DECISION — calls whose verdict was `Deny` (hard-blocked by gateway).
    pub ceg_blocked_count: AtomicU64,

    /// X5 SANDBOX — calls routed through the dry-run sandbox stage.
    pub ceg_sandboxed_count: AtomicU64,

    /// X6 CAPABILITY-GATE — calls that took the fast path (provably pure /
    /// no capability needed). Complement of `ceg_sandboxed_count`.
    pub ceg_fast_path_count: AtomicU64,

    /// X2 STATIC / X3 VGP — workflow anti-patterns detected (any severity).
    pub workflow_antipattern_detected_count: AtomicU64,

    /// X9 LEARN — advisory advice emitted for `Warn` decisions.
    pub workflow_advice_emitted_count: AtomicU64,

    /// X9 LEARN — anti-patterns converted: a previously-blocked pattern was
    /// subsequently allowed after the caller applied the canonical fix.
    pub antipattern_converted_count: AtomicU64,

    // ── TR-2 — STR (Signal-to-Token Ratio) observability ─────────────────────
    //
    // STR = (actionable signal) / (tokens injected into context).
    // Approximated here as bytes of `additionalContext` emitted by
    // hook-enrichment paths (cli_suggester, pre_grep, etc.).
    //
    // Observable STR proxy:
    //   mean_enrichment_bytes = enrichment_context_bytes_total / enrichment_emit_count
    //
    // A **lower** mean_enrichment_bytes indicates higher STR — more signal per
    // token injected.  Track via `touring gate-metrics -j` under the keys
    // `enrichment_context_bytes_total` and `enrichment_emit_count`.
    //
    // Reference: Rn2 §11 TR-2 — "Expor a métrica STR como sinal observável —
    // `touring gate-metrics` poderia rastrear tokens-injetados/operação".
    /// Cumulative bytes of `additionalContext` injected by enrichment hooks.
    ///
    /// Incremented on every non-empty context emission (cli_suggester, pre_grep,
    /// etc.).  Together with `enrichment_emit_count` this allows computing the
    /// mean bytes per injection — the inverse proxy for STR.
    pub enrichment_context_bytes_total: AtomicU64,

    /// Number of enrichment injections that emitted a non-empty context.
    ///
    /// Denominator for the STR proxy ratio.  Zero when no enrichment has fired
    /// in the current daemon lifetime (e.g. fresh restart with no tool calls).
    pub enrichment_emit_count: AtomicU64,

    /// **F2 (2026-06-28)** — coupling suggestion-uptake denominator: number of
    /// `cli_suggester` redirect suggestions emitted. Paired with
    /// `suggestion_uptake_followed_count` for `touring.coupling.suggestion_uptake`.
    pub suggestion_uptake_emitted_count: AtomicU64,

    /// **F2 (2026-06-28)** — coupling suggestion-uptake numerator: emitted
    /// suggestions whose next same-session action followed the redirect (a
    /// `touring`/code-mode command). `uptake = followed / emitted`.
    pub suggestion_uptake_followed_count: AtomicU64,

    /// **F3 (2026-06-28)** — coupling adoption_ratio numerator: `Bash` actions
    /// that invoke `touring` (the prior-touring side). With
    /// `adoption_antipattern_count` powers `touring.coupling.adoption_ratio` —
    /// the prior-bash→prior-touring inversion. Unconditional (every Bash action),
    /// unlike F2's suggestion-gated uptake.
    pub adoption_touring_count: AtomicU64,

    /// **F3 (2026-06-28)** — coupling adoption_ratio denominator contribution:
    /// `Bash` actions that are raw-shell inspection antipatterns (grep/cat/find/
    /// sed — the prior-bash side). `adoption_ratio = touring / (touring + antipattern)`.
    pub adoption_antipattern_count: AtomicU64,

    /// **Task #6 (2026-06-29)** — pillar-induction denominator: armed pillar nudges
    /// emitted by `cli_suggester` (master-cli / learning-memory — the differentials
    /// the upstream classifiers miss). Paired with `pillar_induction_followed_count`
    /// for `touring.coupling.pillar_induction_ratio`.
    pub pillar_induction_emitted_count: AtomicU64,

    /// **Task #6 (2026-06-29)** — pillar-induction numerator: pillar nudges whose
    /// next same-session action used a master/recall command. `ratio = followed /
    /// emitted` — the per-pillar adoption signal F7 promote/demote consumes.
    pub pillar_induction_followed_count: AtomicU64,

    /// **ES3 P2 (2026-06-02)** — cumulative count of write-paths observed
    /// by the S-01 observe hook (`ceg_adapter::run_observe_only`) when
    /// `AccessDeclaration::from_tool_payload_full` inferred a non-empty
    /// write-set. The counter is the observability surface for the
    /// production wire-inference path — a non-zero value confirms that
    /// shell redirects and write-tool commands are now being surfaced
    /// in the lock manager's conflict rule.
    pub ceg_write_paths_observed_count: AtomicU64,
    /// ES3 P4 — total cross-agent outcome events written to the cross-agent
    /// ledger. Substrate-only delivery: producers wire in P4.3, the
    /// `LedgerConsumer` poll loop is deferred to a followup wave.
    pub cross_agent_events_written_count: AtomicU64,
    /// ES3 P5 — total worktree-create events that promoted the runtime to
    /// `IsolationMode::Worktree(path)`. Indicator of capability-readiness
    /// uptake (CAH roadmap §3).
    pub worktree_isolation_active_count: AtomicU64,
    /// ES4 P2 — total (success, failure) observations applied via
    /// `LearnedOutcomeModel::merge_into_global`.
    pub outcome_learner_distill_count: AtomicU64,
    /// ES4 P3 — total X4 PREDICT invocations.
    pub outcome_learner_predict_count: AtomicU64,
    /// ES4 P3 — running sum of Brier score contributions (bit-cast f64).
    pub outcome_learner_brier_running_sum: AtomicU64,
    /// ES4 P3 — total cold-start predictions (n < 10).
    pub outcome_learner_cold_start_count: AtomicU64,
    /// ES4 P4 — total speculative-driver queries against the durable model.
    pub speculative_durable_model_queries_count: AtomicU64,
}

impl Default for GateMetrics {
    fn default() -> Self {
        Self {
            pre_edit_fast_path_count: AtomicU64::new(0),
            pre_edit_full_enrichment_count: AtomicU64::new(0),
            pre_write_fast_path_count: AtomicU64::new(0),
            pre_write_full_enrichment_count: AtomicU64::new(0),
            post_tool_l4_mandatory_count: AtomicU64::new(0),
            metadata_cache_hit_count: AtomicU64::new(0),
            metadata_backpressure_dropped_count: AtomicU64::new(0),
            pre_grep_enrichment_count: AtomicU64::new(0),
            pre_grep_zero_results_count: AtomicU64::new(0),
            tantivy_upsert_count: AtomicU64::new(0),
            tantivy_query_latency_us: AtomicU64::new(0),
            tantivy_commit_count: AtomicU64::new(0),
            reindex_failure_count: AtomicU64::new(0),
            rkyv_dispatch_count: AtomicU64::new(0),
            rkyv_dispatch_bytes: AtomicU64::new(0),
            rkyv_parse_error_count: AtomicU64::new(0),
            rkyv_response_count: AtomicU64::new(0),
            rkyv_dispatch_latency: LatencyHistogram::new(),
            hook_dispatch_latency: LatencyHistogram::new(),
            tantivy_query_latency_hist: LatencyHistogram::new(),
            actor_queue_wait: LatencyHistogram::new(),
            actor_budget_timeout_count: AtomicU64::new(0),
            actor_send_timeout_count: AtomicU64::new(0),
            health_delta_record_count: AtomicU64::new(0),
            health_delta_compute_count: AtomicU64::new(0),
            health_delta_regression_count: AtomicU64::new(0),
            health_delta_improvement_count: AtomicU64::new(0),
            health_delta_streak_alert_count: AtomicU64::new(0),
            health_delta_recovery_count: AtomicU64::new(0),
            query_cache_hit_count: AtomicU64::new(0),
            query_cache_miss_count: AtomicU64::new(0),
            query_cache_invalidate_count: AtomicU64::new(0),
            blast_inject_count: AtomicU64::new(0),
            blast_timeout_count: AtomicU64::new(0),
            blast_mutation_count: AtomicU64::new(0),
            linucb_route_manual_count: AtomicU64::new(0),
            linucb_route_generator_count: AtomicU64::new(0),
            linucb_route_hint_count: AtomicU64::new(0),
            mcts_shadow_run_count: AtomicU64::new(0),
            mcts_shadow_timeout_count: AtomicU64::new(0),
            mcts_shadow_deadlock_detected_count: AtomicU64::new(0),
            ann_search_latency: LatencyHistogram::new(),
            tantivy_stream_enqueued_count: AtomicU64::new(0),
            tantivy_stream_backpressure_drop_count: AtomicU64::new(0),
            tantivy_stream_flush_count: AtomicU64::new(0),
            tantivy_stream_flush_docs_count: AtomicU64::new(0),
            prefetch_enqueued_count: AtomicU64::new(0),
            prefetch_warmed_count: AtomicU64::new(0),
            jdm_class_a_count: AtomicU64::new(0),
            jdm_class_b_count: AtomicU64::new(0),
            jdm_class_c_count: AtomicU64::new(0),
            jdm_class_d_count: AtomicU64::new(0),
            tokio_num_workers: AtomicU64::new(0),
            tokio_num_idle_threads: AtomicU64::new(0),
            tokio_num_blocking_threads: AtomicU64::new(0),
            tokio_injection_queue_depth: AtomicU64::new(0),
            diagnostic_wiring_finding_emitted_count: AtomicU64::new(0),
            diagnostic_tdg_emitted_count: AtomicU64::new(0),
            diagnostic_b302_emitted_count: AtomicU64::new(0),
            task_sync_create_count: AtomicU64::new(0),
            task_sync_deduped_count: AtomicU64::new(0),
            task_sync_update_propagated_count: AtomicU64::new(0),
            diagnostic_q220_nonidempotent_emitted_count: AtomicU64::new(0),
            diagnostic_w115_skipped_region_written_count: AtomicU64::new(0),
            activity_pre_edit_count: AtomicU64::new(0),
            activity_post_edit_count: AtomicU64::new(0),
            activity_post_write_count: AtomicU64::new(0),
            activity_instructions_loaded_count: AtomicU64::new(0),
            activity_pre_compact_count: AtomicU64::new(0),
            memory_pressure_green_count: AtomicU64::new(0),
            memory_pressure_yellow_count: AtomicU64::new(0),
            memory_pressure_red_count: AtomicU64::new(0),
            memory_pressure_total_tick_count: AtomicU64::new(0),
            swap_thrashing_detected_count: AtomicU64::new(0),
            cargo_test_paused_count: AtomicU64::new(0),
            core_pinning_p_count: AtomicU64::new(0),
            core_pinning_e_count: AtomicU64::new(0),
            tool_output_routed_count: AtomicU64::new(0),
            sandbox_timeout_fallback_count: AtomicU64::new(0),
            phrase_query_match_count: AtomicU64::new(0),
            tantivy_trigram_query_count: AtomicU64::new(0),
            tool_outputs_ttl_skip_count: AtomicU64::new(0),
            tool_outputs_cleanup_deleted_count: AtomicU64::new(0),
            sandbox_tee_persisted_count: AtomicU64::new(0),
            compression_profile_applied_count: AtomicU64::new(0),
            // Wave 3 T1 — 19 counters
            ctx_replay_count: AtomicU64::new(0),
            ctx_purge_count: AtomicU64::new(0),
            ctx_doctor_call_count: AtomicU64::new(0),
            gate_metrics_daily_flush_count: AtomicU64::new(0),
            ctx_gain_graph_count: AtomicU64::new(0),
            ctx_session_adoption_query_count: AtomicU64::new(0),
            touring_init_invocation_count: AtomicU64::new(0),
            ctx_smart_count: AtomicU64::new(0),
            read_aggressive_chunked_count: AtomicU64::new(0),
            read_aggressive_passthrough_count: AtomicU64::new(0),
            ctx_explain_count: AtomicU64::new(0),
            ctx_budget_warning_emitted_count: AtomicU64::new(0),
            ctx_budget_alert_emitted_count: AtomicU64::new(0),
            ctx_batch_execute_count: AtomicU64::new(0),
            ctx_batch_item_count: AtomicU64::new(0),
            ctx_execute_file_count: AtomicU64::new(0),
            ctx_upgrade_count: AtomicU64::new(0),
            ctx_discover_session_count: AtomicU64::new(0),
            ctx_discover_opportunities_found: AtomicU64::new(0),
            wave3_t201_count: AtomicU64::new(0),
            wave3_t202_count: AtomicU64::new(0),
            wave3_t203_count: AtomicU64::new(0),
            wave3_t204_count: AtomicU64::new(0),
            wave3_t205_count: AtomicU64::new(0),
            wave3_t206_count: AtomicU64::new(0),
            wave3_t207_count: AtomicU64::new(0),
            wave3_t208_count: AtomicU64::new(0),
            wave3_t209_count: AtomicU64::new(0),
            wave3_t210_count: AtomicU64::new(0),
            wave3_t211_count: AtomicU64::new(0),
            wave3_t212_count: AtomicU64::new(0),
            wave3_t213_count: AtomicU64::new(0),
            wave3_t214_count: AtomicU64::new(0),
            wave3_t215_count: AtomicU64::new(0),
            wave3_t301_count: AtomicU64::new(0),
            wave3_t302_count: AtomicU64::new(0),
            wave3_t303_count: AtomicU64::new(0),
            wave3_t304_count: AtomicU64::new(0),
            wave3_t305_count: AtomicU64::new(0),
            wave3_t306_count: AtomicU64::new(0),
            wave3_t307_count: AtomicU64::new(0),
            wave3_t308_count: AtomicU64::new(0),
            wave3_t309_count: AtomicU64::new(0),
            wave3_t310_count: AtomicU64::new(0),
            // CEG Pln2 FASE 5a — P7.1
            ceg_captured_count: AtomicU64::new(0),
            ceg_blocked_count: AtomicU64::new(0),
            ceg_sandboxed_count: AtomicU64::new(0),
            ceg_fast_path_count: AtomicU64::new(0),
            workflow_antipattern_detected_count: AtomicU64::new(0),
            workflow_advice_emitted_count: AtomicU64::new(0),
            antipattern_converted_count: AtomicU64::new(0),
            enrichment_context_bytes_total: AtomicU64::new(0),
            enrichment_emit_count: AtomicU64::new(0),
            suggestion_uptake_emitted_count: AtomicU64::new(0),
            suggestion_uptake_followed_count: AtomicU64::new(0),
            adoption_touring_count: AtomicU64::new(0),
            adoption_antipattern_count: AtomicU64::new(0),
            pillar_induction_emitted_count: AtomicU64::new(0),
            pillar_induction_followed_count: AtomicU64::new(0),
            ceg_write_paths_observed_count: AtomicU64::new(0),
            cross_agent_events_written_count: AtomicU64::new(0),
            worktree_isolation_active_count: AtomicU64::new(0),
            outcome_learner_distill_count: AtomicU64::new(0),
            outcome_learner_predict_count: AtomicU64::new(0),
            outcome_learner_brier_running_sum: AtomicU64::new(0),
            outcome_learner_cold_start_count: AtomicU64::new(0),
            speculative_durable_model_queries_count: AtomicU64::new(0),
        }
    }
}

/// Get the global gate metrics singleton, initializing on first call.
///
/// Thread-safe: `OnceLock` guarantees single initialization even under
/// concurrent access.
#[inline]
pub fn global() -> &'static GateMetrics {
    GATE_METRICS.get_or_init(GateMetrics::default)
}

// ── Recording functions ─────────────────────────────────────────────────

/// Record that pre_edit took the fast-path (skipped compose_edit_context).
#[inline]
pub fn record_pre_edit_fast_path() {
    global()
        .pre_edit_fast_path_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record that pre_edit ran the full enrichment (compose_edit_context executed).
#[inline]
pub fn record_pre_edit_full() {
    global()
        .pre_edit_full_enrichment_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record that pre_write took the fast-path (skipped SignalPipeline).
#[inline]
pub fn record_pre_write_fast_path() {
    global()
        .pre_write_fast_path_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record that pre_write ran the full enrichment pipeline.
#[inline]
pub fn record_pre_write_full() {
    global()
        .pre_write_full_enrichment_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record that post_tool_use triggered L4+ mandatory enrichment.
#[inline]
pub fn record_post_tool_l4_mandatory() {
    global()
        .post_tool_l4_mandatory_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a metadata cache hit (MetadataDedup returned cached result).
#[inline]
pub fn record_metadata_cache_hit() {
    global()
        .metadata_cache_hit_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record that pre_grep / pre_glob emitted an enrichment block.
///
/// D43 (master plan W2) — Grep/Glob symbol enrichment counter.
#[inline]
pub fn record_pre_grep_enrichment() {
    global()
        .pre_grep_enrichment_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record that pre_grep / pre_glob fell through silently after `find_symbol`
/// returned 0 locations for a symbol-like pattern.
///
/// D43 (master plan W2) — false-positive watchdog. If this counter dominates
/// `pre_grep_enrichment_count`, the whitelist regex is too permissive.
#[inline]
pub fn record_pre_grep_zero_results() {
    global()
        .pre_grep_zero_results_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a metadata backpressure drop (TokenBudget dropped enrichment).
#[inline]
pub fn record_metadata_backpressure_dropped() {
    global()
        .metadata_backpressure_dropped_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── Tantivy FTS recording functions (U21) ──────────────────────────────

// ── rkyv IPC recording functions (Wave 3 D1) ──────────────────────────

/// Record one successful rkyv-dispatched inbound request.
///
/// Call after `check_archived_root` returns `Ok`. `body_bytes` is the
/// declared body length from the frame header (excludes the 8-byte
/// prefix). `Ordering::Relaxed` is sufficient — counter is monotonic and
/// we tolerate slight re-ordering against concurrent snapshots.
#[inline]
pub fn record_rkyv_dispatch(body_bytes: u64) {
    let m = global();
    m.rkyv_dispatch_count.fetch_add(1, Ordering::Relaxed);
    m.rkyv_dispatch_bytes
        .fetch_add(body_bytes, Ordering::Relaxed);
}

/// Record an rkyv frame rejection (magic, truncated, length, bytecheck).
#[inline]
pub fn record_rkyv_parse_error() {
    global()
        .rkyv_parse_error_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one outbound response emitted in rkyv framed form.
#[inline]
pub fn record_rkyv_response() {
    global().rkyv_response_count.fetch_add(1, Ordering::Relaxed);
}

// ── Health Delta recording functions (Wave 12, 2026-04-18) ────────────

/// Record one successful pre-record into the `HealthDeltaCache`.
///
/// Called by `health_delta::record_pre_signals` when a source's quality
/// score is stored. High counts confirm the bridge is actively populated.
#[inline]
pub fn record_health_delta_record() {
    global()
        .health_delta_record_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one successful post-compute that produced a `HealthDelta`.
///
/// Called by `health_delta::compute_signals_delta` when the cache
/// returns either a delta-bearing entry or a first-observation result.
/// Should track `health_delta_record_count` asymptotically; large
/// divergence signals leaks (record without consume) or daemon restarts.
#[inline]
pub fn record_health_delta_compute() {
    global()
        .health_delta_compute_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one compute event whose delta tripped `is_regression()`.
#[inline]
pub fn record_health_delta_regression() {
    global()
        .health_delta_regression_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one compute event whose delta tripped `is_improvement()`.
#[inline]
pub fn record_health_delta_improvement() {
    global()
        .health_delta_improvement_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Wave 13 — Record that the per-path regression streak crossed the
/// alert threshold (default = 3 consecutive regressions).
#[inline]
pub fn record_health_delta_streak_alert() {
    global()
        .health_delta_streak_alert_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Wave 13 — Record one recovery event (improvement after a regression
/// streak ≥ 1). Tracks the directionality of corrective edits.
#[inline]
pub fn record_health_delta_recovery() {
    global()
        .health_delta_recovery_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Wave 17 — Record one query_cache hit (cli_index_find, cli_tantivy_search…).
#[inline]
pub fn record_query_cache_hit() {
    global()
        .query_cache_hit_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Wave 17 — Record one query_cache miss (compute path ran).
#[inline]
pub fn record_query_cache_miss() {
    global()
        .query_cache_miss_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Wave 18 — Record N entries invalidated by path-scoped invalidation.
#[inline]
pub fn record_query_cache_invalidate(n: u64) {
    global()
        .query_cache_invalidate_count
        .fetch_add(n, Ordering::Relaxed);
}

// ── Predictive Wave counters (D2/D3/D4) ───────────────────────────────────

/// D2 — Record one blast radius injection invocation at `PreToolUse[Task*]`.
#[inline]
pub fn record_blast_inject() {
    global().blast_inject_count.fetch_add(1, Ordering::Relaxed);
}

/// D2 — Record one blast radius compute that exceeded its timeout budget.
#[inline]
pub fn record_blast_timeout() {
    global().blast_timeout_count.fetch_add(1, Ordering::Relaxed);
}

/// D2 — Record one PreToolUse response that mutated the tool input.
#[inline]
pub fn record_blast_mutation() {
    global()
        .blast_mutation_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── D2.4: Tool output routing counters ───────────────────────────────────

/// Record one tool invocation routed to sandbox execution (D2.4).
#[inline]
pub fn record_tool_output_routed() {
    global()
        .tool_output_routed_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one sandbox execution that fell back to original args due to timeout.
#[inline]
/// I-02 — record a successful PhraseQuery match in `search()`.
pub fn record_phrase_query_match() {
    global()
        .phrase_query_match_count
        .fetch_add(1, Ordering::Relaxed);
}

/// I-01 — record a NgramTokenizer trigram-field query.
pub fn record_tantivy_trigram_query() {
    global()
        .tantivy_trigram_query_count
        .fetch_add(1, Ordering::Relaxed);
}

/// I-05 — record a TTL fast-skip in `ToolOutputsIndex::store_tool_output`.
pub fn record_tool_outputs_ttl_skip() {
    global()
        .tool_outputs_ttl_skip_count
        .fetch_add(1, Ordering::Relaxed);
}

/// I-05 — record a doc deletion by the cleanup actor.
pub fn record_tool_outputs_cleanup_deleted(n: u64) {
    global()
        .tool_outputs_cleanup_deleted_count
        .fetch_add(n, Ordering::Relaxed);
}

/// NEW-2 — record a sandbox failure persisted via tee mode.
pub fn record_sandbox_tee_persisted() {
    global()
        .sandbox_tee_persisted_count
        .fetch_add(1, Ordering::Relaxed);
}

/// NEW-1 — record a compression profile application.
pub fn record_compression_profile_applied() {
    global()
        .compression_profile_applied_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Records one fallback caused by a sandbox execution timeout.
pub fn record_sandbox_timeout_fallback() {
    global()
        .sandbox_timeout_fallback_count
        .fetch_add(1, Ordering::Relaxed);
}

/// D3 — Record one LinUCB routing decision where ManualEdit won.
#[inline]
pub fn record_linucb_route_manual() {
    global()
        .linucb_route_manual_count
        .fetch_add(1, Ordering::Relaxed);
}

/// D3 — Record one LinUCB routing decision where a Generator* arm won.
#[inline]
pub fn record_linucb_route_generator() {
    global()
        .linucb_route_generator_count
        .fetch_add(1, Ordering::Relaxed);
}

/// D3 — Record one TaskList response that emitted a `[TOURING RL-ROUTER]` hint.
#[inline]
pub fn record_linucb_route_hint() {
    global()
        .linucb_route_hint_count
        .fetch_add(1, Ordering::Relaxed);
}

/// D4 — Record one MCTS shadow rollout execution (CilaLevel gate passed).
#[inline]
pub fn record_mcts_shadow_run() {
    global()
        .mcts_shadow_run_count
        .fetch_add(1, Ordering::Relaxed);
}

/// D4 — Record one MCTS shadow rollout that exceeded its budget.
#[inline]
pub fn record_mcts_shadow_timeout() {
    global()
        .mcts_shadow_timeout_count
        .fetch_add(1, Ordering::Relaxed);
}

/// D4 — Record one MCTS shadow rollout that detected a deadlock cycle.
#[inline]
pub fn record_mcts_shadow_deadlock_detected() {
    global()
        .mcts_shadow_deadlock_detected_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── ANN search latency recording (Wave 22) ────────────────────────────────

/// Record one ANN (HNSW) vector search latency in microseconds.
///
/// Called from `cli_memory_recall` immediately after `ann_recall.search()`
/// returns, so it captures only the k-NN scan time (excluding SQL and RRF).
/// Drives the `ann_search_latency` distribution in `touring gate-metrics -j`.
#[inline]
pub fn record_ann_search_latency_us(micros: u64) {
    global().ann_search_latency.record_us(micros);
}

// ── Tantivy Stream Actor recording functions (Suggestion 2 — 2026-04-20) ──

/// Record a doc successfully sent to the stream actor channel.
#[inline]
pub fn record_tantivy_stream_enqueued() {
    global()
        .tantivy_stream_enqueued_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a doc dropped due to stream channel backpressure.
#[inline]
pub fn record_tantivy_stream_backpressure_drop() {
    global()
        .tantivy_stream_backpressure_drop_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one stream flush (commit) with the batch doc count.
#[inline]
pub fn record_tantivy_stream_flush(docs: u64) {
    global()
        .tantivy_stream_flush_count
        .fetch_add(1, Ordering::Relaxed);
    global()
        .tantivy_stream_flush_docs_count
        .fetch_add(docs, Ordering::Relaxed);
}

/// Record a file path enqueued to the prefetch actor.
#[inline]
pub fn record_prefetch_enqueued() {
    global()
        .prefetch_enqueued_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a file successfully warmed into the global parser cache.
#[inline]
pub fn record_prefetch_warmed() {
    global()
        .prefetch_warmed_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── JDM Routing record helpers (Grupo 5 §1 — 2026-04-21) ─────────────

/// Record a TaskCreate classified as JDM class-A (code implementation).
#[inline]
pub fn record_jdm_class_a() {
    global().jdm_class_a_count.fetch_add(1, Ordering::Relaxed);
}

/// Record a TaskCreate classified as JDM class-B (infra/devops).
#[inline]
pub fn record_jdm_class_b() {
    global().jdm_class_b_count.fetch_add(1, Ordering::Relaxed);
}

/// Record a TaskCreate classified as JDM class-C (cognitive/architecture).
#[inline]
pub fn record_jdm_class_c() {
    global().jdm_class_c_count.fetch_add(1, Ordering::Relaxed);
}

/// Record a TaskCreate classified as JDM class-D (multi-agent/orchestration).
#[inline]
pub fn record_jdm_class_d() {
    global().jdm_class_d_count.fetch_add(1, Ordering::Relaxed);
}

// ── Tokio Runtime Metrics recording functions (Wave 26 — 2026-04-21) ─

/// Record a snapshot of Tokio runtime metrics (worker count, idle threads,
/// blocking threads, injection queue depth).
///
/// Called periodically from the watchdog task or on-demand via `cli_tokio_metrics`.
/// Uses Relaxed ordering — counters are monotonically increasing and
/// exact ordering against concurrent snapshots is not critical.
#[inline]
pub fn record_tokio_metrics(workers: u64, idle: u64, blocking: u64, injection: u64) {
    let m = global();
    m.tokio_num_workers.fetch_add(workers, Ordering::Relaxed);
    m.tokio_num_idle_threads.fetch_add(idle, Ordering::Relaxed);
    m.tokio_num_blocking_threads
        .fetch_add(blocking, Ordering::Relaxed);
    m.tokio_injection_queue_depth
        .fetch_add(injection, Ordering::Relaxed);
}

/// Snapshot the Tokio metrics as an absolute gauge (resets on capture).
///
/// Unlike other counters which are monotonically increasing, Tokio metrics
/// are point-in-time gauges. `GateMetricsSnapshot` exposes them as the
/// raw current values. Diagnostic interpretation:
/// - idle > 0 AND injection > 0 → backpressure downstream
/// - idle == 0 sustained → workers undersized for workload
/// - blocking approaching max → blocking pool saturated
#[inline]
pub fn snapshot_tokio_metrics() -> (u64, u64, u64, u64) {
    let m = global();
    (
        m.tokio_num_workers.load(Ordering::Relaxed),
        m.tokio_num_idle_threads.load(Ordering::Relaxed),
        m.tokio_num_blocking_threads.load(Ordering::Relaxed),
        m.tokio_injection_queue_depth.load(Ordering::Relaxed),
    )
}

// ── Latency recording functions (2026-04-17) ───────────────────────────

/// Record one rkyv dispatch latency in microseconds.
///
/// Call after the full rkyv parse → hook execution → response framing
/// completes. Paired with `record_rkyv_dispatch(body_bytes)` which records
/// the body size independently.
#[inline]
pub fn record_rkyv_dispatch_latency_us(micros: u64) {
    global().rkyv_dispatch_latency.record_us(micros);
}

/// Record one actor-thread hook dispatch latency in microseconds.
///
/// Measured from `ProjectCommand::RunHook` receipt to oneshot response.
/// Validates the 15s light / 300s heavy handler budgets empirically.
#[inline]
pub fn record_hook_dispatch_latency_us(micros: u64) {
    global().hook_dispatch_latency.record_us(micros);
}

/// Record how long one `ProjectCommand::RunHook` waited in the per-project
/// actor queue (enqueue → dequeue), in microseconds.
///
/// Separates queue-wait from handler execution: `hook_dispatch_latency`
/// starts at dequeue, so a hook stuck behind a heavy handler from another
/// session (same project → same serial actor) shows up here and nowhere
/// else. Added 2026-07-01 to turn perceived cross-session stalls into a
/// number.
#[inline]
pub fn record_actor_queue_wait_us(micros: u64) {
    global().actor_queue_wait.record_us(micros);
}

/// Record one dispatch that gave up waiting for the actor's oneshot reply
/// because the handler exceeded its budget (15s light / 300s heavy).
#[inline]
pub fn record_actor_budget_timeout() {
    global()
        .actor_budget_timeout_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one dispatch dropped on the send side because the actor's bounded
/// queue stayed full past `REQUEST_TIMEOUT`.
#[inline]
pub fn record_actor_send_timeout() {
    global()
        .actor_send_timeout_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one Tantivy query latency in microseconds (distribution).
///
/// Complements `record_tantivy_query_latency(us)` (cumulative sum) with
/// per-query distribution — outliers surface that the cumulative hides.
#[inline]
pub fn record_tantivy_query_latency_hist_us(micros: u64) {
    global().tantivy_query_latency_hist.record_us(micros);
}

/// Record one Tantivy upsert operation.
///
/// Called from `TantivyIndex::upsert_symbol()` (feature-gated under
/// `tantivy-fts`). Safe to call from any thread — `Ordering::Relaxed`
/// is sufficient for a monotonic counter.
#[inline]
pub fn record_tantivy_upsert() {
    global()
        .tantivy_upsert_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record the latency of a single Tantivy search call in microseconds.
///
/// Called from `TantivyIndex::search()` and `TantivyIndex::fuzzy_search()`
/// (feature-gated under `tantivy-fts`). Accumulated as a sum; divide by
/// the upsert/commit count to derive average query latency externally.
#[inline]
pub fn record_tantivy_query_latency(us: u64) {
    global()
        .tantivy_query_latency_us
        .fetch_add(us, Ordering::Relaxed);
}

/// Record one Tantivy index commit (auto or explicit).
///
/// Called from `TantivyIndex::commit()` (feature-gated under `tantivy-fts`).
/// Tracks both auto-commits (batch_size threshold) and explicit commits.
#[inline]
pub fn record_tantivy_commit() {
    global()
        .tantivy_commit_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one incremental-indexing failure (B2, 2026-05-10).
///
/// Call this from `post_edit`/`post_write` whenever `reindex_file` returns
/// `Err`. Paired with a `tracing::warn!` at the same site (promoted from the
/// silent `tracing::debug!` that previously hid these failures from
/// production observability).
///
/// A non-zero value while the editor is actively writing files is a strong
/// signal that the symbol index is drifting — operators should respond with
/// `touring index ingest <file>` (per-file) or `touring index rebuild`
/// (whole workspace).
#[inline]
pub fn record_reindex_failure() {
    global()
        .reindex_failure_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── RFC-100 Diagnostic Code recording functions (2026-04-25) ─────────────

/// Record one wiring-finding diagnostic emitted (W-1xx RFC-100 code).
///
/// Call when `to_diagnostic_opt()` fires for an orphan, unused pub symbol,
/// or dead functional chain. Drives W-1xx prevalence in gate-metrics.
#[inline]
pub fn record_diagnostic_wiring_finding_emitted() {
    global()
        .diagnostic_wiring_finding_emitted_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one TDG grade D or F diagnostic emitted (Q-201/Q-202 RFC-100 codes).
///
/// Call when `cli_ast_tdg` computes a TDG grade of D or F and emits a
/// diagnostic. Drives Q-2xx prevalence in gate-metrics.
#[inline]
pub fn record_diagnostic_tdg_emitted() {
    global()
        .diagnostic_tdg_emitted_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one B-302 PatchExpansion diagnostic emitted.
///
/// Call when `pre_write::emit_b302_if_low_confidence_expansion` (or any
/// equivalent caller) emits a B-302 RFC-100 diagnostic for a low-confidence
/// fuzzy patch expansion. Drives B-3xx prevalence in gate-metrics. Wave 12.
#[inline]
pub fn record_diagnostic_b302_emitted() {
    global()
        .diagnostic_b302_emitted_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Call when `bridge_task_created` mints a new CC-mirror container (F0-pre).
pub fn record_task_sync_create() {
    global()
        .task_sync_create_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Call when `bridge_task_created` dedupes a repeat sync arrival (F0-pre).
pub fn record_task_sync_deduped() {
    global()
        .task_sync_deduped_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Call when a TaskUpdate status change propagates onto the mirror (F0-pre).
pub fn record_task_sync_update_propagated() {
    global()
        .task_sync_update_propagated_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Call when `idempotency::check_idempotency` detects a non-idempotent format
/// in `pre_edit` (`format(format(x)) != format(x)`). Increments the Q-220
/// counter and emits the diagnostic. Drives Q-220 prevalence in gate-metrics.
/// Wave A.3.
#[inline]
pub fn record_diagnostic_q220_nonidempotent_emitted() {
    global()
        .diagnostic_q220_nonidempotent_emitted_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Call when `post_edit` detects an Edit tool write overlapped a
/// `// touring:skip-region` … `// touring:skip-end` frozen region.
/// Increments the W-115 counter and emits the diagnostic. Drives W-115
/// prevalence in gate-metrics. Wave A.2.4.
#[inline]
pub fn record_diagnostic_w115_skipped_region_written() {
    global()
        .diagnostic_w115_skipped_region_written_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── ES3 P2 (2026-06-02) — CEG write-paths observability ─────────────────

/// **ES3 P2 (2026-06-02)** — record the count of write-paths inferred by
/// the S-01 observe hook (`ceg_adapter::run_observe_only`) for one tool
/// call. Called when `AccessDeclaration::from_tool_payload_full` returns
/// a non-empty write-set — i.e. when the call would mutate state
/// (shell redirect, `rm`, `mv`, `cp`, `sed -i`, etc.).
///
/// A non-zero value in `touring gate-metrics -j` confirms that
/// production traffic is exercising the new write-inference path. The
/// `n` argument is the size of the inferred write-set, NOT the number
/// of observed tool calls — so the sum across many calls measures the
/// total write-set surface area seen by the hook.
#[inline]
pub fn record_ceg_write_paths_observed(n: usize) {
    global()
        .ceg_write_paths_observed_count
        .fetch_add(n as u64, Ordering::Relaxed);
}

// ── ES3 P4-P5 counters ────────────────────────────────────────────────────

/// ES3 P4 — record a cross-agent outcome event was written to the ledger.
#[inline]
pub fn record_cross_agent_events_written() {
    global()
        .cross_agent_events_written_count
        .fetch_add(1, Ordering::Relaxed);
}

/// ES3 P5 — record a worktree-create event promoted the runtime to
/// `IsolationMode::Worktree(path)`.
#[inline]
pub fn record_worktree_isolation_active() {
    global()
        .worktree_isolation_active_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── ES4 P2-P4 counters ────────────────────────────────────────────────────

/// ES4 P2 — total (success, failure) observations applied via
/// `LearnedOutcomeModel::merge_into_global`. Monitors distillation throughput.
#[inline]
pub fn record_outcome_learner_distill(n: usize) {
    global()
        .outcome_learner_distill_count
        .fetch_add(n as u64, Ordering::Relaxed);
}

/// ES4 P3 — record one X4 PREDICT was emitted (any confidence bucket).
#[inline]
pub fn record_outcome_learner_predict() {
    global()
        .outcome_learner_predict_count
        .fetch_add(1, Ordering::Relaxed);
}

/// ES4 P3 — add the per-prediction Brier contribution `(1 - prob)^2` to a
/// running sum (bit-cast f64 → u64; acceptable for Brier values `[0,1]`).
/// Final Brier score = sum / count, exposed via `touring gate-metrics -j`.
#[inline]
pub fn record_outcome_learner_brier(b: f64) {
    global()
        .outcome_learner_brier_running_sum
        .fetch_add(b.to_bits(), Ordering::Relaxed);
}

/// ES4 P3 — record a cold-start prediction (n < MIN_CALIBRATION = 10).
#[inline]
pub fn record_outcome_learner_cold_start() {
    global()
        .outcome_learner_cold_start_count
        .fetch_add(1, Ordering::Relaxed);
}

/// ES4 P4 — count of speculative-driver queries against the durable model.
#[inline]
pub fn record_speculative_durable_model_queries(n: usize) {
    global()
        .speculative_durable_model_queries_count
        .fetch_add(n as u64, Ordering::Relaxed);
}

// ── ESAA activity hook recording functions (v8 D1.6) ─────────────────────

/// Records one pre_edit activity event fired to activity.jsonl.
#[inline]
pub fn record_activity_pre_edit() {
    global()
        .activity_pre_edit_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Records one post_edit activity event fired to activity.jsonl.
#[inline]
pub fn record_activity_post_edit() {
    global()
        .activity_post_edit_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Records one post_write activity event fired to activity.jsonl.
#[inline]
pub fn record_activity_post_write() {
    global()
        .activity_post_write_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Records one instructions_loaded activity event fired at session_start.
#[inline]
pub fn record_activity_instructions_loaded() {
    global()
        .activity_instructions_loaded_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Records one pre_compact activity event fired before context compaction.
#[inline]
pub fn record_activity_pre_compact() {
    global()
        .activity_pre_compact_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── resource-monitor recording functions (W-540..W-543, 2026-05-02) ─────

/// Record one PSI pressure tick classified as Green.
///
/// Called by `touring_resilience::sentinel::metrics::record_pressure_tick(Pressure::Green)`
/// (via the feature-gated bridge in `shared::resource_monitor_bridge`).
/// Always increments `memory_pressure_total_tick_count` as well.
#[inline]
pub fn record_pressure_green() {
    let m = global();
    m.memory_pressure_green_count
        .fetch_add(1, Ordering::Relaxed);
    m.memory_pressure_total_tick_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one PSI pressure tick classified as Yellow (W-541).
#[inline]
pub fn record_pressure_yellow() {
    let m = global();
    m.memory_pressure_yellow_count
        .fetch_add(1, Ordering::Relaxed);
    m.memory_pressure_total_tick_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one PSI pressure tick classified as Red (W-540).
#[inline]
pub fn record_pressure_red() {
    let m = global();
    m.memory_pressure_red_count.fetch_add(1, Ordering::Relaxed);
    m.memory_pressure_total_tick_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one swap-thrashing detection event (W-542).
///
/// Called when `pgmajfault` rate exceeds the thrashing threshold inside
/// the MemoryGuard ticker loop.
#[inline]
pub fn record_swap_thrashing_detected() {
    global()
        .swap_thrashing_detected_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one heavy spawn paused by the W-540 memory pressure gate
/// in `pre_bash`. Incremented when Pressure::Red blocks a cargo/npm/pip command.
#[inline]
pub fn record_cargo_test_paused() {
    global()
        .cargo_test_paused_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one successful P-core (performance core) affinity pinning
/// performed by `CoreScheduler`. Linux-only; counter stays 0 otherwise.
#[inline]
pub fn record_core_pinning_p() {
    global()
        .core_pinning_p_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one successful E-core (efficiency core) affinity pinning
/// performed by `CoreScheduler`. Linux-only; counter stays 0 otherwise.
#[inline]
pub fn record_core_pinning_e() {
    global()
        .core_pinning_e_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── GateMetricsSnapshot + wave3/ceg/ctx record helpers extracted to
// gate_metrics_snapshot.rs (F-9). Re-exported so call paths resolve unchanged.
pub use crate::gate_metrics_snapshot::{
    GateMetricsSnapshot, record_adoption_antipattern, record_adoption_touring,
    record_antipattern_converted, record_ceg_blocked, record_ceg_captured, record_ceg_fast_path,
    record_ceg_sandboxed, record_ctx_batch_execute, record_ctx_budget_alert,
    record_ctx_budget_warning, record_ctx_discover_session, record_ctx_doctor_call,
    record_ctx_execute_file, record_ctx_execute_file_count, record_ctx_explain,
    record_ctx_gain_graph, record_ctx_purge, record_ctx_replay, record_ctx_session_adoption_query,
    record_ctx_smart, record_ctx_upgrade, record_enrichment_emitted,
    record_gate_metrics_daily_flush, record_pillar_induction_emitted,
    record_pillar_induction_followed, record_read_aggressive_chunked,
    record_read_aggressive_passthrough, record_suggestion_emitted, record_suggestion_followed,
    record_touring_init_invocation, record_wave3_t201, record_wave3_t202, record_wave3_t203,
    record_wave3_t204, record_wave3_t205, record_wave3_t206, record_wave3_t207, record_wave3_t208,
    record_wave3_t209, record_wave3_t210, record_wave3_t211, record_wave3_t212, record_wave3_t213,
    record_wave3_t214, record_wave3_t215, record_wave3_t301, record_wave3_t302, record_wave3_t303,
    record_wave3_t304, record_wave3_t305, record_wave3_t306, record_wave3_t307, record_wave3_t308,
    record_wave3_t309, record_wave3_t310, record_workflow_advice_emitted,
    record_workflow_antipattern_detected,
};

#[cfg(test)]
#[path = "gate_metrics_tests.rs"]
mod tests;
