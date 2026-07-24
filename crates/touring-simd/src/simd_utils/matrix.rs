#![allow(clippy::indexing_slicing)]

//! Matrix operations with SIMD acceleration.
//!
//! Provides batch matrix-vector operations optimized for embedding inference.
//!
//! # Layout
//!
//! [`MatrixView`] uses flat row-major storage for cache-friendly SIMD access.
//! Legacy `Vec<Vec<f32>>` functions are deprecated but still available.

use crate::simd_utils::dispatch::arch;
use crate::simd_utils::ops;
use crate::simd_utils::portable_dot_f32;
use rayon::prelude::*;

// ============================================================
// MatrixView — Flat row-major layout
// ============================================================

/// A read-only view into a flat row-major matrix.
///
/// All data is contiguous in memory for maximum cache locality and SIMD throughput.
///
/// # Example
///
/// ```
/// use touring_simd::simd_utils::matrix::MatrixView;
///
/// // 2x3 matrix stored flat: [row0..., row1...]
/// let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
/// let m = MatrixView::new(&data, 2, 3);
/// assert_eq!(m.row(0), &[1.0, 2.0, 3.0]);
/// assert_eq!(m.row(1), &[4.0, 5.0, 6.0]);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct MatrixView<'a> {
    data: &'a [f32],
    rows: usize,
    cols: usize,
}

impl<'a> MatrixView<'a> {
    /// Create a new matrix view from flat data.
    ///
    /// # Panics
    ///
    /// Panics if `data.len() != rows * cols`.
    #[must_use]
    pub fn new(data: &'a [f32], rows: usize, cols: usize) -> Self {
        assert_eq!(
            data.len(),
            rows * cols,
            "data length {} != rows {} * cols {}",
            data.len(),
            rows,
            cols
        );
        Self { data, rows, cols }
    }

    /// Number of rows.
    #[inline]
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Number of columns.
    #[inline]
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Get a slice view of row `i`.
    #[inline]
    #[must_use]
    pub fn row(&self, i: usize) -> &[f32] {
        let start = i * self.cols;
        &self.data[start..start + self.cols]
    }

    /// Get the underlying flat data.
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[f32] {
        self.data
    }
}

/// Compute matrix-vector product with flat layout: out = matrix * vec.
///
/// Uses SIMD-accelerated dot product for each row.
pub fn mat_vec_mul_flat(matrix: MatrixView<'_>, v: &[f32], out: &mut [f32]) {
    assert_eq!(matrix.cols(), v.len());
    assert_eq!(matrix.rows(), out.len());

    for (i, o) in out.iter_mut().enumerate() {
        *o = arch().dispatch(ops::DotF32 {
            a: matrix.row(i),
            b: v,
        });
    }
}

/// Parallel matrix-vector product with flat layout.
#[must_use]
pub fn mat_vec_mul_flat_par(matrix: MatrixView<'_>, v: &[f32]) -> Vec<f32> {
    assert_eq!(matrix.cols(), v.len());

    if matrix.rows() < 100 {
        let mut out = vec![0.0; matrix.rows()];
        mat_vec_mul_flat(matrix, v, &mut out);
        return out;
    }

    (0..matrix.rows())
        .into_par_iter()
        .map(|i| {
            arch().dispatch(ops::DotF32 {
                a: matrix.row(i),
                b: v,
            })
        })
        .collect()
}

/// Row-wise normalization on flat matrix (in-place).
pub fn normalize_rows_flat(data: &mut [f32], rows: usize, cols: usize) {
    assert_eq!(data.len(), rows * cols);

    for i in 0..rows {
        let start = i * cols;
        let row = &data[start..start + cols];
        let norm = arch().dispatch(ops::DotF32 { a: row, b: row }).sqrt();
        if norm > 0.0 {
            let inv_norm = 1.0 / norm;
            for elem in &mut data[start..start + cols] {
                *elem *= inv_norm;
            }
        }
    }
}

/// L2-normalize a single vector in place via SIMD dot-product + scalar rescale.
///
/// Mathematically equivalent to the idiomatic scalar pattern
/// `let norm = v.iter().map(|x| x*x).sum::<f32>().sqrt(); v.iter_mut().for_each(|x| *x /= norm)`
/// but uses `pulp::Arch`-dispatched SIMD for the norm computation (≈4-8×
/// faster on AVX2/NEON for 32-dim+ vectors; near-scalar for tiny vectors
/// due to dispatch being cached after first call).
///
/// Zero-norm vectors are left unchanged (matches the common guard against
/// division by zero). The threshold `1e-10` is standard for f32 embedding
/// pipelines — tight enough to detect genuine zero vectors, loose enough
/// to avoid amplifying denormals.
///
/// # Why a dedicated helper
///
/// The workspace has 4+ hot-path call sites doing this exact scalar pattern
/// (ann_memory, hnsw blast-radius embeddings, tfidf, pensieve). Each was
/// duplicating ~4 lines of scalar arithmetic; each now becomes a single
/// SIMD call while preserving numerical compatibility.
#[inline]
pub fn l2_normalize_inplace(v: &mut [f32]) {
    if v.is_empty() {
        return;
    }
    let norm = arch().dispatch(ops::DotF32 { a: v, b: v }).sqrt();
    if norm > 1e-10 {
        let inv_norm = 1.0 / norm;
        for elem in v.iter_mut() {
            *elem *= inv_norm;
        }
    }
}

// ============================================================
// Activations — SIMD accelerated
// ============================================================

/// ReLU activation in-place: x\[i\] = max(0, x\[i\]). SIMD via pulp.
pub fn relu_inplace(x: &mut [f32]) {
    use pulp::{Simd, WithSimd};

    struct ReluInplace<'a> {
        x: &'a mut [f32],
    }
    impl WithSimd for ReluInplace<'_> {
        type Output = ();
        #[inline(always)]
        fn with_simd<S: Simd>(self, simd: S) {
            let (head, tail) = S::as_mut_simd_f32s(self.x);
            let zero = simd.splat_f32s(0.0);
            for v in head.iter_mut() {
                *v = simd.max_f32s(*v, zero);
            }
            for v in tail.iter_mut() {
                *v = v.max(0.0);
            }
        }
    }

    arch().dispatch(ReluInplace { x });
}

/// ReLU activation: out\[i\] = max(0, x\[i\]).
#[must_use]
pub fn relu(x: &[f32]) -> Vec<f32> {
    let mut result = x.to_vec();
    relu_inplace(&mut result);
    result
}

/// Softmax activation: out\[i\] = exp(x\[i\] - max) / sum(exp(x\[j\] - max)).
///
/// Uses SIMD for max-finding, exponentiation preparation, and sum reduction.
/// Numerically stable via max subtraction.
#[must_use]
pub fn softmax(x: &[f32]) -> Vec<f32> {
    if x.is_empty() {
        return vec![];
    }

    // Find max for numerical stability (SIMD via reduce_max)
    use pulp::{Simd, WithSimd};

    struct FindMax<'a> {
        x: &'a [f32],
    }
    impl WithSimd for FindMax<'_> {
        type Output = f32;
        #[inline(always)]
        fn with_simd<S: Simd>(self, simd: S) -> f32 {
            let (head, tail) = S::as_simd_f32s(self.x);
            let mut acc = simd.splat_f32s(f32::NEG_INFINITY);
            for v in head {
                acc = simd.max_f32s(acc, *v);
            }
            let mut max = simd.reduce_max_f32s(acc);
            for &v in tail {
                max = max.max(v);
            }
            max
        }
    }

    let max_val = arch().dispatch(FindMax { x });

    // Compute exponentials and sum
    let exps: Vec<f32> = x.iter().map(|&v| (v - max_val).exp()).collect();
    let sum = arch().dispatch(ops::ReduceSumF32 { a: &exps });

    if sum == 0.0 {
        return vec![0.0; x.len()];
    }

    exps.iter().map(|&e| e / sum).collect()
}

/// Add bias to each row: out\[i\]\[j\] = matrix\[i\]\[j\] + bias\[i\]
#[must_use]
pub fn add_bias_flat(data: &[f32], rows: usize, cols: usize, bias: &[f32]) -> Vec<f32> {
    assert_eq!(data.len(), rows * cols);
    assert_eq!(bias.len(), rows);

    let mut out = data.to_vec();
    for (i, &b) in bias.iter().enumerate() {
        let start = i * cols;
        for elem in &mut out[start..start + cols] {
            *elem += b;
        }
    }
    out
}

// ============================================================
// Legacy API (deprecated — use flat variants)
// ============================================================

/// Compute matrix-vector product: out = matrix * vec
///
/// # Deprecated
///
/// Use [`mat_vec_mul_flat`] with [`MatrixView`] for better cache performance.
pub fn mat_vec_mul(matrix: &[Vec<f32>], vec: &[f32], out: &mut [f32]) {
    let rows = matrix.len();
    let cols = vec.len();

    assert_eq!(out.len(), rows);
    assert!(matrix.iter().all(|row| row.len() == cols));

    for (i, row) in matrix.iter().enumerate() {
        out[i] = portable_dot_f32(row, vec);
    }
}

/// Parallel matrix-vector product for large matrices.
///
/// Use [`mat_vec_mul_flat_par`] with [`MatrixView`] for better cache performance.
#[must_use]
pub fn mat_vec_mul_par(matrix: &[Vec<f32>], vec: &[f32]) -> Vec<f32> {
    let rows = matrix.len();
    let cols = vec.len();

    assert!(matrix.iter().all(|row| row.len() == cols));

    if rows < 100 {
        let mut out = vec![0.0; rows];
        mat_vec_mul(matrix, vec, &mut out);
        return out;
    }

    matrix
        .par_iter()
        .map(|row| portable_dot_f32(row, vec))
        .collect()
}

/// Batch matrix-vector products.
pub fn batch_mat_vec_mul(matrices: &[Vec<Vec<f32>>], vec: &[f32], out: &mut [Vec<f32>]) {
    assert_eq!(out.len(), matrices.len());

    for (i, matrix) in matrices.iter().enumerate() {
        out[i].resize(matrix.len(), 0.0);
        mat_vec_mul(matrix, vec, &mut out[i]);
    }
}

/// Parallel batch matrix-vector products.
#[must_use]
pub fn batch_mat_vec_mul_par(matrices: &[Vec<Vec<f32>>], vec: &[f32]) -> Vec<Vec<f32>> {
    matrices
        .par_iter()
        .map(|matrix| {
            let mut result = vec![0.0; matrix.len()];
            mat_vec_mul(matrix, vec, &mut result);
            result
        })
        .collect()
}

/// Compute batch outer products.
#[must_use]
pub fn batch_outer_product(vectors: &[Vec<f32>]) -> Vec<Vec<Vec<f32>>> {
    vectors
        .iter()
        .map(|v| {
            let n = v.len();
            let mut outer = vec![vec![0.0; n]; n];
            for (i, vi) in v.iter().enumerate() {
                for (j, vj) in v.iter().enumerate() {
                    outer[i][j] = vi * vj;
                }
            }
            outer
        })
        .collect()
}

/// Row-wise normalization (legacy \<Vec\>\<Vec\> API).
pub fn normalize_rows(matrix: &mut [Vec<f32>]) {
    for row in matrix.iter_mut() {
        let norm = portable_dot_f32(row, row).sqrt();
        if norm > 0.0 {
            let inv_norm = 1.0 / norm;
            for elem in row.iter_mut() {
                *elem *= inv_norm;
            }
        }
    }
}

/// Parallel row-wise normalization (legacy).
pub fn normalize_rows_par(matrix: &mut [Vec<f32>]) {
    if matrix.len() < 100 {
        normalize_rows(matrix);
        return;
    }

    matrix.par_iter_mut().for_each(|row| {
        let norm = portable_dot_f32(row, row).sqrt();
        if norm > 0.0 {
            let inv_norm = 1.0 / norm;
            for elem in row.iter_mut() {
                *elem *= inv_norm;
            }
        }
    });
}

/// Add bias (legacy API).
#[must_use]
pub fn add_bias(matrix: &[Vec<f32>], bias: &[f32]) -> Vec<Vec<f32>> {
    assert_eq!(matrix.len(), bias.len());

    matrix
        .iter()
        .zip(bias.iter())
        .map(|(row, &b)| row.iter().map(|&x| x + b).collect())
        .collect()
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── l2_normalize_inplace tests ───────────────────────────

    fn scalar_l2_norm(v: &[f32]) -> f32 {
        v.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    #[test]
    fn test_l2_normalize_inplace_unit_output() {
        let mut v = vec![3.0f32, 4.0, 0.0]; // norm = 5
        l2_normalize_inplace(&mut v);
        let result_norm = scalar_l2_norm(&v);
        assert!(
            (result_norm - 1.0).abs() < 1e-5,
            "normalized vector must have unit norm, got {result_norm}"
        );
        // Direction preserved: v[0]/v[1] == 3/4.
        assert!((v[0] / v[1] - 0.75).abs() < 1e-5);
    }

    #[test]
    fn test_l2_normalize_inplace_matches_scalar() {
        // Verify SIMD result is numerically close to the scalar reference
        // across a range of dims and values. f32 SIMD accumulation can
        // differ from scalar sequential by 1-2 ULP, so use a 1e-4 epsilon.
        for &dim in &[1usize, 3, 7, 8, 16, 31, 32, 64, 127, 128, 256] {
            let mut simd_v: Vec<f32> = (0..dim).map(|i| ((i * 7 + 3) as f32).sin() * 2.5).collect();
            let mut scalar_v = simd_v.clone();

            // SIMD path
            l2_normalize_inplace(&mut simd_v);

            // Scalar reference
            let norm = scalar_l2_norm(&scalar_v);
            if norm > 1e-10 {
                scalar_v.iter_mut().for_each(|x| *x /= norm);
            }

            for (i, (s, r)) in simd_v.iter().zip(scalar_v.iter()).enumerate() {
                assert!(
                    (s - r).abs() < 1e-4,
                    "dim={dim} i={i}: SIMD={s} scalar={r} diff={}",
                    (s - r).abs()
                );
            }
        }
    }

    #[test]
    fn test_l2_normalize_inplace_zero_vector() {
        let mut v = vec![0.0f32; 10];
        l2_normalize_inplace(&mut v);
        // Zero vector must remain zero (no division by zero).
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_l2_normalize_inplace_tiny_vector_guard() {
        // Values below the 1e-10 threshold must be left alone to avoid
        // amplifying denormals.
        let mut v = vec![1e-20f32, 1e-20, 1e-20];
        let original = v.clone();
        l2_normalize_inplace(&mut v);
        // Result depends on whether norm > 1e-10: for this input norm ≈
        // 1.73e-20, below the guard, so the vector is left unchanged.
        assert_eq!(v, original);
    }

    #[test]
    fn test_l2_normalize_inplace_empty() {
        let mut v: Vec<f32> = Vec::new();
        l2_normalize_inplace(&mut v);
        assert!(v.is_empty());
    }

    #[test]
    fn test_l2_normalize_inplace_single_element() {
        let mut v = vec![5.0f32];
        l2_normalize_inplace(&mut v);
        assert!((v[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_inplace_negative_values() {
        let mut v = vec![-3.0f32, 4.0];
        l2_normalize_inplace(&mut v);
        // Direction preserved: sign of each element stays the same.
        assert!(v[0] < 0.0);
        assert!(v[1] > 0.0);
        let n = scalar_l2_norm(&v);
        assert!((n - 1.0).abs() < 1e-5);
    }

    // ── MatrixView tests ─────────────────────────────────────

    #[test]
    fn test_matrix_view_basic() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let m = MatrixView::new(&data, 2, 3);
        assert_eq!(m.rows(), 2);
        assert_eq!(m.cols(), 3);
        assert_eq!(m.row(0), &[1.0, 2.0, 3.0]);
        assert_eq!(m.row(1), &[4.0, 5.0, 6.0]);
    }

    #[test]
    fn test_mat_vec_mul_flat() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let m = MatrixView::new(&data, 2, 3);
        let v = vec![1.0, 0.0, 0.0];
        let mut out = vec![0.0; 2];
        mat_vec_mul_flat(m, &v, &mut out);
        assert_eq!(out, vec![1.0, 4.0]);
    }

    #[test]
    fn test_mat_vec_mul_flat_full() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let m = MatrixView::new(&data, 2, 3);
        let v = vec![1.0, 1.0, 1.0];
        let mut out = vec![0.0; 2];
        mat_vec_mul_flat(m, &v, &mut out);
        assert_eq!(out, vec![6.0, 15.0]);
    }

    #[test]
    fn test_normalize_rows_flat() {
        let mut data = vec![3.0, 4.0, 0.0, 5.0];
        normalize_rows_flat(&mut data, 2, 2);
        assert!((data[0] - 0.6).abs() < 1e-6);
        assert!((data[1] - 0.8).abs() < 1e-6);
        assert!((data[2] - 0.0).abs() < 1e-6);
        assert!((data[3] - 1.0).abs() < 1e-6);
    }

    // ── Legacy API tests (backward compat) ───────────────────

    #[test]
    fn test_mat_vec_mul() {
        let matrix = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
        let vec = vec![1.0, 0.0, 0.0];
        let mut out = vec![0.0; 2];
        mat_vec_mul(&matrix, &vec, &mut out);
        assert_eq!(out, vec![1.0, 4.0]);
    }

    #[test]
    fn test_mat_vec_mul_full() {
        let matrix = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
        let vec = vec![1.0, 1.0, 1.0];
        let mut out = vec![0.0; 2];
        mat_vec_mul(&matrix, &vec, &mut out);
        assert_eq!(out, vec![6.0, 15.0]);
    }

    #[test]
    fn test_normalize_rows() {
        let mut matrix = vec![vec![3.0, 4.0], vec![0.0, 5.0]];
        normalize_rows(&mut matrix);
        assert_eq!(matrix[0], vec![0.6, 0.8]);
        assert_eq!(matrix[1], vec![0.0, 1.0]);
    }

    #[test]
    fn test_add_bias() {
        let matrix = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let bias = vec![1.0, -1.0];
        let result = add_bias(&matrix, &bias);
        assert_eq!(result[0], vec![2.0, 3.0]);
        assert_eq!(result[1], vec![2.0, 3.0]);
    }

    // ── Activation tests ─────────────────────────────────────

    #[test]
    fn test_relu() {
        let x = vec![-1.0, 0.0, 1.0, -0.5];
        let result = relu(&x);
        assert_eq!(result, vec![0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn test_relu_inplace() {
        let mut x = vec![-1.0, 0.0, 1.0, -0.5, 2.0, -3.0, 0.1, -0.1, 5.0];
        relu_inplace(&mut x);
        assert_eq!(x, vec![0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.1, 0.0, 5.0]);
    }

    #[test]
    fn test_softmax() {
        let x = vec![1.0, 2.0, 3.0];
        let result = softmax(&x);

        let sum: f32 = result.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
        assert!(result.iter().all(|&v| v > 0.0 && v < 1.0));
        assert!(result[2] > result[1]);
        assert!(result[1] > result[0]);
    }

    #[test]
    fn test_softmax_empty() {
        assert!(softmax(&[]).is_empty());
    }

    #[test]
    fn test_batch_mat_vec_mul() {
        let matrices = vec![
            vec![vec![1.0, 2.0], vec![3.0, 4.0]],
            vec![vec![5.0, 6.0], vec![7.0, 8.0]],
        ];
        let vec = vec![1.0, 1.0];
        let mut out = vec![vec![0.0; 2]; 2];
        batch_mat_vec_mul(&matrices, &vec, &mut out);
        assert_eq!(out[0], vec![3.0, 7.0]);
        assert_eq!(out[1], vec![11.0, 15.0]);
    }

    #[test]
    fn test_add_bias_flat() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let result = add_bias_flat(&data, 2, 2, &[10.0, 20.0]);
        assert_eq!(result, vec![11.0, 12.0, 23.0, 24.0]);
    }
}
