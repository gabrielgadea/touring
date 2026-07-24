//! Post-Edit RuleEngine Bridge — Wires classification rules into the post-edit hook.
//!
//! ## Purpose
//! This module bridges the gap between the post-edit hook and the `RuleEngine`
//! from `touring-definitions`. It consults the rule engine after edits to classify
//! symbol changes and feeds matches into the cascade queue for predictive routing.
//!
//! ## Design Principles
//! 1. **Fire-and-forget**: Bridge call is fallible — never blocks Claude Code
//! 2. **Exit 0 invariant**: Preserves the hook exit guarantee
//! 3. **RuleEngine lookup**: Uses `RuleEngine::find_rule(name, path)` to match
//!    symbol names + file paths against the embedded `universal_rules.json` ruleset
//! 4. **Cascade integration**: When a rule matches, creates a `SubtaskProposal`
//!    and pushes it to `runtime.ctx.cascade_queue`

use crate::runtime::HookRuntime;
use std::path::Path;
use touring_code::ast::api_cascade::{CascadePlan, Severity, SubtaskProposal};
use touring_code::ast::rust_semantic::ApiChangeKind;
use touring_foundation::semantic::rules::RuleEngine;

/// Bridge: `post_edit` hook → RuleEngine classification → cascade queue.
///
// Análise:
//     - Called from `post_edit.rs` after quality assessment
//     - Looks up the edited symbol in RuleEngine
//     - If a rule matches, creates a SubtaskProposal and pushes to cascade queue
//     - Returns `Ok(String)` with outcome message
//     - Fire-and-forget: never blocks on errors
pub fn bridge_post_edit_rule_engine(
    runtime: &mut HookRuntime,
    symbol: &str,
    file_path: &str,
) -> Result<String, String> {
    let re = RuleEngine::new();
    if let Some(rule) = re.find_rule(symbol, file_path) {
        // Map rule.class to ApiChangeKind — Added for new classifications,
        // Modified for existing ones. Using Added as default since RuleEngine
        // primarily classifies new symbols.
        let kind = ApiChangeKind::Added;

        let proposal = SubtaskProposal {
            api_item: format!("{} '{}'", rule.class, symbol),
            symbol: symbol.to_string(),
            kind,
            callers: vec![],
            reason: format!(
                "classified by RuleEngine as {} (confidence={}, priority={})",
                rule.class, rule.confidence, rule.priority
            ),
            severity: Severity::High,
        };

        let plan = CascadePlan {
            proposals: vec![proposal],
        };

        runtime.ctx.cascade_queue.push(Path::new(file_path), &plan);

        tracing::debug!(
            symbol = %symbol,
            file = %file_path,
            class = %rule.class,
            "bridge_post_edit_rule_engine: rule matched and queued"
        );
        Ok(format!("rule matched: {}", rule.class))
    } else {
        tracing::debug!(
            symbol = %symbol,
            file = %file_path,
            "bridge_post_edit_rule_engine: no matching rule"
        );
        Ok("no matching rule".to_string())
    }
}
