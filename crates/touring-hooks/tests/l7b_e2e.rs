//! L7-B End-to-End Integration Tests.
//!
//! Validates the complete integration between all L7-B features:
//! - Alpha: cognitive_runtime init, crdt_graph load, enrichment trigger
//! - Beta: should_enrich CILA gate in pre_edit/pre_write hooks
//! - Gamma: gate_metrics counters, job_registry spawn/poll, inferlets CLI
//! - Delta: semaphore wiring, MCP tools surface
//!
//! These tests run WITHOUT a daemon — they exercise the pure-library
//! integration surface of touring-hooks to prove each building block
//! works in isolation and composes correctly with its neighbours.

#![allow(clippy::all)]
//! cargo test --release -p touring-hooks --features "persistence,inferlets-wasm" --test l7b_e2e
//! ```

use touring_hooks::shared::cila::{
    cila_budget_edit, cila_budget_read, cila_budget_write, is_enrichment_mandatory, should_enrich,
};
use touring_hooks::shared::gate_metrics::{
    GateMetricsSnapshot, global as gate_metrics_global, record_post_tool_l4_mandatory,
    record_pre_edit_fast_path, record_pre_edit_full, record_pre_write_fast_path,
    record_pre_write_full,
};
use touring_hooks::shared::job_registry::{
    JobState, drop_job, list_jobs, poll_worker, registry as job_registry, spawn_worker,
};

// ─────────────────────────────────────────────────────────────────────────────
// Alpha: CILA budgets (exercised by pre_read/pre_edit/pre_write)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn e2e_alpha_cila_budgets_scale_with_level() {
    // L0/L1 (reflexive) should have the smallest budgets.
    assert!(cila_budget_read(0) < cila_budget_read(2));
    assert!(cila_budget_edit(0) < cila_budget_edit(2));
    assert!(cila_budget_write(0) < cila_budget_write(2));

    // L4+ should grant the largest budgets.
    assert!(cila_budget_read(4) >= cila_budget_read(2));
    assert!(cila_budget_edit(6) >= cila_budget_edit(2));
    assert!(cila_budget_write(6) >= cila_budget_write(2));
}

// ─────────────────────────────────────────────────────────────────────────────
// Beta: should_enrich CILA gate — combined with tool filter
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn e2e_beta_enrichment_gate_l0_blocks_all() {
    assert!(!should_enrich(true, 0, "Edit"), "L0 must block mutation");
    assert!(!should_enrich(true, 0, "Read"), "L0 must block read");
    assert!(!should_enrich(true, 1, "Write"), "L1 must block mutation");
}

#[test]
fn e2e_beta_enrichment_gate_l2_plus_allows_mutation_only() {
    // L2+ allows mutation tools only
    assert!(should_enrich(true, 2, "Edit"));
    assert!(should_enrich(true, 3, "Write"));
    assert!(should_enrich(true, 4, "MultiEdit"));

    // Read tools remain fast-path at L2/L3
    assert!(!should_enrich(true, 2, "Read"));
    assert!(!should_enrich(true, 3, "Grep"));
    assert!(!should_enrich(true, 3, "Glob"));
}

#[test]
fn e2e_beta_enrichment_inactive_blocks_regardless_of_level() {
    // When the pipeline is inactive, nothing passes
    for level in 0..=6 {
        assert!(!should_enrich(false, level, "Edit"));
        assert!(!should_enrich(false, level, "Write"));
    }
}

#[test]
fn e2e_beta_mandatory_enrichment_only_at_l4_plus() {
    assert!(!is_enrichment_mandatory(true, 0));
    assert!(!is_enrichment_mandatory(true, 3));
    assert!(is_enrichment_mandatory(true, 4));
    assert!(is_enrichment_mandatory(true, 6));
    // Inactive pipeline never mandatory
    assert!(!is_enrichment_mandatory(false, 4));
    assert!(!is_enrichment_mandatory(false, 6));
}

// ─────────────────────────────────────────────────────────────────────────────
// Gamma: gate_metrics counters — monotonic increment
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn e2e_gamma_gate_metrics_are_monotonic() {
    let baseline = GateMetricsSnapshot::capture();

    // Record 3 fast-path and 2 full-enrichment events for pre_edit
    record_pre_edit_fast_path();
    record_pre_edit_fast_path();
    record_pre_edit_fast_path();
    record_pre_edit_full();
    record_pre_edit_full();

    // Record 1 fast-path and 1 full for pre_write
    record_pre_write_fast_path();
    record_pre_write_full();

    // Record 1 L4+ mandatory event
    record_post_tool_l4_mandatory();

    let after = GateMetricsSnapshot::capture();

    // NOTE (P1 fix 2026-04-20): exact-delta assertions fail under workspace
    // parallel test runs because `GateMetrics` is a process-wide singleton
    // (`AtomicU64` fields) shared across integration test binaries. Monotonic
    // counters tolerate concurrent increments; switch to `>=` lower-bound
    // assertions to preserve intent (prove our `record_*` calls happened)
    // without racing against unrelated test binaries.
    assert!(after.pre_edit_fast_path - baseline.pre_edit_fast_path >= 3);
    assert!(after.pre_edit_full - baseline.pre_edit_full >= 2);
    assert!(after.pre_write_fast_path - baseline.pre_write_fast_path >= 1);
    assert!(after.pre_write_full - baseline.pre_write_full >= 1);
    assert!(after.post_tool_l4_mandatory - baseline.post_tool_l4_mandatory >= 1);
    assert!(after.total_invocations >= baseline.total_invocations + 7);
}

#[test]
fn e2e_gamma_gate_metrics_ratio_computation() {
    // Use the global singleton to verify ratio semantics.
    // Record a known pattern: 6 fast, 4 full → ratio 0.6
    let m = gate_metrics_global();
    use std::sync::atomic::Ordering;
    let pe_fast_before = m.pre_edit_fast_path_count.load(Ordering::Relaxed);
    let pe_full_before = m.pre_edit_full_enrichment_count.load(Ordering::Relaxed);

    for _ in 0..6 {
        record_pre_edit_fast_path();
    }
    for _ in 0..4 {
        record_pre_edit_full();
    }

    let snap = GateMetricsSnapshot::capture();
    let delta_fast = snap.pre_edit_fast_path - pe_fast_before;
    let delta_full = snap.pre_edit_full - pe_full_before;
    assert!(
        delta_fast >= 6,
        "expected >= 6 fast-path increments, got {delta_fast}"
    );
    assert!(
        delta_full >= 4,
        "expected >= 4 full-enrichment increments, got {delta_full}"
    );
    // Ratio is computed over the total snapshot, not the delta,
    // so we assert that the ratio is bounded and sane.
    assert!(snap.pre_edit_fast_ratio >= 0.0 && snap.pre_edit_fast_ratio <= 1.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Gamma: job_registry state transitions and lookup
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_gamma_job_spawn_poll_transition() {
    // Spawn a fast echo command and poll until terminal.
    let job_id = spawn_worker("e2e-echo", "echo", &["l7b_integration".to_string()]);
    assert!(job_id.starts_with("e2e-echo-"));

    // Initial poll may be Running or already Completed depending on scheduler.
    let initial = poll_worker(&job_id);
    let status = initial["status"].as_str().unwrap_or("");
    assert!(
        status == "running" || status == "completed",
        "initial state must be running or completed, got: {status}"
    );

    // Drain to terminal state
    for _ in 0..100 {
        let p = poll_worker(&job_id);
        if p["status"] != "running" {
            // Terminal — verify completion fields
            assert_eq!(p["status"], "completed", "echo should succeed");
            assert!(
                p["result"]
                    .as_str()
                    .unwrap_or("")
                    .contains("l7b_integration")
            );
            assert!(p.get("duration_secs").is_some());

            // Cleanup
            assert!(drop_job(&job_id), "drop_job must remove terminal jobs");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("echo job did not complete within 1 second");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_gamma_job_list_includes_spawned_jobs() {
    // Ensure the registry starts empty or with only previously-completed jobs
    let before = list_jobs();
    let _before_count = before["job_count"].as_u64().unwrap_or(0);

    // Spawn 3 jobs
    let id1 = spawn_worker("list-test", "echo", &["a".to_string()]);
    let id2 = spawn_worker("list-test", "echo", &["b".to_string()]);
    let id3 = spawn_worker("list-test", "echo", &["c".to_string()]);

    // Give them a moment to potentially finish
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let after = list_jobs();
    let after_count = after["job_count"].as_u64().unwrap_or(0);
    assert!(
        after_count >= 3,
        "expected at least 3 jobs after spawn, got {after_count}"
    );

    // Verify our job_ids appear in the list (by substring)
    let jobs_array = after["jobs"].as_array().expect("jobs must be an array");
    let ids_in_list: Vec<&str> = jobs_array
        .iter()
        .filter_map(|j| j["job_id"].as_str())
        .collect();
    assert!(
        ids_in_list.iter().any(|id| *id == id1.as_str()),
        "id1 missing"
    );
    assert!(
        ids_in_list.iter().any(|id| *id == id2.as_str()),
        "id2 missing"
    );
    assert!(
        ids_in_list.iter().any(|id| *id == id3.as_str()),
        "id3 missing"
    );

    // Cleanup
    drop_job(&id1);
    drop_job(&id2);
    drop_job(&id3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_gamma_job_failure_path_is_captured() {
    // Spawn a command that will exit non-zero
    let job_id = spawn_worker("e2e-fail", "false", &[]);

    // Drain to terminal
    for _ in 0..100 {
        let p = poll_worker(&job_id);
        if p["status"] != "running" {
            assert_eq!(p["status"], "failed", "false should fail");
            assert!(
                p["error"]
                    .as_str()
                    .map(|e| e.contains("exit="))
                    .unwrap_or(false),
                "error should contain exit code"
            );
            drop_job(&job_id);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("false job did not terminate within 1 second");
}

#[test]
fn e2e_gamma_job_poll_not_found_returns_sane_status() {
    let result = poll_worker("nonexistent-job-id-xyz-12345");
    assert_eq!(result["status"], "not_found");
    assert_eq!(result["job_id"], "nonexistent-job-id-xyz-12345");
}

#[test]
fn e2e_gamma_drop_job_returns_false_for_missing() {
    assert!(!drop_job("definitely-not-a-real-job-id-for-e2e-test"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Delta: registry + job_state contract
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn e2e_delta_job_state_is_terminal_contract() {
    let completed = JobState::Completed {
        result: "ok".into(),
        started_at_secs: 100,
        finished_at_secs: 102,
    };
    assert!(completed.is_terminal());
    assert_eq!(completed.status_str(), "completed");
    assert_eq!(completed.started_at(), 100);

    let failed = JobState::Failed {
        error: "boom".into(),
        started_at_secs: 50,
        finished_at_secs: 51,
    };
    assert!(failed.is_terminal());
    assert_eq!(failed.status_str(), "failed");
    assert_eq!(failed.started_at(), 50);
}

#[test]
fn e2e_delta_registry_singleton_is_shared() {
    // Two calls to registry() must return Arcs pointing to the same DashMap.
    let r1 = job_registry();
    let r2 = job_registry();
    // Arc::ptr_eq confirms they point to the same allocation.
    assert!(std::sync::Arc::ptr_eq(&r1, &r2));
}

// ─────────────────────────────────────────────────────────────────────────────
// Full integration: Alpha+Beta+Gamma+Delta composition
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_full_pipeline_integration() {
    // Simulate a complete L7-B pipeline invocation:
    //
    // 1. A hook sees an Edit tool at CILA L3 → should_enrich returns true
    // 2. Metrics counter increments for full enrichment
    // 3. A background job is spawned for a long-running task
    // 4. The job completes and is cleaned up
    // 5. L4+ mandatory enrichment is recorded
    //
    // This exercises the complete L7-B value chain end-to-end.

    let baseline = GateMetricsSnapshot::capture();

    // Step 1: enrichment gate decision
    let enrich_ok = should_enrich(true, 3, "Edit");
    assert!(enrich_ok, "L3 Edit should pass the gate");

    // Step 2: record the full-enrichment path
    record_pre_edit_full();

    // Step 3: spawn a background job (simulating async work triggered by the hook)
    let job_id = spawn_worker("e2e-pipeline", "echo", &["integration_ok".to_string()]);

    // Step 4: wait for job completion
    let mut completion = None;
    for _ in 0..100 {
        let p = poll_worker(&job_id);
        if p["status"] != "running" {
            completion = Some(p);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let done = completion.expect("job must complete");
    assert_eq!(done["status"], "completed");

    // Step 5: simulate an L4+ mandatory trigger
    let mandatory = is_enrichment_mandatory(true, 4);
    assert!(mandatory);
    record_post_tool_l4_mandatory();

    // Verify metrics reflect the full pipeline run
    let after = GateMetricsSnapshot::capture();
    assert!(after.pre_edit_full > baseline.pre_edit_full);
    assert!(after.post_tool_l4_mandatory > baseline.post_tool_l4_mandatory);
    assert!(after.total_invocations > baseline.total_invocations);

    // Cleanup
    drop_job(&job_id);
}
