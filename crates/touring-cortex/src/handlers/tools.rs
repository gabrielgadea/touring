//! Tool Intelligence Handlers — Subagent context, lint gate, tool search, failure recording.
//!
//! - `SubagentContextInjectorV2`: SubagentStart (sync) — FTS5 + Wilson context
//! - `SubagentOutcomeRecorderV2`: SubagentStop (async) — outcome + Wilson update
//! - `ShadowLintGateHandler`: PreToolUse[Write|Edit|MultiEdit] (sync, CAN BLOCK)
//! - `ToolSearchAdvisorHandler`: Pre+PostToolUse (sync/async) — pattern recall/store
//! - `FailureRecorderHandler`: PostToolUseFailure (async) — failure pattern storage

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

// ── H37: SubagentContextInjectorV2 ────────────────────────────────────

/// Inject touring context (memories + Wilson patterns) into subagent prompts.
/// Replaces the simpler SubagentStartHandler in lifecycle.rs.
pub struct SubagentContextInjectorV2;

impl Handler for SubagentContextInjectorV2 {
    fn name(&self) -> &str {
        "subagent_context_injector_v2"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SubagentStart]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let task_desc = ctx
            .input
            .get("task_description")
            .or_else(|| ctx.input.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if task_desc.is_empty() || ctx.context_budget_remaining < 50 {
            return HandlerResult::skip(self.name());
        }

        let mut parts: Vec<String> = Vec::new();

        // FTS5 search for relevant chunks
        if let Some(ref recall) = ctx.recall {
            let keywords = extract_keywords(task_desc, 4);
            if !keywords.is_empty() {
                let query = keywords.join(" ");
                if let Ok(chunks) = recall.fts_search(&query, 3) {
                    for chunk in &chunks {
                        let snippet = truncate(&chunk.content, 80);
                        parts.push(format!("[memory] {snippet}"));
                    }
                }
            }
        }

        // Wilson top-k patterns
        if let Some(ref persistence) = ctx.persistence {
            if let Ok(top) = persistence.wilson_top_k(3) {
                for (id, score, _s, _t) in &top {
                    parts.push(format!("[pattern] {id} (conf={score:.2})"));
                }
            }
            let _ = persistence.log_hook_event(
                "SubagentStart",
                ctx.input
                    .get("subagent_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                &format!("memories={}", parts.len()),
                0.5,
            );
        }

        if parts.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let context = format!("Touring: {}", parts.join(" | "));
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── H38: SubagentOutcomeRecorderV2 ────────────────────────────────────

/// Record subagent results and update Wilson scores.
pub struct SubagentOutcomeRecorderV2;

impl Handler for SubagentOutcomeRecorderV2 {
    fn name(&self) -> &str {
        "subagent_outcome_recorder_v2"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SubagentStop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let subagent_id = ctx
            .input
            .get("subagent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let success = ctx
            .input
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let result_summary = ctx
            .input
            .get("result_summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(300)
            .collect::<String>();

        // Extract agent type from subagent_id (prefix before random suffix)
        let agent_type = subagent_id
            .split(|c: char| c.is_ascii_hexdigit() && subagent_id.contains(c))
            .next()
            .unwrap_or("general")
            .trim_end_matches(['-', '_']);
        let agent_type = if agent_type.is_empty() {
            "general"
        } else {
            agent_type
        };

        // Store outcome in RlmMemory
        if let Some(ref rlm) = ctx.rlm {
            let key = format!("subagent:result:{subagent_id}");
            let _ = rlm.store(
                &key,
                touring_intelligence::rl::memory::rlm::MemoryTier::Reference,
                &result_summary,
                Some("subagent_outcome"),
                None,
            );
        }

        // Update Wilson for agent type
        if let Some(ref persistence) = ctx.persistence {
            let wilson_key = format!("subagent:{agent_type}");
            let _ = persistence.wilson_update(&wilson_key, success);

            let reward = if success { 1.0 } else { -0.5 };
            let _ = persistence.log_hook_event(
                "SubagentStop",
                subagent_id,
                &format!("success={success}"),
                reward,
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ── H39: ShadowLintGate — REMOVED (superseded by H51 CodeStandardsEnforcer in quality.rs)
// H51 is a strict superset: content-hash cache + diff-based baseline comparison.

// ── H40: ToolSearchAdvisor ────────────────────────────────────────────

/// Advise on tool selection based on past resolutions.
pub struct ToolSearchAdvisorHandler;

impl Handler for ToolSearchAdvisorHandler {
    fn name(&self) -> &str {
        "tool_search_advisor"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse, HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("ToolSearch")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        match ctx.event {
            HookEvent::PreToolUse => self.pre_tool(ctx),
            HookEvent::PostToolUse => self.post_tool(ctx),
            _ => HandlerResult::skip(self.name()),
        }
    }
}

impl ToolSearchAdvisorHandler {
    fn pre_tool(&self, ctx: &mut CortexContext) -> HandlerResult {
        let query = ctx
            .tool_input
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if query.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Search for past resolutions
        if let Some(ref rlm) = ctx.rlm {
            let results = rlm.search(query, None, 3).unwrap_or_default();

            let past: Vec<String> = results
                .iter()
                .filter(|m| m.key.starts_with("tool_search_pattern:"))
                .map(|m| truncate(&m.value, 80))
                .collect();

            if !past.is_empty() {
                let context = format!("ToolSearch past: {}", past.join(" | "));
                return HandlerResult::allow(self.name(), Some(context));
            }
        }

        HandlerResult::skip(self.name())
    }

    fn post_tool(&self, ctx: &mut CortexContext) -> HandlerResult {
        let query = ctx
            .tool_input
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let result = ctx
            .input
            .get("tool_result")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if query.is_empty() || result.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Store pattern for future recall
        if let Some(ref rlm) = ctx.rlm {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            query.hash(&mut hasher);
            let hash = format!("{:x}", hasher.finish());

            let value = format!(
                "query={} result={}",
                truncate(query, 100),
                truncate(result, 200)
            );
            let _ = rlm.store(
                &format!("tool_search_pattern:{hash}"),
                touring_intelligence::rl::memory::rlm::MemoryTier::Reference,
                &value,
                Some("tool_search_pattern"),
                None,
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ── H41: FailureRecorder ──────────────────────────────────────────────

/// Record tool failures for gotcha matching and learning.
pub struct FailureRecorderHandler;

impl Handler for FailureRecorderHandler {
    fn name(&self) -> &str {
        "failure_recorder"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUseFailure]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let tool = ctx.tool_name.as_deref().unwrap_or("unknown");
        let file = ctx.file_path.as_deref().unwrap_or("unknown");
        let error = ctx
            .input
            .get("error")
            .or_else(|| ctx.input.get("tool_result"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(300)
            .collect::<String>();

        // Store in RlmMemory
        if let Some(ref rlm) = ctx.rlm {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let key = format!("failure:{tool}:{ts}");
            let value = format!("file={file} error={error}");
            let _ = rlm.store(
                &key,
                touring_intelligence::rl::memory::rlm::MemoryTier::Reference,
                &value,
                Some("failure_pattern"),
                None,
            );
        }

        // Log hook event with negative reward
        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.log_hook_event(
                "PostToolUseFailure",
                tool,
                &truncate(&error, 100),
                -0.3,
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

fn extract_keywords(text: &str, max: usize) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| w.len() >= 4)
        .filter(|w| !is_stop_word(w))
        .take(max)
        .map(|w| w.to_lowercase())
        .collect()
}

fn is_stop_word(word: &str) -> bool {
    matches!(
        word.to_lowercase().as_str(),
        "the"
            | "and"
            | "for"
            | "with"
            | "this"
            | "that"
            | "have"
            | "from"
            | "will"
            | "would"
            | "could"
            | "should"
            | "into"
            | "about"
            | "been"
            | "they"
            | "then"
            | "when"
            | "what"
            | "also"
            | "more"
            | "some"
            | "there"
            | "their"
            | "than"
            | "para"
            | "como"
            | "este"
            | "essa"
            | "isso"
            | "aqui"
            | "onde"
            | "qual"
            | "quais"
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // UTF-8 safe: find the nearest char boundary at or before `max`
        let end = s
            .char_indices()
            .take_while(|(i, _)| *i < max)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}...", &s[..end])
    }
}

// ── H56: AcoGoalTrackerHandler ───────────────────────────────────────

/// Track ACO goal progress by detecting milestone patterns in tool output.
pub struct AcoGoalTrackerHandler;

impl Handler for AcoGoalTrackerHandler {
    fn name(&self) -> &str {
        "aco_goal_tracker"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Look for milestone patterns in tool output
        let output_text = ctx
            .input
            .get("output")
            .or_else(|| ctx.input.get("stdout"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if output_text.is_empty() || output_text.len() < 20 {
            return HandlerResult::skip(self.name());
        }

        // Detect milestone patterns
        let patterns = [
            ("test result: ok", "tests_passing"),
            ("PASS", "validation_pass"),
            ("Finished", "build_complete"),
            ("created successfully", "file_created"),
            ("Updated task", "task_progress"),
        ];

        let mut milestones: Vec<&str> = Vec::new();
        for (pattern, label) in &patterns {
            if output_text.contains(pattern) {
                milestones.push(label);
            }
        }

        if milestones.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Persist goal progress in Wilson scores
        if let Some(ref persistence) = ctx.persistence {
            for milestone in &milestones {
                let _ = persistence.wilson_update(&format!("goal:{milestone}"), true);
            }
            let _ = persistence.log_hook_event(
                "PostToolUse",
                "AcoGoalTracker",
                &format!("milestones={}", milestones.join(",")),
                0.8,
            );
        }

        HandlerResult::skip(self.name()) // Tracking only, no context injection
    }
}

// ── H57: FailureRecoveryOrchestratorHandler ──────────────────────────

/// Analyze tool failures and suggest recovery actions.
pub struct FailureRecoveryOrchestratorHandler;

impl Handler for FailureRecoveryOrchestratorHandler {
    fn name(&self) -> &str {
        "failure_recovery_orchestrator"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUseFailure]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let stderr = ctx
            .input
            .get("stderr")
            .or_else(|| ctx.input.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let tool = ctx.tool_name.as_deref().unwrap_or("unknown");

        if stderr.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let stderr_lower = stderr.to_lowercase();

        // Pattern match on common failure modes and suggest recovery
        let suggestion = if stderr_lower.contains("not found")
            || stderr_lower.contains("no such file")
        {
            "Verify the path with ls/Glob before retrying"
        } else if stderr_lower.contains("permission denied") {
            "Check file permissions — you may need chmod or sudo"
        } else if stderr_lower.contains("syntax error") || stderr_lower.contains("syntaxerror") {
            "Run ruff check on the file to identify syntax issues before retrying"
        } else if stderr_lower.contains("connection refused")
            || stderr_lower.contains("connection reset")
        {
            "Service may be down — check if the required service is running"
        } else if stderr_lower.contains("import error")
            || stderr_lower.contains("modulenotfounderror")
        {
            "Missing dependency — check if the required package is installed"
        } else if stderr_lower.contains("timeout") || stderr_lower.contains("timed out") {
            "Operation timed out — consider increasing timeout or simplifying the task"
        } else if stderr_lower.contains("out of memory") || stderr_lower.contains("oom") {
            "Memory exhaustion — reduce batch size or use streaming"
        } else {
            return HandlerResult::skip(self.name()); // Unknown pattern, skip
        };

        // Log failure pattern for learning
        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.wilson_update(&format!("failure:{tool}"), false);
            let _ = persistence.log_hook_event(
                "PostToolUseFailure",
                "FailureRecovery",
                &format!("tool={tool} pattern=detected"),
                0.3,
            );
        }

        let context = format!("Recovery[{tool}]: {suggestion}");
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── Registration ──────────────────────────────────────────────────────

/// Registers all tool/subagent-domain handlers on the given pipeline.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(SubagentContextInjectorV2));
    pipeline.register(Box::new(SubagentOutcomeRecorderV2));
    // H39 ShadowLintGate removed — superseded by H51 CodeStandardsEnforcer
    pipeline.register(Box::new(ToolSearchAdvisorHandler));
    pipeline.register(Box::new(FailureRecorderHandler));
    pipeline.register(Box::new(AcoGoalTrackerHandler));
    pipeline.register(Box::new(FailureRecoveryOrchestratorHandler));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    #[allow(clippy::arc_with_non_send_sync)]
    fn make_ctx(
        event: HookEvent,
        input: serde_json::Value,
    ) -> (TempDir, crate::context::CortexContext) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
        let knowledge = Arc::new(db);
        let ctx = crate::context::CortexContext::from_input(
            event,
            input,
            knowledge,
            tmp.path().to_path_buf(),
        );
        (tmp, ctx)
    }

    #[test]
    fn test_extract_keywords() {
        let kw = extract_keywords("analyze the big process for ANTT", 4);
        assert!(kw.contains(&"analyze".to_string()));
        assert!(kw.contains(&"process".to_string()));
        assert!(kw.contains(&"antt".to_string()));
        assert!(!kw.contains(&"the".to_string())); // stop word
        assert!(!kw.contains(&"for".to_string())); // stop word
    }

    #[test]
    fn test_extract_keywords_max_limit() {
        let kw = extract_keywords("alpha beta gamma delta epsilon zeta", 3);
        assert_eq!(kw.len(), 3);
    }

    #[test]
    fn test_extract_keywords_filters_short_words() {
        let kw = extract_keywords("a ab abc abcd abcde", 10);
        // Words with len < 4 are filtered
        assert!(!kw.contains(&"a".to_string()));
        assert!(!kw.contains(&"ab".to_string()));
        assert!(!kw.contains(&"abc".to_string()));
        assert!(kw.contains(&"abcd".to_string()));
        assert!(kw.contains(&"abcde".to_string()));
    }

    #[test]
    fn test_extract_keywords_stop_words_filtered() {
        let stop_words = ["the", "and", "for", "with", "this", "that", "para", "como"];
        for w in &stop_words {
            let kw = extract_keywords(w, 5);
            assert!(kw.is_empty(), "Stop word '{w}' should be filtered");
        }
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world foo", 5), "hello...");
    }

    #[test]
    fn test_truncate_exact_boundary() {
        // Exactly at boundary → no truncation
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn test_truncate_unicode_safe() {
        // UTF-8 multi-byte chars should not cause panics
        let s = "héllo wörld";
        let result = truncate(s, 5);
        // Should not panic and should end with "..."
        assert!(result.ends_with("...") || result == s);
    }

    #[test]
    fn test_lint_score() {
        // 0 errors, 0 warnings → score 1.0
        let score = (1.0 - 0.0 * 0.3 - 0.0 * 0.05f64).max(0.0);
        assert!((score - 1.0).abs() < 0.001);

        // 1 error → score 0.7
        let score = (1.0 - 1.0 * 0.3 - 0.0 * 0.05f64).max(0.0);
        assert!((score - 0.7).abs() < 0.001);

        // 2 errors → score 0.4 → BLOCK
        let score = (1.0 - 2.0 * 0.3 - 0.0 * 0.05f64).max(0.0);
        assert!((score - 0.4).abs() < 0.001);
        assert!(score < 0.5); // Would block
    }

    #[test]
    fn test_handler_properties() {
        assert_eq!(
            SubagentContextInjectorV2.name(),
            "subagent_context_injector_v2"
        );
        assert_eq!(
            SubagentContextInjectorV2.events(),
            &[HookEvent::SubagentStart]
        );
        assert!(!SubagentContextInjectorV2.is_async());

        assert_eq!(
            SubagentOutcomeRecorderV2.name(),
            "subagent_outcome_recorder_v2"
        );
        assert!(SubagentOutcomeRecorderV2.is_async());

        // H39 ShadowLintGate removed — superseded by H51 CodeStandardsEnforcer

        assert_eq!(ToolSearchAdvisorHandler.name(), "tool_search_advisor");
        assert_eq!(ToolSearchAdvisorHandler.tool_matcher(), Some("ToolSearch"));

        assert_eq!(FailureRecorderHandler.name(), "failure_recorder");
        assert!(FailureRecorderHandler.is_async());
    }

    // ── H37: SubagentContextInjectorV2 ───────────────────────────────

    #[test]
    fn test_subagent_context_injector_v2_name_events() {
        assert_eq!(
            SubagentContextInjectorV2.name(),
            "subagent_context_injector_v2"
        );
        assert_eq!(
            SubagentContextInjectorV2.events(),
            &[HookEvent::SubagentStart]
        );
        assert!(!SubagentContextInjectorV2.is_async());
        assert!(SubagentContextInjectorV2.tool_matcher().is_none());
    }

    #[test]
    fn test_subagent_context_injector_skips_empty_task_desc() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::SubagentStart,
            serde_json::json!({ "task_description": "" }),
        );
        let result = SubagentContextInjectorV2.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_subagent_context_injector_skips_low_budget() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::SubagentStart,
            serde_json::json!({ "task_description": "analyze the legal document for ANTT" }),
        );
        ctx.context_budget_remaining = 30;
        let result = SubagentContextInjectorV2.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_subagent_context_injector_skips_no_input() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::SubagentStart, serde_json::json!({}));
        let result = SubagentContextInjectorV2.execute(&mut ctx);
        // Empty task_description ("") → skip
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_subagent_context_injector_uses_prompt_fallback() {
        // When task_description absent, falls back to "prompt" key
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::SubagentStart,
            serde_json::json!({ "prompt": "" }),
        );
        let result = SubagentContextInjectorV2.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    // ── H38: SubagentOutcomeRecorderV2 ───────────────────────────────

    #[test]
    fn test_subagent_outcome_recorder_v2_name_events() {
        assert_eq!(
            SubagentOutcomeRecorderV2.name(),
            "subagent_outcome_recorder_v2"
        );
        assert_eq!(
            SubagentOutcomeRecorderV2.events(),
            &[HookEvent::SubagentStop]
        );
        assert!(SubagentOutcomeRecorderV2.is_async());
        assert!(SubagentOutcomeRecorderV2.tool_matcher().is_none());
    }

    #[test]
    fn test_subagent_outcome_recorder_skips_context_injection() {
        // Always skip (recording only)
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::SubagentStop,
            serde_json::json!({
                "subagent_id": "researcher-abc123",
                "success": true,
                "result_summary": "Analysis complete with 5 findings."
            }),
        );
        let result = SubagentOutcomeRecorderV2.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_subagent_outcome_recorder_handles_missing_fields() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::SubagentStop, serde_json::json!({}));
        // Missing subagent_id, success, result_summary → defaults used, should not panic
        let result = SubagentOutcomeRecorderV2.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_subagent_outcome_recorder_handles_failure() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::SubagentStop,
            serde_json::json!({
                "subagent_id": "analyzer-xyz789",
                "success": false,
                "result_summary": "Failed to process document."
            }),
        );
        let result = SubagentOutcomeRecorderV2.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_subagent_outcome_recorder_truncates_long_summary() {
        let long_summary = "x".repeat(500);
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::SubagentStop,
            serde_json::json!({
                "subagent_id": "agent-001",
                "success": true,
                "result_summary": long_summary
            }),
        );
        // Should not panic even with long summary
        let result = SubagentOutcomeRecorderV2.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    // ── H40: ToolSearchAdvisorHandler ────────────────────────────────

    #[test]
    fn test_tool_search_advisor_name_events() {
        assert_eq!(ToolSearchAdvisorHandler.name(), "tool_search_advisor");
        assert_eq!(
            ToolSearchAdvisorHandler.events(),
            &[HookEvent::PreToolUse, HookEvent::PostToolUse]
        );
        assert_eq!(ToolSearchAdvisorHandler.tool_matcher(), Some("ToolSearch"));
        assert!(!ToolSearchAdvisorHandler.is_async());
    }

    #[test]
    fn test_tool_search_advisor_pre_tool_skips_empty_query() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PreToolUse,
            serde_json::json!({
                "tool_name": "ToolSearch",
                "tool_input": { "query": "" }
            }),
        );
        let result = ToolSearchAdvisorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_tool_search_advisor_pre_tool_skips_no_query() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PreToolUse,
            serde_json::json!({
                "tool_name": "ToolSearch",
                "tool_input": {}
            }),
        );
        let result = ToolSearchAdvisorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_tool_search_advisor_post_tool_skips_empty_result() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUse,
            serde_json::json!({
                "tool_name": "ToolSearch",
                "tool_input": { "query": "database connection" },
                "tool_result": ""
            }),
        );
        let result = ToolSearchAdvisorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_tool_search_advisor_skips_unknown_event() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::SessionStart, serde_json::json!({}));
        let result = ToolSearchAdvisorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    // ── H41: FailureRecorderHandler ──────────────────────────────────

    #[test]
    fn test_failure_recorder_name_events() {
        assert_eq!(FailureRecorderHandler.name(), "failure_recorder");
        assert_eq!(
            FailureRecorderHandler.events(),
            &[HookEvent::PostToolUseFailure]
        );
        assert!(FailureRecorderHandler.is_async());
        assert!(FailureRecorderHandler.tool_matcher().is_none());
    }

    #[test]
    fn test_failure_recorder_skips_context_injection() {
        // FailureRecorder always returns Skip (recording only)
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Bash",
                "error": "command not found: touring"
            }),
        );
        let result = FailureRecorderHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_failure_recorder_handles_missing_fields() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::PostToolUseFailure, serde_json::json!({}));
        // All fields default to "unknown" — should not panic
        let result = FailureRecorderHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_failure_recorder_uses_tool_result_as_error_fallback() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Read",
                "tool_result": "File not found: /missing/path.rs"
            }),
        );
        let result = FailureRecorderHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    // ── H56: AcoGoalTrackerHandler ────────────────────────────────────

    #[test]
    fn test_aco_goal_tracker_name_events() {
        assert_eq!(AcoGoalTrackerHandler.name(), "aco_goal_tracker");
        assert_eq!(AcoGoalTrackerHandler.events(), &[HookEvent::PostToolUse]);
        assert!(AcoGoalTrackerHandler.is_async());
        assert!(AcoGoalTrackerHandler.tool_matcher().is_none());
    }

    #[test]
    fn test_aco_goal_tracker_skips_empty_output() {
        let (_tmp, mut ctx) = make_ctx(HookEvent::PostToolUse, serde_json::json!({ "output": "" }));
        let result = AcoGoalTrackerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_aco_goal_tracker_skips_short_output() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUse,
            serde_json::json!({ "output": "ok" }),
        );
        let result = AcoGoalTrackerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_aco_goal_tracker_skips_output_without_milestones() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUse,
            serde_json::json!({ "output": "This is a regular output with no milestone patterns at all." }),
        );
        let result = AcoGoalTrackerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_aco_goal_tracker_skips_on_milestone_detection_without_persistence() {
        // Even when milestone detected, returns Skip (tracking only, no context injection)
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUse,
            serde_json::json!({ "output": "test result: ok. 42 passed; 0 failed" }),
        );
        let result = AcoGoalTrackerHandler.execute(&mut ctx);
        // Always skip — tracking only
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_aco_goal_tracker_detects_pass_pattern() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUse,
            serde_json::json!({ "output": "All checks PASS — 15 tests validated successfully." }),
        );
        let result = AcoGoalTrackerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_aco_goal_tracker_uses_stdout_fallback() {
        // Falls back to "stdout" key when "output" is absent
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUse,
            serde_json::json!({ "stdout": "Finished build successfully in 2.3s" }),
        );
        let result = AcoGoalTrackerHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    // ── H57: FailureRecoveryOrchestratorHandler ───────────────────────

    #[test]
    fn test_failure_recovery_orchestrator_name_events() {
        assert_eq!(
            FailureRecoveryOrchestratorHandler.name(),
            "failure_recovery_orchestrator"
        );
        assert_eq!(
            FailureRecoveryOrchestratorHandler.events(),
            &[HookEvent::PostToolUseFailure]
        );
        assert!(!FailureRecoveryOrchestratorHandler.is_async());
        assert!(FailureRecoveryOrchestratorHandler.tool_matcher().is_none());
    }

    #[test]
    fn test_failure_recovery_orchestrator_skips_empty_stderr() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({ "stderr": "" }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_failure_recovery_orchestrator_suggests_path_check() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Read",
                "stderr": "No such file or directory: /tmp/missing.rs"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Allow);
        assert!(!result.context_lines.is_empty());
        assert!(result.context_lines[0].contains("ls/Glob"));
    }

    #[test]
    fn test_failure_recovery_orchestrator_suggests_chmod_for_permission_denied() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Write",
                "stderr": "Permission denied: cannot write to /etc/hosts"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Allow);
        assert!(result.context_lines[0].contains("chmod"));
    }

    #[test]
    fn test_failure_recovery_orchestrator_suggests_ruff_for_syntax_error() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Bash",
                "stderr": "SyntaxError: unexpected token at line 42"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Allow);
        assert!(result.context_lines[0].contains("ruff"));
    }

    #[test]
    fn test_failure_recovery_orchestrator_suggests_service_check_for_connection_refused() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Bash",
                "stderr": "Connection refused: could not connect to localhost:5432"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Allow);
        assert!(result.context_lines[0].contains("service"));
    }

    #[test]
    fn test_failure_recovery_orchestrator_suggests_install_for_import_error() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Bash",
                "stderr": "ModuleNotFoundError: No module named 'numpy'"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Allow);
        assert!(
            result.context_lines[0].contains("dependency")
                || result.context_lines[0].contains("package")
        );
    }

    #[test]
    fn test_failure_recovery_orchestrator_suggests_timeout_handling() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Bash",
                "stderr": "Operation timed out after 30 seconds"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Allow);
        assert!(result.context_lines[0].contains("timeout"));
    }

    #[test]
    fn test_failure_recovery_orchestrator_suggests_memory_reduction_for_oom() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Bash",
                "stderr": "Out of memory: killed process due to OOM"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Allow);
        assert!(
            result.context_lines[0].contains("batch") || result.context_lines[0].contains("Memory")
        );
    }

    #[test]
    fn test_failure_recovery_orchestrator_skips_unknown_pattern() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Bash",
                "stderr": "Some completely unknown and unrecognized error xyz123"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Skip);
    }

    #[test]
    fn test_failure_recovery_orchestrator_uses_error_key_fallback() {
        // Falls back to "error" key when "stderr" is absent
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Read",
                "error": "not found: file does not exist"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Allow);
    }

    #[test]
    fn test_failure_recovery_orchestrator_context_includes_tool_name() {
        let (_tmp, mut ctx) = make_ctx(
            HookEvent::PostToolUseFailure,
            serde_json::json!({
                "tool_name": "Edit",
                "stderr": "permission denied: cannot modify /etc/config"
            }),
        );
        let result = FailureRecoveryOrchestratorHandler.execute(&mut ctx);
        assert_eq!(result.decision, crate::types::Decision::Allow);
        assert!(result.context_lines[0].contains("Edit"));
    }

    // ── Registration ─────────────────────────────────────────────────

    #[test]
    fn test_register_tools_handlers() {
        let mut pipeline = crate::pipeline::Pipeline::new();
        register(&mut pipeline);
        // 6 handlers: H37, H38, H40, H41, H56, H57
        assert_eq!(pipeline.handler_count(), 6);
    }

    #[test]
    fn test_register_subagent_start_handler() {
        let mut pipeline = crate::pipeline::Pipeline::new();
        register(&mut pipeline);
        let handlers = pipeline.for_event(HookEvent::SubagentStart, None);
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].name(), "subagent_context_injector_v2");
    }

    #[test]
    fn test_register_subagent_stop_handler() {
        let mut pipeline = crate::pipeline::Pipeline::new();
        register(&mut pipeline);
        let handlers = pipeline.for_event(HookEvent::SubagentStop, None);
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].name(), "subagent_outcome_recorder_v2");
    }

    #[test]
    fn test_register_failure_handlers() {
        let mut pipeline = crate::pipeline::Pipeline::new();
        register(&mut pipeline);
        let handlers = pipeline.for_event(HookEvent::PostToolUseFailure, None);
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        assert!(names.contains(&"failure_recorder"));
        assert!(names.contains(&"failure_recovery_orchestrator"));
    }
}
