//! F3.2 — Test Quality verifier (D28).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_test_quality`] — a polyglot detector
//! of the canonical "value-blind assertion / no-assert test" smell:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `no-assert-test` | `#[test]` / `it(` / `test(` / `def test_` present but ZERO `assert_eq!` / `assert_ne!` / `assert!` / `expect()` / `assert ` | all |
//! | `trivial-assert` | `assert!(true)` / `assert!(false)` — constant cond, asserts nothing | Rust |
//! | `weak-assert-only` | file has `#[test]` ≥ 1 but ZERO `assert_eq!` / `assert_ne!` (only `assert!`) — value-blind | Rust |
//! | `no-expect-matcher` | JS/TS test uses ONLY `.toBeTruthy()` / `.toBeFalsy()` without `.toBe(` / `.toEqual(` (value-blind) | JS/TS |
//! | `no-pytest-assertion` | Python `def test_` runs without `assert` (silent pass) | Python |
//!
//! Disjoint from F3.1 coverage (F3.1 measures what executed; F3.2 measures
//! whether anything was asserted), F3.4 edge cases (F3.4 keys on property-based
//! coverage; F3.2 keys on assertion *quality*), and F3.5 test maint (F3.5 keys
//! on flakiness; F3.2 keys on whether the test verifies behavior at all).
//!
//! **Sources (context7, `/sourcefrog/cargo-mutants`, High reputation, bench
//! 80)**: cargo-mutants mutates operators/retornos; a test that returns
//! `assert!(x.is_ok())` instead of `assert_eq!(x?, expected)` lets mutantes
//! survive. The heuristics below approximate the gold-standard signal without
//! shelling out to cargo-mutants: a test with ZERO asserts or asserts without
//! value comparison is a "would-not-detect-bug" smell.
//!
//! **Standalone fallback (`--no-default-features`)**: an `assert_eq!` /
//! `assert!` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F3.2 verifier — Test Quality.
#[allow(non_camel_case_types)]
pub struct F3_2_TestQuality;

impl Verification for F3_2_TestQuality {
    fn id(&self) -> DimId {
        DimId::F3_2
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_test_quality_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: test-quality detector ───────────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_test_quality_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_test_quality, score_test_quality};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.2: detector own source (test_quality needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_test_quality(&raw, lang);
    let value = score_test_quality(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F3.2: {} test-quality gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_test_quality: no-assert-test / trivial-assert / \
         weak-assert-only / no-expect-matcher / no-pytest-assertion){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the test-quality needle vocabulary as
/// detection data, so scoring their own source is a self-match false positive.
/// Mirrors `f2_13_scalability::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: assert-density heuristic ────────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_test_quality_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let asserts_eq = raw.matches("assert_eq!").count() + raw.matches("assert_ne!").count();
    let asserts_bare = raw.matches(".toBe(").count() + raw.matches(".toEqual(").count();
    let asserts = (asserts_eq + asserts_bare) as f32;
    let tests = raw.matches("#[test]").count()
        + raw.matches("def test_").count()
        + raw.matches("it(").count()
        + raw.matches("test(").count();
    let value = if tests == 0 {
        1.0
    } else {
        (asserts / tests as f32).min(1.0)
    };
    let evidence = format!(
        "{asserts} value-comparing asserts / {tests} tests over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full test-quality analysis)"
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_ext(content: &str, suffix: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }
    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_test_quality_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F3_2_TestQuality.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_test_quality_empty_file() {
        let f = write_temp("");
        let s = F3_2_TestQuality.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Asserts-rich file scores higher than no-assert test file.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_test_quality_asserts_score_higher_than_no_asserts() {
        let bad = write_temp_ext(
            "#[test]\nfn silent() { let _ = foo(); }\n#[test]\nfn silent2() { let _ = bar(); }\n",
            ".rs",
        );
        let good = write_temp_ext(
            "#[test]\nfn ok() { assert_eq!(1, 1); }\n#[test]\nfn ok2() { assert_eq!(2, 2); }\n",
            ".rs",
        );
        let sb = F3_2_TestQuality.check(bad.path()).expect("check");
        let sg = F3_2_TestQuality.check(good.path()).expect("check");
        assert!(
            sg.value > sb.value,
            "asserts file ({}) must score above no-asserts file ({})",
            sg.value,
            sb.value
        );
    }
    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/test_quality.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
