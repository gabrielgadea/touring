//! Public types for the workspace metric rules engine.
//!
//! Sentrux Master Plan Wave 2 P3 (2026-05-09). Rules in this module
//! enforce *metric budgets* against the workspace state computed by
//! [`crate::quality::signal::compute_quality_signal`]. They are
//! **complementary** to (and intentionally namespaced apart from) the
//! YAML+regex *autofix* rules in `touring_foundation::rules` (absorbed W3.4)
//! crate — naming is `MetricRule` / `MetricRuleSet` / `MetricViolation`
//! to avoid homonimia per VP-Scout Cadeia 4.
//!
//! # Conceptual model
//!
//! ```text
//! .touring/quality.toml ── parser ──► MetricRuleSet ─┐
//!                                                    ▼
//!                              evaluate(rules, ws, signal) ──► Vec<MetricViolation>
//! ```

use serde::{Deserialize, Serialize};

/// Severity level attached to a [`MetricRule`].
///
/// `Deny` is intended to be a hard gate (CI fails); `Warn` and `Info`
/// are advisory. The evaluator does not itself decide what to *do* with
/// a violation — that policy lives in the caller (CLI gate, hook, etc.).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational: surfaced for visibility only, never affects exit status.
    Info,
    /// Advisory: reported as a warning but does not fail a gate.
    #[default]
    Warn,
    /// Hard gate: a matching violation should fail CI.
    Deny,
}

/// Comparison operator used by a [`MetricRule`] threshold check.
///
/// The rule fires when `actual <op> threshold` is **false** — i.e. when
/// the metric violates the budget. For example `op=Lt threshold=15`
/// fires whenever `actual >= 15` for the metric in question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
}

impl Op {
    /// Evaluate the operator: `true` means the metric is within budget.
    #[must_use]
    pub fn satisfied(self, actual: f64, threshold: f64) -> bool {
        match self {
            Op::Lt => actual < threshold,
            Op::Le => actual <= threshold,
            Op::Gt => actual > threshold,
            Op::Ge => actual >= threshold,
            Op::Eq => (actual - threshold).abs() < f64::EPSILON,
            Op::Ne => (actual - threshold).abs() >= f64::EPSILON,
        }
    }

    /// Human-readable symbol (`"<"`, `">="`, etc.) for diagnostics.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Eq => "==",
            Op::Ne => "!=",
        }
    }
}

/// The metric a [`MetricRule`] inspects.
///
/// Per-file / per-function metrics use the rule's `applies_to` glob to
/// scope which entities the rule covers. Workspace-level metrics ignore
/// `applies_to` (they have one value per workspace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    /// Per-file: total lines in a `.rs` (or other) file.
    FileLines,
    /// Per-function: cyclomatic complexity proxy.
    FunctionCc,
    /// Workspace-level: number of dependency cycles (Tarjan SCC).
    CycleCount,
    /// Workspace-level: longest acyclic dependency chain.
    MaxDepth,
    /// Workspace-level: redundant function ratio in `[0.0, 1.0]`.
    RedundancyRatio,
    /// Workspace-level: complexity-equality Gini in `[0.0, 1.0]`.
    ComplexityGini,
    /// Workspace-level: Newman modularity score in `[0.0, 1.0]`.
    ModularityQ,
    /// Workspace-level: Sentrux aggregate signal in `[0, 10000]`.
    ///
    /// Explicit `serde(rename)` because the default `snake_case` rule
    /// produces `"signal0_10000"` (no separator before the digit), but
    /// the canonical TOML key includes the leading underscore.
    #[serde(rename = "signal_0_10000")]
    Signal0_10000,
}

impl MetricKind {
    /// Returns `true` if the metric is per-file or per-function and
    /// therefore consults `applies_to`.
    #[must_use]
    pub const fn is_per_entity(self) -> bool {
        matches!(self, MetricKind::FileLines | MetricKind::FunctionCc)
    }

    /// Canonical TOML name (round-trips through serde).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            MetricKind::FileLines => "file_lines",
            MetricKind::FunctionCc => "function_cc",
            MetricKind::CycleCount => "cycle_count",
            MetricKind::MaxDepth => "max_depth",
            MetricKind::RedundancyRatio => "redundancy_ratio",
            MetricKind::ComplexityGini => "complexity_gini",
            MetricKind::ModularityQ => "modularity_q",
            MetricKind::Signal0_10000 => "signal_0_10000",
        }
    }
}

/// A single metric budget rule.
///
/// Loaded from TOML via the [`crate::rules::parser`] module. The
/// evaluator reads each rule, gathers the relevant metric value(s)
/// from the [`crate::quality::signal::Workspace`] /
/// [`crate::quality::signal::WorkspaceQualitySignal`], and emits
/// [`MetricViolation`]s when a budget is broken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRule {
    /// Rule identifier. Must be unique within a [`MetricRuleSet`].
    pub name: String,
    /// Glob pattern (e.g. `"src/handlers/**/*.rs"`) applied to the
    /// per-file path string. `None` means "all entities".
    ///
    /// Workspace-level metrics ignore this field.
    #[serde(default)]
    pub applies_to: Option<String>,
    /// Which metric to inspect.
    pub metric: MetricKind,
    /// Comparison operator.
    pub op: Op,
    /// Threshold value to compare against.
    pub threshold: f64,
    /// Severity attached to violations of this rule.
    #[serde(default)]
    pub severity: Severity,
    /// Optional human-readable message attached to violations.
    #[serde(default)]
    pub message: Option<String>,
}

/// A collection of metric rules, typically loaded from a single TOML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRuleSet {
    /// Schema version tag. Currently the only supported value is `"1.0"`.
    #[serde(default = "default_version")]
    pub version: String,
    /// The metric rules.
    #[serde(default, rename = "rule")]
    pub rules: Vec<MetricRule>,
}

impl Default for MetricRuleSet {
    fn default() -> Self {
        Self {
            version: default_version(),
            rules: Vec::new(),
        }
    }
}

fn default_version() -> String {
    String::from("1.0")
}

/// A single rule violation surfaced by the evaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricViolation {
    /// Name of the [`MetricRule`] that fired.
    pub rule_name: String,
    /// Severity copied from the rule.
    pub severity: Severity,
    /// Metric inspected.
    pub metric: MetricKind,
    /// Comparison operator copied from the rule.
    pub op: Op,
    /// Threshold value copied from the rule.
    pub threshold: f64,
    /// Actual metric value observed in the workspace.
    pub actual: f64,
    /// `Some("path")` for per-file rules, `Some("file:fn_name")` for
    /// per-function rules, `None` for workspace-level rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Human-readable message (rule's `message` field, or a default).
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_lt_satisfied() {
        assert!(Op::Lt.satisfied(10.0, 15.0));
        assert!(!Op::Lt.satisfied(15.0, 15.0));
        assert!(!Op::Lt.satisfied(20.0, 15.0));
    }

    #[test]
    fn op_eq_uses_epsilon() {
        assert!(Op::Eq.satisfied(0.0, 0.0));
        assert!(Op::Eq.satisfied(1.0, 1.0));
        assert!(!Op::Eq.satisfied(1.0, 1.0001));
    }

    #[test]
    fn op_symbol_matches_serde() {
        assert_eq!(Op::Lt.symbol(), "<");
        assert_eq!(Op::Ge.symbol(), ">=");
        assert_eq!(Op::Ne.symbol(), "!=");
    }

    #[test]
    fn metric_kind_categorisation() {
        assert!(MetricKind::FileLines.is_per_entity());
        assert!(MetricKind::FunctionCc.is_per_entity());
        assert!(!MetricKind::CycleCount.is_per_entity());
        assert!(!MetricKind::Signal0_10000.is_per_entity());
    }

    #[test]
    fn metric_kind_as_str_is_canonical() {
        assert_eq!(MetricKind::Signal0_10000.as_str(), "signal_0_10000");
        assert_eq!(MetricKind::ModularityQ.as_str(), "modularity_q");
    }

    #[test]
    fn ruleset_default_version() {
        let rs = MetricRuleSet::default();
        assert_eq!(rs.version, "1.0");
        assert!(rs.rules.is_empty());
    }

    #[test]
    fn rule_serde_round_trips() {
        let rule = MetricRule {
            name: "no-god-files".into(),
            applies_to: Some("**/*.rs".into()),
            metric: MetricKind::FileLines,
            op: Op::Lt,
            threshold: 1000.0,
            severity: Severity::Warn,
            message: Some("files should be < 1000 LOC".into()),
        };
        let json = serde_json::to_string(&rule).expect("serialize");
        let back: MetricRule = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, rule.name);
        assert_eq!(back.metric.as_str(), "file_lines");
    }
}
