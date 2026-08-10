//! Test quality analysis (D28 / F3.2) — polyglot detector of the canonical
//! "value-blind assertion / no-assert test" smell. Coverage (F3.1) measures
//! *what executed*; test quality measures *what was actually verified* — a test
//! that runs but only does `assert!(x.is_ok())` is mutation-blind (the gold
//! standard is `cargo mutants`: every surviving mutante is a gap).
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `no-assert-test` | `#[test]` / `it(` / `test(` / `def test_` present but ZERO `assert_eq!` / `assert_ne!` / `assert!` / `expect()` / `assert ` | all |
//! | `trivial-assert` | `assert!(true)` / `assert!(false)` — constant cond, asserts nothing | Rust |
//! | `weak-assert-only` | file has `#[test]` ≥ 1 but ZERO `assert_eq!` / `assert_ne!` (only `assert!`) — value-blind | Rust |
//! | `no-expect-matcher` | JS/TS test uses ONLY `.toBeTruthy()` / `.toBeFalsy()` without `.toBe(` / `.toEqual(` (value-blind) | JS/TS |
//! | `no-pytest-assertion` | Python `def test_` runs without `assert` (silent pass) | Python |
//!
//! **Disjoint** from F3.1 coverage (F3.1 measures lines executed — F3.2 measures
//! what was actually verified); F3.4 edge cases (F3.4 keys on property-based
//! coverage — F3.2 keys on whether existing tests assert behavior at all);
//! F3.5 maint (F3.5 keys on `#[ignore]` + sleep flakiness — F3.2 keys on the
//! *quality* of the assertion itself).
//!
//! **Sources (context7, `/sourcefrog/cargo-mutants`, High reputation, bench 80)**:
//! cargo-mutants mutates operators/retornos; a test that returns
//! `assert!(x.is_ok())` instead of `assert_eq!(x?, expected)` lets mutantes
//! survive. The heuristics below approximate the gold-standard signal without
//! shelling out to cargo-mutants: a test with ZERO asserts or asserts
//! without value comparison is a "would-not-detect-bug" smell.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::{count_executable_including_test_bodies, density_score};

/// Density→score scale (ADVISORY-tier).
const SCALE: f32 = 6.0;

/// Rust strong-assertion needles.
const ASSERT_EQ: &[u8] = b"assert_eq!";
const ASSERT_NE: &[u8] = b"assert_ne!";
/// Rust weak-assertion needle (we count occurrences and subtract strong).
const ASSERT_BANG: &[u8] = b"assert!";
/// Rust trivial-assertion constants (theater — `assert!(true)` / `assert!(false)`).
const ASSERT_TRUE: &[u8] = b"assert!(true)";
const ASSERT_FALSE: &[u8] = b"assert!(false)";

/// JS/TS strong matchers (value-comparing).
const TO_BE: &[u8] = b".toBe(";
const TO_EQUAL: &[u8] = b".toEqual(";
/// JS/TS weak matchers (boolean-only).
const TO_BE_TRUTHY: &[u8] = b".toBeTruthy()";
const TO_BE_FALSY: &[u8] = b".toBeFalsy()";

/// Rust test attribute.
const RUST_TEST_ATTR: &[u8] = b"#[test]";
/// JS/TS test invocation patterns.
const JS_TEST_IT: &[u8] = b"it(";
const JS_TEST_TEST: &[u8] = b"test(";
/// Python test function.
const PYTHON_TEST_DEF: &[u8] = b"def test_";

/// Findings of a single test-quality analysis pass: the canonical
/// "value-blind assertion / no-assert test" smell rolled up per file.
pub type TestQualityReport = crate::quality::SmellReport;

/// Count occurrences of `needle` in `bytes` outside non-executable regions.
fn count_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

/// Line-walk count of `#[test]` attributes, skipping lines that start with
/// `//` (line comments). `#[test]` itself is a non-executable region marker
/// in `code_regions` — a region-filtered count returns 0 — so this hand-rolled
/// line-walk is required (lesson from F3.4 edge_cases).
fn count_rust_tests(bytes: &[u8]) -> usize {
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
        if !trimmed.starts_with(b"//") && memmem::find(line, RUST_TEST_ATTR).is_some() {
            count += 1;
        }
        line_start = line_end + 1;
    }
    count
}

/// Rust test-quality: assert presence + trivial-assert theatre + weak-only.
///
/// Uses line-walk counting (not `non_executable_regions` filter) for asserts
/// because `#[test]` fn bodies are marked non-executable — we WANT to count
/// the asserts inside test fn bodies (the whole point of F3.2).
fn analyze_rust(bytes: &[u8], _regions: &[(usize, usize)]) -> (usize, usize, usize, usize) {
    let tests = count_rust_tests(bytes);
    let strong = count_executable_including_test_bodies(bytes, ASSERT_EQ)
        + count_executable_including_test_bodies(bytes, ASSERT_NE);
    let weak_total = count_executable_including_test_bodies(bytes, ASSERT_BANG);
    let weak = weak_total.saturating_sub(strong);
    let trivial = count_executable_including_test_bodies(bytes, ASSERT_TRUE)
        + count_executable_including_test_bodies(bytes, ASSERT_FALSE);
    (tests, strong, weak, trivial)
}

/// JS/TS test-quality: presence of strong matchers + weak-only.
fn analyze_js_ts(bytes: &[u8], regions: &[(usize, usize)]) -> (usize, usize, usize) {
    let tests = count_executable(bytes, regions, JS_TEST_IT)
        + count_executable(bytes, regions, JS_TEST_TEST);
    let strong =
        count_executable(bytes, regions, TO_BE) + count_executable(bytes, regions, TO_EQUAL);
    let weak = count_executable(bytes, regions, TO_BE_TRUTHY)
        + count_executable(bytes, regions, TO_BE_FALSY);
    (tests, strong, weak)
}

/// Python test-quality: `def test_` count + `assert` count.
fn analyze_python(bytes: &[u8], regions: &[(usize, usize)]) -> (usize, usize) {
    let tests = count_executable(bytes, regions, PYTHON_TEST_DEF);
    let asserts = count_executable(bytes, regions, b"assert ");
    (tests, asserts)
}

/// Emit Rust-branch findings.
fn emit_rust_findings(
    report: &mut TestQualityReport,
    tests: usize,
    strong: usize,
    weak: usize,
    trivial: usize,
) {
    if tests >= 1 && strong == 0 && weak == 0 {
        report.push(
            "#[test] function(s) without any assert! / assert_eq! macro \
             (silent-pass test — bug-injected code would still pass)",
            1,
            0.9,
        );
        return;
    }
    if tests >= 1 && strong == 0 {
        report.push(
            "no assert_eq! / assert_ne! in a file with #[test] \
             (assertions are value-blind — cargo mutants would survive here)",
            1,
            0.7,
        );
    }
    if trivial > 0 {
        report.push(
            "trivial assertion assert!(true|false) — \
             asserts nothing, mutante survives trivially",
            trivial,
            0.8,
        );
    }
}

/// Emit JS/TS-branch findings.
fn emit_js_ts_findings(report: &mut TestQualityReport, tests: usize, strong: usize, weak: usize) {
    if tests >= 1 && strong == 0 && weak == 0 {
        report.push(
            "JS/TS test() / it() blocks without .toBe()/.toEqual()/.toBeTruthy() \
             (silent-pass test — assertion-free regression guard)",
            1,
            0.9,
        );
        return;
    }
    if tests >= 1 && strong == 0 && weak > 0 {
        report.push(
            "JS/TS tests use only .toBeTruthy()/.toBeFalsy() without \
             .toBe()/.toEqual() (value-blind matchers — regression guard is loose)",
            1,
            0.7,
        );
    }
}

/// Emit Python-branch findings.
fn emit_python_findings(report: &mut TestQualityReport, tests: usize, asserts: usize) {
    if tests >= 1 && asserts == 0 {
        report.push(
            "def test_* function(s) without `assert` statement \
             (silent-pass test — pytest would report PASS for any return)",
            1,
            0.9,
        );
    }
}

/// Analyze test-quality in `source` for the given language.
pub fn analyze_test_quality(source: &str, lang: &str) -> TestQualityReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = TestQualityReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        "rust" | "rs" => {
            let (tests, strong, weak, trivial) = analyze_rust(bytes, &regions);
            emit_rust_findings(&mut report, tests, strong, weak, trivial);
        }
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => {
            let (tests, strong, weak) = analyze_js_ts(bytes, &regions);
            emit_js_ts_findings(&mut report, tests, strong, weak);
        }
        "python" | "py" => {
            let (tests, asserts) = analyze_python(bytes, &regions);
            emit_python_findings(&mut report, tests, asserts);
        }
        _ => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`TestQualityReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
pub fn score_test_quality(report: &TestQualityReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_with_assert_eq_clean() {
        let src = r#"
#[test]
fn add_two() {
    assert_eq!(add(1, 2), 3);
}
#[test]
fn add_three() {
    assert_ne!(add(1, 2), 4);
}
"#;
        let r = analyze_test_quality(src, "rust");
        assert_eq!(r.violations, 0, "strong asserts is clean: {:?}", r.findings);
    }

    #[test]
    fn rust_with_only_assert_flagged_weak() {
        let src = r#"
#[test]
fn no_value_check() {
    let r = add(1, 2);
    assert!(r.is_ok());
}
#[test]
fn another() {
    let r = add(1, 2);
    assert!(r.is_ok());
}
"#;
        let r = analyze_test_quality(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("value-blind")),
            "weak-only file is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_trivial_assert_flagged() {
        let src = r#"
#[test]
fn t() { assert!(true); }
#[test]
fn u() { assert!(false); }
"#;
        let r = analyze_test_quality(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("trivial assertion")),
            "trivial assert!(true|false) is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_no_assert_at_all_flagged() {
        let src = r#"
#[test]
fn silent_pass() {
    let _ = add(1, 2);
}
"#;
        let r = analyze_test_quality(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("without any assert!")),
            "test with no asserts is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_no_tests_clean() {
        let src = "fn prod() { 1 + 2 }";
        let r = analyze_test_quality(src, "rust");
        assert_eq!(
            r.violations, 0,
            "no tests → no quality smell: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_with_to_equal_clean() {
        let src = r#"
test('add', () => {
    expect(add(1, 2)).toBe(3);
});
test('eq', () => {
    expect(add(1, 2)).toEqual(3);
});
"#;
        let r = analyze_test_quality(src, "javascript");
        assert_eq!(r.violations, 0, "toBe/toEqual is clean: {:?}", r.findings);
    }

    #[test]
    fn js_with_only_truthy_flagged() {
        let src = r#"
test('a', () => {
    expect(add(1, 2)).toBeTruthy();
});
test('b', () => {
    expect(add(1, 2)).toBeTruthy();
});
"#;
        let r = analyze_test_quality(src, "javascript");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("toBeTruthy")),
            "truthy-only is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_no_assertion_at_all_flagged() {
        let src = r#"
test('silent', () => {
    add(1, 2);
});
"#;
        let r = analyze_test_quality(src, "javascript");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("silent-pass")),
            "assertion-free test is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_with_assert_clean() {
        let src = r#"
def test_add():
    assert add(1, 2) == 3

def test_sub():
    assert sub(5, 2) == 3
"#;
        let r = analyze_test_quality(src, "python");
        assert_eq!(r.violations, 0, "with assert is clean: {:?}", r.findings);
    }

    #[test]
    fn python_no_assert_flagged() {
        let src = r#"
def test_silent():
    add(1, 2)

def test_also_silent():
    sub(5, 2)
"#;
        let r = analyze_test_quality(src, "python");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("without `assert`")),
            "Python test without assert is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn comment_excluded() {
        let src = r#"
// assert_eq!(1, 2);
#[test]
fn t() {
    // assert_eq!(...) inside comment
    assert_eq!(1, 1);
}
"#;
        let r = analyze_test_quality(src, "rust");
        assert_eq!(r.violations, 0, "comment excluded: {:?}", r.findings);
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_test_quality(
            r#"
#[test]
fn silent_a() { let _ = foo(); }
#[test]
fn silent_b() { let _ = bar(); }
#[test]
fn silent_c() { let _ = baz(); }
#[test]
fn silent_d() { let _ = qux(); }
"#,
            "rust",
        );
        let good = analyze_test_quality(
            r#"
#[test]
fn ok_a() { assert_eq!(1, 1); }
#[test]
fn ok_b() { assert_eq!(2, 2); }
#[test]
fn ok_c() { assert_eq!(3, 3); }
#[test]
fn ok_d() { assert_eq!(4, 4); }
"#,
            "rust",
        );
        assert!(
            score_test_quality(&bad) < score_test_quality(&good),
            "no-assert file ({:.3}) must score below assert_eq-rich file ({:.3})",
            score_test_quality(&bad),
            score_test_quality(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_test_quality(
            r#"#[test]
fn silent() { let _ = foo(); }
#[test]
fn silent2() { let _ = bar(); }
#[test]
fn silent3() { let _ = baz(); }
#[test]
fn silent4() { let _ = qux(); }
"#,
            "rust",
        );
        let s = score_test_quality(&r);
        assert!(
            s > 0.0,
            "short file with silent tests must not score 0.0: {s}"
        );
    }
}
