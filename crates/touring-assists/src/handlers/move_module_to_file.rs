//! `move_module_to_file` assist — extract inline module to separate file.

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};
use std::path::PathBuf;
use touring_generator::source_change::{FileSystemEdit, Indel, SourceChange, TextEdit};

/// Stable identifier for the move-module-to-file assist.
pub const MOVE_MODULE_TO_FILE_ID: AssistId = "move_module_to_file";

/// Handler that extracts the selected inline module into its own sibling file.
pub const MOVE_MODULE_TO_FILE: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text().to_string();
    if selected.trim().is_empty() {
        return Some(());
    }

    let file_id = ctx.file_id;
    let range = ctx.range.clone();

    // Extract module name from `mod name { ... }` or `pub mod name { ... }`
    let mod_name = extract_module_name(&selected);
    let filename = mod_name
        .as_deref()
        .map(|n| format!("{}.rs", n))
        .unwrap_or_else(|| "new_module.rs".to_string());
    let mod_stmt = mod_name
        .as_deref()
        .map(|n| format!("mod {};", n))
        .unwrap_or_else(|| "mod new_module;".to_string());

    let label = format!(
        "Move module to file: {}",
        mod_name.as_deref().unwrap_or("new_module")
    );

    let lazy = LazySourceChange::new(move || {
        let text_edit = TextEdit::try_from_iter(vec![Indel {
            delete: range.clone(),
            insert: mod_stmt.clone(),
        }])
        .expect("valid indels");

        SourceChange::new()
            .with_fs_edit(FileSystemEdit::CreateFile {
                path: PathBuf::from(filename.as_str()),
                content: format!("// Extracted from inline module\n{}", selected),
            })
            .with_edit(file_id, text_edit)
    });

    super::add_refactor_assist(assists, ctx, MOVE_MODULE_TO_FILE_ID, label, lazy);

    Some(())
};

/// Extract module name from `mod name { ... }` or `pub mod name { ... }`.
fn extract_module_name(text: &str) -> Option<String> {
    let trimmed = text.trim();
    // Strip `pub ` prefix if present
    let after_pub = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
    // Must start with `mod `
    let after_mod = after_pub.strip_prefix("mod ")?;
    // Get the identifier (stop at whitespace, `{`, or `;`)
    let name_end = after_mod.find(|c: char| c.is_whitespace() || c == '{' || c == ';')?;
    let name = &after_mod[..name_end];
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    // Identifiers can't start with a digit
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_module_to_file_handler_registered() {
        assert_eq!(MOVE_MODULE_TO_FILE_ID, "move_module_to_file");
    }

    #[test]
    fn extract_module_name_basic() {
        assert_eq!(
            extract_module_name("mod foo { bar }").as_deref(),
            Some("foo")
        );
        assert_eq!(extract_module_name("mod bar;").as_deref(), Some("bar"));
        assert_eq!(
            extract_module_name("mod my_module { }").as_deref(),
            Some("my_module")
        );
    }

    #[test]
    fn extract_module_name_pub() {
        assert_eq!(
            extract_module_name("pub mod foo { }").as_deref(),
            Some("foo")
        );
        assert_eq!(extract_module_name("pub mod bar;").as_deref(), Some("bar"));
    }

    #[test]
    fn extract_module_name_invalid() {
        assert_eq!(extract_module_name("mod;"), None);
        assert_eq!(extract_module_name("mod 123 { }").as_deref(), None);
        assert_eq!(extract_module_name("pub mod;").as_deref(), None);
        assert_eq!(extract_module_name("let x = 1;").as_deref(), None);
    }
}
