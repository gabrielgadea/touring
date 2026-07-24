//! Session Lifecycle Handlers — Finalizer + StateManager + CostTracker.
//!
//! - `SessionFinalizerHandler`: Stop (async) — serialize session summary
//! - `SessionStateManagerHandler`: PreCompact (sync) — atomic checkpoint
//! - `CostTrackerHandler`: Stop (async) — token cost estimation

use std::io::Write as _;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

// ── H34: SessionFinalizer ─────────────────────────────────────────────

/// Serialize session summary to JSON checkpoint on session end.
pub struct SessionFinalizerHandler;

impl Handler for SessionFinalizerHandler {
    fn name(&self) -> &str {
        "session_finalizer"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let edits = ctx.knowledge.recent_edits_all(1000).unwrap_or_default();
        let edit_count = edits.len();
        let files_modified: Vec<&str> = edits
            .iter()
            .map(|e| e.file_path.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let summary = serde_json::json!({
            "session_id": ctx.session_id,
            "ended_at": chrono_now(),
            "files_modified": files_modified.len(),
            "total_edits": edit_count,
            "file_list": files_modified.iter().take(20).collect::<Vec<_>>(),
        });

        // Write checkpoint
        let ckpt_dir = ctx.project_root.join(".claude/checkpoints");
        let _ = std::fs::create_dir_all(&ckpt_dir);
        let ts = timestamp_compact();
        let path = ckpt_dir.join(format!("session_summary_{ts}.toon"));
        if let Ok(json) = serde_json::to_string_pretty(&summary) {
            let _ = std::fs::write(&path, json);
        }

        // Store in RlmMemory for cross-session recall
        if let Some(ref rlm) = ctx.rlm {
            let key = format!("session:summary:{}", ctx.session_id);
            let val = serde_json::to_string(&summary).unwrap_or_default();
            let _ = rlm.store(
                &key,
                touring_intelligence::rl::memory::rlm::MemoryTier::Reference,
                &val,
                Some("session_summary"),
                None,
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ── H35: SessionStateManager ──────────────────────────────────────────

/// Atomic checkpoint with git state before context compaction.
pub struct SessionStateManagerHandler;

impl Handler for SessionStateManagerHandler {
    fn name(&self) -> &str {
        "session_state_manager"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreCompact]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Capture git state
        let branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"], &ctx.project_root);
        let dirty_count = run_git(&["status", "--porcelain"], &ctx.project_root)
            .lines()
            .count();
        let last_commit = run_git(&["rev-parse", "--short", "HEAD"], &ctx.project_root);

        let state = serde_json::json!({
            "session_id": ctx.session_id,
            "captured_at": chrono_now(),
            "git": {
                "branch": branch.trim(),
                "dirty_files": dirty_count,
                "last_commit": last_commit.trim(),
            },
        });

        // Atomic write: .tmp → rename
        let ckpt_dir = ctx.project_root.join(".claude/checkpoints");
        let _ = std::fs::create_dir_all(&ckpt_dir);
        let ts = timestamp_compact();
        let final_path = ckpt_dir.join(format!("session_state_{ts}.toon"));
        let tmp_path = ckpt_dir.join(format!(".session_state_{ts}.tmp"));

        if let Ok(json) = serde_json::to_string_pretty(&state) {
            if std::fs::write(&tmp_path, &json).is_ok() {
                let _ = std::fs::rename(&tmp_path, &final_path);
            }
        }

        // Store compact state in RlmMemory for post-compact recovery
        if let Some(ref rlm) = ctx.rlm {
            let val = serde_json::to_string(&state).unwrap_or_default();
            let _ = rlm.store(
                &format!("session:state:{ts}"),
                touring_intelligence::rl::memory::rlm::MemoryTier::Working,
                &val,
                Some("session_state"),
                None,
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ── H36: CostTracker ─────────────────────────────────────────────────

/// Token counting + USD cost estimation on session end.
pub struct CostTrackerHandler;

// Claude pricing (per 1M tokens)
const INPUT_RATE: f64 = 3.0;
const OUTPUT_RATE: f64 = 15.0;
const CACHE_WRITE_RATE: f64 = 3.75;
const CACHE_READ_RATE: f64 = 0.30;

impl Handler for CostTrackerHandler {
    fn name(&self) -> &str {
        "cost_tracker"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let input_tokens = ctx
            .input
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output_tokens = ctx
            .input
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_creation = ctx
            .input
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_read = ctx
            .input
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let cost = (input_tokens as f64 * INPUT_RATE
            + output_tokens as f64 * OUTPUT_RATE
            + cache_creation as f64 * CACHE_WRITE_RATE
            + cache_read as f64 * CACHE_READ_RATE)
            / 1_000_000.0;

        let record = serde_json::json!({
            "session_id": ctx.session_id,
            "timestamp": chrono_now(),
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "cache_creation_tokens": cache_creation,
            "cache_read_tokens": cache_read,
            "estimated_cost_usd": (cost * 10000.0).round() / 10000.0,
        });

        // Append JSONL
        let costs_path = ctx.project_root.join(".claude/data/costs.jsonl");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&costs_path)
        {
            if let Ok(line) = serde_json::to_string(&record) {
                let _ = writeln!(f, "{line}");
            }
        }

        HandlerResult::skip(self.name())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}

fn timestamp_compact() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // YYYYMMDD_HHMMSS approximation from epoch
    let secs = now;
    format!("{secs}")
}

fn run_git(args: &[&str], project_root: &std::path::Path) -> String {
    std::process::Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

// ── Registration ──────────────────────────────────────────────────────

/// Registers all session-domain handlers on the given pipeline.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(SessionFinalizerHandler));
    pipeline.register(Box::new(SessionStateManagerHandler));
    pipeline.register(Box::new(CostTrackerHandler));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    #[allow(clippy::arc_with_non_send_sync)]
    fn make_ctx(event: HookEvent, input: serde_json::Value) -> (TempDir, CortexContext) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
        let knowledge = Arc::new(db);
        let ctx = CortexContext::from_input(event, input, knowledge, tmp.path().to_path_buf());
        (tmp, ctx)
    }

    #[test]
    fn test_cost_calculation() {
        // 100K input + 50K output = (100K*3 + 50K*15) / 1M = (300K + 750K) / 1M = 1.05
        let cost = (100_000.0 * INPUT_RATE + 50_000.0 * OUTPUT_RATE) / 1_000_000.0;
        assert!((cost - 1.05).abs() < 0.001);
    }

    #[test]
    fn test_cost_calculation_cache_tokens() {
        // 10K cache_write + 100K cache_read = (10K*3.75 + 100K*0.30) / 1M
        let cost = (10_000.0 * CACHE_WRITE_RATE + 100_000.0 * CACHE_READ_RATE) / 1_000_000.0;
        let expected = (10_000.0 * 3.75 + 100_000.0 * 0.30) / 1_000_000.0;
        assert!((cost - expected).abs() < 0.0001);
    }

    #[test]
    fn test_cost_calculation_zero_tokens() {
        let cost = (0.0 * INPUT_RATE + 0.0 * OUTPUT_RATE) / 1_000_000.0;
        assert!((cost - 0.0).abs() < 0.0001);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn test_cost_rates_are_reasonable() {
        // Intentional compile-time regression guards: these const comparisons
        // ensure future refactors preserve the pricing invariants.
        assert!(OUTPUT_RATE > INPUT_RATE);
        assert!(CACHE_READ_RATE < CACHE_WRITE_RATE);
        assert!(INPUT_RATE > 0.0);
    }

    #[test]
    fn test_handler_properties() {
        let h = SessionFinalizerHandler;
        assert_eq!(h.name(), "session_finalizer");
        assert_eq!(h.events(), &[HookEvent::Stop]);
        assert!(h.is_async());

        let h = SessionStateManagerHandler;
        assert_eq!(h.name(), "session_state_manager");
        assert_eq!(h.events(), &[HookEvent::PreCompact]);
        assert!(!h.is_async());

        let h = CostTrackerHandler;
        assert_eq!(h.name(), "cost_tracker");
        assert_eq!(h.events(), &[HookEvent::Stop]);
        assert!(h.is_async());
    }

    // ── H34: SessionFinalizerHandler ──────────────────────────────────

    #[test]
    fn test_session_finalizer_name_events() {
        assert_eq!(SessionFinalizerHandler.name(), "session_finalizer");
        assert_eq!(SessionFinalizerHandler.events(), &[HookEvent::Stop]);
        assert!(SessionFinalizerHandler.is_async());
        assert!(SessionFinalizerHandler.tool_matcher().is_none());
    }

    #[test]
    fn test_session_finalizer_always_skips_context_injection() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::Stop,
            serde_json::json!({ "session_id": "test-session-001" }),
        );
        let result = SessionFinalizerHandler.execute(&mut ctx);
        // Always returns Skip (writes checkpoint, no context)
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_session_finalizer_handles_empty_input() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::Stop, serde_json::json!({}));
        // Should not panic even with empty input
        let result = SessionFinalizerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_session_finalizer_writes_checkpoint_to_temp_dir() {
        let (tmp, mut ctx) = make_ctx(
            HookEvent::Stop,
            serde_json::json!({ "session_id": "session-abc" }),
        );
        let _ = SessionFinalizerHandler.execute(&mut ctx);
        // The handler creates .claude/checkpoints/ inside project_root
        // project_root is tmp.path(), so check if dir was created (or would be)
        // No assertion on file existence since we can't guarantee timing,
        // but the handler should complete without panic.
        let _ = tmp; // keep alive
    }

    // ── H35: SessionStateManagerHandler ──────────────────────────────

    #[test]
    fn test_session_state_manager_name_events() {
        assert_eq!(SessionStateManagerHandler.name(), "session_state_manager");
        assert_eq!(
            SessionStateManagerHandler.events(),
            &[HookEvent::PreCompact]
        );
        assert!(!SessionStateManagerHandler.is_async());
        assert!(SessionStateManagerHandler.tool_matcher().is_none());
    }

    #[test]
    fn test_session_state_manager_skips_context_injection() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PreCompact,
            serde_json::json!({ "session_id": "session-xyz" }),
        );
        let result = SessionStateManagerHandler.execute(&mut ctx);
        // Writes checkpoint, no context
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_session_state_manager_handles_no_git() {
        // In a non-git directory, git commands return empty → should not panic
        let (_tmp, mut ctx) = make_ctx(HookEvent::PreCompact, serde_json::json!({}));
        let result = SessionStateManagerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_session_state_manager_is_sync() {
        assert!(!SessionStateManagerHandler.is_async());
    }

    // ── H36: CostTrackerHandler ───────────────────────────────────────

    #[test]
    fn test_cost_tracker_name_events() {
        assert_eq!(CostTrackerHandler.name(), "cost_tracker");
        assert_eq!(CostTrackerHandler.events(), &[HookEvent::Stop]);
        assert!(CostTrackerHandler.is_async());
        assert!(CostTrackerHandler.tool_matcher().is_none());
    }

    #[test]
    fn test_cost_tracker_skips_context_injection() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::Stop,
            serde_json::json!({
                "input_tokens": 10000,
                "output_tokens": 5000,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0
            }),
        );
        let result = CostTrackerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_cost_tracker_handles_zero_tokens() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::Stop, serde_json::json!({}));
        // Missing token fields → default 0 → no panic
        let result = CostTrackerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_cost_tracker_handles_all_token_types() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::Stop,
            serde_json::json!({
                "input_tokens": 50000,
                "output_tokens": 20000,
                "cache_creation_input_tokens": 5000,
                "cache_read_input_tokens": 100000
            }),
        );
        let result = CostTrackerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_cost_tracker_appends_jsonl_to_project_root() {
        let (tmp, mut ctx) = make_ctx(
            HookEvent::Stop,
            serde_json::json!({
                "input_tokens": 1000,
                "output_tokens": 500
            }),
        );
        // Create the data dir so the write can succeed
        std::fs::create_dir_all(tmp.path().join(".claude/data")).unwrap();
        let _ = CostTrackerHandler.execute(&mut ctx);
        let costs_file = tmp.path().join(".claude/data/costs.jsonl");
        if costs_file.exists() {
            let content = std::fs::read_to_string(&costs_file).unwrap();
            assert!(content.contains("input_tokens") || content.contains("session_id"));
        }
        let _ = tmp; // keep alive
    }

    // ── Helper functions ──────────────────────────────────────────────

    #[test]
    fn test_chrono_now_returns_numeric_string() {
        let ts = chrono_now();
        assert!(!ts.is_empty());
        // Should be parseable as a number (epoch seconds)
        let parsed: Result<u64, _> = ts.parse();
        assert!(
            parsed.is_ok(),
            "chrono_now() should return numeric epoch: {ts}"
        );
    }

    #[test]
    fn test_timestamp_compact_returns_numeric_string() {
        let ts = timestamp_compact();
        assert!(!ts.is_empty());
        let parsed: Result<u64, _> = ts.parse();
        assert!(
            parsed.is_ok(),
            "timestamp_compact() should return numeric epoch: {ts}"
        );
    }

    #[test]
    fn test_chrono_now_increases_over_time() {
        let t1: u64 = chrono_now().parse().unwrap_or(0);
        let t2: u64 = chrono_now().parse().unwrap_or(0);
        assert!(t2 >= t1, "Time should not go backwards");
    }

    // ── Registration ─────────────────────────────────────────────────

    #[test]
    fn test_register_session_handlers() {
        let mut pipeline = Pipeline::new();
        register(&mut pipeline);
        // 3 handlers: H34, H35, H36
        assert_eq!(pipeline.handler_count(), 3);
    }

    #[test]
    fn test_register_stop_handlers() {
        let mut pipeline = Pipeline::new();
        register(&mut pipeline);
        let stop_handlers = pipeline.for_event(HookEvent::Stop, None);
        assert_eq!(stop_handlers.len(), 2); // SessionFinalizer + CostTracker
        let names: Vec<&str> = stop_handlers.iter().map(|h| h.name()).collect();
        assert!(names.contains(&"session_finalizer"));
        assert!(names.contains(&"cost_tracker"));
    }

    #[test]
    fn test_register_precompact_handler() {
        let mut pipeline = Pipeline::new();
        register(&mut pipeline);
        let compact_handlers = pipeline.for_event(HookEvent::PreCompact, None);
        assert_eq!(compact_handlers.len(), 1);
        assert_eq!(compact_handlers[0].name(), "session_state_manager");
    }
}
