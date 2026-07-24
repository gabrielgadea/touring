//! Quality-signal models — shared serde shapes for the dashboard.
//!
//! Sentrux Master Plan Wave 4 P10.2 (2026-05-09). The structures here
//! mirror the public touring-analysis::quality::signal API; we keep
//! them as light wrappers so the WASM client does not need to compile
//! the full analysis crate. The fields use `#[serde(default)]` to
//! tolerate forward-compatible additions on the server side.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Five normalized scores in `[0, 1]` — one per Sentrux root-cause axis.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RootCauseScores {
    /// Modularity — community structure quality (Newman/Leiden).
    #[serde(default)]
    pub modularity: f64,
    /// Acyclicity — DAG-ness (penalizes cycle count).
    #[serde(default)]
    pub acyclicity: f64,
    /// Depth — penalizes excessively deep call graphs.
    #[serde(default)]
    pub depth: f64,
    /// Equality — Gini coefficient of complexity distribution.
    #[serde(default)]
    pub equality: f64,
    /// Redundancy — penalizes copy-pasted code regions.
    #[serde(default)]
    pub redundancy: f64,
}

/// Raw structural counters that feed the normalized scores.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RootCauseRaw {
    /// Modularity Q value (Newman) computed by the analysis pipeline.
    #[serde(default)]
    pub modularity_q: f64,
    /// Number of dependency cycles detected (Tarjan SCC).
    #[serde(default)]
    pub cycle_count: u64,
    /// Maximum call-graph depth.
    #[serde(default)]
    pub max_depth: u64,
    /// Gini coefficient of cyclomatic complexity across functions.
    #[serde(default)]
    pub complexity_gini: f64,
    /// Fraction of code that is structurally redundant.
    #[serde(default)]
    pub redundancy_ratio: f64,
    /// Total functions discovered in the workspace.
    #[serde(default)]
    pub total_functions: u64,
    /// Total graph nodes (modules + functions + types).
    #[serde(default)]
    pub total_nodes: u64,
    /// Total edges (call/import/use relationships).
    #[serde(default)]
    pub total_edges: u64,
}

/// Single-workspace Sentrux quality signal — the central dashboard datum.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceQualitySignal {
    /// Composite quality score, integer in `[0, 10_000]`.
    #[serde(default)]
    pub signal_0_10000: u32,
    /// Same composite score normalized to `[0.0, 1.0]`.
    #[serde(default)]
    pub signal_normalized: f64,
    /// Dominant bottleneck axis ("modularity", "acyclicity", …).
    #[serde(default)]
    pub bottleneck: String,
    /// Per-axis normalized scores.
    #[serde(default)]
    pub root_causes: RootCauseScores,
    /// Underlying raw counters.
    #[serde(default)]
    pub raw: RootCauseRaw,
    /// Workspace root path (None when computed for the local repo).
    #[serde(default)]
    pub root: Option<String>,
    /// Error envelope key — populated when the analysis pipeline fails.
    #[serde(default)]
    pub error: Option<String>,
}

/// Aggregate Sentrux signal across N federated workspaces.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FederationSummary {
    /// Number of workspaces aggregated.
    #[serde(default)]
    pub entries: u64,
    /// Aggregate (arithmetic-mean) composite score, integer `[0, 10_000]`.
    #[serde(default)]
    pub aggregate_signal_0_10000: u32,
    /// Lowest single-workspace score in the federation.
    #[serde(default)]
    pub min_signal: u32,
    /// Highest single-workspace score in the federation.
    #[serde(default)]
    pub max_signal: u32,
    /// Standard deviation of scores across the federation.
    #[serde(default)]
    pub stddev_signal: f64,
    /// Histogram of dominant bottlenecks (axis name → workspace count).
    #[serde(default)]
    pub bottleneck_distribution: BTreeMap<String, u64>,
    /// Name of the lowest-scoring workspace, when available.
    #[serde(default)]
    pub worst_workspace: Option<String>,
    /// Name of the highest-scoring workspace, when available.
    #[serde(default)]
    pub best_workspace: Option<String>,
}
