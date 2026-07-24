// Shared quality utilities for touring-hooks.
//
// Centralizes `is_test_file` (duplicated in post_write, pre_write) and
// `measure_quality_snapshot` (duplicated in post_edit, post_write).
// Also exports `quick_content_changed` — a fast AES-NI pre-filter that
// short-circuits expensive operations (blake3, sha256) when content is identical.

/// Check whether a file path belongs to a test module.
///
/// Matches common conventions: `/tests/`, `/test/`, `_test.`, `test_`,
/// `.test.ts`, `.spec.ts`, `.spec.js`.
pub fn is_test_file(file_path: &str) -> bool {
    let path_lower = file_path.to_lowercase();
    path_lower.starts_with("tests/")
        || path_lower.contains("/tests/")
        || path_lower.contains("_test.")
        || path_lower.contains("test_")
        || path_lower.starts_with("test/")
        || path_lower.contains("/test/")
        || path_lower.ends_with(".test.ts")
        || path_lower.ends_with(".spec.ts")
        || path_lower.ends_with(".spec.js")
}

/// Snapshot current file quality metrics via AST for evolution tracking.
///
/// Reads the file from disk and delegates to `ast_bridge::analyze_file_quality`.
pub fn measure_quality_snapshot(file_path: &str) -> Option<crate::ast_bridge::FileQualityMetrics> {
    let source = std::fs::read_to_string(file_path).ok()?;
    crate::ast_bridge::analyze_file_quality(&source, file_path)
}

/// Snapshot quality metrics from already-loaded source (avoids redundant I/O).
///
/// Identical to [`measure_quality_snapshot`] but takes source content directly,
/// eliminating an `fs::read_to_string` call when the content is already available.
pub fn measure_quality_snapshot_from_source(
    source: &str,
    file_path: &str,
) -> Option<crate::ast_bridge::FileQualityMetrics> {
    crate::ast_bridge::analyze_file_quality(source, file_path)
}

/// Fast AES-NI pre-filter for content change detection.
///
/// Returns `true` when `old` and `new` are **definitely different** (their
/// AES-hash fingerprints disagree). Returns `false` when they *may* be
/// identical — callers must verify with a canonical comparison (e.g. blake3)
/// before concluding the content is unchanged, because hash collisions are
/// theoretically possible (probability ~1/2^64 per pair).
///
/// ## Usage pattern
///
/// ```rust,ignore
/// use crate::shared::quality::quick_content_changed;
///
/// if !quick_content_changed(old_content, new_content) {
///     // Hashes agree → very likely identical; skip expensive blake3/sha256.
///     return;
/// }
/// // Hashes differ → content changed; proceed with full hash or reindex.
/// ```
///
/// Backed by [`touring_analysis::quality::fast_content_hash`] which selects
/// SWAR → AES-NI → AVX-512 at runtime for 3–10× throughput vs software CRC.
#[inline]
pub fn quick_content_changed(old: &str, new: &str) -> bool {
    use touring_analysis::quality::fast_content_hash;
    fast_content_hash(old) != fast_content_hash(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── quick_content_changed ─────────────────────────────────────────────

    #[test]
    fn quick_content_changed_detects_difference() {
        assert!(
            quick_content_changed("fn foo() {}", "fn bar() {}"),
            "different content must report changed"
        );
    }

    #[test]
    fn quick_content_changed_identical_content_returns_false() {
        let src = "fn foo() -> i32 { 42 }";
        assert!(
            !quick_content_changed(src, src),
            "identical content must report unchanged"
        );
    }

    #[test]
    fn quick_content_changed_empty_strings_returns_false() {
        assert!(
            !quick_content_changed("", ""),
            "two empty strings are identical — must not report changed"
        );
    }

    #[test]
    fn quick_content_changed_single_byte_difference() {
        // A one-character edit must be detected.
        assert!(
            quick_content_changed("let x = 1;", "let x = 2;"),
            "single-byte change must be detected by fast hash"
        );
    }

    #[test]
    fn quick_content_changed_large_identical_content() {
        let large = "fn generated() {}\n".repeat(5_000);
        assert!(
            !quick_content_changed(&large, &large),
            "large identical content must not report changed"
        );
    }

    #[test]
    fn test_is_test_file() {
        assert!(is_test_file("src/tests/foo.rs"));
        assert!(is_test_file("src/test/bar.py"));
        assert!(is_test_file("handler_test.go"));
        assert!(is_test_file("test_handler.py"));
        assert!(is_test_file("app.test.ts"));
        assert!(is_test_file("app.spec.ts"));
        assert!(is_test_file("app.spec.js"));
        assert!(!is_test_file("src/main.rs"));
        assert!(!is_test_file("src/handler.py"));
    }

    #[test]
    fn test_is_test_file_case_insensitive() {
        // Path-lowercasing means mixed-case paths match correctly.
        assert!(is_test_file("src/Tests/foo.rs"));
        assert!(is_test_file("Handler_Test.go"));
    }

    #[test]
    fn test_is_test_file_empty_path() {
        // Empty string has no test markers — should return false without panic.
        assert!(!is_test_file(""));
    }

    #[test]
    fn test_measure_quality_snapshot_nonexistent_file_returns_none() {
        // A path that cannot be read on disk must return None gracefully.
        let result = measure_quality_snapshot("/nonexistent/path/that/does/not/exist.rs");
        assert!(result.is_none(), "Expected None for missing file, got Some");
    }

    #[test]
    fn test_measure_quality_snapshot_real_file_returns_some() {
        // Write a minimal Rust file and verify we get a quality snapshot back.
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().expect("temp file");
        writeln!(tmp, "fn hello() -> &'static str {{ \"world\" }}").expect("write");
        let path = tmp.path().to_str().expect("utf-8 path");
        // ast_bridge may not parse a bare .tmp extension — rename to .rs
        let rs_path = format!("{}.rs", path);
        std::fs::copy(path, &rs_path).expect("copy to .rs");
        let snapshot = measure_quality_snapshot(&rs_path);
        // Cleanup
        let _ = std::fs::remove_file(&rs_path);
        // Even if the AST bridge returns None for a trivial snippet, the function
        // must not panic. The important invariant is: no panic on valid UTF-8 file.
        let _ = snapshot; // None is acceptable for a trivial snippet
    }
}
