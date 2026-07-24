//! Newman's Modularity Q for the dependency graph.
//!
//! Implements the directed-graph variant from Newman 2004:
//!
//! ```text
//! Q = (1/m) * Σ_ij [A_ij - (k_out_i * k_in_j) / m] * δ(c_i, c_j)
//! ```
//!
//! Modules (communities) are derived from the **directory prefix** of each
//! file path (Sentrux convention: `src/foo/bar.rs` belongs to module `src/foo`).
//! This avoids re-running an expensive community detection algorithm just to
//! compute a single number; the workspace's directory layout already encodes
//! a meaningful module assignment.
//!
//! Rationale: implementing community detection here would duplicate the work
//! of the existing `LeidenCommunityDetector` in
//! `crates/touring-hooks/src/shared/leiden.rs`. We take the simpler — and for
//! Q-on-the-real-graph faster — path of using directories as modules. The
//! resulting Q is consistent with how Sentrux derives Q in
//! `sentrux-core/src/metrics/root_causes.rs::compute_modularity_q`.

use std::collections::{HashMap, HashSet};

use super::types::Workspace;

/// Compute Newman's Q over the merged import + call graph of `ws`.
///
/// * `Q > 0.3` indicates significant modular structure.
/// * `Q > 0.6` indicates strong modular structure.
/// * `Q ≤ 0` indicates worse-than-random (anti-modular) structure.
///
/// Returns `1.0` for an empty graph (trivially modular: nothing connects).
#[must_use]
pub fn compute_modularity_q(ws: &Workspace) -> f64 {
    let edges: HashSet<(&str, &str)> = ws
        .edges
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();

    let m = edges.len();
    if m == 0 {
        return 1.0;
    }

    let mut k_out: HashMap<&str, usize> = HashMap::new();
    let mut k_in: HashMap<&str, usize> = HashMap::new();
    let mut all_nodes: HashSet<&str> = HashSet::new();
    for &(from, to) in &edges {
        *k_out.entry(from).or_default() += 1;
        *k_in.entry(to).or_default() += 1;
        all_nodes.insert(from);
        all_nodes.insert(to);
    }

    let m_f = m as f64;

    let mut intra: usize = 0;
    for &(from, to) in &edges {
        if module_of(from) == module_of(to) {
            intra += 1;
        }
    }

    let mut mod_k_out_sum: HashMap<&str, f64> = HashMap::new();
    let mut mod_k_in_sum: HashMap<&str, f64> = HashMap::new();
    for &node in &all_nodes {
        let m_node = module_of(node);
        let ko = *k_out.get(node).unwrap_or(&0) as f64;
        let ki = *k_in.get(node).unwrap_or(&0) as f64;
        *mod_k_out_sum.entry(m_node).or_default() += ko;
        *mod_k_in_sum.entry(m_node).or_default() += ki;
    }

    let mut expected_intra = 0.0_f64;
    for (module, &ko) in &mod_k_out_sum {
        let ki = mod_k_in_sum.get(module).copied().unwrap_or(0.0);
        expected_intra += ko * ki / m_f;
    }

    let q = (intra as f64 - expected_intra) / m_f;
    q.clamp(-0.5, 1.0)
}

/// Normalize Newman Q from `[-0.5, 1.0]` to `[0.0, 1.0]`.
///
/// Formula: `score = (q + 0.5) / 1.5` (Sentrux convention, no arbitrary params).
#[must_use]
pub fn normalize_modularity(q: f64) -> f64 {
    ((q + 0.5) / 1.5).clamp(0.0, 1.0)
}

/// Derive a module label from a file path: the parent directory.
///
/// `src/foo/bar.rs` → `src/foo`. Files at the root are assigned the empty
/// module — these still participate in Q because two root-level files share
/// the same module ("").
#[must_use]
pub fn module_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws_from(edges: &[(&str, &str)]) -> Workspace {
        let mut ws = Workspace::empty("/tmp/test");
        ws.edges = edges
            .iter()
            .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
            .collect();
        ws
    }

    #[test]
    fn empty_graph_is_trivially_modular() {
        let ws = Workspace::empty("/tmp");
        assert!((compute_modularity_q(&ws) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn perfect_modular_graph_high_q() {
        // Three disjoint clusters — all edges intra-module → Q close to 1.
        let edges = [
            ("a/x.rs", "a/y.rs"),
            ("a/y.rs", "a/z.rs"),
            ("b/x.rs", "b/y.rs"),
            ("b/y.rs", "b/z.rs"),
            ("c/x.rs", "c/y.rs"),
        ];
        let ws = ws_from(&edges);
        let q = compute_modularity_q(&ws);
        assert!(q > 0.5, "expected highly-modular graph Q > 0.5, got {q}");
    }

    #[test]
    fn anti_modular_graph_negative_q() {
        // All edges cross-module — Q must be negative or near zero.
        let edges = [
            ("a/x.rs", "b/x.rs"),
            ("a/y.rs", "c/y.rs"),
            ("b/x.rs", "c/y.rs"),
            ("b/y.rs", "a/x.rs"),
            ("c/x.rs", "a/y.rs"),
        ];
        let ws = ws_from(&edges);
        let q = compute_modularity_q(&ws);
        assert!(q < 0.1, "expected anti-modular graph Q < 0.1, got {q}");
    }

    #[test]
    fn normalize_endpoints_correct() {
        assert!((normalize_modularity(-0.5) - 0.0).abs() < f64::EPSILON);
        assert!((normalize_modularity(1.0) - 1.0).abs() < f64::EPSILON);
        assert!((normalize_modularity(0.0) - 0.333_333_3).abs() < 1e-5);
    }

    #[test]
    fn module_of_basic() {
        assert_eq!(module_of("src/foo/bar.rs"), "src/foo");
        assert_eq!(module_of("foo.rs"), "");
        assert_eq!(module_of("a/b/c/d.rs"), "a/b/c");
    }
}
