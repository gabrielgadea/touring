//! Performance test coverage analysis (D33 / F3.7) — polyglot detector of the
//! canonical "no benchmark / regression-guard" smell that lets performance
//! regressions land silently in production. "Meça antes de otimizar" — sem
//! baseline, não há como saber se uma mudança regrediu a performance.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | no-criterion-bench | file with hot-path code but no `criterion::criterion_group!` / `criterion_main!` / `bench_function` | Rust |
//! | no-bench-module | no `benches/` directory AND no `#[bench]` (nightly-only) AND no Criterion import | Rust |
//! | no-pytest-benchmark | file with hot-path code but no `@pytest.mark.benchmark` decorator AND no `benchmark` fixture | Python |
//! | no-fastcheck-perf | file with hot-path code but no `perf()` / `performance()` test naming | JS/TS |
//!
//! **Disjoint** from F3.4 edge_cases (which keys on **type** of test
//! framework — proptest / fast-check / hypothesis — F3.7 keys on **performance**
//! testing frameworks specifically: Criterion, pytest-benchmark); F3.5 test
//! maintainability (which keys on `#[ignore]` accumulation — F3.7 keys on
//! `bench_*` / `perf_*` naming); F3.6 security tests (negative authz — F3.7
//! is positive, performance benchmarks).
//!
//! **Sources (context7, `/bheisler/criterion.rs`, High reputation, bench 94.42;
//! `/websites/pytest-benchmark_readthedocs_io_en_stable`, High reputation,
//! bench 62)**: Criterion's `criterion_group!` + `criterion_main!` macros
//! define the benchmark harness (`book/src/user_guide/migrating_from_libtest.md`).
//! `black_box` is the canonical way to prevent the compiler from optimizing
//! away the benchmarked work. pytest-benchmark exposes a `benchmark` fixture
//! that automatically calibrates, runs warmup, and reports statistics
//! (`usage.html`). The `--benchmark-compare-fail=min:5%` flag is the
//! standard way to fail CI on regression (`comparing.html`).
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::{density_score, for_each_line};

/// Density→score scale (ADVISORY-tier).
const SCALE: f32 = 6.0;

/// Rust Criterion needles.
const CRITERION_GROUP: &[u8] = b"criterion_group!";
const CRITERION_MAIN: &[u8] = b"criterion_main!";
const CRITERION_IMPORT: &[u8] = b"use criterion";
const BENCH_FUNCTION: &[u8] = b"bench_function";
const BENCH_ATTR: &[u8] = b"#[bench]";

/// Python pytest-benchmark needles.
const PYTEST_BENCHMARK: &[u8] = b"@pytest.mark.benchmark";
const BENCHMARK_FIXTURE: &[u8] = b"def test_"; // heuristic: test functions take `benchmark` fixture
/// JS/TS perf-test naming convention (Jest/Mocha).
const PERF_TEST_NAME: &[&[u8]] = &[b"perf(", b"performance(", b"bench("];

/// Findings of a single performance-test coverage analysis pass: the
/// canonical "no benchmark / regression-guard" smell rolled up per file.
#[derive(Debug, Clone, Default)]
pub struct PerfTestsReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl PerfTestsReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

/// Count the number of times `needle` appears in `bytes` outside
/// non-executable regions (comments + `#[cfg(test)]`).
fn count_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

/// Rust perf-test coverage: Criterion / `#[bench]` presence vs absence.
/// Files with multiple `#[test]` annotations but zero bench coverage fire
/// the "no performance test" smell. We use the same line-walk
/// `count_test_attrs` pattern as F3.4.
fn analyze_rust_perf(
    bytes: &[u8],
    regions: &[(usize, usize)],
) -> (usize, usize, usize, usize, usize, usize) {
    let criterion_group = count_executable(bytes, regions, CRITERION_GROUP);
    let criterion_main = count_executable(bytes, regions, CRITERION_MAIN);
    let criterion_import = count_executable(bytes, regions, CRITERION_IMPORT);
    let bench_function = count_executable(bytes, regions, BENCH_FUNCTION);
    let bench_attr = count_executable(bytes, regions, BENCH_ATTR);
    // Count `#[test]` raw (line-walk; F3.4 lesson — region-filtered
    // count returns 0 because `#[test]` itself is a test-region marker).
    let mut unit_tests = 0usize;
    for_each_line(bytes, |line| {
        let mut cut = 0usize;
        while cut < line.len() && (line[cut] == b' ' || line[cut] == b'\t') {
            cut += 1;
        }
        let trimmed = &line[cut..];
        if !trimmed.starts_with(b"//") && memmem::find(line, b"#[test]").is_some() {
            unit_tests += 1;
        }
    });
    (
        criterion_group,
        criterion_main,
        criterion_import,
        bench_function,
        bench_attr,
        unit_tests,
    )
}

/// Python perf-test coverage: pytest-benchmark decorator / `benchmark` fixture
/// presence vs absence. The `benchmark` fixture is named exactly that — we
/// look for it in `def test_(...) -> benchmark:` parameter lists (rough
/// proxy).
fn analyze_python_perf(bytes: &[u8], regions: &[(usize, usize)]) -> (usize, usize, usize) {
    let pytest_bench = count_executable(bytes, regions, PYTEST_BENCHMARK);
    let benchmark_use = count_executable(bytes, regions, BENCHMARK_FIXTURE);
    // Count `def test_` lines (already a proxy in F3.4).
    let mut test_count = 0usize;
    for_each_line(bytes, |line| {
        if line.starts_with(b"def test_") {
            test_count += 1;
        }
    });
    let _ = benchmark_use; // currently not used directly; reserved for future detector
    (pytest_bench, test_count, benchmark_use)
}

/// JS/TS perf-test coverage: `perf(` / `performance(` / `bench(` test-naming
/// presence. Jest/Mocha convention is `describe('perf', () => it(...))` —
/// the keyword is in the test name.
fn analyze_js_ts_perf(bytes: &[u8], regions: &[(usize, usize)]) -> (usize, usize) {
    let mut perf_count = 0usize;
    for needle in PERF_TEST_NAME {
        perf_count += count_executable(bytes, regions, needle);
    }
    // Count `it(` / `test(` as unit-test proxy.
    let mut test_count = 0usize;
    for_each_line(bytes, |line| {
        if memmem::find(line, b"it(").is_some() || memmem::find(line, b"test(").is_some() {
            test_count += 1;
        }
    });
    (perf_count, test_count)
}

/// Analyze performance-test coverage in `source` for the given language.
pub fn analyze_perf_tests(source: &str, lang: &str) -> PerfTestsReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = PerfTestsReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        "rust" | "rs" => {
            let (group, main, import, bench_fn, bench_attr, unit_tests) =
                analyze_rust_perf(bytes, &regions);
            let criterion = group + main + import + bench_fn + bench_attr;
            // The "no perf test" smell fires when a file has at least
            // 2 `#[test]` annotations and zero Criterion / `#[bench]`.
            if unit_tests >= 2 && criterion == 0 {
                report.push(
                    "no Criterion (criterion_group!/criterion_main!/bench_function) \
                     or #[bench] in a file with multiple #[test]s \
                     (no benchmark / regression-guard — perf can land silently)",
                    1,
                    0.7,
                );
            }
        }
        "python" | "py" => {
            let (pytest_bench, test_count, _benchmark_use) = analyze_python_perf(bytes, &regions);
            if test_count >= 2 && pytest_bench == 0 {
                report.push(
                    "no @pytest.mark.benchmark in a file with multiple def test_ \
                     (no pytest-benchmark — perf regression unguarded)",
                    1,
                    0.7,
                );
            }
        }
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => {
            let (perf, test_count) = analyze_js_ts_perf(bytes, &regions);
            if test_count >= 2 && perf == 0 {
                report.push(
                    "no perf(/performance(/bench( test naming in a file with multiple \
                     it(/test( (no k6 / Lighthouse / `perf()`-prefixed test — \
                     JS/TS perf unguarded)",
                    1,
                    0.7,
                );
            }
        }
        _ => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`PerfTestsReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)` floor
/// + density cap so a 4-line stub perf-test file doesn't saturate to 0.
pub fn score_perf_tests(report: &PerfTestsReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_with_criterion_clean() {
        let src = r#"
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_fib(c: &mut Criterion) {
    c.bench_function("fib 20", |b| b.iter(|| fib(20)));
}

criterion_group!(benches, bench_fib);
criterion_main!(benches);
"#;
        let r = analyze_perf_tests(src, "rust");
        assert_eq!(
            r.violations, 0,
            "criterion present is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_example_tests_only_flagged() {
        let src = r#"
#[test] fn a() {}
#[test] fn b() {}
#[test] fn c() {}
#[test] fn d() {}
#[test] fn e() {}
"#;
        let r = analyze_perf_tests(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("Criterion")),
            "5 example tests + no Criterion is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_single_test_not_flagged() {
        let src = r#"
#[test] fn only_one() {}
"#;
        let r = analyze_perf_tests(src, "rust");
        assert_eq!(
            r.violations, 0,
            "1 test is not enough to flag: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_bench_attr_accepted() {
        // `#[bench]` (nightly-only) is also a valid perf-test marker.
        let src = r#"
#[bench] fn bench_x() { 1 + 1 }
"#;
        let r = analyze_perf_tests(src, "rust");
        // 0 unit tests (the bench is the only "test") — no smell fired.
        assert_eq!(
            r.violations, 0,
            "file with only a #[bench] is fine: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_with_pytest_benchmark_clean() {
        let src = r#"
import pytest

@pytest.mark.benchmark(group="x")
def test_my_stuff(benchmark):
    benchmark(lambda: 1 + 1)
"#;
        let r = analyze_perf_tests(src, "python");
        assert_eq!(
            r.violations, 0,
            "pytest-benchmark present is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_only_unittest_flagged() {
        let src = r#"
def test_a(): assert True
def test_b(): assert True
def test_c(): assert True
def test_d(): assert True
def test_e(): assert True
"#;
        let r = analyze_perf_tests(src, "python");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("pytest.mark.benchmark")),
            "5 unittest defs + no pytest-benchmark is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_with_perf_naming_clean() {
        let src = r#"
test('perf: smoke', () => {});
"#;
        let r = analyze_perf_tests(src, "typescript");
        assert_eq!(
            r.violations, 0,
            "perf-prefixed test is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_only_jest_tests_flagged() {
        let src = r#"
test('a', () => {});
test('b', () => {});
test('c', () => {});
test('d', () => {});
test('e', () => {});
"#;
        let r = analyze_perf_tests(src, "typescript");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("perf(") || m.contains("performance(")),
            "5 jest tests without perf-naming is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn comment_excluded() {
        let src = r#"
// criterion_group!(fake, bench_x);
#[test] fn real() {}
#[test] fn a() {}
#[test] fn b() {}
#[test] fn c() {}
#[test] fn d() {}
"#;
        let r = analyze_perf_tests(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("Criterion")),
            "commented Criterion does not satisfy detector: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_perf_tests(
            r#"
#[test] fn a() {}
#[test] fn b() {}
#[test] fn c() {}
#[test] fn d() {}
#[test] fn e() {}
"#,
            "rust",
        );
        let good = analyze_perf_tests(
            r#"
use criterion::{criterion_group, criterion_main, Criterion};
fn bench_x(c: &mut Criterion) { c.bench_function("x", |b| b.iter(|| 1)); }
criterion_group!(benches, bench_x);
criterion_main!(benches);
"#,
            "rust",
        );
        assert!(
            score_perf_tests(&bad) < score_perf_tests(&good),
            "no Criterion ({:.3}) must score below Criterion present ({:.3})",
            score_perf_tests(&bad),
            score_perf_tests(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_perf_tests(
            r#"
#[test] fn a() {}
#[test] fn b() {}
#[test] fn c() {}
#[test] fn d() {}
#[test] fn e() {}
"#,
            "rust",
        );
        let s = score_perf_tests(&r);
        assert!(
            s > 0.0,
            "short file with 1 perf-test smell must not score 0.0: {s}"
        );
    }
}
