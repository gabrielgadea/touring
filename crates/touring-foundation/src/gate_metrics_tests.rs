use super::*;

/// Helper: reset ONLY local test counters. Does NOT reset the global static
/// (which is shared with other tests). Tests use fresh instance.
fn fresh_metrics() -> GateMetrics {
    GateMetrics::default()
}

#[test]
fn test_default_counters_are_zero() {
    let m = fresh_metrics();
    assert_eq!(m.pre_edit_fast_path_count.load(Ordering::Relaxed), 0);
    assert_eq!(m.pre_edit_full_enrichment_count.load(Ordering::Relaxed), 0);
    assert_eq!(m.pre_write_fast_path_count.load(Ordering::Relaxed), 0);
    assert_eq!(m.pre_write_full_enrichment_count.load(Ordering::Relaxed), 0);
    assert_eq!(m.post_tool_l4_mandatory_count.load(Ordering::Relaxed), 0);
}

#[test]
fn test_record_functions_are_additive() {
    let baseline_fast = global().pre_edit_fast_path_count.load(Ordering::Relaxed);
    record_pre_edit_fast_path();
    record_pre_edit_fast_path();
    let after = global().pre_edit_fast_path_count.load(Ordering::Relaxed);
    assert_eq!(after - baseline_fast, 2);
}

#[test]
fn test_snapshot_zero_ratio_when_no_calls() {
    let snap = GateMetricsSnapshot {
        pre_edit_fast_path: 0,
        pre_edit_full: 0,
        pre_edit_fast_ratio: 0.0,
        pre_write_fast_path: 0,
        pre_write_full: 0,
        pre_write_fast_ratio: 0.0,
        post_tool_l4_mandatory: 0,
        metadata_cache_hit: 0,
        metadata_backpressure_dropped: 0,
        pre_grep_enrichment_count: 0,
        pre_grep_zero_results_count: 0,
        total_invocations: 0,
        tantivy_upsert_count: 0,
        tantivy_query_latency_us: 0,
        tantivy_commit_count: 0,
        reindex_failure_count: 0,
        rkyv_dispatch_count: 0,
        rkyv_dispatch_bytes: 0,
        rkyv_parse_error_count: 0,
        rkyv_response_count: 0,
        rkyv_mean_bytes: 0.0,
        rkyv_dispatch_latency: LatencySnapshot::default(),
        hook_dispatch_latency: LatencySnapshot::default(),
        tantivy_query_latency: LatencySnapshot::default(),
        actor_queue_wait: LatencySnapshot::default(),
        actor_budget_timeout_count: 0,
        actor_send_timeout_count: 0,
        health_delta_record_count: 0,
        health_delta_compute_count: 0,
        health_delta_regression_count: 0,
        health_delta_improvement_count: 0,
        health_delta_outstanding: 0,
        health_delta_streak_alert_count: 0,
        health_delta_recovery_count: 0,
        query_cache_hit_count: 0,
        query_cache_miss_count: 0,
        query_cache_hit_ratio: 0.0,
        query_cache_invalidate_count: 0,
        blast_inject_count: 0,
        blast_timeout_count: 0,
        blast_mutation_count: 0,
        linucb_route_manual_count: 0,
        linucb_route_generator_count: 0,
        linucb_route_hint_count: 0,
        mcts_shadow_run_count: 0,
        mcts_shadow_timeout_count: 0,
        mcts_shadow_deadlock_detected_count: 0,
        ann_search_latency: LatencySnapshot::default(),
        memory_rss_mb: 0.0,
        memory_virt_mb: 0.0,
        tantivy_stream_enqueued_count: 0,
        tantivy_stream_backpressure_drop_count: 0,
        tantivy_stream_flush_count: 0,
        tantivy_stream_flush_docs_count: 0,
        tantivy_stream_index_unavailable_drop_count: 0,
        prefetch_enqueued_count: 0,
        prefetch_warmed_count: 0,
        jdm_class_a_count: 0,
        jdm_class_b_count: 0,
        jdm_class_c_count: 0,
        jdm_class_d_count: 0,
        tokio_num_workers: 0,
        tokio_num_idle_threads: 0,
        tokio_num_blocking_threads: 0,
        tokio_injection_queue_depth: 0,
        diagnostic_wiring_finding_emitted_count: 0,
        diagnostic_tdg_emitted_count: 0,
        diagnostic_b302_emitted_count: 0,
        task_sync_create_count: 0,
        task_sync_deduped_count: 0,
        task_sync_update_propagated_count: 0,
        diagnostic_q220_nonidempotent_emitted_count: 0,
        diagnostic_w115_skipped_region_written_count: 0,
        activity_pre_edit_count: 0,
        activity_post_edit_count: 0,
        activity_post_write_count: 0,
        activity_instructions_loaded_count: 0,
        activity_pre_compact_count: 0,
        memory_pressure_green_count: 0,
        memory_pressure_yellow_count: 0,
        memory_pressure_red_count: 0,
        memory_pressure_total_tick_count: 0,
        swap_thrashing_detected_count: 0,
        cargo_test_paused_count: 0,
        core_pinning_p_count: 0,
        core_pinning_e_count: 0,
        tool_output_routed_count: 0,
        sandbox_timeout_fallback_count: 0,
        phrase_query_match_count: 0,
        tantivy_trigram_query_count: 0,
        tool_outputs_ttl_skip_count: 0,
        tool_outputs_cleanup_deleted_count: 0,
        sandbox_tee_persisted_count: 0,
        compression_profile_applied_count: 0,
        ctx_replay_count: 0,
        ctx_purge_count: 0,
        ctx_doctor_call_count: 0,
        gate_metrics_daily_flush_count: 0,
        ctx_gain_graph_count: 0,
        ctx_session_adoption_query_count: 0,
        touring_init_invocation_count: 0,
        ctx_smart_count: 0,
        read_aggressive_chunked_count: 0,
        read_aggressive_passthrough_count: 0,
        ctx_explain_count: 0,
        ctx_budget_warning_emitted_count: 0,
        ctx_budget_alert_emitted_count: 0,
        ctx_batch_execute_count: 0,
        ctx_batch_item_count: 0,
        ctx_execute_file_count: 0,
        ctx_upgrade_count: 0,
        ctx_discover_session_count: 0,
        ctx_discover_opportunities_found: 0,
        wave3_t201_count: 0,
        wave3_t202_count: 0,
        wave3_t203_count: 0,
        wave3_t204_count: 0,
        wave3_t205_count: 0,
        wave3_t206_count: 0,
        wave3_t207_count: 0,
        wave3_t208_count: 0,
        wave3_t209_count: 0,
        wave3_t210_count: 0,
        wave3_t211_count: 0,
        wave3_t212_count: 0,
        wave3_t213_count: 0,
        wave3_t214_count: 0,
        wave3_t215_count: 0,
        wave3_t301_count: 0,
        wave3_t302_count: 0,
        wave3_t303_count: 0,
        wave3_t304_count: 0,
        wave3_t305_count: 0,
        wave3_t306_count: 0,
        wave3_t307_count: 0,
        wave3_t308_count: 0,
        wave3_t309_count: 0,
        wave3_t310_count: 0,
        ceg_captured_count: 0,
        ceg_blocked_count: 0,
        ceg_sandboxed_count: 0,
        ceg_fast_path_count: 0,
        workflow_antipattern_detected_count: 0,
        workflow_advice_emitted_count: 0,
        antipattern_converted_count: 0,
        enrichment_context_bytes_total: 0,
        enrichment_emit_count: 0,
        enrichment_mean_bytes_per_emit: 0.0,
        suggestion_uptake_emitted_count: 0,
        suggestion_uptake_followed_count: 0,
        adoption_touring_count: 0,
        adoption_antipattern_count: 0,
        pillar_induction_emitted_count: 0,
        pillar_induction_followed_count: 0,
    };
    assert_eq!(snap.pre_edit_fast_ratio, 0.0);
    assert_eq!(snap.total_invocations, 0);
    assert_eq!(snap.tantivy_upsert_count, 0);
    assert_eq!(snap.tantivy_commit_count, 0);
    assert_eq!(snap.rkyv_dispatch_latency.count, 0);
}

#[test]
fn test_snapshot_ratio_50_percent() {
    // Manually construct with known values (does not use global state).
    let snap = GateMetricsSnapshot {
        pre_edit_fast_path: 10,
        pre_edit_full: 10,
        pre_edit_fast_ratio: 0.5,
        pre_write_fast_path: 7,
        pre_write_full: 3,
        pre_write_fast_ratio: 0.7,
        post_tool_l4_mandatory: 2,
        metadata_cache_hit: 5,
        metadata_backpressure_dropped: 1,
        pre_grep_enrichment_count: 0,
        pre_grep_zero_results_count: 0,
        total_invocations: 30,
        tantivy_upsert_count: 100,
        tantivy_query_latency_us: 5000,
        tantivy_commit_count: 2,
        reindex_failure_count: 0,
        rkyv_dispatch_count: 0,
        rkyv_dispatch_bytes: 0,
        rkyv_parse_error_count: 0,
        rkyv_response_count: 0,
        rkyv_mean_bytes: 0.0,
        rkyv_dispatch_latency: LatencySnapshot::default(),
        hook_dispatch_latency: LatencySnapshot::default(),
        tantivy_query_latency: LatencySnapshot::default(),
        actor_queue_wait: LatencySnapshot::default(),
        actor_budget_timeout_count: 0,
        actor_send_timeout_count: 0,
        health_delta_record_count: 0,
        health_delta_compute_count: 0,
        health_delta_regression_count: 0,
        health_delta_improvement_count: 0,
        health_delta_outstanding: 0,
        health_delta_streak_alert_count: 0,
        health_delta_recovery_count: 0,
        query_cache_hit_count: 0,
        query_cache_miss_count: 0,
        query_cache_hit_ratio: 0.0,
        query_cache_invalidate_count: 0,
        blast_inject_count: 0,
        blast_timeout_count: 0,
        blast_mutation_count: 0,
        linucb_route_manual_count: 0,
        linucb_route_generator_count: 0,
        linucb_route_hint_count: 0,
        mcts_shadow_run_count: 0,
        mcts_shadow_timeout_count: 0,
        mcts_shadow_deadlock_detected_count: 0,
        ann_search_latency: LatencySnapshot::default(),
        memory_rss_mb: 0.0,
        memory_virt_mb: 0.0,
        tantivy_stream_enqueued_count: 0,
        tantivy_stream_backpressure_drop_count: 0,
        tantivy_stream_flush_count: 0,
        tantivy_stream_flush_docs_count: 0,
        tantivy_stream_index_unavailable_drop_count: 0,
        prefetch_enqueued_count: 0,
        prefetch_warmed_count: 0,
        jdm_class_a_count: 0,
        jdm_class_b_count: 0,
        jdm_class_c_count: 0,
        jdm_class_d_count: 0,
        tokio_num_workers: 0,
        tokio_num_idle_threads: 0,
        tokio_num_blocking_threads: 0,
        tokio_injection_queue_depth: 0,
        diagnostic_wiring_finding_emitted_count: 0,
        diagnostic_tdg_emitted_count: 0,
        diagnostic_b302_emitted_count: 0,
        task_sync_create_count: 0,
        task_sync_deduped_count: 0,
        task_sync_update_propagated_count: 0,
        diagnostic_q220_nonidempotent_emitted_count: 0,
        diagnostic_w115_skipped_region_written_count: 0,
        activity_pre_edit_count: 0,
        activity_post_edit_count: 0,
        activity_post_write_count: 0,
        activity_instructions_loaded_count: 0,
        activity_pre_compact_count: 0,
        memory_pressure_green_count: 0,
        memory_pressure_yellow_count: 0,
        memory_pressure_red_count: 0,
        memory_pressure_total_tick_count: 0,
        swap_thrashing_detected_count: 0,
        cargo_test_paused_count: 0,
        core_pinning_p_count: 0,
        core_pinning_e_count: 0,
        tool_output_routed_count: 0,
        sandbox_timeout_fallback_count: 0,
        phrase_query_match_count: 0,
        tantivy_trigram_query_count: 0,
        tool_outputs_ttl_skip_count: 0,
        tool_outputs_cleanup_deleted_count: 0,
        sandbox_tee_persisted_count: 0,
        compression_profile_applied_count: 0,
        ctx_replay_count: 0,
        ctx_purge_count: 0,
        ctx_doctor_call_count: 0,
        gate_metrics_daily_flush_count: 0,
        ctx_gain_graph_count: 0,
        ctx_session_adoption_query_count: 0,
        touring_init_invocation_count: 0,
        ctx_smart_count: 0,
        read_aggressive_chunked_count: 0,
        read_aggressive_passthrough_count: 0,
        ctx_explain_count: 0,
        ctx_budget_warning_emitted_count: 0,
        ctx_budget_alert_emitted_count: 0,
        ctx_batch_execute_count: 0,
        ctx_batch_item_count: 0,
        ctx_execute_file_count: 0,
        ctx_upgrade_count: 0,
        ctx_discover_session_count: 0,
        ctx_discover_opportunities_found: 0,
        wave3_t201_count: 0,
        wave3_t202_count: 0,
        wave3_t203_count: 0,
        wave3_t204_count: 0,
        wave3_t205_count: 0,
        wave3_t206_count: 0,
        wave3_t207_count: 0,
        wave3_t208_count: 0,
        wave3_t209_count: 0,
        wave3_t210_count: 0,
        wave3_t211_count: 0,
        wave3_t212_count: 0,
        wave3_t213_count: 0,
        wave3_t214_count: 0,
        wave3_t215_count: 0,
        wave3_t301_count: 0,
        wave3_t302_count: 0,
        wave3_t303_count: 0,
        wave3_t304_count: 0,
        wave3_t305_count: 0,
        wave3_t306_count: 0,
        wave3_t307_count: 0,
        wave3_t308_count: 0,
        wave3_t309_count: 0,
        wave3_t310_count: 0,
        ceg_captured_count: 0,
        ceg_blocked_count: 0,
        ceg_sandboxed_count: 0,
        ceg_fast_path_count: 0,
        workflow_antipattern_detected_count: 0,
        workflow_advice_emitted_count: 0,
        antipattern_converted_count: 0,
        enrichment_context_bytes_total: 0,
        enrichment_emit_count: 0,
        enrichment_mean_bytes_per_emit: 0.0,
        suggestion_uptake_emitted_count: 0,
        suggestion_uptake_followed_count: 0,
        adoption_touring_count: 0,
        adoption_antipattern_count: 0,
        pillar_induction_emitted_count: 0,
        pillar_induction_followed_count: 0,
    };
    assert!((snap.pre_edit_fast_ratio - 0.5).abs() < 1e-9);
    assert!((snap.pre_write_fast_ratio - 0.7).abs() < 1e-9);
    assert_eq!(snap.tantivy_upsert_count, 100);
    assert_eq!(snap.tantivy_query_latency_us, 5000);
    assert_eq!(snap.tantivy_commit_count, 2);
}

// ── LatencyHistogram tests (2026-04-17) ─────────────────────────

#[test]
fn latency_histogram_empty_snapshot_is_zero() {
    // INVARIANT: a fresh histogram returns all-zero snapshot, matching
    // the `AtomicU64` convention so JSON consumers can branch on count.
    let h = LatencyHistogram::new();
    let s = h.snapshot();
    assert_eq!(s.count, 0);
    assert_eq!(s.p50_us, 0);
    assert_eq!(s.p99_us, 0);
    assert_eq!(s.max_us, 0);
}

#[test]
fn latency_histogram_records_and_reports_percentiles() {
    // HAPPY PATH: record 1000 values from 100 to 100_000 μs in 100μs
    // steps. P50 should sit near the middle, P99 near the top.
    let h = LatencyHistogram::new();
    for i in 0..1000 {
        h.record_us(100 + i * 100);
    }
    let s = h.snapshot();
    assert_eq!(s.count, 1000);
    // P50 near middle of the distribution
    assert!(s.p50_us >= 40_000 && s.p50_us <= 60_000, "p50={}", s.p50_us);
    // P99 approaches the top
    assert!(s.p99_us >= 95_000, "p99={}", s.p99_us);
    // max equals the largest value recorded (100_099 rounded within precision)
    assert!(s.max_us >= 99_000);
}

#[test]
fn latency_histogram_clamps_zero_to_one() {
    // BOUNDARY: 0μs violates the histogram lower bound of 1. Must clamp
    // (not panic), preserving `count` monotonicity.
    let h = LatencyHistogram::new();
    h.record_us(0);
    let s = h.snapshot();
    assert_eq!(s.count, 1);
    assert_eq!(s.p50_us, 1);
}

#[test]
fn latency_histogram_clamps_above_upper_bound() {
    // BOUNDARY: values > 60s clamp to 60_000_000 — keeps the counter
    // advancing for misbehaving callers without poisoning the range.
    // Note: hdrhistogram rounds recorded values up to the highest
    // value in the bucket at 3 sigfigs, so the reported `max` can
    // exceed the clamp by up to 1 bucket width (~0.1% at 60M).
    let h = LatencyHistogram::new();
    h.record_us(u64::MAX);
    let s = h.snapshot();
    assert_eq!(s.count, 1);
    // Tolerate up to 1% bucket rounding above the nominal clamp.
    assert!(
        s.max_us <= 60_600_000,
        "max_us={} exceeded clamp + 1% bucket tolerance",
        s.max_us
    );
}

#[test]
fn latency_snapshot_serializes_to_json() {
    // INVARIANT: LatencySnapshot serde-compat required for
    // `touring gate-metrics -j` CLI output consumers.
    let s = LatencySnapshot {
        count: 42,
        p50_us: 10,
        p90_us: 50,
        p99_us: 120,
        p999_us: 500,
        max_us: 700,
    };
    let json = serde_json::to_string(&s).expect("serialize");
    assert!(json.contains("\"count\":42"));
    assert!(json.contains("\"p99_us\":120"));
}

// ── RFC-100 Diagnostic Counter tests (2026-04-25) ─────────────────────────

#[test]
fn test_diagnostic_tdg_emitted_counter_increments() {
    // PROPERTY: record_diagnostic_tdg_emitted() must be additive against
    // the global singleton — uses delta to tolerate ambient state from
    // other tests (same process, shared static GATE_METRICS).
    let baseline = global()
        .diagnostic_tdg_emitted_count
        .load(Ordering::Relaxed);
    record_diagnostic_tdg_emitted();
    record_diagnostic_tdg_emitted();
    record_diagnostic_tdg_emitted();
    let after = global()
        .diagnostic_tdg_emitted_count
        .load(Ordering::Relaxed);
    assert_eq!(
        after - baseline,
        3,
        "record_diagnostic_tdg_emitted() must increment by exactly 1 per call"
    );
}

#[test]
fn test_diagnostic_wiring_finding_emitted_counter_increments() {
    // PROPERTY: record_diagnostic_wiring_finding_emitted() must be additive.
    let baseline = global()
        .diagnostic_wiring_finding_emitted_count
        .load(Ordering::Relaxed);
    record_diagnostic_wiring_finding_emitted();
    record_diagnostic_wiring_finding_emitted();
    let after = global()
        .diagnostic_wiring_finding_emitted_count
        .load(Ordering::Relaxed);
    assert_eq!(
        after - baseline,
        2,
        "record_diagnostic_wiring_finding_emitted() must increment by exactly 1 per call"
    );
}

#[test]
fn test_diagnostic_counters_appear_in_snapshot_json() {
    // WIRE-COMPAT: consumers of `touring gate-metrics -j` rely on the
    // key names being stable — verify both new fields serialize correctly.
    let snap = GateMetricsSnapshot::capture();
    let json = serde_json::to_string(&snap).expect("snapshot must serialize");
    assert!(
        json.contains("diagnostic_wiring_finding_emitted_count"),
        "snapshot JSON must contain diagnostic_wiring_finding_emitted_count"
    );
    assert!(
        json.contains("diagnostic_tdg_emitted_count"),
        "snapshot JSON must contain diagnostic_tdg_emitted_count"
    );
}

#[test]
fn test_diagnostic_counters_deserialize_from_legacy_json_with_default() {
    // WIRE-COMPAT: older `touring gate-metrics -j` producers will not emit
    // the new fields. `#[serde(default)]` must keep deserialization intact.
    let legacy_json = r#"{
            "pre_edit_fast_path":0,"pre_edit_full":0,"pre_edit_fast_ratio":0.0,
            "pre_write_fast_path":0,"pre_write_full":0,"pre_write_fast_ratio":0.0,
            "post_tool_l4_mandatory":0,"metadata_cache_hit":0,
            "metadata_backpressure_dropped":0,"total_invocations":0,
            "tantivy_upsert_count":0,"tantivy_query_latency_us":0,
            "tantivy_commit_count":0,"rkyv_dispatch_count":0,
            "rkyv_dispatch_bytes":0,"rkyv_parse_error_count":0,
            "rkyv_response_count":0,"rkyv_mean_bytes":0.0,
            "rkyv_dispatch_latency":{"count":0,"p50_us":0,"p90_us":0,"p99_us":0,"p999_us":0,"max_us":0},
            "hook_dispatch_latency":{"count":0,"p50_us":0,"p90_us":0,"p99_us":0,"p999_us":0,"max_us":0},
            "tantivy_query_latency":{"count":0,"p50_us":0,"p90_us":0,"p99_us":0,"p999_us":0,"max_us":0},
            "health_delta_record_count":0,"health_delta_compute_count":0,
            "health_delta_regression_count":0,"health_delta_improvement_count":0,
            "health_delta_outstanding":0,"health_delta_streak_alert_count":0,
            "health_delta_recovery_count":0,"query_cache_hit_count":0,
            "query_cache_miss_count":0,"query_cache_hit_ratio":0.0,
            "query_cache_invalidate_count":0,
            "ann_search_latency":{"count":0,"p50_us":0,"p90_us":0,"p99_us":0,"p999_us":0,"max_us":0}
        }"#;
    let snap: GateMetricsSnapshot =
        serde_json::from_str(legacy_json).expect("legacy JSON must deserialize");
    assert_eq!(
        snap.diagnostic_wiring_finding_emitted_count, 0,
        "missing field must default to 0"
    );
    assert_eq!(
        snap.diagnostic_tdg_emitted_count, 0,
        "missing field must default to 0"
    );
}

#[test]
fn test_snapshot_serializes_to_json() {
    let snap = GateMetricsSnapshot::capture();
    let json = serde_json::to_string(&snap).expect("serialize must succeed");
    assert!(json.contains("pre_edit_fast_path"));
    assert!(json.contains("pre_edit_fast_ratio"));
    assert!(json.contains("total_invocations"));
    // U21: tantivy fields present in serialized output
    assert!(json.contains("tantivy_upsert_count"));
    assert!(json.contains("tantivy_query_latency_us"));
    assert!(json.contains("tantivy_commit_count"));
    // 2026-04-20: memory probe fields present in serialized output.
    // Consumers of `touring gate-metrics -j` rely on the JSON key
    // names — a rename would be a breaking change to the CLI contract.
    assert!(json.contains("memory_rss_mb"));
    assert!(json.contains("memory_virt_mb"));
}

#[test]
fn capture_populates_memory_fields_in_test_process() {
    // PROPERTY: cargo-test runs as a real process, so the probe should report
    // positive RSS. The `memory-stats` probe can transiently return 0 under heavy
    // parallel load (`cargo test --workspace` spawns many processes contending on
    // /proc) — an environment hiccup, not a probe regression. Retry a few times
    // before failing loudly: a PERSISTENT 0 still fails (a real regression).
    let mut snap = GateMetricsSnapshot::capture();
    for _ in 0..5 {
        if snap.memory_rss_mb > 0.0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        snap = GateMetricsSnapshot::capture();
    }
    assert!(
        snap.memory_rss_mb > 0.0,
        "memory_rss_mb must be > 0 in test process (after retries), got {}",
        snap.memory_rss_mb
    );
    // Virtual is always >= physical on a healthy probe.
    assert!(
        snap.memory_virt_mb >= snap.memory_rss_mb,
        "virt={} should be >= rss={}",
        snap.memory_virt_mb,
        snap.memory_rss_mb
    );
}

#[test]
fn snapshot_missing_memory_fields_deserializes_with_defaults() {
    // WIRE-COMPAT: older `touring gate-metrics -j` producers emitted
    // JSON without `memory_rss_mb` / `memory_virt_mb`. The `#[serde(default)]`
    // attributes on those fields must preserve round-trip-from-old-JSON
    // so mixed-version clients do not fail parsing after an upgrade.
    let legacy_json = r#"{
            "pre_edit_fast_path":0,"pre_edit_full":0,"pre_edit_fast_ratio":0.0,
            "pre_write_fast_path":0,"pre_write_full":0,"pre_write_fast_ratio":0.0,
            "post_tool_l4_mandatory":0,"metadata_cache_hit":0,
            "metadata_backpressure_dropped":0,"total_invocations":0,
            "tantivy_upsert_count":0,"tantivy_query_latency_us":0,
            "tantivy_commit_count":0,"rkyv_dispatch_count":0,
            "rkyv_dispatch_bytes":0,"rkyv_parse_error_count":0,
            "rkyv_response_count":0,"rkyv_mean_bytes":0.0,
            "rkyv_dispatch_latency":{"count":0,"p50_us":0,"p90_us":0,"p99_us":0,"p999_us":0,"max_us":0},
            "hook_dispatch_latency":{"count":0,"p50_us":0,"p90_us":0,"p99_us":0,"p999_us":0,"max_us":0},
            "tantivy_query_latency":{"count":0,"p50_us":0,"p90_us":0,"p99_us":0,"p999_us":0,"max_us":0},
            "health_delta_record_count":0,"health_delta_compute_count":0,
            "health_delta_regression_count":0,"health_delta_improvement_count":0,
            "health_delta_outstanding":0,"health_delta_streak_alert_count":0,
            "health_delta_recovery_count":0,"query_cache_hit_count":0,
            "query_cache_miss_count":0,"query_cache_hit_ratio":0.0,
            "query_cache_invalidate_count":0,
            "ann_search_latency":{"count":0,"p50_us":0,"p90_us":0,"p99_us":0,"p999_us":0,"max_us":0}
        }"#;
    let snap: GateMetricsSnapshot =
        serde_json::from_str(legacy_json).expect("legacy JSON must deserialize");
    assert_eq!(snap.memory_rss_mb, 0.0);
    assert_eq!(snap.memory_virt_mb, 0.0);
}

#[test]
fn test_tantivy_record_functions_are_additive() {
    let baseline_upserts = global().tantivy_upsert_count.load(Ordering::Relaxed);
    let baseline_latency = global().tantivy_query_latency_us.load(Ordering::Relaxed);
    let baseline_commits = global().tantivy_commit_count.load(Ordering::Relaxed);

    record_tantivy_upsert();
    record_tantivy_upsert();
    record_tantivy_query_latency(250);
    record_tantivy_query_latency(750);
    record_tantivy_commit();

    assert_eq!(
        global().tantivy_upsert_count.load(Ordering::Relaxed) - baseline_upserts,
        2
    );
    assert_eq!(
        global().tantivy_query_latency_us.load(Ordering::Relaxed) - baseline_latency,
        1000
    );
    assert_eq!(
        global().tantivy_commit_count.load(Ordering::Relaxed) - baseline_commits,
        1
    );
}

/// Predictive Wave D2/D3/D4 — verify each record_* helper increments
/// exactly its own counter and is composable across the 9-counter surface.
///
/// Uses deltas against `baseline_*` loads so the test is robust to any
/// ambient global-state contamination from earlier tests (the static
/// `GATE_METRICS` singleton is shared process-wide).
#[test]
fn test_predictive_wave_record_functions_are_additive() {
    let baseline_blast_inject = global().blast_inject_count.load(Ordering::Relaxed);
    let baseline_blast_timeout = global().blast_timeout_count.load(Ordering::Relaxed);
    let baseline_blast_mutation = global().blast_mutation_count.load(Ordering::Relaxed);
    let baseline_linucb_manual = global().linucb_route_manual_count.load(Ordering::Relaxed);
    let baseline_linucb_generator = global()
        .linucb_route_generator_count
        .load(Ordering::Relaxed);
    let baseline_linucb_hint = global().linucb_route_hint_count.load(Ordering::Relaxed);
    let baseline_mcts_run = global().mcts_shadow_run_count.load(Ordering::Relaxed);
    let baseline_mcts_timeout = global().mcts_shadow_timeout_count.load(Ordering::Relaxed);
    let baseline_mcts_deadlock = global()
        .mcts_shadow_deadlock_detected_count
        .load(Ordering::Relaxed);

    // D2 — blast family
    record_blast_inject();
    record_blast_inject();
    record_blast_timeout();
    record_blast_mutation();

    // D3 — linucb family
    record_linucb_route_manual();
    record_linucb_route_generator();
    record_linucb_route_generator();
    record_linucb_route_hint();

    // D4 — mcts family
    record_mcts_shadow_run();
    record_mcts_shadow_run();
    record_mcts_shadow_run();
    record_mcts_shadow_timeout();
    record_mcts_shadow_deadlock_detected();

    assert_eq!(
        global().blast_inject_count.load(Ordering::Relaxed) - baseline_blast_inject,
        2,
        "blast_inject should increment by 2"
    );
    assert_eq!(
        global().blast_timeout_count.load(Ordering::Relaxed) - baseline_blast_timeout,
        1
    );
    assert_eq!(
        global().blast_mutation_count.load(Ordering::Relaxed) - baseline_blast_mutation,
        1
    );
    assert_eq!(
        global().linucb_route_manual_count.load(Ordering::Relaxed) - baseline_linucb_manual,
        1
    );
    assert_eq!(
        global()
            .linucb_route_generator_count
            .load(Ordering::Relaxed)
            - baseline_linucb_generator,
        2
    );
    assert_eq!(
        global().linucb_route_hint_count.load(Ordering::Relaxed) - baseline_linucb_hint,
        1
    );
    assert_eq!(
        global().mcts_shadow_run_count.load(Ordering::Relaxed) - baseline_mcts_run,
        3
    );
    assert_eq!(
        global().mcts_shadow_timeout_count.load(Ordering::Relaxed) - baseline_mcts_timeout,
        1
    );
    assert_eq!(
        global()
            .mcts_shadow_deadlock_detected_count
            .load(Ordering::Relaxed)
            - baseline_mcts_deadlock,
        1
    );

    // Snapshot round-trip — ensure serde_json can serialize the expanded struct.
    let snap = GateMetricsSnapshot::capture();
    let json = serde_json::to_string(&snap).expect("snapshot serializes");
    assert!(json.contains("blast_inject_count"));
    assert!(json.contains("linucb_route_hint_count"));
    assert!(json.contains("mcts_shadow_deadlock_detected_count"));
}

#[test]
fn actor_queue_observability_records_and_snapshots() {
    // Actor queue observability (2026-07-01): queue-wait histogram + the two
    // timeout counters must be recordable and must surface in the snapshot
    // JSON — this is the signal that separates serialized-actor backpressure
    // from handler execution time.
    let baseline_budget = global().actor_budget_timeout_count.load(Ordering::Relaxed);
    let baseline_send = global().actor_send_timeout_count.load(Ordering::Relaxed);
    let baseline_wait_count = global().actor_queue_wait.snapshot().count;

    record_actor_queue_wait_us(1_500);
    record_actor_queue_wait_us(250_000);
    record_actor_budget_timeout();
    record_actor_send_timeout();

    assert_eq!(
        global().actor_budget_timeout_count.load(Ordering::Relaxed) - baseline_budget,
        1
    );
    assert_eq!(
        global().actor_send_timeout_count.load(Ordering::Relaxed) - baseline_send,
        1
    );
    let wait = global().actor_queue_wait.snapshot();
    assert_eq!(wait.count - baseline_wait_count, 2);
    assert!(wait.max_us >= 250_000, "250ms sample must register");

    // Snapshot exposes the new fields with the expected JSON keys.
    let snap = GateMetricsSnapshot::capture();
    let json = serde_json::to_string(&snap).expect("snapshot serializes");
    assert!(json.contains("actor_queue_wait"));
    assert!(json.contains("actor_budget_timeout_count"));
    assert!(json.contains("actor_send_timeout_count"));
}
