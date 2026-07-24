//! CLI commands for the rule engine.

use super::{Fix, Result, RuleEngine, RuleSet};
use std::path::PathBuf;
use tracing::info;

/// List all rules from a YAML ruleset file.
pub fn list_rules(ruleset_path: &PathBuf) -> Result<()> {
    let ruleset = RuleSet::load_from_file(ruleset_path)?;
    for rule in &ruleset.rules {
        let langs = if rule.languages.is_empty() {
            "all".to_string()
        } else {
            rule.languages.join(", ")
        };
        println!(
            "{} [{}] ({:?}): {}",
            rule.name,
            langs,
            rule.severity,
            rule.description.as_deref().unwrap_or("—")
        );
        if let Some(ref p) = rule.path {
            println!("  path: {}", p);
        }
        println!("  pattern: {}", rule.pattern);
        if let Some(ref fix) = rule.fix {
            println!("  fix: {}", fix);
        }
    }
    Ok(())
}

/// Check files against a ruleset (dry-run — list fixes without applying).
pub fn check(ruleset_path: &PathBuf, files: &[PathBuf]) -> Result<()> {
    let engine = RuleEngine::load_rules(ruleset_path)?;
    let fixes = engine.apply_rules(files);
    print_fixes(&fixes);
    Ok(())
}

/// Apply fixes from a ruleset to files.
pub fn apply(ruleset_path: &PathBuf, files: &[PathBuf], dry_run: bool) -> Result<()> {
    let engine = RuleEngine::load_rules(ruleset_path)?;
    let fixes = engine.apply_rules(files);

    if fixes.is_empty() {
        info!("No fixes found.");
        return Ok(());
    }

    if dry_run {
        print_fixes(&fixes);
        println!("\nDry run — no changes written.");
    } else {
        apply_fixes(&fixes)?;
        println!("Applied {} fix(es).", fixes.len());
    }
    Ok(())
}

/// Print fixes in a human-readable format.
fn print_fixes(fixes: &[Fix]) {
    for fix in fixes {
        println!(
            "{}:{}:{} — {} → {} [{}]",
            fix.file_path.display(),
            fix.line,
            fix.column,
            fix.original,
            fix.replacement,
            fix.rule_name
        );
    }
}

/// Write fixes back to files.
fn apply_fixes(fixes: &[Fix]) -> Result<()> {
    // Group fixes by file
    use std::collections::HashMap;
    let mut by_file: HashMap<PathBuf, Vec<&Fix>> = HashMap::new();
    for fix in fixes {
        by_file.entry(fix.file_path.clone()).or_default().push(fix);
    }

    for (path, file_fixes) in by_file {
        let content = std::fs::read_to_string(&path)?;
        let mut new_content = content.to_string();

        // Apply in reverse order to preserve line numbers
        for fix in file_fixes.iter().rev() {
            if let Some(pos) = new_content.find(&fix.original) {
                new_content = format!(
                    "{}{}{}",
                    &new_content[..pos],
                    fix.replacement,
                    &new_content[pos + fix.original.len()..]
                );
            }
        }

        std::fs::write(&path, new_content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_fixes_empty() {
        let fixes: Vec<Fix> = vec![];
        print_fixes(&fixes);
        // Should not panic
    }
}
