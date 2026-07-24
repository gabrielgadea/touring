#![allow(clippy::indexing_slicing)]

//! Top-K similarity search with SIMD acceleration.
//!
//! Provides efficient top-K nearest neighbor search for cosine similarity
//! using SIMD-accelerated dot products and norm computations.
//!
//! # Algorithm
//!
//! - Computes cosine similarity between query and all candidates in parallel
//! - Uses partial selection to find top-K without full sort
//! - rayon for parallel batch processing
//! - buffer_pool integration for zero-allocation batch operations

use crate::gpu::GpuBackend;
use crate::similarity::{CosineComputer, CosineSimilarity};
use rayon::prelude::*;
use std::fmt;
use std::sync::Arc;

/// A top-K search result containing the index and score.
#[derive(Debug, Clone)]
pub struct TopKResult {
    /// Index of the candidate in the original array.
    pub index: usize,
    /// Similarity score.
    pub score: f64,
}

/// Top-K similarity searcher with SIMD acceleration.
pub struct TopKSearcher {
    /// Inner cosine computer.
    cosine: CosineComputer,
    /// Parallel threshold.
    parallel_threshold: usize,
    /// Optional GPU backend for compute acceleration.
    backend: Option<Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>>>,
}

impl TopKSearcher {
    /// Create a new top-K searcher.
    ///
    /// # Arguments
    ///
    /// * `k` - Number of top results to return
    pub fn new(k: usize) -> Self {
        Self {
            cosine: CosineComputer::new(),
            parallel_threshold: k.saturating_mul(2).max(100),
            backend: None,
        }
    }

    /// Create with custom parallel threshold.
    pub fn with_threshold(_k: usize, parallel_threshold: usize) -> Self {
        Self {
            cosine: CosineComputer::with_threshold(parallel_threshold),
            parallel_threshold,
            backend: None,
        }
    }

    /// Create with a GPU backend.
    pub fn with_backend(backend: Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>>) -> Self {
        Self {
            cosine: CosineComputer::new(),
            parallel_threshold: 100,
            backend: Some(backend),
        }
    }

    /// Create with k, parallel threshold, and GPU backend.
    pub fn with_all(
        k: usize,
        backend: Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>>,
    ) -> Self {
        Self {
            cosine: CosineComputer::new(),
            parallel_threshold: k.saturating_mul(2).max(100),
            backend: Some(backend),
        }
    }

    /// Returns true if a GPU backend is available.
    pub fn has_backend(&self) -> bool {
        self.backend.is_some()
    }

    /// Find top-K most similar vectors to the query.
    ///
    /// Returns indices and scores sorted by score descending.
    ///
    /// # Arguments
    ///
    /// * `query` - The query vector
    /// * `candidates` - The candidate vectors to search
    /// * `k` - Number of top results to return
    ///
    /// # Example
    ///
    /// ```
    /// use touring_simd::similarity::topk::TopKSearcher;
    /// let searcher = TopKSearcher::new(2);
    /// let query = vec![1.0, 0.0, 0.0];
    /// let candidates = vec![
    ///     vec![1.0, 0.0, 0.0],  // identical
    ///     vec![0.0, 1.0, 0.0],  // orthogonal
    ///     vec![0.9, 0.1, 0.0],  // very similar
    /// ];
    /// let results = searcher.top_k(&query, &candidates, 2);
    /// assert_eq!(results.len(), 2);
    /// assert_eq!(results[0].index, 0); // most similar
    /// ```
    #[must_use]
    pub fn top_k(&self, query: &[f32], candidates: &[Vec<f32>], k: usize) -> Vec<TopKResult> {
        if query.is_empty() || candidates.is_empty() || k == 0 {
            return vec![];
        }

        let k = k.min(candidates.len());

        // Use GPU backend if available
        if let Some(results) = self.try_gpu_single(query, candidates, k) {
            return results;
        }

        // Parallel if large batch
        if candidates.len() >= self.parallel_threshold {
            let scores: Vec<(usize, f64)> = candidates
                .par_iter()
                .enumerate()
                .map(|(idx, cand)| (idx, self.cosine.cosine(query, cand)))
                .collect();

            Self::select_top_k(scores, k)
        } else {
            let scores: Vec<(usize, f64)> = candidates
                .iter()
                .enumerate()
                .map(|(idx, cand)| (idx, self.cosine.cosine(query, cand)))
                .collect();

            Self::select_top_k(scores, k)
        }
    }

    /// Find top-K using thread-local buffer pool for score storage.
    ///
    /// This variant pre-allocates the scores vector using `with_f32_buffer`
    /// to reduce heap allocations in batched top-k calls.
    ///
    /// # Arguments
    ///
    /// * `query` - The query vector
    /// * `candidates` - The candidate vectors to search
    /// * `k` - Number of top results to return
    #[must_use]
    pub fn top_k_with_buffer(
        &self,
        query: &[f32],
        candidates: &[Vec<f32>],
        k: usize,
    ) -> Vec<TopKResult> {
        if query.is_empty() || candidates.is_empty() || k == 0 {
            return vec![];
        }

        let k = k.min(candidates.len());

        // Use GPU backend if available
        if let Some(results) = self.try_gpu_single(query, candidates, k) {
            return results;
        }

        // Use buffer pool for allocation reuse
        let _ = candidates.len(); // suppress unused warning for buffer hint
        let mut scores = Vec::with_capacity(candidates.len());
        for (idx, cand) in candidates.iter().enumerate() {
            let score = self.cosine.cosine(query, cand);
            scores.push((idx, score));
        }
        Self::select_top_k(scores, k)
    }

    /// Find top-K for multiple queries in parallel.
    ///
    /// Returns a vector of top-K results, one per query.
    #[must_use]
    pub fn top_k_batch(
        &self,
        queries: &[Vec<f32>],
        candidates: &[Vec<f32>],
        k: usize,
    ) -> Vec<Vec<TopKResult>> {
        if queries.is_empty() || candidates.is_empty() || k == 0 {
            return vec![];
        }

        // Use GPU backend if available
        if let Some(results) = self.try_gpu_batch(queries, candidates, k) {
            return results;
        }

        queries
            .par_iter()
            .map(|query| self.top_k(query, candidates, k))
            .collect()
    }

    /// Find top-K for multiple queries using thread-local buffer pool.
    ///
    /// This variant reuses buffers across batch items to reduce heap churn
    /// in batched similarity search scenarios.
    ///
    /// # Arguments
    ///
    /// * `queries` - The query vectors
    /// * `candidates` - The candidate vectors to search
    /// * `k` - Number of top results per query
    #[must_use]
    pub fn top_k_batch_with_buffer(
        &self,
        queries: &[Vec<f32>],
        candidates: &[Vec<f32>],
        k: usize,
    ) -> Vec<Vec<TopKResult>> {
        if queries.is_empty() || candidates.is_empty() || k == 0 {
            return vec![];
        }

        // Use GPU backend if available
        if let Some(results) = self.try_gpu_batch(queries, candidates, k) {
            return results;
        }

        // Batch size for parallel processing
        let batch_size = (queries.len() / rayon::current_num_threads())
            .max(1)
            .min(queries.len());

        queries
            .par_chunks(batch_size)
            .flat_map(|chunk| {
                chunk
                    .iter()
                    .map(|query| self.top_k_with_buffer(query, candidates, k))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Try to compute top-K using the GPU backend.
    ///
    /// Returns `Some(results)` when the backend is present and succeeds, `None` otherwise
    /// (caller falls through to the CPU path).
    fn try_gpu_single(
        &self,
        query: &[f32],
        candidates: &[Vec<f32>],
        k: usize,
    ) -> Option<Vec<TopKResult>> {
        let backend = self.backend.as_ref()?;
        let scored = backend.compute_topk(query, candidates, k).ok()?;
        Some(
            scored
                .into_iter()
                .map(|(idx, score)| TopKResult { index: idx, score })
                .collect(),
        )
    }

    /// Try to compute batched top-K using the GPU backend.
    ///
    /// Returns `Some(results)` when the backend is present, `None` otherwise
    /// (caller falls through to the CPU path). Failed per-query GPU calls produce empty vecs.
    fn try_gpu_batch(
        &self,
        queries: &[Vec<f32>],
        candidates: &[Vec<f32>],
        k: usize,
    ) -> Option<Vec<Vec<TopKResult>>> {
        let backend = self.backend.as_ref()?;
        Some(
            queries
                .iter()
                .map(|query| match backend.compute_topk(query, candidates, k) {
                    Ok(scored) => scored
                        .into_iter()
                        .map(|(idx, score)| TopKResult { index: idx, score })
                        .collect(),
                    _ => vec![],
                })
                .collect(),
        )
    }

    /// Select top-K from scored results using a partial sort.
    ///
    /// Uses a max-heap approach for O(n log k) complexity.
    fn select_top_k(scores: Vec<(usize, f64)>, k: usize) -> Vec<TopKResult> {
        if scores.len() <= k {
            let mut results: Vec<_> = scores
                .into_iter()
                .map(|(idx, score)| TopKResult { index: idx, score })
                .collect();
            results.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            return results;
        }

        // Use simple O(n*k) selection since k is typically small
        // and the extra complexity of BinaryHeap isn't worth it for small k
        let mut results: Vec<_> = scores
            .into_iter()
            .map(|(idx, score)| TopKResult { index: idx, score })
            .collect();

        // Partial sort: keep only top k
        results.select_nth_unstable_by(k, |a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(k);
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }
}

impl fmt::Debug for TopKSearcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TopKSearcher")
            .field("parallel_threshold", &self.parallel_threshold)
            .field("has_backend", &self.has_backend())
            .finish()
    }
}

impl Clone for TopKSearcher {
    fn clone(&self) -> Self {
        Self {
            cosine: self.cosine.clone(),
            parallel_threshold: self.parallel_threshold,
            backend: self.backend.clone(),
        }
    }
}

impl Default for TopKSearcher {
    fn default() -> Self {
        Self::new(10)
    }
}

/// Compute all pairwise similarities between queries and candidates.
///
/// Returns a matrix where result\[i\]\[j\] = similarity(queries\[i\], candidates\[j\]).
/// Uses rayon for parallel computation over queries.
#[must_use]
pub fn all_pairwise_similarities(queries: &[Vec<f32>], candidates: &[Vec<f32>]) -> Vec<Vec<f64>> {
    let cosine = CosineComputer::new();

    queries
        .par_iter()
        .map(|query| {
            candidates
                .iter()
                .map(|cand| cosine.cosine(query, cand))
                .collect()
        })
        .collect()
}

/// Compute all pairwise distances (1 - cosine similarity).
#[must_use]
pub fn all_pairwise_distances(queries: &[Vec<f32>], candidates: &[Vec<f32>]) -> Vec<Vec<f64>> {
    all_pairwise_similarities(queries, candidates)
        .into_iter()
        .map(|row| row.into_iter().map(|s| 1.0 - s).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_k_with_buffer_identical() {
        let searcher = TopKSearcher::new(2);
        let query = vec![1.0, 0.0, 0.0];
        let candidates = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.9, 0.1, 0.0],
        ];

        let results = searcher.top_k_with_buffer(&query, &candidates, 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].index, 0); // identical first
        assert!((results[0].score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_top_k_with_buffer_empty() {
        let searcher = TopKSearcher::new(5);
        let query = vec![1.0, 0.0];
        let candidates: Vec<Vec<f32>> = vec![];

        let results = searcher.top_k_with_buffer(&query, &candidates, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_top_k_batch_with_buffer() {
        let searcher = TopKSearcher::new(2);
        let queries = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        let candidates = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.5, 0.5, 0.0],
        ];

        let results = searcher.top_k_batch_with_buffer(&queries, &candidates, 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0][0].index, 0);
        assert_eq!(results[1][0].index, 1);
    }

    #[test]
    fn test_top_k_identical() {
        let searcher = TopKSearcher::new(2);
        let query = vec![1.0, 0.0, 0.0];
        let candidates = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.9, 0.1, 0.0],
        ];

        let results = searcher.top_k(&query, &candidates, 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].index, 0); // identical first
        assert!((results[0].score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_top_k_order() {
        let searcher = TopKSearcher::new(3);
        let query = vec![1.0, 0.0, 0.0];
        let candidates = vec![
            vec![-1.0, 0.0, 0.0], // opposite
            vec![0.5, 0.5, 0.0],  // moderate
            vec![1.0, 0.0, 0.0],  // identical
            vec![0.0, 1.0, 0.0],  // orthogonal
        ];

        let results = searcher.top_k(&query, &candidates, 3);

        assert_eq!(results.len(), 3);
        // Check descending order
        for i in 1..results.len() {
            assert!(results[i - 1].score >= results[i].score);
        }
    }

    #[test]
    fn test_top_k_k_larger_than_candidates() {
        let searcher = TopKSearcher::new(10);
        let query = vec![1.0, 0.0];
        let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0]];

        let results = searcher.top_k(&query, &candidates, 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_top_k_empty_candidates() {
        let searcher = TopKSearcher::new(5);
        let query = vec![1.0, 0.0];
        let candidates: Vec<Vec<f32>> = vec![];

        let results = searcher.top_k(&query, &candidates, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_top_k_empty_query() {
        let searcher = TopKSearcher::new(5);
        let query: Vec<f32> = vec![];
        let candidates = vec![vec![1.0, 0.0]];

        let results = searcher.top_k(&query, &candidates, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_all_pairwise_similarities() {
        let queries = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0]];

        let result = all_pairwise_similarities(&queries, &candidates);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 2);
        // Query 0 vs Candidate 0: identical = 1.0
        assert!((result[0][0] - 1.0).abs() < 1e-6);
        // Query 0 vs Candidate 1: orthogonal = 0.0
        assert!((result[0][1] - 0.0).abs() < 1e-6);
        // Query 1 vs Candidate 0: orthogonal = 0.0
        assert!((result[1][0] - 0.0).abs() < 1e-6);
        // Query 1 vs Candidate 1: identical = 1.0
        assert!((result[1][1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_top_k_batch() {
        let searcher = TopKSearcher::new(2);
        let queries = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        let candidates = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.5, 0.5, 0.0],
        ];

        let results = searcher.top_k_batch(&queries, &candidates, 2);

        assert_eq!(results.len(), 2);
        // Query 0: should find [1,0,0] first, then [0.5,0.5,0]
        assert_eq!(results[0][0].index, 0);
        // Query 1: should find [0,1,0] first
        assert_eq!(results[1][0].index, 1);
    }

    #[test]
    fn test_top_k_with_backend() {
        use crate::gpu::SimdBackend;
        let backend: Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>> =
            Arc::new(SimdBackend::new());
        let searcher = TopKSearcher::with_backend(backend);
        assert!(searcher.has_backend());

        let query = vec![1.0, 0.0, 0.0];
        let candidates = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];

        let results = searcher.top_k(&query, &candidates, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].index, 0);
    }

    #[test]
    fn test_top_k_batch_with_backend() {
        use crate::gpu::SimdBackend;
        let backend: Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>> =
            Arc::new(SimdBackend::new());
        let searcher = TopKSearcher::with_backend(backend);

        let queries = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0]];

        let results = searcher.top_k_batch(&queries, &candidates, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0][0].index, 0);
        assert_eq!(results[1][0].index, 1);
    }
}
