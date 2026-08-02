//! Test maintainability analysis (D31 / F3.5) — polyglot detector of the canonical
//! "flaky / ignored / stateful / non-isolated test" smell. A flaky test destroys
//! the team's trust in the suite; a `#[ignore]`-accumulating suite hides gaps;
//! a state-sharing test breaks under parallel execution. The dimension scores
//! the *engineering hygiene* of the test corpus.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `ignored-test` | `#[ignore]` attribute (gap hidden — each is a hidden cost) | Rust |
//! | `sleep-in-test` | `.sleep(` / `tokio::time::sleep` / `time.sleep` in test body (flaky) | all |
//! | `no-clean-mock` | `Mock` / `mockall` / `wiremock` / `mockito` reference indicates clean mocking; absence in a file with many I/O calls is a state-coupling smell | all |
//! | `lazy-static-in-test` | `lazy_static!` / `once_cell` / `static` in `#[cfg(test)]` (state sharing) | Rust |
//! | `now-rand-in-test` | `now()` / `rand::thread_rng` in test body without deterministic seed injection | all |
//!
//! **Disjoint** from F3.1 coverage (F3.1 measures what executed; F3.5 keys on
//! the *hygiene* of how it ran); F3.2 test quality (F3.2 keys on *whether
//! anything was asserted*; F3.5 keys on *whether the test is stable*);
//! F3.3 pyramid (F3.3 keys on the layer shape; F3.5 keys on flakiness within
//! a layer).
//!
//! **Sources (context7, `/testcontainers/testcontainers-rs`, High reputation;
//! WireMock; VP-Scout Cadeia 3b `#[ignore]` verification)**: testcontainers-rs
//! is the gold standard for hermetic, isolated test dependencies (DB/svc in
//! disposable container per test). WireMock mocks the HTTP boundary with
//! verified contract. A `#[ignore]` test is a hidden gap — it is debt that
//! must be paid, not silenced.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::{count_executable_including_test_bodies, density_score};

/// Density→score scale (ADVISORY-tier).
const SCALE: f32 = 6.0;

/// `#[ignore]` attribute — Rust tests marked as ignored (hidden gap).
const RUST_IGNORE: &[u8] = b"#[ignore]";
/// Rust test attribute (used to detect files where ignore is in production code by mistake).
const RUST_TEST: &[u8] = b"#[test]";

/// Sleep in test (flaky).
const SLEEP: &[u8] = b".sleep(";
const TOKIO_SLEEP: &[u8] = b"tokio::time::sleep";
const PY_SLEEP: &[u8] = b"time.sleep";
const JS_SLEEP: &[u8] = b"setTimeout";

/// Clean mocking frameworks.
const MOCKALL: &[u8] = b"mockall";
const WIREMOCK: &[u8] = b"wiremock";
const MOCKITO: &[u8] = b"mockito";
const TESTCONTAINERS: &[u8] = b"testcontainers";
const PY_MOCKER: &[u8] = b"unittest.mock";

/// State-sharing patterns in Rust (anti-pattern when in `#[cfg(test)]`).
const LAZY_STATIC: &[u8] = b"lazy_static!";
const ONCE_CELL: &[u8] = b"OnceCell";
const STATIC_MUT: &[u8] = b"static mut";
const MOCK: &[u8] = b"Mock";

/// Non-deterministic time / random in test.
const NOW_CALL: &[u8] = b"now()";
const INSTANT_NOW: &[u8] = b"std::time::Instant::now";
const SYSTEM_TIME: &[u8] = b"SystemTime::now";
const THREAD_RNG: &[u8] = b"thread_rng";
const PY_RANDOM: &[u8] = b"random.";
const JS_MATH_RANDOM: &[u8] = b"Math.random";

/// Findings of a single test-maintainability analysis pass.
#[derive(Debug, Clone, Default)]
pub struct TestMaintReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl TestMaintReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

/// Count occurrences of `needle` in `bytes` outside non-executable regions.
fn count_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

/// Line-walk count of `#[test]` (handles region-marker issue).
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
        if !trimmed.starts_with(b"//") && memmem::find(line, RUST_TEST).is_some() {
            count += 1;
        }
        line_start = line_end + 1;
    }
    count
}

/// Per-language sleep count.
///
/// For Rust uses line-walk (sleeps inside `#[test]` fn bodies are precisely
/// what we want to flag — the region-filter would mask them).
fn count_sleeps(bytes: &[u8], regions: &[(usize, usize)], lang: &str) -> usize {
    match lang {
        "rust" | "rs" => {
            count_executable_including_test_bodies(bytes, SLEEP)
                + count_executable_including_test_bodies(bytes, TOKIO_SLEEP)
        }
        "python" | "py" => {
            count_executable(bytes, regions, SLEEP) + count_executable(bytes, regions, PY_SLEEP)
        }
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => {
            count_executable(bytes, regions, SLEEP) + count_executable(bytes, regions, JS_SLEEP)
        }
        _ => count_executable(bytes, regions, SLEEP),
    }
}

/// Non-deterministic time / random in test.
///
/// For Rust uses line-walk (now()/thread_rng inside `#[test]` fn bodies are
/// precisely what we want to flag).
fn count_nondeterministic(bytes: &[u8], regions: &[(usize, usize)], lang: &str) -> usize {
    let base =
        count_executable(bytes, regions, NOW_CALL) + count_executable(bytes, regions, SYSTEM_TIME);
    match lang {
        "rust" | "rs" => {
            count_executable_including_test_bodies(bytes, NOW_CALL)
                + count_executable_including_test_bodies(bytes, SYSTEM_TIME)
                + count_executable_including_test_bodies(bytes, INSTANT_NOW)
                + count_executable_including_test_bodies(bytes, THREAD_RNG)
        }
        "python" | "py" => base + count_executable(bytes, regions, PY_RANDOM),
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => {
            base + count_executable(bytes, regions, JS_MATH_RANDOM)
        }
        _ => base,
    }
}

/// Clean mocking framework count.
fn count_clean_mocks(bytes: &[u8], regions: &[(usize, usize)], lang: &str) -> usize {
    let base = count_executable(bytes, regions, MOCKALL)
        + count_executable(bytes, regions, WIREMOCK)
        + count_executable(bytes, regions, MOCKITO)
        + count_executable(bytes, regions, TESTCONTAINERS);
    match lang {
        "python" | "py" => base + count_executable(bytes, regions, PY_MOCKER),
        _ => base,
    }
}

/// Rust-specific: state-sharing in test (lazy_static / OnceCell / static mut).
///
/// Uses line-walk counting (the `#[test]` body filter would otherwise mask
/// `lazy_static!` / `OnceCell` inside test fn bodies).
fn count_rust_state_sharing(bytes: &[u8]) -> usize {
    count_executable_including_test_bodies(bytes, LAZY_STATIC)
        + count_executable_including_test_bodies(bytes, ONCE_CELL)
        + count_executable_including_test_bodies(bytes, STATIC_MUT)
        + count_executable_including_test_bodies(bytes, MOCK)
}

/// Rust-branch findings.
///
/// Uses line-walk counting (not `non_executable_regions` filter) for things
/// inside `#[test]` fn bodies — those bodies are marked non-executable by
/// `code_regions`, but we WANT to see what's inside (sleep / now() /
/// lazy_static! / #[ignore] — those are precisely the test smells).
fn emit_rust_findings(
    report: &mut TestMaintReport,
    bytes: &[u8],
    _regions: &[(usize, usize)],
    tests: usize,
) {
    let ignored = count_executable_including_test_bodies(bytes, RUST_IGNORE);
    if ignored > 0 {
        report.push(
            "#[ignore] attribute on test(s) — ignored tests are hidden gaps \
             (test is not running, regression guard is silent)",
            ignored,
            0.7,
        );
    }
    if tests >= 1 {
        let sleeps = count_sleeps(bytes, _regions, "rust");
        if sleeps > 0 {
            report.push(
                ".sleep( / tokio::time::sleep inside #[test] body — \
                 flaky under load; inject tokio::time::pause() or a virtual clock",
                sleeps,
                0.8,
            );
        }
        let nondet = count_nondeterministic(bytes, _regions, "rust");
        if nondet > 0 {
            report.push(
                "now() / SystemTime::now() / thread_rng in test without deterministic \
                 seed injection — flaky on different hosts/runs",
                nondet,
                0.6,
            );
        }
        let state = count_rust_state_sharing(bytes);
        if state > 0 {
            report.push(
                "lazy_static! / OnceCell / static mut in test — \
                 state leaks across tests, breaks parallel execution",
                state,
                0.6,
            );
        }
        let mocks = count_clean_mocks(bytes, _regions, "rust");
        let has_reqwest = bytes.windows(7).any(|w| w == b"reqwest");
        let has_sql = bytes.windows(3).any(|w| w == b"sql");
        if mocks == 0 && (has_reqwest || has_sql) {
            report.push(
                "HTTP/DB I/O in test without mockall / wiremock / testcontainers — \
                 real network/DB dependency (slow + flaky + side-effect)",
                1,
                0.5,
            );
        }
    }
}

/// JS/TS-branch findings.
fn emit_js_ts_findings(
    report: &mut TestMaintReport,
    bytes: &[u8],
    regions: &[(usize, usize)],
    tests: usize,
) {
    if tests == 0 {
        return;
    }
    let sleeps = count_sleeps(bytes, regions, "javascript");
    if sleeps > 0 {
        report.push(
            "setTimeout / .sleep( inside test() / it() — flaky; \
             use Playwright's auto-wait expect(locator).toBeVisible()",
            sleeps,
            0.7,
        );
    }
    let nondet = count_nondeterministic(bytes, regions, "javascript");
    if nondet > 0 {
        report.push(
            "Math.random() / Date.now() in test without seed injection — \
             non-deterministic regression guard",
            nondet,
            0.6,
        );
    }
    let mocks = count_clean_mocks(bytes, regions, "javascript");
    if mocks == 0 && memmem::find(bytes, b"fetch(").is_some() {
        report.push(
            "fetch( in test without wiremock / msw (mock service worker) — \
             real-network dependency, slow + flaky",
            1,
            0.5,
        );
    }
}

/// Python-branch findings.
fn emit_python_findings(
    report: &mut TestMaintReport,
    bytes: &[u8],
    regions: &[(usize, usize)],
    tests: usize,
) {
    if tests == 0 {
        return;
    }
    let sleeps = count_sleeps(bytes, regions, "python");
    if sleeps > 0 {
        report.push(
            "time.sleep( inside def test_* — flaky; use freezegun.freeze_time() \
             or unittest.mock.patch on time",
            sleeps,
            0.7,
        );
    }
    let nondet = count_nondeterministic(bytes, regions, "python");
    if nondet > 0 {
        report.push(
            "random.* / now() in def test_* without deterministic seed — \
             flaky under different seeds/clock",
            nondet,
            0.6,
        );
    }
    let mocks = count_clean_mocks(bytes, regions, "python");
    if mocks == 0
        && (bytes.windows(4).any(|w| w == b"http") || bytes.windows(5).any(|w| w == b"urllib"))
    {
        report.push(
            "HTTP client in test without unittest.mock — real network dependency",
            1,
            0.5,
        );
    }
}

/// Analyze test-maintainability in `source` for the given language.
pub fn analyze_test_maint(source: &str, lang: &str) -> TestMaintReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = TestMaintReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        "rust" | "rs" => {
            let tests = count_rust_tests(bytes);
            emit_rust_findings(&mut report, bytes, &regions, tests);
        }
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => {
            let tests = count_executable(bytes, &regions, b"it(")
                + count_executable(bytes, &regions, b"test(");
            emit_js_ts_findings(&mut report, bytes, &regions, tests);
        }
        "python" | "py" => {
            let tests = count_executable(bytes, &regions, b"def test_");
            emit_python_findings(&mut report, bytes, &regions, tests);
        }
        _ => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`TestMaintReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
pub fn score_test_maint(report: &TestMaintReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_with_clean_tests_clean() {
        let src = r#"
#[test]
fn a() { assert_eq!(1, 1); }
#[test]
fn b() { assert_eq!(2, 2); }
#[test]
fn c() { assert_eq!(3, 3); }
#[test]
fn d() { assert_eq!(4, 4); }
"#;
        let r = analyze_test_maint(src, "rust");
        assert_eq!(r.violations, 0, "clean test file: {:?}", r.findings);
    }

    #[test]
    fn rust_ignored_flagged() {
        let src = r#"
#[test] #[ignore] fn a() {}
#[test] #[ignore = "broken"] fn b() {}
"#;
        let r = analyze_test_maint(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("#[ignore]") && m.contains("hidden")),
            "#[ignore] flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_sleep_in_test_flagged() {
        let src = r#"
#[test]
fn a() { std::thread::sleep(std::time::Duration::from_secs(1)); }
#[test]
fn b() { tokio::time::sleep(std::time::Duration::from_millis(10)).await; }
"#;
        let r = analyze_test_maint(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("flaky")),
            "sleep in test flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_nondeterministic_flagged() {
        let src = r#"
#[test]
fn a() { let _ = std::time::SystemTime::now(); }
#[test]
fn b() { let mut rng = rand::thread_rng(); }
"#;
        let r = analyze_test_maint(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("non-deterministic") || m.contains("seed")),
            "non-det flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_state_sharing_flagged() {
        let src = r#"
#[test]
fn a() { lazy_static! { static ref X: i32 = 1; } let _ = *X; }
"#;
        let r = analyze_test_maint(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("state leaks")),
            "state sharing flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_no_tests_clean() {
        let src = "fn prod() { 1 + 2 }";
        let r = analyze_test_maint(src, "rust");
        assert_eq!(r.violations, 0, "no tests: {:?}", r.findings);
    }

    #[test]
    fn js_with_settimeout_flagged() {
        let src = r#"
test('a', async () => {
    await new Promise((r) => setTimeout(r, 1000));
});
"#;
        let r = analyze_test_maint(src, "javascript");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("flaky")),
            "setTimeout flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_math_random_flagged() {
        let src = r#"
test('a', () => {
    expect(Math.random()).toBe(0.5);
});
"#;
        let r = analyze_test_maint(src, "javascript");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("Math.random") || m.contains("non-deterministic")),
            "Math.random flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_time_sleep_flagged() {
        let src = r#"
def test_a():
    import time
    time.sleep(1)
"#;
        let r = analyze_test_maint(src, "python");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("time.sleep")),
            "Python sleep flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_random_flagged() {
        let src = r#"
def test_a():
    assert random.random() > 0
"#;
        let r = analyze_test_maint(src, "python");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("non-deterministic") || m.contains("seed")),
            "Python random flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn comment_excluded() {
        // Sleep in a comment must NOT count.
        let src = r#"
// std::thread::sleep(...)  // would be flaky if executable
#[test]
fn a() { assert_eq!(1, 1); }
#[test]
fn b() { assert_eq!(2, 2); }
#[test]
fn c() { assert_eq!(3, 3); }
#[test]
fn d() { assert_eq!(4, 4); }
"#;
        let r = analyze_test_maint(src, "rust");
        assert_eq!(
            r.violations, 0,
            "commented sleep must not count: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_test_maint(
            r#"
#[test] #[ignore] fn a() {}
#[test] #[ignore] fn b() {}
#[test] #[ignore] fn c() {}
#[test] #[ignore] fn d() {}
#[test] fn e() { std::thread::sleep(std::time::Duration::from_secs(1)); }
"#,
            "rust",
        );
        let good = analyze_test_maint(
            r#"
#[test] fn a() { assert_eq!(1, 1); }
#[test] fn b() { assert_eq!(2, 2); }
#[test] fn c() { assert_eq!(3, 3); }
#[test] fn d() { assert_eq!(4, 4); }
#[test] fn e() { assert_eq!(5, 5); }
"#,
            "rust",
        );
        assert!(
            score_test_maint(&bad) < score_test_maint(&good),
            "ignored+sleep ({:.3}) must score below clean ({:.3})",
            score_test_maint(&bad),
            score_test_maint(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_test_maint(
            r#"#[test] #[ignore] fn a() {}
#[test] #[ignore] fn b() {}
#[test] #[ignore] fn c() {}
#[test] #[ignore] fn d() {}
"#,
            "rust",
        );
        let s = score_test_maint(&r);
        assert!(s > 0.0, "short file with 4 ignored must not score 0.0: {s}");
    }
}
