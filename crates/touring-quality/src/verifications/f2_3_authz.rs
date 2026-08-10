//! F2.3 — Authentication/Authorization verifier (D16).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_authz`] — a polyglot detector of
//! the canonical "broken access control" smell (OWASP A01:2021 — the #1
//! critical security risk):
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `sensitive-no-authz` | sensitive identifier (admin/delete_user/role/permission) without `authorize`/`require_role`/`check_permission`/`verify_token` call | all |
//! | `idor-pattern` | function with `user_id`/`account_id`/`doc_id` parameter but no authz call (CWE-639 — Insecure Direct Object Reference) | all |
//! | `hardcoded-role-string` | `if .*\.role == "admin"` / `== "user"` (string-literal role comparison) | all |
//! | `client-side-authz` | `window.confirm` / `localStorage.getItem` / `location.search` / `document.cookie` (authz in browser — bypass is trivial) | JS/TS |
//!
//! Disjoint from F2.1 OWASP injection (F2.1 detects the *sink*; F2.3
//! detects the *missing authorization gate*), F2.2 input validation (F2.2
//! detects unsafe input handling; F2.3 detects missing access checks on
//! sensitive ops), and F3.6 sec-tests (F3.6 detects missing tests of
//! authz; F2.3 detects missing authz code itself).
//!
//! **Sources (context7, `/owasp/cheatsheetseries`, High reputation, bench
//! 78.47; Authorization_Cheat_Sheet; Insecure_Direct_Object_Reference
//! _Prevention_Cheat_Sheet)**: "Authorization strategies must rely on
//! server-side enforcement with a default-deny policy… verify the prevention
//! of IDOR (CWE-639) and privilege escalation, ensure function-level controls
//! are protected, and confirm that access control logic is centralized and
//! consistently applied after authentication." Per the IDOR Cheat Sheet:
//! "Applications should avoid exposing internal object identifiers (like
//! database primary keys) directly to users, as this can lead to
//! authorization bypasses and privilege escalation vulnerabilities."
//!
//! **Standalone fallback (`--no-default-features`)**: an `authenticate`/
//! `authorize`/`permission` density heuristic (preserves the W3/W4 stub
//! interface).
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. WARN-tier
//! (one of the 6 BLOCK-aligned dims — F2.1/F2.3/F2.4/F2.5/F2.6/F4.3/F4.5
//! — but per-file detect; workspace-level authz cycle detection is F1.12).

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F2.3 verifier — Authentication/Authorization.
#[allow(non_camel_case_types)]
pub struct F2_3_Authz;

impl Verification for F2_3_Authz {
    fn id(&self) -> DimId {
        DimId::F2_3
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_authz_dim(target)
    }
}

// ── Real engine: authz detector ────────────────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_authz_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_authz, score_authz};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F2.3: detector own source (authz needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_authz(&raw, lang);
    let value = score_authz(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F2.3: {} broken-access-control smell(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_authz: sensitive-no-authz / idor-pattern / \
         hardcoded-role-string / client-side-authz — OWASP A01){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the authz needle vocabulary as
/// detection data, so scoring their own source is a self-match false positive.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: auth-density heuristic ────────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_authz_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let auth = raw.matches("authenticate").count()
        + raw.matches("authorize").count()
        + raw.matches("permission").count();
    let value = if auth > 0 { 1.0 } else { 0.5 };
    let evidence = format!(
        "{auth} authz/permission call(s) over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full authz analysis — OWASP A01)"
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
    fn test_authz_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F2_3_Authz.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_authz_empty_file() {
        let f = write_temp("");
        let s = F2_3_Authz.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Sensitive op WITH authz call scores higher than sensitive op WITHOUT.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_with_authz_call_scores_higher_than_without() {
        let bad = write_temp_ext(
            "fn delete_user(user_id: u64) -> Result<(), String> { db.delete(user_id); Ok(()) }\n\
             fn assign_role(user_id: u64, role: &str) { db.set_role(user_id, role); }\n",
            ".rs",
        );
        let good = write_temp_ext(
            "fn authorize(actor: &User, op: &str) -> Result<(), AppError> { Ok(()) }\n\
             fn delete_user(user_id: u64, actor: &User) -> Result<(), AppError> {\n\
                 authorize(actor, \"delete_user\")?; db.delete(user_id); Ok(())\n\
             }\n",
            ".rs",
        );
        let sb = F2_3_Authz.check(bad.path()).expect("check");
        let sg = F2_3_Authz.check(good.path()).expect("check");
        assert!(
            sg.value > sb.value,
            "with-authz file ({}) must score above without-authz file ({})",
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
            "/x/touring-analysis/src/quality/authz.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
