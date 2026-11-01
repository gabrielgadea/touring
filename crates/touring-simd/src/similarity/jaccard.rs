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
    /// Jaccard of two SETS: |A ∩ B| / |A ∪ B|.
    ///
    /// 25/09/2026 (a d4, medido): a varredura rápida exigia entrada ordenada e
    /// sem repetição e NUNCA dizia isso — `[3,2,1]×[4,3,2]` respondia 0.0 em
    /// vez de 0.5, `[1,1,2]×[1,2]` 0.667 em vez de 1.0. Número errado em
    /// silêncio. O método agora canoniza por dentro: uma varredura O(n) toma o
    /// caminho zero-copy quando a entrada já está limpa; senão uma cópia
    /// ordenada e deduplicada (O(n log n)). O nome promete conjuntos — a
    /// resposta de conjunto É o contrato.
    fn jaccard(&self, a: &[u32], b: &[u32]) -> f64 {
        if a.is_empty() && b.is_empty() {
            return 1.0; // Empty sets are identical
        }
        if a.is_empty() || b.is_empty() {
            return 0.0; // One empty, one not = no overlap
        }

        let ca;
        let a = if is_sorted_dedup(a) {
            a
        } else {
            ca = canonicalized(a);
            &ca[..]
        };
        let cb;
        let b = if is_sorted_dedup(b) {
            b
        } else {
            cb = canonicalized(b);
            &cb[..]
        };

        // Use sorted set intersection - O(n + m) on canonical inputs
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
/// PRECONDITION (the name says it): both inputs sorted ascending AND free of
/// duplicates. The public [`JaccardComputer::jaccard`] enforces it by
/// canonicalizing; call this directly only with input already canonical.
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

/// True when the slice is sorted ascending AND duplicate-free — the
/// precondition of the fast walk.
fn is_sorted_dedup(s: &[u32]) -> bool {
    s.windows(2).all(|w| w[0] < w[1])
}

/// The canonical form: sorted ascending, duplicates removed (O(n log n)).
fn canonicalized(s: &[u32]) -> Vec<u32> {
    let mut v = s.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

/// Sort a slice of u32 for Jaccard computation.
///
/// Since 25/09/2026 `JaccardComputer::jaccard` canonicalizes internally, so
/// callers no longer need to prepare input; this helper remains for callers
/// that pre-prepare ONCE and reuse across many calls — note it only sorts,
/// so it is sufficient only for inputs already duplicate-free.
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

    // ── 25/09/2026 (a d4): the set contract, never the walk's precondition ──

    #[test]
    fn test_jaccard_unordered_and_repeated_inputs_answer_the_set() {
        let computer = JaccardComputer::new();
        // Os 3 contraexemplos medidos pela d4 (os valores ERRADOS de antes em
        // comentário):
        // [2,1]×[9,8,7,6,5,4,3,2,1] — {1,2} contra 9 elementos: 2/9 (dava 0.1)
        let a = vec![2, 1];
        let b = vec![9, 8, 7, 6, 5, 4, 3, 2, 1];
        assert!((computer.jaccard(&a, &b) - 2.0 / 9.0).abs() < 1e-10);
        // [3,2,1]×[4,3,2] — interseção {2,3}, união {1,2,3,4}: 0.5 (dava 0.0)
        let a = vec![3, 2, 1];
        let b = vec![4, 3, 2];
        assert_eq!(computer.jaccard(&a, &b), 0.5);
        // [1,1,2]×[1,2] — os DOIS conjuntos são {1,2}: 1.0 (dava 0.667)
        let a = vec![1, 1, 2];
        let b = vec![1, 2];
        assert_eq!(computer.jaccard(&a, &b), 1.0);
    }

    #[test]
    fn test_jaccard_matches_the_set_oracle_with_repetition_and_disorder() {
        // Oráculo ingênuo: conjuntos de verdade. Entradas com repetição E
        // desordem, semente fixa (reproduzível), 400 casos.
        fn oracle(a: &[u32], b: &[u32]) -> f64 {
            use std::collections::BTreeSet;
            let sa: BTreeSet<u32> = a.iter().copied().collect();
            let sb: BTreeSet<u32> = b.iter().copied().collect();
            let inter = sa.intersection(&sb).count();
            let union = sa.union(&sb).count();
            if union == 0 {
                return 1.0; // dois conjuntos vazios são idênticos
            }
            inter as f64 / union as f64
        }
        // PRNG determinístico (xorshift32), semente fixa.
        let mut state = 20260925u32;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let computer = JaccardComputer::new();
        for case in 0..400 {
            let la = (next() % 12) as usize;
            let lb = (next() % 12) as usize;
            let a: Vec<u32> = (0..la).map(|_| next() % 15).collect();
            let b: Vec<u32> = (0..lb).map(|_| next() % 15).collect();
            let got = computer.jaccard(&a, &b);
            let want = oracle(&a, &b);
            assert!(
                (got - want).abs() < 1e-10,
                "caso {case}: {a:?} × {b:?} — medido {got}, oráculo {want}"
            );
        }
    }
}
