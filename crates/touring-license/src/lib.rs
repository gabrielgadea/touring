//! touring-license — Tier model + license parsing skeleton (W14.1 + W14.2 partial).
//!
//! Provides the runtime [`Tier`] enum (Free / Standard / Premium / Enterprise)
//! used to gate paid capabilities, plus a [`License`] struct that can be
//! parsed from a JSON-encoded token. The shape mirrors a JWT payload so that
//! adding signature verification later (when `jsonwebtoken` + `ed25519-dalek`
//! enter the workspace deps) is a drop-in extension behind the `jwt-verify`
//! feature.
//!
//! # Feature precedence (additive)
//!
//! ```text
//! tier-free       — always on (default)
//! tier-standard   — implies tier-free
//! tier-premium    — implies tier-standard
//! tier-enterprise — implies tier-premium (highest)
//! ```
//!
//! A consumer crate writes `cfg!(feature = "tier-premium")` to gate behaviour
//! at the binary's *maximum* tier. Runtime tier (from the [`License`]) is then
//! compared with [`Tier::at_least`].
//!
//! # 30-day offline grace
//!
//! [`License::is_valid_at`] accepts a window past `expires_at` to support
//! offline use after subscription end. Default grace: 30 days.
//!
//! # Cryptographic verification
//!
//! NOT YET IMPLEMENTED. The `parse_unverified` constructor returns the parsed
//! payload **without checking the signature**. Production callers should
//! refuse to enable `tier-premium` / `tier-enterprise` features unless a
//! `parse_verified` (future, behind `jwt-verify`) returns Ok. Adding
//! `jsonwebtoken` + `ed25519-dalek` to the workspace is W14.2 follow-up.

#![deny(missing_docs)]
// Lint-ceiling ratchet (RBP-01, 2026-06-13): unwrap-free in production; deny it
// so license/entitlement checks can never panic (they gate paid features).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

/// Default offline grace window (in seconds) past `License::expires_at`.
/// 30 days = 2_592_000 seconds.
pub const DEFAULT_GRACE_SECONDS: i64 = 30 * 24 * 60 * 60;

/// Commercial tier of a Touring deployment. Ordered Free < Standard < Premium
/// < Enterprise — `at_least` is monotonic up the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Free tier — the always-on baseline. Everyone gets this.
    Free,
    /// Standard tier — first paid level.
    Standard,
    /// Premium tier — adds advanced capabilities (telemetry off, etc.).
    Premium,
    /// Enterprise tier — adds SSO, audit log SIEM export, private registry.
    Enterprise,
}

impl Tier {
    /// Returns `true` if `self` is at least `other` (monotonic up the chain).
    /// `Enterprise.at_least(Premium) == true`, `Free.at_least(Premium) == false`.
    pub fn at_least(self, other: Tier) -> bool {
        self >= other
    }

    /// Parses a string token into a [`Tier`]. Case-insensitive. Unknown
    /// strings return [`Tier::Free`] (fail-safe — never escalate without
    /// explicit license).
    pub fn parse_or_free(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "enterprise" => Self::Enterprise,
            "premium" => Self::Premium,
            "standard" => Self::Standard,
            _ => Self::Free,
        }
    }

    /// String representation (lowercase).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Standard => "standard",
            Self::Premium => "premium",
            Self::Enterprise => "enterprise",
        }
    }

    /// W14.3 — Telemetry policy by tier.
    ///
    /// Per the W14 spec ("telemetria free/std ON, premium/ent OFF"):
    /// Free + Standard tiers send anonymised usage telemetry; Premium +
    /// Enterprise tiers do NOT (premium customers pay for privacy).
    ///
    /// Consumer crates (touring-telemetry, touring-server) should call this
    /// at startup with the runtime [`License::effective_tier`] and skip
    /// telemetry init when `false`.
    pub fn telemetry_enabled(self) -> bool {
        matches!(self, Tier::Free | Tier::Standard)
    }

    /// W14.4 partial — private registry opt-in policy.
    ///
    /// Only Enterprise tier may point at a private (non-crates.io) registry.
    /// Used by future `touring toolchain install` URL logic to decide whether
    /// to honour a `--registry` flag pointing at an internal artifact server.
    pub fn private_registry_allowed(self) -> bool {
        self == Tier::Enterprise
    }

    /// W14.6 partial — audit-log SIEM export opt-in policy.
    ///
    /// Premium and Enterprise tiers may export audit logs to SIEM systems
    /// (Splunk, Datadog, ELK). Free and Standard tiers do not have this hook.
    pub fn siem_export_allowed(self) -> bool {
        self >= Tier::Premium
    }

    /// W14.5 partial — SSO scaffold opt-in policy.
    ///
    /// Only Enterprise tier may configure OAuth2 SSO providers (Okta,
    /// Google, GitHub). Free/Standard/Premium use local auth (or no auth).
    pub fn sso_allowed(self) -> bool {
        self == Tier::Enterprise
    }
}

/// License payload parsed from a JSON token. Mirrors JWT claim shape so that
/// future cryptographic verification (W14.2 follow-up) is a drop-in extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    /// Subject — typically the licensee organisation name.
    pub sub: String,
    /// Granted tier.
    pub tier: Tier,
    /// Unix timestamp (seconds) of issuance.
    pub iat: i64,
    /// Unix timestamp (seconds) of expiry. After this + grace, the license
    /// MUST be refused.
    pub exp: i64,
}

impl License {
    /// Parse a JSON-encoded license payload **without verifying any
    /// signature**. Returns the parsed [`License`] on success.
    ///
    /// Production callers MUST not trust this for tier escalation — it is
    /// the responsibility of the `jwt-verify` feature (W14.2 follow-up) to
    /// add ed25519 signature verification.
    pub fn parse_unverified(json: &str) -> Result<Self> {
        serde_json::from_str::<Self>(json).map_err(|e| anyhow!("license payload parse failed: {e}"))
    }

    /// Returns `true` if the license is valid at the given unix timestamp,
    /// accepting `grace_seconds` past `exp`. Default grace: 30 days
    /// (see [`DEFAULT_GRACE_SECONDS`]).
    pub fn is_valid_at(&self, now_unix: i64, grace_seconds: i64) -> bool {
        now_unix >= self.iat && now_unix <= self.exp.saturating_add(grace_seconds)
    }

    /// Returns the effective tier: the granted [`Tier`] if the license is
    /// still valid, otherwise [`Tier::Free`] (fail-safe — never honour an
    /// expired license at higher tier).
    pub fn effective_tier(&self, now_unix: i64, grace_seconds: i64) -> Tier {
        if self.is_valid_at(now_unix, grace_seconds) {
            self.tier
        } else {
            Tier::Free
        }
    }
}

/// Compile-time tier ceiling derived from Cargo features. Used by consumers
/// to know the **maximum tier this binary could honour** (the actual runtime
/// tier still depends on the [`License`]).
pub fn binary_max_tier() -> Tier {
    if cfg!(feature = "tier-enterprise") {
        Tier::Enterprise
    } else if cfg!(feature = "tier-premium") {
        Tier::Premium
    } else if cfg!(feature = "tier-standard") {
        Tier::Standard
    } else {
        Tier::Free
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_ordering_is_monotonic() {
        assert!(Tier::Enterprise > Tier::Premium);
        assert!(Tier::Premium > Tier::Standard);
        assert!(Tier::Standard > Tier::Free);
    }

    #[test]
    fn at_least_is_monotonic() {
        assert!(Tier::Enterprise.at_least(Tier::Free));
        assert!(Tier::Enterprise.at_least(Tier::Premium));
        assert!(!Tier::Free.at_least(Tier::Premium));
        assert!(!Tier::Standard.at_least(Tier::Premium));
        assert!(Tier::Premium.at_least(Tier::Premium));
    }

    #[test]
    fn parse_or_free_recognizes_known() {
        assert_eq!(Tier::parse_or_free("free"), Tier::Free);
        assert_eq!(Tier::parse_or_free("STANDARD"), Tier::Standard);
        assert_eq!(Tier::parse_or_free("  premium  "), Tier::Premium);
        assert_eq!(Tier::parse_or_free("Enterprise"), Tier::Enterprise);
    }

    #[test]
    fn parse_or_free_falls_back_on_unknown() {
        // Fail-safe: never escalate from unknown input
        assert_eq!(Tier::parse_or_free("god-mode"), Tier::Free);
        assert_eq!(Tier::parse_or_free(""), Tier::Free);
        assert_eq!(Tier::parse_or_free("PREMIUM_PLUS"), Tier::Free);
    }

    #[test]
    fn license_parse_unverified_valid_json() {
        let json = r#"{"sub":"acme.inc","tier":"premium","iat":1000,"exp":5000}"#;
        let lic = License::parse_unverified(json).expect("parse");
        assert_eq!(lic.sub, "acme.inc");
        assert_eq!(lic.tier, Tier::Premium);
        assert_eq!(lic.iat, 1000);
        assert_eq!(lic.exp, 5000);
    }

    #[test]
    fn license_parse_unverified_malformed_errors() {
        assert!(License::parse_unverified("not-json").is_err());
        assert!(License::parse_unverified("{}").is_err());
        assert!(License::parse_unverified(r#"{"sub":"x"}"#).is_err());
    }

    #[test]
    fn is_valid_at_within_window() {
        let lic = License {
            sub: "x".into(),
            tier: Tier::Premium,
            iat: 1000,
            exp: 5000,
        };
        assert!(lic.is_valid_at(1000, 0));
        assert!(lic.is_valid_at(3000, 0));
        assert!(lic.is_valid_at(5000, 0));
        assert!(!lic.is_valid_at(5001, 0));
        assert!(!lic.is_valid_at(999, 0));
    }

    #[test]
    fn is_valid_at_honours_grace() {
        let lic = License {
            sub: "x".into(),
            tier: Tier::Premium,
            iat: 1000,
            exp: 5000,
        };
        // 30-day grace = 2_592_000 sec
        assert!(lic.is_valid_at(5000 + 2_592_000, DEFAULT_GRACE_SECONDS));
        assert!(!lic.is_valid_at(5000 + 2_592_001, DEFAULT_GRACE_SECONDS));
    }

    #[test]
    fn effective_tier_falls_to_free_on_expired() {
        let lic = License {
            sub: "x".into(),
            tier: Tier::Enterprise,
            iat: 1000,
            exp: 5000,
        };
        assert_eq!(lic.effective_tier(3000, 0), Tier::Enterprise);
        // Outside grace → Free
        assert_eq!(lic.effective_tier(10_000, 0), Tier::Free);
        // Inside default 30-day grace → still Enterprise
        assert_eq!(
            lic.effective_tier(5000 + 1000, DEFAULT_GRACE_SECONDS),
            Tier::Enterprise
        );
    }

    #[test]
    fn binary_max_tier_default_is_free() {
        // Default features only enable tier-free
        assert_eq!(binary_max_tier(), Tier::Free);
    }

    #[test]
    fn tier_as_str_round_trip() {
        for t in [Tier::Free, Tier::Standard, Tier::Premium, Tier::Enterprise] {
            assert_eq!(Tier::parse_or_free(t.as_str()), t);
        }
    }

    // ── W14.3-W14.6 partial — tier-based policy fns ───────────────────────

    #[test]
    fn telemetry_enabled_free_and_standard_only() {
        assert!(Tier::Free.telemetry_enabled());
        assert!(Tier::Standard.telemetry_enabled());
        assert!(!Tier::Premium.telemetry_enabled());
        assert!(!Tier::Enterprise.telemetry_enabled());
    }

    #[test]
    fn private_registry_enterprise_only() {
        assert!(!Tier::Free.private_registry_allowed());
        assert!(!Tier::Standard.private_registry_allowed());
        assert!(!Tier::Premium.private_registry_allowed());
        assert!(Tier::Enterprise.private_registry_allowed());
    }

    #[test]
    fn siem_export_premium_and_above() {
        assert!(!Tier::Free.siem_export_allowed());
        assert!(!Tier::Standard.siem_export_allowed());
        assert!(Tier::Premium.siem_export_allowed());
        assert!(Tier::Enterprise.siem_export_allowed());
    }

    #[test]
    fn sso_enterprise_only() {
        assert!(!Tier::Free.sso_allowed());
        assert!(!Tier::Standard.sso_allowed());
        assert!(!Tier::Premium.sso_allowed());
        assert!(Tier::Enterprise.sso_allowed());
    }

    #[test]
    fn policy_consistency_with_at_least() {
        // siem_export_allowed should equal at_least(Premium)
        for t in [Tier::Free, Tier::Standard, Tier::Premium, Tier::Enterprise] {
            assert_eq!(t.siem_export_allowed(), t.at_least(Tier::Premium));
        }
    }
}
