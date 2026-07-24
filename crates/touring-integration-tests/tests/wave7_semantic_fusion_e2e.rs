#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic indexing
//! Wave 7 — Semantic Fusion Gate E2E (cross-crate).
//!
//! Proves that `QualityGateAdapter` correctly fuses two complementary
//! engines into a single decision surface:
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────┐
//! │ touring-generator::QualityGateAdapter                            │
//! │   ├──▶ touring-analysis::quality::QualityPipeline                │
//! │   │     └── tree-sitter: antipatterns + unwrap + complexity     │
//! │   │                                                              │
//! │   └──▶ touring-analysis::quality::RustQualitySignals             │
//! │         └── touring-ast::rust_semantic::RustSemanticReport       │
//! │             └── syn 2.0: generics, lifetimes, unsafe, bounds   │
//! │                                                                  │
//! │ Fusion verdict = AND(tree_sitter_ok, syn_health ≥ threshold)    │
//! │ Fusion score   = (tree_sitter_score + syn_health) / 2           │
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Nine axes pin cross-crate invariants; drift in any of the three
//! crates (generator / analysis / ast) breaks one and fails loud.

use touring_analysis::engine::AnalysisConfig;
use touring_analysis::quality::RustQualitySignals;
use touring_generator::core::context::QualityGateAdapter;
use touring_generator::plan::result::{FileAction, RenderedFile};

fn rf(path: &str, content: &str) -> RenderedFile {
    RenderedFile::new(path, content.to_string(), FileAction::Created)
}

/// Gate with semantic fusion ON (strict health threshold), non-semantic relaxed.
fn fusion_gate(min_health: f32) -> QualityGateAdapter {
    QualityGateAdapter::new(AnalysisConfig::standard())
        .with_thresholds(100, 100, 0.0)
        .with_semantic_threshold(min_health)
}

// ── Axis 1: gate with semantic OFF preserves legacy behaviour ─────────────────

#[test]
fn axis1_semantic_off_is_backwards_compatible() {
    // Default construction must keep semantic disabled — existing callers
    // (touring-server::make_context, wave6 E2E) must stay green.
    let gate = QualityGateAdapter::new(AnalysisConfig::standard()).with_thresholds(100, 100, 0.0);
    let files = vec![rf(
        "abstract.rs",
        "pub fn deep<'a, T: Send + Sync + Clone + 'static>(x: &'a T) -> &'a T { x }\n",
    )];
    gate.check(&files)
        .expect("default gate must not apply semantic threshold");
}

// ── Axis 2: with_semantic_threshold rejects unsafe Rust ────────────────────────

#[test]
fn axis2_semantic_gate_rejects_unsafe() {
    let gate = fusion_gate(0.9);
    let files = vec![rf(
        "danger.rs",
        "pub unsafe fn boom() {\n    unsafe { std::ptr::null::<u8>(); }\n}\n",
    )];
    let err = gate.check(&files).expect_err("unsafe must be blocked");
    let msg = format!("{err}");
    assert!(
        msg.contains("[rust-semantic]"),
        "semantic tag missing: {msg}"
    );
    assert!(msg.contains("unsafe="), "must report unsafe count: {msg}");
    assert!(msg.contains("complexity="), "must report complexity: {msg}");
}

// ── Axis 3: clean Rust passes the fused gate ──────────────────────────────────

#[test]
fn axis3_clean_rust_passes_fusion() {
    let gate = fusion_gate(0.9);
    let files = vec![rf("ok.rs", "pub fn add(a: i32, b: i32) -> i32 { a + b }\n")];
    gate.check(&files).expect("clean rust must pass fusion");
}

// ── Axis 4: semantic gate only applies to .rs files ────────────────────────────

#[test]
fn axis4_semantic_gate_skips_non_rust() {
    // Fusion enabled but every file is non-Rust: gate must be a no-op for them.
    let gate = fusion_gate(0.9);
    let files = vec![
        rf("a.py", "def add(a, b): return a + b\n"),
        rf(
            "b.ts",
            "export const add = (a: number, b: number) => a + b;\n",
        ),
        rf(
            "c.go",
            "package main\nfunc Add(a, b int) int { return a + b }\n",
        ),
    ];
    gate.check(&files)
        .expect("non-rust must bypass semantic fusion entirely");
}

// ── Axis 5: unparseable Rust is conservative — gate does not crash ─────────────

#[test]
fn axis5_unparseable_rust_does_not_crash_gate() {
    let gate = fusion_gate(0.9);
    let files = vec![rf(
        "broken.rs",
        // Syntactically garbage — from_source returns None.
        "this {{{{ is not valid rust ::::: &&&&\n",
    )];
    // Tree-sitter-based pipeline tolerates malformed input; semantic path
    // returns None on parse fail and is skipped. Gate must return Ok.
    gate.check(&files)
        .expect("unparseable rust must be skipped gracefully");
}

// ── Axis 6: fusion score for simple Rust blends tree-sitter + syn ─────────────

#[test]
fn axis6_fusion_score_blends_signals_for_rust() {
    let gate = fusion_gate(0.1);
    let files = vec![rf("simple.rs", "pub fn inc(x: i32) -> i32 { x + 1 }\n")];
    let fused = gate.average_score(&files);
    // Cross-check: compute expected blend independently.
    let signals =
        RustQualitySignals::from_source(&files[0].content).expect("simple rust must parse");
    let health = f64::from(signals.health_score());
    assert!(
        health >= 0.9,
        "trivial safe rust must score >= 0.9, got {health}"
    );
    // Gate's average = (tree_sitter_score + syn_health) / 2.
    // Tree-sitter score is >= 0.5 for trivial safe code, so fused >= 0.7.
    assert!(
        fused >= 0.7,
        "fused score must be >= 0.7 for simple rust, got {fused}"
    );
    assert!(fused <= 1.0, "fused score must be bounded, got {fused}");
}

// ── Axis 7: fusion score for non-Rust is pure tree-sitter (no blending) ───────

#[test]
fn axis7_fusion_score_is_pure_treesitter_for_non_rust() {
    let gate = fusion_gate(0.9);
    let files = vec![rf(
        "clean.ts",
        "export function add(a: number, b: number): number { return a + b; }\n",
    )];
    let fused_enabled = gate.average_score(&files);
    let gate_disabled =
        QualityGateAdapter::new(AnalysisConfig::standard()).with_thresholds(100, 100, 0.0);
    let pure_treesitter = gate_disabled.average_score(&files);
    assert!(
        (fused_enabled - pure_treesitter).abs() < f64::EPSILON,
        "non-rust score must be unaffected by semantic fusion: {fused_enabled} vs {pure_treesitter}",
    );
}

// ── Axis 8: mixed bundle — unsafe .rs caught, clean .py passes ────────────────

#[test]
fn axis8_mixed_bundle_surfaces_semantic_violation() {
    let gate = fusion_gate(0.9);
    let files = vec![
        rf("good.py", "def ok(a: int) -> int:\n    return a + 1\n"),
        rf(
            "bad.rs",
            "pub unsafe fn boom() { unsafe { std::ptr::null::<u8>(); } }\n",
        ),
    ];
    let err = gate
        .check(&files)
        .expect_err("unsafe .rs must fail fused gate");
    let msg = format!("{err}");
    assert!(msg.contains("[rust-semantic]"), "semantic tag: {msg}");
    assert!(msg.contains("bad.rs"), "file path: {msg}");
}

// ── Axis 9: fusion verdict is deterministic across 5 runs ─────────────────────

#[test]
fn axis9_fusion_is_deterministic_across_runs() {
    let files = vec![
        rf("a.rs", "pub fn ok(x: i32) -> i32 { x + 1 }\n"),
        rf("b.rs", "pub fn id<T>(x: T) -> T { x }\n"),
    ];
    let mut scores = Vec::with_capacity(5);
    for _ in 0..5 {
        let gate = fusion_gate(0.1);
        gate.check(&files).expect("must pass every run");
        scores.push(gate.average_score(&files));
    }
    for w in scores.windows(2) {
        assert!(
            (w[0] - w[1]).abs() < f64::EPSILON,
            "non-deterministic fused score: {} vs {}",
            w[0],
            w[1]
        );
    }
}

// ── Axis 10: semantic threshold is independent of tree-sitter thresholds ──────

#[test]
fn axis10_semantic_threshold_independent_of_treesitter() {
    // Tree-sitter pass (thresholds relaxed); semantic fails on unsafe.
    // Must still reject because semantic gate operates independently.
    let gate = QualityGateAdapter::new(AnalysisConfig::standard())
        .with_thresholds(1000, 1000, 0.0) // tree-sitter = effectively off
        .with_semantic_threshold(0.9);
    let files = vec![rf("unsafe.rs", "pub unsafe fn danger() { unsafe {} }\n")];
    let err = gate
        .check(&files)
        .expect_err("semantic must reject even when tree-sitter relaxed");
    let msg = format!("{err}");
    assert!(msg.contains("[rust-semantic]"), "tag: {msg}");
}
