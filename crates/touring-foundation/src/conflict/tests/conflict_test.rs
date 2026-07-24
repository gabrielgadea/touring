//! Integration tests for conflict detection module.

use touring_foundation::conflict::{
    detect_conflict, ConflictReport, ConflictTier,
};

#[test]
fn test_detect_conflict_ast_diff_identical() {
    let r = detect_conflict("fn foo() {}", "fn foo() {}", ConflictTier::AstDiff);
    assert!(!r.has_conflict);
    assert_eq!(r.tier, ConflictTier::AstDiff);
    assert!(r.sla_violations.is_empty());
}

#[test]
fn test_detect_conflict_ast_diff_conflict() {
    let r = detect_conflict("fn foo() {}", "fn bar() {}", ConflictTier::AstDiff);
    assert!(r.has_conflict);
    assert!(r.severity > 0.0);
}

#[test]
fn test_detect_conflict_semantic_identical() {
    let r = detect_conflict("fn foo() {}", "fn foo() {}", ConflictTier::Semantic);
    assert!(!r.has_conflict);
    assert_eq!(r.tier, ConflictTier::Semantic);
}

#[test]
fn test_detect_conflict_semantic_signature_conflict() {
    let r = detect_conflict(
        "fn foo(a: i32) -> i32 {}",
        "fn foo(a: String) -> i32 {}",
        ConflictTier::Semantic,
    );
    assert!(r.has_conflict);
}

#[test]
fn test_detect_conflict_graph_impact_identical() {
    let r = detect_conflict("pub fn foo() {}", "pub fn foo() {}", ConflictTier::GraphImpact);
    assert!(!r.has_conflict);
    assert_eq!(r.tier, ConflictTier::GraphImpact);
}

#[test]
fn test_detect_conflict_graph_impact_public_api_change() {
    let r = detect_conflict(
        "pub fn foo() {}",
        "pub fn foo() {}\npub fn bar() {}",
        ConflictTier::GraphImpact,
    );
    assert!(r.has_conflict);
    assert_eq!(r.severity, 1.0);
}

#[test]
fn test_detect_conflict_all_tiers() {
    for tier in [ConflictTier::AstDiff, ConflictTier::Semantic, ConflictTier::GraphImpact] {
        let r = detect_conflict("fn foo() {}", "fn foo() {}", tier);
        assert!(!r.has_conflict, "tier {tier:?} should have no conflict");
        assert_eq!(r.tier, tier);
        assert!(!r.description.is_empty());
    }
}

#[test]
fn test_conflict_tier_sla_values() {
    assert_eq!(ConflictTier::AstDiff.sla_spec().p99_ms, 100);
    assert_eq!(ConflictTier::Semantic.sla_spec().p99_ms, 1_000);
    assert_eq!(ConflictTier::GraphImpact.sla_spec().p99_ms, 5_000);
}

#[test]
fn test_conflict_report_serialization() {
    let r = detect_conflict("fn foo() {}", "fn bar() {}", ConflictTier::AstDiff);
    let json = serde_json::to_string(&r).expect("should serialize");
    assert!(json.contains("has_conflict"));
    assert!(json.contains("\"tier\":"));
}

#[test]
fn test_multiline_input() {
    let a = "fn foo()\n  let x = 1;\n  x\nfn bar()";
    let b = "fn foo()\n  let x = 2;\n  x\nfn baz()";

    let r = detect_conflict(a, b, ConflictTier::Semantic);
    assert!(r.has_conflict);
    assert!(!r.locations.is_empty() || a == b); // Diff detected or identical
}

#[test]
fn test_empty_input() {
    let r = detect_conflict("", "", ConflictTier::AstDiff);
    assert!(!r.has_conflict); // Empty = identical
}

#[test]
fn test_sla_violation_tracking() {
    // Large inputs should still track SLA violations even if fast
    let r = detect_conflict(
        "// line1\n".repeat(100).as_str(),
        "// line2\n".repeat(100).as_str(),
        ConflictTier::AstDiff,
    );
    assert!(r.latency_ms > 0);
    // SLA should be tracked (may or may not be violated)
    assert!(r.sla.p99_ms > 0);
}