//! Session-Aware Hints — Dynamic tool suggestions based on session history.
//!
//! Tracks which MCP tools were called during a session, infers the user's
//! intent (reviewing, debugging, refactoring, etc.), and generates
//! context-aware suggestions that evolve as the session progresses.
//!
//! Inspired by code-review-graph's `hints.py` session state tracking.

use serde::Serialize;
use std::collections::VecDeque;

/// Maximum number of tool calls to keep in history.
const MAX_HISTORY: usize = 20;

/// A recorded tool call in the session.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Name of the tool that was invoked.
    pub tool_name: String,
    /// Unix timestamp of the call in milliseconds.
    pub timestamp_ms: u64,
}

/// Inferred session intent based on tool call patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionIntent {
    /// User is reviewing code (reads, audits, metadata lookups).
    Reviewing,
    /// User is diagnosing a failure or unexpected behaviour.
    Debugging,
    /// User is restructuring or renaming code.
    Refactoring,
    /// User is navigating an unfamiliar codebase.
    Exploring,
    /// User is designing or decomposing upcoming work.
    Planning,
    /// User is studying how something works.
    Learning,
    /// Intent could not be inferred from the call history.
    Unknown,
}

impl SessionIntent {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Reviewing => "reviewing",
            Self::Debugging => "debugging",
            Self::Refactoring => "refactoring",
            Self::Exploring => "exploring",
            Self::Planning => "planning",
            Self::Learning => "learning",
            Self::Unknown => "unknown",
        }
    }
}

/// A context-aware tool suggestion.
#[derive(Debug, Clone, Serialize)]
pub struct SessionHint {
    /// Name of the suggested tool.
    pub tool: String,
    /// Rationale for suggesting the tool.
    pub reason: String,
    /// Confidence in the suggestion (0.0-1.0).
    pub confidence: f64,
}

/// Session hint engine that tracks tool call history and infers intent.
#[derive(Debug)]
pub struct SessionHintEngine {
    history: VecDeque<ToolCall>,
    current_intent: SessionIntent,
}

impl Default for SessionHintEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionHintEngine {
    /// Create a new engine with empty history.
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(MAX_HISTORY),
            current_intent: SessionIntent::Exploring,
        }
    }

    /// Record a tool call and update inferred intent.
    pub fn record_call(&mut self, tool_name: &str) {
        if self.history.len() >= MAX_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(ToolCall {
            tool_name: tool_name.to_string(),
            timestamp_ms: 0, // simplified; real impl would use std::time
        });
        self.current_intent = self.infer_intent();
    }

    /// Get the current inferred intent.
    pub fn intent(&self) -> SessionIntent {
        self.current_intent
    }

    /// Get the number of recorded calls.
    pub fn call_count(&self) -> usize {
        self.history.len()
    }

    /// Generate context-aware hints based on history + intent + last tool.
    pub fn get_hints(&self, last_tool: &str, max_hints: usize) -> Vec<SessionHint> {
        let intent = self.current_intent;
        let mut hints = self.suggest_for_intent(intent, last_tool);

        // Deduplicate: don't suggest tools already called recently
        let recent: Vec<&str> = self
            .history
            .iter()
            .rev()
            .take(3)
            .map(|t| t.tool_name.as_str())
            .collect();
        hints.retain(|h| !recent.contains(&h.tool.as_str()));

        hints.truncate(max_hints);
        hints
    }

    /// Infer intent from tool call history using weighted scoring.
    fn infer_intent(&self) -> SessionIntent {
        if self.history.is_empty() {
            return SessionIntent::Exploring;
        }

        let mut scores = [0.0_f64; 6]; // Reviewing, Debugging, Refactoring, Exploring, Planning, Learning

        for (i, call) in self.history.iter().enumerate() {
            // Recency bias: last 5 calls get 2x weight
            let recency = if i >= self.history.len().saturating_sub(5) {
                2.0
            } else {
                1.0
            };

            let tool = call.tool_name.as_str();
            match tool {
                "touring_detect_changes" | "touring_wiring_audit" => {
                    scores[0] += 1.5 * recency; // Reviewing
                }
                "touring_gotcha" => {
                    scores[0] += 1.0 * recency; // Reviewing
                    scores[1] += 0.5 * recency; // Debugging
                }
                "touring_ast_find" => {
                    scores[1] += 1.0 * recency; // Debugging
                    scores[2] += 0.5 * recency; // Refactoring
                }
                "touring_memory_recall" => {
                    scores[1] += 0.8 * recency; // Debugging
                    scores[5] += 0.3 * recency; // Learning
                }
                "touring_ast_overview" => {
                    scores[2] += 0.8 * recency; // Refactoring
                    scores[3] += 0.5 * recency; // Exploring
                }
                "touring_speculate" | "touring_refactor" | "touring_ast_edit" => {
                    scores[2] += 1.5 * recency; // Refactoring
                }
                "touring_graph" => {
                    scores[2] += 0.5 * recency; // Refactoring
                    scores[0] += 0.5 * recency; // Reviewing
                }
                "touring_minimal_context"
                | "touring_wiring"
                | "touring_index_status"
                | "touring_project" => {
                    scores[3] += 1.0 * recency; // Exploring
                }
                "touring_decompose" | "touring_session" | "touring_mcts_search" => {
                    scores[4] += 1.5 * recency; // Planning
                }
                "touring_memory_store"
                | "touring_evolve"
                | "touring_learn_pattern"
                | "touring_cluster_skills" => {
                    scores[5] += 1.5 * recency; // Learning
                }
                _ => {
                    scores[3] += 0.1 * recency; // Unknown → slight Exploring
                }
            }
        }

        let intents = [
            SessionIntent::Reviewing,
            SessionIntent::Debugging,
            SessionIntent::Refactoring,
            SessionIntent::Exploring,
            SessionIntent::Planning,
            SessionIntent::Learning,
        ];

        intents
            .iter()
            .zip(scores.iter())
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(intent, _)| *intent)
            .unwrap_or(SessionIntent::Unknown)
    }

    /// Generate suggestions based on intent and last tool called.
    fn suggest_for_intent(&self, intent: SessionIntent, last_tool: &str) -> Vec<SessionHint> {
        match intent {
            SessionIntent::Reviewing => self.review_suggestions(last_tool),
            SessionIntent::Debugging => self.debug_suggestions(last_tool),
            SessionIntent::Refactoring => self.refactor_suggestions(last_tool),
            SessionIntent::Exploring => self.explore_suggestions(last_tool),
            SessionIntent::Planning => self.planning_suggestions(last_tool),
            SessionIntent::Learning => self.learning_suggestions(last_tool),
            SessionIntent::Unknown => self.explore_suggestions(last_tool),
        }
    }

    fn review_suggestions(&self, last_tool: &str) -> Vec<SessionHint> {
        match last_tool {
            "touring_detect_changes" => vec![
                hint(
                    "touring_wiring_audit",
                    "Audit wiring of affected modules",
                    0.9,
                ),
                hint("touring_gotcha", "Check pitfalls in changed files", 0.8),
                hint("touring_ast_find", "Inspect high-risk symbols", 0.7),
            ],
            "touring_wiring_audit" => vec![
                hint("touring_ast_find", "Locate orphan symbol definitions", 0.9),
                hint("touring_memory_recall", "Check past similar reviews", 0.7),
            ],
            "touring_gotcha" => vec![
                hint(
                    "touring_memory_recall",
                    "Recall fixes for matched pitfalls",
                    0.9,
                ),
                hint(
                    "touring_detect_changes",
                    "Re-check risk after addressing issues",
                    0.7,
                ),
            ],
            _ => vec![
                hint(
                    "touring_detect_changes",
                    "Start with risk-scored change analysis",
                    0.9,
                ),
                hint(
                    "touring_wiring_audit",
                    "Audit integration completeness",
                    0.7,
                ),
            ],
        }
    }

    fn debug_suggestions(&self, last_tool: &str) -> Vec<SessionHint> {
        match last_tool {
            "touring_gotcha" => vec![
                hint(
                    "touring_memory_recall",
                    "Recall past fixes for this pattern",
                    0.9,
                ),
                hint("touring_ast_find", "Locate the problematic function", 0.8),
            ],
            "touring_ast_find" => vec![
                hint("touring_graph", "Trace blast radius of found symbol", 0.9),
                hint("touring_speculate", "Test a fix speculatively", 0.8),
            ],
            "touring_memory_recall" => vec![
                hint(
                    "touring_ast_find",
                    "Locate symbol from recalled pattern",
                    0.9,
                ),
                hint("touring_gotcha", "Check known pitfalls", 0.7),
            ],
            _ => vec![
                hint("touring_gotcha", "Check known pitfalls first", 0.9),
                hint("touring_memory_recall", "Recall past similar bugs", 0.8),
                hint("touring_ast_find", "Find the suspect function", 0.7),
            ],
        }
    }

    fn refactor_suggestions(&self, last_tool: &str) -> Vec<SessionHint> {
        match last_tool {
            "touring_ast_overview" => vec![
                hint("touring_ast_find", "Drill into specific symbols", 0.9),
                hint(
                    "touring_graph",
                    "Check dependencies before refactoring",
                    0.8,
                ),
            ],
            "touring_graph" => vec![
                hint("touring_speculate", "Test refactoring speculatively", 0.9),
                hint("touring_ast_find", "Find all references to rename", 0.8),
            ],
            "touring_speculate" => vec![
                hint("touring_wiring_audit", "Verify wiring after changes", 0.9),
                hint("touring_ast_edit", "Apply the validated refactoring", 0.8),
            ],
            _ => vec![
                hint("touring_ast_overview", "Map file structure first", 0.9),
                hint("touring_graph", "Check blast radius", 0.8),
            ],
        }
    }

    fn explore_suggestions(&self, last_tool: &str) -> Vec<SessionHint> {
        match last_tool {
            "touring_minimal_context" => vec![
                hint("touring_wiring", "Check wiring health", 0.9),
                hint("touring_ast_overview", "Explore key files", 0.8),
            ],
            "touring_wiring" => vec![
                hint("touring_wiring_audit", "Deep audit", 0.9),
                hint("touring_gotcha", "Check known issues", 0.7),
            ],
            _ => vec![
                hint("touring_minimal_context", "Get project overview first", 0.9),
                hint("touring_wiring", "Check integration health", 0.7),
            ],
        }
    }

    fn planning_suggestions(&self, last_tool: &str) -> Vec<SessionHint> {
        match last_tool {
            "touring_decompose" => vec![
                hint("touring_suggest", "Get RL-guided task ordering", 0.9),
                hint(
                    "touring_graph",
                    "Check blast radius of planned changes",
                    0.8,
                ),
            ],
            "touring_session" => vec![
                hint("touring_decompose", "Break down tasks", 0.9),
                hint("touring_memory_recall", "Recall relevant patterns", 0.7),
            ],
            _ => vec![
                hint("touring_session", "Start a session", 0.9),
                hint("touring_decompose", "Create task decomposition", 0.8),
            ],
        }
    }

    fn learning_suggestions(&self, last_tool: &str) -> Vec<SessionHint> {
        match last_tool {
            "touring_memory_store" => vec![
                hint(
                    "touring_evolve",
                    "Extract patterns from stored knowledge",
                    0.9,
                ),
                hint("touring_cluster_skills", "Cluster related skills", 0.7),
            ],
            "touring_evolve" => vec![
                hint("touring_memory_recall", "Verify evolution insights", 0.9),
                hint(
                    "touring_learn_pattern",
                    "Update Q-table with learnings",
                    0.7,
                ),
            ],
            _ => vec![
                hint("touring_memory_store", "Persist current insights", 0.9),
                hint("touring_evolve", "Run evolution analysis", 0.8),
            ],
        }
    }
}

fn hint(tool: &str, reason: &str, confidence: f64) -> SessionHint {
    SessionHint {
        tool: tool.to_string(),
        reason: reason.to_string(),
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_engine_is_exploring() {
        let engine = SessionHintEngine::new();
        assert_eq!(engine.intent(), SessionIntent::Exploring);
    }

    #[test]
    fn test_record_call_updates_history() {
        let mut engine = SessionHintEngine::new();
        engine.record_call("touring_ast_find");
        assert_eq!(engine.call_count(), 1);
        engine.record_call("touring_memory_recall");
        assert_eq!(engine.call_count(), 2);
    }

    #[test]
    fn test_max_history_bounded() {
        let mut engine = SessionHintEngine::new();
        for i in 0..30 {
            engine.record_call(&format!("tool_{}", i));
        }
        assert_eq!(engine.call_count(), MAX_HISTORY);
    }

    #[test]
    fn test_intent_reviewing() {
        let mut engine = SessionHintEngine::new();
        engine.record_call("touring_detect_changes");
        engine.record_call("touring_wiring_audit");
        engine.record_call("touring_gotcha");
        assert_eq!(engine.intent(), SessionIntent::Reviewing);
    }

    #[test]
    fn test_intent_debugging() {
        let mut engine = SessionHintEngine::new();
        engine.record_call("touring_ast_find");
        engine.record_call("touring_memory_recall");
        engine.record_call("touring_gotcha");
        engine.record_call("touring_ast_find");
        assert_eq!(engine.intent(), SessionIntent::Debugging);
    }

    #[test]
    fn test_intent_refactoring() {
        let mut engine = SessionHintEngine::new();
        engine.record_call("touring_ast_overview");
        engine.record_call("touring_speculate");
        engine.record_call("touring_refactor");
        assert_eq!(engine.intent(), SessionIntent::Refactoring);
    }

    #[test]
    fn test_intent_planning() {
        let mut engine = SessionHintEngine::new();
        engine.record_call("touring_session");
        engine.record_call("touring_decompose");
        engine.record_call("touring_mcts_search");
        assert_eq!(engine.intent(), SessionIntent::Planning);
    }

    #[test]
    fn test_intent_learning() {
        let mut engine = SessionHintEngine::new();
        engine.record_call("touring_memory_store");
        engine.record_call("touring_evolve");
        engine.record_call("touring_learn_pattern");
        assert_eq!(engine.intent(), SessionIntent::Learning);
    }

    #[test]
    fn test_intent_evolves_with_recency() {
        let mut engine = SessionHintEngine::new();
        // Start with review tools
        engine.record_call("touring_detect_changes");
        engine.record_call("touring_wiring_audit");
        assert_eq!(engine.intent(), SessionIntent::Reviewing);

        // Switch to debugging tools — recency should shift intent
        engine.record_call("touring_ast_find");
        engine.record_call("touring_memory_recall");
        engine.record_call("touring_ast_find");
        engine.record_call("touring_gotcha");
        engine.record_call("touring_ast_find");
        assert_eq!(engine.intent(), SessionIntent::Debugging);
    }

    #[test]
    fn test_hints_exclude_recent_calls() {
        let mut engine = SessionHintEngine::new();
        engine.record_call("touring_detect_changes");
        let hints = engine.get_hints("touring_detect_changes", 5);
        // Should NOT suggest touring_detect_changes again
        assert!(
            !hints.iter().any(|h| h.tool == "touring_detect_changes"),
            "Should not suggest the tool just called"
        );
    }

    #[test]
    fn test_hints_respect_max() {
        let mut engine = SessionHintEngine::new();
        engine.record_call("touring_detect_changes");
        let hints = engine.get_hints("touring_detect_changes", 1);
        assert!(hints.len() <= 1);
    }

    #[test]
    fn test_hints_confidence_ordering() {
        let mut engine = SessionHintEngine::new();
        engine.record_call("touring_detect_changes");
        let hints = engine.get_hints("touring_detect_changes", 5);
        for i in 1..hints.len() {
            assert!(
                hints[i - 1].confidence >= hints[i].confidence,
                "Hints should be ordered by confidence"
            );
        }
    }

    #[test]
    fn test_default_trait() {
        let engine = SessionHintEngine::default();
        assert_eq!(engine.call_count(), 0);
        assert_eq!(engine.intent(), SessionIntent::Exploring);
    }
}
