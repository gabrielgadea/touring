//! Workspace-level quality signal — Sentrux-inspired, gameproof, action-guiding.
//!
//! # What this crate computes
//!
//! Given a [`Workspace`] (paths + edges + per-fn metrics + dead/dup hints),
//! [`compute_quality_signal`] returns a single [`WorkspaceQualitySignal`]
//! containing:
//!
//! 1. `signal_0_10000` — integer score on the Sentrux scale `[0, 10000]`,
//!    computed as the **geometric mean** of five normalized root-cause
//!    scores × 10000. Gameproof by Nash-1950: improving one root cause at
//!    the cost of another never raises the geometric mean.
//! 2. `signal_normalized` — same value in `[0.0, 1.0]` (Touring scale).
//! 3. `bottleneck` — single-string label of the lowest-scoring root cause,
//!    designed to drive the AI agent's next action via gradient descent.
//! 4. `root_causes` — five normalized scores: modularity, acyclicity, depth,
//!    equality, redundancy.
//! 5. `raw` — un-normalized values for display.
//! 6. `diagnostics` — per-root-cause god files / hotspots / dead /
//!    duplicates / complex functions / large files.
//!
//! # Reuse
//!
//! Touring already has 60% of the necessary infrastructure:
//! * Tarjan SCC: `petgraph::algo::tarjan_scc` (consumed by `acyclicity`)
//! * Leiden community detection: `LeidenCommunityDetector` in
//!   `crates/touring-hooks/src/shared/leiden.rs` (referenced as the
//!   higher-fidelity option for `modularity`; the simple Newman-Q
//!   formulation here uses directory-prefix modules and is sufficient).
//! * Call graph: `build_call_graph` in `crates/touring-ast/src/call_graph.rs`
//!   (caller populates `Workspace::edges` from this).
//!
//! # Public API surface
//!
//! ```ignore
//! use touring_quality_signal::{compute_quality_signal, Workspace};
//! let ws = Workspace::empty("/path/to/project");
//! let signal = compute_quality_signal(&ws);
//! println!("{}", signal.signal_0_10000);
//! println!("{}", signal.bottleneck.action_hint());
//! ```

pub mod acyclicity;
pub mod aggregate;
pub mod depth;
pub mod diagnostics;
pub mod diff;
pub mod equality;
pub mod error;
pub mod modularity;
pub mod redundancy;
pub mod types;
pub mod workspace_io;

pub use diff::{
    DEFAULT_TREND_EPSILON, RootCauseDeltas, SignalDiff, SignalTrend, diff_signals,
    diff_signals_with_epsilon,
};
pub use error::{Error, Result};
pub use types::{
    AcyclicityDiagnostics, Bottleneck, DepthDiagnostics, Diagnostics, DuplicateGroup,
    EqualityDiagnostics, FileFanin, FileFanout, FileLines, FuncComplexity, FuncIdent,
    InstabilityEntry, ModularityDiagnostics, RedundancyDiagnostics, RootCauseRaw, RootCauseScores,
    Workspace, WorkspaceQualitySignal,
};
pub use workspace_io::{WorkspaceIoError, build_workspace_from_path};

use std::time::SystemTime;

/// Compute the workspace quality signal from the populated [`Workspace`].
///
/// This is a pure function — all its inputs come from `ws`. The caller is
/// responsible for collecting edges, per-fn complexity, dead/duplicate
/// lists, etc. (typically using touring CLI / hooks). Tests can construct
/// synthetic workspaces directly via [`Workspace::empty`] and pushing
/// values into the public fields.
#[must_use]
pub fn compute_quality_signal(ws: &Workspace) -> WorkspaceQualitySignal {
    let modularity_q = modularity::compute_modularity_q(ws);
    let cycle_count = acyclicity::compute_cycle_count(ws);
    let max_depth = depth::compute_max_depth(ws);
    let complexity_gini = equality::compute_complexity_gini(ws);
    let redundancy_ratio = redundancy::compute_redundancy_ratio(ws);

    let raw = RootCauseRaw {
        modularity_q,
        cycle_count,
        max_depth,
        complexity_gini,
        redundancy_ratio,
        total_functions: ws.function_count(),
        total_nodes: ws.node_count(),
        total_edges: ws.edge_count(),
    };

    let scores = RootCauseScores {
        modularity: modularity::normalize_modularity(modularity_q),
        acyclicity: acyclicity::acyclicity_score(cycle_count),
        depth: depth::depth_score(max_depth),
        equality: equality::equality_score(complexity_gini),
        redundancy: redundancy::redundancy_score(redundancy_ratio),
    };

    let signal_normalized = aggregate::aggregate_geometric_mean(&scores);
    let signal_0_10000 = aggregate::aggregate_geometric_mean_int10k(&scores);
    let bottleneck = aggregate::detect_bottleneck(&scores);
    let diagnostics = diagnostics::collect_diagnostics(ws);

    WorkspaceQualitySignal {
        signal_0_10000,
        signal_normalized,
        bottleneck,
        root_causes: scores,
        raw,
        diagnostics,
        computed_at: SystemTime::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_workspace_returns_perfect_signal() {
        // Empty graph: Q=1.0, cycles=0, depth=0, no functions → gini fallback 0,
        // redundancy ratio 0. All scores 1.0 → integer signal 10000.
        let ws = Workspace::empty("/tmp/empty");
        let signal = compute_quality_signal(&ws);
        assert_eq!(signal.signal_0_10000, 10_000);
        assert_eq!(signal.bottleneck, Bottleneck::Tied);
    }

    #[test]
    fn synthetic_workspace_produces_sane_signal_range() {
        let mut ws = Workspace::empty("/tmp/synth");
        for i in 0..5 {
            ws.edges.push((format!("a/x{i}.rs"), format!("a/y{i}.rs")));
            ws.edges.push((format!("b/x{i}.rs"), format!("b/y{i}.rs")));
        }
        ws.function_cc = (0..30)
            .map(|i| FuncComplexity {
                file: format!("a/x{i}.rs"),
                func: format!("f{i}"),
                cc: if i % 10 == 0 { 25 } else { 5 },
            })
            .collect();
        let signal = compute_quality_signal(&ws);
        assert!(
            signal.signal_0_10000 >= 1000 && signal.signal_0_10000 <= 10_000,
            "synthetic workspace signal out of sane range: {}",
            signal.signal_0_10000
        );
    }

    #[test]
    fn bottleneck_action_hint_resolves() {
        let ws = Workspace::empty("/tmp/bn");
        let signal = compute_quality_signal(&ws);
        assert!(!signal.bottleneck.action_hint().is_empty());
    }

    #[test]
    fn json_serialization_includes_required_fields() {
        let ws = Workspace::empty("/tmp/ser");
        let signal = compute_quality_signal(&ws);
        let json = serde_json::to_value(&signal).expect("serialize");
        assert!(json.get("signal_0_10000").is_some());
        assert!(json.get("bottleneck").is_some());
        assert!(json.get("root_causes").is_some());
        assert!(json.get("diagnostics").is_some());
    }
}
