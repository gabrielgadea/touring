//! Wave 5 (2026-04-18) — Cross-crate end-to-end validation.
//!
//! # What this test proves
//!
//! The automated code-generation pipeline built across the Wave 5
//! iterations spans four crates:
//!
//! ```text
//!   touring-ast           → CodeGenWorkflow::analyze       (parse + semantic + format)
//!   touring-analysis      → SecurityDb::scan_package       (advisory DB lookup)
//!   touring-hooks         → wave5_workflow::*              (hint + reward mapping)
//!   touring-learning      → ImmediateReward / LinUCB       (RL updates)
//! ```
//!
//! Isolated unit tests validate each crate individually. This file
//! proves that the **composition** holds:
//!
//! 1. A representative `.rs` source flows through `CodeGenWorkflow`.
//! 2. The V6 helper maps the result to both a hint string and a
//!    bounded reward value.
//! 3. The advisory + reward are consistent across the split
//!    (`rust_workflow_hint` / `rust_workflow_reward`) and aggregate
//!    (`rust_workflow_advisory`) APIs.
//! 4. The reward falls inside the envelope the RL engine expects.
//! 5. The `SecurityDb` offline path integrates cleanly alongside the
//!    semantic pipeline without producing false positives.
//!
//! Running `cargo test -p touring-integration-tests --test wave5_cross_crate_e2e`
//! therefore catches regressions that a single-crate test matrix would
//! miss: API drift across crate boundaries, reward envelope changes,
//! hint format divergence.

use touring_analysis::SecurityDb;
use touring_code::ast::{CodeGenWorkflow, Lang, WorkflowReport};
use touring_hooks::wave5_workflow::{
    rust_workflow_advisory, rust_workflow_hint, rust_workflow_reward,
};

/// Stable enumeration of the `Lang` variants this integration test
/// cares about. We avoid `strum::IntoEnumIterator` here because
/// `touring-integration-tests` does not depend on `strum` — the
/// representative subset below is sufficient to prove the
/// `touring-ast::Lang` surface is reachable cross-crate.
const REPRESENTATIVE_LANGS: &[Lang] =
    &[Lang::Rust, Lang::Python, Lang::TypeScript, Lang::JavaScript];

/// Canonical clean-code fixture. `pub fn add` with trivial body — the
/// kind of edit Claude Code produces frequently.
const CLEAN_SOURCE: &str = "pub fn add(a: u32, b: u32) -> u32 { a + b }";

/// Canonical risky fixture — `unsafe` in a pub function. Wave 5
/// mapping must downweight this edit in the RL signal.
const UNSAFE_SOURCE: &str = "pub unsafe fn raw_ptr() -> *const u8 { std::ptr::null() }";

/// Canonical trivial fixture — private helper with no complexity.
/// Wave 5 mapping MUST skip reward injection here (trivial edits are
/// not learning-worthy).
const TRIVIAL_SOURCE: &str = "fn _internal() {}";

// ─── Axis 1: CodeGenWorkflow (touring-ast) — deep semantic pass ────

#[test]
fn axis1_code_gen_workflow_produces_consistent_report_on_clean_source() {
    let report: WorkflowReport =
        CodeGenWorkflow::analyze(CLEAN_SOURCE).expect("CLEAN_SOURCE must parse");
    assert!(report.has_public_surface(), "pub fn must surface");
    assert!(report.formatted_source.is_some(), "format path wired");
    assert_eq!(report.semantic.async_fns, 0);
    assert_eq!(report.semantic.unsafe_blocks, 0);
    // Complexity band for a one-liner is simple.
    assert_eq!(report.complexity_band(), "simple");
}

#[test]
fn axis1_unsafe_source_increments_unsafe_block_counter() {
    let report = CodeGenWorkflow::analyze(UNSAFE_SOURCE).expect("UNSAFE_SOURCE must parse");
    assert!(
        report.semantic.unsafe_blocks >= 1,
        "unsafe fn must surface in the semantic visitor"
    );
}

#[test]
fn axis1_malformed_source_errors_without_panic() {
    let malformed = "pub fn broken( { unclosed";
    assert!(CodeGenWorkflow::analyze(malformed).is_err());
    // Defensive: `analyze_no_format` on the same input must also
    // error — divergence between the two entry points would be a bug.
    assert!(CodeGenWorkflow::analyze_no_format(malformed).is_err());
}

// ─── Axis 2: wave5_workflow (touring-hooks) — V6 advisory bridge ──

#[test]
fn axis2_wave5_hint_matches_expected_format_for_clean_source() {
    let hint =
        rust_workflow_hint("lib.rs", Some(CLEAN_SOURCE)).expect("clean source must emit hint");
    assert!(hint.starts_with("⚙ rust-workflow:"));
    assert!(hint.contains("pub_surface=1"));
    assert!(hint.contains("(simple)"));
    assert!(hint.contains("unsafe=0"));
    assert!(hint.contains("async_fns=0"));
}

#[test]
fn axis2_wave5_reward_maps_clean_to_positive() {
    let reward = rust_workflow_reward("lib.rs", Some(CLEAN_SOURCE))
        .expect("clean source must produce reward");
    assert!(
        (reward - 0.10).abs() < f64::EPSILON,
        "expected +0.10, got {reward}"
    );
}

#[test]
fn axis2_wave5_reward_maps_unsafe_to_negative() {
    let reward = rust_workflow_reward("lib.rs", Some(UNSAFE_SOURCE))
        .expect("unsafe source must produce reward");
    assert!(
        (reward + 0.10).abs() < f64::EPSILON,
        "expected -0.10, got {reward}"
    );
}

#[test]
fn axis2_wave5_reward_skips_trivial_and_non_rust() {
    assert_eq!(rust_workflow_reward("lib.rs", Some(TRIVIAL_SOURCE)), None);
    assert_eq!(rust_workflow_reward("lib.py", Some(CLEAN_SOURCE)), None);
    assert_eq!(rust_workflow_reward("empty.rs", Some("")), None);
}

#[test]
fn axis2_split_and_aggregate_apis_must_agree() {
    // The `rust_workflow_advisory` aggregate must produce identical
    // (hint, reward) tuples as the split APIs. If they drift, callers
    // that chose one or the other will see inconsistent V6 signals.
    let fixtures = [
        ("lib.rs", CLEAN_SOURCE),
        ("lib.rs", UNSAFE_SOURCE),
        ("lib.rs", TRIVIAL_SOURCE),
        ("lib.py", CLEAN_SOURCE), // non-Rust ignored
    ];
    for (path, src) in fixtures {
        let hint_split = rust_workflow_hint(path, Some(src));
        let reward_split = rust_workflow_reward(path, Some(src));
        let (hint_agg, reward_agg) = rust_workflow_advisory(path, Some(src));
        assert_eq!(hint_split, hint_agg, "hint divergence for {path}/{src:?}");
        assert_eq!(
            reward_split, reward_agg,
            "reward divergence for {path}/{src:?}"
        );
    }
}

// ─── Axis 3: Reward envelope invariant (touring-learning contract) ──

#[test]
fn axis3_reward_envelope_is_respected_across_all_bands() {
    // The RL engine uses the reward as a `quality_score ∈ [0, 1]`
    // after normalization. Wave 5's raw envelope is `[-0.10, +0.10]`.
    // Values outside this range would bias the learner.
    let fixtures = [
        ("lib.rs", "pub fn a() {}"),
        ("lib.rs", "pub unsafe fn u() {}"),
        ("lib.rs", "pub async fn c() {}"),
        (
            "lib.rs",
            r#"pub fn big<T: Clone + Send + 'static, U>(_: T, _: U) -> u32 where T: std::fmt::Debug { 0 }"#,
        ),
    ];
    for (path, src) in fixtures {
        if let Some(r) = rust_workflow_reward(path, Some(src)) {
            assert!(
                (-0.10..=0.10).contains(&r),
                "envelope violation: reward={r} for {src:?}"
            );
        }
    }
}

// ─── Axis 4: SecurityDb (touring-analysis) — advisory DB offline ───

#[test]
fn axis4_security_db_offline_never_errors() {
    let db = SecurityDb::offline();
    assert!(!db.is_online(), "offline() must report offline");
    assert_eq!(
        db.scan_package("serde", "1.0.0").len(),
        0,
        "offline scan must always return empty"
    );
    // try_open is safe to call regardless of DB presence.
    let _ = SecurityDb::try_open();
}

// ─── Axis 5: Lang (touring-ast) — cross-crate reachability ─────────
//
// `strum::EnumIter` exhaustive iteration is covered by the in-crate
// `touring-ast::languages::tests::test_lang_iter_contains_every_variant`
// test. Here we only validate that the representative subset of
// `Lang` variants is reachable from `touring-integration-tests` AND
// that `as_str` produces a stable lowercase tag.

#[test]
fn axis5_representative_langs_are_reachable_cross_crate() {
    for lang in REPRESENTATIVE_LANGS {
        let tag = lang.as_str();
        assert!(!tag.is_empty(), "Lang::{lang:?}::as_str must not be empty");
        // Round-trip through FromStr (proves the manual impl in
        // touring-ast::languages is reachable cross-crate).
        let parsed: Lang = tag
            .parse()
            .unwrap_or_else(|e| panic!("round-trip failed for {lang:?}: {e}"));
        assert_eq!(parsed, *lang, "round-trip mismatch for {lang:?}");
    }
}

// ─── Axis 6: Composition — parse → V6 → reward pipeline ────────────

#[test]
fn axis6_full_pipeline_composes_without_regression() {
    // This is the integration heart: for every fixture, we exercise
    // the exact sequence that `post_edit::run_returning` calls:
    //
    //   1. CodeGenWorkflow::analyze(src)
    //   2. wave5_workflow::rust_workflow_advisory(path, Some(src))
    //
    // The two must agree on which fixtures produce signals vs. skip,
    // and the composite report must never panic.
    let fixtures = [
        (CLEAN_SOURCE, /*expect_signal*/ true),
        (UNSAFE_SOURCE, true),
        (TRIVIAL_SOURCE, false),
        ("pub async fn fetch() -> u32 { 0 }", true),
    ];
    for (src, expect_signal) in fixtures {
        let report = CodeGenWorkflow::analyze_no_format(src);
        let (hint, reward) = rust_workflow_advisory("fixture.rs", Some(src));

        if expect_signal {
            assert!(
                report.is_ok(),
                "analyze_no_format must succeed for signal-producing fixture: {src:?}"
            );
            assert!(hint.is_some(), "hint missing for {src:?}");
            assert!(reward.is_some(), "reward missing for {src:?}");
            let r = reward.expect("reward must be Some");
            assert!(
                (-0.10..=0.10).contains(&r),
                "reward {r} out of envelope for {src:?}"
            );
        } else {
            // Trivial fixtures parse OK but produce no V6 signal.
            assert!(
                report.is_ok(),
                "trivial source must still parse successfully"
            );
            assert_eq!(reward, None, "trivial source must skip reward for {src:?}");
        }
    }
}

// ─── Axis 7: Determinism — same input → same output across calls ───

#[test]
fn axis7_pipeline_is_deterministic_across_repeated_calls() {
    for _ in 0..5 {
        let (h1, r1) = rust_workflow_advisory("lib.rs", Some(CLEAN_SOURCE));
        let (h2, r2) = rust_workflow_advisory("lib.rs", Some(CLEAN_SOURCE));
        assert_eq!(h1, h2, "hint drifted across identical calls");
        assert_eq!(r1, r2, "reward drifted across identical calls");
    }
}
