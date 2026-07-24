//! `worktree-create` hook handler.
//!
//! Emitted when Claude Code creates a new git worktree (typically for an
//! isolated task). Records the path in the access log and returns a hint
//! suggesting follow-up wiring commands. Extracted from `lifecycle.rs` as
//! part of FIX-3 modularization.

use serde_json::Value;

use crate::hook_runtime::IsolationMode;
use crate::runtime::HookRuntime;

/// worktree-create: record a new worktree path and emit a wiring hint.
pub(crate) fn handle_worktree_create(rt: &mut HookRuntime, input: &Value) -> String {
    let worktree_path = input
        .get("worktree_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let intent = input
        .get("description")
        .or_else(|| input.get("intent"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if !worktree_path.is_empty() {
        // ES3 P5 — promote the runtime to Worktree isolation mode so all
        // subsequent `AccessDeclaration` paths are scoped (path-rewrite at
        // `txn::from_tool_payload_full`, see P5.3). Honest scope (CAH
        // roadmap §3): N>1 concurrent agents is capability-readiness, not
        // current demand.
        rt.isolation_mode = IsolationMode::Worktree(std::path::PathBuf::from(worktree_path));
        crate::shared::gate_metrics::record_worktree_isolation_active();
        tracing::info!(
            path = worktree_path,
            "worktree isolation activated (ES3 P5)"
        );
        let _ = rt
            .ctx
            .knowledge
            .record_access(worktree_path, "__worktree_create__");
        tracing::debug!(path = worktree_path, intent, "worktree created — recorded");
    }
    worktree_create_hint(worktree_path, intent)
}

/// R25-S2: Build wiring + generator hint for a new worktree (CC=3).
///
/// Returns empty for blank paths. Prepends a generator-kind suffix when the
/// intent matches a known keyword so Claude Code can scaffold the right
/// artifact immediately.
///
/// `pub(crate)` so tests in `lifecycle::tests` can exercise the helper
/// directly via `super::worktree_create_hint` (after the re-export in
/// `lifecycle.rs`). Re-export visibility must be ≤ item visibility, so the
/// item needs to be at least `pub(crate)`.
pub(crate) fn worktree_create_hint(worktree_path: &str, intent: &str) -> String {
    if worktree_path.is_empty() {
        return String::new();
    }
    let gen_hint = super::suggest_generator_for_task_subject(intent);
    // Fix-B2: gen_hint already starts with " | generator: ..." so use directly (no double-generator).
    let gen_suffix = gen_hint;
    format!(
        "worktree-created: {worktree_path}{gen_suffix} | \
        run `touring wiring score {worktree_path}` for integration score | \
        run `touring wiring suggest {worktree_path}` for orphan opportunities"
    )
}
