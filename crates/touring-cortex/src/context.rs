//! CortexContext — Shared mutable context passed through all handlers in a pipeline.

use std::collections::HashSet;
use std::sync::Arc;

use touring_intelligence::rl::evolution::LearningPersistence;
use touring_intelligence::rl::memory::recall::SemanticRecall;
use touring_intelligence::rl::memory::rlm::RlmMemory;

use crate::runtime::KnowledgeRef;
use crate::types::{CortexOutput, Decision, HandlerResult, HookEvent, HookSpecificOutput};

/// Shared mutable context passed through all handlers in a pipeline.
#[derive(Debug)]
pub struct CortexContext {
    /// The raw input from stdin.
    pub input: serde_json::Value,
    /// Parsed event type.
    pub event: HookEvent,
    /// Tool name (for PreToolUse/PostToolUse).
    pub tool_name: Option<String>,
    /// Tool input (for PreToolUse/PostToolUse).
    pub tool_input: serde_json::Value,
    /// Session ID.
    pub session_id: String,
    /// File path (extracted from tool_input if present).
    pub file_path: Option<String>,
    /// Accumulated context lines from all handlers.
    pub context_lines: Vec<String>,
    /// Scored context entries: (relevance_score, text). When present, `build_output`
    /// sorts by score descending so highest-relevance lines survive budget truncation.
    scored_lines: Vec<(f32, String)>,
    /// Current decision (most restrictive wins).
    pub decision: Decision,
    /// Handler metrics accumulator: (handler_name, metrics_value).
    pub handler_metrics: Vec<(String, serde_json::Value)>,
    /// Reference to the knowledge DB (touring_knowledge.db).
    pub knowledge: Arc<KnowledgeRef>,
    /// Reference to RLM memory DB (rlm_memory.db) — QTable, Wilson, memory_entries.
    pub rlm: Option<Arc<RlmMemory>>,
    /// Reference to semantic recall DB (semantic_recall.db) — FTS5 chunks.
    pub recall: Option<Arc<SemanticRecall>>,
    /// Reference to learning persistence (Wilson/Drift/QTable save/load + hook events).
    pub persistence: Option<Arc<LearningPersistence>>,
    /// Project root path.
    pub project_root: std::path::PathBuf,
    /// Remaining context budget in chars. Handlers check this to avoid flooding.
    pub context_budget_remaining: usize,
    /// E1-S2: Hash set of seen context lines for deduplication.
    /// Prevents multiple handlers from injecting identical context strings.
    context_seen: HashSet<u64>,
    /// E5-S6: Flag set by handlers that require cache invalidation (e.g., FileChanged).
    /// Pipeline checks this after each handler and clears filter_cache if set.
    pub needs_cache_invalidation: bool,
}

impl CortexContext {
    /// Create a CortexContext from parsed event and raw stdin input.
    /// Convenience constructor wrapping `from_input_full` with all optional DBs set to None.
    /// Used by pipeline handlers and test helpers.
    pub fn from_input(
        event: HookEvent,
        input: serde_json::Value,
        knowledge: Arc<KnowledgeRef>,
        project_root: std::path::PathBuf,
    ) -> Self {
        Self::from_input_full(event, input, knowledge, None, None, None, project_root)
    }

    /// Create a CortexContext with all DB references (used by CortexRuntime).
    pub fn from_input_full(
        event: HookEvent,
        input: serde_json::Value,
        knowledge: Arc<KnowledgeRef>,
        rlm: Option<Arc<RlmMemory>>,
        recall: Option<Arc<SemanticRecall>>,
        persistence: Option<Arc<LearningPersistence>>,
        project_root: std::path::PathBuf,
    ) -> Self {
        // Extract tool_name
        let tool_name = input
            .get("tool_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Extract tool_input
        let tool_input = input
            .get("tool_input")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        // Extract session_id
        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Extract file_path from various locations in tool_input
        let file_path = tool_input
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Budget per event type (chars). Tighter for high-frequency events.
        let context_budget_remaining = match event {
            HookEvent::UserPromptSubmit => 400,
            HookEvent::PreToolUse => 300,
            HookEvent::SessionStart => 300,
            HookEvent::PreCompact => 250,
            HookEvent::PostCompact => 3000, // ~750 tokens — re-injection budget
            _ => 500,
        };

        Self {
            input,
            event,
            tool_name,
            tool_input,
            session_id,
            file_path,
            context_lines: Vec::new(),
            decision: Decision::Skip,
            handler_metrics: Vec::new(),
            knowledge,
            rlm,
            recall,
            persistence,
            project_root,
            context_budget_remaining,
            scored_lines: Vec::new(),
            context_seen: HashSet::new(),
            needs_cache_invalidation: false,
        }
    }

    /// The edited file's path from `tool_input`, checking `file_path` then `path`.
    ///
    /// Returns `""` when neither key is present. Centralizes the `file_path` →
    /// `path` fallback that PreToolUse/PostToolUse handlers use to locate the
    /// target file; prefer this over re-deriving the `tool_input.get(...)` chain.
    /// (The pre-computed [`Self::file_path`] field omits the `path` fallback.)
    pub fn tool_file_path(&self) -> &str {
        self.tool_input
            .get("file_path")
            .or_else(|| self.tool_input.get("path"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    /// Emit a scored context line. Higher scores survive budget truncation.
    ///
    /// Use this instead of raw `context_lines` when the handler can quantify
    /// relevance. Lines with score >= 1.0 are high-priority (blast radius,
    /// cycle warnings). Lines with score ~0.5 are informational.
    pub fn emit_scored(&mut self, score: f32, line: String) {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        line.hash(&mut hasher);
        let hash = hasher.finish();
        if self.context_seen.insert(hash) {
            let len = line.len();
            self.scored_lines.push((score, line));
            self.context_budget_remaining = self.context_budget_remaining.saturating_sub(len);
        }
    }

    /// Merge a handler result into the accumulated context.
    ///
    /// - Context lines are appended.
    /// - Decision escalates (Block > Allow > Skip).
    /// - Metrics are accumulated.
    pub fn merge_result(&mut self, result: HandlerResult) {
        // E1-S2: Deduplicate context lines using hash-based seen set.
        // Multiple handlers may inject identical context (e.g., pre_read + symbol_enricher
        // both mentioning the same file). Dedup saves 10-20% of token budget.
        let mut added_chars: usize = 0;
        for line in result.context_lines {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            line.hash(&mut hasher);
            let hash = hasher.finish();
            if self.context_seen.insert(hash) {
                added_chars += line.len();
                self.context_lines.push(line);
            }
        }

        // Decrement budget by (deduplicated) context size
        self.context_budget_remaining = self.context_budget_remaining.saturating_sub(added_chars);

        // Escalate decision (most restrictive wins)
        let current = std::mem::replace(&mut self.decision, Decision::Skip);
        self.decision = current.escalate(result.decision);

        // Accumulate metrics
        if !result.metrics.is_null() {
            self.handler_metrics
                .push((result.handler_name, result.metrics));
        }
    }

    /// Check if the current tool matches a pipe-separated pattern.
    ///
    /// Pattern format: "Write|Edit|MultiEdit" — matches if tool_name equals any segment.
    pub fn tool_matches(&self, pattern: &str) -> bool {
        match &self.tool_name {
            Some(tool) => pattern.split('|').any(|p| p.trim() == tool),
            None => false,
        }
    }

    /// Extract output from a mutable reference, replacing fields with defaults.
    ///
    /// This is the **Single Source of Truth** for output generation — both owned
    /// (`into_output`) and borrowed (`take_output`) paths use the same logic.
    /// `pipeline.rs` must call this instead of duplicating the output logic.
    pub fn take_output(&mut self) -> CortexOutput {
        let context_lines = std::mem::take(&mut self.context_lines);
        let scored_lines = std::mem::take(&mut self.scored_lines);
        let decision = std::mem::replace(&mut self.decision, Decision::Skip);
        let handler_metrics = std::mem::take(&mut self.handler_metrics);
        let merged = Self::merge_and_rank(context_lines, scored_lines);
        Self::build_output(merged, decision, handler_metrics, &self.event)
    }

    /// Convert accumulated context + decision into the final output JSON.
    ///
    /// Claude Code accepts `hookSpecificOutput` with `additionalContext` for ALL
    /// event types. The `hookEventName` field is set to the event's canonical name.
    pub fn into_output(self) -> CortexOutput {
        let merged = Self::merge_and_rank(self.context_lines, self.scored_lines);
        Self::build_output(merged, self.decision, self.handler_metrics, &self.event)
    }

    /// Merge unscored context_lines (default score 0.5) with scored_lines,
    /// then sort descending by score so highest-relevance survives truncation.
    fn merge_and_rank(unscored: Vec<String>, mut scored: Vec<(f32, String)>) -> Vec<String> {
        // Assign default score to unscored lines
        for line in unscored {
            scored.push((0.5, line));
        }
        // Sort descending by score (highest first)
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().map(|(_, text)| text).collect()
    }

    /// Shared output builder — Single Source of Truth.
    fn build_output(
        context_lines: Vec<String>,
        decision: Decision,
        handler_metrics: Vec<(String, serde_json::Value)>,
        event: &HookEvent,
    ) -> CortexOutput {
        let has_context = !context_lines.is_empty();
        let is_blocked = matches!(decision, Decision::Block(_));
        let hook_output_name = event.hook_output_name();

        // Build metrics object if any handlers produced metrics
        let metrics = if handler_metrics.is_empty() {
            None
        } else {
            let mut map = serde_json::Map::new();
            for (name, value) in handler_metrics {
                map.insert(name, value);
            }
            Some(serde_json::Value::Object(map))
        };

        // Build context string
        let context_str = if has_context {
            if is_blocked {
                context_lines.join("\n")
            } else {
                context_lines.join(" | ")
            }
        } else {
            String::new()
        };

        if is_blocked {
            let reason = match &decision {
                Decision::Block(r) => r.clone(),
                _ => "blocked".to_string(),
            };
            // For block decisions, use hookSpecificOutput only for supported events,
            // otherwise use systemMessage
            let (hso, sys_msg) = if has_context {
                if let Some(name) = hook_output_name {
                    (
                        Some(HookSpecificOutput {
                            hook_event_name: name.to_string(),
                            additional_context: context_str,
                        }),
                        None,
                    )
                } else {
                    (None, Some(context_str))
                }
            } else {
                (None, None)
            };
            CortexOutput {
                hook_specific_output: hso,
                suppress_output: true,
                decision: Some("block".to_string()),
                reason: Some(reason),
                system_message: sys_msg,
                metrics,
            }
        } else if has_context {
            // For context injection, use hookSpecificOutput for PreToolUse/PostToolUse/
            // UserPromptSubmit, and systemMessage for all other events (Stop, SessionStart, etc.)
            let (hso, sys_msg) = if let Some(name) = hook_output_name {
                (
                    Some(HookSpecificOutput {
                        hook_event_name: name.to_string(),
                        additional_context: context_str,
                    }),
                    None,
                )
            } else {
                (None, Some(context_str))
            };
            CortexOutput {
                hook_specific_output: hso,
                suppress_output: true,
                decision: None,
                reason: None,
                system_message: sys_msg,
                metrics,
            }
        } else {
            // No context, no block → empty output (approve silently)
            CortexOutput {
                hook_specific_output: None,
                suppress_output: false,
                decision: None,
                reason: None,
                system_message: None,
                metrics,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    #[allow(clippy::arc_with_non_send_sync)] // single-threaded test context
    fn make_test_knowledge() -> (TempDir, Arc<KnowledgeRef>) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        (tmp, Arc::new(db))
    }

    #[test]
    fn test_context_from_input_pretool() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({
            "tool_name": "Read",
            "tool_input": {
                "file_path": "/tmp/test.py"
            },
            "session_id": "session-abc123"
        });

        let ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        assert_eq!(ctx.event, HookEvent::PreToolUse);
        assert_eq!(ctx.tool_name.as_deref(), Some("Read"));
        assert_eq!(ctx.file_path.as_deref(), Some("/tmp/test.py"));
        assert_eq!(ctx.session_id, "session-abc123");
        assert!(ctx.context_lines.is_empty());
        assert_eq!(ctx.decision, Decision::Skip);
    }

    #[test]
    fn test_context_from_input_no_tool() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({
            "session_id": "s1"
        });

        let ctx = CortexContext::from_input(
            HookEvent::SessionStart,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        assert!(ctx.tool_name.is_none());
        assert!(ctx.file_path.is_none());
    }

    #[test]
    fn test_context_merge_accumulates() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        // First handler: Allow with context
        let r1 = HandlerResult {
            decision: Decision::Allow,
            context_lines: vec!["line1".to_string()],
            metrics: serde_json::json!({"handler1": 42}),
            handler_name: "h1".to_string(),
            duration_ms: 1.0,
        };
        ctx.merge_result(r1);

        assert_eq!(ctx.context_lines.len(), 1);
        assert_eq!(ctx.decision, Decision::Allow);
        assert_eq!(ctx.handler_metrics.len(), 1);

        // Second handler: Allow with more context
        let r2 = HandlerResult {
            decision: Decision::Allow,
            context_lines: vec!["line2".to_string(), "line3".to_string()],
            metrics: serde_json::Value::Null,
            handler_name: "h2".to_string(),
            duration_ms: 2.0,
        };
        ctx.merge_result(r2);

        assert_eq!(ctx.context_lines.len(), 3);
        assert_eq!(ctx.decision, Decision::Allow);
        // Null metrics not accumulated
        assert_eq!(ctx.handler_metrics.len(), 1);
    }

    #[test]
    fn test_context_merge_decision_escalation() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        // Skip result
        ctx.merge_result(HandlerResult::skip("h1"));
        assert_eq!(ctx.decision, Decision::Skip);

        // Allow result escalates from Skip
        ctx.merge_result(HandlerResult::allow("h2", None));
        assert_eq!(ctx.decision, Decision::Allow);

        // Block result escalates from Allow
        ctx.merge_result(HandlerResult::block("h3", "blocked!".to_string()));
        assert!(matches!(ctx.decision, Decision::Block(_)));
    }

    #[test]
    fn test_tool_matching() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({"tool_name": "Edit"});
        let ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        assert!(ctx.tool_matches("Edit"));
        assert!(ctx.tool_matches("Write|Edit|MultiEdit"));
        assert!(!ctx.tool_matches("Read"));
        assert!(!ctx.tool_matches("Bash"));
        assert!(!ctx.tool_matches("Write|MultiEdit"));
    }

    #[test]
    fn test_tool_matching_no_tool() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let ctx = CortexContext::from_input(
            HookEvent::SessionStart,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        assert!(!ctx.tool_matches("Read"));
        assert!(!ctx.tool_matches("Write|Edit"));
    }

    #[test]
    fn test_into_output_empty() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        let output = ctx.into_output();
        assert!(output.hook_specific_output.is_none());
        assert!(!output.suppress_output);
        assert!(output.decision.is_none());
        assert!(output.reason.is_none());
    }

    #[test]
    fn test_into_output_with_context() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        ctx.merge_result(HandlerResult::allow("h1", Some("signal1".to_string())));
        ctx.merge_result(HandlerResult::allow("h2", Some("signal2".to_string())));

        let output = ctx.into_output();
        assert!(output.hook_specific_output.is_some());
        let hso = output.hook_specific_output.unwrap();
        assert_eq!(hso.hook_event_name, "PreToolUse");
        assert!(hso.additional_context.contains("signal1"));
        assert!(hso.additional_context.contains("signal2"));
        assert!(output.suppress_output);
        assert!(output.decision.is_none());
    }

    #[test]
    fn test_into_output_blocked() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        ctx.merge_result(HandlerResult::block("h1", "bad input".to_string()));

        let output = ctx.into_output();
        assert_eq!(output.decision.as_deref(), Some("block"));
        assert_eq!(output.reason.as_deref(), Some("bad input"));
        assert!(output.suppress_output);
    }

    #[test]
    fn test_context_budget_initial_prompt() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({"prompt": "hello"});
        let ctx = CortexContext::from_input(
            HookEvent::UserPromptSubmit,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.context_budget_remaining, 400);
    }

    #[test]
    fn test_context_budget_initial_pretool() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.context_budget_remaining, 300);
    }

    #[test]
    fn test_context_budget_decrements_on_merge() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PostToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.context_budget_remaining, 500);

        ctx.merge_result(HandlerResult::allow("h1", Some("12345".to_string())));
        assert_eq!(ctx.context_budget_remaining, 495); // 500 - 5

        ctx.merge_result(HandlerResult::allow("h2", Some("x".repeat(100))));
        assert_eq!(ctx.context_budget_remaining, 395); // 495 - 100
    }

    #[test]
    fn test_context_budget_saturates_at_zero() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreCompact,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.context_budget_remaining, 250);

        ctx.merge_result(HandlerResult::allow("h1", Some("x".repeat(300))));
        assert_eq!(ctx.context_budget_remaining, 0); // saturating_sub
    }

    #[test]
    fn test_cortex_output_json_format() {
        // Verify the output matches Claude Code's expected format
        let output = CortexOutput {
            hook_specific_output: Some(HookSpecificOutput {
                hook_event_name: "PreToolUse".to_string(),
                additional_context: "some context here".to_string(),
            }),
            suppress_output: true,
            decision: None,
            reason: None,
            system_message: None,
            metrics: None,
        };

        let json = serde_json::to_value(&output).unwrap();
        assert!(json.get("hookSpecificOutput").is_some());
        assert_eq!(json["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(
            json["hookSpecificOutput"]["additionalContext"],
            "some context here"
        );
        assert_eq!(json["suppressOutput"], true);
        // Null fields skipped
        assert!(json.get("decision").is_none());
        assert!(json.get("reason").is_none());
        assert!(json.get("metrics").is_none());
    }

    // I-1: Tests for systemMessage fallback (events that DON'T support hookSpecificOutput)

    #[test]
    fn test_into_output_session_start_uses_system_message() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::SessionStart,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx.context_lines.push("session context info".to_string());

        let output = ctx.into_output();
        // SessionStart does NOT support hookSpecificOutput → must use systemMessage
        assert!(output.hook_specific_output.is_none());
        assert_eq!(
            output.system_message.as_deref(),
            Some("session context info")
        );
        assert!(output.suppress_output);
    }

    #[test]
    fn test_into_output_session_start_blocked_uses_system_message() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::SessionStart,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx.context_lines.push("blocked reason".to_string());
        ctx.decision = Decision::Block("security violation".to_string());

        let output = ctx.into_output();
        assert!(output.hook_specific_output.is_none());
        assert_eq!(output.system_message.as_deref(), Some("blocked reason"));
        assert_eq!(output.decision.as_deref(), Some("block"));
        assert_eq!(output.reason.as_deref(), Some("security violation"));
    }

    // I-2: Test take_output parity with into_output

    #[test]
    fn test_take_output_matches_into_output() {
        let (_tmp, knowledge) = make_test_knowledge();

        // Build two identical contexts
        let input1 = serde_json::json!({"tool_name": "Read"});
        let mut ctx1 = CortexContext::from_input(
            HookEvent::PreToolUse,
            input1,
            knowledge.clone(),
            std::path::PathBuf::from("/project"),
        );
        ctx1.context_lines.push("test context".to_string());

        let input2 = serde_json::json!({"tool_name": "Read"});
        let mut ctx2 = CortexContext::from_input(
            HookEvent::PreToolUse,
            input2,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx2.context_lines.push("test context".to_string());

        let output_owned = ctx1.into_output();
        let output_taken = ctx2.take_output();

        // Both must produce identical JSON
        let json1 = serde_json::to_value(&output_owned).unwrap();
        let json2 = serde_json::to_value(&output_taken).unwrap();
        assert_eq!(json1, json2);
    }

    #[test]
    fn test_take_output_drains_context() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PostToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx.context_lines.push("some context".to_string());

        let output1 = ctx.take_output();
        assert!(output1.suppress_output); // has context

        // Second call: context is drained → empty/silent output
        let output2 = ctx.take_output();
        assert!(!output2.suppress_output); // no context
        assert!(output2.hook_specific_output.is_none());
        assert!(output2.system_message.is_none());
    }

    // ── Additional context tests ──────────────────────────────────────

    #[test]
    fn test_context_session_id_default_unknown() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let ctx = CortexContext::from_input(
            HookEvent::SessionStart,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.session_id, "unknown");
    }

    #[test]
    fn test_context_session_id_from_input() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({ "session_id": "my-session-42" });
        let ctx = CortexContext::from_input(
            HookEvent::Stop,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.session_id, "my-session-42");
    }

    #[test]
    fn test_context_tool_input_extracted() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": { "command": "cargo test" }
        });
        let ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.tool_name.as_deref(), Some("Bash"));
        assert_eq!(ctx.tool_input["command"], "cargo test");
    }

    #[test]
    fn test_context_file_path_extracted_from_tool_input() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": { "file_path": "/src/main.rs" }
        });
        let ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.file_path.as_deref(), Some("/src/main.rs"));
    }

    #[test]
    fn test_context_budget_precompact() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let ctx = CortexContext::from_input(
            HookEvent::PreCompact,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.context_budget_remaining, 250);
    }

    #[test]
    fn test_context_budget_postcompact_large() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let ctx = CortexContext::from_input(
            HookEvent::PostCompact,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.context_budget_remaining, 3000);
    }

    #[test]
    fn test_context_budget_session_start() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let ctx = CortexContext::from_input(
            HookEvent::SessionStart,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.context_budget_remaining, 300);
    }

    #[test]
    fn test_context_initial_decision_is_skip() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert_eq!(ctx.decision, Decision::Skip);
        assert!(ctx.context_lines.is_empty());
        assert!(ctx.handler_metrics.is_empty());
    }

    #[test]
    fn test_context_merge_null_metrics_not_accumulated() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        // Null metrics should not be stored
        ctx.merge_result(HandlerResult {
            decision: Decision::Allow,
            context_lines: vec!["ctx".to_string()],
            metrics: serde_json::Value::Null,
            handler_name: "h1".to_string(),
            duration_ms: 1.0,
        });
        assert_eq!(ctx.handler_metrics.len(), 0);
    }

    #[test]
    fn test_context_merge_non_null_metrics_accumulated() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx.merge_result(HandlerResult {
            decision: Decision::Allow,
            context_lines: vec![],
            metrics: serde_json::json!({"score": 0.9}),
            handler_name: "h1".to_string(),
            duration_ms: 1.0,
        });
        assert_eq!(ctx.handler_metrics.len(), 1);
        assert_eq!(ctx.handler_metrics[0].0, "h1");
    }

    #[test]
    fn test_into_output_stop_uses_system_message() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::Stop,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx.context_lines.push("stop context".to_string());
        let output = ctx.into_output();
        // Stop does NOT support hookSpecificOutput
        assert!(output.hook_specific_output.is_none());
        assert_eq!(output.system_message.as_deref(), Some("stop context"));
    }

    #[test]
    fn test_into_output_pretooluse_uses_hook_specific_output() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx.context_lines.push("pre-tool context".to_string());
        let output = ctx.into_output();
        // PreToolUse DOES support hookSpecificOutput
        assert!(output.hook_specific_output.is_some());
        assert!(output.system_message.is_none());
        let hso = output.hook_specific_output.unwrap();
        assert_eq!(hso.hook_event_name, "PreToolUse");
        assert_eq!(hso.additional_context, "pre-tool context");
    }

    #[test]
    fn test_into_output_posttooluse_uses_hook_specific_output() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PostToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx.context_lines.push("post-tool info".to_string());
        let output = ctx.into_output();
        assert!(output.hook_specific_output.is_some());
        let hso = output.hook_specific_output.unwrap();
        assert_eq!(hso.hook_event_name, "PostToolUse");
    }

    #[test]
    fn test_into_output_userpromptsubmit_uses_hook_specific_output() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::UserPromptSubmit,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx.context_lines.push("prompt hint".to_string());
        let output = ctx.into_output();
        assert!(output.hook_specific_output.is_some());
        let hso = output.hook_specific_output.unwrap();
        assert_eq!(hso.hook_event_name, "UserPromptSubmit");
    }

    #[test]
    fn test_context_project_root_is_stored() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let project = std::path::PathBuf::from("/my/project/root");
        let ctx =
            CortexContext::from_input(HookEvent::SessionStart, input, knowledge, project.clone());
        assert_eq!(ctx.project_root, project);
    }

    #[test]
    fn test_tool_matches_pipe_separated_patterns() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({ "tool_name": "Write" });
        let ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        assert!(ctx.tool_matches("Write|Edit|MultiEdit"));
        assert!(ctx.tool_matches("Write"));
        assert!(!ctx.tool_matches("Edit|MultiEdit"));
        assert!(!ctx.tool_matches("Read"));
    }

    #[test]
    fn test_merge_multiple_context_lines() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        ctx.merge_result(HandlerResult {
            decision: Decision::Allow,
            context_lines: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            metrics: serde_json::Value::Null,
            handler_name: "h".to_string(),
            duration_ms: 0.0,
        });
        assert_eq!(ctx.context_lines.len(), 3);
    }

    // ── E1-S2: Context deduplication tests ───────────────────────────

    #[test]
    fn test_context_dedup_removes_duplicates() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        // Handler 1 injects "same_context"
        ctx.merge_result(HandlerResult::allow("h1", Some("same_context".to_string())));
        // Handler 2 injects same string — should be deduplicated
        ctx.merge_result(HandlerResult::allow("h2", Some("same_context".to_string())));
        // Handler 3 injects different string — should be kept
        ctx.merge_result(HandlerResult::allow(
            "h3",
            Some("different_context".to_string()),
        ));

        assert_eq!(
            ctx.context_lines.len(),
            2,
            "Duplicate context should be removed"
        );
        assert!(ctx.context_lines.contains(&"same_context".to_string()));
        assert!(ctx.context_lines.contains(&"different_context".to_string()));
    }

    #[test]
    fn test_context_dedup_budget_only_counts_unique() {
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PostToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        let initial_budget = ctx.context_budget_remaining; // 500
        let content = "12345"; // 5 chars

        ctx.merge_result(HandlerResult::allow("h1", Some(content.to_string())));
        assert_eq!(ctx.context_budget_remaining, initial_budget - 5);

        // Duplicate should NOT consume additional budget
        ctx.merge_result(HandlerResult::allow("h2", Some(content.to_string())));
        assert_eq!(
            ctx.context_budget_remaining,
            initial_budget - 5,
            "Duplicate context should not consume budget"
        );
    }
}
