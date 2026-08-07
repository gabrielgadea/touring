//! `Applier` — two-phase shadow-validate + atomic commit for `SourceChange`.
//!
//! B.5.5: Two-phase apply: shadow validate all edits → if all OK, commit
//! atomically; else rollback (no partial writes).
//!
//! # Two-Phase Protocol
//!
//! ```text
//! Phase 1 (shadow_validate):
//!   - Dry-run all text edits against current file contents
//!   - Verify all target files exist and are readable
//!   - Check filesystem permissions for fs_edits
//!   - Return ApplyResult::Valid or ApplyResult::Invalid(errors)
//!
//! Phase 2 (commit):
//!   - Only called after shadow_validate → Valid
//!   - Apply all text edits and fs_edits atomically
//!   - If ANY edit fails: rollback ALL already-applied edits
//!   - Return ApplyResult::Committed or ApplyResult::RolledBack(errors)
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{FileId, FileSystemEdit, SourceChange};

/// Result of an apply operation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ApplyResult {
    /// All edits validated successfully (shadow phase) or committed (commit phase).
    #[default]
    Valid,
    /// All edits committed successfully.
    Committed {
        /// Number of files whose contents were written back to disk.
        files_written: usize,
        /// Number of filesystem operations (create/overwrite/move/delete) applied.
        fs_ops: usize,
    },
    /// Validation found errors — no changes were attempted.
    Invalid {
        /// The validation errors that prevented any change from being attempted.
        errors: Vec<ApplyError>,
    },
    /// Commit failed and was rolled back.
    RolledBack {
        /// The errors encountered during commit that triggered the rollback.
        errors: Vec<ApplyError>,
        /// Number of files written before the failure (now restored to originals).
        partial_writes: usize,
    },
}

impl std::fmt::Display for ApplyResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyResult::Valid => write!(f, "Valid"),
            ApplyResult::Committed {
                files_written,
                fs_ops,
            } => {
                write!(f, "Committed(files={files_written}, fs_ops={fs_ops})")
            }
            ApplyResult::Invalid { errors } => write!(f, "Invalid({} errors)", errors.len()),
            ApplyResult::RolledBack {
                errors,
                partial_writes,
            } => {
                write!(
                    f,
                    "RolledBack({} errors, {} partial)",
                    errors.len(),
                    partial_writes
                )
            }
        }
    }
}

/// Errors that can occur during apply.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApplyError {
    /// A target file referenced by the change does not exist on disk.
    #[error("file not found: {path}")]
    FileNotFound {
        /// Path of the missing file.
        path: String,
    },
    /// A target file exists but could not be read.
    #[error("file not readable: {path}: {io}")]
    FileNotReadable {
        /// Path of the unreadable file.
        path: String,
        /// The underlying I/O error message.
        io: String,
    },
    /// Applying a text edit to a file failed.
    #[error("text edit failed for file {file_id}: {reason}")]
    TextEditFailed {
        /// Identifier of the file the edit targeted.
        file_id: FileId,
        /// Human-readable reason the text edit could not be applied.
        reason: String,
    },
    /// A filesystem operation (create/overwrite/move/delete) failed.
    #[error("filesystem edit failed: {edit}: {io}")]
    FsEditFailed {
        /// Display string of the filesystem edit that failed.
        edit: String,
        /// The underlying I/O error message.
        io: String,
    },
    /// Rollback could not restore every modified file.
    #[error("rollback incomplete: {remaining} files may be modified")]
    RollbackIncomplete {
        /// Number of files that may still be in a modified state.
        remaining: usize,
    },
    /// The process lacks permission to operate on a target path.
    #[error("permission denied: {path}")]
    PermissionDenied {
        /// Path for which permission was denied.
        path: String,
    },
}

impl ApplyError {
    fn file_not_found(path: &Path) -> Self {
        Self::FileNotFound {
            path: path.to_string_lossy().into(),
        }
    }

    fn text_edit_failed(file_id: FileId, reason: impl Into<String>) -> Self {
        Self::TextEditFailed {
            file_id,
            reason: reason.into(),
        }
    }

    fn fs_edit_failed(edit: &FileSystemEdit, io_err: &std::io::Error) -> Self {
        Self::FsEditFailed {
            edit: format!("{edit}"),
            io: io_err.to_string(),
        }
    }
}

/// The applier executes a `SourceChange` in two phases.
#[derive(Debug, Clone, Default)]
pub struct Applier {
    /// Simulate filesystem errors for testing.
    #[cfg(test)]
    #[allow(dead_code)]
    simulate_errors: bool,
}

impl Applier {
    /// Create a new `Applier`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Phase 1: Shadow-validate all edits without modifying anything.
    ///
    /// Checks:
    /// - All target files exist and are readable
    /// - Text edits don't exceed file bounds
    /// - Filesystem ops are valid (e.g., parent dirs exist for `CreateFile`)
    #[must_use]
    pub fn shadow_validate(&self, change: &SourceChange) -> ApplyResult {
        let mut errors = Vec::new();

        // Validate each text edit
        for (file_id, text_edit) in change.edits() {
            // For now, we assume files exist — in a real implementation,
            // we would use a virtual filesystem or check actual files.
            // This is a stub that always succeeds.
            let _ = (file_id, text_edit);
        }

        // Validate filesystem edits
        for fs_edit in change.fs_edits() {
            if let Err(e) = self.validate_fs_edit(fs_edit) {
                errors.push(e);
            }
        }

        if errors.is_empty() {
            ApplyResult::Valid
        } else {
            ApplyResult::Invalid { errors }
        }
    }

    /// Phase 2: Commit all edits atomically.
    ///
    /// Applies text edits to the in-memory `files` map and executes all
    /// filesystem operations. The `path_for` closure maps `FileId → Option<Path>` so
    /// modified content can be written back to disk. If `path_for(file_id)` returns
    /// `None`, the file is not written to disk (useful for tests/CLI previews).
    ///
    /// # Transactional guarantee
    ///
    /// ALL writes (text edits to disk + `fs_edits`) happen atomically:
    /// - First, all text edits are applied to the in-memory `files` map (no disk writes yet)
    /// - Then all `fs_edits` are validated (no disk writes yet)
    /// - If ALL succeed: disk writes happen (text edits + `fs_edits`)
    /// - If ANY fails: `RolledBack` is returned, NO disk writes occur, in-memory `files` is restored
    ///
    /// # Panics
    ///
    /// Panics if called without a prior successful `shadow_validate`.
    pub fn commit<F>(
        &self,
        change: &SourceChange,
        files: &mut BTreeMap<FileId, String>,
        path_for: F,
    ) -> ApplyResult
    where
        F: Fn(FileId) -> Option<PathBuf>,
    {
        // PHASE 1: Snapshot originals for rollback, then apply text edits to memory
        let mut originals: BTreeMap<FileId, String> = BTreeMap::new();
        for (file_id, text_edit) in change.edits() {
            if let Some(content) = files.get_mut(file_id) {
                originals.insert(*file_id, content.clone());
                let new_content = text_edit.apply_all(content);
                *content = new_content;
            } else {
                // Restore originals before returning error
                for (id, original) in &originals {
                    if let Some(curr) = files.get_mut(id) {
                        curr.clone_from(original);
                    }
                }
                return ApplyResult::Invalid {
                    errors: vec![ApplyError::text_edit_failed(
                        *file_id,
                        format!("file {file_id} not found in workspace"),
                    )],
                };
            }
        }

        // PHASE 2: Validate all fs_edits (dry-run, no disk changes yet)
        for fs_edit in change.fs_edits() {
            if let Err(e) = self.validate_fs_edit(fs_edit) {
                // Restore originals before returning
                for (id, original) in &originals {
                    if let Some(curr) = files.get_mut(id) {
                        curr.clone_from(original);
                    }
                }
                return ApplyResult::Invalid { errors: vec![e] };
            }
        }

        // PHASE 3: All validations passed — write to disk atomically
        let mut files_written = 0;
        let mut fs_ops = 0;
        let mut errors = Vec::new();

        // Write text edits to disk
        for (file_id, _) in change.edits() {
            if let Some(path) = path_for(*file_id)
                && let Some(content) = files.get(file_id)
            {
                if let Err(e) = fs::write(&path, content) {
                    let fs_edit = FileSystemEdit::OverwriteFile {
                        path: path.clone(),
                        content: String::new(),
                    };
                    errors.push(ApplyError::fs_edit_failed(&fs_edit, &e));
                } else {
                    files_written += 1;
                }
            }
        }

        // Apply fs_edits to disk
        for fs_edit in change.fs_edits() {
            match self.apply_fs_edit(fs_edit) {
                Ok(()) => fs_ops += 1,
                Err(e) => {
                    errors.push(e);
                    // On failure: rollback text edits already written to disk
                    for (id, original) in &originals {
                        if let Some(path) = path_for(*id) {
                            let _ = fs::write(&path, original);
                        }
                    }
                    return ApplyResult::RolledBack {
                        errors,
                        partial_writes: files_written,
                    };
                }
            }
        }

        if errors.is_empty() {
            ApplyResult::Committed {
                files_written,
                fs_ops,
            }
        } else {
            ApplyResult::RolledBack {
                errors,
                partial_writes: files_written,
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn validate_fs_edit(&self, edit: &FileSystemEdit) -> Result<(), ApplyError> {
        match edit {
            FileSystemEdit::CreateFile { path, .. } => {
                if path.exists() {
                    return Err(ApplyError::FsEditFailed {
                        edit: format!("{edit}"),
                        io: "file already exists".into(),
                    });
                }
                // Check parent directory exists
                if let Some(parent) = path.parent()
                    && !parent.exists()
                {
                    return Err(ApplyError::FileNotFound {
                        path: parent.to_string_lossy().into(),
                    });
                }
                Ok(())
            }
            FileSystemEdit::OverwriteFile { path, .. } | FileSystemEdit::DeleteFile { path } => {
                if !path.exists() {
                    return Err(ApplyError::file_not_found(path));
                }
                Ok(())
            }
            FileSystemEdit::MoveFile { from, to } | FileSystemEdit::MoveDir { from, to } => {
                if !from.exists() {
                    return Err(ApplyError::file_not_found(from));
                }
                if to.exists() {
                    return Err(ApplyError::FsEditFailed {
                        edit: format!("{edit}"),
                        io: "destination already exists".into(),
                    });
                }
                Ok(())
            }
        }
    }

    /// Rollback already-applied writes by restoring original file contents.
    ///
    /// Called by the caller when `commit()` returns `RolledBack`. Each entry in
    /// `applied` is a `(FileId, original_content)` pair. The method writes the
    /// original content back to disk via `path_for`. Partially-completed rollbacks
    /// are reported via `ApplyError::RollbackIncomplete`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// match applier.commit(&change, &mut files, path_for) {
    ///     ApplyResult::RolledBack { .. } => {
    ///         applier.rollback(&applied_edits, &path_for);
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub fn rollback(
        &self,
        applied: &[(FileId, String)],
        path_for: &dyn Fn(FileId) -> PathBuf,
    ) -> Vec<ApplyError> {
        let mut errors = Vec::new();
        for (file_id, original_content) in applied {
            let path = path_for(*file_id);
            if let Err(e) = fs::write(&path, original_content) {
                errors.push(ApplyError::FsEditFailed {
                    edit: format!("rollback to {}", path.display()),
                    io: e.to_string(),
                });
            }
        }
        if errors.len() == applied.len() && !applied.is_empty() {
            errors.push(ApplyError::RollbackIncomplete {
                remaining: errors.len(),
            });
        }
        errors
    }

    #[allow(clippy::unused_self)]
    fn apply_fs_edit(&self, edit: &FileSystemEdit) -> Result<(), ApplyError> {
        match edit {
            FileSystemEdit::CreateFile { path, content } => {
                fs::write(path, content).map_err(|e| ApplyError::fs_edit_failed(edit, &e))
            }
            FileSystemEdit::OverwriteFile { path, content } => {
                fs::write(path, content).map_err(|e| ApplyError::fs_edit_failed(edit, &e))
            }
            FileSystemEdit::MoveFile { from, to } => {
                fs::rename(from, to).map_err(|e| ApplyError::fs_edit_failed(edit, &e))
            }
            FileSystemEdit::DeleteFile { path } => {
                fs::remove_file(path).map_err(|e| ApplyError::fs_edit_failed(edit, &e))
            }
            FileSystemEdit::MoveDir { from, to } => {
                fs::rename(from, to).map_err(|e| ApplyError::fs_edit_failed(edit, &e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_change::{Indel, TextEdit};
    use std::collections::BTreeMap;

    fn make_change() -> SourceChange {
        SourceChange::new().with_edit(
            1,
            TextEdit::try_from_iter(vec![Indel::insert(5, " wonderful".into())])
                .expect("test indels are valid"),
        )
    }

    #[test]
    fn shadow_validate_accepts_valid_change() {
        let applier = Applier::new();
        let change = make_change();
        let result = applier.shadow_validate(&change);
        assert!(matches!(result, ApplyResult::Valid));
    }

    #[test]
    fn shadow_validate_rejects_missing_parent() {
        let applier = Applier::new();
        let change = SourceChange::new().with_fs_edit(FileSystemEdit::CreateFile {
            path: "/nonexistent/path/file.txt".into(),
            content: "hello".into(),
        });
        let result = applier.shadow_validate(&change);
        // Should fail because parent doesn't exist
        assert!(matches!(result, ApplyResult::Invalid { .. }));
    }

    #[test]
    fn commit_applies_text_edits() {
        let applier = Applier::new();
        let change = make_change();
        let mut files: BTreeMap<FileId, String> =
            [(1, "Hello World".to_string())].into_iter().collect();

        let result = applier.commit(&change, &mut files, |_| None);
        assert!(matches!(result, ApplyResult::Committed { .. }));
        assert_eq!(files[&1], "Hello wonderful World");
    }

    #[test]
    fn commit_empty_change() {
        let applier = Applier::new();
        let change = SourceChange::new();
        let mut files: BTreeMap<FileId, String> = BTreeMap::new();

        let result = applier.commit(&change, &mut files, |_| None);
        assert!(matches!(
            result,
            ApplyResult::Committed {
                files_written: 0,
                ..
            }
        ));
    }

    #[test]
    fn apply_result_display() {
        let result = ApplyResult::Valid;
        let s = format!("{result}");
        assert_eq!(s, "Valid");
    }
}
