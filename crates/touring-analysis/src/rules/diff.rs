//! Diff two [`super::types::MetricViolation`] result sets.
//!
//! Sentrux Master Plan Wave 2 P4 (2026-05-09). Given a `previous` and
//! a `current` violation list (typically from two evaluations of the
//! same [`super::types::MetricRuleSet`] against two
//! [`crate::quality::signal::Workspace`] snapshots), classify each
//! violation into one of three buckets:
//!
//! * `resolved`   — fired previously but not now (good).
//! * `introduced` — fires now but didn't previously (regression).
//! * `persisting` — fires in both snapshots; carries `delta_actual`
//!   so callers can tell whether the violation got worse or better
//!   even though it remains in violation.
//!
//! Equality is keyed by `(rule_name, location)`. For workspace-level
//! rules the `location` is `None`, so the key collapses to just the
//! rule name (which is unique within a [`super::types::MetricRuleSet`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::types::MetricViolation;

/// A persisting violation paired with its `current.actual - previous.actual`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistingViolation {
    /// The current-snapshot violation.
    pub current: MetricViolation,
    /// `current.actual - previous.actual`. Negative means the metric
    /// got better (closer to budget) even though the rule still fires;
    /// positive means it got worse.
    pub delta_actual: f64,
}

/// Categorised diff between two violation lists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViolationsDiff {
    /// Violations that fired previously but no longer fire.
    pub resolved: Vec<MetricViolation>,
    /// Violations that fire now but did not fire previously.
    pub introduced: Vec<MetricViolation>,
    /// Violations that fire in both snapshots, with delta context.
    pub persisting: Vec<PersistingViolation>,
}

impl ViolationsDiff {
    /// Total count across all three buckets.
    #[must_use]
    pub fn total(&self) -> usize {
        self.resolved.len() + self.introduced.len() + self.persisting.len()
    }

    /// `(resolved, introduced, persisting)` counts.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize) {
        (
            self.resolved.len(),
            self.introduced.len(),
            self.persisting.len(),
        )
    }
}

/// Compute the diff of two violation lists.
///
/// Two violations are considered the same when their `(rule_name,
/// location)` tuple matches. The first occurrence of a given key in
/// each list wins (later duplicates with the same key are ignored).
#[must_use]
pub fn diff_violations(
    previous: &[MetricViolation],
    current: &[MetricViolation],
) -> ViolationsDiff {
    let prev_index: BTreeMap<(String, Option<String>), &MetricViolation> = previous
        .iter()
        .map(|v| ((v.rule_name.clone(), v.location.clone()), v))
        .collect();
    let curr_index: BTreeMap<(String, Option<String>), &MetricViolation> = current
        .iter()
        .map(|v| ((v.rule_name.clone(), v.location.clone()), v))
        .collect();

    let mut resolved: Vec<MetricViolation> = Vec::new();
    let mut introduced: Vec<MetricViolation> = Vec::new();
    let mut persisting: Vec<PersistingViolation> = Vec::new();

    for (key, prev) in &prev_index {
        if let Some(curr) = curr_index.get(key) {
            persisting.push(PersistingViolation {
                current: (*curr).clone(),
                delta_actual: curr.actual - prev.actual,
            });
        } else {
            resolved.push((*prev).clone());
        }
    }
    for (key, curr) in &curr_index {
        if !prev_index.contains_key(key) {
            introduced.push((*curr).clone());
        }
    }

    ViolationsDiff {
        resolved,
        introduced,
        persisting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::types::{MetricKind, Op, Severity};

    fn v(rule_name: &str, loc: Option<&str>, actual: f64) -> MetricViolation {
        MetricViolation {
            rule_name: rule_name.to_string(),
            severity: Severity::Warn,
            metric: MetricKind::FileLines,
            op: Op::Lt,
            threshold: 100.0,
            actual,
            location: loc.map(|s| s.to_string()),
            message: "msg".to_string(),
        }
    }

    #[test]
    fn empty_diffs_yield_empty_buckets() {
        let d = diff_violations(&[], &[]);
        assert_eq!(d.counts(), (0, 0, 0));
        assert_eq!(d.total(), 0);
    }

    #[test]
    fn introduced_appears_only_in_current() {
        let d = diff_violations(&[], &[v("r1", Some("a.rs"), 200.0)]);
        assert_eq!(d.counts(), (0, 1, 0));
        assert_eq!(d.introduced[0].rule_name, "r1");
    }

    #[test]
    fn resolved_appears_only_in_previous() {
        let d = diff_violations(&[v("r1", Some("a.rs"), 200.0)], &[]);
        assert_eq!(d.counts(), (1, 0, 0));
        assert_eq!(d.resolved[0].rule_name, "r1");
    }

    #[test]
    fn persisting_records_delta_actual() {
        let prev = vec![v("r1", Some("a.rs"), 200.0)];
        let curr = vec![v("r1", Some("a.rs"), 250.0)];
        let d = diff_violations(&prev, &curr);
        assert_eq!(d.counts(), (0, 0, 1));
        assert!((d.persisting[0].delta_actual - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn workspace_level_violations_keyed_by_rule_only() {
        let prev = vec![v("no-cycles", None, 7.0)];
        let curr = vec![v("no-cycles", None, 5.0)];
        let d = diff_violations(&prev, &curr);
        assert_eq!(d.counts(), (0, 0, 1));
        assert!((d.persisting[0].delta_actual + 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn different_locations_treated_as_distinct_violations() {
        let prev = vec![v("r1", Some("a.rs"), 200.0)];
        let curr = vec![v("r1", Some("b.rs"), 300.0)];
        let d = diff_violations(&prev, &curr);
        assert_eq!(d.counts(), (1, 1, 0));
    }

    #[test]
    fn mixed_diff_classifies_correctly() {
        let prev = vec![
            v("god-files", Some("a.rs"), 1500.0),
            v("god-files", Some("b.rs"), 1100.0),
            v("no-cycles", None, 7.0),
        ];
        let curr = vec![
            v("god-files", Some("a.rs"), 2000.0), // persisting, worse
            v("limit-cc", Some("c.rs::f"), 50.0), // introduced
        ];
        let d = diff_violations(&prev, &curr);
        assert_eq!(d.counts(), (2, 1, 1));
        assert!(
            d.persisting
                .iter()
                .any(|p| (p.delta_actual - 500.0).abs() < f64::EPSILON)
        );
        assert!(d.resolved.iter().any(|v| v.rule_name == "no-cycles"));
        assert!(d.introduced.iter().any(|v| v.rule_name == "limit-cc"));
    }
}
