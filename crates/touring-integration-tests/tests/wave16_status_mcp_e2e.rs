#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic indexing
//! Wave 16 — Status dashboard + MCP exposure E2E (cross-crate).
//!
//! Two integration concerns:
//!
//! 1. **`touring status -j` includes `health_delta`** — the unified
//!    session-start dashboard now exposes the streak-tracking
//!    subsystem alongside `gate_metrics`, `learning`, `wiring`, etc.
//!
//! 2. **MCP tool exposure** — `touring_health_delta_status` and
//!    `touring_health_delta_reset` are addressable by Claude Code via
//!    the MCP server. We validate the underlying pure functions
//!    (`status_json` / `reset_json`) which both surfaces delegate to,
//!    proving the single-source-of-truth invariant.

use std::process::Command;

use touring_hooks::health_delta::{
    STREAK_ALERT_THRESHOLD, compute_signals_delta, discard_pre_health, record_pre_signals,
    regression_streak, reset_streak, status_json,
};

const TOURING_BIN: &str = "/home/gabrielgadea/.claude/rust/target/release/touring";

fn binary_available() -> bool {
    std::path::Path::new(TOURING_BIN).exists()
}

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

// ── Axis 1: pure status_json aggregate is well-formed ───────────────────────

#[test]
fn axis1_status_json_aggregate_is_well_formed() {
    let out = status_json(None);
    let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
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
            "aggregate must include `{key}`: {out}"
        );
    }
}

// ── Axis 2: pure status_json per-path matches recorded streak state ─────────

#[test]
fn axis2_status_json_per_path_matches_state() {
    let path = "/wave16e2e/axis2.rs";
    reset_streak(path);
    drive_regressions(path, STREAK_ALERT_THRESHOLD);

    let out = status_json(Some(path));
    let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(v["regression_streak"].as_u64(), Some(3));
    assert!(v["warning_hint"].is_string(), "must surface warning: {out}");
}

// ── Axis 3: `touring status -j` includes `health_delta` key ─────────────────

#[test]
fn axis3_touring_status_includes_health_delta_key() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let out = Command::new(TOURING_BIN)
        .args(["status", "-j"])
        .output()
        .expect("spawn touring status");
    assert!(out.status.success(), "status -j failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("status JSON valid");
    assert!(
        v.get("health_delta").is_some(),
        "status dashboard must include health_delta key: {stdout}",
    );
    let hd = &v["health_delta"];
    assert!(
        hd.get("alert_threshold").is_some(),
        "health_delta entry must contain aggregate fields: {hd}",
    );
}

// ── Axis 4: status_json is byte-identical between aggregate calls ───────────

#[test]
fn axis4_status_json_aggregate_is_stable_shape() {
    // Schema is stable across calls (counters may differ but keys must match).
    let a = status_json(None);
    let b = status_json(None);
    let va: serde_json::Value = serde_json::from_str(&a).expect("a");
    let vb: serde_json::Value = serde_json::from_str(&b).expect("b");
    let keys_a: Vec<&str> = va
        .as_object()
        .expect("obj")
        .keys()
        .map(String::as_str)
        .collect();
    let keys_b: Vec<&str> = vb
        .as_object()
        .expect("obj")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys_a, keys_b, "schema drift between calls");
}

// ── Axis 5: MCP tool function signatures resolve (compile-time check) ───────

#[test]
fn axis5_mcp_helpers_compile_and_return_string() {
    // The MCP tools call status_json + reset_json directly — same as
    // the CLI handlers. We verify the pure functions are reachable
    // and return a non-empty String.
    let agg = status_json(None);
    assert!(!agg.is_empty(), "aggregate must produce JSON");

    let path = "/wave16e2e/axis5.rs";
    let reset_out = touring_hooks::health_delta::reset_json(path);
    let v: serde_json::Value = serde_json::from_str(&reset_out).expect("reset JSON");
    assert_eq!(v["reset"].as_bool(), Some(true));
    assert_eq!(v["file_path"].as_str(), Some(path));
}

// ── Axis 6: CLI handler / pure function / MCP tool share JSON SHAPE ─────────

#[test]
fn axis6_single_source_of_truth_invariant() {
    // Note: the daemon process and the test process have separate
    // `STREAK_CACHE` singletons (process-scoped). We can't compare
    // STATE — instead we validate SHAPE: both surfaces must emit
    // the same set of keys, proving they share the same JSON
    // contract from `status_json`.
    let path = "/wave16e2e/axis6.rs";
    let pure = status_json(Some(path));
    if !binary_available() {
        eprintln!("skipping CLI half: {TOURING_BIN} not built");
        return;
    }
    let out = Command::new(TOURING_BIN)
        .args(["health-delta", "status", path])
        .output()
        .expect("spawn touring");
    assert!(out.status.success(), "cli failed: {:?}", out);
    let cli = String::from_utf8_lossy(&out.stdout).to_string();
    let v_pure: serde_json::Value = serde_json::from_str(&pure).expect("pure JSON");
    let v_cli: serde_json::Value = serde_json::from_str(&cli).expect("cli JSON");
    let mut keys_pure: Vec<&str> = v_pure
        .as_object()
        .expect("obj")
        .keys()
        .map(String::as_str)
        .collect();
    let mut keys_cli: Vec<&str> = v_cli
        .as_object()
        .expect("obj")
        .keys()
        .map(String::as_str)
        .collect();
    keys_pure.sort();
    keys_cli.sort();
    assert_eq!(
        keys_pure, keys_cli,
        "pure status_json and CLI must emit identical JSON shape",
    );
    // file_path is preserved by both.
    assert_eq!(v_pure["file_path"].as_str(), Some(path));
    assert_eq!(v_cli["file_path"].as_str(), Some(path));
}

// ── Axis 7: `touring status -j` is parseable end-to-end ─────────────────────

#[test]
fn axis7_touring_status_parses_full_dashboard() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let out = Command::new(TOURING_BIN)
        .args(["status", "-j"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("status -j must be valid JSON");
    // Confirm Wave 16 sits next to Wave 12 in the same dashboard.
    for key in &[
        "daemon_health",
        "index",
        "wiring",
        "sessions",
        "learning",
        "incremental",
        "gate_metrics",
        "health_delta",
    ] {
        assert!(
            v.get(key).is_some(),
            "status dashboard missing `{key}`: {}",
            String::from_utf8_lossy(&out.stdout),
        );
    }
}

// ── Axis 8: regression_streak field reflects underlying state ───────────────

#[test]
fn axis8_regression_streak_field_reflects_state() {
    let path = "/wave16e2e/axis8.rs";
    reset_streak(path);
    assert_eq!(regression_streak(path), 0);
    let out = status_json(Some(path));
    let v: serde_json::Value = serde_json::from_str(&out).expect("valid");
    assert_eq!(v["regression_streak"].as_u64(), Some(0));
    assert_eq!(v["improvement_streak"].as_u64(), Some(0));
}
