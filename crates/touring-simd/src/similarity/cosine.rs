//! Cosine similarity for dense vectors.
//!
//! Pln3 Phase 1B - L2: Implementation
//! Uses SIMD-accelerated dot product and norms.

use super::traits::CosineSimilarity;
use crate::buffer_pool::with_f32_buffer;
use crate::gpu::GpuBackend;
use crate::simd_utils::dispatch::arch;
use crate::simd_utils::ops::CosineSinglePass;
use crate::simd_utils::simd_norm_f32;
use rayon::prelude::*;
use std::fmt;
use std::sync::Arc;

/// Cosine similarity computer with SIMD acceleration.
pub struct CosineComputer {
    /// Minimum batch size for parallel processing.
    parallel_threshold: usize,
    /// Optional GPU backend for compute acceleration.
    backend: Option<Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>>>,
}

impl CosineComputer {
    /// Create new cosine computer with default settings.
    pub fn new() -> Self {
        Self {
            parallel_threshold: 100,
            backend: None,
        }
    }

    /// Create with custom parallel threshold.
    pub fn with_threshold(parallel_threshold: usize) -> Self {
        Self {
            parallel_threshold,
            backend: None,
        }
    }

    /// Create with a GPU backend.
    pub fn with_backend(backend: Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>>) -> Self {
        Self {
            parallel_threshold: 100,
            backend: Some(backend),
        }
    }

    /// Create with custom threshold and GPU backend.
    pub fn with_threshold_and_backend(
        parallel_threshold: usize,
        backend: Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>>,
    ) -> Self {
        Self {
            parallel_threshold,
            backend: Some(backend),
        }
    }

    /// Returns true if a GPU backend is available.
    pub fn has_backend(&self) -> bool {
        self.backend.is_some()
    }
}

impl fmt::Debug for CosineComputer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CosineComputer")
            .field("parallel_threshold", &self.parallel_threshold)
            .field("has_backend", &self.has_backend())
            .finish()
    }
}

impl Clone for CosineComputer {
    fn clone(&self) -> Self {
        Self {
            parallel_threshold: self.parallel_threshold,
            backend: self.backend.clone(),
        }
    }
}

impl Default for CosineComputer {
    fn default() -> Self {
        Self::new()
    }
}

impl CosineSimilarity for CosineComputer {
    fn cosine(&self, a: &[f32], b: &[f32]) -> f64 {
        // Single-pass with 3 accumulators + mixed-precision f64 reduction.
        // 3x fewer cache misses vs separate dot + norm_a + norm_b calls.
        arch().dispatch(CosineSinglePass { a, b })
    }

    fn cosine_batch(&self, queries: &[Vec<f32>], candidates: &[Vec<f32>]) -> Vec<f64> {
        if queries.is_empty() || candidates.is_empty() {
            return vec![];
        }

        // Use GPU backend if available
        if let Some(ref backend) = self.backend {
            return queries
                .iter()
                .flat_map(|query| {
                    candidates
                        .iter()
                        .map(|candidate| backend.compute_cosine(query, candidate).unwrap_or(0.0))
                        .collect::<Vec<_>>()
                })
                .collect();
        }

        let use_parallel = queries.len() * candidates.len() >= self.parallel_threshold;

        if use_parallel {
            queries
                .par_iter()
                .flat_map(|query| {
                    candidates
                        .par_iter()
                        .map(|candidate| self.cosine(query, candidate))
                        .collect::<Vec<_>>()
                })
                .collect()
        } else {
            queries
                .iter()
                .flat_map(|query| {
                    candidates
                        .iter()
                        .map(|candidate| self.cosine(query, candidate))
                        .collect::<Vec<_>>()
                })
                .collect()
        }
    }
}

/// Normalize a vector to unit length in place.
///
/// If the vector has zero norm, it is left unchanged.
pub fn normalize_vector(v: &mut [f32]) {
    let norm = simd_norm_f32(v);
    if norm > 0.0 {
        let inv_norm = 1.0 / norm;
        for elem in v.iter_mut() {
            *elem *= inv_norm;
        }
    }
}

/// Normalize a vector to unit length, returning a new vector.
///
/// Returns a zero vector if the input has zero norm.
#[must_use]
pub fn normalized(v: &[f32]) -> Vec<f32> {
    let mut result = v.to_vec();
    normalize_vector(&mut result);
    result
}

/// Normalize a vector to unit length using thread-local buffer pool.
///
/// This version avoids heap allocation for intermediate storage by reusing
/// a thread-local buffer. Use this in hot paths with many normalizations.
///
/// Returns a zero vector if the input has zero norm.
///
/// # Example
///
/// ```
/// use touring_simd::{CosineComputer, CosineSimilarity};
/// let computer = CosineComputer::new();
/// // Self-similarity of a normalized vector is 1.0
/// let n = CosineSimilarity::cosine(&computer, &[3.0, 4.0], &[3.0, 4.0]);
/// assert!((n - 1.0).abs() < 1e-6);
/// ```
#[must_use]
pub fn normalized_with_buffer(v: &[f32]) -> Vec<f32> {
    with_f32_buffer(v.len(), |buf| {
        buf.copy_from_slice(v);
        let norm = simd_norm_f32(buf);
        if norm > 0.0 {
            let inv_norm = 1.0 / norm;
            for elem in buf.iter_mut() {
                *elem *= inv_norm;
            }
        }
        buf.to_vec()
    })
}

/// L2-normalize a batch of vectors in-place using SIMD.
///
/// This is more cache-friendly than calling [`normalize_vector`] in a loop,
/// as it processes vectors in cache-friendly sequential order.
///
/// # Arguments
///
/// * `batch` - Slice of mutable vector slices to normalize in-place
pub fn batch_normalize(batch: &mut [&mut [f32]]) {
    for vec in batch {
        normalize_vector(vec);
    }
}

/// Compute cosine distance (1 - cosine similarity).
///
/// Returns a value in [0, 2] where 0 = identical direction, 2 = opposite.
#[inline]
#[must_use]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
    static COMPUTER: CosineComputer = CosineComputer {
        parallel_threshold: 100,
        backend: None,
    };
    1.0 - COMPUTER.cosine(a, b)
}

impl super::traits::Similarity<[f32]> for CosineComputer {
    fn similarity(&self, a: &[f32], b: &[f32]) -> f64 {
        self.cosine(a, b)
    }
}

impl super::traits::Similarity<Vec<f32>> for CosineComputer {
    fn similarity(&self, a: &Vec<f32>, b: &Vec<f32>) -> f64 {
        self.cosine(a, b)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs are asserted non-empty before indexing
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_normalized_with_buffer_returns_unit() {
        let v = vec![3.0, 4.0, 0.0];
        let n = normalized_with_buffer(&v);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert_relative_eq!(norm, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_normalized_with_buffer_zero_vector() {
        let v = vec![0.0, 0.0, 0.0];
        let n = normalized_with_buffer(&v);
        // Zero vector remains zero
        assert!(n.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_normalized_with_buffer_buffer_reuse() {
        // Verify buffer reuse works across multiple calls
        let v1 = vec![3.0, 4.0];
        let v2 = vec![1.0, 0.0];
        let n1 = normalized_with_buffer(&v1);
        let n2 = normalized_with_buffer(&v2);
        // v1 normalized should be [0.6, 0.8]
        assert_relative_eq!(n1[0], 0.6, epsilon = 1e-6);
        assert_relative_eq!(n1[1], 0.8, epsilon = 1e-6);
        // v2 normalized is [1.0, 0.0]
        assert_relative_eq!(n2[0], 1.0, epsilon = 1e-6);
        assert_relative_eq!(n2[1], 0.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_identical() {
        let computer = CosineComputer::new();
        let a = vec![1.0, 2.0, 3.0, 4.0];
        assert_relative_eq!(computer.cosine(&a, &a), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let computer = CosineComputer::new();
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert_relative_eq!(computer.cosine(&a, &b), 0.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_opposite() {
        let computer = CosineComputer::new();
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert_relative_eq!(computer.cosine(&a, &b), -1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_dimension_mismatch() {
        let computer = CosineComputer::new();
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0];
        assert_eq!(computer.cosine(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_zero_vector() {
        let computer = CosineComputer::new();
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![0.0, 0.0, 0.0];
        assert_eq!(computer.cosine(&a, &b), 0.0);
    }

    #[test]
    fn test_normalize_vector() {
        let mut v = vec![3.0, 4.0];
        normalize_vector(&mut v);
        assert_relative_eq!(v[0], 0.6, epsilon = 1e-6);
        assert_relative_eq!(v[1], 0.8, epsilon = 1e-6);

        // Check that norm is now 1
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert_relative_eq!(norm, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_batch() {
        let computer = CosineComputer::new();
        let queries = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0]];

        let results = computer.cosine_batch(&queries, &candidates);

        // [1,0] vs [1,0] = 1.0
        // [1,0] vs [0,1] = 0.0
        // [0,1] vs [1,0] = 0.0
        // [0,1] vs [0,1] = 1.0
        assert_eq!(results.len(), 4);
        assert_relative_eq!(results[0], 1.0, epsilon = 1e-6);
        assert_relative_eq!(results[1], 0.0, epsilon = 1e-6);
        assert_relative_eq!(results[2], 0.0, epsilon = 1e-6);
        assert_relative_eq!(results[3], 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_large_vectors() {
        let computer = CosineComputer::new();
        let size = 1536; // Common embedding dimension

        // Create two similar but not identical vectors
        let a: Vec<f32> = (0..size).map(|i| (i as f32 * 0.001).sin()).collect();
        let mut b: Vec<f32> = a.clone();
        b[0] += 0.1; // Small perturbation

        let similarity = computer.cosine(&a, &b);
        // Should be close to 1.0 but not exactly 1.0
        assert!(similarity > 0.99);
        assert!(similarity < 1.0);
    }

    #[test]
    fn test_normalize_vector_zero() {
        let mut v = vec![0.0, 0.0, 0.0];
        normalize_vector(&mut v);
        // Zero vector remains zero
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_normalized_returns_unit() {
        let v = vec![3.0, 4.0, 0.0];
        let n = normalized(&v);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert_relative_eq!(norm, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_distance() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert_relative_eq!(cosine_distance(&a, &b), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_distance_identical() {
        let a = vec![1.0, 2.0, 3.0];
        assert_relative_eq!(cosine_distance(&a, &a), 0.0, epsilon = 1e-6);
    }

    #[test]
    fn test_similarity_trait_impl() {
        use crate::similarity::traits::Similarity;
        let computer = CosineComputer::new();
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        assert_relative_eq!(computer.similarity(&a, &b), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_batch_empty() {
        let computer = CosineComputer::new();
        let empty: Vec<Vec<f32>> = vec![];
        let candidates = vec![vec![1.0, 0.0]];
        assert!(computer.cosine_batch(&empty, &candidates).is_empty());
        assert!(computer.cosine_batch(&candidates, &empty).is_empty());
    }

    #[test]
    fn test_cosine_with_threshold() {
        let computer = CosineComputer::with_threshold(2);
        let queries = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let candidates = vec![vec![1.0, 0.0]];
        let results = computer.cosine_batch(&queries, &candidates);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_cosine_with_backend() {
        use crate::gpu::SimdBackend;
        let backend: Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>> =
            Arc::new(SimdBackend::new());
        let computer = CosineComputer::with_backend(backend);
        assert!(computer.has_backend());
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        assert_relative_eq!(computer.cosine(&a, &b), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_batch_with_backend() {
        use crate::gpu::SimdBackend;
        let backend: Arc<dyn GpuBackend<DotInput = f32, DotOutput = f32>> =
            Arc::new(SimdBackend::new());
        let computer = CosineComputer::with_backend(backend);
        let queries = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let results = computer.cosine_batch(&queries, &candidates);
        assert_eq!(results.len(), 4);
        assert_relative_eq!(results[0], 1.0, epsilon = 1e-6);
        assert_relative_eq!(results[3], 1.0, epsilon = 1e-6);
    }
}
