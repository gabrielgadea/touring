//! Public data types for the workspace quality signal.
//!
//! These types are the contract consumed by:
//! - `lib.rs::compute_quality_signal` (entry point)
//! - CLI `touring quality-signal -j`
//! - MCP tool `touring_quality_signal` (Sentrux-compat shape)
//! - `touring-rules` engine (W2 P3 — gates threshold checks)
//! - `touring-federation` aggregator (W3 P7 — cross-workspace signals)

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Top-level quality signal envelope (Sentrux-compatible JSON shape).
///
/// Computed via geometric mean of 5 root cause scores, scaled to 0..=10000.
/// Lyapunov-stable convergence: improving any dim raises the signal; gaming
/// one while degrading another never raises it (Nash bargaining theorem).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceQualitySignal {
    /// Integer score on Sentrux 0..=10000 scale (geometric_mean × 10_000).
    pub signal_0_10000: u32,
    /// Same value normalized to [0.0, 1.0] (Touring `composite_health_score` scale).
    pub signal_normalized: f64,
    /// Action-guiding label of the lowest-scoring root cause (or `Tied`).
    pub bottleneck: Bottleneck,
    /// Per-root-cause normalized scores in `[0,1]`.
    pub root_causes: RootCauseScores,
    /// Un-normalized raw values (for display/diagnostics).
    pub raw: RootCauseRaw,
    /// Per-root-cause diagnostics (god_files, hotspots, dead, duplicates, etc.).
    pub diagnostics: Diagnostics,
    /// Wallclock when the signal was computed.
    #[serde(with = "serde_systemtime")]
    pub computed_at: SystemTime,
}

impl Default for WorkspaceQualitySignal {
    /// Neutral default: 5000 (mid-range), no bottleneck, empty diagnostics.
    fn default() -> Self {
        Self {
            signal_0_10000: 5000,
            signal_normalized: 0.5,
            bottleneck: Bottleneck::Tied,
            root_causes: RootCauseScores::neutral(),
            raw: RootCauseRaw::default(),
            diagnostics: Diagnostics::default(),
            computed_at: SystemTime::UNIX_EPOCH,
        }
    }
}

/// Normalized scores per root cause, each in [0.0, 1.0] (1.0 = best).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RootCauseScores {
    /// Newman's Q (modular structure). `(Q + 0.5) / 1.5` rescale.
    pub modularity: f64,
    /// Acyclicity score: `1 / (1 + cycle_count)` sigmoid.
    pub acyclicity: f64,
    /// Depth score: `1 / (1 + max_depth / 8)` sigmoid (midpoint=8).
    pub depth: f64,
    /// Equality score: `1 - gini` direct invert.
    pub equality: f64,
    /// Non-redundancy score: `1 - redundancy_ratio` direct invert.
    pub redundancy: f64,
}

impl RootCauseScores {
    /// Neutral 0.5 across all dims (midpoint).
    #[must_use]
    pub const fn neutral() -> Self {
        Self {
            modularity: 0.5,
            acyclicity: 0.5,
            depth: 0.5,
            equality: 0.5,
            redundancy: 0.5,
        }
    }

    /// Perfect 1.0 across all dims (theoretical max).
    #[must_use]
    pub const fn perfect() -> Self {
        Self {
            modularity: 1.0,
            acyclicity: 1.0,
            depth: 1.0,
            equality: 1.0,
            redundancy: 1.0,
        }
    }

    /// Iterator over the five (label, score) pairs in canonical order.
    pub fn iter_labelled(&self) -> [(&'static str, f64); 5] {
        [
            ("modularity", self.modularity),
            ("acyclicity", self.acyclicity),
            ("depth", self.depth),
            ("equality", self.equality),
            ("redundancy", self.redundancy),
        ]
    }
}

/// Raw (un-normalized) values for display purposes.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RootCauseRaw {
    /// Newman's Q ∈ [-0.5, 1.0] (theoretical bounds).
    pub modularity_q: f64,
    /// Strongly connected components with > 1 member.
    pub cycle_count: usize,
    /// Longest dependency chain in the DAG.
    pub max_depth: u32,
    /// Gini coefficient ∈ [0, 1] of per-fn complexity distribution.
    pub complexity_gini: f64,
    /// Redundancy ratio ∈ [0, 1] = (dead + duplicate) / total_functions.
    pub redundancy_ratio: f64,
    /// Total functions surveyed (denominator for redundancy_ratio).
    pub total_functions: usize,
    /// Total nodes in the dependency graph.
    pub total_nodes: usize,
    /// Total edges in the dependency graph.
    pub total_edges: usize,
}

/// The single lowest-scoring root cause (or `Tied` if all equal).
///
/// This is the action-guiding field for AI agents: `fix bottleneck → re-scan
/// → next bottleneck` until convergence (gradient descent on quality_signal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bottleneck {
    /// Modularity is the bottleneck (focus: god files, hotspots, instability).
    Modularity,
    /// Acyclicity is the bottleneck (focus: break dependency cycles).
    Acyclicity,
    /// Depth is the bottleneck (focus: flatten layered architecture).
    Depth,
    /// Equality is the bottleneck (focus: split god functions, large files).
    Equality,
    /// Redundancy is the bottleneck (focus: remove dead/duplicate code).
    Redundancy,
    /// All scores within tie threshold (no clear bottleneck).
    Tied,
}

impl Bottleneck {
    /// Human-readable label for CLI rendering.
    #[must_use]
    pub const fn as_label(&self) -> &'static str {
        match self {
            Self::Modularity => "modularity",
            Self::Acyclicity => "acyclicity",
            Self::Depth => "depth",
            Self::Equality => "equality",
            Self::Redundancy => "redundancy",
            Self::Tied => "tied",
        }
    }

    /// Suggested action prompt for AI agents.
    #[must_use]
    pub const fn action_hint(&self) -> &'static str {
        match self {
            Self::Modularity => "Refactor god files (high fan_out) and break hotspot dependencies",
            Self::Acyclicity => "Break circular dependencies (Tarjan SCC > 1)",
            Self::Depth => "Flatten dependency chains; introduce facade modules",
            Self::Equality => "Split god functions and large files; distribute complexity",
            Self::Redundancy => "Remove dead functions and consolidate duplicates",
            Self::Tied => "Balanced — choose any axis; prefer the one your domain values",
        }
    }
}

/// Per-root-cause diagnostics (Sentrux MCP `diagnostics` shape).
///
/// All sub-structs default to empty vectors so callers can construct
/// partial diagnostics without a separate builder.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Diagnostics {
    /// Modularity diagnostics: god_files, hotspots, most_unstable.
    pub modularity: ModularityDiagnostics,
    /// Acyclicity diagnostics: cycle paths.
    pub acyclicity: AcyclicityDiagnostics,
    /// Depth diagnostics: longest path witness.
    pub depth: DepthDiagnostics,
    /// Equality diagnostics: complex functions, large files.
    pub equality: EqualityDiagnostics,
    /// Redundancy diagnostics: dead functions, duplicate groups.
    pub redundancy: RedundancyDiagnostics,
}

/// Modularity diagnostics (god files, hotspots, instability).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModularityDiagnostics {
    /// Files with `fan_out > 10` (sources of dependency tangles).
    pub god_files: Vec<FileFanout>,
    /// Files with `fan_in > 15 && instability > 0.7` (bottlenecks).
    pub hotspot_files: Vec<FileFanin>,
    /// Top-10 files by Martin instability `I = Ce / (Ca + Ce)`.
    pub most_unstable: Vec<InstabilityEntry>,
}

/// Acyclicity diagnostics: discovered cycles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcyclicityDiagnostics {
    /// Each cycle is a list of file paths forming the SCC (size > 1).
    pub cycle_paths: Vec<Vec<String>>,
}

/// Depth diagnostics: a witness for the longest path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DepthDiagnostics {
    /// Concrete chain that produced `max_depth` (file paths in order).
    pub longest_path_witness: Vec<String>,
}

/// Equality diagnostics: complex functions and large files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EqualityDiagnostics {
    /// Functions with cyclomatic complexity > 15.
    pub complex_functions: Vec<FuncComplexity>,
    /// Files with > 500 lines (per default threshold).
    pub large_files: Vec<FileLines>,
}

/// Redundancy diagnostics: dead and duplicate code.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RedundancyDiagnostics {
    /// Public functions without consumers (orphans).
    pub dead_functions: Vec<FuncIdent>,
    /// Groups of functions with identical body hashes.
    pub duplicate_groups: Vec<DuplicateGroup>,
}

/// File path + fan_out value (used in god_files).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFanout {
    /// Repository-relative path.
    pub path: String,
    /// Outgoing import/call edges from this file.
    pub fan_out: usize,
}

/// File path + fan_in value (used in hotspot_files).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFanin {
    /// Repository-relative path.
    pub path: String,
    /// Incoming dependencies on this file.
    pub fan_in: usize,
    /// Martin instability `I = Ce / (Ca + Ce)` ∈ [0, 1].
    pub instability: f64,
}

/// Martin instability metric per file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstabilityEntry {
    /// Repository-relative path.
    pub path: String,
    /// `I = Ce / (Ca + Ce)`. 0 = stable, 1 = unstable.
    pub instability: f64,
    /// Afferent coupling (incoming).
    pub fan_in: usize,
    /// Efferent coupling (outgoing).
    pub fan_out: usize,
}

/// Function identity + cyclomatic complexity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuncComplexity {
    /// File containing the function.
    pub file: String,
    /// Function name (or qualified path).
    pub func: String,
    /// Cyclomatic complexity.
    pub cc: u32,
}

/// File path + line count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLines {
    /// Repository-relative path.
    pub path: String,
    /// Total source lines (including blanks/comments).
    pub lines: usize,
}

/// Function identity (file + name) — used for orphan/dead lists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuncIdent {
    /// File containing the function.
    pub file: String,
    /// Function name.
    pub func: String,
}

/// A group of functions sharing the same body hash (copy-paste duplicates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// blake3 (or similar) digest of the function body.
    pub hash: String,
    /// Concrete instances in the workspace.
    pub instances: Vec<FuncIdent>,
}

/// Workspace handle the public API operates on.
///
/// Built from a root path + collected import/call edges and per-fn metrics.
/// Construction is the responsibility of the caller (typically the CLI or
/// MCP tool) which already has access to indexers (touring-ast,
/// touring-wiring, touring-cognitive). Tests can construct synthetic
/// `Workspace` values directly.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    /// Repository root.
    pub root: std::path::PathBuf,
    /// Directed edges (from → to). Both import and call edges merged.
    pub edges: Vec<(String, String)>,
    /// Per-file fan-in/out (cached for diagnostics).
    pub file_fan_in: std::collections::HashMap<String, usize>,
    /// Per-file fan-out.
    pub file_fan_out: std::collections::HashMap<String, usize>,
    /// Per-file line counts.
    pub file_lines: std::collections::HashMap<String, usize>,
    /// Per-function cyclomatic complexity entries.
    pub function_cc: Vec<FuncComplexity>,
    /// Dead/orphan public functions reported by wiring layer.
    pub dead_functions: Vec<FuncIdent>,
    /// Duplicate function groups reported by dependency_cache.
    pub duplicate_groups: Vec<DuplicateGroup>,
}

impl Workspace {
    /// Build an empty workspace rooted at `root` for tests / spike usage.
    #[must_use]
    pub fn empty(root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            root: root.into(),
            ..Self::default()
        }
    }

    /// Total nodes referenced by edges (deduped union of from + to).
    #[must_use]
    pub fn node_count(&self) -> usize {
        let mut nodes: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (a, b) in &self.edges {
            nodes.insert(a.as_str());
            nodes.insert(b.as_str());
        }
        nodes.len()
    }

    /// Total directed edges (deduped).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        let set: std::collections::HashSet<&(String, String)> = self.edges.iter().collect();
        set.len()
    }

    /// Total functions surveyed.
    #[must_use]
    pub fn function_count(&self) -> usize {
        self.function_cc.len()
    }
}

mod serde_systemtime {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S: Serializer>(t: &SystemTime, ser: S) -> Result<S::Ok, S::Error> {
        let secs = t
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        secs.serialize(ser)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<SystemTime, D::Error> {
        let secs = f64::deserialize(de)?;
        if secs <= 0.0 {
            return Ok(UNIX_EPOCH);
        }
        Ok(UNIX_EPOCH + Duration::from_secs_f64(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_quality_signal_default_is_neutral() {
        let s = WorkspaceQualitySignal::default();
        assert_eq!(s.signal_0_10000, 5000);
        assert!((s.signal_normalized - 0.5).abs() < f64::EPSILON);
        assert_eq!(s.bottleneck, Bottleneck::Tied);
    }

    #[test]
    fn root_cause_scores_iter_labelled_canonical_order() {
        let labels: Vec<&str> = RootCauseScores::neutral()
            .iter_labelled()
            .iter()
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(
            labels,
            vec![
                "modularity",
                "acyclicity",
                "depth",
                "equality",
                "redundancy"
            ]
        );
    }

    #[test]
    fn bottleneck_action_hint_non_empty_for_all_variants() {
        for b in [
            Bottleneck::Modularity,
            Bottleneck::Acyclicity,
            Bottleneck::Depth,
            Bottleneck::Equality,
            Bottleneck::Redundancy,
            Bottleneck::Tied,
        ] {
            assert!(!b.action_hint().is_empty(), "missing hint for {:?}", b);
            assert!(!b.as_label().is_empty(), "missing label for {:?}", b);
        }
    }

    #[test]
    fn workspace_node_count_dedups_edges() {
        let mut ws = Workspace::empty("/tmp/x");
        ws.edges.push(("a.rs".to_string(), "b.rs".to_string()));
        ws.edges.push(("b.rs".to_string(), "c.rs".to_string()));
        assert_eq!(ws.node_count(), 3);
        assert_eq!(ws.edge_count(), 2);
    }

    #[test]
    fn diagnostics_serialization_round_trips() {
        let d = Diagnostics::default();
        let json = serde_json::to_string(&d).expect("serialize");
        let _back: Diagnostics = serde_json::from_str(&json).expect("deserialize");
    }
}
