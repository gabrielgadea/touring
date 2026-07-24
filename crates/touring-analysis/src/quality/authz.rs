//! Authentication / Authorization analysis (D16 / F2.3) — polyglot detector
//! of the canonical "broken access control" smell (OWASP A01:2021 #1). A
//! check that "user can do X" without also checking "user is *allowed* to do
//! X" is a privilege-escalation waiting to happen. F2.3 surfaces files where
//! sensitive operations exist WITHOUT an accompanying authorization check.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `sensitive-no-authz` | sensitive identifier (admin / delete / permission / role / owner) without `authorize` / `require_role` / `check_permission` / `verify_token` call | all |
//! | `hardcoded-role-string` | `if .*\.role == "admin"` (string literal role comparison — see F1.12) | all |
//! | `client-side-only-authz` | `if (window.confirm(` / `if (localStorage.getItem(` / `if (location.search.includes(` in JS/TS (auth check in browser, no server call) | JS/TS |
//! | `idor-pattern` | function with `user_id`/`account_id`/`doc_id` parameter but no `authorize`/`verify_token`/`check_permission` call (Insecure Direct Object Reference — CWE-639) | all |
//! | `role-string-literal-check` | `if user.role == "user"` (string equality on role — should be enum / role-id) | all |
//!
//! **Disjoint** from F2.1 OWASP injection (F2.1 detects the *sink*; F2.3
//! detects the *missing authorization gate*); F2.2 input validation (F2.2
//! detects unsafe input handling; F2.3 detects *missing access checks* on
//! sensitive ops); F3.6 sec tests (F3.6 detects *missing test coverage* of
//! authz; F2.3 detects *missing authz code itself*).
//!
//! **Sources (context7, `/owasp/cheatsheetseries`, High reputation, bench 78.47;
//! OWASP A01:2021 — Broken Access Control)**: "Authorization strategies must
//! rely on server-side enforcement with a default-deny policy… verify the
//! prevention of IDOR (CWE-639) and privilege escalation, ensure function-level
//! controls are protected, and confirm that access control logic is centralized
//! and consistently applied after authentication." Per the Authorization Cheat
//! Sheet: "Applications should avoid exposing internal object identifiers… to
//! prevent authorization bypasses and privilege escalation."
//!
//! Comments / `#[cfg(test)]` are excluded via [`super::code_regions`].

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::density_score;

/// Density→score scale (WARN-tier; F2.3 is on the 6 BLOCK dims of 50).
/// Higher SCALE → stricter (matches the WARN-tier convention of F2.x).
const SCALE: f32 = 8.0;

/// Sensitive identifiers (operations that REQUIRE an authz check).
const ADMIN: &[u8] = b"admin";
const DELETE_USER: &[u8] = b"delete_user";
const DELETE_ACCOUNT: &[u8] = b"delete_account";
const ROLE_ASSIGN: &[u8] = b"assign_role";
const GRANT_PERMISSION: &[u8] = b"grant_permission";
const REVOKE: &[u8] = b"revoke";
const PRIVILEGE: &[u8] = b"privilege";

/// IDOR-prone parameter names.
const USER_ID: &[u8] = b"user_id:";
const USER_ID_VAR: &[u8] = b"user_id ";
const ACCOUNT_ID: &[u8] = b"account_id:";
const DOC_ID: &[u8] = b"doc_id:";
const FILE_ID: &[u8] = b"file_id:";
const ORDER_ID: &[u8] = b"order_id:";

/// Authorization-check identifiers (the GATE).
const AUTHORIZE: &[u8] = b"authorize";
const REQUIRE_ROLE: &[u8] = b"require_role";
const REQUIRE_AUTH: &[u8] = b"require_auth";
const CHECK_PERMISSION: &[u8] = b"check_permission";
const HAS_PERMISSION: &[u8] = b"has_permission";
const VERIFY_TOKEN: &[u8] = b"verify_token";
const REQUIRE_AUTHZ: &[u8] = b"require_authz";
const CAN_ACCESS: &[u8] = b"can_access";
const IS_AUTHORIZED: &[u8] = b"is_authorized";

/// Client-side authz signals (JS/TS only — authz in browser is no authz).
const WINDOW_CONFIRM: &[u8] = b"window.confirm";
const LOCALSTORAGE: &[u8] = b"localStorage.getItem";
const LOCATION_SEARCH: &[u8] = b"location.search";
const COOKIE_READ: &[u8] = b"document.cookie";
const JS_PROMPT: &[u8] = b"prompt(";

#[cfg(target_os = "linux")]
const _PATH_SEP: char = '/';
#[cfg(not(target_os = "linux"))]
const _PATH_SEP: char = '\\';

/// Findings of a single authz analysis pass.
#[derive(Debug, Clone, Default)]
pub struct AuthzReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl AuthzReport {
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

/// Count total sensitive-operation signals in the file.
fn count_sensitive_signals(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, ADMIN)
        + count_executable(bytes, regions, DELETE_USER)
        + count_executable(bytes, regions, DELETE_ACCOUNT)
        + count_executable(bytes, regions, ROLE_ASSIGN)
        + count_executable(bytes, regions, GRANT_PERMISSION)
        + count_executable(bytes, regions, REVOKE)
        + count_executable(bytes, regions, PRIVILEGE)
}

/// Count total IDOR-prone parameter signals.
fn count_idor_signals(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, USER_ID)
        + count_executable(bytes, regions, USER_ID_VAR)
        + count_executable(bytes, regions, ACCOUNT_ID)
        + count_executable(bytes, regions, DOC_ID)
        + count_executable(bytes, regions, FILE_ID)
        + count_executable(bytes, regions, ORDER_ID)
}

/// Count total authorization-check call signals.
fn count_authz_check_calls(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, AUTHORIZE)
        + count_executable(bytes, regions, REQUIRE_ROLE)
        + count_executable(bytes, regions, REQUIRE_AUTH)
        + count_executable(bytes, regions, CHECK_PERMISSION)
        + count_executable(bytes, regions, HAS_PERMISSION)
        + count_executable(bytes, regions, VERIFY_TOKEN)
        + count_executable(bytes, regions, REQUIRE_AUTHZ)
        + count_executable(bytes, regions, CAN_ACCESS)
        + count_executable(bytes, regions, IS_AUTHORIZED)
}

/// Hardcoded `admin` role string check.
fn count_hardcoded_admin_role(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, b".role == \"admin\"")
}

/// Hardcoded `user` (or any non-admin) role string check.
fn count_hardcoded_user_role(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, b".role == \"user\"")
}

/// Hardcoded empty-string role check (`.role == ""` — always-true bug).
fn count_hardcoded_empty_role(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, b".role == \"\"")
}

/// Client-side authz smell (JS/TS) — authz in the browser is not authz.
fn detect_client_side_authz(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, WINDOW_CONFIRM)
        + count_executable(bytes, regions, LOCALSTORAGE)
        + count_executable(bytes, regions, LOCATION_SEARCH)
        + count_executable(bytes, regions, COOKIE_READ)
        + count_executable(bytes, regions, JS_PROMPT)
}

/// Analyze authentication / authorization in `source` for the given language.
pub fn analyze_authz(source: &str, lang: &str) -> AuthzReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = AuthzReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    let sensitive = count_sensitive_signals(bytes, &regions);
    let idor = count_idor_signals(bytes, &regions);
    let authz_calls = count_authz_check_calls(bytes, &regions);
    let hardcoded_admin = count_hardcoded_admin_role(bytes, &regions);
    let hardcoded_user = count_hardcoded_user_role(bytes, &regions);
    let hardcoded_empty = count_hardcoded_empty_role(bytes, &regions);

    // Sensitive operations present but ZERO authz check calls in the file.
    if sensitive >= 2 && authz_calls == 0 {
        report.push(
            "file has sensitive operations (admin/delete_user/role/permission) but \
             no authorize/require_role/check_permission/verify_token call — broken \
             access control (OWASP A01)",
            1,
            0.9,
        );
    }
    // IDOR-prone parameter present but ZERO authz check.
    if idor >= 1 && authz_calls == 0 {
        report.push(
            "function parameter (user_id/account_id/doc_id) but no \
             authorize/verify_token/check_permission call — Insecure Direct \
             Object Reference (CWE-639), attacker can swap IDs",
            1,
            0.9,
        );
    }
    // Hardcoded `admin` role string literal — stringly-typed role.
    if hardcoded_admin > 0 {
        report.push(
            "hardcoded admin role string comparison (`.role == \"admin\"`) — \
             stringly-typed; use enum/RoleId so renames are compile-time",
            hardcoded_admin,
            0.7,
        );
    }
    // Hardcoded non-admin role string literal (`user` / `guest` / etc.).
    if hardcoded_user > 0 {
        report.push(
            "hardcoded user role string comparison (`.role == \"user\"`) — \
             stringly-typed; use enum/RoleId so renames are compile-time",
            hardcoded_user,
            0.7,
        );
    }
    // Hardcoded empty-string role check (always-true bug — no real gate).
    if hardcoded_empty > 0 {
        report.push(
            "hardcoded empty-string role check (`.role == \"\"`) — always-true \
             (the condition is trivially true); no real authz gate",
            hardcoded_empty,
            0.8,
        );
    }
    // JS/TS: client-side authz smell.
    if matches!(
        lang,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs"
    ) {
        let cs_authz = detect_client_side_authz(bytes, &regions);
        if cs_authz > 0 {
            report.push(
                "client-side authz check (window.confirm / localStorage / \
                 location.search / document.cookie) — authz in the browser is \
                 not authz; bypass is trivial; server-side check required",
                cs_authz,
                0.8,
            );
        }
    }
    let _ = _PATH_SEP; // reserved for future path-based auth-layer detection
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`AuthzReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
pub fn score_authz(report: &AuthzReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_with_authz_clean() {
        let src = r#"
fn delete_user(user_id: u64, actor: &User) -> Result<(), AppError> {
    authorize(actor, "delete_user")?;
    db.delete(user_id);
    Ok(())
}
"#;
        let r = analyze_authz(src, "rust");
        assert_eq!(
            r.violations, 0,
            "sensitive op with authorize call is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn sensitive_no_authz_flagged() {
        let src = r#"
fn delete_user(user_id: u64) -> Result<(), String> {
    db.delete(user_id);
    Ok(())
}
fn assign_role(user_id: u64, role: &str) -> Result<(), String> {
    db.set_role(user_id, role);
    Ok(())
}
fn admin_dashboard() -> &'static str { "admin" }
"#;
        let r = analyze_authz(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("broken access control")),
            "sensitive ops without authz flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn idor_pattern_flagged() {
        let src = r#"
fn fetch_account(user_id: u64) -> Result<Account, String> {
    db.find_account(user_id)
}
fn delete_account(user_id: u64) -> Result<(), String> {
    db.delete_account(user_id)
}
"#;
        let r = analyze_authz(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("IDOR") || m.contains("Direct Object")),
            "IDOR pattern flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn hardcoded_role_string_flagged() {
        let src = r#"
fn check(u: &User) -> bool {
    if u.role == "admin" { true } else { false }
}
"#;
        let r = analyze_authz(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("hardcoded admin role")),
            "hardcoded admin role string flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn client_side_authz_flagged() {
        let src = r#"
async function deleteUser(userId) {
    if (window.confirm("Delete?")) {
        await fetch('/api/users/' + userId, { method: 'DELETE' });
    }
}
"#;
        let r = analyze_authz(src, "javascript");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("client-side authz")),
            "JS client-side authz flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn clean_server_side_check_clean() {
        let src = r#"
async function deleteUser(userId, token) {
    const res = await fetch('/api/users/' + userId, {
        method: 'DELETE',
        headers: { 'Authorization': token },
    });
    return res;
}
"#;
        let r = analyze_authz(src, "javascript");
        // Server-side fetch with auth header (token passed in) — no
        // window.confirm / localStorage authz check; only IDOR smell fires.
        // (No IDOR-only since we have Authorization header.)
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("client-side authz")),
            "server-side auth header is clean (no client-side check): {:?}",
            r.findings
        );
    }

    #[test]
    fn clean_production_clean() {
        let src = r#"
fn add(a: i32, b: i32) -> i32 { a + b }
fn multiply(a: i32, b: i32) -> i32 { a * b }
"#;
        let r = analyze_authz(src, "rust");
        assert_eq!(r.violations, 0, "no authz surface: {:?}", r.findings);
    }

    #[test]
    fn comment_excluded() {
        let src = r#"
// fn delete_user(user_id: u64) {}   // would be flagged if executable
// if u.role == "admin" { ... }
fn add(a: i32, b: i32) -> i32 { a + b }
"#;
        let r = analyze_authz(src, "rust");
        assert_eq!(
            r.violations, 0,
            "commented authz smells excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn empty_file_clean() {
        let r = analyze_authz("", "rust");
        assert_eq!(r.violations, 0, "empty file: {:?}", r.findings);
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_authz(
            r#"
fn delete_user(user_id: u64) { db.delete(user_id); }
fn assign_role(user_id: u64, role: &str) { db.set_role(user_id, role); }
fn admin_dashboard() -> &'static str { "admin" }
fn check(u: &User) -> bool { u.role == "admin" }
fn fetch_account(user_id: u64) -> Result<Account, String> { db.find_account(user_id) }
"#,
            "rust",
        );
        let good = analyze_authz(
            r#"
fn authorize(actor: &User, op: &str) -> Result<(), AppError> { Ok(()) }
fn delete_user(user_id: u64, actor: &User) -> Result<(), AppError> {
    authorize(actor, "delete_user")?;
    db.delete(user_id);
    Ok(())
}
"#,
            "rust",
        );
        assert!(
            score_authz(&bad) < score_authz(&good),
            "broken-access file ({:.3}) must score below with-authz ({:.3})",
            score_authz(&bad),
            score_authz(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_authz(
            r#"fn delete_user(user_id: u64) { db.delete(user_id); }
fn assign_role(user_id: u64, role: &str) { db.set_role(user_id, role); }
fn admin_dashboard() -> &'static str { "admin" }
fn fetch_account(user_id: u64) -> Result<Account, String> { db.find_account(user_id) }
fn check(u: &User) -> bool { u.role == "admin" }
"#,
            "rust",
        );
        let s = score_authz(&r);
        assert!(s > 0.0, "broken-access short file must not score 0.0: {s}");
    }
}
