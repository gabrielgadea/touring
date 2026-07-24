#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic indexing
//! Wave 14 — Streak Warning Hint E2E (cross-crate).
//!
//! Wave 13 introduced per-path streak counters but only emitted gate
//! metrics. Wave 14 makes the streak VISIBLE to CC by surfacing
//! textual hints in pre_edit and pre_read advisories.
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────┐
//! │ touring-hooks::health_delta::streak_warning_hint(path)        │
//! │   ├── streak >= 3 → Some("⚠ regression streak: N consecutive")│
//! │   └── otherwise   → None                                      │
//! │                                                                │
//! │ touring-hooks::health_delta::improvement_streak_hint(path)    │
//! │   ├── streak >= 3 → Some("✓ improvement streak: N consecutive")│
//! │   └── otherwise   → None                                      │
//! │                                                                │
//! │ Wired into:                                                    │
//! │   - pre_edit::compose_edit_context (Signal 13, after Sig 12)  │
//! │   - pre_read::collect_index_signals (weight 1.5)              │
//! └────────────────────────────────────────────────────────────────┘
//! ```

use touring_hooks::health_delta::{
    STREAK_ALERT_THRESHOLD, compute_signals_delta, discard_pre_health, improvement_streak_hint,
    record_pre_signals, regression_streak, reset_streak, streak_warning_hint,
};

/// Drive `n` consecutive regression cycles on `path`.
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

fn drive_improvements(path: &str, n: u32) {
    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";
    for _ in 0..n {
        discard_pre_health(path);
        record_pre_signals(path, degraded).expect("pre");
        let d = compute_signals_delta(path, "pub fn good() -> i32 { 1 }").expect("delta");
        assert!(d.is_improvement(), "fixture must improve");
    }
}

// ── Axis 1: warning hint API contract ───────────────────────────────────────

#[test]
fn axis1_warning_hint_below_threshold_is_none() {
    let path = "/wave14e2e/axis1.rs";
    reset_streak(path);
    assert_eq!(streak_warning_hint(path), None);
    drive_regressions(path, 2);
    assert_eq!(streak_warning_hint(path), None, "below 3 is silent");
}

#[test]
fn axis2_warning_hint_at_threshold_fires() {
    let path = "/wave14e2e/axis2.rs";
    reset_streak(path);
    drive_regressions(path, STREAK_ALERT_THRESHOLD);
    let hint = streak_warning_hint(path).expect("hint at threshold");
    assert!(hint.starts_with("⚠ regression streak:"));
    assert!(hint.contains(path));
}

#[test]
fn axis3_warning_hint_count_reflects_streak_size() {
    let path = "/wave14e2e/axis3.rs";
    reset_streak(path);
    drive_regressions(path, 7);
    let hint = streak_warning_hint(path).expect("hint at 7");
    assert!(
        hint.contains("7 consecutive"),
        "must show actual streak count: {hint:?}"
    );
}

// ── Axis 4: warning hint clears after recovery ──────────────────────────────

#[test]
fn axis4_warning_clears_after_recovery() {
    let path = "/wave14e2e/axis4.rs";
    reset_streak(path);
    drive_regressions(path, 3);
    assert!(streak_warning_hint(path).is_some());

    drive_improvements(path, 1);
    assert_eq!(
        streak_warning_hint(path),
        None,
        "recovery must clear warning",
    );
    assert_eq!(regression_streak(path), 0);
}

// ── Axis 5: improvement hint mirrors warning structure ──────────────────────

#[test]
fn axis5_improvement_hint_at_threshold() {
    let path = "/wave14e2e/axis5.rs";
    reset_streak(path);
    drive_improvements(path, STREAK_ALERT_THRESHOLD);
    let hint = improvement_streak_hint(path).expect("imp hint");
    assert!(hint.starts_with("✓ improvement streak:"));
    assert!(hint.contains("3 consecutive gains"));
    assert!(hint.contains(path));
}

// ── Axis 6: warning + improvement hints are mutually exclusive at any time ──

#[test]
fn axis6_warning_and_improvement_are_mutually_exclusive() {
    let path = "/wave14e2e/axis6.rs";
    reset_streak(path);

    // Drive regressions → warning fires, improvement is None.
    drive_regressions(path, 3);
    assert!(streak_warning_hint(path).is_some());
    assert_eq!(improvement_streak_hint(path), None);

    // Now drive improvements → warning clears, improvement eventually fires.
    drive_improvements(path, 3);
    assert_eq!(streak_warning_hint(path), None);
    assert!(improvement_streak_hint(path).is_some());
}

// ── Axis 7: hint format is parseable (count + path extractable) ─────────────

#[test]
fn axis7_hint_format_is_parseable() {
    let path = "/wave14e2e/axis7.rs";
    reset_streak(path);
    drive_regressions(path, 4);
    let hint = streak_warning_hint(path).expect("hint");
    // Extract count between "streak: " and " consecutive".
    let after_label = hint.split("streak: ").nth(1).expect("has 'streak: '");
    let count_str = after_label
        .split(" consecutive")
        .next()
        .expect("has ' consecutive'");
    let count: u32 = count_str.parse().expect("count is numeric");
    assert_eq!(count, 4);
}

// ── Axis 8: hints are path-keyed (no cross-contamination) ───────────────────

#[test]
fn axis8_hints_are_path_keyed() {
    let p1 = "/wave14e2e/axis8_a.rs";
    let p2 = "/wave14e2e/axis8_b.rs";
    reset_streak(p1);
    reset_streak(p2);
    drive_regressions(p1, 3);
    // p2 has zero streak.
    assert!(streak_warning_hint(p1).is_some());
    assert_eq!(streak_warning_hint(p2), None);
}

// ── Axis 9: reset_streak clears the hint ────────────────────────────────────

#[test]
fn axis9_reset_clears_hint() {
    let path = "/wave14e2e/axis9.rs";
    reset_streak(path);
    drive_regressions(path, 4);
    assert!(streak_warning_hint(path).is_some());
    reset_streak(path);
    assert_eq!(streak_warning_hint(path), None);
}
