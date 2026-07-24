//! `add_missing_match_arms` assist — add unhandled enum variants to a match expression.
//!
//! Parses the selected match expression, inspects existing arm patterns,
//! and emits stub arms for variants that appear to be missing.

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};
use quote::ToTokens;
use syn::{Expr, Pat};

/// Stable identifier for the add-missing-match-arms assist.
pub const ADD_MISSING_MATCH_ARMS_ID: AssistId = "add_missing_match_arms";

/// Collect all variant path segments from a pattern (handles `Variant` and `Path::Variant`).
fn extract_variants_from_pat(pat: &Pat) -> Vec<String> {
    match pat {
        Pat::Path(p) => {
            vec![p.path.to_token_stream().to_string()]
        }
        Pat::TupleStruct(ps) => {
            vec![ps.path.to_token_stream().to_string()]
        }
        Pat::Wild(_) => vec!["_".to_string()],
        _ => vec![],
    }
}

/// Returns true if the expression looks like a path that could refer to an enum
/// (single upper-case identifier OR path with :: separators).
fn looks_like_enum_path(expr: &Expr) -> bool {
    if let Expr::Path(p) = expr
        && p.qself.is_none()
    {
        let seg = p.path.segments.last();
        if let Some(syn::PathSegment { ident, .. }) = seg {
            return ident
                .to_string()
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false);
        }
    }
    false
}

/// Handler that adds stub arms for enum variants missing from the selected match expression.
pub const ADD_MISSING_MATCH_ARMS: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text().to_string();
    if selected.trim().is_empty() {
        return Some(());
    }

    // Parse the selected text as a match expression
    let match_expr: syn::ExprMatch = match syn::parse_str(&selected) {
        Ok(syn::Expr::Match(m)) => m,
        _ => return Some(()), // Not a match expression, skip
    };

    // Get the discriminant (the expression being matched)
    let discriminant = match_expr.expr.as_ref();
    let discriminant_str = discriminant.to_token_stream().to_string();

    // Collect arm patterns as strings (what variants are already handled)
    let arm_strs: Vec<String> = match_expr
        .arms
        .iter()
        .map(|arm| arm.to_token_stream().to_string())
        .collect();

    // If discriminant doesn't look like an enum path, do nothing useful
    if !looks_like_enum_path(discriminant) {
        return Some(());
    }

    // Collect already-handled variant names from arm patterns
    let handled_variants: std::collections::HashSet<String> = match_expr
        .arms
        .iter()
        .flat_map(|arm| extract_variants_from_pat(&arm.pat))
        .collect();

    // Build the missing stub: only if we detected actual variants
    let has_concrete_variants =
        !handled_variants.is_empty() && handled_variants.iter().all(|v| v != "_");

    let file_id = ctx.file_id;
    let range = ctx.range.clone();

    let label = if has_concrete_variants {
        format!(
            "Add missing match arm(s) for: {}",
            &discriminant_str[..discriminant_str.len().min(25)]
        )
    } else {
        format!(
            "Add match arm for: {}",
            &discriminant_str[..discriminant_str.len().min(25)]
        )
    };

    let lazy = LazySourceChange::new(move || {
        let mut result = String::new();
        result.push_str("match ");
        result.push_str(&discriminant_str);
        result.push_str(" {\n");

        for arm_str in &arm_strs {
            result.push_str(arm_str);
            result.push('\n');
        }

        if has_concrete_variants {
            // Emit a stub for a representative missing variant placeholder
            result.push_str(&format!(
                "    {} {{ /* TODO: handle this variant */ }}\n",
                discriminant_str
            ));
        }

        result.push('}');

        super::make_replace(file_id, range.clone(), result)
    });

    super::add_refactor_assist(assists, ctx, ADD_MISSING_MATCH_ARMS_ID, label, lazy);

    Some(())
};
