//! WriteTriadHook — TRIAD pattern (execute + validate + rollback) for Write operations.
//!
//! Implements the TRIAD pattern from ACO `generator_engine.py`:
//! 1. **pre_write**: snapshot files via BranchFs before writing
//! 2. **post_write**: validate written content; rollback on failure
//! 3. **rollback**: restore snapshots if validation fails
//!
//! This hook runs alongside the existing `pre_write` and `post_write` hooks.
//! The TRIAD logic is additive — it enhances, not replaces, existing validation.

use crate::branch_fs::BranchFs;
use crate::errors::Result;
use crate::runtime::{HookResponse, HookRuntime};
use std::path::Path;

/// TRIAD state stored in HookRuntime for the duration of a write operation.
///
/// One instance per write call — cleared after post_write completes.
#[derive(Debug)]
pub struct TriadState {
    /// Snapshot of files before the write operation.
    pub snapshot: BranchFs,
    /// Path that was written.
    pub written_path: String,
    /// Whether the write has been validated (success) or failed (rollback).
    pub validated: bool,
}

impl TriadState {
    /// Create a new TRIAD state by snapshotting the target file.
    ///
    /// Snapshots the file at `file_path` before any write occurs.
    /// If the file doesn't exist, it records the absence.
    pub fn pre_write(file_path: &str) -> Result<Self> {
        let path = Path::new(file_path);
        let snapshot = BranchFs::new(&[path])?;

        Ok(Self {
            snapshot,
            written_path: file_path.to_string(),
            validated: false,
        })
    }

    /// Mark the sequence as validated (success path).
    ///
    /// Calling this after a successful post_write commits the snapshot
    /// (discards it). The file content is kept.
    pub fn mark_validated(mut self) {
        self.validated = true;
        // Commit: drop snapshot without restoring
        self.snapshot.commit();
    }

    /// Rollback the write operation.
    ///
    /// Restores the file to its pre-write state.
    /// Should be called when post_write validation fails.
    pub fn rollback(&self) -> Result<()> {
        Ok(self.snapshot.restore()?)
    }
}

/// Run the TRIAD pre-write phase.
///
/// Call at the start of a write operation. Returns `Some(TriadState)` on success,
/// `None` if snapshotting failed (in which case, proceed without TRIAD protection).
pub fn run_pre_write(file_path: &str) -> Option<TriadState> {
    match TriadState::pre_write(file_path) {
        Ok(state) => {
            tracing::debug!(
                path = %file_path,
                branch_id = %state.snapshot.metadata().branch_id,
                "TRIAD: pre_write snapshot created"
            );
            Some(state)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %file_path,
                "TRIAD: pre_write snapshot failed — proceeding without protection"
            );
            None
        }
    }
}

/// Run the TRIAD post-write validation and commit/rollback.
///
/// Call after post_write validation. If `validation_passed` is true, commits
/// the snapshot (success). If false, rolls back to the pre-write state.
pub fn run_post_write(
    state: &TriadState,
    validation_passed: bool,
    _runtime: Option<&HookRuntime>,
) -> HookResponse {
    if validation_passed {
        tracing::debug!(
            path = %state.written_path,
            branch_id = %state.snapshot.metadata().branch_id,
            "TRIAD: post_write validation passed"
        );
        // Success: do nothing. Dropping TriadState drops BranchFs without restore.
        HookResponse::Allow
    } else {
        tracing::warn!(
            path = %state.written_path,
            branch_id = %state.snapshot.metadata().branch_id,
            "TRIAD: post_write validation failed — rolling back"
        );
        match state.rollback() {
            Ok(()) => {
                let msg = format!(
                    "TRIAD rollback: file restored to pre-write state: {}",
                    state.written_path
                );
                HookResponse::Context {
                    context: msg,
                    event_name: Some("TriadRollback".to_string()),
                }
            }
            Err(e) => {
                let msg = format!(
                    "TRIAD rollback FAILED: {} — manual intervention may be required for {}",
                    e, state.written_path
                );
                tracing::error!(error = %e, path = %state.written_path, "TRIAD: rollback failed");
                HookResponse::Context {
                    context: msg,
                    event_name: Some("TriadRollbackFailed".to_string()),
                }
            }
        }
    }
}

/// Check if a file has drifted since the TRIAD snapshot was taken.
///
/// Use this in post_write to detect whether the file was modified externally
/// (e.g., by a linter) after the snapshot.
pub fn check_drift(state: &TriadState) -> Option<bool> {
    let path = Path::new(&state.written_path);
    state.snapshot.has_drifted(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_pre_write_snapshot_creates_state() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("test.txt");
        fs::write(&file, b"original content").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");
        assert!(!state.validated);
        assert_eq!(state.written_path, file.to_str().unwrap());
        state.snapshot.commit();
    }

    #[test]
    fn test_pre_write_absent_file() {
        let dir = tempdir().expect("tempdir");
        let absent = dir.path().join("nonexistent.txt");

        let state = TriadState::pre_write(absent.to_str().unwrap()).expect("pre_write");
        assert!(state.snapshot.was_absent(&absent));
        state.snapshot.commit();
    }

    #[test]
    fn test_rollback_restores_content() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("rollback_test.txt");
        fs::write(&file, b"original").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        // Simulate a failed write (content not properly written)
        fs::write(&file, b"corrupted").expect("write");

        // Rollback should restore original
        state.rollback().expect("rollback");
        assert_eq!(fs::read(&file).expect("read"), b"original");
    }

    #[test]
    fn test_rollback_deletes_created_file() {
        let dir = tempdir().expect("tempdir");
        let new_file = dir.path().join("new_file.txt");

        // File doesn't exist yet
        let state = TriadState::pre_write(new_file.to_str().unwrap()).expect("pre_write");

        // Simulate creating the file
        fs::write(&new_file, b"new content").expect("write");
        assert!(new_file.exists());

        // Rollback should delete it
        state.rollback().expect("rollback");
        assert!(!new_file.exists());
    }

    #[test]
    fn test_mark_validated_commits() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("commit_test.txt");
        fs::write(&file, b"content").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        // Modify file after pre_write
        fs::write(&file, b"modified").expect("write");

        // Mark validated should keep the modification
        state.mark_validated();
        assert_eq!(fs::read(&file).expect("read"), b"modified");
    }

    #[test]
    fn test_check_drift_detects_change() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("drift.txt");
        fs::write(&file, b"original").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        // Modify after snapshot
        fs::write(&file, b"changed").expect("write");

        let drifted = check_drift(&state);
        assert_eq!(drifted, Some(true));

        state.snapshot.commit();
    }

    #[test]
    fn test_check_drift_no_change() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("no_drift.txt");
        fs::write(&file, b"stable").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        let drifted = check_drift(&state);
        assert_eq!(drifted, Some(false));

        state.snapshot.commit();
    }

    #[test]
    fn test_run_pre_write_proceeds_without_protection_on_error() {
        // Passing a directory (not a file) should return None but not panic
        let dir = tempdir().expect("tempdir");
        let result = run_pre_write(dir.path().to_str().unwrap());
        // Directories don't exist as files, so snapshot may fail
        // The function should return None gracefully
        assert!(result.is_none());
    }

    #[test]
    fn test_run_post_write_allow_on_success() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("post_write.txt");
        fs::write(&file, b"ok").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        // Success path: run_post_write returns HookResponse::Allow without using runtime
        let response = run_post_write(&state, true, None);
        match response {
            HookResponse::Allow => {}
            other => panic!("expected Allow, got {:?}", other),
        }

        // Commit the snapshot
        let _ = state.snapshot.commit();
    }
}
