//! Centralized SIMD dispatch via `pulp`.
//!
//! Provides cached CPU feature detection and automatic dispatch to the
//! best available SIMD instruction set (AVX-512, AVX2+FMA, NEON, or scalar).
//!
//! All SIMD operations in this crate should use `arch()` for dispatch.

use pulp::Arch;

/// Return the cached CPU architecture for SIMD dispatch.
///
/// `Arch::new()` detects CPU features once and caches the result.
/// Subsequent calls are effectively free.
#[inline]
#[must_use]
pub fn arch() -> Arch {
    Arch::new()
}

/// Return a human-readable string describing the active SIMD backend.
#[must_use]
pub fn backend_name() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx512f") {
            return "AVX-512";
        }
        if std::arch::is_x86_feature_detected!("avx2") {
            if std::arch::is_x86_feature_detected!("fma") {
                return "AVX2+FMA";
            }
            return "AVX2";
        }
        if std::arch::is_x86_feature_detected!("sse4.1") {
            return "SSE4.1";
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        return "NEON (aarch64)";
    }
    "scalar"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arch_creation() {
        let _a = arch();
        // Must not panic
    }

    #[test]
    fn test_backend_name_not_empty() {
        let name = backend_name();
        assert!(!name.is_empty());
    }
}
