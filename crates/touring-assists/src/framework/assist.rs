//! `Assist` — a single refactor action (labeled SourceChange).

use serde::{Deserialize, Serialize};
use touring_generator::source_change::SourceChange;

/// Unique identifier for an assist (e.g., "auto_wire", "extract_function").
pub type AssistId = &'static str;

/// Handler function type for an assist.
pub type AssistHandler =
    fn(&mut super::assists::Assists, &super::context::AssistContext) -> Option<()>;

/// Group label for grouping related assists (e.g., "Refactoring", "Code Generation").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistGroup(pub &'static str);

/// A lazy SourceChange — only computed when `evaluate()` is called.
/// This avoids expensive rendering when assists are just being listed.
pub struct LazySourceChange {
    /// Closure that computes the `SourceChange` on demand.
    pub evaluate: Box<dyn Fn() -> SourceChange + Send + Sync>,
}

impl LazySourceChange {
    /// Wrap a closure as a deferred `SourceChange` producer.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn() -> SourceChange + Send + Sync + 'static,
    {
        Self {
            evaluate: Box::new(f),
        }
    }

    /// Force evaluation of the deferred closure, producing the `SourceChange`.
    pub fn evaluate(&self) -> SourceChange {
        (self.evaluate)()
    }
}

/// A single assist — a labeled SourceChange with metadata.
///
/// Created by an assist handler and collected into `Assists`.
pub struct Assist {
    /// Unique assist identifier (e.g., "auto_wire", "extract_function").
    pub id: AssistId,
    /// Human-readable label shown to the user.
    pub label: String,
    /// Group this assist belongs to (for UI grouping).
    pub group: AssistGroup,
    /// The source file range this assist applies to.
    pub target: AssistTarget,
    /// The source change to apply (lazily evaluated).
    pub source_change: LazySourceChange,
    /// Priority score (higher = shown first in applicable assists list).
    pub priority: i32,
}

impl Assist {
    /// Build an assist with default priority (0) from its id, label, group, target, and change.
    pub fn new(
        id: AssistId,
        label: String,
        group: AssistGroup,
        target: AssistTarget,
        source_change: LazySourceChange,
    ) -> Self {
        Self {
            id,
            label,
            group,
            target,
            source_change,
            priority: 0,
        }
    }

    /// Override the priority, returning the assist for builder-style chaining.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// What file and range an assist applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistTarget {
    /// Path to the file (absolute or relative).
    pub file_id: touring_generator::FileId,
    /// Byte range in the file.
    pub range: std::ops::Range<usize>,
}

/// A code action response (LSP-compatible format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistCodeAction {
    /// Assist identifier this action originates from.
    pub id: String,
    /// Human-readable title displayed to the user.
    pub title: String,
    /// Group label used to cluster related actions in the UI.
    pub group: String,
    /// File the action applies to.
    pub file_id: touring_generator::FileId,
    /// Byte range the action targets within the file.
    pub range: std::ops::Range<usize>,
    /// The resolved edit, present only when the action produces changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<AssistEdit>,
    /// An optional command to run instead of (or alongside) the edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// The concrete set of text changes an assist applies to a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistEdit {
    /// File these changes apply to.
    pub file_id: touring_generator::FileId,
    /// Ordered text replacements to perform within the file.
    pub changes: Vec<AssistTextChange>,
}

/// A single text replacement: a byte range and its replacement text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistTextChange {
    /// Byte range in the original file to replace.
    pub range: std::ops::Range<usize>,
    /// Text to insert in place of the range.
    pub new_text: String,
}

impl From<Assist> for AssistCodeAction {
    fn from(assist: Assist) -> Self {
        let source_change = assist.source_change.evaluate();
        let changes: Vec<AssistTextChange> = source_change
            .edits()
            .flat_map(|(_, text_edit)| {
                text_edit.indels().map(|indel| AssistTextChange {
                    range: indel.delete.clone(),
                    new_text: indel.insert.clone(),
                })
            })
            .collect();

        Self {
            id: assist.id.to_string(),
            title: assist.label,
            group: assist.group.0.to_string(),
            file_id: 0, // Would be resolved from the target
            range: assist.target.range.clone(),
            edit: if changes.is_empty() {
                None
            } else {
                Some(AssistEdit {
                    file_id: 0,
                    changes,
                })
            },
            command: None,
        }
    }
}
