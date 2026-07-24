#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic indexing
//! Wave 6 — Multi-language Quality Gate E2E (cross-crate).
//!
//! Proves the `touring-generator::QualityGateAdapter` pipeline holds across
//! 4 crates without drift:
//!
//! ```text
//! touring-generator (QualityGateAdapter + extract_inputs + detect_language)
//!   │
//!   ├──▶ touring-analysis::quality::QualityPipeline (analyze_batch)
//!   │       ├──▶ antipatterns::detect_antipatterns (8 langs)
//!   │       ├──▶ complexity::estimate_complexity
//!   │       ├──▶ unwrap_audit::scan_unwraps
//!   │       └──▶ error_coverage::analyze_error_coverage
//!   │
//!   └──▶ touring-ast::{extract_symbols, analyze_quality, Lang} (via wave5_workflow)
//! ```
//!
//! Seven axes verified:
//!   1. `detect_language` covers every extension `analyze_batch` supports
//!   2. Clean multi-lang bundle passes gate (Rust + Python + TS + Go + Java)
//!   3. Unsafe Rust is rejected with `[rust]` language tag
//!   4. Bare-except Python is rejected with `[python]` language tag
//!   5. `as any` TypeScript is rejected with `[typescript]` language tag
//!   6. `.tsx` files are routed to `typescript` (same engine, different ext)
//!   7. Gate is deterministic across 5 repeated runs (same verdict + same score)
//!
//! Each axis pins a cross-crate contract. Drift in any component (e.g. new
//! antipattern added without updating gate, `extract_inputs` regressing to
//! `.rs`-only) breaks one of these axes and the test fails loud.

use touring_analysis::engine::AnalysisConfig;
use touring_generator::core::context::QualityGateAdapter;
use touring_generator::plan::result::{FileAction, RenderedFile};

fn rf(path: &str, content: &str) -> RenderedFile {
    RenderedFile::new(path, content.to_string(), FileAction::Created)
}

fn strict_gate() -> QualityGateAdapter {
    QualityGateAdapter::new(AnalysisConfig::standard()).with_thresholds(0, 0, 0.0)
}

fn lenient_gate() -> QualityGateAdapter {
    // Score floor at 0.0 so the gate only fires on antipatterns/unwraps, not aggregate score.
    QualityGateAdapter::new(AnalysisConfig::standard()).with_thresholds(100, 100, 0.0)
}

// ── Axis 1: detect_language covers every extension the pipeline supports ─────

#[test]
fn axis1_detect_language_covers_all_pipeline_languages() {
    // Every (ext, language_str) pair must match what antipatterns.rs recognizes.
    // Ground truth from crates/touring-analysis/src/quality/antipatterns.rs L12-64.
    let matrix: &[(&str, &str)] = &[
        ("x.rs", "rust"),
        ("x.py", "python"),
        ("x.pyi", "python"),
        ("x.ts", "typescript"),
        ("x.tsx", "typescript"),
        ("x.js", "javascript"),
        ("x.mjs", "javascript"),
        ("x.cjs", "javascript"),
        ("x.jsx", "javascript"),
        ("x.go", "go"),
        ("x.c", "c"),
        ("x.h", "c"),
        ("x.cpp", "cpp"),
        ("x.cc", "cpp"),
        ("x.cxx", "cpp"),
        ("x.hpp", "cpp"),
        ("x.java", "java"),
    ];
    for (path, expected) in matrix {
        assert_eq!(
            QualityGateAdapter::detect_language(path),
            Some(*expected),
            "detect_language cross-crate contract broken for {path}",
        );
    }
}

// ── Axis 2: clean multi-lang bundle passes gate ──────────────────────────────

#[test]
fn axis2_clean_multilang_bundle_passes_gate() {
    let gate = lenient_gate();
    let files = vec![
        rf(
            "src/lib.rs",
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        ),
        rf(
            "src/util.py",
            "def add(a: int, b: int) -> int:\n    return a + b\n",
        ),
        rf(
            "src/util.ts",
            "export function add(a: number, b: number): number {\n  return a + b;\n}\n",
        ),
        rf(
            "src/util.go",
            "package util\nfunc Add(a, b int) int {\n    return a + b\n}\n",
        ),
        rf(
            "src/Util.java",
            "public class Util {\n  public static int add(int a, int b) { return a + b; }\n}\n",
        ),
    ];
    gate.check(&files)
        .expect("clean multi-lang bundle must pass gate");
}

// ── Axis 3: unsafe Rust rejected with [rust] tag ─────────────────────────────

#[test]
fn axis3_unsafe_rust_rejected_with_language_tag() {
    let gate = strict_gate();
    let files = vec![rf(
        "bad.rs",
        "pub fn dangerous() {\n    unsafe {\n        let _ = 42;\n    }\n}\n",
    )];
    let err = gate.check(&files).expect_err("unsafe Rust must fail gate");
    let msg = format!("{err}");
    assert!(msg.contains("[rust]"), "expected [rust] tag, got: {msg}");
}

// ── Axis 4: bare-except Python rejected with [python] tag ────────────────────

#[test]
fn axis4_bare_except_python_rejected_with_language_tag() {
    let gate = strict_gate();
    let files = vec![rf(
        "bad.py",
        "def parse(s):\n    try:\n        return int(s)\n    except:\n        return None\n",
    )];
    let err = gate.check(&files).expect_err("bare except must fail gate");
    let msg = format!("{err}");
    assert!(
        msg.contains("[python]"),
        "expected [python] tag, got: {msg}"
    );
}

// ── Axis 5: `as any` TypeScript rejected with [typescript] tag ───────────────

#[test]
fn axis5_any_cast_typescript_rejected_with_language_tag() {
    let gate = strict_gate();
    let files = vec![rf(
        "bad.ts",
        "export function widen(x: unknown): number {\n  return x as any;\n}\n",
    )];
    let err = gate.check(&files).expect_err("`as any` must fail gate");
    let msg = format!("{err}");
    assert!(
        msg.contains("[typescript]"),
        "expected [typescript] tag, got: {msg}"
    );
}

// ── Axis 6: .tsx files route to typescript language ──────────────────────────

#[test]
fn axis6_tsx_routed_to_typescript_language() {
    let gate = strict_gate();
    let files = vec![rf(
        "ui/App.tsx",
        "export const App = () => {\n  console.log('debug');\n  return null;\n};\n",
    )];
    let err = gate.check(&files).expect_err(".tsx must still flag");
    let msg = format!("{err}");
    assert!(
        msg.contains("[typescript]"),
        "tsx must be routed to typescript pipeline, got: {msg}",
    );
}

// ── Axis 7: gate is deterministic across 5 repeated runs ─────────────────────

#[test]
fn axis7_gate_is_deterministic_five_runs() {
    let files = vec![
        rf("src/a.rs", "pub fn ok(x: i32) -> i32 { x + 1 }\n"),
        rf("src/b.py", "def ok(x: int) -> int:\n    return x + 1\n"),
        rf(
            "src/c.ts",
            "export const ok = (x: number): number => x + 1;\n",
        ),
    ];
    let gate = lenient_gate();

    let mut scores = Vec::with_capacity(5);
    for _ in 0..5 {
        // Build fresh gate each iteration to rule out internal mutation.
        let fresh = lenient_gate();
        fresh
            .check(&files)
            .expect("determinism: must pass every run");
        scores.push(fresh.average_score(&files));
    }
    // All five runs must produce byte-identical scores.
    for w in scores.windows(2) {
        assert!(
            (w[0] - w[1]).abs() < f64::EPSILON,
            "non-deterministic score: {} vs {}",
            w[0],
            w[1]
        );
    }
    // Baseline gate must also agree with the in-loop gates.
    let baseline = gate.average_score(&files);
    assert!(
        (baseline - scores[0]).abs() < f64::EPSILON,
        "baseline gate disagrees with loop: {} vs {}",
        baseline,
        scores[0]
    );
}

// ── Axis 8: unsupported extensions are silently skipped ──────────────────────

#[test]
fn axis8_unsupported_extensions_skipped_silently() {
    let gate = strict_gate();
    // README.md literally contains every antipattern keyword — must NOT fire.
    let files = vec![rf(
        "README.md",
        "Don't use unsafe {} or panic!() or var or console.log in your code!\n",
    )];
    gate.check(&files)
        .expect("docs must be skipped by quality gate");
}

// ── Axis 9: mixed-language bundle — clean files pass, one bad file fails ─────

#[test]
fn axis9_mixed_bundle_surfaces_bad_file_with_language_tag() {
    let gate = strict_gate();
    let files = vec![
        rf("good.rs", "pub fn add(a: i32, b: i32) -> i32 { a + b }\n"),
        rf(
            "good.py",
            "def add(a: int, b: int) -> int:\n    return a + b\n",
        ),
        rf(
            "bad.go",
            "package main\nfunc crash() {\n  panic(\"explicit panic\")\n}\n",
        ),
    ];
    let err = gate
        .check(&files)
        .expect_err("mixed bundle with bad go must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("[go]"),
        "lang tag must surface bad language, got: {msg}"
    );
    assert!(
        msg.contains("bad.go"),
        "file path must appear in error, got: {msg}"
    );
}
