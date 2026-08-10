//! F3.6 — Security Test Gaps verifier (D32).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_sec_tests`] — a polyglot detector
//! of the canonical "controls-without-tests / hope-not-proof" smell:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `no-authn-test` | file handles auth (token/cookie/login/password/jwt/session) but has no `test_auth*`/`test_login*`/`test_jwt*`/`test_session*` test | all |
//! | `no-authz-test` | file handles authorization (role/permission/forbidden/unauthorized) but has no `test_authz*`/`test_forbidden*`/`test_403*` test | all |
//! | `no-input-validation-test` | file handles input validation (sanitize/validate/escape) but has no `test_xss*`/`test_sql_injection*`/`test_sanitiz*` test | all |
//! | `positive-only-security-test` | security test(s) present but no `401` / `403` / `deny` / `reject` assertion (does not prove the NEGATION) | all |
//! | `no-dast-in-ci` | HTTP framework (reqwest/actix/axum/express) in use without `zap-baseline` / `burp` / `owasp` reference (no DAST in CI) | all |
//!
//! Disjoint from F2.1 OWASP (F2.1 detects the *sink* in code; F3.6 detects
//! whether tests *cover the sink*), F2.3 authz (F2.3 detects missing
//! authorization checks; F3.6 detects missing *tests* of the authorization),
//! and F2.2 input validation (F2.2 detects unsafe input handling; F3.6
//! detects missing test coverage for validation paths).
//!
//! **Sources (context7, `/zaproxy/zaproxy`, High reputation; OWASP ZAP
//! baseline scan)**: the gold standard is a DAST scan in CI
//! (`zap-baseline.py`) PLUS unit/integration tests with explicit negative
//! cases (`expect(response.status).toBe(401)`). A test that only verifies the
//! happy path leaves a 401-bypass regression open.
//!
//! **Standalone fallback (`--no-default-features`)**: an `test_auth`/
//! `test_login`/`test_xss`/`test_sql_injection` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F3.6 verifier — Security Test Gaps.
#[allow(non_camel_case_types)]
pub struct F3_6_SecTests;

impl Verification for F3_6_SecTests {
    fn id(&self) -> DimId {
        DimId::F3_6
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_sec_tests_dim(target)
    }
}

// ── Real engine: sec-tests detector ─────────────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_sec_tests_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_sec_tests, score_sec_tests};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.6: detector own source (sec_tests needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_sec_tests(&raw, lang);
    let value = score_sec_tests(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F3.6: {} security-test gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_sec_tests: no-authn-test / no-authz-test / \
         no-input-validation-test / positive-only-security-test / no-dast-in-ci){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the sec-tests needle vocabulary as
/// detection data, so scoring their own source is a self-match false positive.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: auth-test density heuristic ────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_sec_tests_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let auth_tests = raw.matches("test_auth").count()
        + raw.matches("test_login").count()
        + raw.matches("test_xss").count()
        + raw.matches("test_sql_injection").count();
    let value = if auth_tests > 0 { 1.0 } else { 0.3 };
    let evidence = format!(
        "{auth_tests} auth/security tests over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full sec-tests analysis)"
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
    fn test_sec_tests_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F3_6_SecTests.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_sec_tests_empty_file() {
        let f = write_temp("");
        let s = F3_6_SecTests.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Auth-handling file with auth-test scores higher than auth-handling
    /// file WITHOUT auth-test.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_auth_test_scores_higher_than_no_auth_test() {
        // Fixtures with rich enough auth/authz signal density to actually
        // trigger the detector (the `>= 2 signals` threshold).
        let bad = write_temp_ext(
            "fn login(token: &str) -> bool { !token.is_empty() && validate_cookie(token) }\n\
             fn check_role(user: &User) -> bool { user.permission == \"admin\" }\n",
            ".rs",
        );
        let good = write_temp_ext(
            "fn login(token: &str) -> bool { !token.is_empty() && validate_cookie(token) }\n\
             fn check_role(user: &User) -> bool { user.permission == \"admin\" }\n\
             fn test_login() { assert!(login(\"valid\")); }\n\
             fn test_403() { assert_eq!(check_permission(&USER, &RES), false); }\n",
            ".rs",
        );
        let sb = F3_6_SecTests.check(bad.path()).expect("check");
        let sg = F3_6_SecTests.check(good.path()).expect("check");
        assert!(
            sg.value > sb.value,
            "with-test file ({}) must score above without-test file ({})",
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
            "/x/touring-analysis/src/quality/sec_tests.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
