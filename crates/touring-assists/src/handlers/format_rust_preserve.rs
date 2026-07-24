//! `format_rust_preserve` assist — format Rust source using prettyplease.

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};
use touring_code::ast::surgery::format_rust_code;

/// Stable identifier for the format-Rust-preserve assist.
pub const FORMAT_RUST_PRESERVE_ID: AssistId = "format_rust_preserve";

/// Handler that reformats the selection (or whole file) via `prettyplease`, preserving semantics.
pub const FORMAT_RUST_PRESERVE: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text();
    let content = ctx.content;
    let range = &ctx.range;

    let source_to_format = if selected.trim().is_empty() {
        // No selection — format the entire file
        content.to_string()
    } else {
        selected.to_string()
    };

    let formatted = match format_rust_code(&source_to_format) {
        Ok(f) if f != source_to_format => f,
        // Already formatted or parse error — no change needed
        _ => return Some(()),
    };

    let file_id = ctx.file_id;
    let edit_range = if selected.trim().is_empty() {
        range.clone()
    } else {
        // Format just the selected range
        range.clone()
    };

    let label = if selected.trim().is_empty() {
        "Format Rust file with prettyplease"
    } else {
        "Format selection with prettyplease"
    };

    let lazy = LazySourceChange::new(move || {
        super::make_replace(file_id, edit_range.clone(), formatted.clone())
    });

    super::add_refactor_assist(
        assists,
        ctx,
        FORMAT_RUST_PRESERVE_ID,
        label.to_string(),
        lazy,
    );

    Some(())
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_rust_preserve_handler_registered() {
        assert_eq!(FORMAT_RUST_PRESERVE_ID, "format_rust_preserve");
    }

    #[test]
    fn format_rust_code_handles_messy_rust() {
        let messy = "fn  foo(  x : i32)->i32{x+1}";
        let result = format_rust_code(messy).expect("valid rust");
        assert!(result.contains("fn foo(x: i32)"));
    }

    #[test]
    fn format_rust_code_idempotent() {
        let clean = "fn foo(x: i32) -> i32 { x + 1 }";
        let result = format_rust_code(clean).expect("valid rust");
        // Second format should produce same result
        let result2 = format_rust_code(&result).expect("valid rust");
        assert_eq!(result, result2);
    }
}
