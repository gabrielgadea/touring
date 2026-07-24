//! Aggregate N [`super::types::FederationEntry`]s into one
//! [`super::types::FederationSummary`].
//!
//! Sentrux Master Plan Wave 3 P7 (2026-05-09). The aggregator is
//! deliberately small (one public function) and pure — no I/O, no
//! globals, deterministic output. Callers in `touring-server`'s
//! status pipeline or future MCP federation tools build the entry
//! vector themselves and pass it in.
//!
//! # Aggregation model
//!
//! * `aggregate_signal_0_10000` — arithmetic mean over per-entry
//!   `signal_0_10000`. Geometric mean is intentionally **not** used at
//!   the federation level: per-workspace signals are already gameproof
//!   (Nash-1950 geometric mean over five root causes) — applying a
//!   second geometric mean across workspaces would compound penalties
//!   for healthy federations with one outlier.
//! * `min_signal` / `max_signal` — direct min and max scans.
//! * `worst_workspace` / `best_workspace` — the `workspace_id` whose
//!   signal hit the min / max. Ties are broken by first occurrence
//!   (preserves caller's input ordering).
//! * `bottleneck_distribution` — histogram keyed by lowercase label
//!   (see [`super::types::bottleneck_label`]).
//! * `avg_root_causes` — per-axis arithmetic mean over the five
//!   normalized scores.
//! * `stddev_signal` — population stddev of per-entry signals.

use super::types::{AvgRootCauses, FederationEntry, FederationSummary, bottleneck_label};

/// Aggregate `entries` into a single [`FederationSummary`].
///
/// Returns [`FederationSummary::empty`] when `entries.is_empty()`.
#[must_use]
pub fn aggregate(entries: &[FederationEntry]) -> FederationSummary {
    if entries.is_empty() {
        return FederationSummary::empty();
    }

    let count = entries.len();

    // Arithmetic mean of u32 signals — overflow-safe for any practical
    // federation size (max value 10_000 × usize::MAX is bounded by the
    // u128 accumulator).
    let total_signal: u128 = entries
        .iter()
        .map(|e| u128::from(e.signal.signal_0_10000))
        .sum();
    let mean_signal_u32 = u32::try_from(total_signal / (count as u128)).unwrap_or(u32::MAX);

    // Min / max in a single scan; track workspace_id for the extremes.
    let mut min_signal = u32::MAX;
    let mut max_signal: u32 = 0;
    let mut worst_id: Option<&str> = None;
    let mut best_id: Option<&str> = None;
    for entry in entries {
        let s = entry.signal.signal_0_10000;
        if s < min_signal {
            min_signal = s;
            worst_id = Some(&entry.workspace_id);
        }
        if s > max_signal {
            max_signal = s;
            best_id = Some(&entry.workspace_id);
        }
    }
    if count == 1 {
        // Single entry: min == max; ensure both pointers populated.
        worst_id = worst_id.or(Some(&entries[0].workspace_id));
        best_id = best_id.or(Some(&entries[0].workspace_id));
    }

    // Bottleneck histogram.
    let mut bottleneck_distribution: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for entry in entries {
        let label = bottleneck_label(entry.signal.bottleneck);
        *bottleneck_distribution
            .entry(label.to_string())
            .or_insert(0) += 1;
    }

    // Per-axis means of normalized root-cause scores.
    let mut sum_modularity = 0.0_f64;
    let mut sum_acyclicity = 0.0_f64;
    let mut sum_depth = 0.0_f64;
    let mut sum_equality = 0.0_f64;
    let mut sum_redundancy = 0.0_f64;
    for entry in entries {
        let r = &entry.signal.root_causes;
        sum_modularity += r.modularity;
        sum_acyclicity += r.acyclicity;
        sum_depth += r.depth;
        sum_equality += r.equality;
        sum_redundancy += r.redundancy;
    }
    let n = count as f64;
    let avg_root_causes = AvgRootCauses {
        modularity: sum_modularity / n,
        acyclicity: sum_acyclicity / n,
        depth: sum_depth / n,
        equality: sum_equality / n,
        redundancy: sum_redundancy / n,
    };

    // Population stddev (signed-int-safe).
    let mean = (mean_signal_u32 as f64).round();
    let variance: f64 = entries
        .iter()
        .map(|e| {
            let v = e.signal.signal_0_10000 as f64;
            (v - mean).powi(2)
        })
        .sum::<f64>()
        / n;
    let stddev_signal = variance.sqrt();

    FederationSummary {
        entries: count,
        aggregate_signal_0_10000: mean_signal_u32,
        min_signal,
        max_signal,
        stddev_signal,
        bottleneck_distribution,
        worst_workspace: worst_id.map(String::from),
        best_workspace: best_id.map(String::from),
        avg_root_causes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use touring_analysis::quality::signal::{
        FuncComplexity, Workspace, WorkspaceQualitySignal, compute_quality_signal,
    };

    fn perfect_workspace_signal() -> WorkspaceQualitySignal {
        compute_quality_signal(&Workspace::empty("/tmp/perfect"))
    }

    fn bad_workspace_signal() -> WorkspaceQualitySignal {
        let mut ws = Workspace::empty("/tmp/bad");
        for i in 0..5 {
            ws.edges.push((format!("a{i}.rs"), format!("b{i}.rs")));
            ws.edges.push((format!("b{i}.rs"), format!("a{i}.rs")));
        }
        ws.function_cc = (0..30)
            .map(|i| FuncComplexity {
                file: format!("a{i}.rs"),
                func: format!("f{i}"),
                cc: if i == 0 { 200 } else { 1 },
            })
            .collect();
        compute_quality_signal(&ws)
    }

    fn entry(id: &str, signal: WorkspaceQualitySignal) -> FederationEntry {
        FederationEntry {
            workspace_id: id.to_string(),
            workspace_root: PathBuf::from(format!("/tmp/{id}")),
            signal,
            timestamp_ms: 0,
        }
    }

    #[test]
    fn empty_input_yields_empty_summary() {
        let summary = aggregate(&[]);
        assert_eq!(summary.entries, 0);
        assert_eq!(summary.aggregate_signal_0_10000, 0);
        assert!(summary.worst_workspace.is_none());
        assert!(summary.best_workspace.is_none());
    }

    #[test]
    fn single_entry_passes_through() {
        let s = perfect_workspace_signal();
        let summary = aggregate(&[entry("ws1", s.clone())]);
        assert_eq!(summary.entries, 1);
        assert_eq!(summary.aggregate_signal_0_10000, s.signal_0_10000);
        assert_eq!(summary.min_signal, s.signal_0_10000);
        assert_eq!(summary.max_signal, s.signal_0_10000);
        assert_eq!(summary.worst_workspace.as_deref(), Some("ws1"));
        assert_eq!(summary.best_workspace.as_deref(), Some("ws1"));
        assert!(summary.stddev_signal.abs() < f64::EPSILON);
    }

    #[test]
    fn two_workspaces_identifies_best_and_worst() {
        let entries = vec![
            entry("good", perfect_workspace_signal()),
            entry("bad", bad_workspace_signal()),
        ];
        let summary = aggregate(&entries);
        assert_eq!(summary.entries, 2);
        assert_eq!(summary.best_workspace.as_deref(), Some("good"));
        assert_eq!(summary.worst_workspace.as_deref(), Some("bad"));
        assert!(summary.max_signal > summary.min_signal);
        assert!(summary.stddev_signal > 0.0);
    }

    #[test]
    fn aggregate_signal_is_arithmetic_mean() {
        let entries = vec![
            entry("a", perfect_workspace_signal()),
            entry("b", bad_workspace_signal()),
        ];
        let summary = aggregate(&entries);
        let expected_mean = (entries[0].signal.signal_0_10000 as u128
            + entries[1].signal.signal_0_10000 as u128)
            / 2;
        assert_eq!(summary.aggregate_signal_0_10000 as u128, expected_mean);
    }

    #[test]
    fn bottleneck_distribution_counts_per_label() {
        let entries = vec![
            entry("a", bad_workspace_signal()),
            entry("b", bad_workspace_signal()),
            entry("c", perfect_workspace_signal()),
        ];
        let summary = aggregate(&entries);
        // Three entries → three histogram counts in total.
        let total: usize = summary.bottleneck_distribution.values().sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn avg_root_causes_collapses_to_per_axis_mean() {
        let entries = vec![
            entry("a", perfect_workspace_signal()),
            entry("b", perfect_workspace_signal()),
        ];
        let summary = aggregate(&entries);
        // Two perfect signals — every axis must be near 1.0.
        assert!((summary.avg_root_causes.acyclicity - 1.0).abs() < 1e-9);
        assert!((summary.avg_root_causes.redundancy - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dominant_bottleneck_extracted_from_summary() {
        let entries = vec![
            entry("a", bad_workspace_signal()),
            entry("b", bad_workspace_signal()),
            entry("c", perfect_workspace_signal()),
        ];
        let summary = aggregate(&entries);
        let (label, count) = summary
            .dominant_bottleneck()
            .expect("non-empty distribution must have dominant");
        // bad_workspace_signal puts the bottleneck on whichever axis is
        // worst — but it must show up at least 2× (count for "a" + "b").
        assert!(
            count >= 2,
            "expected dominant count >= 2, got {label}={count}"
        );
    }

    #[test]
    fn ties_broken_by_first_occurrence() {
        let s = perfect_workspace_signal();
        let entries = vec![entry("first", s.clone()), entry("second", s.clone())];
        let summary = aggregate(&entries);
        // All identical → first occurrence wins for both worst and best.
        assert_eq!(summary.worst_workspace.as_deref(), Some("first"));
        assert_eq!(summary.best_workspace.as_deref(), Some("first"));
    }

    #[test]
    fn stddev_zero_when_all_identical() {
        let s = perfect_workspace_signal();
        let entries = vec![entry("a", s.clone()), entry("b", s.clone()), entry("c", s)];
        let summary = aggregate(&entries);
        assert!(summary.stddev_signal.abs() < 1e-9);
    }
}
