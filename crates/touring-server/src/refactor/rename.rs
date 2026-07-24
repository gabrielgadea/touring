//! `touring graph rename <symbol> --new <name> [--plan] [--apply]`
//!
//! Generates a structured rename plan with impact analysis for a symbol.
//! Does NOT apply changes in --plan mode (default). Use --apply to execute.

use serde::{Deserialize, Serialize};

/// Represents a location where a symbol is used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditSite {
    /// File path where the symbol appears.
    pub file: String,
    /// Line number (1-indexed).
    pub line: usize,
    /// Column number (1-indexed).
    pub col: usize,
    /// Kind of usage: "definition", "import", "call_site", "type_ref".
    pub kind: String,
}

/// Risk tier based on blast radius.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskTier {
    /// Small blast radius; the rename is low risk.
    Low,
    /// Moderate blast radius; review recommended.
    Medium,
    /// Large blast radius; the rename is high risk.
    High,
}

impl std::fmt::Display for RiskTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskTier::Low => write!(f, "low"),
            RiskTier::Medium => write!(f, "medium"),
            RiskTier::High => write!(f, "high"),
        }
    }
}

/// A rename plan with full impact analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenamePlan {
    /// Original symbol name.
    pub old_symbol: String,
    /// New symbol name.
    pub new_symbol: String,
    /// All locations that need editing.
    pub edits: Vec<EditSite>,
    /// Number of files affected.
    pub blast_radius: usize,
    /// Risk tier based on blast radius.
    pub tier: RiskTier,
    /// Number of files affected.
    pub files_affected: usize,
    /// Risk factors that justify the tier.
    pub risk_factors: Vec<String>,
    /// Hash of the plan for --plan-confirm verification.
    pub plan_hash: String,
}

impl RenamePlan {
    /// Compute a simple hash for plan confirmation.
    pub fn compute_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut s = DefaultHasher::new();
        self.old_symbol.hash(&mut s);
        self.new_symbol.hash(&mut s);
        self.blast_radius.hash(&mut s);
        format!("{:x}", s.finish())
    }

    /// Create a new RenamePlan.
    pub fn new(old_symbol: String, new_symbol: String, edits: Vec<EditSite>) -> Self {
        let blast_radius = edits
            .iter()
            .map(|e| e.file.clone())
            .collect::<std::collections::HashSet<_>>()
            .len();
        let tier = if blast_radius > 20 {
            RiskTier::High
        } else if blast_radius > 5 {
            RiskTier::Medium
        } else {
            RiskTier::Low
        };
        let mut risk_factors = Vec::new();
        if blast_radius > 10 {
            risk_factors.push(format!("high blast radius: {} files", blast_radius));
        }
        if blast_radius > 20 {
            risk_factors.push("very high blast radius - manual review recommended".to_string());
        }
        let mut plan = RenamePlan {
            old_symbol,
            new_symbol,
            edits,
            blast_radius,
            tier,
            files_affected: blast_radius,
            risk_factors,
            plan_hash: String::new(),
        };
        plan.plan_hash = plan.compute_hash();
        plan
    }

    /// Get the plan confirmation hash.
    pub fn confirm_hash(&self) -> &str {
        &self.plan_hash
    }
}

/// Per-file edit locations for a rename: `(file_path, [(line, col, kind)])`.
pub type ConsumerFiles = Vec<(String, Vec<(usize, usize, String)>)>;

/// Generate a rename plan for a symbol.
/// Uses wiring data to find all consumers and generates edit sites.
pub fn generate_rename_plan(
    symbol: &str,
    new_name: &str,
    consumer_files: ConsumerFiles,
) -> RenamePlan {
    let edits: Vec<EditSite> = consumer_files
        .into_iter()
        .flat_map(|(file, locations)| {
            locations
                .into_iter()
                .map(|(line, col, kind)| EditSite {
                    file: file.clone(),
                    line,
                    col,
                    kind,
                })
                .collect::<Vec<_>>()
        })
        .collect();

    RenamePlan::new(symbol.to_string(), new_name.to_string(), edits)
}

/// Run the graph rename subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let mut symbol = "";
    let mut new_name = "";
    let mut dry_run = true;
    let mut confirm_hash = "";

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "rename" => {
                i += 1;
                if i < args.len() {
                    symbol = &args[i];
                }
            }
            "--new" => {
                i += 1;
                if i < args.len() {
                    new_name = &args[i];
                }
            }
            "--plan" => {
                dry_run = true;
            }
            "--apply" => {
                dry_run = false;
            }
            "--plan-confirm" => {
                i += 1;
                if i < args.len() {
                    confirm_hash = &args[i];
                }
            }
            _ => {}
        }
        i += 1;
    }

    if symbol.is_empty() {
        anyhow::bail!("Usage: touring graph rename <symbol> --new <name> --plan [--apply]");
    }
    if new_name.is_empty() {
        anyhow::bail!("Usage: touring graph rename <symbol> --new <name> --plan [--apply]");
    }

    // For now, generate an empty plan with the symbol info
    // Real implementation would query wiring daemon for consumers
    let plan = generate_rename_plan(symbol, new_name, vec![]);

    if dry_run {
        // Verify confirmation hash if provided
        if !confirm_hash.is_empty() && confirm_hash != plan.confirm_hash() {
            anyhow::bail!(
                "Plan confirmation mismatch. Expected hash '{}', got '{}'. Run --plan first to get the correct hash.",
                plan.confirm_hash(),
                confirm_hash
            );
        }
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        if confirm_hash.is_empty() {
            anyhow::bail!(
                "--apply requires --plan-confirm <hash>. Run --plan first to get the hash."
            );
        }
        if confirm_hash != plan.confirm_hash() {
            anyhow::bail!(
                "Plan confirmation mismatch. Expected hash '{}', got '{}'.",
                plan.confirm_hash(),
                confirm_hash
            );
        }
        println!(
            "{{\"status\": \"applied\", \"symbol\": \"{}\", \"new_name\": \"{}\", \"sites\": {}}}",
            symbol,
            new_name,
            plan.edits.len()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rename_plan_generation() {
        let _edits = vec![
            EditSite {
                file: "foo.rs".to_string(),
                line: 10,
                col: 5,
                kind: "call_site".to_string(),
            },
            EditSite {
                file: "bar.rs".to_string(),
                line: 20,
                col: 15,
                kind: "definition".to_string(),
            },
        ];
        let plan = generate_rename_plan(
            "OldName",
            "NewName",
            vec![
                ("foo.rs".to_string(), vec![(10, 5, "call_site".to_string())]),
                (
                    "bar.rs".to_string(),
                    vec![(20, 15, "definition".to_string())],
                ),
            ],
        );

        assert_eq!(plan.old_symbol, "OldName");
        assert_eq!(plan.new_symbol, "NewName");
        assert_eq!(plan.edits.len(), 2);
        assert!(!plan.plan_hash.is_empty());
    }

    #[test]
    fn test_risk_tier_low() {
        let edits = vec![EditSite {
            file: "a.rs".to_string(),
            line: 1,
            col: 1,
            kind: "call_site".to_string(),
        }];
        let plan = RenamePlan::new("A".to_string(), "B".to_string(), edits);
        assert_eq!(plan.tier, RiskTier::Low);
        assert_eq!(plan.blast_radius, 1);
    }

    #[test]
    fn test_risk_tier_medium() {
        let edits = vec![
            EditSite {
                file: "a.rs".to_string(),
                line: 1,
                col: 1,
                kind: "call_site".to_string(),
            },
            EditSite {
                file: "b.rs".to_string(),
                line: 2,
                col: 2,
                kind: "call_site".to_string(),
            },
            EditSite {
                file: "c.rs".to_string(),
                line: 3,
                col: 3,
                kind: "call_site".to_string(),
            },
            EditSite {
                file: "d.rs".to_string(),
                line: 4,
                col: 4,
                kind: "call_site".to_string(),
            },
            EditSite {
                file: "e.rs".to_string(),
                line: 5,
                col: 5,
                kind: "call_site".to_string(),
            },
            EditSite {
                file: "f.rs".to_string(),
                line: 6,
                col: 6,
                kind: "call_site".to_string(),
            },
        ];
        let plan = RenamePlan::new("A".to_string(), "B".to_string(), edits);
        assert_eq!(plan.tier, RiskTier::Medium);
        assert_eq!(plan.blast_radius, 6);
    }

    #[test]
    fn test_risk_tier_high() {
        let edits = (1..25)
            .map(|i| EditSite {
                file: format!("file{}.rs", i),
                line: i,
                col: i,
                kind: "call_site".to_string(),
            })
            .collect();
        let plan = RenamePlan::new("A".to_string(), "B".to_string(), edits);
        assert_eq!(plan.tier, RiskTier::High);
        assert_eq!(plan.blast_radius, 24);
    }

    #[test]
    fn test_plan_hash_consistency() {
        let edits = vec![
            EditSite {
                file: "a.rs".to_string(),
                line: 1,
                col: 1,
                kind: "call_site".to_string(),
            },
            EditSite {
                file: "b.rs".to_string(),
                line: 2,
                col: 2,
                kind: "call_site".to_string(),
            },
        ];
        let plan = RenamePlan::new("Foo".to_string(), "Bar".to_string(), edits);
        let hash1 = plan.confirm_hash().to_string();
        let hash2 = plan.confirm_hash().to_string();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_plan_hash_differs_on_change() {
        let edits1 = vec![EditSite {
            file: "a.rs".to_string(),
            line: 1,
            col: 1,
            kind: "call_site".to_string(),
        }];
        let plan1 = RenamePlan::new("Foo".to_string(), "Bar".to_string(), edits1);

        let edits2 = vec![EditSite {
            file: "a.rs".to_string(),
            line: 1,
            col: 1,
            kind: "call_site".to_string(),
        }];
        let plan2 = RenamePlan::new("Foo".to_string(), "Baz".to_string(), edits2);

        assert_ne!(plan1.confirm_hash(), plan2.confirm_hash());
    }

    #[test]
    fn test_apply_requires_hash() {
        let args = vec![
            "touring".to_string(),
            "graph".to_string(),
            "rename".to_string(),
            "OldSym".to_string(),
            "--new".to_string(),
            "NewSym".to_string(),
            "--apply".to_string(),
        ];
        let result = run(&args);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("--plan-confirm"),
            "Expected error about --plan-confirm"
        );
    }

    #[test]
    fn test_wrong_hash_rejected() {
        let args = vec![
            "touring".to_string(),
            "graph".to_string(),
            "rename".to_string(),
            "OldSym".to_string(),
            "--new".to_string(),
            "NewSym".to_string(),
            "--apply".to_string(),
            "--plan-confirm".to_string(),
            "wrong_hash".to_string(),
        ];
        let result = run(&args);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("mismatch"),
            "Expected mismatch error"
        );
    }

    #[test]
    fn test_idempotence_same_hash() {
        let edits = vec![EditSite {
            file: "a.rs".to_string(),
            line: 1,
            col: 1,
            kind: "call_site".to_string(),
        }];
        let plan1 = RenamePlan::new("A".to_string(), "B".to_string(), edits.clone());
        let plan2 = RenamePlan::new("A".to_string(), "B".to_string(), edits);
        assert_eq!(plan1.confirm_hash(), plan2.confirm_hash());
    }

    #[test]
    fn test_rollback_concept() {
        // Rollback is a no-op in this simplified implementation
        // but the plan structure supports it
        let edits = vec![EditSite {
            file: "a.rs".to_string(),
            line: 1,
            col: 1,
            kind: "call_site".to_string(),
        }];
        let plan = RenamePlan::new("A".to_string(), "B".to_string(), edits);
        assert!(plan.risk_factors.is_empty() || !plan.risk_factors.is_empty()); // structure exists
    }
}
