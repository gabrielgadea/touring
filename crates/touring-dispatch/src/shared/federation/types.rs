//! Federation types — entries, summary, average root causes.
//!
//! Sentrux Master Plan Wave 3 P7 (2026-05-09). Federation aggregates
//! multiple [`WorkspaceQualitySignal`] snapshots (one per workspace
//! root) into a single [`FederationSummary`] suitable for cross-repo
//! dashboards or fleet-wide quality monitoring.
//!
//! All types in this module are pure data carriers (no I/O, no mutation
//! of external state) — the actual aggregation lives in the sibling
//! [`super::aggregator`] module.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use touring_analysis::quality::signal::{Bottleneck, WorkspaceQualitySignal};

/// A single workspace's contribution to the federated view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationEntry {
    /// Stable identifier for the workspace (typically the directory
    /// name or a tag chosen by the operator). Used to look up the
    /// `worst_workspace` / `best_workspace` fields in
    /// [`FederationSummary`].
    pub workspace_id: String,
    /// Filesystem root that was scanned to produce `signal`.
    pub workspace_root: PathBuf,
    /// The [`WorkspaceQualitySignal`] snapshot for this workspace.
    pub signal: WorkspaceQualitySignal,
    /// Unix milliseconds when the snapshot was taken (used to flag
    /// stale entries downstream — the aggregator itself ignores age).
    pub timestamp_ms: u64,
}

/// Mean of each normalized root-cause score across all entries.
///
/// Each field is in `[0.0, 1.0]` (since input scores already are);
/// `None` is collapsed to `0.0` when there are zero entries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct AvgRootCauses {
    /// Mean modularity score across the federation.
    pub modularity: f64,
    /// Mean acyclicity score across the federation.
    pub acyclicity: f64,
    /// Mean depth score across the federation.
    pub depth: f64,
    /// Mean equality (Gini-inverted) score across the federation.
    pub equality: f64,
    /// Mean redundancy (inverted-ratio) score across the federation.
    pub redundancy: f64,
}

/// Federated summary across N [`FederationEntry`] snapshots.
///
/// The `aggregate_signal_0_10000` is the **arithmetic mean** of
/// per-workspace signals (deliberately *not* the geometric mean — that
/// would over-penalise federations with one outlier; per-workspace
/// signals are already gameproof internally).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationSummary {
    /// Number of [`FederationEntry`]s aggregated.
    pub entries: usize,
    /// Arithmetic mean of `signal_0_10000` across entries (`0` when empty).
    pub aggregate_signal_0_10000: u32,
    /// Lowest per-entry signal in the federation (`0` when empty).
    pub min_signal: u32,
    /// Highest per-entry signal in the federation (`0` when empty).
    pub max_signal: u32,
    /// Population standard deviation of per-entry signals (`0.0` when empty
    /// or single-entry).
    pub stddev_signal: f64,
    /// Histogram of bottleneck root causes across the federation.
    /// Keys are the canonical lowercase labels (`"modularity"`,
    /// `"acyclicity"`, etc.), values are counts.
    pub bottleneck_distribution: BTreeMap<String, usize>,
    /// `workspace_id` of the entry with the lowest signal (`None`
    /// when there are zero entries; ties broken by first occurrence).
    pub worst_workspace: Option<String>,
    /// `workspace_id` of the entry with the highest signal.
    pub best_workspace: Option<String>,
    /// Per-axis means of normalized root-cause scores.
    pub avg_root_causes: AvgRootCauses,
}

impl FederationSummary {
    /// Empty federation summary — used as a default for zero-entry inputs.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: 0,
            aggregate_signal_0_10000: 0,
            min_signal: 0,
            max_signal: 0,
            stddev_signal: 0.0,
            bottleneck_distribution: BTreeMap::new(),
            worst_workspace: None,
            best_workspace: None,
            avg_root_causes: AvgRootCauses::default(),
        }
    }

    /// Returns the most-frequent bottleneck label (and its count) across
    /// the federation, or `None` if every entry is at parity (no
    /// bottleneck) or the federation is empty.
    #[must_use]
    pub fn dominant_bottleneck(&self) -> Option<(&str, usize)> {
        self.bottleneck_distribution
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(label, count)| (label.as_str(), *count))
    }
}

impl Default for FederationSummary {
    fn default() -> Self {
        Self::empty()
    }
}

/// Canonical lowercase label for a [`Bottleneck`] enum variant.
///
/// Used to key the `bottleneck_distribution` histogram. We avoid
/// `format!("{:?}", ...)` because that would produce `"Modularity"`
/// (capitalised) — keeping the labels lowercase matches the
/// `serde(rename_all = "lowercase")` convention used elsewhere in the
/// Sentrux signal codebase.
#[must_use]
pub fn bottleneck_label(b: Bottleneck) -> &'static str {
    match b {
        Bottleneck::Modularity => "modularity",
        Bottleneck::Acyclicity => "acyclicity",
        Bottleneck::Depth => "depth",
        Bottleneck::Equality => "equality",
        Bottleneck::Redundancy => "redundancy",
        Bottleneck::Tied => "tied",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_summary_default_matches_empty() {
        assert_eq!(
            serde_json::to_value(FederationSummary::default()).unwrap(),
            serde_json::to_value(FederationSummary::empty()).unwrap()
        );
    }

    #[test]
    fn dominant_bottleneck_returns_none_for_empty() {
        assert!(FederationSummary::empty().dominant_bottleneck().is_none());
    }

    #[test]
    fn dominant_bottleneck_picks_max() {
        let mut s = FederationSummary::empty();
        s.bottleneck_distribution.insert("modularity".into(), 3);
        s.bottleneck_distribution.insert("acyclicity".into(), 7);
        s.bottleneck_distribution.insert("depth".into(), 1);
        let (label, count) = s.dominant_bottleneck().unwrap();
        assert_eq!(label, "acyclicity");
        assert_eq!(count, 7);
    }

    #[test]
    fn bottleneck_label_round_trips_canonical_lowercase() {
        assert_eq!(bottleneck_label(Bottleneck::Modularity), "modularity");
        assert_eq!(bottleneck_label(Bottleneck::Acyclicity), "acyclicity");
        assert_eq!(bottleneck_label(Bottleneck::Tied), "tied");
    }

    #[test]
    fn avg_root_causes_default_is_all_zero() {
        let z = AvgRootCauses::default();
        assert_eq!(z.modularity, 0.0);
        assert_eq!(z.redundancy, 0.0);
    }
}
