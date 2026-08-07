//! Shared helper utilities extracted from repeated patterns across hook handlers.
//!
//! These helpers eliminate Type-1 duplicate blocks that appeared identically
//! in 3–6 handler files. Each helper is behavior-preserving and tested via the
//! handlers that call it.
//!
//! # Feature-gated usage
//! The callers are behind `#[cfg(feature = "pre-hooks")]` / `"post-hooks"` in
//! `lib.rs`. When those features are absent the functions appear unused, but they
//! ARE called under the correct feature combination.
#![allow(dead_code)]

use touring_hook_runtime::HookRuntime;
use touring_hooks_core::knowledge::FileKnowledgeEnriched;
use touring_intelligence::rl::QTable;

/// Extract the current session CILA level from the hook runtime.
///
/// Tries `stable_session` first (fast path when session context is already
/// established). Falls back to the result cache key
/// `("__meta__", "__session_cila_level__")` for cold-start or new sessions.
/// Returns `default` when neither source is available.
///
/// # Arguments
/// * `runtime` — shared reference to the hook runtime
/// * `default` — fallback level when CILA cannot be determined (typically 3)
pub(crate) fn cila_level_from_runtime(runtime: &HookRuntime, default: u8) -> u8 {
    runtime
        .ctx
        .stable_session
        .borrow()
        .as_ref()
        .map(|s| s.cila_level)
        .unwrap_or_else(|| {
            runtime
                .ctx
                .result_cache
                .get_result("__meta__", "__session_cila_level__")
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        })
}

/// Ensure the QTable is loaded into `runtime.learning.qtable_cache`.
///
/// Idempotent — does nothing when the cache is already populated.
/// On first call, reads from `.claude/data/qtable.rkyv` if the file exists,
/// otherwise initialises with `QTable::new()`.
///
/// # Arguments
/// * `runtime` — mutable reference to the hook runtime (modifies `learning.qtable_cache`)
pub(crate) fn ensure_qtable_loaded(runtime: &mut HookRuntime) {
    let qtable_path = runtime.project_root.join(".claude/data/qtable.rkyv");
    if runtime.learning.qtable_cache.is_none() {
        let mut qt = QTable::new();
        if qtable_path.exists()
            && let Ok((loaded, _rev)) = QTable::load_rkyv(&qtable_path)
        {
            qt = loaded;
        }
        runtime.learning.qtable_cache = Some(qt);
    }
}

/// Build a `"file-meta: ..."` signal string from extended file knowledge.
///
/// Currently encodes `coverage_pct` and `community_id` when present.
/// Returns `None` when neither field is available (avoids injecting empty noise).
///
/// # Arguments
/// * `ext` — extended file knowledge record from `FileKnowledgeDB::query_extended`
pub(crate) fn build_file_meta_signal(ext: &FileKnowledgeEnriched) -> Option<String> {
    let mut meta: Vec<String> = Vec::new();
    if let Some(cov) = ext.coverage_pct {
        meta.push(format!("coverage:{:.0}%", cov));
    }
    if let Some(cid) = ext.community_id {
        meta.push(format!("community:{}", cid));
    }
    if meta.is_empty() {
        None
    } else {
        Some(format!("file-meta: {}", meta.join(" ")))
    }
}
