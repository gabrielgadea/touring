//! Copy-on-write file snapshots for safe edits.
//!
//! Creates snapshots of files before risky operations.
//! On failure, call [`BranchFs::restore`] to return to the pre-edit state.
//! On success, call [`BranchFs::commit`] to discard the snapshot.
//!
//! Implementation is 100% userspace — no overlayfs, no root required.
//! Uses [`tempfile::TempDir`] for automatic cleanup on drop.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use crate::shared::ResultExt;
use touring_hooks_shared::errors::TouringError;

/// Copy-on-write file snapshot for safe edits.
///
/// Creates snapshots of files before risky operations.
/// Allows atomic restore if the operation fails.
///
/// # Lifecycle
///
/// ```text
/// BranchFs::new(files) → [perform edits] → commit()   // success path
///                                        → restore()  // failure path
///                                        → drop       // implicit rollback (TempDir cleaned)
/// ```
#[derive(Debug)]
pub struct BranchFs {
    snapshot_dir: TempDir,
    /// Maps original path → snapshot copy path (None = file was absent at branch time).
    snapshots: HashMap<PathBuf, Option<PathBuf>>,
    metadata: BranchMetadata,
}

/// Metadata describing a branch snapshot.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BranchMetadata {
    /// Unique identifier for this branch.
    pub branch_id: String,
    /// Unix timestamp (seconds) when the branch was created.
    pub created_at: u64,
    /// List of snapshotted files with integrity information.
    pub files: Vec<SnapshotEntry>,
}

/// Information about a single snapshotted file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotEntry {
    /// Original file path (absolute).
    pub original_path: String,
    /// SHA-256 hex digest of file content at snapshot time, or `None` if file was absent.
    pub content_hash: Option<String>,
    /// File size in bytes (0 if absent).
    pub size_bytes: u64,
}

impl BranchFs {
    /// Create a snapshot of the given files.
    ///
    /// Files that don't exist are recorded as "absent" — [`restore`](Self::restore) will
    /// delete them if they were created after the branch.
    #[must_use = "BranchFs must be committed or restored — dropping discards the snapshot"]
    pub fn new(files: &[&Path]) -> Result<Self, BranchFsError> {
        let snapshot_dir = TempDir::new().map_err(BranchFsError::TempDir)?;

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            // EC57: ResultExt::unwrap_or_debug — logs if system clock is before UNIX_EPOCH.
            .unwrap_or_debug(
                std::time::Duration::ZERO,
                "branch_fs: system clock before UNIX_EPOCH",
            )
            .as_secs();

        let branch_id = format!("branch-{created_at}-{}", std::process::id());

        let mut snapshots = HashMap::new();
        let mut entries = Vec::new();

        for (idx, &orig) in files.iter().enumerate() {
            let abs = if orig.is_absolute() {
                orig.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(BranchFsError::Cwd)?
                    .join(orig)
            };

            if abs.exists() {
                let content = std::fs::read(&abs).map_err(|e| BranchFsError::Read {
                    path: abs.display().to_string(),
                    source: e,
                })?;
                let hash = format!("{:x}", Sha256::digest(&content));
                let size = content.len() as u64;

                let snap_name = format!("{idx}");
                let snap_path = snapshot_dir.path().join(snap_name);
                std::fs::copy(&abs, &snap_path).map_err(|e| BranchFsError::Copy {
                    path: abs.display().to_string(),
                    source: e,
                })?;

                entries.push(SnapshotEntry {
                    original_path: abs.display().to_string(),
                    content_hash: Some(hash),
                    size_bytes: size,
                });
                snapshots.insert(abs, Some(snap_path));
            } else {
                // File absent — record so restore() can delete it if created later.
                entries.push(SnapshotEntry {
                    original_path: abs.display().to_string(),
                    content_hash: None,
                    size_bytes: 0,
                });
                snapshots.insert(abs, None);
            }
        }

        Ok(Self {
            snapshot_dir,
            snapshots,
            metadata: BranchMetadata {
                branch_id,
                created_at,
                files: entries,
            },
        })
    }

    /// Restore all snapshotted files to their state at branch creation.
    ///
    /// - Files that existed are overwritten with the snapshot copy.
    /// - Files that were absent are deleted if they now exist.
    #[must_use = "check the Result to ensure restore succeeded"]
    pub fn restore(&self) -> Result<(), BranchFsError> {
        for (orig, snap) in &self.snapshots {
            match snap {
                Some(snap_path) => {
                    // Ensure parent directory exists (may have been deleted).
                    if let Some(parent) = orig.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| BranchFsError::Mkdir {
                            path: parent.display().to_string(),
                            source: e,
                        })?;
                    }
                    std::fs::copy(snap_path, orig).map_err(|e| BranchFsError::Restore {
                        path: orig.display().to_string(),
                        source: e,
                    })?;
                }
                None => {
                    // Was absent at branch time — delete if now exists.
                    if orig.exists() {
                        std::fs::remove_file(orig).map_err(|e| BranchFsError::Remove {
                            path: orig.display().to_string(),
                            source: e,
                        })?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Commit: keep current files, discard snapshot.
    ///
    /// After commit, [`restore`](Self::restore) is no longer possible.
    /// The snapshot directory is automatically cleaned up.
    pub fn commit(self) {
        tracing::debug!(
            branch_id = %self.metadata.branch_id,
            files = self.snapshots.len(),
            "BranchFs committed — snapshot discarded"
        );
        // self.snapshot_dir dropped here — TempDir cleanup is automatic.
    }

    /// Check if a file has changed since the snapshot was taken.
    ///
    /// - For files that existed: compares SHA-256 of current content vs snapshot.
    /// - For files that were absent: returns `true` if the file now exists.
    ///
    /// Returns an error if `path` was not part of this branch.
    pub fn has_drifted(&self, path: &Path) -> Result<bool, BranchFsError> {
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(BranchFsError::Cwd)?
                .join(path)
        };

        let abs_str = abs.display().to_string();
        let entry = self
            .metadata
            .files
            .iter()
            .find(|e| e.original_path == abs_str)
            .ok_or_else(|| BranchFsError::NotInSnapshot(path.display().to_string()))?;

        match &entry.content_hash {
            None => {
                // Was absent — drifted if file now exists.
                Ok(abs.exists())
            }
            Some(original_hash) => {
                if !abs.exists() {
                    // File existed at snapshot time but is now gone.
                    return Ok(true);
                }
                let content = std::fs::read(&abs).map_err(|e| BranchFsError::Read {
                    path: abs.display().to_string(),
                    source: e,
                })?;
                let current_hash = format!("{:x}", Sha256::digest(&content));
                Ok(current_hash != *original_hash)
            }
        }
    }

    /// List all snapshotted file paths.
    pub fn snapshotted_files(&self) -> Vec<&Path> {
        self.snapshots.keys().map(PathBuf::as_path).collect()
    }

    /// Branch metadata (id, timestamp, file list with hashes).
    pub fn metadata(&self) -> &BranchMetadata {
        &self.metadata
    }

    /// Check if a path was absent (did not exist) when the branch was created.
    pub fn was_absent(&self, path: &Path) -> bool {
        self.snapshots.get(path).is_some_and(|v| v.is_none())
    }

    /// Get the snapshot directory path (useful for diagnostics).
    pub fn snapshot_dir(&self) -> &Path {
        self.snapshot_dir.path()
    }
}

/// Errors from [`BranchFs`] snapshot, restore, and drift-check operations.
#[derive(Debug, thiserror::Error)]
pub enum BranchFsError {
    /// Could not create the temporary snapshot directory.
    #[error("BranchFs: cannot create temp dir: {0}")]
    TempDir(#[source] std::io::Error),
    /// Could not resolve the current working directory for a relative path.
    #[error("BranchFs: cannot resolve cwd: {0}")]
    Cwd(#[source] std::io::Error),
    /// Could not read a file being snapshotted or drift-checked.
    #[error("BranchFs: read {path}: {source}")]
    Read {
        /// Absolute path that failed to read.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not copy a file into the snapshot directory.
    #[error("BranchFs: copy {path}: {source}")]
    Copy {
        /// Absolute path that failed to copy.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not recreate a parent directory during restore.
    #[error("BranchFs: mkdir {path}: {source}")]
    Mkdir {
        /// Parent directory path that failed to create.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not restore a snapshotted file to its original location.
    #[error("BranchFs: restore {path}: {source}")]
    Restore {
        /// Original path that failed to restore.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not remove a file that was absent when the branch was created.
    #[error("BranchFs: remove {path}: {source}")]
    Remove {
        /// Path that failed to remove.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The requested path was not part of this branch's snapshot.
    #[error("BranchFs: path not in snapshot: {0}")]
    NotInSnapshot(String),
}

impl From<BranchFsError> for TouringError {
    fn from(e: BranchFsError) -> Self {
        // Preserves the prior `From<String> for TouringError` behaviour: these
        // were formatted `BranchFs: …` strings that became `TouringError::Hook`
        // when propagated via `?` (e.g. in `touring-hook-runtime::triad_hook`).
        TouringError::Hook(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_branch_new_snapshots_existing_file() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("test.txt");
        fs::write(&file, b"original").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        assert_eq!(branch.snapshotted_files().len(), 1);
        assert!(!branch.was_absent(&file));
        branch.commit();
    }

    #[test]
    fn test_branch_absent_file_recorded() {
        let dir = tempdir().expect("tempdir");
        let absent = dir.path().join("nonexistent.txt");

        let branch = BranchFs::new(&[absent.as_path()]).expect("new");
        assert!(branch.was_absent(&absent));
        branch.commit();
    }

    #[test]
    fn test_restore_recovers_original_content() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("file.txt");
        fs::write(&file, b"original").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        fs::write(&file, b"modified").expect("write modified");
        branch.restore().expect("restore");

        assert_eq!(fs::read(&file).expect("read"), b"original");
    }

    #[test]
    fn test_restore_deletes_files_that_were_absent() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("new_file.txt");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        fs::write(&file, b"created after branch").expect("write");
        assert!(file.exists());

        branch.restore().expect("restore");
        assert!(!file.exists());
    }

    #[test]
    fn test_has_drifted_returns_false_unchanged() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("a.txt");
        fs::write(&file, b"same").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        assert!(!branch.has_drifted(&file).expect("drift check"));
        branch.commit();
    }

    #[test]
    fn test_has_drifted_returns_true_after_change() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("b.txt");
        fs::write(&file, b"original").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        fs::write(&file, b"changed").expect("write changed");
        assert!(branch.has_drifted(&file).expect("drift check"));
        branch.commit();
    }

    #[test]
    fn test_has_drifted_absent_then_created() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("c.txt");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        fs::write(&file, b"new content").expect("write");
        assert!(branch.has_drifted(&file).expect("drift check"));
        branch.commit();
    }

    #[test]
    fn test_has_drifted_existed_then_deleted() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("del.txt");
        fs::write(&file, b"will be deleted").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        fs::remove_file(&file).expect("remove");
        assert!(branch.has_drifted(&file).expect("drift check"));
        branch.commit();
    }

    #[test]
    fn test_metadata_branch_id_is_set() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("d.txt");
        fs::write(&file, b"x").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        assert!(branch.metadata().branch_id.starts_with("branch-"));
        assert!(branch.metadata().created_at > 0);
        branch.commit();
    }

    #[test]
    fn test_metadata_files_count() {
        let dir = tempdir().expect("tempdir");
        let f1 = dir.path().join("e.txt");
        let f2 = dir.path().join("f.txt");
        fs::write(&f1, b"a").expect("write f1");
        fs::write(&f2, b"b").expect("write f2");

        let branch = BranchFs::new(&[f1.as_path(), f2.as_path()]).expect("new");
        assert_eq!(branch.metadata().files.len(), 2);
        branch.commit();
    }

    #[test]
    fn test_commit_keeps_current_content() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("g.txt");
        fs::write(&file, b"original").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        fs::write(&file, b"modified").expect("write modified");
        branch.commit(); // consume — no restore possible

        assert_eq!(fs::read(&file).expect("read"), b"modified");
    }

    #[test]
    fn test_snapshot_entry_has_hash_for_existing_file() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("h.txt");
        fs::write(&file, b"content").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        let entry = branch.metadata().files.first().expect("first entry");
        assert!(entry.content_hash.is_some());
        assert_eq!(entry.size_bytes, 7); // "content" = 7 bytes
        branch.commit();
    }

    #[test]
    fn test_snapshot_entry_absent_has_no_hash() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("absent.txt");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        let entry = branch.metadata().files.first().expect("first entry");
        assert!(entry.content_hash.is_none());
        assert_eq!(entry.size_bytes, 0);
        branch.commit();
    }

    #[test]
    fn test_restore_multiple_files() {
        let dir = tempdir().expect("tempdir");
        let f1 = dir.path().join("i.txt");
        let f2 = dir.path().join("j.txt");
        fs::write(&f1, b"f1").expect("write f1");
        fs::write(&f2, b"f2").expect("write f2");

        let branch = BranchFs::new(&[f1.as_path(), f2.as_path()]).expect("new");
        fs::write(&f1, b"f1-modified").expect("write f1 mod");
        fs::write(&f2, b"f2-modified").expect("write f2 mod");
        branch.restore().expect("restore");

        assert_eq!(fs::read(&f1).expect("read f1"), b"f1");
        assert_eq!(fs::read(&f2).expect("read f2"), b"f2");
    }

    #[test]
    fn test_restore_recreates_parent_dir_if_deleted() {
        let dir = tempdir().expect("tempdir");
        let subdir = dir.path().join("sub");
        fs::create_dir(&subdir).expect("mkdir");
        let file = subdir.join("nested.txt");
        fs::write(&file, b"nested content").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        fs::remove_file(&file).expect("remove file");
        fs::remove_dir(&subdir).expect("remove dir");
        assert!(!subdir.exists());

        branch.restore().expect("restore");
        assert!(file.exists());
        assert_eq!(fs::read(&file).expect("read"), b"nested content");
    }

    #[test]
    fn test_has_drifted_error_for_unknown_path() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("known.txt");
        let unknown = dir.path().join("unknown.txt");
        fs::write(&file, b"x").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        let result = branch.has_drifted(&unknown);
        assert!(result.is_err());
        branch.commit();
    }

    #[test]
    fn test_empty_file_list() {
        let branch = BranchFs::new(&[]).expect("new empty");
        assert!(branch.snapshotted_files().is_empty());
        assert!(branch.metadata().files.is_empty());
        branch.commit();
    }

    #[test]
    fn test_snapshot_dir_exists_during_lifetime() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("k.txt");
        fs::write(&file, b"data").expect("write");

        let branch = BranchFs::new(&[file.as_path()]).expect("new");
        assert!(branch.snapshot_dir().exists());
        branch.commit();
    }
}
