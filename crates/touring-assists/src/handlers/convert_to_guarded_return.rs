//! `convert_to_guarded_return` assist — convert if-else with body to early return pattern.
//!
//! Rewrites:  if CONDITION { BODY }  →  if !CONDITION { return; } BODY
//! i.e., moves the body after a negated guard that returns early when the condition is FALSE.

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};
use quote::ToTokens;

/// Stable identifier for the convert-to-guarded-return assist.
pub const CONVERT_TO_GUARDED_RETURN_ID: AssistId = "convert_to_guarded_return";

/// Negate a condition string by wrapping it in `!()` if it looks like a simple expression.
fn negate_cond(cond: &str) -> String {
    let trimmed = cond.trim();
    // If it's already negated, strip the ! to avoid double-negation
    if let Some(stripped) = trimmed.strip_prefix('!') {
        return stripped.trim().to_string();
    }
    // Otherwise wrap in `!(...)`
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        // Already parenthesised: just prefix with !
        format!("!{}", trimmed)
    } else {
        format!("!({})", trimmed)
    }
}

/// Handler that rewrites a selected `if cond { body }` into an early-return guard followed by the body.
pub const CONVERT_TO_GUARDED_RETURN: AssistHandler =
    |assists: &mut Assists, ctx: &AssistContext| {
        let selected = ctx.selected_text().to_string();
        if selected.trim().is_empty() {
            return Some(());
        }

        // Parse the selected text as an if expression
        let if_expr: syn::ExprIf = match syn::parse_str(&selected) {
            Ok(syn::Expr::If(i)) => i,
            _ => return Some(()),
        };

        // Check there is no else branch
        if if_expr.else_branch.is_some() {
            return Some(()); // Already has an else, skip
        }

        // Get the condition and body as strings (capture in closure)
        let cond_str = if_expr.cond.as_ref().to_token_stream().to_string();
        let body_stmts: Vec<String> = if_expr
            .then_branch
            .stmts
            .iter()
            .map(|s| s.to_token_stream().to_string())
            .collect();

        let file_id = ctx.file_id;
        let range = ctx.range.clone();

        let label = format!(
            "Convert to guarded return: {}",
            &cond_str[..cond_str.len().min(25)]
        );

        let lazy = LazySourceChange::new(move || {
            let negated = negate_cond(&cond_str);
            let body = body_stmts.join("\n");

            let replacement = if body_stmts.len() <= 2 {
                // Inline single/lines
                format!("if {} {{\n    return;\n}}\n{}", negated, body)
            } else {
                // Multi-line: keep body on its own lines
                format!("if {} {{\n    return;\n}}\n{}\n", negated, body)
            };

            super::make_replace(file_id, range.clone(), replacement)
        });

        super::add_refactor_assist(assists, ctx, CONVERT_TO_GUARDED_RETURN_ID, label, lazy);

        Some(())
    };
