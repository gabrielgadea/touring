//! Assist handlers.
//!
//! Each handler is a function `fn(&mut Assists, &AssistContext) -> Option<()>`.
//! Handlers analyze the context and add applicable assists to the accumulator.
//!
//! # Shared helpers (used by every handler sub-module)
//!
//! - [`REFACTOR`] — single-source-of-truth group constant (eliminates 11 identical declarations)
//! - [`make_replace`] — builds a single-range `SourceChange` (eliminates 12 identical constructions)
//! - [`add_refactor_assist`] — registers with standard context target (eliminates 12 identical 8-line blocks)

use crate::{AssistContext, AssistGroup, AssistId, AssistTarget, Assists, LazySourceChange};
use touring_generator::source_change::{Indel, SourceChange, TextEdit};

/// Shared group label for all refactoring assists.
///
/// Replaces the identical `const REFACTOR: AssistGroup = AssistGroup("Refactoring")`
/// that was declared once in each of the 11 handler files.
const REFACTOR: AssistGroup = AssistGroup("Refactoring");

/// Build a `SourceChange` that replaces `range` with `insert` in `file_id`.
///
/// Shared single-edit helper — replaces the repeated
/// `SourceChange::new().with_edit(file_id, TextEdit::try_from_iter(…).expect(…))`
/// pattern present in 10 of the 11 handlers (12 call-sites total).
fn make_replace(
    file_id: touring_generator::FileId,
    range: std::ops::Range<usize>,
    insert: String,
) -> SourceChange {
    SourceChange::new().with_edit(
        file_id,
        TextEdit::try_from_iter(vec![Indel {
            delete: range,
            insert,
        }])
        .expect("valid indels"),
    )
}

/// Register a refactoring assist using the standard context-derived target.
///
/// Replaces the repeated `assists.add_with_group(…, REFACTOR, AssistTarget { … })`
/// 8-line block that appeared identically at 12 call-sites across all handlers.
fn add_refactor_assist(
    assists: &mut Assists,
    ctx: &AssistContext,
    id: AssistId,
    label: String,
    lazy: LazySourceChange,
) {
    assists.add_with_group(
        id,
        label,
        REFACTOR,
        AssistTarget {
            file_id: ctx.file_id,
            range: ctx.range.clone(),
        },
        lazy,
    );
}

mod add_missing_match_arms;
mod auto_import;
mod auto_wire;
mod change_visibility;
mod convert_to_guarded_return;
mod extract_function;
mod format_rust_preserve;
mod generate_impl;
mod inline_call;
mod merge_imports;
mod move_module_to_file;

pub use add_missing_match_arms::ADD_MISSING_MATCH_ARMS;
pub use add_missing_match_arms::ADD_MISSING_MATCH_ARMS_ID;
pub use auto_import::AUTO_IMPORT;
pub use auto_import::AUTO_IMPORT_ID;
pub use auto_wire::AUTO_WIRE;
pub use auto_wire::AUTO_WIRE_ID;
pub use change_visibility::CHANGE_VISIBILITY;
pub use change_visibility::CHANGE_VISIBILITY_ID;
pub use convert_to_guarded_return::CONVERT_TO_GUARDED_RETURN;
pub use convert_to_guarded_return::CONVERT_TO_GUARDED_RETURN_ID;
pub use extract_function::EXTRACT_FUNCTION;
pub use extract_function::EXTRACT_FUNCTION_ID;
pub use format_rust_preserve::FORMAT_RUST_PRESERVE;
pub use format_rust_preserve::FORMAT_RUST_PRESERVE_ID;
pub use generate_impl::GENERATE_IMPL;
pub use generate_impl::GENERATE_IMPL_ID;
pub use inline_call::INLINE_CALL;
pub use inline_call::INLINE_CALL_ID;
pub use merge_imports::MERGE_IMPORTS;
pub use merge_imports::MERGE_IMPORTS_ID;
pub use move_module_to_file::MOVE_MODULE_TO_FILE;
pub use move_module_to_file::MOVE_MODULE_TO_FILE_ID;

/// Handler type.
pub type HandlerFn = fn(&mut crate::Assists, &crate::AssistContext) -> Option<()>;

/// All registered handlers (catalog).
pub const ALL_HANDLERS: &[(&str, HandlerFn)] = &[
    ("add_missing_match_arms", ADD_MISSING_MATCH_ARMS),
    ("auto_import", AUTO_IMPORT),
    ("auto_wire", AUTO_WIRE),
    ("change_visibility", CHANGE_VISIBILITY),
    ("convert_to_guarded_return", CONVERT_TO_GUARDED_RETURN),
    ("extract_function", EXTRACT_FUNCTION),
    ("format_rust_preserve", FORMAT_RUST_PRESERVE),
    ("generate_impl", GENERATE_IMPL),
    ("inline_call", INLINE_CALL),
    ("merge_imports", MERGE_IMPORTS),
    ("move_module_to_file", MOVE_MODULE_TO_FILE),
];
