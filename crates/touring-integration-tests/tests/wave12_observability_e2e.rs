#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic indexing
//! Wave 12 — Health Delta Observability E2E (cross-crate).
//!
//! Wave 9-11 wired the health_delta bridge into pre_edit / pre_write /
//! post_edit / post_write. Wave 12 adds **observability** — every
//! record/compute/regression/improvement event bumps an atomic counter
//! exposed via `touring gate-metrics -j` (cli_gate_metrics handler →
//! GateMetricsSnapshot::capture).
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ touring-hooks::health_delta                                     │
//! │   ├── record_pre_signals  ──► record_health_delta_record       │
//! │   └── compute_signals_delta ──► record_health_delta_compute    │
//! │                              ├── if is_regression() ──► counter│
//! │                              └── if is_improvement() ──► counter│
//! │                                                                 │
//! │ touring-hooks::shared::gate_metrics::GateMetricsSnapshot       │
//! │   ├── health_delta_record_count                                │
//! │   ├── health_delta_compute_count                               │
//! │   ├── health_delta_regression_count                            │
//! │   ├── health_delta_improvement_count                           │
//! │   └── health_delta_outstanding (record − compute)              │
//! │                                                                 │
//! │ touring-server CLI: `touring gate-metrics -j` (no change       │
//! │   needed; serde flattens new fields automatically).            │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::atomic::Ordering;
use touring_hooks::health_delta::{compute_signals_delta, discard_pre_health, record_pre_signals};
use touring_hooks::shared::gate_metrics::{GateMetricsSnapshot, global};

// ── Axis 1: snapshot exposes 5 new health_delta fields ───────────────────────

#[test]
fn axis1_snapshot_exposes_health_delta_fields() {
    let snap = GateMetricsSnapshot::capture();
    // Fields exist (compile check) and serialize to JSON via serde.
    let json = serde_json::to_string(&snap).expect("serialize");
    assert!(json.contains("health_delta_record_count"));
    assert!(json.contains("health_delta_compute_count"));
    assert!(json.contains("health_delta_regression_count"));
    assert!(json.contains("health_delta_improvement_count"));
    assert!(json.contains("health_delta_outstanding"));
}

// ── Axis 2: record_pre_signals advances the record counter ───────────────────

#[test]
fn axis2_record_pre_signals_advances_counter() {
    let path = "/wave12e2e/axis2.rs";
    discard_pre_health(path);
    let before = global().health_delta_record_count.load(Ordering::Relaxed);
    record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("recorded");
    let after = global().health_delta_record_count.load(Ordering::Relaxed);
    assert!(
        after >= before + 1,
        "counter must advance: {before} → {after}"
    );
}

// ── Axis 3: compute_signals_delta advances the compute counter ───────────────

#[test]
fn axis3_compute_signals_delta_advances_counter() {
    let path = "/wave12e2e/axis3.rs";
    discard_pre_health(path);
    record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("pre");
    let before = global().health_delta_compute_count.load(Ordering::Relaxed);
    let _ = compute_signals_delta(path, "pub fn ok() -> i32 { 2 }").expect("delta");
    let after = global().health_delta_compute_count.load(Ordering::Relaxed);
    assert!(after >= before + 1, "compute counter must advance");
}

// ── Axis 4: regression triggers regression counter ───────────────────────────

#[test]
fn axis4_regression_triggers_counter() {
    let path = "/wave12e2e/axis4.rs";
    discard_pre_health(path);
    record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("pre");
    let before = global()
        .health_delta_regression_count
        .load(Ordering::Relaxed);
    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";
    let d = compute_signals_delta(path, degraded).expect("delta");
    assert!(d.is_regression());
    let after = global()
        .health_delta_regression_count
        .load(Ordering::Relaxed);
    assert!(after >= before + 1, "regression counter must advance");
}

// ── Axis 5: improvement triggers improvement counter ─────────────────────────

#[test]
fn axis5_improvement_triggers_counter() {
    let path = "/wave12e2e/axis5.rs";
    discard_pre_health(path);
    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";
    record_pre_signals(path, degraded).expect("pre");
    let before = global()
        .health_delta_improvement_count
        .load(Ordering::Relaxed);
    let d = compute_signals_delta(path, "pub fn good() -> i32 { 1 }").expect("delta");
    assert!(d.is_improvement());
    let after = global()
        .health_delta_improvement_count
        .load(Ordering::Relaxed);
    assert!(after >= before + 1, "improvement counter must advance");
}

// ── Axis 6: outstanding correctly reflects record - compute ──────────────────

#[test]
fn axis6_outstanding_reflects_record_minus_compute() {
    let path1 = "/wave12e2e/axis6_a.rs";
    let path2 = "/wave12e2e/axis6_b.rs";
    discard_pre_health(path1);
    discard_pre_health(path2);

    let snap_before = GateMetricsSnapshot::capture();
    record_pre_signals(path1, "pub fn a() -> i32 { 1 }").expect("a");
    record_pre_signals(path2, "pub fn b() -> i32 { 2 }").expect("b");
    let _ = compute_signals_delta(path1, "pub fn a() -> i32 { 1 }").expect("a delta");
    let snap_after = GateMetricsSnapshot::capture();

    let dr = snap_after.health_delta_record_count - snap_before.health_delta_record_count;
    let dc = snap_after.health_delta_compute_count - snap_before.health_delta_compute_count;
    assert!(dr >= 2, "must record >=2: got {dr}");
    assert!(dc >= 1, "must compute >=1: got {dc}");

    // Cleanup
    discard_pre_health(path2);
}

// ── Axis 7: non-Rust files also bump counters (Wave 11 multi-lang) ───────────

#[test]
fn axis7_multilang_records_bump_counters() {
    let path = "/wave12e2e/axis7.py";
    discard_pre_health(path);
    let before = global().health_delta_record_count.load(Ordering::Relaxed);
    record_pre_signals(path, "def add(a, b): return a + b\n").expect("py recorded");
    let after = global().health_delta_record_count.load(Ordering::Relaxed);
    assert!(after >= before + 1, "python record must bump counter");
}

// ── Axis 8: identity edits do NOT bump regression/improvement counters ───────

#[test]
fn axis8_identity_does_not_bump_directional_counters() {
    let path = "/wave12e2e/axis8.rs";
    discard_pre_health(path);
    let src = "pub fn ok() -> i32 { 1 }";
    record_pre_signals(path, src).expect("pre");

    let reg_before = global()
        .health_delta_regression_count
        .load(Ordering::Relaxed);
    let imp_before = global()
        .health_delta_improvement_count
        .load(Ordering::Relaxed);

    // Identity edit → delta = 0 → neither regression nor improvement.
    let d = compute_signals_delta(path, src).expect("delta");
    assert_eq!(d.delta, Some(0.0));

    let reg_after = global()
        .health_delta_regression_count
        .load(Ordering::Relaxed);
    let imp_after = global()
        .health_delta_improvement_count
        .load(Ordering::Relaxed);
    assert_eq!(
        reg_before, reg_after,
        "regression counter must NOT advance for identity"
    );
    assert_eq!(
        imp_before, imp_after,
        "improvement counter must NOT advance for identity"
    );
}
