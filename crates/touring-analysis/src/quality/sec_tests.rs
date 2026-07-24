//! Security test coverage analysis (D32 / F3.6) — polyglot detector of the
//! canonical "controls-without-tests / hope-not-proof" smell. F2.1/F2.3
//! detect security controls in code; F3.6 checks whether tests NEGATE the
//! attack (positive tests don't prove the control — "user can read" doesn't
//! prove "user without permission can't").
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `no-auth-test` | file with auth-related code (token / cookie / auth) but ZERO `test_auth*` / `test_login*` / `test_jwt*` / `test_session*` | all |
//! | `no-authz-test` | file with role/permission code but ZERO `test_authz*` / `test_forbidden*` / `test_403*` / `test_401*` | all |
//! | `no-input-validation-test` | file with input-validation code (sanitiz/valid) but ZERO `test_xss*` / `test_sql_injection*` / `test_sanitiz*` | all |
//! | `no-negative-test` | security test (auth/authz) without `assert_eq!(*status*, 401)` / `assert_eq!(*status*, 403)` / `.toBe(401)` / `.toBe(403)` (positive-only test) | all |
//! | `no-dast-reference` | HTTP-handling file (reqwest/actix/axum/Express) without `zap_` / `burp` / `owasp` reference (no DAST in CI) | all |
//!
//! **Disjoint** from F2.1 OWASP (F2.1 detects the *sink* in code; F3.6 detects
//! whether tests *cover the sink*); F2.3 authz (F2.3 detects missing
//! authorization checks; F3.6 detects missing *tests* of the authorization);
//! F2.2 input validation (F2.2 detects unsafe input handling; F3.6 detects
//! missing test coverage for validation paths).
//!
//! **Sources (context7, `/zaproxy/zapropy`, High reputation; OWASP ZAP
//! baseline scan)**: the gold standard is a DAST scan in CI (`zap-baseline.py`)
//! PLUS unit/integration tests with explicit negative cases
//! (`expect(response.status).toBe(401)`). A test that only verifies the
//! happy path leaves a 401-bypass regression open.
//!
//! Comments / `#[cfg(test)]` are excluded via [`super::code_regions`].

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::density_score;

/// Density→score scale (ADVISORY-tier).
const SCALE: f32 = 6.0;

/// Authn test needles.
const TEST_AUTH: &[u8] = b"test_auth";
const TEST_LOGIN: &[u8] = b"test_login";
const TEST_LOGOUT: &[u8] = b"test_logout";
const TEST_JWT: &[u8] = b"test_jwt";
const TEST_SESSION: &[u8] = b"test_session";
const TEST_PASSWORD: &[u8] = b"test_password";
const TEST_TOKEN: &[u8] = b"test_token";

/// Authz test needles (negative tests — prove the NEGATION).
const TEST_AUTHZ: &[u8] = b"test_authz";
const TEST_AUTHORIZ: &[u8] = b"test_authoriz";
const TEST_PERMISSION: &[u8] = b"test_permission";
const TEST_ROLE: &[u8] = b"test_role";
const TEST_FORBIDDEN: &[u8] = b"test_forbidden";
const TEST_UNAUTHOR: &[u8] = b"test_unauthor";
const TEST_401: &[u8] = b"test_401";
const TEST_403: &[u8] = b"test_403";

/// Input-validation test needles.
const TEST_XSS: &[u8] = b"test_xss";
const TEST_SQL_INJECTION: &[u8] = b"test_sql_injection";
const TEST_SANITIZ: &[u8] = b"test_sanitiz";
const TEST_VALID: &[u8] = b"test_valid";
const TEST_CSRF: &[u8] = b"test_csrf";

/// DAST (Dynamic Application Security Testing) reference.
const ZAP: &[u8] = b"zap_";
const ZAP_BASELINE: &[u8] = b"zap-baseline";
const BURP: &[u8] = b"burp";
const GAUNTLT: &[u8] = b"gauntlt";
const OWASP_TEST: &[u8] = b"owasp";

/// Authn-related code signals (heuristic: if any of these appear, the file
/// handles auth and SHOULD have auth tests).
const CODE_TOKEN: &[u8] = b"token";
const CODE_COOKIE: &[u8] = b"cookie";
const CODE_AUTH: &[u8] = b"auth";
const CODE_LOGIN: &[u8] = b"login";
const CODE_PASSWORD: &[u8] = b"password";
const CODE_JWT: &[u8] = b"jwt";
const CODE_SESSION: &[u8] = b"session";

/// Authz-related code signals.
const CODE_ROLE: &[u8] = b"role";
const CODE_PERMISSION: &[u8] = b"permission";
const CODE_FORBIDDEN: &[u8] = b"forbidden";
const CODE_UNAUTHOR: &[u8] = b"unauthor";

/// Input-validation code signals.
const CODE_SANITIZ: &[u8] = b"sanitiz";
const CODE_VALID: &[u8] = b"valid";
const CODE_ESCAPE: &[u8] = b"escape";

/// HTTP framework code signals.
const CODE_REQWEST: &[u8] = b"reqwest";
const CODE_ACTIX: &[u8] = b"actix";
const CODE_AXUM: &[u8] = b"axum";
const CODE_EXPRESS: &[u8] = b"express";
const CODE_HAPI: &[u8] = b"@hapi";
const CODE_FASTAPI: &[u8] = b"fastapi";
const CODE_FLASK: &[u8] = b"flask";
const CODE_DJANGO: &[u8] = b"django";

/// Strong negative-test patterns (status code assertions for 401/403).
const STATUS_401: &[u8] = b"401";
const STATUS_403: &[u8] = b"403";
const DENY: &[u8] = b"deny";
const REJECT: &[u8] = b"reject";
const FORBID: &[u8] = b"forbid";

/// Findings of a single security-test analysis pass.
#[derive(Debug, Clone, Default)]
pub struct SecTestsReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl SecTestsReport {
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

/// Authn-test marker count.
fn count_authn_tests(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, TEST_AUTH)
        + count_executable(bytes, regions, TEST_LOGIN)
        + count_executable(bytes, regions, TEST_LOGOUT)
        + count_executable(bytes, regions, TEST_JWT)
        + count_executable(bytes, regions, TEST_SESSION)
        + count_executable(bytes, regions, TEST_PASSWORD)
        + count_executable(bytes, regions, TEST_TOKEN)
}

/// Authz-test marker count.
fn count_authz_tests(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, TEST_AUTHZ)
        + count_executable(bytes, regions, TEST_AUTHORIZ)
        + count_executable(bytes, regions, TEST_PERMISSION)
        + count_executable(bytes, regions, TEST_ROLE)
        + count_executable(bytes, regions, TEST_FORBIDDEN)
        + count_executable(bytes, regions, TEST_UNAUTHOR)
        + count_executable(bytes, regions, TEST_401)
        + count_executable(bytes, regions, TEST_403)
}

/// Input-validation test marker count.
fn count_input_tests(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, TEST_XSS)
        + count_executable(bytes, regions, TEST_SQL_INJECTION)
        + count_executable(bytes, regions, TEST_SANITIZ)
        + count_executable(bytes, regions, TEST_VALID)
        + count_executable(bytes, regions, TEST_CSRF)
}

/// DAST (zap/burp) reference count.
fn count_dast_refs(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, ZAP)
        + count_executable(bytes, regions, ZAP_BASELINE)
        + count_executable(bytes, regions, BURP)
        + count_executable(bytes, regions, GAUNTLT)
        + count_executable(bytes, regions, OWASP_TEST)
}

/// Authn code-signal count (heuristic: file handles auth).
fn count_authn_signals(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, CODE_TOKEN)
        + count_executable(bytes, regions, CODE_COOKIE)
        + count_executable(bytes, regions, CODE_AUTH)
        + count_executable(bytes, regions, CODE_LOGIN)
        + count_executable(bytes, regions, CODE_PASSWORD)
        + count_executable(bytes, regions, CODE_JWT)
        + count_executable(bytes, regions, CODE_SESSION)
}

/// Authz code-signal count.
fn count_authz_signals(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, CODE_ROLE)
        + count_executable(bytes, regions, CODE_PERMISSION)
        + count_executable(bytes, regions, CODE_FORBIDDEN)
        + count_executable(bytes, regions, CODE_UNAUTHOR)
}

/// Input-validation code-signal count.
fn count_input_signals(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, CODE_SANITIZ)
        + count_executable(bytes, regions, CODE_VALID)
        + count_executable(bytes, regions, CODE_ESCAPE)
}

/// HTTP-framework code-signal count.
fn count_http_signals(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, CODE_REQWEST)
        + count_executable(bytes, regions, CODE_ACTIX)
        + count_executable(bytes, regions, CODE_AXUM)
        + count_executable(bytes, regions, CODE_EXPRESS)
        + count_executable(bytes, regions, CODE_HAPI)
        + count_executable(bytes, regions, CODE_FASTAPI)
        + count_executable(bytes, regions, CODE_FLASK)
        + count_executable(bytes, regions, CODE_DJANGO)
}

/// Negative-test pattern count (status code or deny verb).
fn count_negative_asserts(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, STATUS_401)
        + count_executable(bytes, regions, STATUS_403)
        + count_executable(bytes, regions, DENY)
        + count_executable(bytes, regions, REJECT)
        + count_executable(bytes, regions, FORBID)
}

/// Analyze security-test coverage in `source` for the given language.
pub fn analyze_sec_tests(source: &str, lang: &str) -> SecTestsReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = SecTestsReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    let authn_code = count_authn_signals(bytes, &regions);
    let authz_code = count_authz_signals(bytes, &regions);
    let input_code = count_input_signals(bytes, &regions);
    let http_code = count_http_signals(bytes, &regions);
    let authn_tests = count_authn_tests(bytes, &regions);
    let authz_tests = count_authz_tests(bytes, &regions);
    let input_tests = count_input_tests(bytes, &regions);
    let dast_refs = count_dast_refs(bytes, &regions);
    let negative_asserts = count_negative_asserts(bytes, &regions);

    // Authn code present but ZERO authn tests.
    if authn_code >= 2 && authn_tests == 0 {
        report.push(
            "file handles auth (token/cookie/login/password) but has no \
             test_auth*/test_login*/test_jwt* test — controls untested",
            1,
            0.7,
        );
    }
    // Authz code present but ZERO authz tests.
    if authz_code >= 2 && authz_tests == 0 {
        report.push(
            "file handles authorization (role/permission/forbidden) but has \
             no test_authz*/test_forbidden*/test_403* test — authz untested",
            1,
            0.7,
        );
    }
    // Input-validation code present but ZERO input-validation tests.
    if input_code >= 2 && input_tests == 0 {
        report.push(
            "file handles input validation (sanitize/validate/escape) but has \
             no test_xss*/test_sql_injection*/test_sanitiz* test — validation untested",
            1,
            0.6,
        );
    }
    // Security tests present but ZERO negative asserts (proves the attack is rejected).
    if (authn_tests + authz_tests) >= 1 && negative_asserts == 0 {
        report.push(
            "security test(s) present but no status 401/403 / deny / reject \
             assertion — positive-only test, does not prove the control",
            1,
            0.6,
        );
    }
    // HTTP-handling file with no DAST reference.
    if http_code >= 1 && dast_refs == 0 {
        report.push(
            "HTTP framework in use (reqwest/actix/axum/express) without \
             zap-baseline / burp / owasp reference — no DAST in CI",
            1,
            0.5,
        );
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`SecTestsReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
pub fn score_sec_tests(report: &SecTestsReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authn_code_no_test_flagged() {
        let src = r#"
fn login(token: &str) -> bool {
    !token.is_empty() && validate_cookie(token)
}
fn logout() {}
"#;
        let r = analyze_sec_tests(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("test_auth") && m.contains("untested")),
            "authn code without test flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn authn_code_with_test_clean() {
        let src = r#"
fn login(token: &str) -> bool { !token.is_empty() }
fn test_login() { assert!(login("valid")); }
fn test_session() { assert!(true); }
"#;
        let r = analyze_sec_tests(src, "rust");
        // No authn-coverage finding.
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("controls untested")),
            "authn with test is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn authz_code_no_test_flagged() {
        let src = r#"
fn check_permission(role: &Role, user: &User) -> bool {
    user.role == role
}
fn is_forbidden() -> bool { true }
"#;
        let r = analyze_sec_tests(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("test_authz") && m.contains("untested")),
            "authz code without test flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn input_validation_no_test_flagged() {
        let src = r#"
fn sanitize(s: &str) -> String { s.replace("'", "''") }
fn validate(s: &str) -> bool { !s.contains(";") }
fn escape_html(s: &str) -> String { s.replace("<", "&lt;") }
"#;
        let r = analyze_sec_tests(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("validation untested")),
            "input-validation without test flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn security_test_positive_only_flagged() {
        let src = r#"
fn test_login_valid() { assert!(login("valid")); }
"#;
        let r = analyze_sec_tests(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("positive-only")),
            "positive-only security test flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn security_test_with_negative_clean() {
        let src = r#"
fn test_login_valid() { assert!(login("valid")); }
fn test_403() { assert_eq!(check_permission(&USER, &RESOURCE), false); }
"#;
        let r = analyze_sec_tests(src, "rust");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("positive-only")),
            "with negative assert is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn http_no_dast_flagged() {
        let src = r#"
use reqwest;
fn handler() { reqwest::get("https://api.example.com"); }
"#;
        let r = analyze_sec_tests(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("no DAST")),
            "HTTP without DAST flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn http_with_dast_clean() {
        // DAST ref is a string literal (not a comment) — must be detected.
        let src = r#"
use reqwest;
const DAST_CMD: &str = "zap-baseline.py --target https://api.example.com";
fn handler() { reqwest::get("https://api.example.com"); }
"#;
        let r = analyze_sec_tests(src, "rust");
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("no DAST")),
            "HTTP with DAST reference is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn clean_production_file_clean() {
        let src = r#"
fn add(a: i32, b: i32) -> i32 { a + b }
"#;
        let r = analyze_sec_tests(src, "rust");
        assert_eq!(r.violations, 0, "no security surface: {:?}", r.findings);
    }

    #[test]
    fn comment_excluded() {
        // Authn signal in comment must NOT count.
        let src = r#"
// fn login(token: &str) -> bool { !token.is_empty() }
fn prod() { 1 + 2 }
"#;
        let r = analyze_sec_tests(src, "rust");
        assert_eq!(
            r.violations, 0,
            "commented authn signal excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_sec_tests(
            r#"
fn login(token: &str) -> bool { !token.is_empty() }
fn check_permission(role: &Role, user: &User) -> bool { user.role == role }
fn sanitize(s: &str) -> String { s.replace("'", "''") }
fn escape_html(s: &str) -> String { s.replace("<", "&lt;") }
fn is_forbidden() -> bool { true }
"#,
            "rust",
        );
        let good = analyze_sec_tests(
            r#"
fn add(a: i32, b: i32) -> i32 { a + b }
"#,
            "rust",
        );
        assert!(
            score_sec_tests(&bad) < score_sec_tests(&good),
            "untested-controls file ({:.3}) must score below no-surface file ({:.3})",
            score_sec_tests(&bad),
            score_sec_tests(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_sec_tests(
            r#"fn login(t: &str) -> bool { !t.is_empty() }
fn check_role(r: &Role) -> bool { true }
fn sanitize(s: &str) -> String { s.to_string() }
fn escape_html(s: &str) -> String { s.to_string() }
fn is_forbidden() -> bool { true }
fn has_permission() -> bool { false }
"#,
            "rust",
        );
        let s = score_sec_tests(&r);
        assert!(
            s > 0.0,
            "short file with 6 surface signals must not score 0.0: {s}"
        );
    }
}
