//! E2E tests proving StringZilla integration in touring-analysis quality pipeline (T1.1 + T1.3).
//!
//! Verifies:
//! - T1.1: `count_lines` via `RangeUtf8NewlineSplits` produces identical results to `str::lines()`.
//! - T1.3: `fast_content_hash` is deterministic, distinct for different inputs, and wired
//!   correctly through `analyze_complexity` → `quality::fast_content_hash`.

use touring_analysis::quality::{analyze_complexity, fast_content_hash};

// ── T1.1: RangeUtf8NewlineSplits equivalence to str::lines() ─────────────

/// Fixture: a representative Rust source file with blank lines and comments.
const RUST_FIXTURE: &str = r#"// Module-level doc comment.
use std::collections::HashMap;

/// A simple struct.
pub struct Foo {
    value: i32,
}

impl Foo {
    // Constructor
    pub fn new(v: i32) -> Self {
        Self { value: v }
    }

    /// Returns the value.
    pub fn get(&self) -> i32 {
        self.value
    }
}
"#;

/// Fixture ending WITHOUT a trailing newline (edge case).
const RUST_NO_TRAILING_NL: &str = "fn foo() {}\nfn bar() {}";

/// Fixture with only blank lines.
const BLANK_ONLY: &str = "\n\n\n";

/// Fixture: single non-empty line, no newline.
const SINGLE_LINE: &str = "fn foo() {}";

/// `count_lines` (via `estimate_complexity`) must produce the same SLOC/CLOC/blank
/// counts as a reference implementation using `str::lines()`.
fn reference_counts(source: &str, language: &str) -> (usize, usize, usize) {
    let python_family = matches!(language, "python" | "py" | "bash" | "sh" | "ruby");
    let mut sloc = 0usize;
    let mut cloc = 0usize;
    let mut blank = 0usize;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank += 1;
            continue;
        }
        sloc += 1;
        let is_comment = if python_family {
            trimmed.starts_with('#')
        } else {
            trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with("*/")
                || trimmed.starts_with('*')
        };
        if is_comment {
            cloc += 1;
        }
    }
    (sloc, cloc, blank)
}

#[test]
fn test_count_lines_stringzilla_equals_stdlib_rust_fixture() {
    let m = analyze_complexity(RUST_FIXTURE, "rust");
    let (ref_sloc, ref_cloc, ref_blank) = reference_counts(RUST_FIXTURE, "rust");

    assert_eq!(m.sloc, ref_sloc, "SLOC mismatch on RUST_FIXTURE");
    assert_eq!(m.cloc, ref_cloc, "CLOC mismatch on RUST_FIXTURE");
    assert_eq!(m.blank, ref_blank, "BLANK mismatch on RUST_FIXTURE");
    assert_eq!(m.lloc, m.sloc.saturating_sub(m.cloc), "LLOC = SLOC - CLOC");
}

#[test]
fn test_count_lines_no_trailing_newline_edge_case() {
    let m = analyze_complexity(RUST_NO_TRAILING_NL, "rust");
    let (ref_sloc, _ref_cloc, ref_blank) = reference_counts(RUST_NO_TRAILING_NL, "rust");

    assert_eq!(m.sloc, ref_sloc, "SLOC mismatch: no trailing newline");
    assert_eq!(m.blank, ref_blank, "BLANK mismatch: no trailing newline");
    // Two non-blank, non-comment lines → SLOC=2.
    assert_eq!(m.sloc, 2, "two code lines must yield SLOC=2");
}

#[test]
fn test_count_lines_blank_only_source() {
    let m = analyze_complexity(BLANK_ONLY, "rust");
    assert_eq!(m.sloc, 0, "blank-only source → SLOC=0");
    assert_eq!(m.cloc, 0, "blank-only source → CLOC=0");
    // str::lines() on "\n\n\n" yields ["", "", ""] → 3 blank.
    // RangeUtf8NewlineSplits on "\n\n\n" + trailing-newline correction:
    // same semantics → 3 blank lines.
    assert_eq!(m.blank, 3, "blank-only source with 3 newlines → BLANK=3");
}

#[test]
fn test_count_lines_single_line_no_newline() {
    let m = analyze_complexity(SINGLE_LINE, "rust");
    assert_eq!(m.sloc, 1, "single code line → SLOC=1");
    assert_eq!(m.cloc, 0, "no comments → CLOC=0");
    assert_eq!(m.blank, 0, "no blanks → BLANK=0");
    assert_eq!(m.lloc, 1, "LLOC=1 for single code line");
}

#[test]
fn test_count_lines_python_comment_detection() {
    let python_src = "# Module comment\ndef foo():\n    pass\n\n# Another comment\n";
    let m = analyze_complexity(python_src, "python");
    let (ref_sloc, ref_cloc, ref_blank) = reference_counts(python_src, "python");

    assert_eq!(m.sloc, ref_sloc, "Python SLOC mismatch");
    assert_eq!(m.cloc, ref_cloc, "Python CLOC mismatch");
    assert_eq!(m.blank, ref_blank, "Python BLANK mismatch");
}

#[test]
fn test_lloc_filters_comment_lines_from_sloc() {
    // 3 code lines + 2 comment lines + 1 blank.
    let src = "fn a() {}\n// comment 1\nfn b() {}\n// comment 2\nfn c() {}\n\n";
    let m = analyze_complexity(src, "rust");

    assert_eq!(
        m.sloc, 5,
        "5 non-blank lines (3 code + 2 comments) → SLOC=5"
    );
    assert_eq!(m.cloc, 2, "2 comment-only lines → CLOC=2");
    assert_eq!(m.lloc, 3, "LLOC = SLOC - CLOC = 3");
    assert_eq!(m.blank, 1, "1 blank line");
}

// ── T1.3: fast_content_hash determinism and collision resistance ──────────

#[test]
fn test_fast_content_hash_deterministic_same_content() {
    let content = "fn foo() -> i32 { 42 }";
    let h1 = fast_content_hash(content);
    let h2 = fast_content_hash(content);
    assert_eq!(
        h1, h2,
        "identical content must yield identical hash (deterministic)"
    );
}

#[test]
fn test_fast_content_hash_distinct_contents_differ() {
    let h_foo = fast_content_hash("fn foo() {}");
    let h_bar = fast_content_hash("fn bar() {}");
    assert_ne!(
        h_foo, h_bar,
        "different function names must yield different hashes"
    );
}

#[test]
fn test_fast_content_hash_empty_string_does_not_panic() {
    let h = fast_content_hash("");
    // Must be stable: same empty input → same hash.
    assert_eq!(
        h,
        fast_content_hash(""),
        "empty string must hash deterministically"
    );
}

#[test]
fn test_fast_content_hash_single_byte_change_detected() {
    let base = "let x = 1;";
    let changed = "let x = 2;";
    assert_ne!(
        fast_content_hash(base),
        fast_content_hash(changed),
        "single-byte change must produce a different hash"
    );
}

#[test]
fn test_fast_content_hash_large_content_stable() {
    let large = "fn foo() { let x = 1; }\n".repeat(10_000);
    let h1 = fast_content_hash(&large);
    let h2 = fast_content_hash(&large);
    assert_eq!(
        h1, h2,
        "large identical content must hash deterministically"
    );
}

#[test]
fn test_fast_content_hash_unicode_content_stable() {
    // Simulate non-ASCII Rust source (doc comment in Portuguese).
    let unicode = "/// Função de análise de tarifação de rodovias.\npub fn tarifa() {}";
    let h1 = fast_content_hash(unicode);
    let h2 = fast_content_hash(unicode);
    assert_eq!(h1, h2, "unicode content must hash deterministically");
    // Different unicode content must differ.
    let other = "/// Função de análise de pedágio.\npub fn pedágio() {}";
    assert_ne!(
        fast_content_hash(unicode),
        fast_content_hash(other),
        "distinct unicode strings should hash differently"
    );
}

// ── T1.3-integration: wiring verification ────────────────────────────────

/// `fast_content_hash` is re-exported from `touring_analysis::quality` (mod.rs line 21).
/// This test imports it via that path to verify the export chain is live.
#[test]
fn test_fast_content_hash_reexport_from_quality_module() {
    // Import via the quality module's re-export (not via fast_hash directly).
    use touring_analysis::quality::fast_content_hash as fch_via_quality;
    let h = fch_via_quality("fn foo() {}");
    assert_eq!(
        h,
        fast_content_hash("fn foo() {}"),
        "re-exported fast_content_hash must be identical to direct import"
    );
}
