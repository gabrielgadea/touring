//! SIMD vector operations with automatic multi-ISA dispatch.
//!
//! All operations use `pulp` for automatic dispatch to AVX-512, AVX2+FMA,
//! NEON, or scalar fallback. FMA (fused multiply-add) is used where available.
//!
//! These are the primary public API functions for SIMD vector operations.

/// Compute dot product of two f32 slices with automatic SIMD dispatch.
///
/// Uses `pulp` for automatic dispatch to AVX-512, AVX2+FMA, NEON, or scalar
/// depending on CPU capabilities. FMA (fused multiply-add) is used where available.
///
/// # Panics
///
/// Debug-asserts that `a.len() == b.len()`.
#[inline]
#[must_use]
pub fn simd_dot_f32(a: &[f32], b: &[f32]) -> f32 {
    super::dispatch::arch().dispatch(super::ops::DotF32 { a, b })
}

/// Compute L2 norm (Euclidean length) of an f32 slice.
///
/// Equivalent to `sqrt(dot(a, a))`. Uses automatic SIMD dispatch.
#[inline]
#[must_use]
pub fn simd_norm_f32(a: &[f32]) -> f32 {
    simd_dot_f32(a, a).sqrt()
}

/// Element-wise addition: `out\[i\] = a\[i\] + b\[i\]` with automatic SIMD dispatch.
#[inline]
pub fn simd_add_f32(a: &[f32], b: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), out.len());
    super::dispatch::arch().dispatch(super::ops::AddF32 { a, b, out })
}

/// Element-wise subtraction: `out\[i\] = a\[i\] - b\[i\]` with automatic SIMD dispatch.
#[inline]
pub fn simd_sub_f32(a: &[f32], b: &[f32], out: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), out.len());
    super::dispatch::arch().dispatch(super::ops::SubF32 { a, b, out })
}

/// Scalar multiplication: `out\[i\] = a\[i\] * scalar` with automatic SIMD dispatch.
#[inline]
pub fn simd_scale_f32(a: &[f32], scalar: f32, out: &mut [f32]) {
    debug_assert_eq!(a.len(), out.len());
    super::dispatch::arch().dispatch(super::ops::ScaleF32 { a, scalar, out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_dot_product_simple() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![1.0, 1.0, 1.0, 1.0];
        assert_relative_eq!(simd_dot_f32(&a, &b), 10.0, epsilon = 1e-6);
    }

    #[test]
    fn test_dot_product_large() {
        let size = 1536;
        let a: Vec<f32> = (0..size).map(|i| i as f32 * 0.001).collect();
        let b: Vec<f32> = (0..size).map(|i| (size - i) as f32 * 0.001).collect();

        let result = simd_dot_f32(&a, &b);

        let expected: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        assert_relative_eq!(result, expected, epsilon = 1e-2);
    }

    #[test]
    fn test_norm() {
        let a = vec![3.0, 4.0];
        assert_relative_eq!(simd_norm_f32(&a), 5.0, epsilon = 1e-6);
    }

    #[test]
    fn test_add() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let b = vec![9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        let mut out = vec![0.0; 9];
        simd_add_f32(&a, &b, &mut out);
        assert!(out.iter().all(|&x| (x - 10.0).abs() < 1e-6));
    }

    #[test]
    fn test_scale() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let mut out = vec![0.0; 9];
        simd_scale_f32(&a, 2.0, &mut out);
        let expected: Vec<f32> = a.iter().map(|x| x * 2.0).collect();
        for (o, e) in out.iter().zip(expected.iter()) {
            assert_relative_eq!(o, e, epsilon = 1e-6);
        }
    }

    #[test]
    fn test_sub() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let mut out = vec![0.0; 5];
        simd_sub_f32(&a, &b, &mut out);
        let expected = vec![-4.0, -2.0, 0.0, 2.0, 4.0];
        for (o, e) in out.iter().zip(expected.iter()) {
            assert_relative_eq!(o, e, epsilon = 1e-6);
        }
    }

    #[test]
    fn test_dot_product_scalar_remainder() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let b = vec![7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        let result = simd_dot_f32(&a, &b);
        let expected: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        assert_relative_eq!(result, expected, epsilon = 1e-6);
    }

    #[test]
    fn test_add_scalar_remainder() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let b = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let mut out = vec![0.0; 7];
        simd_add_f32(&a, &b, &mut out);
        for (i, o) in out.iter().enumerate() {
            assert_relative_eq!(o, &(a[i] + b[i]), epsilon = 1e-6);
        }
    }

    #[test]
    fn test_dot_product_empty() {
        assert_eq!(simd_dot_f32(&[], &[]), 0.0);
    }

    #[test]
    fn test_norm_single() {
        let a = vec![5.0];
        assert_relative_eq!(simd_norm_f32(&a), 5.0, epsilon = 1e-6);
    }
}
