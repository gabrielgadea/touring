//! Audit `.unwrap()` calls in source code.
//!
//! Uses `memchr::memmem` for fast byte-level scanning, then maps
//! byte offsets to line numbers.

use memchr::memmem;

/// Result of an unwrap audit.
#[derive(Debug, Clone)]
pub struct UnwrapAudit {
    /// Total count of `.unwrap()` calls.
    pub count: usize,
    /// Line numbers (1-indexed) containing `.unwrap()`.
    pub lines: Vec<usize>,
    /// Risk score: 0.0 (clean) to 1.0 (high risk).
    pub risk_score: f64,
}

/// Count `.unwrap()` calls in source code.
///
/// Returns the count and line numbers of each occurrence.
pub fn count_unwraps(source: &str) -> UnwrapAudit {
    let bytes = source.as_bytes();
    let finder = memmem::Finder::new(b".unwrap()");

    let mut lines = Vec::new();
    for offset in finder.find_iter(bytes) {
        let line = bytes
            .get(..offset)
            .map_or(1, |slice| slice.iter().filter(|&&b| b == b'\n').count() + 1);
        lines.push(line);
    }

    let count = lines.len();
    let total_lines = source.lines().count().max(1);
    // Risk: unwraps per 100 lines, capped at 1.0
    let risk_score = ((count as f64 / total_lines as f64) * 100.0 * 0.1).min(1.0);

    UnwrapAudit {
        count,
        lines,
        risk_score,
    }
}

/// Count `.expect(` calls in source code (less risky but still notable).
pub fn count_expects(source: &str) -> usize {
    memmem::find_iter(source.as_bytes(), b".expect(").count()
}

/// Production-only error-handling hazards: `.unwrap()`, `.expect(`, and
/// `panic!` occurrences that are **not** inside comments or
/// `#[cfg(test)]`/`#[test]` regions.
///
/// This is the faithful signal for the D06 error-handling dimension, which
/// exempts test code and documentation. Counting a test's `.unwrap()` or a
/// commented-out `panic!` as a *production* hazard is a false positive that the
/// raw [`count_unwraps`] / [`count_expects`] scanners cannot avoid — see
/// `test_unwrap_in_comment_still_counted` for the raw scanner's documented
/// limitation. Suppression mirrors the SAST gold standard (Semgrep ignores
/// commented-out code and excludes test paths) via [`super::code_regions`], but
/// at in-file region granularity (a production sink sharing a file with a
/// `#[cfg(test)]` module is still counted).
#[derive(Debug, Clone, Default)]
pub struct ProdHazards {
    /// Production `.unwrap()` calls (comment / test occurrences excluded).
    pub unwraps: usize,
    /// Production `.expect(` calls.
    pub expects: usize,
    /// Production `panic!` invocations.
    pub panics: usize,
    /// 1-indexed line numbers of the production `.unwrap()` calls.
    pub unwrap_lines: Vec<usize>,
    /// Total source lines (denominator for density-based scoring).
    pub total_lines: usize,
}

impl ProdHazards {
    /// Total hazard count (`unwraps + expects + panics`).
    #[must_use]
    pub fn total(&self) -> usize {
        self.unwraps + self.expects + self.panics
    }
}

/// Count production-only error-handling hazards, excluding comment and
/// `#[cfg(test)]`/`#[test]` regions via [`super::code_regions`].
///
/// `lang` selects the comment/string lexer (`"rust"`, `"python"`,
/// `"typescript"`, `"go"`, …); only Rust additionally suppresses test regions.
/// Production string literals are intentionally *not* suppressed (consistent
/// with `code_regions`), so a `panic!` inside a production string is counted —
/// a rare, acceptable over-count, not a test/comment false positive.
#[must_use]
pub fn count_prod_hazards(source: &str, lang: &str) -> ProdHazards {
    let regions = super::code_regions::non_executable_regions(source, lang);
    let bytes = source.as_bytes();

    // Map a byte offset to its 1-indexed line number.
    let line_of = |offset: usize| -> usize {
        bytes
            .get(..offset)
            .map_or(1, |slice| slice.iter().filter(|&&b| b == b'\n').count() + 1)
    };

    let mut unwraps = 0usize;
    let mut unwrap_lines = Vec::new();
    for off in memmem::find_iter(bytes, b".unwrap()") {
        if super::code_regions::offset_suppressed(off, &regions) {
            continue;
        }
        unwraps += 1;
        unwrap_lines.push(line_of(off));
    }

    let expects = memmem::find_iter(bytes, b".expect(")
        .filter(|&off| !super::code_regions::offset_suppressed(off, &regions))
        .count();

    let panics = memmem::find_iter(bytes, b"panic!")
        .filter(|&off| !super::code_regions::offset_suppressed(off, &regions))
        .count();

    ProdHazards {
        unwraps,
        expects,
        panics,
        unwrap_lines,
        total_lines: source.lines().count().max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_unwraps() {
        let result = count_unwraps("fn main() -> Result<(), Error> { Ok(()) }");
        assert_eq!(result.count, 0);
        assert!(result.lines.is_empty());
        assert_eq!(result.risk_score, 0.0);
    }

    #[test]
    fn test_single_unwrap() {
        let result = count_unwraps("let x = foo().unwrap();");
        assert_eq!(result.count, 1);
        assert_eq!(result.lines, vec![1]);
    }

    #[test]
    fn test_multiple_unwraps() {
        let source = "a.unwrap();\nb.unwrap();\nc.unwrap();";
        let result = count_unwraps(source);
        assert_eq!(result.count, 3);
        assert_eq!(result.lines, vec![1, 2, 3]);
    }

    #[test]
    fn test_unwrap_in_comment_still_counted() {
        // We count syntactically — comment detection is out of scope
        let source = "// foo.unwrap()\nbar.unwrap();";
        let result = count_unwraps(source);
        assert_eq!(result.count, 2);
    }

    #[test]
    fn test_risk_score_scales() {
        // 10 unwraps in 10 lines = high risk
        let source = (0..10)
            .map(|i| format!("x{i}.unwrap();"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = count_unwraps(&source);
        assert!(
            result.risk_score > 0.5,
            "risk should be high: {}",
            result.risk_score
        );
    }

    #[test]
    fn test_empty_source() {
        let result = count_unwraps("");
        assert_eq!(result.count, 0);
        assert_eq!(result.risk_score, 0.0);
    }

    #[test]
    fn test_count_expects() {
        let source = r#"foo.expect("reason"); bar.expect("msg");"#;
        assert_eq!(count_expects(source), 2);
    }

    #[test]
    fn test_unwrap_or_not_counted() {
        let source = "foo.unwrap_or(default)";
        let result = count_unwraps(source);
        assert_eq!(result.count, 0, "unwrap_or should not be counted as unwrap");
    }

    // ── count_prod_hazards: region-aware, prod-only (D06 faithful) ────────────

    #[test]
    fn test_prod_hazards_counts_real_unwrap() {
        let src = "fn run() { let _x = foo().unwrap(); }";
        let h = count_prod_hazards(src, "rust");
        assert_eq!(h.unwraps, 1);
        assert_eq!(h.unwrap_lines, vec![1]);
    }

    #[test]
    fn test_prod_hazards_excludes_comment_unwrap() {
        // Contrast with `test_unwrap_in_comment_still_counted`: the raw scanner
        // counts the commented `.unwrap()`; the prod scanner must NOT.
        let src = "// foo.unwrap()\nfn run() { let _x = bar().unwrap(); }";
        assert_eq!(
            count_unwraps(src).count,
            2,
            "raw scanner counts the comment"
        );
        let h = count_prod_hazards(src, "rust");
        assert_eq!(h.unwraps, 1, "prod scanner drops the commented unwrap");
        assert_eq!(h.unwrap_lines, vec![2]);
    }

    #[test]
    fn test_prod_hazards_excludes_cfg_test_region() {
        let src = "fn prod() { let _x = foo().unwrap(); }\n\
                   #[cfg(test)]\n\
                   mod tests { fn t() { let _y = bar().unwrap(); } }\n";
        let h = count_prod_hazards(src, "rust");
        assert_eq!(
            h.unwraps, 1,
            "the #[cfg(test)] unwrap must not be a production hazard"
        );
    }

    #[test]
    fn test_prod_hazards_divergence_from_raw_scanner() {
        // 1 production + 1 comment + 1 test unwrap. The raw scanner sees all 3;
        // the prod scanner sees only the production one — the effectiveness proof.
        let src = "fn prod() { let _x = foo().unwrap(); }\n\
                   // let _c = commented.unwrap();\n\
                   #[cfg(test)]\n\
                   mod tests { fn t() { let _y = bar().unwrap(); } }\n";
        assert_eq!(count_unwraps(src).count, 3, "raw counts prod+comment+test");
        let h = count_prod_hazards(src, "rust");
        assert_eq!(h.unwraps, 1, "prod-only excludes comment and test");
    }

    #[test]
    fn test_prod_hazards_counts_panic_and_expect() {
        let src = "fn run() { let _x = foo().expect(\"ctx\"); panic!(\"boom\"); }";
        let h = count_prod_hazards(src, "rust");
        assert_eq!(h.expects, 1);
        assert_eq!(h.panics, 1);
        assert_eq!(h.unwraps, 0);
        assert_eq!(h.total(), 2, "expect(1) + panic(1) = 2");
    }

    #[test]
    fn test_prod_hazards_panic_in_comment_excluded() {
        let src = "fn run() { let _x = 1; }\n// panic!(\"documented, not real\")\n";
        let h = count_prod_hazards(src, "rust");
        assert_eq!(h.panics, 0, "commented panic! is not a production hazard");
    }

    #[test]
    fn test_prod_hazards_empty_source() {
        let h = count_prod_hazards("", "rust");
        assert_eq!(h.total(), 0);
        assert_eq!(h.total_lines, 1, "max(1) floor for density denominator");
    }
}
