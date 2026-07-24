//! E2E tests for VGP Layer 7 — Seven-Layer Validation Pipeline.
//!
//! Verifies the full 7-layer `validate_plan` pipeline in integration with
//! real `GeneratorPlan` objects and all layer transitions.
//!
//! D6.6: 21 unit tests + 7 E2E tests
//! - 17 unit tests live in `src/validate/pipeline.rs::mod tests`
//! - 7 E2E tests live here in `tests/seven_layer_e2e.rs`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use touring_generator::NormalizedScore;
use touring_generator::generator::kinds::GeneratorKind;
use touring_generator::plan::contracts::{
    BoundaryEnforcement, Contracts, PathBoundaries, TaskKind,
};
use touring_generator::plan::schema::{
    Assembly, CapacityHints, CilaLevel, CommitPolicy, LearningDirectives, PlanMetadata,
    RollbackPolicy, Target, TemplateSelection, ValidationDirectives,
};
use touring_generator::validate::pipeline::ValidationContext;
use touring_generator::validate::pipeline::validate_plan;

// ── Test Plan Factory ─────────────────────────────────────────────────────────

fn make_plan(
    kind: GeneratorKind,
    target_path: &str,
) -> touring_generator::plan::schema::GeneratorPlan {
    touring_generator::plan::schema::GeneratorPlan {
        version: "2.0".to_string(),
        plan_id: uuid::Uuid::new_v4(),
        intent: "e2e validation test plan".to_string(),
        cila_level: CilaLevel::L3,
        target: Target {
            file_path: target_path.to_string(),
            module_path: None,
            crate_name: None,
        },
        kind,
        contracts: Contracts::default(),
        template: TemplateSelection::default(),
        assembly: Assembly::default(),
        validation: ValidationDirectives::default(),
        commit_policy: CommitPolicy::default(),
        rollback: RollbackPolicy::default(),
        learning: LearningDirectives::default(),
        spec_inputs: None,
        capacity_hints: CapacityHints::default(),
        execution_trace: vec![],
        metadata: PlanMetadata::default(),
    }
}

fn impl_contracts() -> Contracts {
    Contracts {
        path_boundaries: Some(PathBoundaries {
            task_kind: TaskKind::Impl,
            read: vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
            write: vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
            forbidden_write: vec!["docs/**".into()],
            enforcement: BoundaryEnforcement::FailClosed,
        }),
        ..Default::default()
    }
}

fn spec_contracts() -> Contracts {
    Contracts {
        path_boundaries: Some(PathBoundaries {
            task_kind: TaskKind::Spec,
            read: vec!["docs/".into(), "spec/".into(), "*.md".into()],
            write: vec![
                "docs/".into(),
                "docs/**".into(),
                "spec/".into(),
                "spec/**".into(),
                "*.md".into(),
            ],
            forbidden_write: vec![
                "crates/**".into(),
                "src/**".into(),
                "tests/**".into(),
                "benches/**".into(),
            ],
            enforcement: BoundaryEnforcement::FailClosed,
        }),
        ..Default::default()
    }
}

// ── E2E: Full pipeline passes with valid plan ─────────────────────────────────

#[test]
fn e2e_all_layers_pass_rust_module() {
    let plan = make_plan(GeneratorKind::RustModule, "crates/touring-foo/src/lib.rs");
    let ctx = ValidationContext::new();
    let report = validate_plan(&plan, &ctx);
    assert_eq!(
        report.layers_passed, 7,
        "all 7 layers should pass with valid empty context"
    );
    assert!(report.all_passed, "report.all_passed should be true");
    assert_eq!(report.layer_results.len(), 7);
    for r in &report.layer_results {
        assert!(r.passed, "layer {} should pass", r.name);
    }
}

// ── E2E: L3 VocabularyAllowed gates kind ───────────────────────────────────────

#[test]
fn e2e_l3_blocks_unknown_kind() {
    let plan = make_plan(GeneratorKind::PythonScript, "script.py");
    let ctx = ValidationContext::new().with_allowed_kinds(vec!["RustModule".into()]);
    let report = validate_plan(&plan, &ctx);
    assert!(!report.all_passed, "unknown kind should cause failure");
    let l3_results: Vec<_> = report
        .layer_results
        .iter()
        .filter(|r| r.name.contains("vocabulary"))
        .collect();
    assert_eq!(l3_results.len(), 1, "exactly one L3 result expected");
    assert!(!l3_results[0].passed, "L3 should fail for unknown kind");
    assert_eq!(report.layers_passed, 6, "L3 is the only failure");
}

#[test]
fn e2e_l3_allows_matching_kind() {
    let plan = make_plan(GeneratorKind::RustModule, "src/lib.rs");
    let ctx = ValidationContext::new().with_allowed_kinds(vec!["RustModule".into()]);
    let report = validate_plan(&plan, &ctx);
    assert!(report.all_passed, "matching kind should pass L3");
}

// ── E2E: L5 PathBoundary blocks / allows writes ───────────────────────────────

#[test]
fn e2e_l5_impl_blocks_docs_write() {
    let plan = make_plan(GeneratorKind::RustModule, "docs/changes.md");
    let ctx = ValidationContext::new().with_contracts(impl_contracts());
    let report = validate_plan(&plan, &ctx);
    assert!(
        !report.all_passed,
        "impl writing to docs/ should be blocked by L5"
    );
    let l5_results: Vec<_> = report
        .layer_results
        .iter()
        .filter(|r| r.name.contains("path_boundary"))
        .collect();
    assert_eq!(l5_results.len(), 1);
    assert!(!l5_results[0].passed, "L5 should fail for impl→docs write");
}

#[test]
fn e2e_l5_spec_allows_docs_write() {
    let plan = make_plan(GeneratorKind::RustModule, "docs/spec.md");
    let ctx = ValidationContext::new().with_contracts(spec_contracts());
    let report = validate_plan(&plan, &ctx);
    assert!(report.all_passed, "spec writing to docs/ should pass L5");
}

// ── E2E: L6 Immutability detects committed paths ─────────────────────────────

#[test]
fn e2e_l6_detects_committed_path() {
    let plan = make_plan(GeneratorKind::RustModule, "src/lib.rs");
    let mut ctx = ValidationContext::new();
    ctx.committed_history
        .committed_paths
        .push("src/lib.rs".to_string());
    let report = validate_plan(&plan, &ctx);
    assert!(!report.all_passed, "committed path should fail L6");
    let l6_results: Vec<_> = report
        .layer_results
        .iter()
        .filter(|r| r.name.contains("immutability"))
        .collect();
    assert_eq!(l6_results.len(), 1);
    assert!(!l6_results[0].passed, "L6 should fail for committed path");
}

#[test]
fn e2e_l6_allows_new_path() {
    let plan = make_plan(GeneratorKind::RustModule, "src/new_file.rs");
    let ctx = ValidationContext::new();
    let report = validate_plan(&plan, &ctx);
    assert!(report.all_passed, "new path should pass L6");
}

// ── E2E: L7 VerificationGate composite health ─────────────────────────────────

#[test]
fn e2e_l7_fails_below_085() {
    let plan = make_plan(GeneratorKind::RustModule, "src/lib.rs");
    let ctx = ValidationContext::new().with_composite_health(0.70);
    let report = validate_plan(&plan, &ctx);
    assert!(!report.all_passed, "health 0.70 should fail L7");
    let l7_results: Vec<_> = report
        .layer_results
        .iter()
        .filter(|r| r.name.contains("verification_gate"))
        .collect();
    assert_eq!(l7_results.len(), 1);
    assert!(!l7_results[0].passed, "L7 should fail below 0.85");
}

#[test]
fn e2e_l7_passes_at_085_exactly() {
    let plan = make_plan(GeneratorKind::RustModule, "src/lib.rs");
    let ctx = ValidationContext::new().with_composite_health(0.85);
    let report = validate_plan(&plan, &ctx);
    assert!(report.all_passed, "health 0.85 should pass L7 exactly");
}

#[test]
fn e2e_l7_passes_above_085() {
    let plan = make_plan(GeneratorKind::RustModule, "src/lib.rs");
    let ctx = ValidationContext::new().with_composite_health(0.95);
    let report = validate_plan(&plan, &ctx);
    assert!(report.all_passed, "health 0.95 should pass L7");
}

// ── E2E: LayerResult score + passed semantics ─────────────────────────────────

#[test]
fn e2e_passed_layer_has_score_one() {
    let plan = make_plan(GeneratorKind::RustModule, "crates/foo/src/lib.rs");
    let ctx = ValidationContext::new();
    let report = validate_plan(&plan, &ctx);
    for r in &report.layer_results {
        assert_eq!(
            r.score,
            NormalizedScore::ONE,
            "passed layer {} should have score 1.0",
            r.name
        );
    }
}

#[test]
fn e2e_failed_layer_has_score_zero() {
    let plan = make_plan(GeneratorKind::PythonScript, "script.py");
    let ctx = ValidationContext::new().with_allowed_kinds(vec!["RustModule".into()]);
    let report = validate_plan(&plan, &ctx);
    let l3_results: Vec<_> = report
        .layer_results
        .iter()
        .filter(|r| r.name.contains("vocabulary"))
        .collect();
    assert!(!l3_results[0].passed);
    assert_eq!(l3_results[0].score, NormalizedScore::ZERO);
}

#[test]
fn e2e_layer_result_order_preserved() {
    let plan = make_plan(GeneratorKind::RustModule, "src/lib.rs");
    let ctx = ValidationContext::new();
    let report = validate_plan(&plan, &ctx);
    let names: Vec<_> = report
        .layer_results
        .iter()
        .map(|r| r.name.clone())
        .collect();
    let expected = vec![
        "l1_json_parse",
        "l2_schema",
        "l3_vocabulary",
        "l4_state_machine",
        "l5_path_boundary",
        "l6_immutability",
        "l7_verification_gate",
    ];
    assert_eq!(names, expected, "layer results must be in order L1→L7");
}

// ── E2E: layer_durations_ms populated ────────────────────────────────────────

#[test]
fn e2e_layer_durations_ms_populated() {
    let plan = make_plan(GeneratorKind::RustModule, "src/lib.rs");
    let ctx = ValidationContext::new();
    let report = validate_plan(&plan, &ctx);
    assert_eq!(
        report.layer_durations_ms.len(),
        7,
        "all 7 layers should report duration"
    );
    for name in &[
        "l1_json_parse",
        "l2_schema",
        "l3_vocabulary",
        "l4_state_machine",
        "l5_path_boundary",
        "l6_immutability",
        "l7_verification_gate",
    ] {
        assert!(
            report.layer_durations_ms.contains_key(*name),
            "duration for {name} should be present"
        );
    }
}
