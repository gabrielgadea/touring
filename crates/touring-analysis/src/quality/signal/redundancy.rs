//! Redundancy root cause — fraction of functions that are dead or duplicate.
//!
//! `R = (dead + duplicate) / total_functions`. The score is `1 - R`. The
//! workspace handle carries pre-collected `dead_functions` and
//! `duplicate_groups` lists (typically populated from `touring wiring orphans`
//! and a body-hash duplicate detector); this module just performs the ratio.

use super::types::Workspace;

/// Total redundant functions = dead + (sum over groups of `instances - 1`).
///
/// The `-1` is intentional: each duplicate group always retains exactly one
/// canonical instance; only the *extras* are wasteful.
#[must_use]
pub fn compute_dead_count(ws: &Workspace) -> usize {
    ws.dead_functions.len()
}

/// Total duplicate function instances beyond the canonical (one per group).
#[must_use]
pub fn compute_duplicate_count(ws: &Workspace) -> usize {
    ws.duplicate_groups
        .iter()
        .map(|g| g.instances.len().saturating_sub(1))
        .sum()
}

/// Redundancy ratio: `(dead + duplicate) / total_functions`, clamped `[0,1]`.
///
/// Returns `0.0` when there are no functions to attribute waste to.
#[must_use]
pub fn compute_redundancy_ratio(ws: &Workspace) -> f64 {
    let total = ws.function_count();
    if total == 0 {
        return 0.0;
    }
    let dead = compute_dead_count(ws);
    let dup = compute_duplicate_count(ws);
    let waste = (dead + dup).min(total);
    (waste as f64) / (total as f64)
}

/// Non-redundancy score: direct invert `1 - ratio`.
#[must_use]
pub fn redundancy_score(ratio: f64) -> f64 {
    (1.0 - ratio).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::super::types::{DuplicateGroup, FuncComplexity, FuncIdent};
    use super::*;

    fn ws_with_fns(n: usize) -> Workspace {
        let mut ws = Workspace::empty("/tmp");
        ws.function_cc = (0..n)
            .map(|i| FuncComplexity {
                file: "a".into(),
                func: format!("f{i}"),
                cc: 1,
            })
            .collect();
        ws
    }

    #[test]
    fn no_waste_when_no_dead_no_dup() {
        let ws = ws_with_fns(10);
        assert!(compute_redundancy_ratio(&ws).abs() < f64::EPSILON);
        assert!((redundancy_score(0.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fifty_percent_dead_yields_half_score() {
        let mut ws = ws_with_fns(10);
        for i in 0..5 {
            ws.dead_functions.push(FuncIdent {
                file: "a".into(),
                func: format!("dead{i}"),
            });
        }
        let r = compute_redundancy_ratio(&ws);
        assert!((r - 0.5).abs() < f64::EPSILON);
        assert!((redundancy_score(r) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn duplicate_group_counts_extras_only() {
        let mut ws = ws_with_fns(10);
        ws.duplicate_groups.push(DuplicateGroup {
            hash: "h1".into(),
            instances: vec![
                FuncIdent {
                    file: "a".into(),
                    func: "x1".into(),
                },
                FuncIdent {
                    file: "b".into(),
                    func: "x2".into(),
                },
                FuncIdent {
                    file: "c".into(),
                    func: "x3".into(),
                },
            ],
        });
        // 1 group of 3 instances → 2 extras.
        assert_eq!(compute_duplicate_count(&ws), 2);
    }

    #[test]
    fn waste_clamped_to_total() {
        let mut ws = ws_with_fns(2);
        // Synthesize over-counting: 5 dead functions but only 2 total fns.
        for i in 0..5 {
            ws.dead_functions.push(FuncIdent {
                file: "a".into(),
                func: format!("d{i}"),
            });
        }
        // ratio must clamp at 1.0
        assert!((compute_redundancy_ratio(&ws) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn zero_functions_zero_ratio() {
        let ws = Workspace::empty("/tmp");
        assert!(compute_redundancy_ratio(&ws).abs() < f64::EPSILON);
    }
}
