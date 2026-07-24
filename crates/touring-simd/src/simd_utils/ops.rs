#![allow(clippy::indexing_slicing)]

//! SIMD operations implemented via `pulp` for automatic multi-ISA dispatch.
//!
//! These implementations use `pulp::WithSimd` to generate optimized code for
//! SSE, AVX2, AVX-512, and NEON from a single generic implementation.
//! FMA (fused multiply-add) is used automatically where available.
//!
//! # Architecture
//!
//! Each operation is a struct implementing `pulp::WithSimd`. The public API
//! dispatches via `Arch::new().dispatch(Op { ... })`, which caches CPU
//! feature detection and routes to the best available ISA.

use pulp::{Simd, WithSimd};

// ============================================================
// Dot Product
// ============================================================

/// SIMD-accelerated f32 dot product with FMA.
pub(crate) struct DotF32<'a> {
    pub a: &'a [f32],
    pub b: &'a [f32],
}

impl WithSimd for DotF32<'_> {
    type Output = f32;

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) -> f32 {
        let (a_head, a_tail) = S::as_simd_f32s(self.a);
        let (b_head, b_tail) = S::as_simd_f32s(self.b);

        let mut acc = simd.splat_f32s(0.0);
        for (va, vb) in a_head.iter().zip(b_head) {
            acc = simd.mul_add_f32s(*va, *vb, acc);
        }

        let mut sum = simd.reduce_sum_f32s(acc);
        for (a, b) in a_tail.iter().zip(b_tail) {
            sum += a * b;
        }
        sum
    }
}

/// SIMD-accelerated f64 dot product with FMA.
pub(crate) struct DotF64<'a> {
    pub a: &'a [f64],
    pub b: &'a [f64],
}

impl WithSimd for DotF64<'_> {
    type Output = f64;

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) -> f64 {
        let (a_head, a_tail) = S::as_simd_f64s(self.a);
        let (b_head, b_tail) = S::as_simd_f64s(self.b);

        let mut acc = simd.splat_f64s(0.0);
        for (va, vb) in a_head.iter().zip(b_head) {
            acc = simd.mul_add_f64s(*va, *vb, acc);
        }

        let mut sum = simd.reduce_sum_f64s(acc);
        for (a, b) in a_tail.iter().zip(b_tail) {
            sum += a * b;
        }
        sum
    }
}

// ============================================================
// Squared Euclidean Distance
// ============================================================

/// SIMD-accelerated squared Euclidean distance: sum((a\[i\] - b\[i\])^2).
pub(crate) struct SqEuclideanF32<'a> {
    pub a: &'a [f32],
    pub b: &'a [f32],
}

impl WithSimd for SqEuclideanF32<'_> {
    type Output = f32;

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) -> f32 {
        let (a_head, a_tail) = S::as_simd_f32s(self.a);
        let (b_head, b_tail) = S::as_simd_f32s(self.b);

        let mut acc = simd.splat_f32s(0.0);
        for (va, vb) in a_head.iter().zip(b_head) {
            let diff = simd.sub_f32s(*va, *vb);
            acc = simd.mul_add_f32s(diff, diff, acc);
        }

        let mut sum = simd.reduce_sum_f32s(acc);
        for (a, b) in a_tail.iter().zip(b_tail) {
            let d = a - b;
            sum += d * d;
        }
        sum
    }
}

// ============================================================
// Element-wise Operations
// ============================================================

/// SIMD-accelerated element-wise addition: out\[i\] = a\[i\] + b\[i\].
pub(crate) struct AddF32<'a> {
    pub a: &'a [f32],
    pub b: &'a [f32],
    pub out: &'a mut [f32],
}

impl WithSimd for AddF32<'_> {
    type Output = ();

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) {
        let (a_head, a_tail) = S::as_simd_f32s(self.a);
        let (b_head, b_tail) = S::as_simd_f32s(self.b);
        let (out_head, out_tail) = S::as_mut_simd_f32s(self.out);

        for ((o, va), vb) in out_head.iter_mut().zip(a_head).zip(b_head) {
            *o = simd.add_f32s(*va, *vb);
        }
        for ((o, a), b) in out_tail.iter_mut().zip(a_tail).zip(b_tail) {
            *o = a + b;
        }
    }
}

/// SIMD-accelerated element-wise subtraction: out\[i\] = a\[i\] - b\[i\].
pub(crate) struct SubF32<'a> {
    pub a: &'a [f32],
    pub b: &'a [f32],
    pub out: &'a mut [f32],
}

impl WithSimd for SubF32<'_> {
    type Output = ();

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) {
        let (a_head, a_tail) = S::as_simd_f32s(self.a);
        let (b_head, b_tail) = S::as_simd_f32s(self.b);
        let (out_head, out_tail) = S::as_mut_simd_f32s(self.out);

        for ((o, va), vb) in out_head.iter_mut().zip(a_head).zip(b_head) {
            *o = simd.sub_f32s(*va, *vb);
        }
        for ((o, a), b) in out_tail.iter_mut().zip(a_tail).zip(b_tail) {
            *o = a - b;
        }
    }
}

/// SIMD-accelerated scalar multiplication: out\[i\] = a\[i\] * scalar.
pub(crate) struct ScaleF32<'a> {
    pub a: &'a [f32],
    pub scalar: f32,
    pub out: &'a mut [f32],
}

impl WithSimd for ScaleF32<'_> {
    type Output = ();

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) {
        let (a_head, a_tail) = S::as_simd_f32s(self.a);
        let (out_head, out_tail) = S::as_mut_simd_f32s(self.out);
        let vs = simd.splat_f32s(self.scalar);

        for (o, va) in out_head.iter_mut().zip(a_head) {
            *o = simd.mul_f32s(*va, vs);
        }
        for (o, a) in out_tail.iter_mut().zip(a_tail) {
            *o = a * self.scalar;
        }
    }
}

// ============================================================
// Reductions
// ============================================================

/// SIMD-accelerated sum reduction for f32.
pub(crate) struct ReduceSumF32<'a> {
    pub a: &'a [f32],
}

impl WithSimd for ReduceSumF32<'_> {
    type Output = f32;

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) -> f32 {
        let (head, tail) = S::as_simd_f32s(self.a);

        let mut acc = simd.splat_f32s(0.0);
        for v in head {
            acc = simd.add_f32s(acc, *v);
        }

        let mut sum = simd.reduce_sum_f32s(acc);
        for &x in tail {
            sum += x;
        }
        sum
    }
}

/// SIMD-accelerated sum reduction for f64.
pub(crate) struct ReduceSumF64<'a> {
    pub a: &'a [f64],
}

impl WithSimd for ReduceSumF64<'_> {
    type Output = f64;

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) -> f64 {
        let (head, tail) = S::as_simd_f64s(self.a);

        let mut acc = simd.splat_f64s(0.0);
        for v in head {
            acc = simd.add_f64s(acc, *v);
        }

        let mut sum = simd.reduce_sum_f64s(acc);
        for &x in tail {
            sum += x;
        }
        sum
    }
}

// ============================================================
// Cosine Similarity — Single-Pass with Mixed Precision
// ============================================================

/// Single-pass cosine similarity with 3 accumulators and f64 mixed-precision reduction.
///
/// Computes dot(a,b), ||a||², ||b||² in ONE pass over the data (3x fewer cache misses
/// vs the naive 3-pass approach). FMA is used for all accumulations.
/// Final reduction uses f64 to avoid precision loss on high-dimensional vectors.
pub(crate) struct CosineSinglePass<'a> {
    pub a: &'a [f32],
    pub b: &'a [f32],
}

impl WithSimd for CosineSinglePass<'_> {
    type Output = f64;

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) -> f64 {
        if self.a.len() != self.b.len() || self.a.is_empty() {
            return 0.0;
        }

        let (a_head, a_tail) = S::as_simd_f32s(self.a);
        let (b_head, b_tail) = S::as_simd_f32s(self.b);

        let mut dot_acc = simd.splat_f32s(0.0);
        let mut norm_a_acc = simd.splat_f32s(0.0);
        let mut norm_b_acc = simd.splat_f32s(0.0);

        for (va, vb) in a_head.iter().zip(b_head) {
            dot_acc = simd.mul_add_f32s(*va, *vb, dot_acc);
            norm_a_acc = simd.mul_add_f32s(*va, *va, norm_a_acc);
            norm_b_acc = simd.mul_add_f32s(*vb, *vb, norm_b_acc);
        }

        // Mixed precision reduction to f64
        let mut dot = simd.reduce_sum_f32s(dot_acc) as f64;
        let mut na = simd.reduce_sum_f32s(norm_a_acc) as f64;
        let mut nb = simd.reduce_sum_f32s(norm_b_acc) as f64;

        // Scalar tail
        for (a, b) in a_tail.iter().zip(b_tail) {
            dot += (*a as f64) * (*b as f64);
            na += (*a as f64) * (*a as f64);
            nb += (*b as f64) * (*b as f64);
        }

        let denom = na.sqrt() * nb.sqrt();
        if denom == 0.0 { 0.0 } else { dot / denom }
    }
}

// ============================================================
// Nibble Block Dot Product (AWQ/GGUF-style)
// ============================================================

/// SIMD-accelerated dot product for a block of nibble-quantized vectors.
///
/// Computes Σ(a_nibble * b_nibble), Σ(a_nibble), Σ(b_nibble) in one pass
/// over the nibble-packed bytes. Uses pulp for multi-ISA dispatch.
///
/// Each byte stores 2 nibbles: hi (bits 4-7) and lo (bits 0-3).
/// Even nibble index → hi nibble, odd → lo nibble.
#[cfg(feature = "quantization")]
pub(crate) struct DotBlockNibble<'a> {
    /// Nibble-packed bytes for vector a (2 nibbles per byte).
    pub a: &'a [u8],
    /// Nibble-packed bytes for vector b.
    pub b: &'a [u8],
}

#[cfg(feature = "quantization")]
impl WithSimd for DotBlockNibble<'_> {
    type Output = (i32, i32, i32);

    #[inline(always)]
    fn with_simd<S: Simd>(self, _simd: S) -> (i32, i32, i32) {
        // Nibble extraction requires scalar byte operations (>>4, &0x0F) that do
        // not map directly to pulp SIMD vector types. Processing byte-by-byte here
        // lets the compiler auto-vectorize via the dispatched ISA (AVX2/SSE4/NEON).

        let mut sum_products: i32 = 0;
        let mut sum_a: i32 = 0;
        let mut sum_b: i32 = 0;

        for i in 0..self.a.len() {
            let a_byte = self.a[i];
            let b_byte = self.b[i];

            let hi_a = (a_byte >> 4) & 0x0F;
            let lo_a = a_byte & 0x0F;
            let hi_b = (b_byte >> 4) & 0x0F;
            let lo_b = b_byte & 0x0F;

            sum_products += (hi_a as i32) * (hi_b as i32);
            sum_products += (lo_a as i32) * (lo_b as i32);
            sum_a += hi_a as i32;
            sum_a += lo_a as i32;
            sum_b += hi_b as i32;
            sum_b += lo_b as i32;
        }

        (sum_products, sum_a, sum_b)
    }
}

// ============================================================
// Manhattan Distance (L1)
// ============================================================

/// SIMD-accelerated Manhattan distance: sum(|a\[i\] - b\[i\]|).
pub(crate) struct ManhattanF32<'a> {
    pub a: &'a [f32],
    pub b: &'a [f32],
}

impl WithSimd for ManhattanF32<'_> {
    type Output = f32;

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) -> f32 {
        let (a_head, a_tail) = S::as_simd_f32s(self.a);
        let (b_head, b_tail) = S::as_simd_f32s(self.b);

        let mut acc = simd.splat_f32s(0.0);
        for (va, vb) in a_head.iter().zip(b_head) {
            let diff = simd.sub_f32s(*va, *vb);
            acc = simd.add_f32s(acc, simd.abs_f32s(diff));
        }

        let mut sum = simd.reduce_sum_f32s(acc);
        for (a, b) in a_tail.iter().zip(b_tail) {
            sum += (a - b).abs();
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simd_utils::dispatch::arch;
    use approx::assert_relative_eq;

    #[test]
    fn test_dot_f32_basic() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![1.0, 1.0, 1.0, 1.0];
        let result = arch().dispatch(DotF32 { a: &a, b: &b });
        assert_relative_eq!(result, 10.0, epsilon = 1e-6);
    }

    #[test]
    fn test_dot_f32_large() {
        let size = 1536;
        let a: Vec<f32> = (0..size).map(|i| i as f32 * 0.001).collect();
        let b: Vec<f32> = (0..size).map(|i| (size - i) as f32 * 0.001).collect();
        let result = arch().dispatch(DotF32 { a: &a, b: &b });
        let expected: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        assert_relative_eq!(result, expected, epsilon = 1e-2);
    }

    #[test]
    fn test_dot_f32_remainder() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]; // 7 elements
        let b = vec![7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        let result = arch().dispatch(DotF32 { a: &a, b: &b });
        let expected: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        assert_relative_eq!(result, expected, epsilon = 1e-6);
    }

    #[test]
    fn test_dot_f32_empty() {
        let result = arch().dispatch(DotF32 { a: &[], b: &[] });
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_dot_f64_basic() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![4.0, 3.0, 2.0, 1.0];
        let result = arch().dispatch(DotF64 { a: &a, b: &b });
        assert_relative_eq!(result, 20.0, epsilon = 1e-10);
    }

    #[test]
    fn test_sqeuclidean_basic() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        let result = arch().dispatch(SqEuclideanF32 { a: &a, b: &b });
        assert_relative_eq!(result, 25.0, epsilon = 1e-6);
    }

    #[test]
    fn test_add_f32() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let b = vec![9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        let mut out = vec![0.0; 9];
        arch().dispatch(AddF32 {
            a: &a,
            b: &b,
            out: &mut out,
        });
        assert!(out.iter().all(|&x| (x - 10.0).abs() < 1e-6));
    }

    #[test]
    fn test_sub_f32() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let mut out = vec![0.0; 5];
        arch().dispatch(SubF32 {
            a: &a,
            b: &b,
            out: &mut out,
        });
        let expected = vec![-4.0, -2.0, 0.0, 2.0, 4.0];
        for (o, e) in out.iter().zip(expected.iter()) {
            assert_relative_eq!(o, e, epsilon = 1e-6);
        }
    }

    #[test]
    fn test_scale_f32() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let mut out = vec![0.0; 9];
        arch().dispatch(ScaleF32 {
            a: &a,
            scalar: 2.0,
            out: &mut out,
        });
        let expected: Vec<f32> = a.iter().map(|x| x * 2.0).collect();
        for (o, e) in out.iter().zip(expected.iter()) {
            assert_relative_eq!(o, e, epsilon = 1e-6);
        }
    }

    #[test]
    fn test_reduce_sum_f32() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let result = arch().dispatch(ReduceSumF32 { a: &a });
        assert_relative_eq!(result, 55.0, epsilon = 1e-6);
    }

    #[test]
    fn test_reduce_sum_f64() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = arch().dispatch(ReduceSumF64 { a: &a });
        assert_relative_eq!(result, 15.0, epsilon = 1e-10);
    }

    #[test]
    fn test_reduce_sum_empty() {
        let result = arch().dispatch(ReduceSumF32 { a: &[] });
        assert_eq!(result, 0.0);
    }

    // ── Cosine single-pass tests ─────────────────────────────

    #[test]
    fn test_cosine_identical() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let result = arch().dispatch(CosineSinglePass { a: &a, b: &a });
        assert_relative_eq!(result, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let result = arch().dispatch(CosineSinglePass { a: &a, b: &b });
        assert_relative_eq!(result, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let result = arch().dispatch(CosineSinglePass { a: &a, b: &b });
        assert_relative_eq!(result, -1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_cosine_empty() {
        let result = arch().dispatch(CosineSinglePass { a: &[], b: &[] });
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_cosine_high_dim_precision() {
        // 1536d vectors — tests mixed-precision f64 accumulation
        let size = 1536;
        let a: Vec<f32> = (0..size).map(|i| (i as f32 * 0.001).sin()).collect();
        let mut b = a.clone();
        b[0] += 0.1;
        let result = arch().dispatch(CosineSinglePass { a: &a, b: &b });
        assert!(result > 0.99 && result < 1.0);
    }

    #[test]
    fn test_cosine_zero_vector() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![0.0, 0.0, 0.0];
        let result = arch().dispatch(CosineSinglePass { a: &a, b: &b });
        assert_eq!(result, 0.0);
    }

    // ── Manhattan tests ──────────────────────────────────────

    #[test]
    fn test_manhattan_basic() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 6.0, 3.0];
        let result = arch().dispatch(ManhattanF32 { a: &a, b: &b });
        assert_relative_eq!(result, 7.0, epsilon = 1e-6);
    }

    #[test]
    fn test_manhattan_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let result = arch().dispatch(ManhattanF32 { a: &a, b: &a });
        assert_relative_eq!(result, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn test_manhattan_remainder() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]; // 7 elements
        let b = vec![7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        let result = arch().dispatch(ManhattanF32 { a: &a, b: &b });
        let expected: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum();
        assert_relative_eq!(result, expected, epsilon = 1e-6);
    }
}
