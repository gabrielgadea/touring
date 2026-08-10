//! F3.3 — Test Pyramid verifier (D29).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_test_pyramid`] — a polyglot detector
//! of the canonical "ice-cream cone / pyramid-inverted" smell:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `large-suite-no-e2e` | ≥5 `#[test]` in a Rust file with no Playwright/Cypress/Selenium reference (missing E2E pyramid top) | Rust |
//! | `e2e-heavy-no-unit` | a JS/TS file with multiple browser actions (`page.click`/`page.locator`/`page.goto`/`page.fill`) but no `it(`/`test(` (heavy E2E, no unit base) | JS/TS |
//! | `e2e-no-pytest` | Python file references Selenium/Playwright but no `def test_*` (E2E-only test file) | Python |
//! | `e2e-no-assertion` | JS/TS file with multiple browser actions but no `.toBeVisible` / `.toHaveText` (E2E test is a no-op) | JS/TS |
//!
//! Disjoint from F3.1 coverage (F3.1 measures what executed; F3.3 keys on the
//! *layer shape* — unit vs integration vs E2E), F3.4 edge cases (F3.4 keys on
//! property-based coverage; F3.3 keys on the layer mix), and F3.5 test maint
//! (F3.5 keys on flakiness; F3.3 keys on shape).
//!
//! **Sources (context7, `/microsoft/playwright`, High reputation, bench 90;
//! Cypress)**: Playwright/Cypress E2E tests are slow and flaky; best practice
//! is to keep them as a narrow top of the pyramid, with broad unit/integration
//! base. Auto-wait (`expect(locator).toBeVisible()`) reduces flakiness, but
//! the cost is still 10-100× unit tests.
//!
//! **Standalone fallback (`--no-default-features`)**: a `#[cfg(test)]` vs
//! `e2e`/`playwright` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F3.3 verifier — Test Pyramid.
#[allow(non_camel_case_types)]
pub struct F3_3_TestPyramid;

impl Verification for F3_3_TestPyramid {
    fn id(&self) -> DimId {
        DimId::F3_3
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_test_pyramid_dim(target)
    }
}

// ── Real engine: test-pyramid detector ───────────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_test_pyramid_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_test_pyramid, score_test_pyramid};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.3: detector own source (test_pyramid needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_test_pyramid(&raw, lang);
    let value = score_test_pyramid(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F3.3: {} pyramid-shape gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_test_pyramid: large-suite-no-e2e / \
         e2e-heavy-no-unit / e2e-no-pytest / e2e-no-assertion){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the test-pyramid needle vocabulary as
/// detection data, so scoring their own source is a self-match false positive.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: unit-vs-e2e density heuristic ───────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_test_pyramid_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let units = raw.matches("#[cfg(test)]").count()
        + raw.matches("def test_").count()
        + raw.matches("it(").count()
        + raw.matches("test(").count();
    let e2e = raw.matches("e2e").count()
        + raw.matches("playwright").count()
        + raw.matches("cypress").count()
        + raw.matches("page.click").count();
    let value = if units >= e2e { 1.0 } else { 0.5 };
    let evidence = format!(
        "{units} unit markers / {e2e} e2e markers over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full pyramid analysis)"
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
    fn test_test_pyramid_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F3_3_TestPyramid.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_test_pyramid_empty_file() {
        let f = write_temp("");
        let s = F3_3_TestPyramid.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Large test suite without E2E (Playwright) framework scores below
    /// suite WITH playwright reference (pyramid top present).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_pyramid_with_e2e_scores_higher() {
        let bad = write_temp_ext(
            "#[test] fn a() {}\n#[test] fn b() {}\n#[test] fn c() {}\n\
             #[test] fn d() {}\n#[test] fn e() {}\n#[test] fn f() {}\n",
            ".rs",
        );
        let good = write_temp_ext(
            "use playwright;\n#[test] fn a() {}\n#[test] fn b() {}\n#[test] fn c() {}\n\
             #[test] fn d() {}\n#[test] fn e() {}\n",
            ".rs",
        );
        let sb = F3_3_TestPyramid.check(bad.path()).expect("check");
        let sg = F3_3_TestPyramid.check(good.path()).expect("check");
        assert!(
            sg.value > sb.value,
            "with-e2e file ({}) must score above without-e2e file ({})",
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
            "/x/touring-analysis/src/quality/test_pyramid.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
