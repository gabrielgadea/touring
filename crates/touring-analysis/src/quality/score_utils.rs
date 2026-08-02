//! Shared scoring primitives for the quality engines (D-rules 1-52).
//!
//! The 50-dim quality engine layer used to copy the same density formula
//! `1 - density·SCALE` across every per-file `score_X` function, with
//! `density = weighted_total / total_lines.max(1)`. That formula saturates
//! to 0 on short files (4-10 LOC) with multiple findings — empirically
//! surfaced in the F2.13 wave (a 4-line test file with 3 weighted findings
//! had `density = 0.675 → 1 - 0.675·6 = -3.05 → clamp(0)` = indistinguishable
//! from a clean file with 10 findings). The same anti-pattern lived in 12
//! other engines (`api_design`, `caching`, `concurrency`, `data_model`,
//! `db_perf`, `design_patterns`, `frontend`, `idioms`, `input_validation`,
//! `io`, `memory`, `modernization`).
//!
//! [`density_score`] applies a `max(20)` floor on the line count so the
//! formula has a sensible baseline: no 4-line test file is "so small that
//! 3 smells are acceptable density" — the smell is the smell, regardless
//! of LOC. The floor is a heuristic (not a tunable parameter) because the
//! intent is uniform across all 50-dim per-file density scores: 20 LOC
//! is the minimum size at which a file is large enough for density to be
//! meaningful in the first place.
//!
//! Comments / `#[cfg(test)]` regions are irrelevant to this module (it is
//! pure arithmetic; the `weighted_total` / `total_lines` inputs are
//! already region-aware upstream).

/// Density-to-score conversion: `1 - density·scale`, clamped to `[0, 1]`.
///
/// Two safeguards against score saturation on short files with multiple
/// findings:
///
/// 1. **`max(20)` floor on the line count** so a 4-line test file with 3
///    findings has a sensible baseline (no file is "so small that 3 smells
///    are acceptable density" — the smell is the smell, regardless of LOC).
/// 2. **Density cap at `1/scale · 0.99`** so the score never lands exactly
///    on 0 when there are findings (preserves visibility for regression
///    tests on short files and avoids the case where many findings on a
///    4-line fixture → 1 - 0.30·6 = -0.80 → clamp 0, indistinguishable
///    from a clean file with 10 findings).
///
/// SCALE is caller-provided (different engines have different
/// sensibilities: e.g. `idioms`/`input_validation` use 8.0 because they
/// detect more findings per file in idiomatic Rust, vs the default 6.0).
#[inline]
pub fn density_score(weighted_total: f32, total_lines: usize, scale: f32) -> f32 {
    let lines = total_lines.max(20) as f32;
    let raw_density = weighted_total / lines;
    // Cap density at 1/scale · 0.99 so score = 1 - density·scale > 0.01
    // whenever there are findings (preserves visibility for the regression
    // tests in `caching/concurrency/frontend/io` short-file tests).
    let cap = 1.0 / scale * 0.99;
    let density = raw_density.min(cap);
    (1.0 - density * scale).clamp(0.0, 1.0)
}

/// Line-walk count of `needle` occurrences in `bytes`, **skipping regular
/// `//` line comments only**.
///
/// The `super::code_regions` pipeline marks `#[test]` fn bodies and `///`
/// doc comments as "non-executable" — appropriate for *security* analysis
/// (avoids flagging `// SQL injection` as a vuln, avoids flagging SQL inside
/// `#[cfg(test)]` corpora) but the *wrong filter* for **test-quality,
/// test-maintainability, and API-documentation** engines, which WANT to see
/// what's inside test bodies and doc comments (asserts in `#[test]` fn
/// bodies, `# Examples` in `///` doc comments, etc.).
///
/// This helper line-walks each line, only excluding lines whose first
/// non-whitespace token is `//` (a regular line comment). It explicitly
/// **includes**:
///
/// - `///` doc-comment lines (we count `# Examples` / `# Panics` sections).
/// - `#[test]` fn bodies (we count asserts inside test fn bodies).
/// - All other code.
///
/// Block comments (`/* … */`) are not specially handled — rare in test
/// code, and the cost/benefit of multi-line brace matching is high for
/// negligible precision gain in these dimensions.
#[inline]
pub fn count_executable_including_test_bodies(bytes: &[u8], needle: &[u8]) -> usize {
    use memchr::memmem;
    let mut count = 0usize;
    let mut line_start = 0usize;
    while line_start < bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| line_start + p)
            .unwrap_or(bytes.len());
        let line = &bytes[line_start..line_end];
        let trimmed_start = line
            .iter()
            .position(|&b| b != b' ' && b != b'\t')
            .unwrap_or(line.len());
        let trimmed = &line[trimmed_start..];
        let is_doc_comment = trimmed.starts_with(b"///");
        let is_regular_comment = trimmed.starts_with(b"//") && !is_doc_comment;
        if !is_regular_comment && memmem::find(line, needle).is_some() {
            count += 1;
        }
        line_start = line_end + 1;
    }
    count
}

/// Walk every line of `bytes`, calling `f` with each line slice (newline excluded).
///
/// This is the shared line-walk scaffold used by multiple quality analyzers
/// (`edge_cases`, `perf_tests`, …) that need a raw byte-level line iterator.
/// All per-line logic (trimming, needle matching, comment skipping) is left to
/// the closure — only the `while`/`position`/`map`/`unwrap_or` scaffold is here.
///
/// **Borrow note**: `f` takes `FnMut` so it can close over a `&mut usize`
/// accumulator without violating the borrow checker.
#[inline]
pub(super) fn for_each_line(bytes: &[u8], mut f: impl FnMut(&[u8])) {
    let mut line_start = 0usize;
    while line_start < bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| line_start + p)
            .unwrap_or(bytes.len());
        f(&bytes[line_start..line_end]);
        line_start = line_end + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Empty report → perfect score.
    #[test]
    fn empty_clean() {
        assert_eq!(density_score(0.0, 0, 6.0), 1.0);
        assert_eq!(density_score(0.0, 100, 6.0), 1.0);
    }
    /// A short file (5 LOC) with 3 weighted findings: density = 3/20 = 0.15
    /// (floored), score = 1 - 0.15·6 = 0.10 — *not* saturated to 0.
    /// This is the canonical F2.13 fix regression test.
    #[test]
    fn short_file_with_findings_does_not_saturate() {
        let score = density_score(3.0, 5, 6.0);
        assert!(
            score > 0.0,
            "3 weighted findings in 5 lines must not score 0.0: {score}"
        );
        assert!((score - 0.1).abs() < 1e-6, "expected 0.1, got {score}");
    }
    /// A large file (500 LOC) with the same 3 findings: density = 3/500 = 0.006,
    /// score = 1 - 0.006·6 = 0.964 — proportionally less penalized.
    #[test]
    fn large_file_with_same_findings_scores_higher() {
        let short = density_score(3.0, 5, 6.0);
        let large = density_score(3.0, 500, 6.0);
        assert!(
            large > short,
            "large file should score higher than short file for the same findings"
        );
    }
    /// `total_lines = 0` (empty file): floors to 20, score is computed as if
    /// the file had 20 lines — empty file with findings is not "perfectly
    /// clean by virtue of being empty".
    #[test]
    fn zero_lines_floored() {
        let score = density_score(2.0, 0, 6.0);
        assert!((score - 0.4).abs() < 1e-6, "expected 0.4, got {score}");
    }
    /// SCALE = 8.0 (idioms / input_validation): same 3 findings, stricter.
    #[test]
    fn scale_8_stricter_than_scale_6() {
        let s6 = density_score(3.0, 100, 6.0);
        let s8 = density_score(3.0, 100, 8.0);
        assert!(
            s8 < s6,
            "scale 8 should be stricter than scale 6 for the same density"
        );
    }
    /// Extreme: 100 weighted findings in 5 lines → density is capped at
    /// `1/scale · 0.99` so score saturates to ≈ 0.01 (not exactly 0) —
    /// **the visibility-guard floor** that preserves distinction between
    /// "many findings" and "no findings" on short regression-test fixtures.
    /// Earlier (pre-visibility-guard) the score saturated to exactly 0.0
    /// and the regression tests couldn't distinguish `0 findings` from
    /// `N findings` on short stubs (the F2.13 short-file bug).
    #[test]
    fn extreme_density_caps_near_zero() {
        let score = density_score(100.0, 5, 6.0);
        assert!(
            score > 0.0,
            "visibility-guard: extreme density must NOT exactly clamp to 0: {score}"
        );
        assert!(
            score < 0.05,
            "extreme density should be near 0 (visibility-guard ceiling): {score}"
        );
    }
    /// Monotonicity: more findings → lower score (for same line count + scale).
    #[test]
    fn monotonic_in_findings() {
        let s1 = density_score(1.0, 100, 6.0);
        let s2 = density_score(2.0, 100, 6.0);
        let s3 = density_score(3.0, 100, 6.0);
        assert!(
            s1 > s2 && s2 > s3,
            "scores must decrease with more findings: {s1} > {s2} > {s3}"
        );
    }
    /// Monotonicity: more lines → higher score (for same findings + scale).
    #[test]
    fn monotonic_in_lines() {
        let s_short = density_score(3.0, 20, 6.0);
        let s_long = density_score(3.0, 200, 6.0);
        assert!(
            s_long > s_short,
            "longer file should score higher for same findings: {s_long} > {s_short}"
        );
    }
}
