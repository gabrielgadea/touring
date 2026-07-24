//! F3.4 — Edge Cases verifier (D30).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_edge_cases`] — a polyglot detector of
//! the canonical "no property-based / fuzz coverage" smell:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | no-proptest | file with multiple `#[test]` but no `proptest!` / `prop_assert` | Rust |
//! | no-fuzz-target | file under `fuzz/` with no `fuzz_target!` call | Rust |
//! | quickcheck-untested | no `#[quickcheck]` macro | Rust |
//! | no-hypothesis | file with multiple `def test_` but no `@hypothesis.given` | Python |
//! | no-fastcheck | file with multiple `it(`/`test(` but no `fc.assert` / `fc.property` | JS/TS |
//!
//! Disjoint from F3.5 test maintainability (which keys on `#[ignore]`
//! accumulation — F3.4 keys on **type** of test framework); F3.6 security
//! tests (which keys on negative-path / DAST — F3.4 keys on property-based
//! / fuzz coverage of the happy/sad paths); F3.7 perf tests (Criterion / k6
//! — F3.4 keys on `cargo-fuzz` + proptest).
//!
//! **Sources (context7, `/proptest-rs/proptest`, High reputation, bench 91.73;
//! `/rust-fuzz/cargo-fuzz`, High reputation, bench 57.4)**: proptest is the
//! canonical property-based framework; per the proptest book: "Property
//! testing complements traditional unit testing by searching for complex
//! inputs that might cause problems, whereas unit tests focus on specific,
//! manually chosen edge cases and known bug-revealing inputs" (both needed).
//! `fuzz_target!(|data: &[u8]| { ... })` is the cargo-fuzz template.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `proptest!` / `fuzz_target!` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F3.4 verifier — Edge Cases.
#[allow(non_camel_case_types)]
pub struct F3_4_EdgeCases;

impl Verification for F3_4_EdgeCases {
    fn id(&self) -> DimId {
        DimId::F3_4
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_edge_cases_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: edge-case coverage ─────────────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_edge_cases_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_edge_cases, score_edge_cases};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.4: detector own source (edge_cases needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let path_str = target.to_string_lossy().to_string();
    let r = analyze_edge_cases(&raw, lang, &path_str);
    let value = score_edge_cases(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F3.4: {} edge-case gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_edge_cases: no-proptest / no-fuzz-target / quickcheck-untested / \
         no-hypothesis / no-fastcheck){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the edge-case needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_8_memory::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: proptest-density heuristic ──────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_edge_cases_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let proptest = raw.matches("proptest!").count() + raw.matches("prop_assert").count();
    let fuzz = raw.matches("fuzz_target!").count();
    let unit_tests = raw.matches("#[test]").count().max(1);
    let property_ratio = (proptest + fuzz) as f32 / unit_tests as f32;
    let value = property_ratio.min(1.0);
    let evidence = format!(
        "{} proptest/fuzz macro(s) / {} #[test] over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full edge-case analysis)",
        proptest + fuzz,
        unit_tests
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
    fn test_edge_cases_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F3_4_EdgeCases.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_edge_cases_empty_file() {
        let f = write_temp("");
        let s = F3_4_EdgeCases.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Proptest presence scores higher than example-only tests.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_proptest_scores_higher_than_examples_only() {
        let bad = write_temp_ext(
            "#[test] fn a() {}\n#[test] fn b() {}\n#[test] fn c() {}\n#[test] fn d() {}\n#[test] fn e() {}\n",
            ".rs",
        );
        let good = write_temp_ext(
            "use proptest::prelude::*;\nproptest! { #[test] fn x(a in 0u32..1) {} }\n#[test] fn y() {}\n",
            ".rs",
        );
        let sb = F3_4_EdgeCases.check(bad.path()).expect("check");
        let sg = F3_4_EdgeCases.check(good.path()).expect("check");
        assert!(
            sg.value > sb.value,
            "proptest ({}) must score above examples-only ({})",
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
            "/x/touring-analysis/src/quality/edge_cases.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
