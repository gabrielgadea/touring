//! Test pyramid analysis (D29 / F3.3) — polyglot detector of the canonical
//! "ice-cream cone / pyramid-inverted" smell. The healthy pyramid has many
//! unit tests (fast, isolated), fewer integration, and few E2E (slow, fragile).
//! An inverted pyramid is dominated by E2E tests that are slow and flaky.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `large-suite-no-e2e` | Rust project with ≥5 `#[test]` but no Playwright/Cypress/Selenium (missing E2E pyramid top) | Rust |
//! | `e2e-heavy-no-unit` | JS/TS file with multiple browser actions (`page.click`/`page.locator`/`page.goto`/`page.fill`) but no `it(`/`test(` (heavy E2E, no unit base) | JS/TS |
//! | `e2e-no-pytest` | Python file references Selenium/Playwright but no `def test_*` (E2E-only test file) | Python |
//! | `e2e-no-assertion` | JS/TS file with multiple browser actions but no `.toBeVisible` / `.toHaveText` (E2E test is a no-op) | JS/TS |
//!
//! **Disjoint** from F3.1 coverage (F3.1 measures *what executed*; F3.3 keys
//! on the *layer shape* — unit vs integration vs E2E); F3.4 edge cases
//! (F3.4 keys on property-based coverage; F3.3 keys on the layer mix);
//! F3.5 maint (F3.5 keys on flakiness; F3.3 keys on the layer ratio).
//!
//! **Sources (context7, `/microsoft/playwright`, High reputation; Cypress;
//! Playwright auto-wait)**: Playwright/Cypress E2E tests are slow and flaky;
//! best practice is to keep them as a narrow top of the pyramid, with
//! broad unit/integration base. Auto-wait (`expect(locator).toBeVisible()`)
//! reduces flakiness, but the cost is still 10-100× unit tests.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::density_score;

/// Density→score scale (ADVISORY-tier).
const SCALE: f32 = 6.0;

/// Rust test markers (unit).
const RUST_TEST: &[u8] = b"#[test]";
/// JS/TS unit markers (Jest/Vitest).
const JS_TEST_IT: &[u8] = b"it(";
const JS_TEST_TEST: &[u8] = b"test(";
/// Python unit markers (pytest).
const PYTHON_TEST_DEF: &[u8] = b"def test_";

/// E2E framework references.
const E2E_PLAYWRIGHT: &[u8] = b"playwright";
const E2E_CYPRESS: &[u8] = b"cypress";
const E2E_SELENIUM: &[u8] = b"selenium";
const E2E_PUPPETEER: &[u8] = b"puppeteer";
const E2E_K6: &[u8] = b"k6";

/// Playwright/Cypress locator / action (browser-side).
const BROWSER_LOCATOR: &[u8] = b"page.locator";
const BROWSER_CLICK: &[u8] = b"page.click";
const BROWSER_GOTO: &[u8] = b"page.goto";
const BROWSER_FILL: &[u8] = b"page.fill";

/// Strong assertions (browser-side).
const BROWSER_ASSERT: &[u8] = b".toBeVisible";
const BROWSER_ASSERT_TEXT: &[u8] = b".toHaveText";

/// Findings of a single test-pyramid analysis pass: the canonical
/// "ice-cream cone / pyramid-inverted" smell rolled up per file.
#[derive(Debug, Clone, Default)]
pub struct TestPyramidReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl TestPyramidReport {
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

/// Line-walk count of `#[test]` (handles the region-marker issue).
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

/// E2E framework reference count (any of the canonical frameworks).
fn count_e2e_refs(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, E2E_PLAYWRIGHT)
        + count_executable(bytes, regions, E2E_CYPRESS)
        + count_executable(bytes, regions, E2E_SELENIUM)
        + count_executable(bytes, regions, E2E_PUPPETEER)
        + count_executable(bytes, regions, E2E_K6)
}

/// Browser-side action count (page.click / page.locator / etc.).
fn count_browser_actions(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, BROWSER_LOCATOR)
        + count_executable(bytes, regions, BROWSER_CLICK)
        + count_executable(bytes, regions, BROWSER_GOTO)
        + count_executable(bytes, regions, BROWSER_FILL)
}

/// Analyze test-pyramid shape in `source` for the given language.
pub fn analyze_test_pyramid(source: &str, lang: &str) -> TestPyramidReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = TestPyramidReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        "rust" | "rs" => {
            let tests = count_rust_tests(bytes);
            let e2e_refs = count_e2e_refs(bytes, &regions);
            // Large test suite (≥ 5) without ANY E2E framework reference → no
            // pyramid top; missing the E2E guard for browser/HTTP behaviour.
            if tests >= 5 && e2e_refs == 0 {
                report.push(
                    "large test suite (≥5 #[test]) with no Playwright/Cypress/Selenium \
                     reference (missing E2E pyramid top — browser flows untested)",
                    1,
                    0.4,
                );
            }
        }
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => {
            let unit = count_executable(bytes, &regions, JS_TEST_IT)
                + count_executable(bytes, &regions, JS_TEST_TEST);
            let actions = count_browser_actions(bytes, &regions);
            let asserts = count_executable(bytes, &regions, BROWSER_ASSERT)
                + count_executable(bytes, &regions, BROWSER_ASSERT_TEXT);
            // File is dominated by browser actions (E2E) without unit markers.
            if actions >= 2 && unit == 0 {
                report.push(
                    "browser-only test file (page.click/locator/goto without it(/test() \
                     — inverted pyramid: heavy E2E, no unit base)",
                    1,
                    0.7,
                );
            }
            // E2E test file with no strong assertions (auto-wait is weak here).
            if actions >= 2 && asserts == 0 {
                report.push(
                    "browser actions (page.click/locator/goto) without .toBeVisible \
                     / .toHaveText assertions (E2E test is a no-op — flaky regression guard)",
                    1,
                    0.6,
                );
            }
        }
        "python" | "py" => {
            let unit = count_executable(bytes, &regions, PYTHON_TEST_DEF);
            let e2e_refs = count_e2e_refs(bytes, &regions);
            // Test file with E2E framework but no pytest unit markers.
            if e2e_refs >= 1 && unit == 0 {
                report.push(
                    "Selenium/Playwright reference without pytest def test_* markers \
                     (E2E-only test file — inverted pyramid)",
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

/// Score a [`TestPyramidReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
pub fn score_test_pyramid(report: &TestPyramidReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_with_tests_clean_when_e2e_present() {
        let src = r#"
use playwright;
#[test] fn a() { assert_eq!(1, 1); }
#[test] fn b() { assert_eq!(2, 2); }
#[test] fn c() { assert_eq!(3, 3); }
#[test] fn d() { assert_eq!(4, 4); }
#[test] fn e() { assert_eq!(5, 5); }
"#;
        let r = analyze_test_pyramid(src, "rust");
        assert_eq!(
            r.violations, 0,
            "playwright + 5 tests is healthy: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_large_suite_no_e2e_flagged() {
        let src = r#"
#[test] fn a() {}
#[test] fn b() {}
#[test] fn c() {}
#[test] fn d() {}
#[test] fn e() {}
#[test] fn f() {}
"#;
        let r = analyze_test_pyramid(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("missing E2E pyramid top")),
            "6 tests, no E2E flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_small_suite_no_e2e_clean() {
        let src = r#"
#[test] fn a() {}
#[test] fn b() {}
"#;
        let r = analyze_test_pyramid(src, "rust");
        assert_eq!(
            r.violations, 0,
            "small suite (<5) is fine without E2E: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_browser_actions_no_unit_flagged() {
        // No `test(` / `it(` markers — file is pure browser-script (E2E only).
        // Uses describe blocks (which are NOT counted as unit markers in our
        // analyzer — they are grouping, not assertion markers).
        let src = r#"
describe('home', () => {
    page.goto('/');
    page.click('#login');
});
describe('dashboard', () => {
    page.goto('/dash');
    page.locator('#menu').click();
});
"#;
        let r = analyze_test_pyramid(src, "typescript");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("inverted pyramid")),
            "browser-only flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_browser_actions_with_assertions_clean() {
        let src = r#"
test('home', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('#login')).toBeVisible();
});
test('dashboard', async ({ page }) => {
    await page.goto('/dash');
    await expect(page.locator('#menu')).toBeVisible();
});
"#;
        let r = analyze_test_pyramid(src, "typescript");
        assert_eq!(
            r.violations, 0,
            "browser + assert is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_browser_actions_no_assertions_flagged() {
        let src = r#"
test('a', async ({ page }) => {
    await page.goto('/');
    await page.click('#x');
});
test('b', async ({ page }) => {
    await page.click('#y');
});
"#;
        let r = analyze_test_pyramid(src, "javascript");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("no-op") || m.contains("flaky")),
            "browser actions without assert flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_with_unit_and_e2e_clean() {
        let src = r#"
from playwright.sync_api import sync_playwright

def test_unit_addition():
    assert 1 + 1 == 2
"#;
        let r = analyze_test_pyramid(src, "python");
        assert_eq!(
            r.violations, 0,
            "pytest + playwright is balanced: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_e2e_no_pytest_flagged() {
        let src = r#"
from selenium import webdriver

def setup():
    driver = webdriver.Chrome()
    return driver
"#;
        let r = analyze_test_pyramid(src, "python");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("inverted pyramid")),
            "selenium without pytest flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn empty_file_clean() {
        let r = analyze_test_pyramid("", "rust");
        assert_eq!(r.violations, 0);
    }

    #[test]
    fn comment_excluded() {
        // Playwright reference inside a comment must NOT count as E2E.
        // (If it did, the file would have 5+ #[test] + 1 playwright ref → no
        // "missing E2E top" finding, demonstrating the comment was excluded.)
        let src = r#"
// use playwright; // would be E2E if executable
fn prod() { 1 + 2 }
"#;
        let r = analyze_test_pyramid(src, "rust");
        // 0 tests → no missing-E2E-top finding (the threshold is tests >= 5).
        assert_eq!(
            r.violations, 0,
            "no tests → no missing-E2E-top finding: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_test_pyramid(
            r#"
test('a', async ({ page }) => {
    await page.goto('/');
    await page.click('#x');
});
test('b', async ({ page }) => {
    await page.click('#y');
});
"#,
            "javascript",
        );
        let good = analyze_test_pyramid(
            r#"
test('a', () => { expect(1).toBe(1); });
test('b', () => { expect(2).toBe(2); });
"#,
            "javascript",
        );
        assert!(
            score_test_pyramid(&bad) < score_test_pyramid(&good),
            "browser-heavy ({:.3}) must score below unit-rich ({:.3})",
            score_test_pyramid(&bad),
            score_test_pyramid(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_test_pyramid(
            r#"
test('a', async ({ page }) => { await page.goto('/'); await page.click('#x'); });
test('b', async ({ page }) => { await page.click('#y'); });
"#,
            "javascript",
        );
        let s = score_test_pyramid(&r);
        assert!(s > 0.0, "short browser-heavy file must not score 0.0: {s}");
    }
}
