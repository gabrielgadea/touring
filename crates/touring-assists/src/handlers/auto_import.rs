//! `auto_import` assist — insert use statement for unresolved symbol.

use crate::{AssistContext, AssistHandler, AssistId, Assists, LazySourceChange};

/// Stable identifier for the auto-import assist.
pub const AUTO_IMPORT_ID: AssistId = "auto_import";

/// Handler that inserts a `use` statement for the selected unresolved symbol.
pub const AUTO_IMPORT: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text().to_string();
    if selected.trim().is_empty() {
        return Some(());
    }

    let file_id = ctx.file_id;
    let range = ctx.range.clone();
    let label = format!("Add import: {}", selected);
    let lazy = LazySourceChange::new(move || {
        super::make_replace(file_id, range.clone(), format!("use {};", selected))
    });

    super::add_refactor_assist(assists, ctx, AUTO_IMPORT_ID, label, lazy);

    Some(())
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_import_handler_registered() {
        assert_eq!(AUTO_IMPORT_ID, "auto_import");
    }
}
