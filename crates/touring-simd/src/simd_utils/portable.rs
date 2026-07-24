#![allow(clippy::indexing_slicing)]

//! Portable SIMD-accelerated operations via `pulp`.
//!
//! All functions delegate to `pulp::WithSimd` implementations in `ops.rs`,
//! which automatically dispatch to the best available SIMD ISA
//! (AVX-512, AVX2+FMA, NEON, or scalar fallback).
//!
//! These functions exist for backward compatibility — new code should
//! prefer the dispatch functions in `vector_ops.rs` or use `ops.rs` directly.

use super::dispatch::arch;
use super::ops;

/// Compute dot product of two f32 slices via pulp SIMD dispatch.
#[inline]
#[must_use]
pub fn portable_dot_f32(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have equal length");
    arch().dispatch(ops::DotF32 { a, b })
}

/// Compute dot product of two f64 slices via pulp SIMD dispatch.
#[inline]
#[must_use]
pub fn portable_dot_f64(a: &[f64], b: &[f64]) -> f64 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have equal length");
    arch().dispatch(ops::DotF64 { a, b })
}

/// Compute L2 norm of an f32 slice.
#[inline]
#[must_use]
pub fn portable_norm_f32(a: &[f32]) -> f32 {
    portable_dot_f32(a, a).sqrt()
}

/// Compute L2 norm of an f64 slice.
#[inline]
#[must_use]
pub fn portable_norm_f64(a: &[f64]) -> f64 {
    portable_dot_f64(a, a).sqrt()
}

/// Element-wise addition for f32: out\[i\] = a\[i\] + b\[i\]
#[inline]
pub fn portable_add_f32(a: &[f32], b: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), out.len());
    arch().dispatch(ops::AddF32 { a, b, out })
}

/// Element-wise addition for f64
#[inline]
pub fn portable_add_f64(a: &[f64], b: &[f64], out: &mut [f64]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), out.len());
    for i in 0..a.len() {
        out[i] = a[i] + b[i];
    }
}

/// Element-wise subtraction for f32: out\[i\] = a\[i\] - b\[i\]
#[inline]
pub fn portable_sub_f32(a: &[f32], b: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), out.len());
    arch().dispatch(ops::SubF32 { a, b, out })
}

/// Scalar multiplication for f32: out\[i\] = a\[i\] * scalar
#[inline]
pub fn portable_scale_f32(a: &[f32], scalar: f32, out: &mut [f32]) {
    debug_assert_eq!(a.len(), out.len());
    arch().dispatch(ops::ScaleF32 { a, scalar, out })
}

/// Compute squared Euclidean distance between two f32 vectors.
#[inline]
#[must_use]
pub fn portable_sqeuclidean_f32(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    arch().dispatch(ops::SqEuclideanF32 { a, b })
}

/// Reduce sum for f32 via pulp SIMD dispatch.
#[inline]
#[must_use]
pub fn portable_reduce_sum_f32(a: &[f32]) -> f32 {
    arch().dispatch(ops::ReduceSumF32 { a })
}

/// Reduce max for f32.
#[inline]
#[must_use]
pub fn portable_reduce_max_f32(a: &[f32]) -> f32 {
    if a.is_empty() {
        return f32::NEG_INFINITY;
    }
    let mut max = a[0];
    for &x in &a[1..] {
        if x > max {
            max = x;
        }
    }
    max
}

/// Reduce min for f32.
#[inline]
#[must_use]
pub fn portable_reduce_min_f32(a: &[f32]) -> f32 {
    if a.is_empty() {
        return f32::INFINITY;
    }
    let mut min = a[0];
    for &x in &a[1..] {
        if x < min {
            min = x;
        }
    }
    min
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_portable_dot_f32() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let b = vec![8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        let result = portable_dot_f32(&a, &b);
        assert_relative_eq!(result, 120.0, epsilon = 1e-6);
    }

    #[test]
    fn test_portable_dot_f64() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![4.0, 3.0, 2.0, 1.0];
        let result = portable_dot_f64(&a, &b);
        assert_relative_eq!(result, 20.0, epsilon = 1e-10);
    }

    #[test]
    fn test_portable_norm_f32() {
        let a = vec![3.0, 4.0];
        assert_relative_eq!(portable_norm_f32(&a), 5.0, epsilon = 1e-6);
    }

    #[test]
    fn test_portable_add_f32() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let b = vec![8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        let mut out = vec![0.0; 8];
        portable_add_f32(&a, &b, &mut out);
        for o in &out {
            assert_relative_eq!(*o, 9.0, epsilon = 1e-6);
        }
    }

    #[test]
    fn test_portable_scale_f32() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut out = vec![0.0; 8];
        portable_scale_f32(&a, 2.0, &mut out);
        let expected: Vec<f32> = a.iter().map(|x| x * 2.0).collect();
        for (o, e) in out.iter().zip(expected.iter()) {
            assert_relative_eq!(*o, *e, epsilon = 1e-6);
        }
    }

    #[test]
    fn test_portable_reduce_max() {
        let a = vec![1.0, 5.0, 3.0, 9.0, 2.0];
        assert_relative_eq!(portable_reduce_max_f32(&a), 9.0, epsilon = 1e-6);
    }

    #[test]
    fn test_portable_reduce_min() {
        let a = vec![5.0, 1.0, 3.0, 9.0, 2.0];
        assert_relative_eq!(portable_reduce_min_f32(&a), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_portable_dot_f32_remainder() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let b = vec![7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        let result = portable_dot_f32(&a, &b);
        assert_relative_eq!(
            result,
            7.0 + 12.0 + 15.0 + 16.0 + 15.0 + 12.0 + 7.0,
            epsilon = 1e-6
        );
    }
}
