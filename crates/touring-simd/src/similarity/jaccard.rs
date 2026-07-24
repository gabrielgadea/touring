//! Jaccard similarity for token sets.
//!
//! Pln3 Phase 1B - L2: Implementation
//! Uses sorted set intersection with O(n + m) complexity.

use super::traits::JaccardSimilarity;
use rayon::prelude::*;

/// Jaccard similarity computer with parallel batch support.
#[derive(Debug, Clone, Default)]
pub struct JaccardComputer {
    /// Minimum set size for parallel processing.
    parallel_threshold: usize,
}

impl JaccardComputer {
    /// Create new Jaccard computer with default settings.
    pub fn new() -> Self {
        Self {
            parallel_threshold: 100,
        }
    }

    /// Create with custom parallel threshold.
    pub fn with_threshold(parallel_threshold: usize) -> Self {
        Self { parallel_threshold }
    }
}

impl JaccardSimilarity for JaccardComputer {
    fn jaccard(&self, a: &[u32], b: &[u32]) -> f64 {
        if a.is_empty() && b.is_empty() {
            return 1.0; // Empty sets are identical
        }
        if a.is_empty() || b.is_empty() {
            return 0.0; // One empty, one not = no overlap
        }

        // Use sorted set intersection - O(n + m)
        let (intersection, union_size) = sorted_intersection_union_count(a, b);

        if union_size == 0 {
            return 0.0;
        }

        intersection as f64 / union_size as f64
    }

    fn jaccard_batch(&self, queries: &[Vec<u32>], candidates: &[Vec<u32>]) -> Vec<f64> {
        if queries.is_empty() || candidates.is_empty() {
            return vec![];
        }

        // Use parallel iteration for large batches
        let use_parallel = queries.len() * candidates.len() >= self.parallel_threshold;

        if use_parallel {
            queries
                .par_iter()
                .flat_map(|query| {
                    candidates
                        .par_iter()
                        .map(|candidate| self.jaccard(query, candidate))
                        .collect::<Vec<_>>()
                })
                .collect()
        } else {
            queries
                .iter()
                .flat_map(|query| {
                    candidates
                        .iter()
                        .map(|candidate| self.jaccard(query, candidate))
                        .collect::<Vec<_>>()
                })
                .collect()
        }
    }
}

/// Count intersection and union size for sorted sets.
///
/// Assumes inputs are sorted in ascending order.
#[inline]
#[allow(clippy::indexing_slicing)]
fn sorted_intersection_union_count(a: &[u32], b: &[u32]) -> (usize, usize) {
    let mut intersection = 0;
    let mut i = 0;
    let mut j = 0;

    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                intersection += 1;
                i += 1;
                j += 1;
            }
        }
    }

    // Union = |A| + |B| - |A ∩ B|
    let union_size = a.len() + b.len() - intersection;

    (intersection, union_size)
}

/// Sort a slice of u32 for Jaccard computation.
///
/// Must be called before passing tokens to `JaccardComputer::jaccard()`.
pub fn sort_tokens(tokens: &mut [u32]) {
    tokens.sort_unstable();
}

/// Compute Jaccard distance (1 - Jaccard similarity) between sorted sets.
///
/// Returns a value in [0, 1] where 0 = identical sets.
#[inline]
#[must_use]
pub fn jaccard_distance(a: &[u32], b: &[u32]) -> f64 {
    let computer = JaccardComputer::new();
    1.0 - computer.jaccard(a, b)
}

impl super::traits::Similarity<[u32]> for JaccardComputer {
    fn similarity(&self, a: &[u32], b: &[u32]) -> f64 {
        self.jaccard(a, b)
    }
}

impl super::traits::Similarity<Vec<u32>> for JaccardComputer {
    fn similarity(&self, a: &Vec<u32>, b: &Vec<u32>) -> f64 {
        self.jaccard(a, b)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs are asserted non-empty before indexing
    use super::*;

    #[test]
    fn test_jaccard_identical() {
        let computer = JaccardComputer::new();
        let a = vec![1, 2, 3, 4, 5];
        assert_eq!(computer.jaccard(&a, &a), 1.0);
    }

    #[test]
    fn test_jaccard_disjoint() {
        let computer = JaccardComputer::new();
        let a = vec![1, 2, 3];
        let b = vec![4, 5, 6];
        assert_eq!(computer.jaccard(&a, &b), 0.0);
    }

    #[test]
    fn test_jaccard_partial_overlap() {
        let computer = JaccardComputer::new();
        let a = vec![1, 2, 3, 4];
        let b = vec![3, 4, 5, 6];
        // Intersection: {3, 4} = 2
        // Union: {1, 2, 3, 4, 5, 6} = 6
        // Jaccard = 2/6 = 0.333...
        let result = computer.jaccard(&a, &b);
        assert!((result - 1.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_jaccard_empty_sets() {
        let computer = JaccardComputer::new();
        let empty: Vec<u32> = vec![];
        let non_empty = vec![1, 2, 3];

        assert_eq!(computer.jaccard(&empty, &empty), 1.0);
        assert_eq!(computer.jaccard(&empty, &non_empty), 0.0);
        assert_eq!(computer.jaccard(&non_empty, &empty), 0.0);
    }

    #[test]
    fn test_jaccard_batch() {
        let computer = JaccardComputer::new();
        let queries = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let candidates = vec![vec![1, 2, 3], vec![7, 8, 9]];

        let results = computer.jaccard_batch(&queries, &candidates);

        // Query 0 vs Candidate 0: identical = 1.0
        // Query 0 vs Candidate 1: disjoint = 0.0
        // Query 1 vs Candidate 0: disjoint = 0.0
        // Query 1 vs Candidate 1: disjoint = 0.0
        assert_eq!(results.len(), 4);
        assert_eq!(results[0], 1.0);
        assert_eq!(results[1], 0.0);
        assert_eq!(results[2], 0.0);
        assert_eq!(results[3], 0.0);
    }

    #[test]
    fn test_jaccard_distance() {
        let a = vec![1, 2, 3];
        let b = vec![4, 5, 6];
        assert_eq!(jaccard_distance(&a, &b), 1.0);
    }

    #[test]
    fn test_jaccard_distance_identical() {
        let a = vec![1, 2, 3];
        assert_eq!(jaccard_distance(&a, &a), 0.0);
    }

    #[test]
    fn test_similarity_trait_impl() {
        use crate::similarity::traits::Similarity;
        let computer = JaccardComputer::new();
        let a = vec![1u32, 2, 3];
        let b = vec![1u32, 2, 3];
        assert_eq!(computer.similarity(&a, &b), 1.0);
    }

    #[test]
    fn test_jaccard_batch_empty() {
        let computer = JaccardComputer::new();
        let empty: Vec<Vec<u32>> = vec![];
        let candidates = vec![vec![1u32, 2, 3]];
        assert!(computer.jaccard_batch(&empty, &candidates).is_empty());
        assert!(computer.jaccard_batch(&candidates, &empty).is_empty());
    }

    #[test]
    fn test_sort_tokens() {
        let mut tokens = vec![5, 3, 1, 4, 2];
        sort_tokens(&mut tokens);
        assert_eq!(tokens, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_jaccard_single_element() {
        let computer = JaccardComputer::new();
        let a = vec![1];
        let b = vec![1];
        assert_eq!(computer.jaccard(&a, &b), 1.0);
    }

    #[test]
    fn test_jaccard_subset() {
        let computer = JaccardComputer::new();
        let a = vec![1, 2, 3];
        let b = vec![1, 2, 3, 4, 5];
        // intersection = 3, union = 5, jaccard = 3/5 = 0.6
        let result = computer.jaccard(&a, &b);
        assert!((result - 0.6).abs() < 1e-10);
    }
}
