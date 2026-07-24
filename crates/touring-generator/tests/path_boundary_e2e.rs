//! E2E tests for VGP Layer 5 — Path Boundary Enforcement.
//!
//! Tests the boundary validation layer in the full speculate pipeline:
//! - Impl allows crates/** write
//! - Spec blocks crates/** write (`FailClosed`)
//! - Audit warns on crates/** write (`WarnOnly`)
//! - `WarnOnly` does not block violations
//! - `FailClosed` blocks violations

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use touring_generator::NormalizedScore;
use touring_generator::plan::contracts::{BoundaryEnforcement, PathBoundaries, TaskKind};
use touring_generator::plan::result::FileAction;
use touring_generator::validate::boundary::{BoundaryResult, BoundaryValidator, default_boundary};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_rendered(path: &str) -> touring_generator::plan::result::RenderedFile {
    touring_generator::plan::result::RenderedFile::new(path, "// content", FileAction::Created)
}

fn impl_boundaries() -> PathBoundaries {
    default_boundary(&TaskKind::Impl)
}

fn spec_boundaries() -> PathBoundaries {
    let mut b = default_boundary(&TaskKind::Spec);
    b.enforcement = BoundaryEnforcement::FailClosed;
    b
}

fn audit_boundaries() -> PathBoundaries {
    default_boundary(&TaskKind::Audit)
}

fn warn_only_boundaries() -> PathBoundaries {
    let mut b = default_boundary(&TaskKind::Impl);
    b.enforcement = BoundaryEnforcement::WarnOnly;
    b
}

fn fail_closed_boundaries() -> PathBoundaries {
    let mut b = default_boundary(&TaskKind::Spec);
    b.enforcement = BoundaryEnforcement::FailClosed;
    b
}

// ── E2E: Impl allows crates write ───────────────────────────────────────────────

#[test]
fn e2e_impl_allows_crate_file() {
    let bv = BoundaryValidator::new(&impl_boundaries()).unwrap();
    let artifacts = &[make_rendered("crates/touring-foo/src/lib.rs")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Valid),
        "impl should allow crates/** write"
    );
}

#[test]
fn e2e_impl_allows_src_file() {
    let bv = BoundaryValidator::new(&impl_boundaries()).unwrap();
    let artifacts = &[make_rendered("src/main.rs")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Valid),
        "impl should allow src/** write"
    );
}

#[test]
fn e2e_impl_allows_test_file() {
    let bv = BoundaryValidator::new(&impl_boundaries()).unwrap();
    let artifacts = &[make_rendered("tests/integration_test.rs")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Valid),
        "impl should allow tests/** write"
    );
}

// ── E2E: Spec blocks crates write ─────────────────────────────────────────────

#[test]
fn e2e_spec_blocks_crate_file() {
    let bv = BoundaryValidator::new(&spec_boundaries()).unwrap();
    let artifacts = &[make_rendered("crates/foo/src/lib.rs")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Violations(_)),
        "spec should block crates/** write"
    );
}

#[test]
fn e2e_spec_blocks_src_file() {
    let bv = BoundaryValidator::new(&spec_boundaries()).unwrap();
    let artifacts = &[make_rendered("src/lib.rs")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Violations(_)),
        "spec should block src/** write"
    );
}

// ── E2E: Audit warns on forbidden write ────────────────────────────────────────

#[test]
fn e2e_audit_warns_on_crate_write() {
    let bv = BoundaryValidator::new(&audit_boundaries()).unwrap();
    let artifacts = &[make_rendered("crates/foo/src/lib.rs")];
    let result = bv.validate_artifacts(artifacts);
    // Audit uses WarnOnly, so ForbiddenWrite in crates/** produces Warnings
    assert!(
        matches!(result, BoundaryResult::Warnings(_)),
        "audit should warn, not block, on crates/** write"
    );
}

#[test]
fn e2e_audit_allows_docs_write() {
    let bv = BoundaryValidator::new(&audit_boundaries()).unwrap();
    let artifacts = &[make_rendered("docs/audit/report.md")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Valid),
        "audit should allow docs/audit/** write"
    );
}

// ── E2E: WarnOnly does not block ───────────────────────────────────────────────

#[test]
fn e2e_warn_only_allows_code_write() {
    let bv = BoundaryValidator::new(&warn_only_boundaries()).unwrap();
    // WarnOnly lets violations through as warnings but still records them.
    // crates/** is in Impl write allowlist so this is valid (not a violation).
    let artifacts = &[make_rendered("crates/touring-foo/src/lib.rs")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Valid),
        "warn_only should allow valid code write"
    );
}

#[test]
fn e2e_warn_only_warns_not_violations() {
    let bv = BoundaryValidator::new(&warn_only_boundaries()).unwrap();
    // docs/** is not in impl write allowlist, should produce warning
    let artifacts = &[make_rendered("docs/changes.md")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Warnings(_)),
        "warn_only should warn on out-of-scope write"
    );
}

// ── E2E: FailClosed blocks violations ─────────────────────────────────────────

#[test]
fn e2e_fail_closed_blocks_violation() {
    let bv = BoundaryValidator::new(&fail_closed_boundaries()).unwrap();
    let artifacts = &[make_rendered("src/lib.rs")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Violations(_)),
        "fail_closed should block src/** write for Spec"
    );
}

#[test]
fn e2e_fail_closed_passes_valid() {
    let bv = BoundaryValidator::new(&fail_closed_boundaries()).unwrap();
    let artifacts = &[make_rendered("docs/spec.md")];
    let result = bv.validate_artifacts(artifacts);
    assert!(
        matches!(result, BoundaryResult::Valid),
        "fail_closed should allow valid docs write"
    );
}

// ── E2E: LayerResult conversion ────────────────────────────────────────────────

#[test]
fn e2e_layer_result_from_valid() {
    let started = std::time::Instant::now();
    let layer: touring_generator::plan::result::LayerResult =
        (BoundaryResult::Valid, started).into();
    assert_eq!(layer.name, "l5_path_boundary");
    assert_eq!(layer.score, NormalizedScore::ONE);
    assert!(layer.passed);
    assert!(layer.issues.is_empty());
}

#[test]
fn e2e_layer_result_from_warnings() {
    let started = std::time::Instant::now();
    let warnings = vec![touring_generator::validate::boundary::BoundaryViolation {
        file_path: "docs/extra.md".into(),
        task_kind: TaskKind::Impl,
        violation_kind: touring_generator::validate::boundary::ViolationKind::NotAllowedWrite,
        matched_pattern: String::new(),
    }];
    let layer: touring_generator::plan::result::LayerResult =
        (BoundaryResult::Warnings(warnings), started).into();
    assert_eq!(layer.name, "l5_path_boundary");
    assert_eq!(layer.score, NormalizedScore::ONE);
    assert!(layer.passed); // warnings still pass
    assert!(!layer.issues.is_empty());
}

#[test]
fn e2e_layer_result_from_violations() {
    let started = std::time::Instant::now();
    let violations = vec![touring_generator::validate::boundary::BoundaryViolation {
        file_path: "crates/foo/src/lib.rs".into(),
        task_kind: TaskKind::Spec,
        violation_kind: touring_generator::validate::boundary::ViolationKind::ForbiddenWrite,
        matched_pattern: String::new(),
    }];
    let layer: touring_generator::plan::result::LayerResult =
        (BoundaryResult::Violations(violations), started).into();
    assert_eq!(layer.name, "l5_path_boundary");
    assert_eq!(layer.score, NormalizedScore::ZERO);
    assert!(!layer.passed);
    assert!(!layer.issues.is_empty());
}

// ── E2E: Multi-artifact ─────────────────────────────────────────────────────────

#[test]
fn e2e_multiple_artifacts_mixed_results() {
    let bv = BoundaryValidator::new(&impl_boundaries()).unwrap();
    let artifacts = &[
        make_rendered("crates/foo/src/lib.rs"),
        make_rendered("crates/bar/src/main.rs"),
        make_rendered("src/utils.rs"),
    ];
    let result = bv.validate_artifacts(artifacts);
    // All are valid for Impl
    assert!(matches!(result, BoundaryResult::Valid));
}

#[test]
fn e2e_multiple_artifacts_first_violation() {
    let bv = BoundaryValidator::new(&spec_boundaries()).unwrap();
    let artifacts = &[
        make_rendered("crates/foo/src/lib.rs"), // violation
        make_rendered("docs/spec.md"),          // valid
    ];
    let result = bv.validate_artifacts(artifacts);
    assert!(matches!(result, BoundaryResult::Violations(_)));
}
