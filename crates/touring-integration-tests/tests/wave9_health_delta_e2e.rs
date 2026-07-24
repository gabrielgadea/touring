#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic indexing
//! Wave 9 — Cross-Hook Health Delta E2E (cross-crate).
//!
//! Proves the pre_edit → edit → post_edit cycle produces a coherent
//! `health_delta` signal consumed uniformly across:
//!
//! ```text
//! touring-hooks::health_delta   ←── new bridge (Wave 9)
//! touring-hooks::wave5_workflow ←── hint/reward producer (Wave 5/8)
//! touring-analysis::quality     ←── RustQualitySignals::health_score
//! touring-ast::rust_semantic    ←── RustSemanticReport (syn 2.0)
//! touring-generator::QualityGateAdapter ←── absolute-quality gate (Wave 7)
//! ```
//!
//! Ten axes pin cross-crate invariants. Each axis uses a distinct file
//! path so the shared `HealthDeltaCache` never collides across parallel
//! test execution.

use touring_generator::core::context::QualityGateAdapter;
use touring_generator::plan::result::{FileAction, RenderedFile};
use touring_hooks::health_delta::{
    HealthDelta, compute_health_delta, delta_reward, discard_pre_health, format_delta_hint,
    record_pre_health,
};
use touring_hooks::wave5_workflow::rust_workflow_hint;

fn rf(path: &str, content: &str) -> RenderedFile {
    RenderedFile::new(path, content.to_string(), FileAction::Created)
}

/// Extract a `key=X.XX` numeric field from an advisory hint.
fn parse_numeric_field(hint: &str, key: &str) -> f64 {
    let idx = hint.find(key).expect("key present");
    let start = idx + key.len();
    let chunk = &hint[start..];
    let end = chunk
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(chunk.len());
    chunk[..end].parse().expect("numeric")
}

// ── Axis 1: pre_edit health matches what wave5_workflow hint reports ──────────

#[test]
fn axis1_pre_edit_health_matches_wave5_hint() {
    let path = "/wave9e2e/axis1.rs";
    let src = "pub fn add(a: i32, b: i32) -> i32 { a + b }";

    let recorded = record_pre_health(path, src).expect("rust source records health");
    let hint = rust_workflow_hint(path, Some(src)).expect("hint emitted");
    let hint_health = parse_numeric_field(&hint, "health=") as f32;

    // Both paths consume RustQualitySignals → must agree (within {:.2} rounding).
    assert!(
        (f64::from(recorded - hint_health)).abs() < 0.02,
        "record vs hint diverge: recorded={recorded}, hint={hint_health}",
    );
    discard_pre_health(path);
}

// ── Axis 2: clean→clean edit has zero delta ───────────────────────────────────

#[test]
fn axis2_identity_edit_has_zero_delta() {
    let path = "/wave9e2e/axis2.rs";
    let src = "pub fn ok(x: i32) -> i32 { x + 1 }";

    record_pre_health(path, src).expect("pre");
    let delta = compute_health_delta(path, src).expect("delta");
    assert_eq!(
        delta.delta,
        Some(0.0),
        "identity edit must produce zero delta"
    );
    assert!(!delta.is_regression());
    assert!(!delta.is_improvement());
}

// ── Axis 3: edit introducing unsafe emits negative delta + regression flag ────

#[test]
fn axis3_regression_edit_emits_negative_delta() {
    let path = "/wave9e2e/axis3.rs";
    let clean = "pub fn ok(x: i32) -> i32 { x + 1 }";
    let degraded = "pub unsafe fn raw() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";

    record_pre_health(path, clean).expect("pre");
    let delta = compute_health_delta(path, degraded).expect("delta");
    assert!(
        delta.delta.expect("delta") < -0.05,
        "regression must drop >0.05"
    );
    assert!(delta.is_regression());
    assert!(!delta.is_improvement());

    // Reward must be in the negative band.
    let r = delta_reward(&delta).expect("reward");
    assert!(r <= -0.05, "regression reward must be <= -0.05, got {r}");
    assert!((-0.10..=0.10).contains(&r), "envelope bounded");
}

// ── Axis 4: refactor removing unsafe emits positive delta + improvement flag ──

#[test]
fn axis4_improvement_edit_emits_positive_delta() {
    let path = "/wave9e2e/axis4.rs";
    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";
    let clean = "pub fn good(x: i32) -> i32 { x + 1 }";

    record_pre_health(path, degraded).expect("pre");
    let delta = compute_health_delta(path, clean).expect("delta");
    assert!(
        delta.delta.expect("delta") > 0.05,
        "improvement must rise >0.05"
    );
    assert!(delta.is_improvement());

    let r = delta_reward(&delta).expect("reward");
    assert!(r >= 0.05, "improvement reward must be >= 0.05, got {r}");
}

// ── Axis 5: post_edit without pre_edit yields old=None delta=None ─────────────

#[test]
fn axis5_post_without_pre_yields_no_delta() {
    let path = "/wave9e2e/axis5.rs";
    discard_pre_health(path); // defensive clear

    let delta = compute_health_delta(path, "pub fn a() -> i32 { 1 }").expect("delta");
    assert_eq!(delta.old, None, "no pre_edit → no old");
    assert_eq!(delta.delta, None, "no pre_edit → no delta");
    assert!(delta.new > 0.0, "new is still computed");
}

// ── Axis 6: non-Rust files return None uniformly ──────────────────────────────

#[test]
fn axis6_non_rust_files_bypass_bridge() {
    assert_eq!(record_pre_health("x.py", "def f(): pass"), None);
    assert_eq!(record_pre_health("x.ts", "export const a = 1;"), None);
    assert_eq!(compute_health_delta("x.go", "package main"), None);
    assert_eq!(compute_health_delta("x.js", "console.log(1);"), None);
}

// ── Axis 7: delta reward envelope matches Wave 5/8 envelope ───────────────────

#[test]
fn axis7_reward_envelope_matches_wave5_envelope() {
    // Wave 5/8 invariant: modulator rewards stay in [-0.10, +0.10] so
    // the phase1 `+1.0` base reward dominates final aggregation.
    let fixtures = [
        (Some(0.40_f32), 0.90_f32), // +0.50 delta → +0.10
        (Some(0.80), 0.90),         // +0.10 delta → +0.05
        (Some(0.85), 0.90),         // +0.05 delta → +0.05
        (Some(0.85), 0.85),         // 0 delta → 0.00
        (Some(0.85), 0.80),         // -0.05 delta → -0.05
        (Some(0.90), 0.40),         // -0.50 delta → -0.10
    ];
    for (old, new) in fixtures {
        let delta = new - old.expect("old");
        let hd = HealthDelta {
            old,
            new,
            delta: Some(delta),
        };
        let r = delta_reward(&hd).expect("reward");
        assert!(
            (-0.10..=0.10).contains(&r),
            "envelope violated: old={old:?} new={new} delta={delta} reward={r}",
        );
    }
}

// ── Axis 8: format_delta_hint produces machine-parseable output ───────────────

#[test]
fn axis8_hint_format_is_machine_parseable() {
    let hd = HealthDelta {
        old: Some(0.92),
        new: 0.68,
        delta: Some(-0.24),
    };
    let hint = format_delta_hint(&hd);
    // Hint must start with the ⚙ marker so log parsers can filter.
    assert!(
        hint.starts_with("⚙ health-delta:"),
        "marker missing: {hint:?}"
    );
    // Numeric fields must appear for downstream parsing.
    assert!(parse_numeric_field(&hint, "old=") > 0.0);
    assert!(parse_numeric_field(&hint, "new=") > 0.0);
    // Δ symbol encoded — extract the signed value after "Δ=".
    assert!(hint.contains("Δ=-0.24"));
    assert!(hint.contains("(regression)"));
}

// ── Axis 9: QualityGateAdapter verdict aligns with delta direction ────────────

#[test]
fn axis9_gate_agrees_with_delta_direction() {
    // Edit: clean → degraded.
    let path = "/wave9e2e/axis9.rs";
    let clean = "pub fn ok(x: i32) -> i32 { x + 1 }";
    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";

    record_pre_health(path, clean).expect("pre");
    let delta = compute_health_delta(path, degraded).expect("delta");
    assert!(delta.is_regression(), "cross-validate: regression expected");

    // Independently, the Wave 7 gate with strict threshold must reject
    // the same degraded source — proves the two systems converge.
    let gate = QualityGateAdapter::new(touring_analysis::engine::AnalysisConfig::standard())
        .with_thresholds(100, 100, 0.0)
        .with_semantic_threshold(0.95);
    let files = vec![rf(path, degraded)];
    gate.check(&files)
        .expect_err("gate must reject degraded source");
}

// ── Axis 10: deterministic across repeated runs ───────────────────────────────

#[test]
fn axis10_bridge_is_deterministic() {
    let path = "/wave9e2e/axis10.rs";
    let src_a = "pub fn a() -> i32 { 1 }";
    let src_b = "pub fn b<T: Clone>(x: T) -> T { x.clone() }";

    let mut deltas = Vec::with_capacity(5);
    for _ in 0..5 {
        record_pre_health(path, src_a).expect("pre");
        let d = compute_health_delta(path, src_b).expect("delta");
        deltas.push(d.delta.expect("delta present"));
    }
    for w in deltas.windows(2) {
        assert!(
            (w[0] - w[1]).abs() < f32::EPSILON,
            "non-deterministic delta: {} vs {}",
            w[0],
            w[1]
        );
    }
}
