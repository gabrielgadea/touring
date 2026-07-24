//! `touring license` — F5/G3 (2026-07-24) — license & tier visibility.
//!
//! First real consumer of the `touring-license` crate (W14.1/W14.2): exposes
//! the tier model read-only. NO feature is gated by tier yet — enforcement is
//! future commercial policy (Gabriel's call); this command makes the substrate
//! visible and testable before any gating exists.
//!
//! ```text
//! touring license status [-j]
//! ```
//!
//! License file resolution: `$TOURING_LICENSE_FILE` > `~/.touring/license.json`.
//! Absent file = unlicensed (effective tier Free) — never an error.

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use touring_license::{DEFAULT_GRACE_SECONDS, License, Tier, binary_max_tier};

const USAGE: &str = "touring license — License & tier visibility (read-only)

USAGE:
    touring license status [-j|--json]

BEHAVIOR:
    Shows the binary's compiled tier cap, the license file (if any) and the
    effective tier (expiry + grace honored). License file resolution:
    $TOURING_LICENSE_FILE > ~/.touring/license.json. No feature is gated by
    tier in this build — enforcement is future commercial policy.";

/// CLI dispatch entry. Called from `cli::command_table`.
pub fn run(args: &[String]) -> Result<()> {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("");
    let json = args.iter().skip(2).any(|a| a == "--json" || a == "-j");
    match sub {
        "status" => status(json),
        "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => {
            println!("{USAGE}");
            Err(anyhow!("missing or unknown subcommand"))
        }
    }
}

/// Resolve the license file path: env override > `~/.touring/license.json`.
fn license_file() -> PathBuf {
    std::env::var("TOURING_LICENSE_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            Path::new(&home).join(".touring").join("license.json")
        })
}

/// The `status` subcommand body.
fn status(json: bool) -> Result<()> {
    let cap = binary_max_tier();
    let path = license_file();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Absent = unlicensed (Free); malformed = loud (a real file that cannot
    // be read is an operator error, not a silent downgrade).
    let license = match std::fs::read_to_string(&path) {
        Ok(text) => Some(License::parse_unverified(&text).map_err(|e| {
            anyhow!("license file {} is malformed: {e}", path.display())
        })?),
        Err(_) => None,
    };
    let effective = license
        .as_ref()
        .map(|l| l.effective_tier(now, DEFAULT_GRACE_SECONDS))
        .unwrap_or(Tier::Free);

    if json {
        let out = serde_json::json!({
            "binary_max_tier": cap.as_str(),
            "license_file": path.display().to_string(),
            "license_present": license.is_some(),
            "license": license.as_ref().map(|l| serde_json::json!({
                "sub": l.sub, "tier": l.tier.as_str(), "iat": l.iat, "exp": l.exp,
                "valid_now": l.is_valid_at(now, DEFAULT_GRACE_SECONDS),
            })),
            "effective_tier": effective.as_str(),
            "enforcement": "none (tier gating is future commercial policy)",
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("touring license status");
        println!("  binary tier cap : {}", cap.as_str());
        println!("  license file    : {} ({})", path.display(),
            if license.is_some() { "present" } else { "absent" });
        if let Some(l) = &license {
            println!("  licensed tier   : {} (sub={}, valid_now={})",
                l.tier.as_str(), l.sub, l.is_valid_at(now, DEFAULT_GRACE_SECONDS));
        }
        println!("  effective tier  : {}", effective.as_str());
        println!("  enforcement     : none (tier gating is future commercial policy)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_cap_is_enterprise_in_official_build() {
        // Cargo.toml activates tier-enterprise for the official binary.
        assert_eq!(binary_max_tier(), Tier::Enterprise);
    }

    #[test]
    fn effective_tier_is_free_without_license() {
        // The unlicensed path: absent file → Free, never an error.
        let l: Option<License> = None;
        let effective = l
            .as_ref()
            .map(|lic| lic.effective_tier(0, DEFAULT_GRACE_SECONDS))
            .unwrap_or(Tier::Free);
        assert_eq!(effective, Tier::Free);
    }

    #[test]
    fn valid_license_json_yields_its_tier() {
        let json = r#"{"sub":"gabriel","tier":"premium","iat":0,"exp":32503680000}"#;
        let lic = License::parse_unverified(json).expect("parse");
        assert_eq!(lic.effective_tier(1_000, DEFAULT_GRACE_SECONDS), Tier::Premium);
    }
}
