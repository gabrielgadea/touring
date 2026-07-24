//! `touring language list|<lang>` — Tier-based language support disclosure.
//!
//! Subcommands:
//!   list              List all supported languages with tiers and capabilities (default)
//!   `<lang>`           Show detailed capability dump for one language
//!
//! Usage:
//!   `touring language list`
//!   `touring language rust`
//!   `touring language python [--json]`

use anyhow::{Context, Result};
use touring_code::languages::LanguageSupport;

/// Entry point for the `touring language …` subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("list");

    // Handle help flags
    if subcommand == "--help" || subcommand == "-h" {
        println!("touring language — Tier-based language support disclosure");
        println!();
        println!("Usage:");
        println!("  touring language list              List all supported languages with tiers");
        println!(
            "  touring language <lang>             Show detailed capability dump for one language"
        );
        println!("  touring language list --json      JSON output for list");
        println!("  touring language <lang> --json    JSON output for detail");
        println!("  touring language --help, -h        Show this help message");
        println!();
        println!("Languages: rust, typescript, python, go, c, kotlin, swift, java, ruby, php");
        println!(
            "Aliases: ts (typescript), py/python3 (python), golang (go), kt (kotlin), rb (ruby)"
        );
        return Ok(());
    }

    match subcommand {
        "list" => cmd_list(args),
        // Language names — fall through to detail view
        "rust" | "typescript" | "python" | "go" | "c" | "kotlin" | "swift" | "java" | "ruby"
        | "php" => cmd_detail(args, subcommand),
        // Aliases (from_str accepts these)
        "ts" => cmd_detail(args, "typescript"),
        "py" | "python3" => cmd_detail(args, "python"),
        "golang" => cmd_detail(args, "go"),
        "kt" => cmd_detail(args, "kotlin"),
        "rb" => cmd_detail(args, "ruby"),
        _ => {
            anyhow::bail!(
                "Unknown language: '{}'. Use: list, rust, typescript, python, go, c, kotlin, swift, java, ruby, php",
                subcommand
            )
        }
    }
}

// ── Subcommands ───────────────────────────────────────────────────────────────

/// Handle `touring language list` — show all languages with tiers and capabilities.
fn cmd_list(args: &[String]) -> anyhow::Result<()> {
    let all = LanguageSupport::all();
    let json = super::common::has_flag(args, "-j") || super::common::has_flag(args, "--json");

    if json {
        let output: Vec<_> = all
            .iter()
            .map(|s| {
                serde_json::json!({
                    "language": s.language.to_string(),
                    "tier": s.tier.as_u8(),
                    "tier_label": s.tier.label(),
                    "capabilities": s.capabilities.len(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&output).context("failed to serialize language list")?
        );
    } else {
        println!(
            "{:12} {:^8} {:-^50}",
            "Language", "Tier", "Capabilities summary"
        );
        println!("{}", "-".repeat(73));
        for s in &all {
            let caps_count = s
                .capabilities
                .iter()
                .filter(|c| c.level != touring_code::languages::SupportLevel::None)
                .count();
            println!(
                "{:12}  Tier {} ({:15}) {} capabilities",
                s.language,
                s.tier.as_u8(),
                s.tier.label(),
                caps_count
            );
            for c in &s.capabilities {
                if c.level != touring_code::languages::SupportLevel::None {
                    println!(
                        "              {:+15} [{:>8}]  {}",
                        format!("{:?}", c.capability),
                        format!("{:?}", c.level),
                        c.note
                    );
                }
            }
            println!();
        }
    }
    Ok(())
}

/// Handle `touring language <lang>` — detailed capability dump for one language.
fn cmd_detail(args: &[String], lang: &str) -> anyhow::Result<()> {
    let all = LanguageSupport::all();
    let target: Result<touring_code::languages::Language, _> = lang.parse();

    let found = if let Ok(language) = target {
        all.iter().find(|s| s.language == language)
    } else {
        None
    };

    match found {
        Some(s) => {
            let json =
                super::common::has_flag(args, "-j") || super::common::has_flag(args, "--json");

            if json {
                #[derive(serde::Serialize)]
                struct Detail {
                    language: String,
                    tier: u8,
                    tier_label: String,
                    tier_description: String,
                    capabilities: Vec<CapabilityDetail>,
                }
                #[derive(serde::Serialize)]
                struct CapabilityDetail {
                    capability: String,
                    level: String,
                    note: String,
                }

                let detail = Detail {
                    language: s.language.to_string(),
                    tier: s.tier.as_u8(),
                    tier_label: s.tier.label().to_owned(),
                    tier_description: s.tier.description().to_owned(),
                    capabilities: s
                        .capabilities
                        .iter()
                        .map(|c| CapabilityDetail {
                            capability: format!("{:?}", c.capability),
                            level: format!("{:?}", c.level),
                            note: c.note.clone(),
                        })
                        .collect(),
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&detail)
                        .context("failed to serialize language detail")?
                );
            } else {
                println!(
                    "{:12} — Tier {} ({})",
                    s.language,
                    s.tier.as_u8(),
                    s.tier.label()
                );
                println!("Description: {}", s.tier.description());
                println!("\nCapabilities:");
                for c in &s.capabilities {
                    println!(
                        "  {:+15} [{:>8}]  {}",
                        format!("{:?}", c.capability),
                        format!("{:?}", c.level),
                        c.note
                    );
                }
            }
        }
        None => {
            anyhow::bail!(
                "Unknown language: '{}'. Available: {}",
                lang,
                LanguageSupport::all()
                    .iter()
                    .map(|s| s.language.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    // ── Subcommand routing ──────────────────────────────────────────────

    #[test]
    fn default_subcommand_is_list() {
        let args = s(&["touring", "language"]);
        let sub = args.get(2).map(|s| s.as_str()).unwrap_or("list");
        assert_eq!(sub, "list");
    }

    #[test]
    fn explicit_list_subcommand() {
        let args = s(&["touring", "language", "list"]);
        let sub = args.get(2).map(|s| s.as_str()).unwrap_or("list");
        assert_eq!(sub, "list");
    }

    // ── Language detail ─────────────────────────────────────────────────

    #[test]
    fn rust_detail_subcommand() {
        let args = s(&["touring", "language", "rust"]);
        let sub = args.get(2).map(|s| s.as_str()).unwrap_or("list");
        assert_eq!(sub, "rust");
    }

    #[test]
    fn ts_alias_resolves_to_typescript() {
        let args = s(&["touring", "language", "ts"]);
        let sub = args.get(2).map(|s| s.as_str()).unwrap_or("list");
        assert_eq!(sub, "ts");
    }

    #[test]
    fn unknown_language_errors() {
        let args = s(&["touring", "language", "cobol"]);
        let result = run(&args);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("cobol"),
            "error should mention the unknown language"
        );
    }

    #[test]
    fn all_languages_have_tier() {
        for lang in touring_code::languages::Language::all() {
            let tier = lang.tier();
            assert!(
                tier.as_u8() >= 1 && tier.as_u8() <= 4,
                "language {:?} must have valid tier, got {}",
                lang,
                tier.as_u8()
            );
        }
    }

    #[test]
    fn all_supported_languages_are_in_matrix() {
        let matrix = LanguageSupport::all();
        for lang in touring_code::languages::Language::all() {
            assert!(
                matrix.iter().any(|s| s.language == *lang),
                "language {:?} should be in the support matrix",
                lang
            );
        }
    }
}
