//! Reciprocal Rank Fusion (RRF) — combines ranked result lists.
//!
//! RRF formula: score(d) = Σ 1 / (k + rank(d))
//! where k is a constant (default 60) and rank(d) is the position in list i.

use std::collections::HashMap;

/// Reciprocal Rank Fusion combiner.
#[derive(Debug, Clone)]
pub struct RrfFusion {
    /// RRF k constant — controls how much rank differences matter.
    /// Higher k = less sensitive to rank order (more uniform distribution).
    pub k: f32,
}

impl RrfFusion {
    /// Creates a new RRF combiner with the given k constant.
    pub fn new(k: f32) -> Self {
        Self { k }
    }

    /// Default RRF with k=60 (standard in information retrieval literature).
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::new(60.0)
    }

    /// Computes the RRF score for a document appearing at the given rank.
    pub fn rrf_score(&self, rank: usize) -> f32 {
        if rank == 0 {
            return 0.0;
        }
        1.0 / (self.k + rank as f32)
    }

    /// Fuses multiple ranked lists into a single scored list.
    ///
    /// # Arguments
    /// * `lists` — A slice of slices of (doc_id, rank) pairs per list.
    /// * `weights` — Per-list weight multiplier (same length as lists).
    ///
    /// # Returns
    /// Vec of (doc_id, combined_score) sorted by descending score.
    pub fn fuse(&self, lists: &[&[(&str, usize)]], weights: &[f32]) -> Vec<(String, f32)> {
        let mut scores: HashMap<String, f32> = HashMap::new();

        for (list_idx, list) in lists.iter().enumerate() {
            let weight = weights.get(list_idx).copied().unwrap_or(1.0);
            for (doc_id, rank) in list.iter() {
                let contribution = weight * self.rrf_score(*rank);
                *scores.entry(doc_id.to_string()).or_insert(0.0) += contribution;
            }
        }

        let mut sorted: Vec<_> = scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted
    }

    /// Fuses two ranked lists with equal weight (1.0 each).
    pub fn fuse_two(
        &self,
        list_a: &[(&str, usize)],
        list_b: &[(&str, usize)],
    ) -> Vec<(String, f32)> {
        self.fuse(&[list_a, list_b], &[1.0, 1.0])
    }

    /// Fuses ranked lists with weights normalized to sum to 1.0.
    pub fn fuse_normalized(&self, lists: &[&[(&str, usize)]]) -> Vec<(String, f32)> {
        let total_weight: f32 = lists.len() as f32;
        let normalized: Vec<f32> = lists.iter().map(|_| 1.0 / total_weight).collect();
        self.fuse(lists, &normalized)
    }
}

impl Default for RrfFusion {
    fn default() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_score_rank1() {
        let fusion = RrfFusion::new(60.0);
        let score = fusion.rrf_score(1);
        assert!((score - 1.0 / 61.0).abs() < 1e-6);
    }

    #[test]
    fn test_rrf_score_rank10() {
        let fusion = RrfFusion::new(60.0);
        let score = fusion.rrf_score(10);
        assert!((score - 1.0 / 70.0).abs() < 1e-6);
    }

    #[test]
    fn test_rrf_score_rank0() {
        let fusion = RrfFusion::new(60.0);
        assert!(fusion.rrf_score(0).abs() < 1e-6);
    }

    #[test]
    fn test_rrf_fuse_two_lists() {
        let fusion = RrfFusion::new(60.0);
        let list_a = vec![("doc1", 1), ("doc2", 2), ("doc3", 3)];
        let list_b = vec![("doc2", 1), ("doc1", 2), ("doc4", 3)];

        let fused = fusion.fuse_two(&list_a, &list_b);

        // doc1: in list_a at rank 1 (1/61) + list_b at rank 2 (1/62)
        // doc2: in list_a at rank 2 (1/62) + list_b at rank 1 (1/61)
        // doc3: in list_a at rank 3 (1/63)
        // doc4: in list_b at rank 3 (1/63)
        assert_eq!(fused.len(), 4);
        // doc1 and doc2 should be tied or close — same total contribution
        let doc1_score = fused.iter().find(|(d, _)| *d == "doc1").unwrap().1;
        let doc2_score = fused.iter().find(|(d, _)| *d == "doc2").unwrap().1;
        assert!((doc1_score - doc2_score).abs() < 1e-4);
    }

    #[test]
    fn test_rrf_fuse_with_weights() {
        let fusion = RrfFusion::new(60.0);
        let list_a = vec![("doc1", 1), ("doc2", 2)];
        let list_b = vec![("doc1", 1), ("doc2", 2)];
        let weights = [2.0, 1.0];

        let fused = fusion.fuse(&[&list_a, &list_b], &weights);

        let doc1 = fused.iter().find(|(d, _)| *d == "doc1").unwrap();
        let doc2 = fused.iter().find(|(d, _)| *d == "doc2").unwrap();

        // doc1: 2*(1/61) + 1*(1/61) = 3/61
        // doc2: 2*(1/62) + 1*(1/62) = 3/62
        // doc1 should have higher score since rank 1 > rank 2
        assert!(doc1.1 > doc2.1);
    }

    #[test]
    fn test_rrf_fuse_empty() {
        let fusion = RrfFusion::new(60.0);
        let empty: Vec<(&str, usize)> = vec![];
        let fused = fusion.fuse(&[&empty], &[1.0]);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_rrf_default_k() {
        let fusion = RrfFusion::default();
        assert!((fusion.k - 60.0).abs() < 1e-6);
    }

    #[test]
    fn test_rrf_fuse_normalized() {
        let fusion = RrfFusion::new(60.0);
        let list_a = vec![("doc1", 1), ("doc2", 2)];
        let list_b = vec![("doc1", 1), ("doc2", 2)];
        let fused = fusion.fuse_normalized(&[&list_a, &list_b]);

        // Each list gets 0.5 weight (normalized)
        // doc1: 0.5/61 + 0.5/61 = 1/61
        // doc2: 0.5/62 + 0.5/62 = 1/62
        let doc1 = fused.iter().find(|(d, _)| *d == "doc1").unwrap();
        let doc2 = fused.iter().find(|(d, _)| *d == "doc2").unwrap();
        assert!(doc1.1 > doc2.1);
    }
}
