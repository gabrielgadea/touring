//! Fast non-cryptographic content hashing via StringZilla AES hash.
//!
//! Provides a 64-bit hash suitable as a **pre-filter** before committing to
//! a heavier cryptographic hash (e.g. blake3).  The underlying
//! `sz_hash` routine selects SWAR → AES-NI → AVX-512 kernels at runtime,
//! making it 3-10× faster than a software CRC-32 on typical x86-64 hardware.
//!
//! ## Usage pattern
//!
//! ```rust
//! use touring_analysis::quality::fast_content_hash;
//!
//! let h1 = fast_content_hash("fn foo() {}");
//! let h2 = fast_content_hash("fn foo() {}");
//! let h3 = fast_content_hash("fn bar() {}");
//!
//! assert_eq!(h1, h2, "same content → same hash");
//! assert_ne!(h1, h3, "different content → different hash (probabilistic)");
//! ```
//!
//! ## Security note
//!
//! This is **NOT** a cryptographic hash.  Do not use for signatures,
//! MACs, or security-sensitive deduplication.  Use blake3 for those.

use stringzilla::stringzilla::hash as sz_hash;

/// Compute a fast 64-bit AES-based hash of `content`.
///
/// Uses StringZilla's `sz_hash` which dispatches to SWAR/AES-NI/AVX-512
/// at runtime.  Deterministic and order-sensitive — identical inputs always
/// produce identical outputs within a single binary build.
///
/// # Use case
///
/// Hot-path change detection: compare `fast_content_hash` first; only invoke
/// blake3 (or file I/O) when hashes differ.  Expected collision rate is
/// ~1 in 2^64 per pair, negligible for code-change detection.
///
/// # Example
///
/// ```rust
/// use touring_analysis::quality::fast_content_hash;
///
/// let unchanged = fast_content_hash("let x = 1;");
/// assert_eq!(unchanged, fast_content_hash("let x = 1;"));
/// ```
#[inline]
pub fn fast_content_hash(content: &str) -> u64 {
    sz_hash(content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_content_yields_same_hash() {
        let h1 = fast_content_hash("fn foo() -> i32 { 42 }");
        let h2 = fast_content_hash("fn foo() -> i32 { 42 }");
        assert_eq!(h1, h2, "deterministic: same input must produce same output");
    }

    #[test]
    fn different_content_yields_different_hash() {
        let h1 = fast_content_hash("fn foo() {}");
        let h2 = fast_content_hash("fn bar() {}");
        assert_ne!(
            h1, h2,
            "distinct inputs should (almost certainly) hash differently"
        );
    }

    #[test]
    fn empty_string_does_not_panic() {
        // sz_hash on a zero-length slice must not panic or read out-of-bounds.
        let h = fast_content_hash("");
        // Result is deterministic — just verify it doesn't panic and is stable.
        assert_eq!(h, fast_content_hash(""));
    }

    #[test]
    fn single_byte_change_detected() {
        let base = "let x = 1;";
        let changed = "let x = 2;";
        assert_ne!(
            fast_content_hash(base),
            fast_content_hash(changed),
            "single-byte change must produce different hash"
        );
    }

    #[test]
    fn large_content_does_not_panic() {
        let large = "fn foo() { let x = 1; }\n".repeat(10_000);
        let h1 = fast_content_hash(&large);
        let h2 = fast_content_hash(&large);
        assert_eq!(h1, h2, "large content must be stable");
    }
}
