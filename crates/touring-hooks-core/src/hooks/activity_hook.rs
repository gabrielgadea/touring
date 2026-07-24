//! D1.6 Activity Hook Integration — ESAA pattern event emission.
//!
//! Writes events to `<project>/.claude/touring/activity.jsonl` via direct
//! `EventStore` API (same process, no IPC/socket overhead).
//!
//! ## Events emitted
//! | Hook | EventAction | Actor | Payload |
//! |---|---|---|---|
//! | pre_edit | `HookFired` | `ClaudeCode` | `{file_path, context_len}` |
//! | post_edit | `HookFired` | `ClaudeCode` | `{file_path, session_id, edits}` |
//! | post_write | `HookFired` | `ClaudeCode` | `{file_path, language}` |
//! | instructions_loaded | `HookFired` | `ClaudeCode` | `{file_path: "~/.claude/..."}` |
//! | pre_compact | `HookFired` | `TouringDaemon` | `{linucb_saved, got_snapshot_saved}` |
//!
//! ## Error handling
//! Activity emission is fire-and-forget — hook handlers never fail due to
//! activity store errors. Errors are logged at debug/tracing level only.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, RwLock};

use touring_foundation::activity::event::{Actor, EventAction};
use touring_foundation::activity::store::EventStore;

/// Per-project EventStore cache: project_root → EventStore.
/// LazyLock defers HashMap allocation to first access (const-compatible).
static STORE_MAP: LazyLock<RwLock<HashMap<String, Arc<EventStore>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Get or create the EventStore for this project.
fn get_store(project_root: &Path) -> Option<Arc<EventStore>> {
    let key = project_root.to_string_lossy().into_owned();
    // Fast path: read-only peek first.
    {
        let map = STORE_MAP.read().ok()?;
        if let Some(store) = map.get(&key) {
            return Some(Arc::clone(store));
        }
    }
    // Slow path: exclusive write to insert new store.
    let mut map = STORE_MAP.write().ok()?;
    if let Some(store) = map.get(&key) {
        return Some(Arc::clone(store));
    }
    let path = project_root
        .join(".claude")
        .join("touring")
        .join("activity.jsonl");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let store = match EventStore::open(path) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::debug!("activity store open failed: {e}");
            return None;
        }
    };
    map.insert(key, Arc::clone(&store));
    Some(store)
}

/// Emit an activity event (fire-and-forget).
fn emit_event(
    project_root: &Path,
    action: EventAction,
    actor: &str,
    payload: Option<serde_json::Value>,
) {
    let store = match get_store(project_root) {
        Some(s) => s,
        None => return,
    };
    let actor = Actor::Agent(actor.to_string());
    if let Err(e) = store.append(action, actor, payload) {
        tracing::debug!("activity append failed: {e}");
    }
}

// ── Convenience emitters ───────────────────────────────────────────────────────

/// Emit pre_edit event.
pub fn emit_pre_edit(project_root: &Path, file_path: &str, context_len: usize) {
    emit_event(
        project_root,
        EventAction::HookFired,
        "ClaudeCode",
        Some(serde_json::json!({
            "hook": "pre_edit",
            "file_path": file_path,
            "context_len": context_len
        })),
    );
    crate::shared::gate_metrics::record_activity_pre_edit();
}

/// Emit post_edit event.
pub fn emit_post_edit(project_root: &Path, file_path: &str, session_id: &str, edit_count: usize) {
    emit_event(
        project_root,
        EventAction::HookFired,
        "ClaudeCode",
        Some(serde_json::json!({
            "hook": "post_edit",
            "file_path": file_path,
            "session_id": session_id,
            "edit_count": edit_count
        })),
    );
    crate::shared::gate_metrics::record_activity_post_edit();
}

/// Emit post_write event.
pub fn emit_post_write(project_root: &Path, file_path: &str, language: &str) {
    emit_event(
        project_root,
        EventAction::HookFired,
        "ClaudeCode",
        Some(serde_json::json!({
            "hook": "post_write",
            "file_path": file_path,
            "language": language
        })),
    );
    crate::shared::gate_metrics::record_activity_post_write();
}

/// Emit instructions_loaded event.
pub fn emit_instructions_loaded(project_root: &Path, session_id: &str) {
    emit_event(
        project_root,
        EventAction::HookFired,
        "ClaudeCode",
        Some(serde_json::json!({
            "hook": "instructions_loaded",
            "file_path": "~/.claude/...",
            "session_id": session_id
        })),
    );
    crate::shared::gate_metrics::record_activity_instructions_loaded();
}

/// Emit pre_compact event.
pub fn emit_pre_compact(project_root: &Path, linucb_saved: bool, got_snapshot_saved: bool) {
    emit_event(
        project_root,
        EventAction::HookFired,
        "TouringDaemon",
        Some(serde_json::json!({
            "hook": "pre_compact",
            "linucb_saved": linucb_saved,
            "got_snapshot_saved": got_snapshot_saved
        })),
    );
    crate::shared::gate_metrics::record_activity_pre_compact();
}

/// D4.6 — Emit boundary.violation event into S1 activity log.
///
/// Called when VGP Layer 5 (path boundary) detects a violation during
/// `speculate()`. Records `{file_path, task_kind, expected_pattern,
/// matched_forbidden_pattern}` so audit trails can trace which artifact was
/// blocked and why.
pub fn emit_boundary_violation(
    project_root: &Path,
    file_path: &str,
    task_kind: &str,
    violation_kind: &str,
    matched_pattern: &str,
) {
    emit_event(
        project_root,
        EventAction::BoundaryViolation,
        "VGP::Layer5::boundary",
        Some(serde_json::json!({
            "file_path": file_path,
            "task_kind": task_kind,
            "violation_kind": violation_kind,
            "matched_pattern": matched_pattern,
        })),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_pre_edit_does_not_panic() {
        let temp = std::env::temp_dir();
        // Fire-and-forget — no store exists, must not panic.
        emit_pre_edit(&temp, "src/main.rs", 512);
    }

    #[test]
    fn emit_post_edit_does_not_panic() {
        let temp = std::env::temp_dir();
        emit_post_edit(&temp, "src/main.rs", "session-abc", 3);
    }

    #[test]
    fn emit_post_write_does_not_panic() {
        let temp = std::env::temp_dir();
        emit_post_write(&temp, "src/main.rs", "rust");
    }

    #[test]
    fn emit_instructions_loaded_does_not_panic() {
        let temp = std::env::temp_dir();
        emit_instructions_loaded(&temp, "session-abc");
    }

    #[test]
    fn emit_pre_compact_does_not_panic() {
        let temp = std::env::temp_dir();
        emit_pre_compact(&temp, true, false);
    }
}
