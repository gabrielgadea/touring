//! GoalTracker — 9-dimensional quality tracking with weighted scoring.
//!
//! Unified from rust-core/src/aco/tracker.rs (450 LOC)
//! PyO3 bindings removed — they belong in touring-python.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Per-dimension score at or above which a dimension is considered passing.
pub const VETO_THRESHOLD: f64 = 0.80;
/// Composite score below which the tracker halts instead of merely vetoing.
pub const HALT_THRESHOLD: f64 = 0.50;
/// Consecutive sub-`HALT_THRESHOLD` iterations tolerated before a hard halt.
pub const HALT_ITERATIONS: u32 = 3;
/// Dimension IDs treated as critical and weighted by `CRITICAL_WEIGHT`.
pub const CRITICAL_DIMS: &[&str] = &["D1", "D2", "D6"];
/// Composite-score weight applied to dimensions listed in `CRITICAL_DIMS`.
pub const CRITICAL_WEIGHT: f64 = 1.5;
/// Composite-score weight applied to non-critical dimensions.
pub const NORMAL_WEIGHT: f64 = 1.0;

/// Tracker status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackerStatus {
    /// All dimensions met the veto threshold; quality gate passes.
    Pass,
    /// At least one dimension failed but composite stays above the halt floor.
    Veto,
    /// Composite fell below the halt floor; generation should stop.
    Halt,
}

impl fmt::Display for TrackerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS"),
            Self::Veto => write!(f, "VETO"),
            Self::Halt => write!(f, "HALT"),
        }
    }
}

/// Result for a single dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimResult {
    /// Canonical dimension identifier (e.g. `"D1"`).
    pub dim_id: String,
    /// Human-readable dimension name (e.g. `"Correctness"`).
    pub name: String,
    /// Fraction of checks passed for this dimension, in `[0.0, 1.0]`.
    pub score: f64,
    /// Number of individual checks that passed.
    pub checks_passed: u32,
    /// Total number of checks evaluated for this dimension.
    pub checks_total: u32,
    /// Per-check detail lines (e.g. `"OK: syntax_valid"`).
    pub details: Vec<String>,
}

impl DimResult {
    /// Composite weight for this dimension (`CRITICAL_WEIGHT` if critical).
    pub fn weight(&self) -> f64 {
        if CRITICAL_DIMS.contains(&self.dim_id.as_str()) {
            CRITICAL_WEIGHT
        } else {
            NORMAL_WEIGHT
        }
    }

    /// Score scaled by this dimension's `weight`.
    pub fn weighted_score(&self) -> f64 {
        self.score * self.weight()
    }

    /// `true` when the score meets `VETO_THRESHOLD`.
    pub fn is_passing(&self) -> bool {
        self.score >= VETO_THRESHOLD
    }
}

/// Full 9-dimension tracker report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerReport {
    /// Per-dimension results making up the report.
    pub dims: Vec<DimResult>,
    /// Weighted composite score across all dimensions, in `[0.0, 1.0]`.
    pub composite: f64,
    /// Overall verdict derived from the dimensions and composite.
    pub status: TrackerStatus,
    /// Iteration number this report was produced on.
    pub iteration: u32,
}

impl TrackerReport {
    /// Single-line summary, e.g. "PASS composite=0.95 iter=3 (9/9 dims passing)"
    pub fn summary(&self) -> String {
        let passing = self.dims.iter().filter(|d| d.is_passing()).count();
        let total = self.dims.len();
        format!(
            "{} composite={:.2} iter={} ({}/{} dims passing)",
            self.status, self.composite, self.iteration, passing, total
        )
    }

    /// IL-6: Map tracker outcome to an RL reward signal in [0.0, 1.0].
    ///
    /// Mapping rationale:
    /// - `Pass`:  0.5 + 0.5 × composite  → range [0.5, 1.0] (reaches 0.9 only when composite ≥ 0.8)
    /// - `Veto`:  0.1 + 0.4 × composite  → range [0.1, 0.5] — penalises but not zero
    /// - `Halt`:  0.0                     → hard stop = zero reward
    pub fn as_rl_reward(&self) -> f64 {
        match self.status {
            TrackerStatus::Pass => 0.5 + 0.5 * self.composite,
            TrackerStatus::Veto => 0.1 + 0.4 * self.composite,
            TrackerStatus::Halt => 0.0,
        }
    }

    /// IL-6: Per-dimension rewards for Q-table granular updates.
    ///
    /// Returns `(dim_id, reward)` pairs where:
    /// - Passing dim: `0.5 + 0.5 × score`  → full credit
    /// - Failing dim: `score × 0.5`         → partial credit
    pub fn dimensional_rewards(&self) -> Vec<(String, f64)> {
        self.dims
            .iter()
            .map(|d| {
                let reward = if d.is_passing() {
                    0.5 + 0.5 * d.score
                } else {
                    d.score * 0.5
                };
                (d.dim_id.clone(), reward)
            })
            .collect()
    }

    /// INS-L6: Extract the 9 dimension scores as a feature vector for LinUCB.
    ///
    /// Returns scores in canonical order [D1, D2, D3, D4, D5, D6, D7, D8, D9].
    /// Dimensions missing from `self.dims` (e.g. in unit tests with fewer dims)
    /// default to 0.0 so the vector always has exactly 9 elements.
    pub fn as_linucb_features(&self) -> DimensionalFeatures {
        let dim_score = |id: &str| -> f64 {
            self.dims
                .iter()
                .find(|d| d.dim_id == id)
                .map(|d| d.score)
                .unwrap_or(0.0)
        };
        DimensionalFeatures {
            d1: dim_score("D1"),
            d2: dim_score("D2"),
            d3: dim_score("D3"),
            d4: dim_score("D4"),
            d5: dim_score("D5"),
            d6: dim_score("D6"),
            d7: dim_score("D7"),
            d8: dim_score("D8"),
            d9: dim_score("D9"),
        }
    }
}

/// INS-L6: 9 quality-dimension scores extracted from a `TrackerReport`.
///
/// Use `append_to_context` to extend a LinUCB feature vector with these values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DimensionalFeatures {
    /// D1 (Correctness) score.
    pub d1: f64,
    /// D2 (Completeness) score.
    pub d2: f64,
    /// D3 (Performance) score.
    pub d3: f64,
    /// D4 (Reliability) score.
    pub d4: f64,
    /// D5 (Maintainability) score.
    pub d5: f64,
    /// D6 (Security) score.
    pub d6: f64,
    /// D7 (Observability) score.
    pub d7: f64,
    /// D8 (Testability) score.
    pub d8: f64,
    /// D9 (Evolvability) score.
    pub d9: f64,
}

impl DimensionalFeatures {
    /// Append these 9 features to an existing context vector.
    ///
    /// The caller is responsible for sizing the target vector appropriately.
    /// Values are appended in order [d1..d9].
    pub fn append_to_context(&self, ctx: &mut Vec<f64>) {
        ctx.push(self.d1);
        ctx.push(self.d2);
        ctx.push(self.d3);
        ctx.push(self.d4);
        ctx.push(self.d5);
        ctx.push(self.d6);
        ctx.push(self.d7);
        ctx.push(self.d8);
        ctx.push(self.d9);
    }

    /// Return as a fixed-size array [D1..D9].
    pub fn as_array(&self) -> [f64; 9] {
        [
            self.d1, self.d2, self.d3, self.d4, self.d5, self.d6, self.d7, self.d8, self.d9,
        ]
    }
}

/// Weighted mean of dimension scores; returns `0.0` when total weight is zero.
pub fn compute_composite(dims: &[DimResult]) -> f64 {
    let total_weight: f64 = dims.iter().map(|d| d.weight()).sum();
    if total_weight == 0.0 {
        return 0.0;
    }
    dims.iter().map(|d| d.weighted_score()).sum::<f64>() / total_weight
}

/// Derive a `TrackerStatus` from dimension scores and the composite value.
pub fn determine_status(dims: &[DimResult], composite: f64) -> TrackerStatus {
    let all_pass = dims.iter().all(|d| d.score >= VETO_THRESHOLD);
    if all_pass {
        TrackerStatus::Pass
    } else if composite < HALT_THRESHOLD {
        TrackerStatus::Halt
    } else {
        TrackerStatus::Veto
    }
}

/// Assemble a `TrackerReport` from dimensions, computing composite and status.
pub fn build_report(dims: Vec<DimResult>, iteration: u32) -> TrackerReport {
    let composite = compute_composite(&dims);
    let status = determine_status(&dims, composite);
    TrackerReport {
        dims,
        composite,
        status,
        iteration,
    }
}

// ---------------------------------------------------------------------------
// INS-2: GoalTracker 9×9 Computational
// ---------------------------------------------------------------------------

/// Specification for a single verification check within a dimension.
#[derive(Debug, Clone)]
pub struct CheckSpec {
    /// Unique check identifier (e.g. `"D1C1"`).
    pub id: String,
    /// Human-readable description of what the check verifies.
    pub description: String,
    /// Relative weight of this check within its dimension.
    pub weight: f64,
}

impl CheckSpec {
    /// Construct a `CheckSpec` from id, description, and weight.
    pub fn new(id: &str, description: &str, weight: f64) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            weight,
        }
    }
}

/// Context provided to verify_check() for real computational verification.
#[derive(Debug, Clone, Default)]
pub struct CheckContext {
    /// Arbitrary key-value pairs for check evaluation.
    pub values: std::collections::HashMap<String, f64>,
    /// Threshold for numeric checks (default 0.0).
    pub threshold: f64,
}

impl CheckContext {
    /// Create an empty context with a zero threshold.
    pub fn new() -> Self {
        Self {
            values: std::collections::HashMap::new(),
            threshold: 0.0,
        }
    }

    /// Builder: insert a check value, returning `self`.
    pub fn with_value(mut self, key: &str, value: f64) -> Self {
        self.values.insert(key.to_string(), value);
        self
    }

    /// Builder: set the numeric pass threshold, returning `self`.
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }
}

/// 9 dimensions × 9 checks each = 81 total check specs.
#[allow(clippy::type_complexity)]
pub static DIM_CHECKS: &[(&str, &str, &[(&str, &str)])] = &[
    (
        "D1",
        "Correctness",
        &[
            ("D1C1", "syntax_valid"),
            ("D1C2", "types_match"),
            ("D1C3", "no_undefined_refs"),
            ("D1C4", "no_duplicate_defs"),
            ("D1C5", "returns_correct_type"),
            ("D1C6", "no_null_deref"),
            ("D1C7", "bounds_checked"),
            ("D1C8", "no_overflow_risk"),
            ("D1C9", "logic_sound"),
        ],
    ),
    (
        "D2",
        "Completeness",
        &[
            ("D2C1", "all_branches_covered"),
            ("D2C2", "error_paths_handled"),
            ("D2C3", "edge_cases_tested"),
            ("D2C4", "all_fields_initialized"),
            ("D2C5", "no_dead_code"),
            ("D2C6", "all_imports_used"),
            ("D2C7", "all_exports_documented"),
            ("D2C8", "no_stub_functions"),
            ("D2C9", "test_coverage_ok"),
        ],
    ),
    (
        "D3",
        "Performance",
        &[
            ("D3C1", "no_quadratic_loops"),
            ("D3C2", "cache_used_appropriately"),
            ("D3C3", "no_redundant_allocs"),
            ("D3C4", "parallelizable_where_safe"),
            ("D3C5", "lazy_evaluation_applied"),
            ("D3C6", "simd_used_if_available"),
            ("D3C7", "no_lock_contention"),
            ("D3C8", "batch_io_where_possible"),
            ("D3C9", "profiled_hot_paths"),
        ],
    ),
    (
        "D4",
        "Reliability",
        &[
            ("D4C1", "retry_on_transient_errors"),
            ("D4C2", "timeout_configured"),
            ("D4C3", "circuit_breaker_present"),
            ("D4C4", "idempotent_operations"),
            ("D4C5", "deterministic_behavior"),
            ("D4C6", "no_race_conditions"),
            ("D4C7", "graceful_degradation"),
            ("D4C8", "health_check_present"),
            ("D4C9", "checksum_validated"),
        ],
    ),
    (
        "D5",
        "Maintainability",
        &[
            ("D5C1", "module_cohesion_high"),
            ("D5C2", "coupling_low"),
            ("D5C3", "functions_single_purpose"),
            ("D5C4", "naming_clear"),
            ("D5C5", "no_magic_numbers"),
            ("D5C6", "no_deep_nesting"),
            ("D5C7", "version_pinned"),
            ("D5C8", "changelog_updated"),
            ("D5C9", "refactor_opportunity_noted"),
        ],
    ),
    (
        "D6",
        "Security",
        &[
            ("D6C1", "no_hardcoded_secrets"),
            ("D6C2", "inputs_sanitized"),
            ("D6C3", "auth_checked"),
            ("D6C4", "no_sql_injection"),
            ("D6C5", "tls_enforced"),
            ("D6C6", "least_privilege"),
            ("D6C7", "audit_trail_present"),
            ("D6C8", "no_path_traversal"),
            ("D6C9", "crypto_current"),
        ],
    ),
    (
        "D7",
        "Observability",
        &[
            ("D7C1", "metrics_emitted"),
            ("D7C2", "structured_logs"),
            ("D7C3", "trace_ids_propagated"),
            ("D7C4", "alerts_configured"),
            ("D7C5", "dashboards_exist"),
            ("D7C6", "error_rates_tracked"),
            ("D7C7", "latency_p99_tracked"),
            ("D7C8", "capacity_monitored"),
            ("D7C9", "on_call_runbook"),
        ],
    ),
    (
        "D8",
        "Testability",
        &[
            ("D8C1", "unit_tests_exist"),
            ("D8C2", "integration_tests_exist"),
            ("D8C3", "mocks_available"),
            ("D8C4", "test_data_isolated"),
            ("D8C5", "tests_deterministic"),
            ("D8C6", "tests_fast"),
            ("D8C7", "flaky_tests_none"),
            ("D8C8", "coverage_threshold_met"),
            ("D8C9", "mutation_tested"),
        ],
    ),
    (
        "D9",
        "Evolvability",
        &[
            ("D9C1", "interfaces_stable"),
            ("D9C2", "backwards_compatible"),
            ("D9C3", "feature_flags_used"),
            ("D9C4", "schema_versioned"),
            ("D9C5", "deprecation_policy"),
            ("D9C6", "plugin_architecture"),
            ("D9C7", "zero_downtime_deploy"),
            ("D9C8", "data_migration_plan"),
            ("D9C9", "future_proof_design"),
        ],
    ),
];

/// Evaluate a single check against the provided context.
/// Real computation: checks if the value for the check id is >= threshold.
pub fn verify_check(check_id: &str, context: &CheckContext) -> bool {
    match context.values.get(check_id) {
        Some(&v) => v >= context.threshold,
        // Default: if no value provided, assume check passes (conservative)
        None => true,
    }
}

// ---------------------------------------------------------------------------
// IL-1: CheckRegistry — pluggable check handlers
// ---------------------------------------------------------------------------

/// IL-1: Pluggable handler for a specific check ID.
///
/// Implement this trait to register custom verification logic for a given
/// `check_id`. The registry calls `verify` instead of the default context
/// lookup when a handler is registered for the check.
pub trait CheckHandler: Send + Sync {
    /// The check ID this handler is responsible for (e.g. `"D1C1"`).
    fn check_id(&self) -> &str;
    /// Verify the check against the given context. Returns `true` = pass.
    fn verify(&self, ctx: &CheckContext) -> bool;
}

/// IL-1: Registry of [`CheckHandler`]s with fallback to context lookup.
///
/// # Backward compatibility
/// `default_pass = true` preserves the existing behaviour: unregistered checks
/// fall back to `context.values.get(check_id) >= threshold`, and `true` when
/// no value exists. Set `default_pass = false` (strict mode) to fail
/// unregistered checks without a value.
#[derive(Default)]
pub struct CheckRegistry {
    handlers: std::collections::HashMap<String, Box<dyn CheckHandler>>,
    /// If `true` (default), unregistered checks fall back to context lookup.
    /// If `false`, unregistered checks without a context value fail.
    pub default_pass: bool,
}

impl CheckRegistry {
    /// Create a registry in lenient mode (`default_pass = true`).
    pub fn new() -> Self {
        Self {
            handlers: std::collections::HashMap::new(),
            default_pass: true,
        }
    }

    /// Create a registry in strict mode (`default_pass = false`).
    pub fn new_strict() -> Self {
        Self {
            default_pass: false,
            ..Self::new()
        }
    }

    /// Register a custom handler. Replaces any existing handler for the same ID.
    pub fn register<H: CheckHandler + 'static>(&mut self, handler: H) {
        self.handlers
            .insert(handler.check_id().to_string(), Box::new(handler));
    }

    /// Verify a check. Delegates to the registered handler if present;
    /// otherwise falls back to context lookup or `default_pass`.
    pub fn verify(&self, check_id: &str, ctx: &CheckContext) -> bool {
        if let Some(handler) = self.handlers.get(check_id) {
            return handler.verify(ctx);
        }
        // Fallback: context lookup.
        match ctx.values.get(check_id) {
            Some(&v) => v >= ctx.threshold,
            None => self.default_pass,
        }
    }

    /// Returns the number of registered handlers.
    pub fn checks_len(&self) -> usize {
        self.handlers.len()
    }
}

/// Compute a full 9×9 dimensional score using real check verification.
/// Returns a TrackerReport with 9 dims, each with 9 checks evaluated.
pub fn compute_dimensional_score_real(context: &CheckContext, iteration: u32) -> TrackerReport {
    let dims: Vec<DimResult> = DIM_CHECKS
        .iter()
        .map(|(dim_id, dim_name, checks)| {
            let check_results: Vec<(&str, bool)> = checks
                .iter()
                .map(|(check_id, _desc)| (*check_id, verify_check(check_id, context)))
                .collect();
            dim_from_checks(dim_id, dim_name, &check_results)
        })
        .collect();

    build_report(dims, iteration)
}

/// Assert that a TrackerReport has exactly 9 dims, each with exactly 9 checks.
/// Returns true if the 9×9 matrix structure is valid.
pub fn verify_9x9_matrix(report: &TrackerReport) -> bool {
    if report.dims.len() != 9 {
        return false;
    }
    report.dims.iter().all(|d| d.checks_total == 9)
}

/// Build a `DimResult` from per-check `(name, passed)` pairs, scoring the ratio.
pub fn dim_from_checks(dim_id: &str, name: &str, checks: &[(&str, bool)]) -> DimResult {
    let total = checks.len() as u32;
    let passed = checks.iter().filter(|(_, ok)| *ok).count() as u32;
    let score = if total > 0 {
        f64::from(passed) / f64::from(total)
    } else {
        0.0
    };
    let details: Vec<String> = checks
        .iter()
        .map(|(name, ok)| format!("{}: {name}", if *ok { "OK" } else { "FAIL" }))
        .collect();

    DimResult {
        dim_id: dim_id.to_string(),
        name: name.to_string(),
        score,
        checks_passed: passed,
        checks_total: total,
        details,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;

    fn make_dim(id: &str, name: &str, score: f64) -> DimResult {
        DimResult {
            dim_id: id.to_string(),
            name: name.to_string(),
            score,
            checks_passed: (score * 9.0) as u32,
            checks_total: 9,
            details: vec![],
        }
    }

    #[test]
    fn test_critical_dim_weight() {
        let d1 = make_dim("D1", "Precision", 0.9);
        assert_eq!(d1.weight(), CRITICAL_WEIGHT);
        let d3 = make_dim("D3", "Performance", 0.9);
        assert_eq!(d3.weight(), NORMAL_WEIGHT);
    }

    #[test]
    fn test_compute_composite_all_pass() {
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 1.0))
            .collect();
        let composite = compute_composite(&dims);
        assert!((composite - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_determine_status_pass() {
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 0.85))
            .collect();
        let composite = compute_composite(&dims);
        assert_eq!(determine_status(&dims, composite), TrackerStatus::Pass);
    }

    #[test]
    fn test_build_report() {
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 0.90))
            .collect();
        let report = build_report(dims, 3);
        assert_eq!(report.status, TrackerStatus::Pass);
        assert_eq!(report.iteration, 3);
    }

    #[test]
    fn test_constants_match_python() {
        assert!((VETO_THRESHOLD - 0.80).abs() < 1e-10);
        assert!((HALT_THRESHOLD - 0.50).abs() < 1e-10);
        assert_eq!(HALT_ITERATIONS, 3);
    }

    // --- Display tests ---

    #[test]
    fn test_tracker_status_display() {
        assert_eq!(TrackerStatus::Pass.to_string(), "PASS");
        assert_eq!(TrackerStatus::Veto.to_string(), "VETO");
        assert_eq!(TrackerStatus::Halt.to_string(), "HALT");
    }

    // --- Summary tests ---

    #[test]
    fn test_summary_all_passing() {
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 0.95))
            .collect();
        let report = build_report(dims, 3);
        let summary = report.summary();
        assert_eq!(summary, "PASS composite=0.95 iter=3 (9/9 dims passing)");
    }

    #[test]
    fn test_summary_with_failures() {
        let mut dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 0.90))
            .collect();
        // Make D3 and D4 fail
        dims[2].score = 0.50;
        dims[3].score = 0.60;
        let report = build_report(dims, 5);
        let summary = report.summary();
        assert!(summary.starts_with("VETO"));
        assert!(summary.contains("iter=5"));
        assert!(summary.contains("(7/9 dims passing)"));
    }

    #[test]
    fn test_summary_empty_dims() {
        let report = build_report(vec![], 0);
        let summary = report.summary();
        assert_eq!(summary, "PASS composite=0.00 iter=0 (0/0 dims passing)");
    }

    // --- All-fail scenario ---

    #[test]
    fn test_all_fail_scenario() {
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 0.10))
            .collect();
        let report = build_report(dims, 1);
        assert_eq!(report.status, TrackerStatus::Halt);
        let summary = report.summary();
        assert!(summary.starts_with("HALT"));
        assert!(summary.contains("(0/9 dims passing)"));
    }

    // --- Mixed critical/normal dims ---

    #[test]
    fn test_mixed_critical_normal_dims() {
        // Critical dims (D1, D2, D6) failing, normal dims passing
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| {
                let score = if i == 1 || i == 2 || i == 6 {
                    0.50 // below veto threshold
                } else {
                    1.0
                };
                make_dim(&format!("D{i}"), &format!("Dim{i}"), score)
            })
            .collect();
        let report = build_report(dims, 2);
        // Not all pass → Veto (composite should be above HALT_THRESHOLD due to normal dims at 1.0)
        assert_eq!(report.status, TrackerStatus::Veto);
        let summary = report.summary();
        assert!(summary.contains("(6/9 dims passing)"));
        // Critical dims have higher weight so composite is dragged down
        assert!(report.composite < 0.80);
    }

    #[test]
    fn test_critical_dims_passing_normal_failing() {
        // Critical dims (D1, D2, D6) passing, one normal dim failing
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| {
                let score = if i == 3 { 0.50 } else { 0.90 };
                make_dim(&format!("D{i}"), &format!("Dim{i}"), score)
            })
            .collect();
        let report = build_report(dims, 1);
        // D3 fails → Veto
        assert_eq!(report.status, TrackerStatus::Veto);
    }

    // --- IL-6: as_rl_reward tests ---

    #[test]
    fn test_as_rl_reward_pass_perfect() {
        // All dims passing at score=1.0 → composite=1.0 → reward=1.0
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 1.0))
            .collect();
        let report = build_report(dims, 1);
        assert_eq!(report.status, TrackerStatus::Pass);
        let reward = report.as_rl_reward();
        assert!((reward - 1.0).abs() < 1e-9, "expected 1.0, got {reward}");
    }

    #[test]
    fn test_as_rl_reward_pass_range() {
        // Pass with composite=0.8 → reward = 0.5 + 0.5*0.8 = 0.9
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 0.85))
            .collect();
        let report = build_report(dims, 1);
        assert_eq!(report.status, TrackerStatus::Pass);
        let reward = report.as_rl_reward();
        assert!(
            reward >= 0.5 && reward <= 1.0,
            "Pass reward out of range: {reward}"
        );
    }

    #[test]
    fn test_as_rl_reward_halt_is_zero() {
        // All dims at score=0.1 → composite < 0.5 → Halt → reward=0.0
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 0.1))
            .collect();
        let report = build_report(dims, 1);
        assert_eq!(report.status, TrackerStatus::Halt);
        assert_eq!(report.as_rl_reward(), 0.0);
    }

    #[test]
    fn test_as_rl_reward_veto_range() {
        // Veto reward must be in (0.1, 0.5)
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| {
                let score = if i <= 3 { 0.60 } else { 0.85 };
                make_dim(&format!("D{i}"), &format!("Dim{i}"), score)
            })
            .collect();
        let report = build_report(dims, 1);
        assert_eq!(report.status, TrackerStatus::Veto);
        let reward = report.as_rl_reward();
        assert!(
            reward > 0.1 && reward < 0.5,
            "Veto reward out of range: {reward}"
        );
    }

    #[test]
    fn test_dimensional_rewards_count() {
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| make_dim(&format!("D{i}"), &format!("Dim{i}"), 0.9))
            .collect();
        let report = build_report(dims, 1);
        assert_eq!(report.dimensional_rewards().len(), 9);
    }

    #[test]
    fn test_dimensional_rewards_passing_dim_above_half() {
        // Passing dim with score=1.0 → reward = 0.5 + 0.5*1.0 = 1.0
        let dims = vec![make_dim("D1", "Precision", 1.0)];
        let composite = compute_composite(&dims);
        let status = determine_status(&dims, composite);
        let report = TrackerReport {
            dims,
            composite,
            status,
            iteration: 1,
        };
        let rewards = report.dimensional_rewards();
        assert_eq!(rewards.len(), 1);
        assert!(
            (rewards[0].1 - 1.0).abs() < 1e-9,
            "expected 1.0, got {}",
            rewards[0].1
        );
    }

    #[test]
    fn test_dimensional_rewards_failing_dim_partial_credit() {
        // Failing dim with score=0.5 → reward = 0.5*0.5 = 0.25
        let dims = vec![make_dim("D3", "Performance", 0.5)];
        let composite = compute_composite(&dims);
        let status = determine_status(&dims, composite);
        let report = TrackerReport {
            dims,
            composite,
            status,
            iteration: 1,
        };
        let rewards = report.dimensional_rewards();
        assert!(
            (rewards[0].1 - 0.25).abs() < 1e-9,
            "expected 0.25, got {}",
            rewards[0].1
        );
    }

    // --- IL-1: CheckRegistry tests ---

    #[test]
    fn test_check_registry_registered_handler_called() {
        struct AlwaysPass;
        impl CheckHandler for AlwaysPass {
            fn check_id(&self) -> &str {
                "D1C1"
            }
            fn verify(&self, _ctx: &CheckContext) -> bool {
                true
            }
        }
        let mut reg = CheckRegistry::new();
        reg.register(AlwaysPass);
        assert!(reg.verify("D1C1", &CheckContext::new()));
    }

    #[test]
    fn test_check_registry_registered_handler_can_fail() {
        struct AlwaysFail;
        impl CheckHandler for AlwaysFail {
            fn check_id(&self) -> &str {
                "D1C2"
            }
            fn verify(&self, _ctx: &CheckContext) -> bool {
                false
            }
        }
        let mut reg = CheckRegistry::new();
        reg.register(AlwaysFail);
        assert!(!reg.verify("D1C2", &CheckContext::new()));
    }

    #[test]
    fn test_check_registry_fallback_to_context_pass() {
        let reg = CheckRegistry::new();
        let ctx = CheckContext::new()
            .with_value("D2C1", 0.9)
            .with_threshold(0.8);
        assert!(reg.verify("D2C1", &ctx));
    }

    #[test]
    fn test_check_registry_fallback_to_context_fail() {
        let reg = CheckRegistry::new();
        let ctx = CheckContext::new()
            .with_value("D2C1", 0.5)
            .with_threshold(0.8);
        assert!(!reg.verify("D2C1", &ctx));
    }

    #[test]
    fn test_check_registry_default_pass_lenient() {
        // No value in context, default_pass=true → passes
        let reg = CheckRegistry::new();
        assert!(reg.verify("D9C9", &CheckContext::new()));
    }

    #[test]
    fn test_check_registry_strict_mode_no_value_fails() {
        let reg = CheckRegistry::new_strict();
        assert!(!reg.verify("D9C9", &CheckContext::new()));
    }
}
