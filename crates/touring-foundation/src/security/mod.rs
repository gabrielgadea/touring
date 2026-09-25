// touring-quality:allow-secrets — the tests below embed synthetic-secret
// fixtures on purpose (the code-vs-literal predicate's own corpus), the
// detect-secrets / gitleaks `pragma: allowlist secret` convention.
//! Security analysis foundation types.
//!
//! Provides zero-dependency foundation types for the security analysis layer:
//! - [`vulnerability`] — [`VulnMatch`] and [`VulnerabilityPattern`] trait
//! - [`is_code_expression`] — the shared code-vs-literal predicate for the
//!   secret / misconfiguration detectors (F2.4 secrets, F2.6 config)

pub mod vulnerability;

pub use vulnerability::{VulnMatch, VulnerabilityPattern};

/// True when `v` is CODE, not a literal: a call (`os.environ.get(…)`) or an
/// identifier/attribute path (`status.HTTP_202_ACCEPTED`, with the call's
/// closing paren attached). Such a value is READ, never hardcoded — and only
/// a hardcoded value can be a leaked credential.
///
/// This is THE single source of the code-vs-literal predicate for the secret
/// detectors (25/09/2026, touring-36): F2.4 secrets
/// (`touring-quality::verifications::f2_4_secrets`) and the OWASP A05
/// config analyzer (`touring-analysis::quality::config_security`,
/// `default_or_weak_credentials`) both judge through it — a second copy of
/// the predicate is how the two dims drift into disagreeing about the same
/// line.
///
/// Measured 25/09/2026 (the analise's backend judge, P0 0.000):
/// `@router.post("/forgot-password", status_code=status.HTTP_202_ACCEPTED)`
/// failed F2.4 — the decorator's first `=` is the `status_code=` kwarg, so
/// the split named the LHS from "password" in the ROUTE, and the enum path
/// scored as a high-entropy assigned value; and F2.6 flagged
/// `data: PasswordResetRequest` / `password_hash=password_hash` as CWE-798,
/// the same class (an identifier read as a credential).
///
/// Why detection holds: no real token carries `(`; a `)`-terminated enum
/// path is never one either; and a DOTTED value that IS a credential (a
/// JWT) still lands on the detectors' structural-token paths, which read
/// the raw line BEFORE this check runs.
pub fn is_code_expression(v: &str) -> bool {
    if v.contains('(') {
        return true;
    }
    let core = v.trim_end_matches(')');
    core.contains('.')
        && core
            .split('.')
            .all(|seg| !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_expressions_are_code_and_literals_are_not() {
        // The measured false-positive forms (touring-36, 25/09/2026).
        assert!(is_code_expression("status.HTTP_202_ACCEPTED)"));
        assert!(is_code_expression("os.environ.get(\"API_TOKEN\")"));
        assert!(is_code_expression("settings.ADMIN"));
        // Bare identifiers are NOT code expressions: a bare high-entropy token
        // (an unquoted API key in a `.env`) must stay judgeable as a value.
        assert!(!is_code_expression("aB3xK9mQ7zR2wY5tE8uI1oP4xC2vB"));
        assert!(!is_code_expression("\"admin\""));
        assert!(!is_code_expression("0123456789abcdef0123456789abcdef"));
    }
}
