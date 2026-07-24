//! `subagent-start` hook handler.
//!
//! Records a `__subagent_start__` pseudo-access event for session tracking.
//! Extracted from `lifecycle.rs` as part of FIX-3 modularization.

use serde_json::Value;

use crate::runtime::HookRuntime;

/// subagent-start: record subagent invocation for session tracking.
///
/// Persists a pseudo `__subagent_start__` file_access row keyed by the
/// caller-provided `session_id`. This establishes a foothold in the access
/// log so later context-prewarm passes know a subagent owned part of the
/// session — useful when the parent agent compacts context and the child
/// agent's trace needs reconstruction.
pub(crate) fn handle_subagent_start(rt: &mut HookRuntime, input: &Value) -> String {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let _ = rt
        .ctx
        .knowledge
        .record_access("__subagent_start__", session_id);
    tracing::debug!(session_id, "subagent started — access recorded");

    String::new()
}
