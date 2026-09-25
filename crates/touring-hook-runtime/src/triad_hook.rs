//! WriteTriadHook — TRIAD pattern (snapshot + validate + describe) for Write operations.
//!
//! 1. **pre_write**: snapshot the target via BranchFs and ARM it in a per-process store
//!    keyed by `(session_id, canonical path)` — [`arm`].
//! 2. **post_write**: TAKE the pair's own snapshot — [`take`] — and let the validation
//!    verdict through; on `Block` the verdict gains a `+N −M lines` summary of the write.
//!
//! **D1-A (approved 25/09/2026): the hook path never restores a file.** Claude Code's
//! PostToolUse "can't undo actions"; rolling back behind the model broke the next Edit's
//! `old_string`, and the audit reproduced a CLEAN write reverted (v1 → v2 → v1) because an
//! advisory context counted as a failure. [`TriadState::rollback`] stays for explicit,
//! human-invoked tooling only.
//!
//! **Why a keyed store**: the previous single `RefCell` slot on `HookRuntime` was shared by
//! every session served by the daemon — the post-write of file A consumed (and rolled back)
//! the snapshot of file B. The key makes a post-write see only its own pre-write.

use crate::branch_fs::BranchFs;
use crate::errors::Result;
use crate::runtime::HookResponse;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// Snapshots older than this are dropped (their TempDir is cleaned); a post-write without
/// its pair is a plain pass-through.
pub const TRIAD_TTL: Duration = Duration::from_secs(600);
/// Upper bound on concurrently armed snapshots (all sessions of one daemon).
pub const TRIAD_MAX_ARMED: u64 = 4_096;
/// Files larger than this get no line summary (the snapshot still exists).
const DIFF_SUMMARY_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Key of an armed snapshot: `(session_id, canonical path)`.
pub type TriadKey = (String, PathBuf);

fn triad_store() -> &'static moka::sync::Cache<TriadKey, Arc<TriadState>> {
    static STORE: OnceLock<moka::sync::Cache<TriadKey, Arc<TriadState>>> = OnceLock::new();
    STORE.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(TRIAD_MAX_ARMED)
            .time_to_live(TRIAD_TTL)
            .build()
    })
}

/// Session id from a hook payload; a payload without one gets its own `"anon"` key,
/// never another session's.
pub fn session_of(input: &serde_json::Value) -> String {
    input
        .get("session_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("anon")
        .to_string()
}

/// Canonical key path: absolute, with the PARENT canonicalized (the file itself may not
/// exist yet at pre-write), so `rel/x.py`, `/abs/x.py` and a symlinked parent agree.
pub fn canonical_path(file_path: &str) -> PathBuf {
    let raw = Path::new(file_path);
    let abs = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(raw))
            .unwrap_or_else(|_| raw.to_path_buf())
    };
    match (abs.parent(), abs.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent)
            .map(|p| p.join(name))
            .unwrap_or(abs.clone()),
        _ => abs,
    }
}

/// Snapshot `file_path` and arm it for `session_id`. Returns false (and arms nothing)
/// when the snapshot cannot be taken — the write proceeds unprotected, never blocked.
/// A second pre-write of the same (session, path) replaces the first (last pre wins).
pub fn arm(session_id: &str, file_path: &str) -> bool {
    match run_pre_write(file_path) {
        Some(state) => {
            triad_store().insert(
                (session_id.to_string(), canonical_path(file_path)),
                Arc::new(state),
            );
            true
        }
        None => false,
    }
}

/// Remove and return the snapshot armed by `session_id` for `file_path` — ONLY that pair.
pub fn take(session_id: &str, file_path: &str) -> Option<Arc<TriadState>> {
    triad_store().remove(&(session_id.to_string(), canonical_path(file_path)))
}

/// TRIAD state for one write operation (one armed snapshot).
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

/// TRIAD post-write: pass the validation verdict through WITHOUT touching the disk (D1-A).
///
/// `Allow` and advisory `Context` are returned unchanged — an advisory note is not a
/// failure. On `Block` the verdict is kept and gains a one-line description of what the
/// write changed (`+N −M lines vs pre-write snapshot`), so the model fixes forward knowing
/// the content IS on disk. The event name is always the real one, `PostToolUse`.
/// The snapshot is discarded when the caller drops its `Arc<TriadState>`.
pub fn run_post_write(state: &TriadState, response: HookResponse) -> HookResponse {
    match response {
        HookResponse::Block {
            reason, context, ..
        } => {
            let note = describe_write(state);
            tracing::info!(path = %state.written_path, %note, "TRIAD: blocked write kept on disk");
            let context = Some(match context {
                Some(c) if !c.is_empty() => format!("{c}\n{note}"),
                _ => note,
            });
            HookResponse::Block {
                reason,
                context,
                event_name: Some("PostToolUse".to_string()),
            }
        }
        other => other,
    }
}

/// `TRIAD: write kept on disk … +N −M lines vs pre-write snapshot` (never panics).
pub fn describe_write(state: &TriadState) -> String {
    let head = "TRIAD: write kept on disk (PostToolUse cannot undo; fix forward)";
    let path = Path::new(&state.written_path);
    let before = match state.snapshot.snapshot_content(path) {
        Ok(b) => b.unwrap_or_default(),
        Err(e) => {
            tracing::warn!(error = %e, path = %state.written_path, "TRIAD: snapshot unreadable");
            return format!("{head} — line summary unavailable ({e})");
        }
    };
    let after = match std::fs::read(path) {
        Ok(a) => a,
        Err(e) => return format!("{head} — line summary unavailable ({e})"),
    };
    if before.len().max(after.len()) > DIFF_SUMMARY_MAX_BYTES {
        return format!("{head} — file above {DIFF_SUMMARY_MAX_BYTES} bytes, no line summary");
    }
    let (added, removed) = line_delta(
        &String::from_utf8_lossy(&before),
        &String::from_utf8_lossy(&after),
    );
    format!("{head} — +{added} −{removed} lines vs pre-write snapshot")
}

/// Multiset line delta: how many lines the new text has beyond the old (added) and the
/// old beyond the new (removed). O(n + m); exact counts, not an edit script.
pub fn line_delta(before: &str, after: &str) -> (usize, usize) {
    let mut balance: HashMap<&str, isize> = HashMap::new();
    for line in before.lines() {
        *balance.entry(line).or_insert(0) -= 1;
    }
    for line in after.lines() {
        *balance.entry(line).or_insert(0) += 1;
    }
    balance.values().fold((0, 0), |(add, rem), &v| {
        if v > 0 {
            (add + v.unsigned_abs(), rem)
        } else {
            (add, rem + v.unsigned_abs())
        }
    })
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

    /// S12 / D1-A (25/09/2026): PostToolUse cannot undo a tool call (Claude Code docs), so the
    /// TRIAD never touches the disk. The audit reproduced v1 → v2 → **v1** on a CLEAN write
    /// because an advisory post-write context counted as "validation failed".
    #[test]
    fn post_write_advisory_context_never_touches_disk() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("clean.py");
        fs::write(&file, b"v1\n").expect("write v1");
        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");
        fs::write(&file, b"v2\n").expect("write v2");
        let advisory = HookResponse::Context {
            context: "post-write: 2 issue(s)".into(),
            event_name: None,
        };
        let _ = run_post_write(&state, advisory);
        assert_eq!(
            fs::read(&file).expect("read"),
            b"v2\n",
            "a post-write verdict must never revert the file"
        );
        let block = HookResponse::Block {
            reason: "4 new anti-patterns".into(),
            context: None,
            event_name: None,
        };
        let _ = run_post_write(&state, block);
        assert_eq!(
            fs::read(&file).expect("read"),
            b"v2\n",
            "not even a Block reverts the file"
        );
    }

    #[test]
    fn advisory_context_passes_through_unchanged() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("ctx.py");
        fs::write(&file, b"a\n").expect("write");
        let state = TriadState::pre_write(file.to_str().expect("utf8")).expect("pre_write");
        let out = run_post_write(
            &state,
            HookResponse::Context {
                context: "note".into(),
                event_name: None,
            },
        );
        match out {
            HookResponse::Context {
                context,
                event_name,
            } => {
                assert_eq!(context, "note");
                assert_eq!(event_name, None);
            }
            other => panic!("expected the advisory Context back, got {other:?}"),
        }
    }

    #[test]
    fn triad_response_uses_posttooluse_event_name() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("blocked.py");
        fs::write(&file, b"keep\nold\n").expect("write v1");
        let state = TriadState::pre_write(file.to_str().expect("utf8")).expect("pre_write");
        fs::write(&file, b"keep\nnew1\nnew2\n").expect("write v2");
        let out = run_post_write(
            &state,
            HookResponse::Block {
                reason: "r".into(),
                context: None,
                event_name: None,
            },
        );
        let HookResponse::Block {
            reason,
            context,
            event_name,
        } = out
        else {
            panic!("a Block verdict must stay a Block");
        };
        assert_eq!(reason, "r");
        assert_eq!(event_name.as_deref(), Some("PostToolUse"));
        let note = context.expect("the block gains the write description");
        assert!(note.contains("+2 −1 lines"), "{note}");
        assert!(
            !note.contains("Triad") && !note.contains("rollback"),
            "{note}"
        );
    }

    #[test]
    fn triad_state_is_keyed_by_session_and_path() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("shared.py");
        fs::write(&file, b"x\n").expect("write");
        let path = file.to_str().expect("utf8");
        assert!(arm("keyed-s1", path));
        assert!(arm("keyed-s2", path));
        assert!(
            take("keyed-s1", path).is_some(),
            "session 1 finds its own snapshot"
        );
        assert!(take("keyed-s1", path).is_none(), "taking consumes it");
        assert!(
            take("keyed-s2", path).is_some(),
            "session 2's snapshot survived session 1's post-write"
        );
    }

    #[test]
    fn post_write_of_a_never_consumes_snapshot_of_b() {
        let dir = tempdir().expect("tempdir");
        let a = dir.path().join("a.py");
        let b = dir.path().join("b.py");
        fs::write(&a, b"a\n").expect("write a");
        fs::write(&b, b"b\n").expect("write b");
        assert!(arm("ab-sess", a.to_str().expect("utf8")));
        assert!(arm("ab-sess", b.to_str().expect("utf8")));
        let got = take("ab-sess", a.to_str().expect("utf8")).expect("a armed");
        assert_eq!(got.written_path, a.to_str().expect("utf8"));
        assert!(
            take("ab-sess", b.to_str().expect("utf8")).is_some(),
            "b is still armed"
        );
    }

    #[test]
    fn relative_and_absolute_paths_share_one_key() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("rel.py");
        fs::write(&file, b"r\n").expect("write");
        let abs = file.to_str().expect("utf8").to_string();
        let dotted = format!("{}/./rel.py", dir.path().display());
        assert!(arm("rel-sess", &dotted));
        assert!(
            take("rel-sess", &abs).is_some(),
            "`dir/./rel.py` and `dir/rel.py` must meet"
        );
    }

    #[test]
    fn missing_session_id_gets_its_own_anon_key() {
        assert_eq!(session_of(&serde_json::json!({})), "anon");
        assert_eq!(session_of(&serde_json::json!({"session_id": ""})), "anon");
        assert_eq!(session_of(&serde_json::json!({"session_id": "s-9"})), "s-9");
    }

    #[test]
    fn line_delta_counts_added_and_removed_lines() {
        assert_eq!(line_delta("a\nb\nc\n", "a\nc\nd\ne\n"), (2, 1));
        assert_eq!(line_delta("", "x\n"), (1, 0));
        assert_eq!(line_delta("same\n", "same\n"), (0, 0));
    }

    #[test]
    fn test_run_post_write_allow_on_success() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("post_write.txt");
        fs::write(&file, b"ok").expect("write");

        let state = TriadState::pre_write(file.to_str().unwrap()).expect("pre_write");

        // Success path: an Allow verdict passes through as Allow
        let response = run_post_write(&state, HookResponse::Allow);
        match response {
            HookResponse::Allow => {}
            other => panic!("expected Allow, got {:?}", other),
        }

        // Commit the snapshot
        let _ = state.snapshot.commit();
    }
}
