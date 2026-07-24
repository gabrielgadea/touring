//! Evaluate a [`MetricRuleSet`] against a [`Workspace`] +
//! [`WorkspaceQualitySignal`] pair, emitting [`MetricViolation`]s.
//!
//! Per-entity rules iterate the relevant collection in `ws` and apply
//! `applies_to` glob filtering. Workspace-level rules read scalars
//! straight off the signal's `raw` block. The evaluator never panics
//! and never short-circuits — every rule is checked even if an earlier
//! one already failed.

use crate::quality::signal::{Workspace, WorkspaceQualitySignal};

use super::error::{Result, RulesError};
use super::matchers::matches_glob;
use super::types::{MetricKind, MetricRule, MetricRuleSet, MetricViolation};

/// Apply each rule in `rules` against `ws` + `signal`, returning every
/// violation discovered.
///
/// # Errors
///
/// Returns the first [`RulesError::Glob`] encountered (a malformed
/// `applies_to` pattern aborts evaluation since we cannot proceed
/// without it). All other failure modes are reported per-rule via
/// [`MetricViolation`]s rather than as errors.
pub fn evaluate(
    rules: &MetricRuleSet,
    ws: &Workspace,
    signal: &WorkspaceQualitySignal,
) -> Result<Vec<MetricViolation>> {
    let mut out: Vec<MetricViolation> = Vec::new();
    for rule in &rules.rules {
        if rule.metric.is_per_entity() {
            evaluate_per_entity(rule, ws, &mut out)?;
        } else if let Some(v) = evaluate_workspace_level(rule, ws, signal) {
            out.push(v);
        }
    }
    Ok(out)
}

fn evaluate_per_entity(
    rule: &MetricRule,
    ws: &Workspace,
    out: &mut Vec<MetricViolation>,
) -> Result<()> {
    let glob = rule.applies_to.as_deref().unwrap_or("**/*");
    match rule.metric {
        MetricKind::FileLines => {
            for (path, lines) in &ws.file_lines {
                if !matches_glob(&rule.name, glob, path)? {
                    continue;
                }
                let actual = *lines as f64;
                if !rule.op.satisfied(actual, rule.threshold) {
                    out.push(make_violation(rule, actual, Some(path.clone())));
                }
            }
        }
        MetricKind::FunctionCc => {
            for fc in &ws.function_cc {
                if !matches_glob(&rule.name, glob, &fc.file)? {
                    continue;
                }
                let actual = f64::from(fc.cc);
                if !rule.op.satisfied(actual, rule.threshold) {
                    out.push(make_violation(
                        rule,
                        actual,
                        Some(format!("{}::{}", fc.file, fc.func)),
                    ));
                }
            }
        }
        // Workspace-level metrics never reach here because of the
        // `is_per_entity` switch in the public `evaluate`.
        _ => {}
    }
    Ok(())
}

fn evaluate_workspace_level(
    rule: &MetricRule,
    _ws: &Workspace,
    signal: &WorkspaceQualitySignal,
) -> Option<MetricViolation> {
    let actual = match rule.metric {
        MetricKind::CycleCount => signal.raw.cycle_count as f64,
        MetricKind::MaxDepth => signal.raw.max_depth as f64,
        MetricKind::RedundancyRatio => signal.raw.redundancy_ratio,
        MetricKind::ComplexityGini => signal.raw.complexity_gini,
        MetricKind::ModularityQ => signal.raw.modularity_q,
        MetricKind::Signal0_10000 => f64::from(signal.signal_0_10000),
        // Per-entity metrics never reach here.
        MetricKind::FileLines | MetricKind::FunctionCc => return None,
    };
    if rule.op.satisfied(actual, rule.threshold) {
        return None;
    }
    Some(make_violation(rule, actual, None))
}

fn make_violation(rule: &MetricRule, actual: f64, location: Option<String>) -> MetricViolation {
    let message = rule.message.clone().unwrap_or_else(|| {
        format!(
            "{} {} {} (actual={:.3}, op={}, threshold={:.3})",
            rule.metric.as_str(),
            rule.op.symbol(),
            rule.threshold,
            actual,
            rule.op.symbol(),
            rule.threshold
        )
    });
    MetricViolation {
        rule_name: rule.name.clone(),
        severity: rule.severity,
        metric: rule.metric,
        op: rule.op,
        threshold: rule.threshold,
        actual,
        location,
        message,
    }
}

/// Convenience: classify violations into `(deny, warn, info)` counts.
#[must_use]
pub fn count_by_severity(violations: &[MetricViolation]) -> (usize, usize, usize) {
    let mut deny = 0usize;
    let mut warn = 0usize;
    let mut info = 0usize;
    for v in violations {
        match v.severity {
            super::types::Severity::Deny => deny += 1,
            super::types::Severity::Warn => warn += 1,
            super::types::Severity::Info => info += 1,
        }
    }
    (deny, warn, info)
}

/// Suppress the unused-parameter lint without binding `_rules` at call
/// sites. Re-exported for future helpers.
#[doc(hidden)]
fn _unused_helper(_: &RulesError) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::signal::{FuncComplexity, compute_quality_signal};
    use crate::rules::types::{MetricRule, MetricRuleSet, Op, Severity};

    fn ws_with_5_files_and_2_god() -> Workspace {
        let mut ws = Workspace::empty("/tmp/synth");
        for (name, lines) in [
            ("ok1.rs", 50_usize),
            ("ok2.rs", 80),
            ("ok3.rs", 200),
            ("god1.rs", 1500),
            ("god2.rs", 1200),
        ] {
            ws.file_lines.insert(name.to_string(), lines);
        }
        ws.function_cc = vec![
            FuncComplexity {
                file: "ok1.rs".into(),
                func: "f".into(),
                cc: 4,
            },
            FuncComplexity {
                file: "ok2.rs".into(),
                func: "g".into(),
                cc: 8,
            },
            FuncComplexity {
                file: "god1.rs".into(),
                func: "monster".into(),
                cc: 50,
            },
        ];
        ws
    }

    fn ruleset(rules: Vec<MetricRule>) -> MetricRuleSet {
        MetricRuleSet {
            version: "1.0".into(),
            rules,
        }
    }

    #[test]
    fn per_file_lines_rule_flags_god_files() {
        let ws = ws_with_5_files_and_2_god();
        let signal = compute_quality_signal(&ws);
        let rule = MetricRule {
            name: "no-god-files".into(),
            applies_to: Some("**/*.rs".into()),
            metric: MetricKind::FileLines,
            op: Op::Lt,
            threshold: 1000.0,
            severity: Severity::Warn,
            message: Some("too long".into()),
        };
        let violations = evaluate(&ruleset(vec![rule]), &ws, &signal).expect("eval");
        assert_eq!(violations.len(), 2);
        assert!(
            violations
                .iter()
                .any(|v| v.location.as_deref() == Some("god1.rs"))
        );
        assert!(
            violations
                .iter()
                .any(|v| v.location.as_deref() == Some("god2.rs"))
        );
    }

    #[test]
    fn per_function_cc_rule_flags_complex_fns() {
        let ws = ws_with_5_files_and_2_god();
        let signal = compute_quality_signal(&ws);
        let rule = MetricRule {
            name: "limit-cc".into(),
            applies_to: Some("**/*.rs".into()),
            metric: MetricKind::FunctionCc,
            op: Op::Lt,
            threshold: 15.0,
            severity: Severity::Deny,
            message: None,
        };
        let violations = evaluate(&ruleset(vec![rule]), &ws, &signal).expect("eval");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].location.as_deref(), Some("god1.rs::monster"));
        assert!((violations[0].actual - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn workspace_cycle_count_rule_fires_when_above_zero() {
        let mut ws = Workspace::empty("/tmp/cycle");
        ws.edges.push(("a.rs".into(), "b.rs".into()));
        ws.edges.push(("b.rs".into(), "a.rs".into()));
        let signal = compute_quality_signal(&ws);
        let rule = MetricRule {
            name: "no-cycles".into(),
            applies_to: None,
            metric: MetricKind::CycleCount,
            op: Op::Eq,
            threshold: 0.0,
            severity: Severity::Deny,
            message: None,
        };
        let violations = evaluate(&ruleset(vec![rule]), &ws, &signal).expect("eval");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].actual >= 1.0);
        assert!(violations[0].location.is_none());
    }

    #[test]
    fn signal_min_score_rule_fires_when_below() {
        let ws = Workspace::empty("/tmp/empty");
        let signal = compute_quality_signal(&ws); // perfect signal = 10000
        let rule = MetricRule {
            name: "min-signal".into(),
            applies_to: None,
            metric: MetricKind::Signal0_10000,
            op: Op::Ge,
            threshold: 12000.0, // unreachable threshold → must fire
            severity: Severity::Warn,
            message: None,
        };
        let violations = evaluate(&ruleset(vec![rule]), &ws, &signal).expect("eval");
        assert_eq!(violations.len(), 1);
        assert!((violations[0].actual - 10000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn count_by_severity_aggregates_correctly() {
        let v = vec![
            MetricViolation {
                rule_name: "a".into(),
                severity: Severity::Deny,
                metric: MetricKind::CycleCount,
                op: Op::Eq,
                threshold: 0.0,
                actual: 1.0,
                location: None,
                message: "a".into(),
            },
            MetricViolation {
                rule_name: "b".into(),
                severity: Severity::Warn,
                metric: MetricKind::CycleCount,
                op: Op::Eq,
                threshold: 0.0,
                actual: 1.0,
                location: None,
                message: "b".into(),
            },
            MetricViolation {
                rule_name: "c".into(),
                severity: Severity::Warn,
                metric: MetricKind::CycleCount,
                op: Op::Eq,
                threshold: 0.0,
                actual: 1.0,
                location: None,
                message: "c".into(),
            },
        ];
        let (deny, warn, info) = count_by_severity(&v);
        assert_eq!((deny, warn, info), (1, 2, 0));
    }

    #[test]
    fn glob_filters_per_entity_rules() {
        let ws = ws_with_5_files_and_2_god();
        let signal = compute_quality_signal(&ws);
        let rule = MetricRule {
            name: "only-god1".into(),
            applies_to: Some("god1.rs".into()),
            metric: MetricKind::FileLines,
            op: Op::Lt,
            threshold: 1000.0,
            severity: Severity::Warn,
            message: None,
        };
        let violations = evaluate(&ruleset(vec![rule]), &ws, &signal).expect("eval");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].location.as_deref(), Some("god1.rs"));
    }

    #[test]
    fn empty_ruleset_yields_zero_violations() {
        let ws = ws_with_5_files_and_2_god();
        let signal = compute_quality_signal(&ws);
        let violations = evaluate(&ruleset(vec![]), &ws, &signal).expect("eval");
        assert!(violations.is_empty());
    }

    #[test]
    fn _unused_helper_does_not_panic() {
        let err = RulesError::Invalid {
            rule: "x".into(),
            reason: "y".into(),
        };
        _unused_helper(&err);
    }
}
