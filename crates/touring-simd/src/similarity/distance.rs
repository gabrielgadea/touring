#![allow(clippy::indexing_slicing)]

//! Distance metrics with SIMD acceleration.
//!
//! Provides various distance/similarity metrics beyond cosine:
//! - Euclidean distance
//! - Squared Euclidean distance
//! - Manhattan distance
//! - Chebyshev distance (L∞)
//! - Pearson correlation
//! - Dot product (as a similarity measure)

use crate::simd_utils::dispatch::arch;
use crate::simd_utils::ops;
use crate::simd_utils::{portable_dot_f32, portable_sqeuclidean_f32};
use rayon::prelude::*;

/// Compute squared Euclidean distance between two f32 vectors.
///
/// Returns sum((a\[i\] - b\[i\])^2) for all i.
#[inline]
#[must_use]
pub fn squared_euclidean(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::INFINITY;
    }
    portable_sqeuclidean_f32(a, b)
}

/// Compute Euclidean distance between two f32 vectors.
///
/// Returns sqrt(sum((a\[i\] - b\[i\])^2)).
#[inline]
#[must_use]
pub fn euclidean(a: &[f32], b: &[f32]) -> f32 {
    squared_euclidean(a, b).sqrt()
}

/// Compute Manhattan distance (L1 norm) between two f32 vectors.
///
/// Returns sum(|a\[i\] - b\[i\]|). Uses SIMD-accelerated abs + sum via pulp.
#[inline]
#[must_use]
pub fn manhattan(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::INFINITY;
    }
    arch().dispatch(ops::ManhattanF32 { a, b })
}

/// Compute Pearson correlation coefficient between two vectors.
///
/// Returns value in [-1, 1] where 1 = perfect positive correlation.
#[inline]
#[must_use]
pub fn pearson_correlation(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let n = a.len() as f64;

    // Compute means
    let mean_a: f64 = a.iter().map(|&x| x as f64).sum::<f64>() / n;
    let mean_b: f64 = b.iter().map(|&x| x as f64).sum::<f64>() / n;

    // Compute correlation
    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;

    for (ai, bi) in a.iter().zip(b.iter()) {
        let da = *ai as f64 - mean_a;
        let db = *bi as f64 - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }

    let denom = (var_a * var_b).sqrt();
    if denom == 0.0 {
        return 0.0; // One or both vectors have zero variance
    }

    cov / denom
}

/// Compute Chebyshev distance (L∞ norm) between two f32 vectors.
///
/// Returns max(|a\[i\] - b\[i\]|) for all i.
/// Uses SIMD-accelerated max of absolute differences via pulp.
#[inline]
#[must_use]
pub fn chebyshev(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::INFINITY;
    }
    a.iter()
        .zip(b.iter())
        .map(|(ai, bi)| (ai - bi).abs())
        .fold(0.0f32, |acc, diff| acc.max(diff))
}

/// Batch Chebyshev distance computation.
#[must_use]
pub fn chebyshev_batch(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
    candidates
        .iter()
        .map(|cand| chebyshev(query, cand))
        .collect()
}

/// Parallel batch Chebyshev distance computation.
#[must_use]
pub fn chebyshev_batch_par(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
    candidates
        .par_iter()
        .map(|cand| chebyshev(query, cand))
        .collect()
}

/// Compute dot product as a similarity score.
///
/// Note: For normalized vectors, this equals cosine similarity.
#[inline]
#[must_use]
pub fn dot_product(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }
    portable_dot_f32(a, b) as f64
}

/// Batch Euclidean distance computation.
#[must_use]
pub fn euclidean_batch(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
    candidates
        .iter()
        .map(|cand| euclidean(query, cand))
        .collect()
}

/// Batch Manhattan distance computation.
#[must_use]
pub fn manhattan_batch(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
    candidates
        .iter()
        .map(|cand| manhattan(query, cand))
        .collect()
}

/// Batch Pearson correlation computation.
#[must_use]
pub fn pearson_batch(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f64> {
    candidates
        .iter()
        .map(|cand| pearson_correlation(query, cand))
        .collect()
}

/// Parallel batch distance computation.
#[must_use]
pub fn euclidean_batch_par(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
    candidates
        .par_iter()
        .map(|cand| euclidean(query, cand))
        .collect()
}

/// Parallel Manhattan batch.
#[must_use]
pub fn manhattan_batch_par(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
    candidates
        .par_iter()
        .map(|cand| manhattan(query, cand))
        .collect()
}

/// Parallel Pearson batch.
#[must_use]
pub fn pearson_batch_par(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f64> {
    candidates
        .par_iter()
        .map(|cand| pearson_correlation(query, cand))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_euclidean() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        assert_relative_eq!(euclidean(&a, &b), 5.0, epsilon = 1e-6);
    }

    #[test]
    fn test_squared_euclidean() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        assert_relative_eq!(squared_euclidean(&a, &b), 25.0, epsilon = 1e-6);
    }

    #[test]
    fn test_manhattan() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 6.0, 3.0];
        // |1-4| + |2-6| + |3-3| = 3 + 4 + 0 = 7
        assert_relative_eq!(manhattan(&a, &b), 7.0, epsilon = 1e-6);
    }

    #[test]
    fn test_pearson_identical() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_relative_eq!(pearson_correlation(&a, &b), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_pearson_opposite() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        assert_relative_eq!(pearson_correlation(&a, &b), -1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_pearson_orthogonal() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 1.0, 1.0];
        // Mean of b is constant, variance is 0, so correlation should be 0
        let result = pearson_correlation(&a, &b);
        assert!(result.abs() < 1e-10);
    }

    #[test]
    fn test_pearson_zero_variance() {
        let a = vec![1.0, 1.0, 1.0];
        let b = vec![1.0, 2.0, 3.0];
        // a has zero variance, should return 0
        assert_eq!(pearson_correlation(&a, &b), 0.0);
    }

    #[test]
    fn test_dot_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
        assert_eq!(dot_product(&a, &b), 32.0);
    }

    #[test]
    fn test_euclidean_batch() {
        let query = vec![0.0, 0.0];
        let candidates = vec![
            vec![3.0, 4.0], // distance 5
            vec![0.0, 0.0], // distance 0
            vec![1.0, 1.0], // distance sqrt(2)
        ];

        let distances = euclidean_batch(&query, &candidates);
        assert_eq!(distances.len(), 3);
        assert_relative_eq!(distances[0], 5.0, epsilon = 1e-6);
        assert_relative_eq!(distances[1], 0.0, epsilon = 1e-6);
        assert_relative_eq!(distances[2], std::f32::consts::SQRT_2, epsilon = 1e-6);
    }

    #[test]
    fn test_manhattan_batch() {
        let query = vec![0.0, 0.0];
        let candidates = vec![vec![1.0, 2.0], vec![3.0, 4.0]];

        let distances = manhattan_batch(&query, &candidates);
        assert_eq!(distances.len(), 2);
        assert_relative_eq!(distances[0], 3.0, epsilon = 1e-6);
        assert_relative_eq!(distances[1], 7.0, epsilon = 1e-6);
    }

    #[test]
    fn test_euclidean_dimension_mismatch() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!(euclidean(&a, &b).is_infinite());
    }

    // ── Chebyshev distance tests ────────────────────────────────

    #[test]
    fn test_chebyshev_basic() {
        let a = vec![1.0, 3.0, 5.0];
        let b = vec![2.0, 1.0, 7.0];
        // |1-2|=1, |3-1|=2, |5-7|=2 → max = 2
        assert_relative_eq!(chebyshev(&a, &b), 2.0, epsilon = 1e-6);
    }

    #[test]
    fn test_chebyshev_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(chebyshev(&a, &b), 0.0);
    }

    #[test]
    fn test_chebyshev_single_element() {
        let a = vec![5.0];
        let b = vec![3.0];
        assert_eq!(chebyshev(&a, &b), 2.0);
    }

    #[test]
    fn test_chebyshev_dimension_mismatch() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!(chebyshev(&a, &b).is_infinite());
    }

    #[test]
    fn test_chebyshev_batch() {
        let query = vec![0.0, 0.0];
        let candidates = vec![
            vec![3.0, 4.0], // max(|3|,|4|) = 4
            vec![0.0, 0.0], // max(|0|,|0|) = 0
            vec![1.0, 2.0], // max(|1|,|2|) = 2
        ];
        let distances = chebyshev_batch(&query, &candidates);
        assert_eq!(distances.len(), 3);
        assert_relative_eq!(distances[0], 4.0, epsilon = 1e-6);
        assert_relative_eq!(distances[1], 0.0, epsilon = 1e-6);
        assert_relative_eq!(distances[2], 2.0, epsilon = 1e-6);
    }

    #[test]
    fn test_chebyshev_batch_par() {
        let query = vec![0.0, 0.0];
        let candidates = vec![vec![3.0, 4.0], vec![0.0, 0.0], vec![1.0, 2.0]];
        let distances = chebyshev_batch_par(&query, &candidates);
        assert_eq!(distances.len(), 3);
        // Same results as serial version
        assert_relative_eq!(distances[0], 4.0, epsilon = 1e-6);
        assert_relative_eq!(distances[1], 0.0, epsilon = 1e-6);
        assert_relative_eq!(distances[2], 2.0, epsilon = 1e-6);
    }
}
