//! `FileSystemEdit` — atomic filesystem operations for the `SourceChange` system.
//!
//! B.5.1: `FileSystemEdit` enum variants for `CreateFile`, `OverwriteFile`,
//! `MoveFile`, `DeleteFile`, and `MoveDir`.

use std::fmt;
use std::path::PathBuf;

/// Filesystem-level edit operations that accompany text edits in a `SourceChange`.
/// These are applied atomically with text edits in the two-phase commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSystemEdit {
    /// Create a new file with the given content. Fails if the file already exists.
    CreateFile {
        /// Absolute path to the new file.
        path: PathBuf,
        /// Initial content of the file.
        content: String,
    },
    /// Overwrite an existing file with new content. Fails if the file does not exist.
    OverwriteFile {
        /// Absolute path to the file to overwrite.
        path: PathBuf,
        /// New content to write.
        content: String,
    },
    /// Move or rename a file. Fails if the source does not exist or dest exists.
    MoveFile {
        /// Source path.
        from: PathBuf,
        /// Destination path.
        to: PathBuf,
    },
    /// Delete a file. Fails if the file does not exist.
    DeleteFile {
        /// Path to the file to delete.
        path: PathBuf,
    },
    /// Move or rename a directory. Fails if source does not exist or dest exists.
    MoveDir {
        /// Source directory path.
        from: PathBuf,
        /// Destination directory path.
        to: PathBuf,
    },
}

impl FileSystemEdit {
    /// Returns the primary path involved in this operation.
    /// For CreateFile/OverwriteFile/DeleteFile, returns that path.
    /// For MoveFile/MoveDir, returns the source path.
    #[must_use]
    pub fn primary_path(&self) -> &PathBuf {
        match self {
            FileSystemEdit::CreateFile { path, .. }
            | FileSystemEdit::OverwriteFile { path, .. }
            | FileSystemEdit::DeleteFile { path, .. } => path,
            FileSystemEdit::MoveFile { from, .. } | FileSystemEdit::MoveDir { from, .. } => from,
        }
    }

    /// Returns true if this operation creates a new path (not an overwrite/delete).
    #[inline]
    #[must_use]
    pub fn is_creation(&self) -> bool {
        matches!(self, FileSystemEdit::CreateFile { .. })
    }

    /// Returns true if this operation deletes a path.
    #[inline]
    #[must_use]
    pub fn is_deletion(&self) -> bool {
        matches!(self, FileSystemEdit::DeleteFile { .. })
    }

    /// Returns true if this is a move operation (file or directory).
    #[inline]
    #[must_use]
    pub fn is_move(&self) -> bool {
        matches!(
            self,
            FileSystemEdit::MoveFile { .. } | FileSystemEdit::MoveDir { .. }
        )
    }
}

impl fmt::Display for FileSystemEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileSystemEdit::CreateFile { path, .. } => {
                write!(f, "CreateFile({})", path.display())
            }
            FileSystemEdit::OverwriteFile { path, .. } => {
                write!(f, "OverwriteFile({})", path.display())
            }
            FileSystemEdit::MoveFile { from, to } => {
                write!(f, "MoveFile({} -> {})", from.display(), to.display())
            }
            FileSystemEdit::DeleteFile { path } => {
                write!(f, "DeleteFile({})", path.display())
            }
            FileSystemEdit::MoveDir { from, to } => {
                write!(f, "MoveDir({} -> {})", from.display(), to.display())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_file_is_creation() {
        let edit = FileSystemEdit::CreateFile {
            path: PathBuf::from("/tmp/new.txt"),
            content: "hello".into(),
        };
        assert!(edit.is_creation());
        assert!(!edit.is_deletion());
        assert!(!edit.is_move());
        assert_eq!(edit.primary_path(), &PathBuf::from("/tmp/new.txt"));
    }

    #[test]
    fn delete_file_is_deletion() {
        let edit = FileSystemEdit::DeleteFile {
            path: PathBuf::from("/tmp/old.txt"),
        };
        assert!(!edit.is_creation());
        assert!(edit.is_deletion());
        assert!(!edit.is_move());
    }

    #[test]
    fn move_file_is_move() {
        let edit = FileSystemEdit::MoveFile {
            from: PathBuf::from("/tmp/a.txt"),
            to: PathBuf::from("/tmp/b.txt"),
        };
        assert!(!edit.is_creation());
        assert!(!edit.is_deletion());
        assert!(edit.is_move());
    }

    #[test]
    fn file_system_edit_display() {
        let edit = FileSystemEdit::CreateFile {
            path: PathBuf::from("/tmp/test.txt"),
            content: "hi".into(),
        };
        let s = format!("{edit}");
        assert!(s.contains("CreateFile"));
        assert!(s.contains("test.txt"));
    }
}
