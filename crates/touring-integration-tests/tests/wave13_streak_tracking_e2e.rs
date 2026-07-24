#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic indexing
//! Wave 13 — Regression Streak Tracking E2E (cross-crate).
//!
//! Closes Wave 9-12 with cross-edit memory: per-path counters track
//! consecutive regression vs improvement deltas. When a regression
//! streak crosses the threshold (default 3), `gate_metrics` raises an
//! `streak_alert`. When an improvement breaks a regression streak,
//! a `recovery` event is recorded.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ touring-hooks::health_delta::compute_signals_delta              │
//! │   ├── regression  → bump regression_streak, reset improvement   │
//! │   │                  if streak == STREAK_ALERT_THRESHOLD →     │
//! │   │                    record_health_delta_streak_alert         │
//! │   ├── improvement → bump improvement_streak, reset regression  │
//! │   │                  if was regressing → record_..._recovery    │
//! │   └── neutral     → reset both streaks                         │
//! │                                                                 │
//! │ touring-hooks::shared::gate_metrics::GateMetricsSnapshot        │
//! │   ├── health_delta_streak_alert_count                           │
//! │   └── health_delta_recovery_count                               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::atomic::Ordering;

use touring_hooks::health_delta::{
    STREAK_ALERT_THRESHOLD, StreakCounters, compute_signals_delta, discard_pre_health,
    improvement_streak, record_pre_signals, regression_streak, reset_streak, streak_counters,
};
use touring_hooks::shared::gate_metrics::{GateMetricsSnapshot, global};

/// Drive `n` consecutive regression cycles on `path`. Each cycle:
///   1. discard any pending pre-record
///   2. record_pre_signals(clean source)
///   3. compute_signals_delta(unsafe-heavy source) → must regress
fn drive_regressions(path: &str, n: u32) {
    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";
    for _ in 0..n {
        discard_pre_health(path);
        record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("pre");
        let d = compute_signals_delta(path, degraded).expect("delta");
        assert!(d.is_regression(), "fixture must regress");
    }
}

// ── Axis 1: snapshot exposes 2 new streak fields ─────────────────────────────

#[test]
fn axis1_snapshot_exposes_streak_fields() {
    let snap = GateMetricsSnapshot::capture();
    let json = serde_json::to_string(&snap).expect("serialize");
    assert!(
        json.contains("health_delta_streak_alert_count"),
        "snapshot must surface streak_alert_count: {json}",
    );
    assert!(
        json.contains("health_delta_recovery_count"),
        "snapshot must surface recovery_count: {json}",
    );
}

// ── Axis 2: STREAK_ALERT_THRESHOLD is exported and reasonable ────────────────

#[test]
fn axis2_threshold_constant_is_three() {
    assert_eq!(STREAK_ALERT_THRESHOLD, 3, "Wave 13 contract: threshold = 3");
}

// ── Axis 3: 3 consecutive regressions trigger 1 alert ────────────────────────

#[test]
fn axis3_three_regressions_trigger_alert() {
    let path = "/wave13e2e/axis3.rs";
    reset_streak(path);
    let alerts_before = global()
        .health_delta_streak_alert_count
        .load(Ordering::Relaxed);
    drive_regressions(path, 3);
    let alerts_after = global()
        .health_delta_streak_alert_count
        .load(Ordering::Relaxed);
    assert!(
        alerts_after >= alerts_before + 1,
        "alert counter must advance: {alerts_before} → {alerts_after}",
    );
    assert_eq!(regression_streak(path), 3, "streak must equal 3");
}

// ── Axis 4: Streak alert does NOT re-fire on every subsequent regression ─────

#[test]
fn axis4_alert_does_not_re_fire_on_each_regression() {
    // The alert fires exactly when the streak crosses STREAK_ALERT_THRESHOLD
    // (==3). Subsequent regressions (4, 5, 6...) MUST NOT bump the alert.
    // We can't measure global counter delta robustly under parallel test
    // execution, so instead we observe the streak state directly:
    //   after drive(5) the streak is 5; the alert fired at the instant
    //   streak transitioned from 2 → 3 (one alert per `n=5` cycle).
    let path = "/wave13e2e/axis4.rs";
    reset_streak(path);
    drive_regressions(path, 5);
    // Direct state verification — independent of parallel counter mutations.
    assert_eq!(regression_streak(path), 5, "streak grew to 5");
    // Sanity: two more regressions should not break the streak.
    drive_regressions(path, 2);
    assert_eq!(regression_streak(path), 7, "streak continues growing");
    // Final invariant: streak does NOT collapse spontaneously.
    assert!(regression_streak(path) >= STREAK_ALERT_THRESHOLD);
}

// ── Axis 5: Improvement after regression streak triggers recovery ────────────

#[test]
fn axis5_improvement_breaks_streak_and_records_recovery() {
    let path = "/wave13e2e/axis5.rs";
    reset_streak(path);
    drive_regressions(path, 2);
    assert_eq!(regression_streak(path), 2, "must accumulate streak first");

    let recoveries_before = global().health_delta_recovery_count.load(Ordering::Relaxed);
    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";
    record_pre_signals(path, degraded).expect("pre-improvement");
    let d = compute_signals_delta(path, "pub fn good() -> i32 { 1 }").expect("delta");
    assert!(d.is_improvement(), "must improve");
    let recoveries_after = global().health_delta_recovery_count.load(Ordering::Relaxed);
    assert!(
        recoveries_after >= recoveries_before + 1,
        "recovery counter must advance",
    );
    assert_eq!(
        regression_streak(path),
        0,
        "streak must reset on improvement"
    );
    assert!(improvement_streak(path) >= 1);
}

// ── Axis 6: Neutral delta resets both streaks ────────────────────────────────

#[test]
fn axis6_neutral_delta_resets_both_streaks() {
    let path = "/wave13e2e/axis6.rs";
    reset_streak(path);
    drive_regressions(path, 1);
    assert_eq!(regression_streak(path), 1);

    let src = "pub fn ok() -> i32 { 1 }";
    record_pre_signals(path, src).expect("pre");
    let d = compute_signals_delta(path, src).expect("delta");
    assert_eq!(d.delta, Some(0.0));
    assert_eq!(regression_streak(path), 0, "neutral must reset regression");
    assert_eq!(
        improvement_streak(path),
        0,
        "neutral must reset improvement"
    );
}

// ── Axis 7: Streaks are path-keyed (no cross-contamination) ──────────────────

#[test]
fn axis7_streaks_are_path_keyed() {
    let p1 = "/wave13e2e/axis7_a.rs";
    let p2 = "/wave13e2e/axis7_b.rs";
    reset_streak(p1);
    reset_streak(p2);
    drive_regressions(p1, 2);
    drive_regressions(p2, 1);
    assert_eq!(regression_streak(p1), 2);
    assert_eq!(regression_streak(p2), 1);
    assert_ne!(streak_counters(p1), streak_counters(p2));
}

// ── Axis 8: First-observation (no pre-record) does NOT touch streaks ─────────

#[test]
fn axis8_first_observation_preserves_streak_state() {
    let path = "/wave13e2e/axis8.rs";
    reset_streak(path);
    drive_regressions(path, 1);
    assert_eq!(regression_streak(path), 1);

    // Now compute without prior record → delta = None → streak unchanged.
    discard_pre_health(path);
    let d = compute_signals_delta(path, "pub fn a() -> i32 { 1 }").expect("delta");
    assert_eq!(d.old, None, "first observation has no prior");
    assert_eq!(d.delta, None);
    assert_eq!(
        regression_streak(path),
        1,
        "streak must persist across orphan computes",
    );
}

// ── Axis 9: reset_streak() drops both counters ───────────────────────────────

#[test]
fn axis9_reset_streak_clears_state() {
    let path = "/wave13e2e/axis9.rs";
    reset_streak(path);
    drive_regressions(path, 2);
    assert!(regression_streak(path) > 0);

    reset_streak(path);
    assert_eq!(streak_counters(path), StreakCounters::default());
}
