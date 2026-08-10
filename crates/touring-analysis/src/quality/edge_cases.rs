//! Edge-case coverage analysis (D30 / F3.4) — polyglot detector of the
//! canonical "no property-based / fuzz coverage" smell that leaves the
//! codebase with only example-based unit tests (the kind of suite that
//! "passes for the inputs the author thought of" — bugs live in the corners
//! the author didn't imagine, per the proptest book: "the kind of bugs that
//! only happen when nobody is looking").
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | no-property-tests | many `#[test]` but **zero** `proptest!` / `proptest::test_runner` calls | Rust |
//! | no-fuzz-targets | file in `fuzz/` directory but **zero** `fuzz_target!` calls | Rust |
//! | quickcheck-untested | no `#[quickcheck]` macro | Rust |
//! | no-hypothesis-tests | many `def test_` but **zero** `@hypothesis.given` decorators | Python |
//! | no-fastcheck-tests | many `it(` but **zero** `fc.assert` / `fc.property` calls | JS/TS |
//!
//! **Disjoint** from F3.5 test maintainability (which keys on `#[ignore]`
//! accumulation and flakiness — F3.4 keys on **type** of test framework);
//! F3.6 security tests (which keys on negative-path / DAST — F3.4 keys on
//! property-based / fuzz coverage of the happy/sad paths); F3.7 perf tests
//! (which keys on Criterion / k6 — F3.4 keys on `cargo-fuzz` + proptest).
//!
//! **Sources (context7, `/proptest-rs/proptest`, High reputation, bench 91.73;
//! `/rust-fuzz/cargo-fuzz`, High reputation, bench 57.4)**:
//! `proptest!` is the canonical property-based testing framework
//! (`book/src/proptest/tutorial/macro-proptest.md`): "Property testing
//! complements traditional unit testing by searching for complex inputs that
//! might cause problems, whereas unit tests focus on specific, manually chosen
//! edge cases and known bug-revealing inputs" (`intro.html`). The proptest
//! book (`limitations.html`) explicitly states: "Property testing explores a
//! randomly sampled portion of the input space, making it extremely unlikely
//! to find single-value edge cases in large spaces. Therefore, traditional
//! unit testing with intelligently selected cases remains necessary for many
//! types of problems" — i.e. **both** property AND example tests are needed.
//! `fuzz_target!(|data: &[u8]| { ... })` is the cargo-fuzz template
//! (`src/templates.rs`); the `#![no_main]` attribute and `fuzz_target!`
//! macro call are the only structural markers per file.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::{density_score, for_each_line};

/// Density→score scale (ADVISORY-tier; quick to compute).
const SCALE: f32 = 6.0;

/// Rust property-testing needles (the `proptest!` / `#[quickcheck]` /
/// `prop_assert*` family + the `fuzz_target!` macro).
const PROPTEST_MACRO: &[u8] = b"proptest!";
const PROP_ASSERT: &[u8] = b"prop_assert";
const QUICKCHECK_ATTR: &[u8] = b"#[quickcheck]";
const FUZZ_TARGET_MACRO: &[u8] = b"fuzz_target!";
const TEST_ATTR: &[u8] = b"#[test]";

/// Python `hypothesis` needles.
const HYPOTHESIS_GIVEN: &[u8] = b"@hypothesis.given";
const HYPOTHESIS_STRATEGIES: &[u8] = b"hypothesis.strategies";

/// JS/TS `fast-check` needles.
const FASTCHECK_ASSERT: &[u8] = b"fc.assert";
const FASTCHECK_PROPERTY: &[u8] = b"fc.property";

/// Findings of a single edge-case coverage analysis pass: the canonical
/// "no property-based / fuzz coverage" smell rolled up per file.
pub type EdgeCasesReport = crate::quality::SmellReport;

/// Count the number of times `needle` appears in `bytes` outside
/// non-executable regions (comments + `#[cfg(test)]`).
fn count_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

/// `true` if `path` looks like a cargo-fuzz target file (lives in a `fuzz/`
/// directory). The fuzz/ directory is the cargo-fuzz convention.
fn is_fuzz_target_path(path: &str) -> bool {
    path.contains("/fuzz/")
        || path.contains("/fuzz\\")
        || path.ends_with("/fuzz")
        || path.starts_with("fuzz/")
}

/// Count `#[test]` attributes that appear on real (non-comment) lines.
/// `non_executable_regions` treats `#[test]` lines as test regions (which
/// they are), so a region-filtered count returns 0 — useless for our
/// "how many tests does this file have" question. We instead walk lines
/// and skip those starting with `//` (so a `// #[test]` in a code comment
/// is correctly not counted).
fn count_test_attrs(bytes: &[u8]) -> usize {
    let mut count = 0usize;
    for_each_line(bytes, |line| {
        let mut cut = 0usize;
        while cut < line.len() && (line[cut] == b' ' || line[cut] == b'\t') {
            cut += 1;
        }
        let trimmed = &line[cut..];
        // Skip line comments (`//` regular, `///` doc, `//!` inner doc).
        if !trimmed.starts_with(b"//") && memmem::find(line, TEST_ATTR).is_some() {
            count += 1;
        }
    });
    count
}

/// Rust edge-case coverage: proptest / quickcheck / fuzz_target presence vs
/// `#[test]` count. File-level finding — fires when a file with multiple
/// `#[test]` annotations has zero property-based / fuzz coverage.
fn analyze_rust(
    bytes: &[u8],
    regions: &[(usize, usize)],
    path: &str,
) -> (usize, usize, usize, bool) {
    let proptest = count_executable(bytes, regions, PROPTEST_MACRO);
    let prop_assert = count_executable(bytes, regions, PROP_ASSERT);
    let quickcheck = count_executable(bytes, regions, QUICKCHECK_ATTR);
    let fuzz = count_executable(bytes, regions, FUZZ_TARGET_MACRO);
    // `#[test]` is the test-region marker itself, so the region filter
    // excludes it — count it via line-walk (skipping `//` comments).
    let unit_tests = count_test_attrs(bytes);
    let is_fuzz = is_fuzz_target_path(path);
    (
        proptest + prop_assert + quickcheck,
        fuzz,
        unit_tests,
        is_fuzz,
    )
}

/// Python edge-case coverage: `@hypothesis.given` decorator presence.
fn analyze_python(bytes: &[u8], regions: &[(usize, usize)]) -> (usize, usize) {
    let given = count_executable(bytes, regions, HYPOTHESIS_GIVEN);
    let strategies = count_executable(bytes, regions, HYPOTHESIS_STRATEGIES);
    // Count `def test_` (the unittest-style) as a unit-test proxy.
    let mut unit_tests = 0usize;
    for_each_line(bytes, |line| {
        if line.starts_with(b"def test_") {
            unit_tests += 1;
        }
    });
    let _ = strategies; // reserved for future detector
    (given, unit_tests)
}

/// JS/TS edge-case coverage: `fast-check` import + `fc.assert` / `fc.property`.
fn analyze_js_ts(bytes: &[u8], regions: &[(usize, usize)]) -> (usize, usize) {
    let fc_assert = count_executable(bytes, regions, FASTCHECK_ASSERT);
    let fc_property = count_executable(bytes, regions, FASTCHECK_PROPERTY);
    // Count `it(` and `test(` as a unit-test proxy.
    let mut unit_tests = 0usize;
    for_each_line(bytes, |line| {
        if memmem::find(line, b"it(").is_some() || memmem::find(line, b"test(").is_some() {
            unit_tests += 1;
        }
    });
    (fc_assert + fc_property, unit_tests)
}

/// Analyze edge-case coverage in `source` for the given language. The
/// `path` parameter is used to detect `fuzz/` target files (cargo-fuzz
/// convention); pass an empty string if path is unknown.
pub fn analyze_edge_cases(source: &str, lang: &str, path: &str) -> EdgeCasesReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = EdgeCasesReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        "rust" | "rs" => {
            let (property, fuzz, unit_tests, is_fuzz) = analyze_rust(bytes, &regions, path);
            // The "no property tests" smell fires when a file has at least
            // 2 `#[test]` annotations and zero proptest/quickcheck usage.
            // One test isn't a "missing edge case" signal.
            if unit_tests >= 2 && property == 0 {
                report.push(
                    "no proptest!/quickcheck! coverage in a file with multiple #[test]s \
                     (example-based tests alone miss randomly-sampled corners)",
                    1,
                    0.7,
                );
            }
            if is_fuzz && fuzz == 0 {
                report.push(
                    "file lives under fuzz/ but defines no fuzz_target! \
                     (cargo-fuzz convention: every fuzz/ file should drive libFuzzer)",
                    1,
                    0.8,
                );
            }
        }
        "python" | "py" => {
            let (given, unit_tests) = analyze_python(bytes, &regions);
            if unit_tests >= 2 && given == 0 {
                report.push(
                    "no @hypothesis.given / strategies coverage in a file with multiple \
                     def test_ (Python property-based gap)",
                    1,
                    0.7,
                );
            }
        }
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => {
            let (property, unit_tests) = analyze_js_ts(bytes, &regions);
            if unit_tests >= 2 && property == 0 {
                report.push(
                    "no fast-check (fc.assert / fc.property) coverage in a file with \
                     multiple it(/test( (JS/TS property-based gap)",
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

/// Score an [`EdgeCasesReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)` floor
/// and density cap, so a 4-line test file with multiple findings still
/// produces a non-zero score instead of clamping to 0.
pub fn score_edge_cases(report: &EdgeCasesReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_with_proptest_clean() {
        let src = r#"
use proptest::prelude::*;
proptest! {
    #[test]
    fn test_roundtrip(a in 0u32..1000) {
        assert!(a < 1000);
    }
}
#[test]
fn example() { assert_eq!(1, 1); }
"#;
        let r = analyze_edge_cases(src, "rust", "src/lib.rs");
        assert_eq!(
            r.violations, 0,
            "proptest presence is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_only_example_tests_flagged() {
        let src = r#"
#[test] fn a() { assert!(true); }
#[test] fn b() { assert!(true); }
#[test] fn c() { assert!(true); }
#[test] fn d() { assert!(true); }
#[test] fn e() { assert!(true); }
"#;
        let r = analyze_edge_cases(src, "rust", "src/lib.rs");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("no proptest")),
            "5 example tests + no proptest is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_single_test_not_flagged() {
        let src = r#"
#[test] fn only_one() { assert!(true); }
"#;
        let r = analyze_edge_cases(src, "rust", "src/lib.rs");
        assert_eq!(
            r.violations, 0,
            "1 test is not enough to flag: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_fuzz_target_in_fuzz_dir_clean() {
        let src = r#"
#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _ = data;
});
"#;
        let r = analyze_edge_cases(src, "rust", "fuzz/fuzz_target.rs");
        assert_eq!(
            r.violations, 0,
            "fuzz_target! in fuzz/ is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_fuzz_dir_without_fuzz_target_flagged() {
        let src = r#"
#![no_main]
use libfuzzer_sys::fuzz_target;
// Oops, forgot to call fuzz_target!
fn helper(_data: &[u8]) {}
"#;
        let r = analyze_edge_cases(src, "rust", "fuzz/empty.rs");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("fuzz_target")),
            "fuzz/ file without fuzz_target! is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_with_hypothesis_clean() {
        let src = r#"
import hypothesis
from hypothesis import given, strategies as st

@given(st.integers())
def test_roundtrip(x):
    assert isinstance(x, int)
"#;
        let r = analyze_edge_cases(src, "python", "tests/test_x.py");
        assert_eq!(r.violations, 0, "hypothesis is clean: {:?}", r.findings);
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
        let r = analyze_edge_cases(src, "python", "tests/test_x.py");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("hypothesis")),
            "5 unittest defs + no hypothesis is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_with_fastcheck_clean() {
        let src = r#"
import { fc, test } from 'fast-check';
test('roundtrip', () => {
  fc.assert(fc.property(fc.integer(), (n) => n + 0 === n));
});
"#;
        let r = analyze_edge_cases(src, "typescript", "test/x.test.ts");
        assert_eq!(r.violations, 0, "fast-check is clean: {:?}", r.findings);
    }

    #[test]
    fn js_only_jest_flagged() {
        let src = r#"
test('a', () => {});
test('b', () => {});
test('c', () => {});
test('d', () => {});
test('e', () => {});
"#;
        let r = analyze_edge_cases(src, "typescript", "test/x.test.ts");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("fast-check")),
            "5 jest tests + no fast-check is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn cfg_test_excluded() {
        let src = r#"
// #[test] fn documented() {}
// proptest! { #[test] fn x(_a in 0u32..1) {} }
#[test] fn real() { assert!(true); }
"#;
        let r = analyze_edge_cases(src, "rust", "src/lib.rs");
        assert_eq!(
            r.violations, 0,
            "commented proptest + 1 real test is not flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn comment_excluded() {
        // proptest inside a `//` comment should not be counted.
        let src = r#"
// proptest! { #[test] fn x() {} }
#[test] fn a() {}
#[test] fn b() {}
#[test] fn c() {}
#[test] fn d() {}
#[test] fn e() {}
"#;
        let r = analyze_edge_cases(src, "rust", "src/lib.rs");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("no proptest")),
            "commented proptest does not satisfy detector: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_edge_cases(
            r#"
#[test] fn a() {}
#[test] fn b() {}
#[test] fn c() {}
#[test] fn d() {}
#[test] fn e() {}
"#,
            "rust",
            "src/lib.rs",
        );
        let good = analyze_edge_cases(
            r#"
use proptest::prelude::*;
proptest! { #[test] fn x(a in 0u32..1) {} }
#[test] fn y() {}
"#,
            "rust",
            "src/lib.rs",
        );
        assert!(
            score_edge_cases(&bad) < score_edge_cases(&good),
            "no proptest ({:.3}) must score below proptest present ({:.3})",
            score_edge_cases(&bad),
            score_edge_cases(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_edge_cases(
            r#"
#[test] fn a() {}
#[test] fn b() {}
#[test] fn c() {}
#[test] fn d() {}
#[test] fn e() {}
"#,
            "rust",
            "src/lib.rs",
        );
        let s = score_edge_cases(&r);
        assert!(
            s > 0.0,
            "short file with 1 edge-case smell must not score 0.0: {s}"
        );
    }
}
