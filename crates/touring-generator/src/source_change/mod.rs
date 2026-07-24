//! Source change module — transactional multi-file edit system.
//!
//! Replaces ad-hoc mpatch per-file atomic writes with a unified transactional
//! model. A `SourceChange` carries edits across multiple files plus filesystem
//! operations (create, move, delete) and is applied atomically or rolled back.
//!
//! # Architecture
//!
//! ```text
//! SourceChange
//!   ├── edits: BTreeMap<FileId, TextEdit>   (text edits per file)
//!   ├── fs_edits: Vec<FileSystemEdit>         (create/move/delete files)
//!   └── snippet: Option<SnippetEdit>          (cursor placement metadata)
//! ```
//!
//! # Two-phase apply
//!
//! 1. **Shadow validate** — dry-run all edits, check file existence, permissions
//! 2. **Atomic commit** — if all OK, write all; else rollback (no partial writes)
//!
//! # Feature flag: `source-change`
//!
//! When disabled, `Committed` typestate falls back to single-file mpatch.

pub mod fs_edit;
pub mod snippet;
pub mod text_edit;

mod applier;

pub use applier::{Applier, ApplyError, ApplyResult};
pub use fs_edit::FileSystemEdit;
pub use snippet::{SnippetEdit, TabStop};
pub use text_edit::{Indel, NonOverlapError, TextEdit};

use std::collections::BTreeMap;

/// File identifier — maps to `touring-core` `FileId` or similar.
/// Using `usize` for now; will integrate with workspace `FileId` later.
pub type FileId = usize;

/// Root-level source change combining text edits, filesystem ops, and snippet.
///
/// # Invariant
///
/// All `Indel` ranges inside each `TextEdit` MUST be non-overlapping and sorted.
/// This is validated at construction time via `TextEdit::try_from_iter()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChange {
    /// Text edits organized by file. Each `TextEdit` must satisfy the
    /// non-overlap invariant (sorted, non-overlapping `Indel` ranges).
    edits: BTreeMap<FileId, TextEdit>,
    /// Filesystem-level operations (create, overwrite, move, delete).
    fs_edits: Vec<FileSystemEdit>,
    /// Optional snippet metadata for cursor placement (`$0`, `${0:default}`).
    snippet: Option<SnippetEdit>,
}

impl SourceChange {
    /// Create a new empty `SourceChange`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            edits: BTreeMap::new(),
            fs_edits: Vec::new(),
            snippet: None,
        }
    }

    /// Add a text edit for a given file.
    #[must_use]
    pub fn with_edit(mut self, file_id: FileId, text_edit: TextEdit) -> Self {
        self.edits.insert(file_id, text_edit);
        self
    }

    /// Add a filesystem edit.
    #[must_use]
    pub fn with_fs_edit(mut self, fs_edit: FileSystemEdit) -> Self {
        self.fs_edits.push(fs_edit);
        self
    }

    /// Set the snippet edit.
    #[must_use]
    pub fn with_snippet(mut self, snippet: SnippetEdit) -> Self {
        self.snippet = Some(snippet);
        self
    }

    /// Returns the number of files being edited.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.edits.len()
    }

    /// Returns the number of filesystem operations.
    #[must_use]
    pub fn fs_edit_count(&self) -> usize {
        self.fs_edits.len()
    }

    /// Returns a reference to the text edit for a file, if any.
    #[must_use]
    pub fn edit_for(&self, file_id: FileId) -> Option<&TextEdit> {
        self.edits.get(&file_id)
    }

    /// Returns the filesystem edits.
    #[must_use]
    pub fn fs_edits(&self) -> &[FileSystemEdit] {
        &self.fs_edits
    }

    /// Returns the snippet edit, if any.
    #[must_use]
    pub fn snippet(&self) -> Option<&SnippetEdit> {
        self.snippet.as_ref()
    }

    /// Returns an iterator over (`file_id`, `text_edit`) pairs.
    pub fn edits(&self) -> impl Iterator<Item = (&FileId, &TextEdit)> {
        self.edits.iter()
    }

    /// Returns true if there are no edits, `fs_edits`, or snippet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.fs_edits.is_empty() && self.snippet.is_none()
    }
}

impl Default for SourceChange {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SourceChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SourceChange(files={}, fs_ops={}",
            self.file_count(),
            self.fs_edit_count()
        )?;
        if let Some(ref s) = self.snippet {
            write!(f, ", snippet={s}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// rkyv serialization (B.5.4 — deferred to touring-rkyv for proper IPC)
// ---------------------------------------------------------------------------

/// Error returned by zero-copy IPC serialization operations on [`SourceChange`].
#[cfg(feature = "zero-copy")]
#[derive(Debug, thiserror::Error)]
pub enum SourceChangeSerializationError {
    /// Serialization to bytes is deferred to B.5.8 implementation.
    #[error("zero-copy serialization deferred to B.5.8")]
    SerializationDeferred,
    /// Deserialization from bytes is deferred to B.5.8 implementation.
    #[error("zero-copy deserialization deferred to B.5.8")]
    DeserializationDeferred,
}

/// Serialize to bytes for IPC (daemon ↔ hooks).
///
/// Currently a stub that returns Err. Proper implementation will delegate
/// to `touring_rkyv` serialization in B.5.8 (MCP tool) where we can
/// properly integrate with the IPC framing system.
///
/// # Errors
///
/// Always returns [`SourceChangeSerializationError::SerializationDeferred`]
/// indicating zero-copy serialization is deferred.
#[cfg(feature = "zero-copy")]
pub fn to_bytes_sc(change: &SourceChange) -> Result<Vec<u8>, SourceChangeSerializationError> {
    let _ = change;
    Err(SourceChangeSerializationError::SerializationDeferred)
}

/// Deserialize from bytes.
///
/// # Errors
///
/// Always returns [`SourceChangeSerializationError::DeserializationDeferred`]
/// indicating zero-copy deserialization is deferred.
#[cfg(feature = "zero-copy")]
pub fn from_bytes_sc(bytes: &[u8]) -> Result<SourceChange, SourceChangeSerializationError> {
    let _ = bytes;
    Err(SourceChangeSerializationError::DeserializationDeferred)
}

#[cfg(not(feature = "zero-copy"))]
mod serialization_stub {
    /// Error returned by serialization stub operations when the zero-copy feature is disabled.
    #[derive(Debug, thiserror::Error)]
    pub enum SourceChangeStubError {
        /// The zero-copy feature is not enabled for this build.
        #[error("zero-copy feature not enabled")]
        FeatureDisabled,
    }

    impl super::SourceChange {
        /// Serialize to bytes for IPC (stub when zero-copy is disabled).
        ///
        /// # Errors
        ///
        /// Always returns [`SourceChangeStubError::FeatureDisabled`]
        /// indicating zero-copy is not enabled.
        pub fn to_bytes(&self) -> Result<Vec<u8>, SourceChangeStubError> {
            Err(SourceChangeStubError::FeatureDisabled)
        }

        /// Deserialize from bytes (stub when zero-copy is disabled).
        ///
        /// # Errors
        ///
        /// Always returns [`SourceChangeStubError::FeatureDisabled`]
        /// indicating zero-copy is not enabled.
        pub fn from_bytes(_bytes: &[u8]) -> Result<Self, SourceChangeStubError> {
            Err(SourceChangeStubError::FeatureDisabled)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FileSystemEdit, Indel, SnippetEdit, SourceChange, TextEdit};

    #[test]
    fn source_change_empty() {
        let sc = SourceChange::new();
        assert!(sc.is_empty());
        assert_eq!(sc.file_count(), 0);
        assert_eq!(sc.fs_edit_count(), 0);
        assert!(sc.snippet().is_none());
    }

    #[test]
    fn source_change_builder() {
        let text_edit = TextEdit::try_from_iter(vec![Indel::insert(0, "fn main() {}".into())])
            .expect("test indels are valid");
        let sc = SourceChange::new()
            .with_edit(1, text_edit)
            .with_fs_edit(FileSystemEdit::CreateFile {
                path: "/tmp/test.txt".into(),
                content: "hello".into(),
            })
            .with_snippet(SnippetEdit::new("fn main() $0".into()));

        assert_eq!(sc.file_count(), 1);
        assert_eq!(sc.fs_edit_count(), 1);
        assert!(sc.snippet().is_some());
    }

    #[test]
    fn source_change_display() {
        let sc = SourceChange::new().with_edit(42, TextEdit::default());
        let display = format!("{sc}");
        assert!(display.contains("files=1"));
    }
}
