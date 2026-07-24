//! Snapshot tests for [`WiringAuditFull::build_recommendations`].
//!
//! Rationale: the `touring_wiring_audit` MCP tool surfaces the
//! recommendations list directly to the model — every wording drift
//! changes how Claude reacts to wiring health. Locking the exact
//! recommendation strings via YAML snapshots ensures we never
//! accidentally rephrase a recommendation such that an LLM consumer
//! learns a stale pattern.
//!
//! Review snapshots with: `cargo insta review -p touring-server`.

use touring_server::tools::wiring_audit::{ModuleScore, WiringAuditFull};

/// Build a ModuleScore with deterministic fields for snapshot fixtures.
fn module(path: &str, integration: f64, pub_syms: i64, orphans: i64) -> ModuleScore {
    ModuleScore {
        file_path: path.to_string(),
        integration_score: integration,
        total_pub_symbols: pub_syms,
        orphan_count: orphans,
    }
}

#[test]
fn snapshot_clean_codebase_yields_single_positive_rec() {
    // INVARIANT: when orphan_count == 0, the function must short-circuit
    // with exactly ONE positive recommendation. If that string changes
    // the LLM's "codebase is healthy" signal moves — guard it.
    let recs = WiringAuditFull::build_recommendations(0, &[]);
    insta::assert_yaml_snapshot!("clean_codebase_single_rec", recs);
}

#[test]
fn snapshot_orphan_modules_only() {
    // Two modules with orphans but all integration_score >= 0.5 →
    // only the "orphan modules" recommendation fires, not the
    // "low integration" one. Locks the branch ordering.
    let modules = vec![module("src/a.rs", 0.9, 5, 2), module("src/b.rs", 0.8, 3, 1)];
    let recs = WiringAuditFull::build_recommendations(3, &modules);
    insta::assert_yaml_snapshot!("orphan_modules_only", recs);
}

#[test]
fn snapshot_low_integration_only() {
    // Modules with orphan_count=0 but integration_score<0.5 → only the
    // "low integration" recommendation fires. We still pass a non-zero
    // orphan_count to the top-level counter so the short-circuit does
    // NOT trigger — tests the cross-branch case where summary metrics
    // disagree with per-module state (possible after stale reindex).
    let modules = vec![
        module("src/isolated.rs", 0.30, 4, 0),
        module("src/coupled.rs", 0.45, 2, 0),
    ];
    let recs = WiringAuditFull::build_recommendations(1, &modules);
    insta::assert_yaml_snapshot!("low_integration_only", recs);
}

#[test]
fn snapshot_both_recommendations_fire() {
    // Mixed case: same module set trips both the orphan filter AND the
    // low-integration filter. The snapshot pins the ORDER of the two
    // recommendation strings — downstream logic may rely on it.
    let modules = vec![
        module("src/weak.rs", 0.20, 6, 4),
        module("src/ok.rs", 0.85, 10, 1),
    ];
    let recs = WiringAuditFull::build_recommendations(5, &modules);
    insta::assert_yaml_snapshot!("both_recommendations_fire", recs);
}
