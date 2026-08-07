//! `GateMetricsSnapshot` (serialization DTO) + wave3/ceg/ctx `record_*` helpers,
//! extracted from `gate_metrics.rs` (F-9). Re-exported from `gate_metrics` so
//! `gate_metrics::{GateMetricsSnapshot, record_*}` paths resolve unchanged.

use crate::gate_metrics::{LatencySnapshot, global};
use std::sync::atomic::Ordering;

// ── Snapshot for serialization ──────────────────────────────────────────

/// Immutable snapshot of all gate counters at a point in time.
///
/// Computes the fast-path ratio for each hook and returns them alongside
/// the raw counts for CLI/JSON consumers.
///
/// Derives `Default` (all fields are `u64`/`f64`/`LatencySnapshot`, each
/// already `Default`) so consumers — notably the `HarnessQuality` aggregator
/// (R5/OP1) and its tests — can construct a deterministic zero-state snapshot
/// without touching the process-global atomic counters.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GateMetricsSnapshot {
    /// Count of `pre_edit` invocations that took the fast path.
    pub pre_edit_fast_path: u64,
    /// Count of `pre_edit` invocations that ran the full pipeline.
    pub pre_edit_full: u64,
    /// Fraction of `pre_edit` invocations that took the fast path.
    pub pre_edit_fast_ratio: f64,
    /// Count of `pre_write` invocations that took the fast path.
    pub pre_write_fast_path: u64,
    /// Count of `pre_write` invocations that ran the full pipeline.
    pub pre_write_full: u64,
    /// Fraction of `pre_write` invocations that took the fast path.
    pub pre_write_fast_ratio: f64,
    /// Count of `post_tool` invocations that triggered the mandatory L4 path.
    pub post_tool_l4_mandatory: u64,
    /// Count of metadata cache hits.
    pub metadata_cache_hit: u64,
    /// Count of metadata computations dropped due to backpressure.
    pub metadata_backpressure_dropped: u64,
    /// pre_grep / pre_glob enrichment block emissions (D43, master plan W2).
    /// `#[serde(default)]` keeps legacy snapshots without this field deserializable.
    #[serde(default)]
    pub pre_grep_enrichment_count: u64,
    /// pre_grep / pre_glob silent fallthroughs after 0 lookup results (D43).
    /// Watchdog metric for whitelist regex calibration.
    /// `#[serde(default)]` keeps legacy snapshots without this field deserializable.
    #[serde(default)]
    pub pre_grep_zero_results_count: u64,
    /// Total gate invocations across all hooks.
    pub total_invocations: u64,
    /// Cumulative Tantivy upsert operations (U21).
    pub tantivy_upsert_count: u64,
    /// Cumulative Tantivy query latency in microseconds (U21).
    pub tantivy_query_latency_us: u64,
    /// Cumulative Tantivy commit operations (U21).
    pub tantivy_commit_count: u64,
    /// Incremental-indexing failures in `post_edit`/`post_write` (B2, 2026-05-10).
    /// `#[serde(default)]` keeps legacy snapshots without this field deserializable.
    /// A non-zero value during an active editor session means `touring index find`
    /// can return stale results — run `touring index ingest <file>` or `rebuild`.
    #[serde(default)]
    pub reindex_failure_count: u64,
    /// Successful rkyv inbound dispatches (Wave 3 D1).
    pub rkyv_dispatch_count: u64,
    /// Total bytes of rkyv-dispatched request bodies (Wave 3 D1).
    pub rkyv_dispatch_bytes: u64,
    /// Rejected rkyv frames — bad magic, truncated, bytecheck fail (Wave 3 D1).
    pub rkyv_parse_error_count: u64,
    /// Outbound responses emitted in rkyv framed form (Wave 3 D4).
    pub rkyv_response_count: u64,
    /// Mean bytes per rkyv dispatch — 0 when no dispatches recorded yet.
    pub rkyv_mean_bytes: f64,
    /// Per-dispatch rkyv IPC latency distribution (2026-04-17).
    pub rkyv_dispatch_latency: LatencySnapshot,
    /// Per-request actor-thread hook dispatch latency distribution.
    pub hook_dispatch_latency: LatencySnapshot,
    /// Per-query Tantivy search latency distribution.
    pub tantivy_query_latency: LatencySnapshot,
    /// Queue-wait distribution of `RunHook` in the per-project actor mpsc
    /// (enqueue → dequeue) — the serialized-actor backpressure signal
    /// (2026-07-01). `#[serde(default)]` keeps legacy snapshots readable.
    #[serde(default)]
    pub actor_queue_wait: LatencySnapshot,
    /// Dispatches that exceeded the 15s light / 300s heavy handler budget.
    #[serde(default)]
    pub actor_budget_timeout_count: u64,
    /// Dispatches dropped on send — actor queue full past `REQUEST_TIMEOUT`.
    #[serde(default)]
    pub actor_send_timeout_count: u64,

    // ── Wave 12 — Health Delta observability (2026-04-18) ────────────────
    /// Pre-records into the `HealthDeltaCache` (Wave 9 bridge).
    pub health_delta_record_count: u64,
    /// Post-computes that produced a `HealthDelta`.
    pub health_delta_compute_count: u64,
    /// Computes whose delta was a regression (`<= -0.05`).
    pub health_delta_regression_count: u64,
    /// Computes whose delta was an improvement (`>= +0.05`).
    pub health_delta_improvement_count: u64,
    /// Records that have not yet been consumed (record - compute).
    /// A persistently growing value indicates pre-edit/pre-write events
    /// without matching post-edit consumers — a wiring leak.
    pub health_delta_outstanding: u64,
    /// Wave 13 — Times a per-path regression streak crossed the
    /// alert threshold (≥3 consecutive). Drift / spiral signal.
    pub health_delta_streak_alert_count: u64,
    /// Wave 13 — Improvement deltas that broke a prior regression
    /// streak. Recovery signal — CC undid the degradation.
    pub health_delta_recovery_count: u64,
    /// Wave 17 — Query result cache hits.
    pub query_cache_hit_count: u64,
    /// Wave 17 — Query result cache misses (compute path executed).
    pub query_cache_miss_count: u64,
    /// Wave 17 — Cache hit ratio in `[0.0, 1.0]`. Zero when no
    /// queries have been observed; otherwise hits / (hits+misses).
    pub query_cache_hit_ratio: f64,
    /// Wave 18 — Cumulative invalidate count from path-scoped invalidation.
    pub query_cache_invalidate_count: u64,

    // ── Predictive Wave (D2/D3/D4) ──────────────────────────────────────
    /// D2 — blast radius injection invocations at `PreToolUse[Task*]`.
    #[serde(default)]
    pub blast_inject_count: u64,
    /// D2 — blast computes that exceeded the timeout budget.
    #[serde(default)]
    pub blast_timeout_count: u64,
    /// D2 — PreToolUse responses that mutated the tool input.
    #[serde(default)]
    pub blast_mutation_count: u64,
    /// D3 — LinUCB decisions where ManualEdit won.
    #[serde(default)]
    pub linucb_route_manual_count: u64,
    /// D3 — LinUCB decisions where a Generator* arm won.
    #[serde(default)]
    pub linucb_route_generator_count: u64,
    /// D3 — TaskList responses that emitted a RL-ROUTER hint.
    #[serde(default)]
    pub linucb_route_hint_count: u64,
    /// D4 — MCTS shadow rollouts actually executed.
    #[serde(default)]
    pub mcts_shadow_run_count: u64,
    /// D4 — MCTS shadow rollouts that exceeded budget.
    #[serde(default)]
    pub mcts_shadow_timeout_count: u64,
    /// D4 — MCTS shadow rollouts that detected a deadlock cycle.
    #[serde(default)]
    pub mcts_shadow_deadlock_detected_count: u64,

    /// Wave 22 — Per-query ANN (HNSW) vector search latency distribution.
    ///
    /// Records only the k-NN scan time inside `cli_memory_recall`, excluding
    /// SQL retrieval and RRF merge overhead. Expected P50 < 500μs for a
    /// 64-dim U4-quantized index at typical daemon workload sizes.
    pub ann_search_latency: LatencySnapshot,

    /// Process memory footprint at snapshot time (2026-04-20).
    ///
    /// Sourced from `shared::memory_stats_probe::snapshot()`. Both fields
    /// are zero when the host OS does not expose a supported API — see
    /// the probe module for the degradation contract. `#[serde(default)]`
    /// keeps older JSON consumers parsing this struct without the field.
    #[serde(default)]
    pub memory_rss_mb: f64,
    /// Virtual-address-space size in MB; always `>= memory_rss_mb` when
    /// the probe succeeds.
    #[serde(default)]
    pub memory_virt_mb: f64,

    /// Docs enqueued to the async stream actor (Suggestion 2 — 2026-04-20).
    #[serde(default)]
    pub tantivy_stream_enqueued_count: u64,
    /// Docs dropped due to stream channel backpressure.
    #[serde(default)]
    pub tantivy_stream_backpressure_drop_count: u64,
    /// Commit operations performed by the stream actor.
    #[serde(default)]
    pub tantivy_stream_flush_count: u64,
    /// Total docs committed across all stream flushes.
    #[serde(default)]
    pub tantivy_stream_flush_docs_count: u64,
    /// Docs descartados no flush porque o índice do PROJETO não resolveu.
    /// Fecha a contabilidade do stream junto com flush_docs e backpressure_drop.
    #[serde(default)]
    pub tantivy_stream_index_unavailable_drop_count: u64,
    /// File paths enqueued for predictive prefetch after MCTS shadow rollout.
    #[serde(default)]
    pub prefetch_enqueued_count: u64,
    /// Files successfully warmed into the global parser cache by the prefetch actor.
    #[serde(default)]
    pub prefetch_warmed_count: u64,

    /// TaskCreate subjects classified as JDM class-A (code implementation).
    #[serde(default)]
    pub jdm_class_a_count: u64,
    /// TaskCreate subjects classified as JDM class-B (infra/devops).
    #[serde(default)]
    pub jdm_class_b_count: u64,
    /// TaskCreate subjects classified as JDM class-C (cognitive/architecture).
    #[serde(default)]
    pub jdm_class_c_count: u64,
    /// TaskCreate subjects classified as JDM class-D (multi-agent/orchestration).
    #[serde(default)]
    pub jdm_class_d_count: u64,

    // ── Tokio Runtime Metrics (Wave 26 — 2026-04-21) ──────────────────────
    //
    // Point-in-time gauges from the Tokio multi-threaded scheduler.
    // Not monotonic — represent current state at snapshot time.
    //
    // Diagnostic interpretation:
    // - idle > 0 AND injection > 0 → backpressure downstream
    // - idle == 0 sustained → workers undersized for workload
    // - blocking threads approaching runtime limit → blocking pool saturated
    /// Tokio scheduler worker threads (work-stealing pool size).
    #[serde(default)]
    pub tokio_num_workers: u64,

    /// Tokio worker threads currently idle (parked, no work available).
    /// Non-zero with injection_queue_depth > 0 indicates downstream backpressure.
    #[serde(default)]
    pub tokio_num_idle_threads: u64,

    /// Tokio blocking pool threads (spawn_blocking).
    /// Saturates when many long-running CPU-bound tasks are queued.
    #[serde(default)]
    pub tokio_num_blocking_threads: u64,

    /// Requests pending injection onto worker queues across all threads.
    /// Non-zero with idle > 0 → workers blocked on a slow handler.
    #[serde(default)]
    pub tokio_injection_queue_depth: u64,

    // ── RFC-100 Diagnostic Code Counters (2026-04-25) ──────────────────────
    /// RFC-100 wiring finding diagnostics emitted (W-1xx codes).
    ///
    /// Counts how often orphan/unused-pub/dead-chain diagnostics fire per
    /// daemon session. Zero-when-absent via `#[serde(default)]` for wire compat.
    #[serde(default)]
    pub diagnostic_wiring_finding_emitted_count: u64,

    /// RFC-100 TDG grade D/F diagnostics emitted (Q-201/Q-202 codes).
    ///
    /// Counts how often `cli_ast_tdg` detects a grade D or F and emits a
    /// diagnostic. Zero-when-absent via `#[serde(default)]` for wire compat.
    #[serde(default)]
    pub diagnostic_tdg_emitted_count: u64,

    /// RFC-100 B-302 PatchExpansion diagnostics emitted (B-302 code).
    ///
    /// Counts how often `pre_write::emit_b302_if_low_confidence_expansion`
    /// (typically called from `cli_mpatch_preview`) detects an mpatch fuzzy
    /// expansion below the 0.7 confidence threshold and emits a B-302
    /// diagnostic. Zero-when-absent via `#[serde(default)]`. Wave 12.
    #[serde(default)]
    pub diagnostic_b302_emitted_count: u64,
    /// F0-pre: CC-mirror containers minted by `bridge_task_created`.
    #[serde(default)]
    pub task_sync_create_count: u64,
    /// F0-pre: repeat sync arrivals deduped by `bridge_task_created`.
    #[serde(default)]
    pub task_sync_deduped_count: u64,
    /// F0-pre: TaskUpdate status changes propagated onto the mirror.
    #[serde(default)]
    pub task_sync_update_propagated_count: u64,

    /// Q-220 NonIdempotentFormat violations detected by the idempotency gate
    /// in `pre_edit`. Incremented when `format(format(x)) != format(x)`.
    /// Zero-when-absent via `#[serde(default)]`. Wave A.3.
    #[serde(default)]
    pub diagnostic_q220_nonidempotent_emitted_count: u64,

    /// W-115 SkippedRegionWritten violations detected by `post_edit`.
    /// Incremented when an Edit tool write overlapped a skip-region marker.
    /// Zero-when-absent via `#[serde(default)]`. Wave A.2.4.
    #[serde(default)]
    pub diagnostic_w115_skipped_region_written_count: u64,

    // ── ESAA activity hook counters (v8 D1.6) ────────────────────────────────
    // Fire-and-forget counters for activity events emitted via EventStore.
    // All fields carry `#[serde(default)]` so existing JSON consumers that
    // were serialized before this wave deserialize cleanly (missing = 0).
    /// pre_edit activity event emitted to activity.jsonl via EventStore.
    #[serde(default)]
    pub activity_pre_edit_count: u64,
    /// post_edit activity event emitted to activity.jsonl via EventStore.
    #[serde(default)]
    pub activity_post_edit_count: u64,
    /// post_write activity event emitted to activity.jsonl via EventStore.
    #[serde(default)]
    pub activity_post_write_count: u64,
    /// instructions_loaded activity event emitted at session_start.
    #[serde(default)]
    pub activity_instructions_loaded_count: u64,
    /// pre_compact activity event emitted before context compaction.
    #[serde(default)]
    pub activity_pre_compact_count: u64,

    // ── resource-monitor counters (W-540..W-543, 2026-05-02) ──────────────
    //
    // All fields carry `#[serde(default)]` so existing JSON consumers that
    // were serialized before this wave deserialize cleanly (missing = 0).
    /// PSI ticks classified as Green (normal memory pressure).
    #[serde(default)]
    pub memory_pressure_green_count: u64,

    /// PSI ticks classified as Yellow (elevated memory pressure, W-541).
    #[serde(default)]
    pub memory_pressure_yellow_count: u64,

    /// PSI ticks classified as Red (critical memory pressure, W-540).
    #[serde(default)]
    pub memory_pressure_red_count: u64,

    /// Total PSI ticks recorded (green + yellow + red).
    #[serde(default)]
    pub memory_pressure_total_tick_count: u64,

    /// Swap-thrashing detection events (W-542).
    #[serde(default)]
    pub swap_thrashing_detected_count: u64,

    /// Heavy spawns paused by the W-540 gate (cargo test/build, npm/pip install).
    #[serde(default)]
    pub cargo_test_paused_count: u64,

    /// Successful P-core (performance core) affinity pinnings (W-543).
    #[serde(default)]
    pub core_pinning_p_count: u64,

    /// Successful E-core (efficiency core) affinity pinnings (W-543).
    #[serde(default)]
    pub core_pinning_e_count: u64,

    // ── Wave 2026-05-08 — context-mode integration ──────────────────────
    /// D2.4 — PreToolUse intercepts that routed a tool to the sandbox
    /// subprocess (D2.2 sandbox_executor). Drives observability of the
    /// context-mode token-reduction flow.
    #[serde(default)]
    pub tool_output_routed_count: u64,
    /// D2.2 — sandbox subprocesses that hit the configured `timeout_ms`
    /// and returned the fallback sentinel (`exit_code = -2`) instead of
    /// propagating an error. Watchdog metric for timeout calibration.
    #[serde(default)]
    pub sandbox_timeout_fallback_count: u64,

    // ── Wave 2026-05-08 Master Plan — Tantivy native upgrades ────────────
    /// I-02 — successful PhraseQuery matches in `search()` (multi-term boost).
    #[serde(default)]
    pub phrase_query_match_count: u64,
    /// I-01 — substring searches via NgramTokenizer trigram field.
    #[serde(default)]
    pub tantivy_trigram_query_count: u64,
    /// I-05 — ToolOutputsIndex stores skipped due to TTL freshness.
    #[serde(default)]
    pub tool_outputs_ttl_skip_count: u64,
    /// I-05 — ToolOutputsIndex docs deleted by retention cleanup actor.
    #[serde(default)]
    pub tool_outputs_cleanup_deleted_count: u64,
    /// NEW-2 — Sandbox failures persisted via tee mode (exit_code != 0).
    #[serde(default)]
    pub sandbox_tee_persisted_count: u64,
    /// NEW-1 — Per-command compression profile applications (any profile).
    #[serde(default)]
    pub compression_profile_applied_count: u64,
    // ── Wave 3 INTELLIGENCE — 19 T1 snapshot fields ────────────────────────
    /// Snapshot of `ctx_replay` invocations.
    #[serde(default)]
    pub ctx_replay_count: u64,
    /// Snapshot of `ctx_purge` invocations.
    #[serde(default)]
    pub ctx_purge_count: u64,
    /// Snapshot of `ctx_doctor` invocations.
    #[serde(default)]
    pub ctx_doctor_call_count: u64,
    /// Snapshot of daily gate-metrics flush operations.
    #[serde(default)]
    pub gate_metrics_daily_flush_count: u64,
    /// Snapshot of `ctx_gain_graph` invocations.
    #[serde(default)]
    pub ctx_gain_graph_count: u64,
    /// Snapshot of `ctx` session-adoption queries.
    #[serde(default)]
    pub ctx_session_adoption_query_count: u64,
    /// Snapshot of `touring init` invocations.
    #[serde(default)]
    pub touring_init_invocation_count: u64,
    /// Snapshot of `ctx_smart` invocations.
    #[serde(default)]
    pub ctx_smart_count: u64,
    /// Snapshot of aggressive reads served via chunking.
    #[serde(default)]
    pub read_aggressive_chunked_count: u64,
    /// Snapshot of aggressive reads served via passthrough.
    #[serde(default)]
    pub read_aggressive_passthrough_count: u64,
    /// Snapshot of `ctx_explain` invocations.
    #[serde(default)]
    pub ctx_explain_count: u64,
    /// Snapshot of context-budget warnings emitted.
    #[serde(default)]
    pub ctx_budget_warning_emitted_count: u64,
    /// Snapshot of context-budget alerts emitted.
    #[serde(default)]
    pub ctx_budget_alert_emitted_count: u64,
    /// Snapshot of `ctx_batch_execute` invocations.
    #[serde(default)]
    pub ctx_batch_execute_count: u64,
    /// Snapshot of total items processed by `ctx_batch_execute`.
    #[serde(default)]
    pub ctx_batch_item_count: u64,
    /// Snapshot of `ctx_execute_file` invocations.
    #[serde(default)]
    pub ctx_execute_file_count: u64,
    /// Snapshot of `ctx_upgrade` invocations.
    #[serde(default)]
    pub ctx_upgrade_count: u64,
    /// Snapshot of `ctx_discover_session` invocations.
    #[serde(default)]
    pub ctx_discover_session_count: u64,
    /// Snapshot of missed opportunities surfaced by session discovery.
    #[serde(default)]
    pub ctx_discover_opportunities_found: u64,
    /// Snapshot of Wave 3 Tier-2 feature T201 invocations.
    #[serde(default)]
    pub wave3_t201_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T202 invocations.
    #[serde(default)]
    pub wave3_t202_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T203 invocations.
    #[serde(default)]
    pub wave3_t203_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T204 invocations.
    #[serde(default)]
    pub wave3_t204_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T205 invocations.
    #[serde(default)]
    pub wave3_t205_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T206 invocations.
    #[serde(default)]
    pub wave3_t206_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T207 invocations.
    #[serde(default)]
    pub wave3_t207_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T208 invocations.
    #[serde(default)]
    pub wave3_t208_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T209 invocations.
    #[serde(default)]
    pub wave3_t209_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T210 invocations.
    #[serde(default)]
    pub wave3_t210_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T211 invocations.
    #[serde(default)]
    pub wave3_t211_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T212 invocations.
    #[serde(default)]
    pub wave3_t212_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T213 invocations.
    #[serde(default)]
    pub wave3_t213_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T214 invocations.
    #[serde(default)]
    pub wave3_t214_count: u64,
    /// Snapshot of Wave 3 Tier-2 feature T215 invocations.
    #[serde(default)]
    pub wave3_t215_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T301 invocations.
    #[serde(default)]
    pub wave3_t301_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T302 invocations.
    #[serde(default)]
    pub wave3_t302_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T303 invocations.
    #[serde(default)]
    pub wave3_t303_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T304 invocations.
    #[serde(default)]
    pub wave3_t304_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T305 invocations.
    #[serde(default)]
    pub wave3_t305_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T306 invocations.
    #[serde(default)]
    pub wave3_t306_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T307 invocations.
    #[serde(default)]
    pub wave3_t307_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T308 invocations.
    #[serde(default)]
    pub wave3_t308_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T309 invocations.
    #[serde(default)]
    pub wave3_t309_count: u64,
    /// Snapshot of Wave 3 Tier-3 feature T310 invocations.
    #[serde(default)]
    pub wave3_t310_count: u64,
    // ── CEG Pln2 FASE 5a — P7.1 snapshot fields ─────────────────────────────
    /// Snapshot of CEG X0 CAPTURE events (code-bearing actions entering the pipeline).
    #[serde(default)]
    pub ceg_captured_count: u64,
    /// Snapshot of CEG X7 DECISION denials (actions blocked).
    #[serde(default)]
    pub ceg_blocked_count: u64,
    /// Snapshot of CEG X5 SANDBOX dry-runs executed.
    #[serde(default)]
    pub ceg_sandboxed_count: u64,
    /// Snapshot of CEG fast-path (provably-pure) skips.
    #[serde(default)]
    pub ceg_fast_path_count: u64,
    /// Snapshot of workflow antipatterns detected.
    #[serde(default)]
    pub workflow_antipattern_detected_count: u64,
    /// Snapshot of workflow advice messages injected.
    #[serde(default)]
    pub workflow_advice_emitted_count: u64,
    /// Snapshot of antipatterns converted to their canonical form.
    #[serde(default)]
    pub antipattern_converted_count: u64,

    // ── TR-2 — STR (Signal-to-Token Ratio) observability ─────────────────────
    //
    // STR proxy: mean_enrichment_bytes = enrichment_context_bytes_total / enrichment_emit_count
    //
    // A lower mean_enrichment_bytes = higher STR (more signal per injected token).
    // Derived field `enrichment_mean_bytes_per_emit` is pre-computed in capture()
    // for direct consumption by dashboards without division.
    //
    // Reference: Rn2 §11 TR-2.
    /// Cumulative bytes of `additionalContext` injected by all enrichment hooks.
    /// `#[serde(default)]` keeps snapshots from prior daemon versions deserializable.
    #[serde(default)]
    pub enrichment_context_bytes_total: u64,

    /// Number of enrichment injections that emitted a non-empty context string.
    /// `#[serde(default)]` keeps snapshots from prior daemon versions deserializable.
    #[serde(default)]
    pub enrichment_emit_count: u64,

    /// **F2** — coupling suggestion-uptake emitted (denominator).
    /// `#[serde(default)]` keeps legacy snapshots without this field deserializable.
    #[serde(default)]
    pub suggestion_uptake_emitted_count: u64,

    /// **F2** — coupling suggestion-uptake followed (numerator).
    /// `#[serde(default)]` keeps legacy snapshots without this field deserializable.
    #[serde(default)]
    pub suggestion_uptake_followed_count: u64,

    /// **F3** — coupling adoption_ratio numerator: Bash actions invoking `touring`.
    /// `#[serde(default)]` keeps legacy snapshots without this field deserializable.
    #[serde(default)]
    pub adoption_touring_count: u64,

    /// **F3** — coupling adoption_ratio denominator: raw-bash antipattern actions.
    /// `#[serde(default)]` keeps legacy snapshots without this field deserializable.
    #[serde(default)]
    pub adoption_antipattern_count: u64,

    /// **Task #6** — pillar-induction emitted (denominator of
    /// `touring.coupling.pillar_induction_ratio`).
    /// `#[serde(default)]` keeps legacy snapshots without this field deserializable.
    #[serde(default)]
    pub pillar_induction_emitted_count: u64,

    /// **Task #6** — pillar-induction followed (numerator): nudges whose next action
    /// used a master/recall command.
    /// `#[serde(default)]` keeps legacy snapshots without this field deserializable.
    #[serde(default)]
    pub pillar_induction_followed_count: u64,

    /// Pre-computed mean bytes per enrichment emission.
    ///
    /// `enrichment_context_bytes_total / enrichment_emit_count`, or `0.0` when
    /// `enrichment_emit_count == 0` (no enrichment has fired yet).
    /// **Inverse proxy for STR**: lower values signal leaner, higher-ratio context.
    /// `#[serde(default)]` keeps snapshots from prior daemon versions deserializable.
    #[serde(default)]
    pub enrichment_mean_bytes_per_emit: f64,
}

impl GateMetricsSnapshot {
    /// Capture a consistent snapshot of the global metrics.
    ///
    /// The snapshot is not atomic across all counters (individual loads are
    /// atomic but the set is not). For monotonic counters this is acceptable
    /// — the ratios may be slightly skewed if updates race with the snapshot.
    pub fn capture() -> Self {
        let m = global();
        let pre_edit_fast = m.pre_edit_fast_path_count.load(Ordering::Relaxed);
        let pre_edit_full = m.pre_edit_full_enrichment_count.load(Ordering::Relaxed);
        let pre_write_fast = m.pre_write_fast_path_count.load(Ordering::Relaxed);
        let pre_write_full = m.pre_write_full_enrichment_count.load(Ordering::Relaxed);
        let post_tool_l4 = m.post_tool_l4_mandatory_count.load(Ordering::Relaxed);
        let metadata_cache_hit = m.metadata_cache_hit_count.load(Ordering::Relaxed);
        let metadata_bp_dropped = m
            .metadata_backpressure_dropped_count
            .load(Ordering::Relaxed);
        let pre_grep_enrichment = m.pre_grep_enrichment_count.load(Ordering::Relaxed);
        let pre_grep_zero_results = m.pre_grep_zero_results_count.load(Ordering::Relaxed);
        let tantivy_upsert_count = m.tantivy_upsert_count.load(Ordering::Relaxed);
        let tantivy_query_latency_us = m.tantivy_query_latency_us.load(Ordering::Relaxed);
        let tantivy_commit_count = m.tantivy_commit_count.load(Ordering::Relaxed);
        let reindex_failure_count = m.reindex_failure_count.load(Ordering::Relaxed);
        let rkyv_dispatch_count = m.rkyv_dispatch_count.load(Ordering::Relaxed);
        let rkyv_dispatch_bytes = m.rkyv_dispatch_bytes.load(Ordering::Relaxed);
        let rkyv_parse_error_count = m.rkyv_parse_error_count.load(Ordering::Relaxed);
        let rkyv_response_count = m.rkyv_response_count.load(Ordering::Relaxed);
        let rkyv_mean_bytes = if rkyv_dispatch_count > 0 {
            rkyv_dispatch_bytes as f64 / rkyv_dispatch_count as f64
        } else {
            0.0
        };

        let pre_edit_total = pre_edit_fast + pre_edit_full;
        let pre_write_total = pre_write_fast + pre_write_full;
        let pre_edit_fast_ratio = if pre_edit_total > 0 {
            pre_edit_fast as f64 / pre_edit_total as f64
        } else {
            0.0
        };
        let pre_write_fast_ratio = if pre_write_total > 0 {
            pre_write_fast as f64 / pre_write_total as f64
        } else {
            0.0
        };

        Self {
            pre_edit_fast_path: pre_edit_fast,
            pre_edit_full,
            pre_edit_fast_ratio,
            pre_write_fast_path: pre_write_fast,
            pre_write_full,
            pre_write_fast_ratio,
            post_tool_l4_mandatory: post_tool_l4,
            metadata_cache_hit,
            metadata_backpressure_dropped: metadata_bp_dropped,
            pre_grep_enrichment_count: pre_grep_enrichment,
            pre_grep_zero_results_count: pre_grep_zero_results,
            total_invocations: pre_edit_total + pre_write_total,
            tantivy_upsert_count,
            tantivy_query_latency_us,
            tantivy_commit_count,
            reindex_failure_count,
            rkyv_dispatch_count,
            rkyv_dispatch_bytes,
            rkyv_parse_error_count,
            rkyv_response_count,
            rkyv_mean_bytes,
            rkyv_dispatch_latency: m.rkyv_dispatch_latency.snapshot(),
            hook_dispatch_latency: m.hook_dispatch_latency.snapshot(),
            tantivy_query_latency: m.tantivy_query_latency_hist.snapshot(),
            actor_queue_wait: m.actor_queue_wait.snapshot(),
            actor_budget_timeout_count: m.actor_budget_timeout_count.load(Ordering::Relaxed),
            actor_send_timeout_count: m.actor_send_timeout_count.load(Ordering::Relaxed),
            health_delta_record_count: {
                let r = m.health_delta_record_count.load(Ordering::Relaxed);
                let _ = r;
                r
            },
            health_delta_compute_count: m.health_delta_compute_count.load(Ordering::Relaxed),
            health_delta_regression_count: m.health_delta_regression_count.load(Ordering::Relaxed),
            health_delta_improvement_count: m
                .health_delta_improvement_count
                .load(Ordering::Relaxed),
            health_delta_outstanding: m
                .health_delta_record_count
                .load(Ordering::Relaxed)
                .saturating_sub(m.health_delta_compute_count.load(Ordering::Relaxed)),
            health_delta_streak_alert_count: m
                .health_delta_streak_alert_count
                .load(Ordering::Relaxed),
            health_delta_recovery_count: m.health_delta_recovery_count.load(Ordering::Relaxed),
            query_cache_hit_count: m.query_cache_hit_count.load(Ordering::Relaxed),
            query_cache_miss_count: m.query_cache_miss_count.load(Ordering::Relaxed),
            query_cache_hit_ratio: {
                let h = m.query_cache_hit_count.load(Ordering::Relaxed);
                let mi = m.query_cache_miss_count.load(Ordering::Relaxed);
                let total = h + mi;
                if total == 0 {
                    0.0
                } else {
                    h as f64 / total as f64
                }
            },
            query_cache_invalidate_count: m.query_cache_invalidate_count.load(Ordering::Relaxed),
            blast_inject_count: m.blast_inject_count.load(Ordering::Relaxed),
            blast_timeout_count: m.blast_timeout_count.load(Ordering::Relaxed),
            blast_mutation_count: m.blast_mutation_count.load(Ordering::Relaxed),
            linucb_route_manual_count: m.linucb_route_manual_count.load(Ordering::Relaxed),
            linucb_route_generator_count: m.linucb_route_generator_count.load(Ordering::Relaxed),
            linucb_route_hint_count: m.linucb_route_hint_count.load(Ordering::Relaxed),
            mcts_shadow_run_count: m.mcts_shadow_run_count.load(Ordering::Relaxed),
            mcts_shadow_timeout_count: m.mcts_shadow_timeout_count.load(Ordering::Relaxed),
            mcts_shadow_deadlock_detected_count: m
                .mcts_shadow_deadlock_detected_count
                .load(Ordering::Relaxed),
            ann_search_latency: m.ann_search_latency.snapshot(),
            // memory_stats_probe hits `/proc/self/statm` (Linux) or the
            // equivalent syscall; <5μs in steady state, acceptable to run
            // on every snapshot() call.
            memory_rss_mb: {
                let mem = crate::memory_stats_probe::snapshot();
                mem.physical_mb
            },
            memory_virt_mb: crate::memory_stats_probe::snapshot().virtual_mb,
            tantivy_stream_enqueued_count: m.tantivy_stream_enqueued_count.load(Ordering::Relaxed),
            tantivy_stream_backpressure_drop_count: m
                .tantivy_stream_backpressure_drop_count
                .load(Ordering::Relaxed),
            tantivy_stream_flush_count: m.tantivy_stream_flush_count.load(Ordering::Relaxed),
            tantivy_stream_flush_docs_count: m
                .tantivy_stream_flush_docs_count
                .load(Ordering::Relaxed),
            tantivy_stream_index_unavailable_drop_count: m
                .tantivy_stream_index_unavailable_drop_count
                .load(Ordering::Relaxed),
            prefetch_enqueued_count: m.prefetch_enqueued_count.load(Ordering::Relaxed),
            prefetch_warmed_count: m.prefetch_warmed_count.load(Ordering::Relaxed),
            jdm_class_a_count: m.jdm_class_a_count.load(Ordering::Relaxed),
            jdm_class_b_count: m.jdm_class_b_count.load(Ordering::Relaxed),
            jdm_class_c_count: m.jdm_class_c_count.load(Ordering::Relaxed),
            jdm_class_d_count: m.jdm_class_d_count.load(Ordering::Relaxed),
            // Tokio runtime metrics — point-in-time gauges, not monotonic
            tokio_num_workers: m.tokio_num_workers.load(Ordering::Relaxed),
            tokio_num_idle_threads: m.tokio_num_idle_threads.load(Ordering::Relaxed),
            tokio_num_blocking_threads: m.tokio_num_blocking_threads.load(Ordering::Relaxed),
            tokio_injection_queue_depth: m.tokio_injection_queue_depth.load(Ordering::Relaxed),
            // RFC-100 diagnostic counters (2026-04-25)
            diagnostic_wiring_finding_emitted_count: m
                .diagnostic_wiring_finding_emitted_count
                .load(Ordering::Relaxed),
            diagnostic_tdg_emitted_count: m.diagnostic_tdg_emitted_count.load(Ordering::Relaxed),
            diagnostic_b302_emitted_count: m.diagnostic_b302_emitted_count.load(Ordering::Relaxed),
            task_sync_create_count: m.task_sync_create_count.load(Ordering::Relaxed),
            task_sync_deduped_count: m.task_sync_deduped_count.load(Ordering::Relaxed),
            task_sync_update_propagated_count: m
                .task_sync_update_propagated_count
                .load(Ordering::Relaxed),
            diagnostic_q220_nonidempotent_emitted_count: m
                .diagnostic_q220_nonidempotent_emitted_count
                .load(Ordering::Relaxed),
            diagnostic_w115_skipped_region_written_count: m
                .diagnostic_w115_skipped_region_written_count
                .load(Ordering::Relaxed),
            activity_pre_edit_count: m.activity_pre_edit_count.load(Ordering::Relaxed),
            activity_post_edit_count: m.activity_post_edit_count.load(Ordering::Relaxed),
            activity_post_write_count: m.activity_post_write_count.load(Ordering::Relaxed),
            activity_instructions_loaded_count: m
                .activity_instructions_loaded_count
                .load(Ordering::Relaxed),
            activity_pre_compact_count: m.activity_pre_compact_count.load(Ordering::Relaxed),
            memory_pressure_green_count: m.memory_pressure_green_count.load(Ordering::Relaxed),
            memory_pressure_yellow_count: m.memory_pressure_yellow_count.load(Ordering::Relaxed),
            memory_pressure_red_count: m.memory_pressure_red_count.load(Ordering::Relaxed),
            memory_pressure_total_tick_count: m
                .memory_pressure_total_tick_count
                .load(Ordering::Relaxed),
            swap_thrashing_detected_count: m.swap_thrashing_detected_count.load(Ordering::Relaxed),
            cargo_test_paused_count: m.cargo_test_paused_count.load(Ordering::Relaxed),
            core_pinning_p_count: m.core_pinning_p_count.load(Ordering::Relaxed),
            core_pinning_e_count: m.core_pinning_e_count.load(Ordering::Relaxed),
            // Wave 2026-05-08 — context-mode integration counters.
            tool_output_routed_count: m.tool_output_routed_count.load(Ordering::Relaxed),
            sandbox_timeout_fallback_count: m
                .sandbox_timeout_fallback_count
                .load(Ordering::Relaxed),
            // Wave 2026-05-08 Master Plan — Tantivy native upgrades.
            phrase_query_match_count: m.phrase_query_match_count.load(Ordering::Relaxed),
            tantivy_trigram_query_count: m.tantivy_trigram_query_count.load(Ordering::Relaxed),
            tool_outputs_ttl_skip_count: m.tool_outputs_ttl_skip_count.load(Ordering::Relaxed),
            tool_outputs_cleanup_deleted_count: m
                .tool_outputs_cleanup_deleted_count
                .load(Ordering::Relaxed),
            sandbox_tee_persisted_count: m.sandbox_tee_persisted_count.load(Ordering::Relaxed),
            compression_profile_applied_count: m
                .compression_profile_applied_count
                .load(Ordering::Relaxed),
            // Wave 3 T1 — 19 snapshot reads
            ctx_replay_count: m.ctx_replay_count.load(Ordering::Relaxed),
            ctx_purge_count: m.ctx_purge_count.load(Ordering::Relaxed),
            ctx_doctor_call_count: m.ctx_doctor_call_count.load(Ordering::Relaxed),
            gate_metrics_daily_flush_count: m
                .gate_metrics_daily_flush_count
                .load(Ordering::Relaxed),
            ctx_gain_graph_count: m.ctx_gain_graph_count.load(Ordering::Relaxed),
            ctx_session_adoption_query_count: m
                .ctx_session_adoption_query_count
                .load(Ordering::Relaxed),
            touring_init_invocation_count: m.touring_init_invocation_count.load(Ordering::Relaxed),
            ctx_smart_count: m.ctx_smart_count.load(Ordering::Relaxed),
            read_aggressive_chunked_count: m.read_aggressive_chunked_count.load(Ordering::Relaxed),
            read_aggressive_passthrough_count: m
                .read_aggressive_passthrough_count
                .load(Ordering::Relaxed),
            ctx_explain_count: m.ctx_explain_count.load(Ordering::Relaxed),
            ctx_budget_warning_emitted_count: m
                .ctx_budget_warning_emitted_count
                .load(Ordering::Relaxed),
            ctx_budget_alert_emitted_count: m
                .ctx_budget_alert_emitted_count
                .load(Ordering::Relaxed),
            ctx_batch_execute_count: m.ctx_batch_execute_count.load(Ordering::Relaxed),
            ctx_batch_item_count: m.ctx_batch_item_count.load(Ordering::Relaxed),
            ctx_execute_file_count: m.ctx_execute_file_count.load(Ordering::Relaxed),
            ctx_upgrade_count: m.ctx_upgrade_count.load(Ordering::Relaxed),
            ctx_discover_session_count: m.ctx_discover_session_count.load(Ordering::Relaxed),
            ctx_discover_opportunities_found: m
                .ctx_discover_opportunities_found
                .load(Ordering::Relaxed),
            wave3_t201_count: m.wave3_t201_count.load(Ordering::Relaxed),
            wave3_t202_count: m.wave3_t202_count.load(Ordering::Relaxed),
            wave3_t203_count: m.wave3_t203_count.load(Ordering::Relaxed),
            wave3_t204_count: m.wave3_t204_count.load(Ordering::Relaxed),
            wave3_t205_count: m.wave3_t205_count.load(Ordering::Relaxed),
            wave3_t206_count: m.wave3_t206_count.load(Ordering::Relaxed),
            wave3_t207_count: m.wave3_t207_count.load(Ordering::Relaxed),
            wave3_t208_count: m.wave3_t208_count.load(Ordering::Relaxed),
            wave3_t209_count: m.wave3_t209_count.load(Ordering::Relaxed),
            wave3_t210_count: m.wave3_t210_count.load(Ordering::Relaxed),
            wave3_t211_count: m.wave3_t211_count.load(Ordering::Relaxed),
            wave3_t212_count: m.wave3_t212_count.load(Ordering::Relaxed),
            wave3_t213_count: m.wave3_t213_count.load(Ordering::Relaxed),
            wave3_t214_count: m.wave3_t214_count.load(Ordering::Relaxed),
            wave3_t215_count: m.wave3_t215_count.load(Ordering::Relaxed),
            wave3_t301_count: m.wave3_t301_count.load(Ordering::Relaxed),
            wave3_t302_count: m.wave3_t302_count.load(Ordering::Relaxed),
            wave3_t303_count: m.wave3_t303_count.load(Ordering::Relaxed),
            wave3_t304_count: m.wave3_t304_count.load(Ordering::Relaxed),
            wave3_t305_count: m.wave3_t305_count.load(Ordering::Relaxed),
            wave3_t306_count: m.wave3_t306_count.load(Ordering::Relaxed),
            wave3_t307_count: m.wave3_t307_count.load(Ordering::Relaxed),
            wave3_t308_count: m.wave3_t308_count.load(Ordering::Relaxed),
            wave3_t309_count: m.wave3_t309_count.load(Ordering::Relaxed),
            wave3_t310_count: m.wave3_t310_count.load(Ordering::Relaxed),
            // CEG Pln2 FASE 5a — P7.1
            ceg_captured_count: m.ceg_captured_count.load(Ordering::Relaxed),
            ceg_blocked_count: m.ceg_blocked_count.load(Ordering::Relaxed),
            ceg_sandboxed_count: m.ceg_sandboxed_count.load(Ordering::Relaxed),
            ceg_fast_path_count: m.ceg_fast_path_count.load(Ordering::Relaxed),
            workflow_antipattern_detected_count: m
                .workflow_antipattern_detected_count
                .load(Ordering::Relaxed),
            workflow_advice_emitted_count: m.workflow_advice_emitted_count.load(Ordering::Relaxed),
            antipattern_converted_count: m.antipattern_converted_count.load(Ordering::Relaxed),
            // TR-2 — STR observability
            enrichment_context_bytes_total: m
                .enrichment_context_bytes_total
                .load(Ordering::Relaxed),
            enrichment_emit_count: { m.enrichment_emit_count.load(Ordering::Relaxed) },
            suggestion_uptake_emitted_count: m
                .suggestion_uptake_emitted_count
                .load(Ordering::Relaxed),
            suggestion_uptake_followed_count: m
                .suggestion_uptake_followed_count
                .load(Ordering::Relaxed),
            adoption_touring_count: m.adoption_touring_count.load(Ordering::Relaxed),
            adoption_antipattern_count: m.adoption_antipattern_count.load(Ordering::Relaxed),
            pillar_induction_emitted_count: m
                .pillar_induction_emitted_count
                .load(Ordering::Relaxed),
            pillar_induction_followed_count: m
                .pillar_induction_followed_count
                .load(Ordering::Relaxed),
            enrichment_mean_bytes_per_emit: {
                let bytes = m.enrichment_context_bytes_total.load(Ordering::Relaxed);
                let count = m.enrichment_emit_count.load(Ordering::Relaxed);
                if count == 0 {
                    0.0
                } else {
                    bytes as f64 / count as f64
                }
            },
        }
    }
}

// ─── Wave 3 Extended (T2 + T3) — 25 record helpers ──────────────────────────
/// Increments the Wave 3 Tier-2 feature T201 counter.
pub fn record_wave3_t201() {
    global().wave3_t201_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T202 counter.
pub fn record_wave3_t202() {
    global().wave3_t202_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T203 counter.
pub fn record_wave3_t203() {
    global().wave3_t203_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T204 counter.
pub fn record_wave3_t204() {
    global().wave3_t204_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T205 counter.
pub fn record_wave3_t205() {
    global().wave3_t205_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T206 counter.
pub fn record_wave3_t206() {
    global().wave3_t206_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T207 counter.
pub fn record_wave3_t207() {
    global().wave3_t207_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T208 counter.
pub fn record_wave3_t208() {
    global().wave3_t208_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T209 counter.
pub fn record_wave3_t209() {
    global().wave3_t209_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T210 counter.
pub fn record_wave3_t210() {
    global().wave3_t210_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T211 counter.
pub fn record_wave3_t211() {
    global().wave3_t211_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T212 counter.
pub fn record_wave3_t212() {
    global().wave3_t212_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T213 counter.
pub fn record_wave3_t213() {
    global().wave3_t213_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T214 counter.
pub fn record_wave3_t214() {
    global().wave3_t214_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-2 feature T215 counter.
pub fn record_wave3_t215() {
    global().wave3_t215_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T301 counter.
pub fn record_wave3_t301() {
    global().wave3_t301_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T302 counter.
pub fn record_wave3_t302() {
    global().wave3_t302_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T303 counter.
pub fn record_wave3_t303() {
    global().wave3_t303_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T304 counter.
pub fn record_wave3_t304() {
    global().wave3_t304_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T305 counter.
pub fn record_wave3_t305() {
    global().wave3_t305_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T306 counter.
pub fn record_wave3_t306() {
    global().wave3_t306_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T307 counter.
pub fn record_wave3_t307() {
    global().wave3_t307_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T308 counter.
pub fn record_wave3_t308() {
    global().wave3_t308_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T309 counter.
pub fn record_wave3_t309() {
    global().wave3_t309_count.fetch_add(1, Ordering::Relaxed);
}
/// Increments the Wave 3 Tier-3 feature T310 counter.
pub fn record_wave3_t310() {
    global().wave3_t310_count.fetch_add(1, Ordering::Relaxed);
}

// ─── CEG Pln2 FASE 5a — P7.1 record helpers ─────────────────────────────────

/// X0 CAPTURE — increment the CEG captured (entered pipeline) counter.
#[inline]
pub fn record_ceg_captured() {
    global().ceg_captured_count.fetch_add(1, Ordering::Relaxed);
}

/// X7 DECISION — increment the CEG blocked (Deny verdict) counter.
#[inline]
pub fn record_ceg_blocked() {
    global().ceg_blocked_count.fetch_add(1, Ordering::Relaxed);
}

/// X5 SANDBOX — increment the CEG sandboxed (routed to dry-run) counter.
#[inline]
pub fn record_ceg_sandboxed() {
    global().ceg_sandboxed_count.fetch_add(1, Ordering::Relaxed);
}

/// X6 CAPABILITY-GATE — increment the fast-path (provably-pure skip) counter.
#[inline]
pub fn record_ceg_fast_path() {
    global().ceg_fast_path_count.fetch_add(1, Ordering::Relaxed);
}

/// X2 STATIC / X3 VGP — increment the workflow anti-pattern detected counter.
#[inline]
pub fn record_workflow_antipattern_detected() {
    global()
        .workflow_antipattern_detected_count
        .fetch_add(1, Ordering::Relaxed);
}

/// X9 LEARN — increment the workflow advice emitted (Warn path) counter.
#[inline]
pub fn record_workflow_advice_emitted() {
    global()
        .workflow_advice_emitted_count
        .fetch_add(1, Ordering::Relaxed);
}

/// X9 LEARN — increment the anti-pattern converted counter (blocked → allowed).
#[inline]
pub fn record_antipattern_converted() {
    global()
        .antipattern_converted_count
        .fetch_add(1, Ordering::Relaxed);
}

// ── TR-2 — STR (Signal-to-Token Ratio) enrichment counter ────────────────────

/// Record a non-empty enrichment context emission from any hook.
///
/// Call this immediately after an `additionalContext` string is assembled and
/// confirmed non-empty — typically in `cli_suggester::run` and pre_grep.
///
/// # Arguments
///
/// * `bytes` — `context.len()` of the assembled `additionalContext` string
///   (UTF-8 byte length, which is a faithful proxy for token count).
///
/// # STR Observable
///
/// The **Signal-to-Token Ratio** (STR) measures how much actionable signal is
/// delivered per token injected.  Approximated here as:
///
/// ```text
/// mean_enrichment_bytes = enrichment_context_bytes_total / enrichment_emit_count
/// ```
///
/// A **lower** mean indicates higher STR — the hook injects leaner, more
/// targeted context.  Monitor via `touring gate-metrics -j`:
///
/// ```json
/// {
///   "enrichment_context_bytes_total": 48200,
///   "enrichment_emit_count": 37,
///   "enrichment_mean_bytes_per_emit": 1302.7
/// }
/// ```
///
/// # Fail-open
///
/// Both increments use `Ordering::Relaxed` — lock-free, infallible, ~1 ns each.
#[inline]
pub fn record_enrichment_emitted(bytes: usize) {
    let m = global();
    m.enrichment_context_bytes_total
        .fetch_add(bytes as u64, Ordering::Relaxed);
    m.enrichment_emit_count.fetch_add(1, Ordering::Relaxed);
}

/// F2 — record that a `cli_suggester` redirect suggestion was emitted
/// (denominator of `touring.coupling.suggestion_uptake`).
pub fn record_suggestion_emitted() {
    global()
        .suggestion_uptake_emitted_count
        .fetch_add(1, Ordering::Relaxed);
}

/// F2 — record that the action following a redirect suggestion followed it — a
/// `touring`/code-mode command (numerator of `touring.coupling.suggestion_uptake`).
pub fn record_suggestion_followed() {
    global()
        .suggestion_uptake_followed_count
        .fetch_add(1, Ordering::Relaxed);
}

/// F3 — record a touring-canonical Bash action — a `touring` invocation
/// (numerator of `touring.coupling.adoption_ratio`).
pub fn record_adoption_touring() {
    global()
        .adoption_touring_count
        .fetch_add(1, Ordering::Relaxed);
}

/// F3 — record a raw-bash inspection antipattern action (grep/cat/find/sed —
/// denominator contribution of `touring.coupling.adoption_ratio`).
pub fn record_adoption_antipattern() {
    global()
        .adoption_antipattern_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Task #6 — record that an armed pillar-induction nudge was emitted (denominator
/// of `touring.coupling.pillar_induction_ratio`).
pub fn record_pillar_induction_emitted() {
    global()
        .pillar_induction_emitted_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Task #6 — record that the action following a pillar nudge used a master/recall
/// command (numerator of `touring.coupling.pillar_induction_ratio`).
pub fn record_pillar_induction_followed() {
    global()
        .pillar_induction_followed_count
        .fetch_add(1, Ordering::Relaxed);
}

// ─── Wave 3 T1 — record helpers (one per counter) ───────────────────────────

/// T1-01: increment ctx_replay invocation counter.
pub fn record_ctx_replay() {
    global().ctx_replay_count.fetch_add(1, Ordering::Relaxed);
}
/// T1-02: increment ctx_purge invocation counter.
pub fn record_ctx_purge() {
    global().ctx_purge_count.fetch_add(1, Ordering::Relaxed);
}
/// T1-03: increment ctx_doctor invocation counter.
pub fn record_ctx_doctor_call() {
    global()
        .ctx_doctor_call_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-04: increment daily flush counter.
pub fn record_gate_metrics_daily_flush() {
    global()
        .gate_metrics_daily_flush_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-05: increment ctx_gain_graph invocation.
pub fn record_ctx_gain_graph() {
    global()
        .ctx_gain_graph_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-06: increment session adoption query.
pub fn record_ctx_session_adoption_query() {
    global()
        .ctx_session_adoption_query_count
        .fetch_add(1, Ordering::Relaxed);
}
/// P5-EXTENSION (2026-06-03): increment ctx_execute invocation counter.
/// Closes the 5th orphan counter (P3-NO-OP pattern) — the field was
/// declared at gate_metrics.rs:585 + init at 805 but NO record function
/// existed prior to this wave. Used by `cli_ctx_execute` (cli_handlers.rs)
/// on every successful ctx_execute invocation.
pub fn record_ctx_execute_file_count() {
    global()
        .ctx_execute_file_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-07: increment touring init invocation.
pub fn record_touring_init_invocation() {
    global()
        .touring_init_invocation_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-08: increment ctx_smart invocation.
pub fn record_ctx_smart() {
    global().ctx_smart_count.fetch_add(1, Ordering::Relaxed);
}
/// T1-09: increment chunked count.
pub fn record_read_aggressive_chunked() {
    global()
        .read_aggressive_chunked_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-09: increment passthrough count.
pub fn record_read_aggressive_passthrough() {
    global()
        .read_aggressive_passthrough_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-10: increment ctx_explain invocation.
pub fn record_ctx_explain() {
    global().ctx_explain_count.fetch_add(1, Ordering::Relaxed);
}
/// T1-11: increment budget warning emission.
pub fn record_ctx_budget_warning() {
    global()
        .ctx_budget_warning_emitted_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-11: increment budget alert emission.
pub fn record_ctx_budget_alert() {
    global()
        .ctx_budget_alert_emitted_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-12: increment ctx_batch_execute invocation.
pub fn record_ctx_batch_execute(items: u64) {
    global()
        .ctx_batch_execute_count
        .fetch_add(1, Ordering::Relaxed);
    global()
        .ctx_batch_item_count
        .fetch_add(items, Ordering::Relaxed);
}
/// T1-13: increment ctx_execute_file invocation.
pub fn record_ctx_execute_file() {
    global()
        .ctx_execute_file_count
        .fetch_add(1, Ordering::Relaxed);
}
/// T1-14: increment ctx_upgrade invocation.
pub fn record_ctx_upgrade() {
    global().ctx_upgrade_count.fetch_add(1, Ordering::Relaxed);
}
/// T1-15: increment discover session invocation + add opportunities found.
pub fn record_ctx_discover_session(opportunities: u64) {
    global()
        .ctx_discover_session_count
        .fetch_add(1, Ordering::Relaxed);
    global()
        .ctx_discover_opportunities_found
        .fetch_add(opportunities, Ordering::Relaxed);
}

#[cfg(test)]
mod coupling_telemetry_counter_tests {
    use super::*;

    #[test]
    fn suggestion_uptake_counters_flow_through_snapshot() {
        // Delta-based (the GateMetrics singleton is process-wide): only this test
        // touches these two F2 counters, so the deltas are deterministic.
        let before = GateMetricsSnapshot::capture();
        record_suggestion_emitted();
        record_suggestion_emitted();
        record_suggestion_followed();
        let after = GateMetricsSnapshot::capture();
        assert_eq!(
            after.suggestion_uptake_emitted_count,
            before.suggestion_uptake_emitted_count + 2
        );
        assert_eq!(
            after.suggestion_uptake_followed_count,
            before.suggestion_uptake_followed_count + 1
        );
    }

    #[test]
    fn adoption_counters_flow_through_snapshot() {
        // F3 — delta-based: only this test touches the two adoption counters, so
        // the deltas are deterministic despite the process-wide singleton.
        let before = GateMetricsSnapshot::capture();
        record_adoption_touring();
        record_adoption_touring();
        record_adoption_touring();
        record_adoption_antipattern();
        let after = GateMetricsSnapshot::capture();
        assert_eq!(
            after.adoption_touring_count,
            before.adoption_touring_count + 3
        );
        assert_eq!(
            after.adoption_antipattern_count,
            before.adoption_antipattern_count + 1
        );
    }
}
