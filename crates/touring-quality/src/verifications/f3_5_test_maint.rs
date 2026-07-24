//! F3.5 — Test Maintainability verifier (D31).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_test_maint`] — a polyglot detector
//! of the canonical "flaky / ignored / stateful / non-isolated test" smell:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `ignored-test` | `#[ignore]` attribute (gap hidden — each is a hidden cost) | Rust |
//! | `sleep-in-test` | `.sleep(` / `tokio::time::sleep` / `time.sleep` / `setTimeout` in test body (flaky) | all |
//! | `nondeterministic-in-test` | `now()` / `SystemTime::now` / `thread_rng` / `Math.random` / `random.*` in test body without deterministic seed injection | all |
//! | `state-sharing-in-test` | `lazy_static!` / `OnceCell` / `static mut` in test (state leaks across tests, breaks parallel exec) | Rust |
//! | `real-network-in-test` | HTTP/DB I/O in test without `mockall` / `wiremock` / `testcontainers` / `mockito` (real network/DB → slow + flaky + side-effect) | all |
//!
//! Disjoint from F3.1 coverage (F3.1 measures what executed; F3.5 keys on the
//! *hygiene* of how it ran), F3.2 test quality (F3.2 keys on *whether anything
//! was asserted*; F3.5 keys on *whether the test is stable*), and F3.3 pyramid
//! (F3.3 keys on the layer shape; F3.5 keys on flakiness within a layer).
//!
//! **Sources (context7, `/testcontainers/testcontainers-rs`, High reputation;
//! WireMock)**: testcontainers-rs is the gold standard for hermetic, isolated
//! test dependencies (DB/svc in disposable container per test). WireMock mocks
//! the HTTP boundary with verified contract. A `#[ignore]` test is a hidden
//! gap — debt that must be paid, not silenced.
//!
//! **Standalone fallback (`--no-default-features`)**: a `sleep` + `Mock`
//! density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F3.5 verifier — Test Maintainability.
#[allow(non_camel_case_types)]
pub struct F3_5_TestMaint;

impl Verification for F3_5_TestMaint {
    fn id(&self) -> DimId {
        DimId::F3_5
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_test_maint_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: test-maintainability detector ──────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_test_maint_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_test_maint, score_test_maint};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.5: detector own source (test_maint needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_test_maint(&raw, lang);
    let value = score_test_maint(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F3.5: {} test-maintainability gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_test_maint: ignored-test / sleep-in-test / \
         nondeterministic-in-test / state-sharing-in-test / real-network-in-test){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the test-maint needle vocabulary as
/// detection data, so scoring their own source is a self-match false positive.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: sleep/mock density heuristic ────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_test_maint_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let sleeps = raw.matches(".sleep(").count() + raw.matches("tokio::time::sleep").count();
    let mocks = raw.matches("Mock").count() + raw.matches("mockall").count();
    let tests = raw.matches("#[test]").count().max(1);
    let smell = (sleeps as f32 + mocks as f32 * 0.3) / tests as f32;
    let value = 1.0 - smell.min(1.0);
    let evidence = format!(
        "{sleeps} sleep / {mocks} mock smell(s) over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full test-maint analysis)"
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
    fn test_test_maint_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F3_5_TestMaint.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_test_maint_empty_file() {
        let f = write_temp("");
        let s = F3_5_TestMaint.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// `#[ignore]`-laden test file scores below clean test file.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_ignored_tests_score_lower_than_clean() {
        let bad = write_temp_ext(
            "#[test] #[ignore] fn a() {}\n\
             #[test] #[ignore] fn b() {}\n\
             #[test] #[ignore] fn c() {}\n\
             #[test] #[ignore] fn d() {}\n",
            ".rs",
        );
        let good = write_temp_ext(
            "#[test] fn a() { assert_eq!(1, 1); }\n\
             #[test] fn b() { assert_eq!(2, 2); }\n\
             #[test] fn c() { assert_eq!(3, 3); }\n\
             #[test] fn d() { assert_eq!(4, 4); }\n",
            ".rs",
        );
        let sb = F3_5_TestMaint.check(bad.path()).expect("check");
        let sg = F3_5_TestMaint.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "ignored file ({}) must score below clean ({})",
            sb.value,
            sg.value
        );
    }
    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/test_maint.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
