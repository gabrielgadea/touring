//! AUTO-GENERATED — W14 JWT license validation integration tests.
//!
//! Each test case corresponds to data/w14-license-test-cases.json.
//! Signature placeholders MUST be replaced with real ed25519 sigs
//! signed by the test private key during W14.4-W14.7 implementation.

use touring_bindings::license::{LicenseError, validate_license};

#[test]
fn test_valid_premium() {
    let jwt = include_str!("./valid_premium.jwt");
    let result = validate_license(jwt, "premium");
    // Expected outcome: ACCEPT
    // TODO: assert!(matches!(result, ...));
}

#[test]
fn test_expired_premium() {
    let jwt = include_str!("./expired_premium.jwt");
    let result = validate_license(jwt, "premium");
    // Expected outcome: REJECT_EXPIRED
    // TODO: assert!(matches!(result, ...));
}

#[test]
fn test_expired_within_grace() {
    let jwt = include_str!("./expired_within_grace.jwt");
    let result = validate_license(jwt, "premium");
    // Expected outcome: ACCEPT_GRACE
    // TODO: assert!(matches!(result, ...));
}

#[test]
fn test_tier_mismatch() {
    let jwt = include_str!("./tier_mismatch.jwt");
    let result = validate_license(jwt, "premium");
    // Expected outcome: REJECT_TIER_MISMATCH
    // TODO: assert!(matches!(result, ...));
}

#[test]
fn test_not_yet_valid() {
    let jwt = include_str!("./not_yet_valid.jwt");
    let result = validate_license(jwt, "premium");
    // Expected outcome: REJECT_NOT_YET_VALID
    // TODO: assert!(matches!(result, ...));
}

#[test]
fn test_wrong_issuer() {
    let jwt = include_str!("./wrong_issuer.jwt");
    let result = validate_license(jwt, "premium");
    // Expected outcome: REJECT_ISSUER
    // TODO: assert!(matches!(result, ...));
}
