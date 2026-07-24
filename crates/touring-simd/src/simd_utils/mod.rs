//! SIMD utilities for vectorized operations.
//!
//! Pln3 Phase 1B - Layer 1: SIMD Foundation
//!
//! Provides portable SIMD operations with automatic fallback to scalar
//! on unsupported platforms. Uses manual loop unrolling for performance
//! while maintaining compatibility with stable Rust.
//!
//! # Architecture (4 layers)
//!
//! 1. **Portable SIMD** (`portable.rs`): `core_simd` crate for cross-platform
//!    SIMD that works on stable Rust. Lane counts: f32x8, f64x4.
//! 2. **AVX2 intrinsics** (`vector_ops.rs`): x86_64-specific with runtime dispatch
//! 3. **Matrix ops** (`matrix.rs`): Batch matrix-vector products, activations
//! 4. **Horizontal reductions** (`horizontal.rs`): Sum/max/min with 8-way unrolling
//!
//! # Performance
//!
//! All operations use 8-way (f32) or 4-way (f64) loop unrolling which
//! enables the compiler to auto-vectorize with AVX2/NEON when available.
//! Scalar remainder elements are handled transparently.

pub mod dispatch;
mod horizontal;
pub mod matrix;
pub(crate) mod ops;
mod portable;
mod vector_ops;

pub use dispatch::{arch, backend_name};
pub use horizontal::*;
pub use matrix::*;
pub use portable::*;
pub use vector_ops::simd_dot_f32;
pub use vector_ops::*;

/// SIMD lane width for f32 operations (8 for AVX2).
pub const SIMD_F32_LANES: usize = 8;

/// SIMD lane width for f64 operations (4 for AVX2).
pub const SIMD_F64_LANES: usize = 4;

/// Check if the current CPU supports AVX2.
#[inline]
#[must_use]
pub fn has_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Check if the current CPU supports NEON (ARM).
#[inline]
#[must_use]
pub fn has_neon() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        true // NEON is mandatory on aarch64
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        false
    }
}

/// Check if any hardware SIMD acceleration is available.
#[inline]
#[must_use]
pub fn has_simd() -> bool {
    has_avx2() || has_neon()
}

/// Return a human-readable string describing the active SIMD backend.
#[must_use]
pub fn simd_backend() -> &'static str {
    if has_avx2() {
        "AVX2 (x86_64)"
    } else if has_neon() {
        "NEON (aarch64)"
    } else {
        "scalar (loop unrolling)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_detection() {
        // These should not panic
        let _has_avx2 = has_avx2();
        let _has_neon = has_neon();
        let _has_simd = has_simd();
    }

    #[test]
    fn test_simd_backend_non_empty() {
        let backend = simd_backend();
        assert!(!backend.is_empty());
    }

    #[test]
    fn test_lane_constants() {
        assert_eq!(SIMD_F32_LANES, 8);
        assert_eq!(SIMD_F64_LANES, 4);
    }
}
