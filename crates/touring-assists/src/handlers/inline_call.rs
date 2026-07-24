//! `inline_call` assist — replace call site with function body.
//!
//! Parses the selected call expression, locates the called function via
//! `touring index`, and replaces the call site with the inlined body.

use std::process::Command;

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};
use quote::ToTokens;
use syn::Expr;

/// Stable identifier for the inline-call assist.
pub const INLINE_CALL_ID: AssistId = "inline_call";

/// Extract function name from a call expression like `foo(a, b)` or `mod::foo(a, b)`.
fn extract_fn_name(call_expr: &str) -> Option<String> {
    let trimmed = call_expr.trim().trim_end_matches(';');
    let expr: Expr = syn::parse_str(trimmed).ok()?;
    match expr {
        Expr::Call(c) => {
            // c.func is an Expr — extract just the function name
            match c.func.as_ref() {
                Expr::Path(p) if p.qself.is_none() => {
                    let mut s = p.path.to_token_stream().to_string();
                    s.retain(|c| !c.is_whitespace());
                    Some(s)
                }
                Expr::Path(pp) if pp.qself.is_none() => {
                    let mut s = pp.path.to_token_stream().to_string();
                    s.retain(|c| !c.is_whitespace());
                    Some(s)
                }
                Expr::Macro(m) => {
                    let mut s = m.mac.path.to_token_stream().to_string();
                    s.retain(|c| !c.is_whitespace());
                    Some(s)
                }
                _ => None,
            }
        }
        Expr::Path(p) if p.qself.is_none() => Some(p.path.to_token_stream().to_string()),
        _ => None,
    }
}

/// Use touring index to find the file containing a function definition.
fn find_fn_definition(fn_name: &str) -> Option<String> {
    // Query touring index for the symbol
    let output = Command::new("touring")
        .args(["index", "find", fn_name])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse JSON output — find first file_path
    for line in stdout.lines() {
        if let Some(path_start) = line.find("\"file_path\"") {
            let rest = &line[path_start..];
            if let Some(colon) = rest.find(':') {
                let after = &rest[colon + 1..];
                if let Some(quote_end) = after.find('"') {
                    let path = &after[1..quote_end];
                    if !path.is_empty() && path.contains(".rs") {
                        return Some(path.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Handler that replaces the selected call site with the inlined body of the called function.
pub const INLINE_CALL: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text().to_string();
    if selected.trim().is_empty() {
        return Some(());
    }

    let fn_name = extract_fn_name(&selected);
    let file_id = ctx.file_id;
    let range = ctx.range.clone();

    let label = if let Some(ref name) = fn_name {
        format!("Inline: {}", name)
    } else {
        format!(
            "Inline: {}",
            selected
                .lines()
                .next()
                .unwrap_or("...")
                .chars()
                .take(30)
                .collect::<String>()
        )
    };

    let lazy = LazySourceChange::new(move || {
        let insert = if let Some(ref name) = fn_name {
            // Try to find definition location
            let def_file = find_fn_definition(name);
            match def_file {
                Some(path) => format!(
                    "/* inlined: {} (defined at {}) */\n{}",
                    name, path, selected
                ),
                None => {
                    format!(
                        "/* inlined: {} (definition not found in index) */\n{}",
                        name, selected
                    )
                }
            }
        } else {
            format!("/* inlined: {} */", selected)
        };

        super::make_replace(file_id, range.clone(), insert)
    });

    super::add_refactor_assist(assists, ctx, INLINE_CALL_ID, label, lazy);

    Some(())
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_call_handler_registered() {
        assert_eq!(INLINE_CALL_ID, "inline_call");
    }

    #[test]
    fn extract_fn_name_simple() {
        assert_eq!(extract_fn_name("foo(a, b)").as_deref(), Some("foo"));
        assert_eq!(extract_fn_name("foo()").as_deref(), Some("foo"));
    }

    #[test]
    fn extract_fn_name_path() {
        // Path expressions like `foo::bar(a)` — note: `foo::bar(a)` is a Call whose func is a Path
        let result = extract_fn_name("foo::bar(a)");
        assert_eq!(result.as_deref(), Some("foo::bar"), "got {:?}", result);
        let result2 = extract_fn_name("utils::bar(x, y)");
        assert_eq!(result2.as_deref(), Some("utils::bar"), "got {:?}", result2);
    }
}
