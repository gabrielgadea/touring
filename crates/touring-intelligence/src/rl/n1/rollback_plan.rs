//! Rollback plan for generated sequences.
//!
//! Provides the means to restore state if a generated sequence
//! fails mid-execution. Part of the TRIAD pattern.

use serde::{Deserialize, Serialize};

/// A plan for rolling back a failed sequence execution.
///
/// Contains all information needed to restore the system to its
/// pre-execution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPlan {
    /// Individual rollback actions, in reverse execution order.
    pub actions: Vec<RollbackAction>,
    /// Whether full rollback is possible.
    pub full_rollback_possible: bool,
    /// Estimated rollback duration (ms).
    pub estimated_duration_ms: u64,
}

/// A single rollback action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackAction {
    /// Action kind.
    pub kind: RollbackActionKind,
    /// Order index (for debugging).
    pub index: usize,
    /// Description of the action.
    pub description: String,
}

/// Kind of rollback action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollbackActionKind {
    /// Restore a file from a backup.
    RestoreFile {
        /// Target file path to restore.
        path: String,
        /// Backup file path to restore from.
        backup_path: String,
    },
    /// Delete a created file.
    DeleteFile(String),
    /// Restore a directory.
    RestoreDirectory {
        /// Target directory path to restore.
        path: String,
        /// Backup directory path to restore from.
        backup_path: String,
    },
    /// Delete a created directory.
    DeleteDirectory(String),
    /// Restore environment variable(s).
    RestoreEnv {
        /// Environment variable key to restore.
        key: String,
        /// Prior value to restore, or `None` to unset.
        old_value: Option<String>,
    },
    /// Execute a custom rollback command.
    CustomCommand {
        /// Command executable to run.
        command: String,
        /// Arguments passed to the command.
        args: Vec<String>,
    },
    /// No-op action (placeholder).
    NoOp(String),
}

impl RollbackPlan {
    /// Create an empty rollback plan.
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            full_rollback_possible: false,
            estimated_duration_ms: 0,
        }
    }

    /// Add a restore file action.
    pub fn with_restore_file(mut self, path: impl Into<String>, backup: impl Into<String>) -> Self {
        let path_str = path.into();
        let backup_str = backup.into();
        let idx = self.actions.len();
        self.actions.push(RollbackAction {
            kind: RollbackActionKind::RestoreFile {
                path: path_str.clone(),
                backup_path: backup_str,
            },
            index: idx,
            description: format!("Restore file: {}", path_str),
        });
        self
    }

    /// Add a delete file action.
    pub fn with_delete_file(mut self, path: impl Into<String>) -> Self {
        let path_str = path.into();
        let idx = self.actions.len();
        self.actions.push(RollbackAction {
            kind: RollbackActionKind::DeleteFile(path_str.clone()),
            index: idx,
            description: format!("Delete file: {}", path_str),
        });
        self
    }

    /// Add a no-op action (placeholder for future actions).
    pub fn with_noop(mut self, description: impl Into<String>) -> Self {
        let idx = self.actions.len();
        self.actions.push(RollbackAction {
            kind: RollbackActionKind::NoOp(description.into()),
            index: idx,
            description: String::new(),
        });
        self
    }

    /// Mark that full rollback is possible.
    pub fn with_full_rollback_possible(mut self) -> Self {
        self.full_rollback_possible = true;
        self
    }

    /// Set estimated duration.
    pub fn with_estimated_duration_ms(mut self, ms: u64) -> Self {
        self.estimated_duration_ms = ms;
        self
    }

    /// Execute the rollback plan, returning errors.
    pub fn execute(&self) -> Result<(), RollbackError> {
        // Execute in reverse order
        for action in self.actions.iter().rev() {
            match &action.kind {
                RollbackActionKind::NoOp(_) => {}
                RollbackActionKind::RestoreFile { path, backup_path } => {
                    std::fs::copy(backup_path, path).map_err(|e| RollbackError::RestoreFailed {
                        path: path.clone(),
                        source: e,
                    })?;
                }
                RollbackActionKind::DeleteFile(path) => {
                    std::fs::remove_file(path).ok(); // Ignore if already deleted
                }
                RollbackActionKind::RestoreDirectory { path, backup_path } => {
                    // Copy directory contents recursively
                    copy_dir_recursive(backup_path, path).map_err(|e| {
                        RollbackError::RestoreFailed {
                            path: path.clone(),
                            source: e,
                        }
                    })?;
                }
                RollbackActionKind::DeleteDirectory(path) => {
                    std::fs::remove_dir_all(path).ok(); // Ignore if already deleted
                }
                RollbackActionKind::RestoreEnv { key, old_value } => match old_value {
                    // TODO: Audit that the environment access only happens in single-threaded code.
                    Some(v) => unsafe { std::env::set_var(key, v) },
                    // TODO: Audit that the environment access only happens in single-threaded code.
                    None => unsafe { std::env::remove_var(key) },
                },
                RollbackActionKind::CustomCommand { command, args } => {
                    std::process::Command::new(command)
                        .args(args)
                        .output()
                        .map_err(|e| RollbackError::CommandFailed(e.to_string()))?;
                }
            }
        }
        Ok(())
    }
}

impl Default for RollbackPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// Error during rollback.
#[derive(Debug)]
pub enum RollbackError {
    /// A file/directory restore failed.
    RestoreFailed {
        /// Path that failed to restore.
        path: String,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A custom rollback command failed.
    CommandFailed(String),
    /// Rollback completed only partially.
    PartialRollback {
        /// Number of actions successfully applied.
        completed: usize,
        /// Total number of actions in the plan.
        total: usize,
    },
}

impl std::fmt::Display for RollbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RollbackError::RestoreFailed { path, source } => {
                write!(f, "Failed to restore {}: {}", path, source)
            }
            RollbackError::CommandFailed(msg) => write!(f, "Rollback command failed: {}", msg),
            RollbackError::PartialRollback { completed, total } => {
                write!(
                    f,
                    "Partial rollback: {} of {} actions completed",
                    completed, total
                )
            }
        }
    }
}

impl std::error::Error for RollbackError {}

fn copy_dir_recursive(src: &str, dst: &str) -> std::io::Result<()> {
    use std::fs;

    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = std::path::Path::new(dst).join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&src_path.to_string_lossy(), &dst_path.to_string_lossy())?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_rollback_plan_builder() {
        let plan = RollbackPlan::new()
            .with_restore_file("src/lib.rs", "/tmp/backup_lib.rs")
            .with_delete_file("target/debug/lib.d")
            .with_noop("placeholder")
            .with_full_rollback_possible()
            .with_estimated_duration_ms(500);

        assert_eq!(plan.actions.len(), 3);
        assert!(plan.full_rollback_possible);
        assert_eq!(plan.estimated_duration_ms, 500);
    }

    #[test]
    fn test_copy_dir_recursive() {
        let tmp_src = TempDir::new().unwrap();
        let tmp_dst = TempDir::new().unwrap();

        // Create source structure
        std::fs::write(tmp_src.path().join("file.txt"), "content").unwrap();
        std::fs::create_dir(tmp_src.path().join("subdir")).unwrap();
        std::fs::write(tmp_src.path().join("subdir/nested.txt"), "nested").unwrap();

        // Copy
        copy_dir_recursive(
            &tmp_src.path().to_string_lossy(),
            &tmp_dst.path().to_string_lossy(),
        )
        .unwrap();

        // Verify
        assert!(tmp_dst.path().join("file.txt").exists());
        assert!(tmp_dst.path().join("subdir/nested.txt").exists());
    }
}
