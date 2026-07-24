//! `AssistContext` — input data for assist handlers.

use touring_generator::FileId;

/// Input data passed to all assist handlers.
/// Contains the file, range, and syntax tree needed to analyze and produce edits.
#[derive(Debug, Clone)]
pub struct AssistContext<'a> {
    /// The file being edited.
    pub file_id: FileId,
    /// The file path (for display and SourceChange construction).
    pub file_path: &'a str,
    /// The current file content.
    pub content: &'a str,
    /// The target range selected by the user (byte offset).
    pub range: std::ops::Range<usize>,
}

impl<'a> AssistContext<'a> {
    /// Construct a context from the file id, path, content, and selected range.
    pub fn new(
        file_id: FileId,
        file_path: &'a str,
        content: &'a str,
        range: std::ops::Range<usize>,
    ) -> Self {
        Self {
            file_id,
            file_path,
            content,
            range,
        }
    }

    /// Get the selected text.
    pub fn selected_text(&self) -> &str {
        &self.content[self.range.clone()]
    }

    /// Check if the range is empty (cursor position).
    pub fn is_cursor(&self) -> bool {
        self.range.is_empty()
    }
}
