//! C14 — consistency gate for parallel engineers (TACO FASE 6).
//!
//! When two or more engineers edit in parallel, their outputs must be reconciled
//! before merge. C14 scores how far apart two produced ASTs are with a
//! **graph-edit-distance (GED)** term plus a semantic **cosine** term:
//!
//! ```text
//! distance(A, B) = GED_norm(A, B) + α · (1 − cos(embedding_A, embedding_B))
//! ```
//!
//! and gates the merge: `consistent` iff `distance ≤ threshold`. A high distance means
//! the two engineers diverged structurally (different nodes/edges) and/or semantically
//! (different embeddings) — the merge must be arbitrated rather than blindly applied.
//!
//! Exact GED is NP-hard, so [`approx_ged`] uses the standard alignment-free relaxation:
//! the symmetric difference of the node-label multisets plus the symmetric difference
//! of the (label, label) edge multisets — an `O(n + m)` lower bound that is exact when
//! the two graphs share a node alignment. The gate is **pure** and unit-tested here.

use std::collections::HashMap;

/// A minimal labelled graph over an engineer's produced AST: each node carries a
/// string label (e.g. `"fn:foo"`, `"let"`, kind+name), edges are `(from, to)` index
/// pairs into [`LabeledGraph::node_labels`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabeledGraph {
    /// Node labels, indexed by position.
    pub node_labels: Vec<String>,
    /// Directed edges as `(from_index, to_index)` into `node_labels`.
    pub edges: Vec<(usize, usize)>,
}

impl LabeledGraph {
    /// A new graph from labels + edges.
    #[must_use]
    pub fn new(node_labels: Vec<String>, edges: Vec<(usize, usize)>) -> Self {
        Self { node_labels, edges }
    }

    /// Graph size = nodes + edges (the GED normalization base).
    #[must_use]
    pub fn size(&self) -> usize {
        self.node_labels.len() + self.edges.len()
    }

    /// Edges rendered as `(from_label, to_label)` pairs — the alignment-free edge
    /// representation. Out-of-range endpoints render as `"?"` (defensive).
    fn edge_label_pairs(&self) -> Vec<(String, String)> {
        let label = |i: usize| {
            self.node_labels
                .get(i)
                .cloned()
                .unwrap_or_else(|| "?".into())
        };
        self.edges
            .iter()
            .map(|&(a, b)| (label(a), label(b)))
            .collect()
    }
}

/// Cardinality of the symmetric difference of two multisets of `T` — `Σ_k |c_a(k) − c_b(k)|`.
fn multiset_sym_diff<T: std::hash::Hash + Eq>(a: &[T], b: &[T]) -> usize {
    let mut counts: HashMap<&T, i64> = HashMap::new();
    for x in a {
        *counts.entry(x).or_insert(0) += 1;
    }
    for x in b {
        *counts.entry(x).or_insert(0) -= 1;
    }
    counts.values().map(|c| c.unsigned_abs() as usize).sum()
}

/// Alignment-free approximate graph-edit-distance: node-label multiset symmetric
/// difference + edge `(label, label)` multiset symmetric difference. `O(n + m)`.
#[must_use]
pub fn approx_ged(a: &LabeledGraph, b: &LabeledGraph) -> usize {
    let node_diff = multiset_sym_diff(&a.node_labels, &b.node_labels);
    let edge_diff = multiset_sym_diff(&a.edge_label_pairs(), &b.edge_label_pairs());
    node_diff + edge_diff
}

/// Cosine similarity of two equal-length vectors, in `[-1, 1]`. Returns `1.0` (treated
/// as "no divergence signal") when either vector is empty, length-mismatched, or
/// zero-norm — so a missing embedding never falsely widens the distance.
#[must_use]
pub fn cosine_similarity(x: &[f32], y: &[f32]) -> f64 {
    if x.is_empty() || x.len() != y.len() {
        return 1.0;
    }
    let dot: f64 = x
        .iter()
        .zip(y)
        .map(|(a, b)| f64::from(*a) * f64::from(*b))
        .sum();
    let nx: f64 = x
        .iter()
        .map(|a| f64::from(*a) * f64::from(*a))
        .sum::<f64>()
        .sqrt();
    let ny: f64 = y
        .iter()
        .map(|a| f64::from(*a) * f64::from(*a))
        .sum::<f64>()
        .sqrt();
    if nx == 0.0 || ny == 0.0 {
        return 1.0;
    }
    dot / (nx * ny)
}

/// The verdict of the consistency gate.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsistencyVerdict {
    /// Raw approximate graph-edit-distance.
    pub ged: usize,
    /// Cosine similarity of the two embeddings (`1.0` when none supplied).
    pub cosine_sim: f64,
    /// Combined distance `GED_norm + α·(1 − cos)` in `[0, 1+α]`.
    pub distance: f64,
    /// `true` when `distance ≤ threshold` — the merge may proceed without arbitration.
    pub consistent: bool,
}

/// Gate a parallel-engineer merge: combine the structural GED and the semantic cosine
/// term into a single distance and compare it to `threshold`. `alpha` weights the
/// semantic term; `emb_a`/`emb_b` are optional engineer-output embeddings (cosine is
/// `1.0` — no penalty — when either is absent).
#[must_use]
pub fn consistency_gate(
    a: &LabeledGraph,
    b: &LabeledGraph,
    emb_a: Option<&[f32]>,
    emb_b: Option<&[f32]>,
    alpha: f64,
    threshold: f64,
) -> ConsistencyVerdict {
    let ged = approx_ged(a, b);
    let base = (a.size() + b.size()).max(1);
    let ged_norm = ged as f64 / base as f64;
    let cosine_sim = match (emb_a, emb_b) {
        (Some(x), Some(y)) => cosine_similarity(x, y),
        _ => 1.0,
    };
    let distance = ged_norm + alpha * (1.0 - cosine_sim);
    ConsistencyVerdict {
        ged,
        cosine_sim,
        distance,
        consistent: distance <= threshold,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(labels: &[&str], edges: &[(usize, usize)]) -> LabeledGraph {
        LabeledGraph::new(
            labels.iter().map(|s| (*s).to_string()).collect(),
            edges.to_vec(),
        )
    }

    #[test]
    fn identical_graphs_have_zero_ged_and_are_consistent() {
        let a = g(&["fn:foo", "let", "ret"], &[(0, 1), (1, 2)]);
        let b = a.clone();
        let v = consistency_gate(&a, &b, None, None, 0.5, 0.2);
        assert_eq!(v.ged, 0);
        assert_eq!(v.distance, 0.0);
        assert!(v.consistent);
    }

    #[test]
    fn extra_node_and_edge_raise_the_ged() {
        let a = g(&["fn:foo", "let"], &[(0, 1)]);
        let b = g(&["fn:foo", "let", "ret"], &[(0, 1), (1, 2)]);
        // node multiset diff = {ret} = 1; edge pair diff = {(let,ret)} = 1.
        assert_eq!(approx_ged(&a, &b), 2);
    }

    #[test]
    fn divergent_graphs_fail_the_gate() {
        let a = g(&["fn:foo", "let"], &[(0, 1)]);
        let b = g(&["fn:bar", "match", "arm"], &[(0, 1), (0, 2)]);
        let v = consistency_gate(&a, &b, None, None, 0.5, 0.2);
        assert!(v.ged > 0);
        assert!(
            !v.consistent,
            "structurally divergent merge must be gated, dist={}",
            v.distance
        );
    }

    #[test]
    fn cosine_term_penalizes_semantic_divergence() {
        // Structurally identical (GED 0) but opposite embeddings → cosine pulls the
        // distance above the threshold.
        let a = g(&["x"], &[]);
        let b = g(&["x"], &[]);
        let v = consistency_gate(&a, &b, Some(&[1.0, 0.0]), Some(&[-1.0, 0.0]), 0.5, 0.2);
        assert_eq!(v.ged, 0);
        assert!((v.cosine_sim - (-1.0)).abs() < 1e-9);
        assert!((v.distance - 1.0).abs() < 1e-9); // 0 + 0.5*(1 - (-1)) = 1.0
        assert!(!v.consistent);
    }

    #[test]
    fn missing_embedding_does_not_penalize() {
        let a = g(&["x"], &[]);
        let b = g(&["x"], &[]);
        let v = consistency_gate(&a, &b, Some(&[1.0, 2.0]), None, 0.5, 0.2);
        assert!((v.cosine_sim - 1.0).abs() < 1e-9);
        assert_eq!(v.distance, 0.0);
        assert!(v.consistent);
    }

    #[test]
    fn cosine_self_similarity_is_one() {
        assert!((cosine_similarity(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]) - 1.0).abs() < 1e-9);
        assert_eq!(cosine_similarity(&[], &[]), 1.0); // empty → no signal
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 1.0); // mismatched → no signal
    }

    #[test]
    fn multiset_diff_counts_repeats() {
        // Two `let` in A, one in B → symmetric difference 1.
        let a = g(&["let", "let", "fn"], &[]);
        let b = g(&["let", "fn"], &[]);
        assert_eq!(approx_ged(&a, &b), 1);
    }
}
