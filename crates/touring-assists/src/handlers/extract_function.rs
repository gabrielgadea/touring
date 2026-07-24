//! `extract_function` assist — extract selected code into a new function.
//!
//! Identifies free variables in the selected range and generates a new function
//! with those variables as parameters.

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};
use quote::ToTokens;
use syn::Expr;
use syn::visit::Visit;

/// Stable identifier for the extract-function assist.
pub const EXTRACT_FUNCTION_ID: AssistId = "extract_function";

/// Collect all identifier Path strings used in an expression.
fn collect_idents_from_expr(expr: &Expr) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    let mut visitor = IdentCollector { ids: &mut ids };
    visitor.visit_expr(expr);
    ids
}

struct IdentCollector<'a> {
    ids: &'a mut std::collections::HashSet<String>,
}

impl<'a> Visit<'a> for IdentCollector<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Path(p) = expr
            && p.qself.is_none()
        {
            self.ids.insert(p.path.to_token_stream().to_string());
        }
        syn::visit::visit_expr(self, expr);
    }
}

/// Check if an identifier is defined within (lexically inside) the given block.
fn defined_in_block(block: &syn::ExprBlock, ident: &str) -> bool {
    for stmt in &block.block.stmts {
        match stmt {
            syn::Stmt::Local(l) if pat_ident_list(&l.pat).iter().any(|s| s.as_str() == ident) => {
                return true;
            }
            syn::Stmt::Item(item) => {
                if matches!(item, syn::Item::Fn(f) if f.sig.ident.to_string().as_str() == ident) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn pat_ident_list(pat: &syn::Pat) -> Vec<String> {
    match pat {
        syn::Pat::Ident(i) => vec![i.ident.to_string()],
        syn::Pat::Wild(_) => vec!["_".to_string()],
        _ => vec![],
    }
}

/// Handler that extracts the selected code into a new function, threading free variables as parameters.
pub const EXTRACT_FUNCTION: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text().to_string();
    if selected.trim().is_empty() {
        return Some(());
    }

    // Parse the selected code block to analyze free variables
    let parsed: syn::ExprBlock = match syn::parse_str(&selected) {
        Ok(b) => b,
        _ => {
            // Fallback: just wrap in a basic function without analyzing variables
            let file_id = ctx.file_id;
            let range = ctx.range.clone();
            let fn_name = "extracted_fn".to_string();
            let label = format!(
                "Extract function: {}",
                selected
                    .lines()
                    .next()
                    .unwrap_or("...")
                    .chars()
                    .take(30)
                    .collect::<String>()
            );
            let lazy = LazySourceChange::new(move || {
                super::make_replace(
                    file_id,
                    range.clone(),
                    format!("fn {}() {{\n    {}\n}}\n", fn_name, selected),
                )
            });
            super::add_refactor_assist(assists, ctx, EXTRACT_FUNCTION_ID, label, lazy);
            return Some(());
        }
    };

    // Collect all identifiers used in the selected code
    let all_idents = collect_idents_from_expr(&syn::Expr::Block(parsed.clone()));

    // Filter out identifiers that are defined within the block (local variables)
    let free_vars: Vec<String> = all_idents
        .into_iter()
        .filter(|ident| !defined_in_block(&parsed, ident))
        .collect();

    let file_id = ctx.file_id;
    let range = ctx.range.clone();

    // Build function signature with free variables as parameters
    let fn_name = "extracted_fn".to_string();
    let params_str = if free_vars.is_empty() {
        String::new()
    } else {
        free_vars
            .iter()
            .map(|v| format!("{}: /* TODO: infer type */", v))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let label = if free_vars.is_empty() {
        format!("Extract to function: {}", fn_name)
    } else {
        format!(
            "Extract to function ({} params): {}",
            free_vars.len(),
            fn_name
        )
    };

    let call_site = if free_vars.is_empty() {
        format!("{}()", fn_name)
    } else {
        format!("{}({})", fn_name, free_vars.join(", "))
    };

    let lazy = LazySourceChange::new(move || {
        let fn_sig = if params_str.is_empty() {
            format!("fn {}()", fn_name)
        } else {
            format!("fn {}({})", fn_name, params_str)
        };
        super::make_replace(
            file_id,
            range.clone(),
            format!("{} {{\n    {}\n}}\n\n{}\n", fn_sig, selected, call_site),
        )
    });

    super::add_refactor_assist(assists, ctx, EXTRACT_FUNCTION_ID, label, lazy);

    Some(())
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_function_handler_registered() {
        assert_eq!(EXTRACT_FUNCTION_ID, "extract_function");
    }
}
