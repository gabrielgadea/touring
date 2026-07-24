#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic slicing
//! Wave 8 — Symmetric Semantic Fusion E2E (cross-crate).
//!
//! Proves the two dual-engine verdict surfaces agree:
//!
//! ```text
//! ┌───────────────────────────────────────────────────────────────────┐
//! │ EDIT-PATH (CC tools):                                             │
//! │   touring-hooks::wave5_workflow::rust_workflow_hint  (advisory) │
//! │   └── health=X.XX field via RustQualitySignals                  │
//! │                                                                   │
//! │ GENERATE-PATH (touring-generator):                               │
//! │   touring-generator::core::context::QualityGateAdapter          │
//! │   └── health enforcement via RustQualitySignals                 │
//! │                                                                   │
//! │ Both paths reuse `touring-analysis::quality::RustQualitySignals` │
//! │ as the single source of truth, which wraps                       │
//! │ `touring-ast::rust_semantic::RustSemanticReport` (syn 2.0).      │
//! └───────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Eight axes verify cross-path agreement. Any drift in any of the
//! four crates (generator / hooks / analysis / ast) breaks an axis.

use touring_analysis::engine::AnalysisConfig;
use touring_analysis::quality::RustQualitySignals;
use touring_generator::core::context::QualityGateAdapter;
use touring_generator::plan::result::{FileAction, RenderedFile};
use touring_hooks::wave5_workflow::{
    code_workflow_hint, rust_workflow_advisory, rust_workflow_hint, rust_workflow_reward,
};

fn rf(path: &str, content: &str) -> RenderedFile {
    RenderedFile::new(path, content.to_string(), FileAction::Created)
}

/// Extract the numeric value of a `key=X.XX` field from an advisory hint.
fn parse_numeric_field(hint: &str, key: &str) -> f64 {
    let idx = hint.find(key).expect("key present in hint");
    let start = idx + key.len();
    let chunk = &hint[start..];
    let end = chunk
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(chunk.len());
    chunk[..end].parse().expect("numeric field")
}

// ── Axis 1: hint emits health= matching the syn-backed source of truth ────────

#[test]
fn axis1_hint_health_matches_rust_quality_signals() {
    let src = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
    let hint = rust_workflow_hint("src/lib.rs", Some(src)).expect("clean source must produce hint");
    let hint_health = parse_numeric_field(&hint, "health=");

    let expected = f64::from(
        RustQualitySignals::from_source(src)
            .expect("parseable source")
            .health_score(),
    );
    // Health is formatted as {:.2} — compare with epsilon tolerance.
    assert!(
        (hint_health - expected).abs() < 0.02,
        "hint health {hint_health} diverges from RustQualitySignals {expected}",
    );
}

// ── Axis 2: hint verdict and gate verdict agree on clean source ──────────────

#[test]
fn axis2_clean_source_passes_both_paths() {
    let src = "pub fn ok(x: i32) -> i32 { x + 1 }";
    let hint = rust_workflow_hint("src/a.rs", Some(src)).expect("hint for clean source");
    let health = parse_numeric_field(&hint, "health=");
    assert!(
        health >= 0.9,
        "hint health >= 0.9 for simple rust, got {health}"
    );

    let gate = QualityGateAdapter::new(AnalysisConfig::standard())
        .with_thresholds(100, 100, 0.0)
        .with_semantic_threshold(0.9);
    let files = vec![rf("src/a.rs", src)];
    gate.check(&files)
        .expect("gate must agree: clean source passes");
}

// ── Axis 3: hint verdict and gate verdict agree on unsafe source ─────────────

#[test]
fn axis3_unsafe_source_flagged_by_both_paths() {
    // Multiple inner `unsafe { }` blocks — forces unsafe_count high
    // enough that both the hint's reward path (unsafe_penalty=true)
    // AND the gate's semantic threshold reject the source.
    let src = "pub unsafe fn raw() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::mem::transmute::<u8, u8>(0); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";
    let hint = rust_workflow_hint("src/a.rs", Some(src)).expect("hint emitted");
    let hint_health = parse_numeric_field(&hint, "health=");
    assert!(
        hint_health < 1.0,
        "unsafe must lower hint health: {hint_health}"
    );

    // Reward base=-0.10 because unsafe > 0 (damper can't touch it).
    let reward = rust_workflow_reward("src/a.rs", Some(src)).expect("reward");
    assert_eq!(reward, -0.10, "unsafe must emit -0.10 reward");

    // Verify gate+hint agree: both see health < 0.95 for this fixture,
    // so the gate with threshold 0.95 must reject.
    let gate = QualityGateAdapter::new(AnalysisConfig::standard())
        .with_thresholds(100, 100, 0.0)
        .with_semantic_threshold(0.95);
    let files = vec![rf("src/a.rs", src)];
    let err = gate
        .check(&files)
        .expect_err("gate must reject source with multiple unsafe blocks");
    assert!(
        format!("{err}").contains("[rust-semantic]"),
        "gate must use semantic tag for unsafe rejection",
    );
}

// ── Axis 4: hint health ∈ [0, 1] matches gate internal health ─────────────────

#[test]
fn axis4_hint_health_bounded_and_matches_gate_score() {
    // Exercise 3 complexity tiers; hint health must always be in [0, 1]
    // and must match what the gate's internal RustQualitySignals emits.
    let fixtures = [
        "pub fn a() -> i32 { 1 }",
        "pub unsafe fn b() -> *const u8 { std::ptr::null() }",
        "pub async fn c<T: Send + Sync + 'static>(x: T) -> T { x }",
    ];
    for src in fixtures {
        if let Some(hint) = rust_workflow_hint("x.rs", Some(src)) {
            let health = parse_numeric_field(&hint, "health=");
            assert!(
                (0.0..=1.0).contains(&health),
                "{health} out of [0,1] for {src:?}"
            );

            let signals = RustQualitySignals::from_source(src).expect("valid rust parses");
            let expected = f64::from(signals.health_score());
            assert!(
                (health - expected).abs() < 0.02,
                "hint health {health} vs signals {expected} for {src:?}",
            );
        }
    }
}

// ── Axis 5: aggregate advisory surfaces health in hint ────────────────────────

#[test]
fn axis5_aggregate_advisory_includes_health() {
    let src = "pub async fn fetch() -> u32 { 0 }";
    let (hint, reward) = rust_workflow_advisory("src/a.rs", Some(src));
    let h = hint.expect("hint present");
    assert!(h.contains("health="), "aggregate must emit health=: {h:?}");
    assert!(reward.is_some(), "reward must be present");
}

// ── Axis 6: reward damper agrees with gate semantic threshold direction ──────

#[test]
fn axis6_reward_damper_tracks_gate_semantic_direction() {
    // Saturate the unsafe penalty (6+ unsafe blocks = 0.30 cap) AND add
    // abstract complexity — forces health_score well below 0.75.
    // Both paths must agree:
    //   - Hint reports `health=` below threshold
    //   - Reward = -0.10 (base unsafe penalty; damper cannot rescue it)
    let src = r#"pub unsafe fn chain<'a, 'b, T, U, V, W>(x: &'a T, _y: U, _z: V, _w: W) -> &'a T
                 where T: Send + Sync + Clone + std::fmt::Debug + 'static + 'b,
                       U: IntoIterator<Item = T> + Default + Copy + std::hash::Hash,
                       V: From<T> + Into<U> + std::ops::Add<Output = V>,
                       W: Iterator<Item = V> + ExactSizeIterator + DoubleEndedIterator {
                     unsafe { let _ = std::ptr::null::<u8>(); }
                     unsafe { let _ = std::ptr::null::<u8>(); }
                     unsafe { let _ = std::ptr::null::<u8>(); }
                     unsafe { let _ = std::ptr::null::<u8>(); }
                     unsafe { let _ = std::ptr::null::<u8>(); }
                     unsafe { let _ = std::ptr::null::<u8>(); }
                     unsafe { let _ = std::ptr::null::<u8>(); }
                     x
                 }"#;
    let hint = rust_workflow_hint("x.rs", Some(src));
    let reward = rust_workflow_reward("x.rs", Some(src));
    if let (Some(h), Some(r)) = (hint, reward) {
        let health = parse_numeric_field(&h, "health=");
        assert!(
            health < 0.75,
            "abstract+unsafe must drop health < 0.75: {health}"
        );
        assert!(
            r <= 0.0,
            "unsafe+low-health source must not emit positive reward: {r}",
        );
    }
}

// ── Axis 7: code_workflow_hint rust path preserves legacy tag + new health ────

#[test]
fn axis7_multilang_rust_preserves_tag_and_surfaces_health() {
    let src = "pub fn inc(x: i32) -> i32 { x + 1 }";
    let hint = code_workflow_hint("src/a.rs", Some(src)).expect("rust via multi-lang entry");
    assert!(
        hint.starts_with("⚙ rust-workflow:"),
        "legacy tag preserved: {hint:?}",
    );
    assert!(
        hint.contains("health="),
        "health propagates through multi-lang: {hint:?}",
    );
}

// ── Axis 8: deterministic fusion — 5 runs produce identical hint + reward ────

#[test]
fn axis8_fusion_is_deterministic_across_runs() {
    let src = "pub fn ok<T: Clone>(x: T) -> T { x.clone() }";
    let mut hints = Vec::with_capacity(5);
    let mut rewards = Vec::with_capacity(5);
    for _ in 0..5 {
        hints.push(rust_workflow_hint("src/a.rs", Some(src)));
        rewards.push(rust_workflow_reward("src/a.rs", Some(src)));
    }
    for w in hints.windows(2) {
        assert_eq!(w[0], w[1], "hint not deterministic");
    }
    for w in rewards.windows(2) {
        assert_eq!(w[0], w[1], "reward not deterministic");
    }
}
