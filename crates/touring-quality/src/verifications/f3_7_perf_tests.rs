//! F3.7 — Performance Tests verifier (D33).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_perf_tests`] — a polyglot detector
//! of the canonical "no benchmark / regression-guard" smell:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `no-criterion-bench` | file with multiple `#[test]` but no `criterion_group!` / `criterion_main!` / `bench_function` / `#[bench]` | Rust |
//! | `no-pytest-benchmark` | file with multiple `def test_` but no `@pytest.mark.benchmark` | Python |
//! | `no-fastcheck-perf` | file with multiple `it(` / `test(` but no `perf(` / `performance(` / `bench(` test naming | JS/TS |
//!
//! Disjoint from F3.4 edge_cases (which keys on **type** of test
//! framework — proptest / fast-check / hypothesis — F3.7 keys on
//! **performance** testing frameworks: Criterion, pytest-benchmark);
//! F3.5 test maintainability (which keys on `#[ignore]` accumulation —
//! F3.7 keys on `bench_*` / `perf_*` naming); F3.6 security tests
//! (negative authz — F3.7 is positive, performance benchmarks).
//!
//! **Sources (context7, `/bheisler/criterion.rs`, High reputation, bench 94.42;
//! `/websites/pytest-benchmark_readthedocs_io_en_stable`, High reputation,
//! bench 62)**: Criterion's `criterion_group!` + `criterion_main!` macros
//! define the benchmark harness; `black_box` prevents compiler optimization
//! of benchmarked work. pytest-benchmark's `--benchmark-compare-fail=min:5%`
//! is the standard CI gate for performance regressions.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `criterion_group!` / `@pytest.mark.benchmark` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F3.7 verifier — Performance Tests.
#[allow(non_camel_case_types)]
pub struct F3_7_PerfTests;

impl Verification for F3_7_PerfTests {
    fn id(&self) -> DimId {
        DimId::F3_7
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_perf_tests_dim(target)
    }
}

// ── Real engine: performance-test coverage ──────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_perf_tests_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_perf_tests, score_perf_tests};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.7: detector own source (perf_tests needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_perf_tests(&raw, lang);
    let value = score_perf_tests(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F3.7: {} perf-test gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_perf_tests: no-criterion-bench / no-pytest-benchmark / \
         no-fastcheck-perf){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the perf-test needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_8_memory::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: criterion-density heuristic ────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_perf_tests_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let criterion = raw.matches("criterion_group!").count() + raw.matches("bench_function").count();
    let value = (criterion as f32 / 1.0).min(1.0);
    let evidence = format!(
        "{criterion} criterion macro(s) over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full perf-test analysis)"
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
    fn test_perf_tests_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F3_7_PerfTests.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_perf_tests_empty_file() {
        let f = write_temp("");
        let s = F3_7_PerfTests.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Criterion presence scores higher than example-only tests.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_criterion_scores_higher_than_examples_only() {
        let bad = write_temp_ext(
            "#[test] fn a() {}\n#[test] fn b() {}\n#[test] fn c() {}\n#[test] fn d() {}\n#[test] fn e() {}\n",
            ".rs",
        );
        let good = write_temp_ext(
            "use criterion::{criterion_group, criterion_main, Criterion};\n\
             fn bench_x(c: &mut Criterion) { c.bench_function(\"x\", |b| b.iter(|| 1)); }\n\
             criterion_group!(benches, bench_x);\ncriterion_main!(benches);\n",
            ".rs",
        );
        let sb = F3_7_PerfTests.check(bad.path()).expect("check");
        let sg = F3_7_PerfTests.check(good.path()).expect("check");
        assert!(
            sg.value > sb.value,
            "criterion ({}) must score above examples-only ({})",
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
            "/x/touring-analysis/src/quality/perf_tests.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
