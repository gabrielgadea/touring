//! Horizontal reduction operations.
//!
//! Provides reduce_sum, reduce_max, reduce_min, argmax, argmin.
//! Sum reductions use pulp SIMD dispatch; max/min/arg* use scalar
//! (index tracking is inherently sequential).

use super::dispatch::arch;
use super::ops;

/// Compute sum of all elements via pulp SIMD dispatch.
///
/// Returns 0.0 for empty slices.
#[inline]
#[must_use]
pub fn reduce_sum_f32(a: &[f32]) -> f32 {
    if a.is_empty() {
        return 0.0;
    }
    arch().dispatch(ops::ReduceSumF32 { a })
}

/// Find maximum element.
///
/// Returns `f32::NEG_INFINITY` for empty slices.
#[inline]
#[must_use]
#[allow(clippy::indexing_slicing)]
pub fn reduce_max_f32(a: &[f32]) -> f32 {
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

/// Find minimum element.
///
/// Returns `f32::INFINITY` for empty slices.
#[inline]
#[must_use]
#[allow(clippy::indexing_slicing)]
pub fn reduce_min_f32(a: &[f32]) -> f32 {
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

/// Find index of maximum element. Returns `None` for empty slices.
#[inline]
#[must_use]
#[allow(clippy::indexing_slicing)]
pub fn argmax_f32(a: &[f32]) -> Option<usize> {
    if a.is_empty() {
        return None;
    }

    let mut max_idx = 0;
    let mut max_val = a[0];

    for (i, &x) in a.iter().enumerate().skip(1) {
        if x > max_val {
            max_val = x;
            max_idx = i;
        }
    }

    Some(max_idx)
}

/// Find index of minimum element. Returns `None` for empty slices.
#[inline]
#[must_use]
#[allow(clippy::indexing_slicing)]
pub fn argmin_f32(a: &[f32]) -> Option<usize> {
    if a.is_empty() {
        return None;
    }

    let mut min_idx = 0;
    let mut min_val = a[0];

    for (i, &x) in a.iter().enumerate().skip(1) {
        if x < min_val {
            min_val = x;
            min_idx = i;
        }
    }

    Some(min_idx)
}

/// Compute sum of f64 array via pulp SIMD dispatch.
///
/// Returns 0.0 for empty slices.
#[inline]
#[must_use]
pub fn reduce_sum_f64(a: &[f64]) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    arch().dispatch(ops::ReduceSumF64 { a })
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_reduce_sum() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_relative_eq!(reduce_sum_f32(&a), 55.0, epsilon = 1e-6);
    }

    #[test]
    fn test_reduce_sum_empty() {
        let a: Vec<f32> = vec![];
        assert_eq!(reduce_sum_f32(&a), 0.0);
    }

    #[test]
    fn test_reduce_max() {
        let a = vec![1.0, 5.0, 3.0, 9.0, 2.0];
        assert_relative_eq!(reduce_max_f32(&a), 9.0, epsilon = 1e-6);
    }

    #[test]
    fn test_reduce_min() {
        let a = vec![5.0, 1.0, 3.0, 9.0, 2.0];
        assert_relative_eq!(reduce_min_f32(&a), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn test_argmax() {
        let a = vec![1.0, 5.0, 3.0, 9.0, 2.0];
        assert_eq!(argmax_f32(&a), Some(3));
    }

    #[test]
    fn test_argmin() {
        let a = vec![5.0, 1.0, 3.0, 9.0, 2.0];
        assert_eq!(argmin_f32(&a), Some(1));
    }

    #[test]
    fn test_reduce_sum_f64() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_relative_eq!(reduce_sum_f64(&a), 15.0, epsilon = 1e-10);
    }
}
