#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic indexing
//! Wave 15 — pre_write streak parity + CLI exposure E2E (cross-crate).
//!
//! Two integration concerns validated here:
//!
//! 1. **pre_write parity**: the Wave 14 streak hint helpers
//!    (`streak_warning_hint`, `improvement_streak_hint`) must be
//!    callable from any consumer that wants to surface them.
//!    `pre_write::ast_content_signals` invokes them via
//!    `health_delta::*` exactly like pre_edit; this E2E confirms
//!    cross-crate availability.
//!
//! 2. **CLI handler payload contract**: the new
//!    `touring health-delta {status,reset}` subcommands must produce
//!    JSON that downstream tools can parse. We invoke the touring
//!    binary directly via `std::process::Command` against the live
//!    daemon socket — proving the full client→daemon→handler chain.

use serde_json::Value;
use std::process::Command;

use touring_hooks::health_delta::{
    STREAK_ALERT_THRESHOLD, compute_signals_delta, discard_pre_health, improvement_streak_hint,
    record_pre_signals, regression_streak, reset_streak, streak_warning_hint,
};

/// Path to the release `touring` binary used for CLI E2E.
const TOURING_BIN: &str = "/home/gabrielgadea/.claude/rust/target/release/touring";

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

/// Skip helper for environments where the touring binary is not built.
fn binary_available() -> bool {
    std::path::Path::new(TOURING_BIN).exists()
}

// ── Axis 1: pre_write streak hint helpers reachable from cross-crate ────────

#[test]
fn axis1_pre_write_hint_helpers_reachable() {
    // pre_write::ast_content_signals invokes these exact helpers; we
    // confirm they remain callable from cross-crate consumers and
    // produce deterministic output.
    let path = "/wave15e2e/axis1.rs";
    reset_streak(path);
    drive_regressions(path, 4);

    let warning = streak_warning_hint(path).expect("warning fires");
    assert!(warning.contains("4 consecutive"), "format: {warning:?}");
    assert_eq!(improvement_streak_hint(path), None);
    assert_eq!(regression_streak(path), 4);
}

// ── Axis 2: pre_write parity — same helper reset across pre_edit/pre_write ──

#[test]
fn axis2_pre_write_parity_with_pre_edit() {
    // Both pre_edit and pre_write call streak_warning_hint(path) on the
    // SAME singleton cache. After a regression streak the hint fires
    // for any caller — proving symmetric coverage.
    let path = "/wave15e2e/axis2.rs";
    reset_streak(path);
    drive_regressions(path, STREAK_ALERT_THRESHOLD);

    // Caller A (pre_edit-style): consumes the warning.
    let warn_a = streak_warning_hint(path);
    assert!(warn_a.is_some());

    // Caller B (pre_write-style): same singleton, identical output.
    let warn_b = streak_warning_hint(path);
    assert_eq!(warn_a, warn_b, "same cache must produce identical hints");
}

// ── Axis 3: CLI status without path returns aggregate JSON ──────────────────

#[test]
fn axis3_cli_status_aggregate_returns_valid_json() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let out = Command::new(TOURING_BIN)
        .args(["health-delta", "status"])
        .output()
        .expect("spawn touring");
    assert!(out.status.success(), "status failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("valid JSON");
    for key in &[
        "record_count",
        "compute_count",
        "regression_count",
        "improvement_count",
        "outstanding",
        "streak_alert_count",
        "recovery_count",
        "alert_threshold",
    ] {
        assert!(
            v.get(key).is_some(),
            "aggregate must include `{key}`: {stdout}"
        );
    }
    assert_eq!(
        v["alert_threshold"].as_u64(),
        Some(u64::from(STREAK_ALERT_THRESHOLD)),
    );
}

// ── Axis 4: CLI status with file_path returns per-path JSON ─────────────────

#[test]
fn axis4_cli_status_with_path_returns_per_path_json() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let path = "/tmp/wave15e2e_axis4.rs";
    let out = Command::new(TOURING_BIN)
        .args(["health-delta", "status", path])
        .output()
        .expect("spawn touring");
    assert!(out.status.success(), "status w/ path failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(v["file_path"].as_str(), Some(path));
    for key in &[
        "regression_streak",
        "improvement_streak",
        "warning_hint",
        "improvement_hint",
        "alert_threshold",
    ] {
        assert!(
            v.get(key).is_some(),
            "per-path must include `{key}`: {stdout}"
        );
    }
}

// ── Axis 5: CLI reset without path errors out ───────────────────────────────

#[test]
fn axis5_cli_reset_without_path_errors() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let out = Command::new(TOURING_BIN)
        .args(["health-delta", "reset"])
        .output()
        .expect("spawn touring");
    // Should fail (non-zero exit OR error message in stderr).
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        !out.status.success() || combined.contains("usage") || combined.contains("error"),
        "reset without path must error: stdout={stdout:?} stderr={stderr:?}",
    );
}

// ── Axis 6: CLI reset with valid path returns success JSON ──────────────────

#[test]
fn axis6_cli_reset_with_path_returns_success() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let path = "/tmp/wave15e2e_axis6.rs";
    let out = Command::new(TOURING_BIN)
        .args(["health-delta", "reset", path])
        .output()
        .expect("spawn touring");
    assert!(out.status.success(), "reset failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(v["reset"].as_bool(), Some(true));
    assert_eq!(v["file_path"].as_str(), Some(path));
}

// ── Axis 7: CLI rejects unknown subcommand ──────────────────────────────────

#[test]
fn axis7_cli_unknown_subcommand_fails() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let out = Command::new(TOURING_BIN)
        .args(["health-delta", "bogus"])
        .output()
        .expect("spawn touring");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && (stderr.contains("unrecognized") || stderr.contains("unknown")),
        "must reject unknown subcommand: {stderr}",
    );
}

// ── Axis 8: CLI status default subcommand is `status` ───────────────────────

#[test]
fn axis8_cli_status_is_default_subcommand() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    // `touring health-delta` (no sub) must default to status (aggregate).
    let out = Command::new(TOURING_BIN)
        .args(["health-delta"])
        .output()
        .expect("spawn touring");
    assert!(out.status.success(), "default status failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(
        v.get("alert_threshold").is_some(),
        "default invokes status: {stdout}"
    );
}
