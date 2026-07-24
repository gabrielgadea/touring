//! Tool Suggestions — Inline next-tool recommendations for MCP responses.
//!
//! Inspired by code-review-graph's `next_tool_suggestions` pattern.
//! Appends context-aware tool suggestions to every MCP tool response,
//! guiding the AI assistant to the optimal next step.

use serde::Serialize;
use serde_json::json;

/// A next-tool suggestion appended to MCP responses.
#[derive(Debug, Clone, Serialize)]
pub struct NextToolSuggestion {
    /// MCP tool name to call next
    pub tool: String,
    /// Why this tool is recommended
    pub reason: String,
    /// Confidence score (0.0-1.0) from RL or heuristic
    pub confidence: f64,
}

/// Mapping of tool → suggested follow-up tools.
///
/// Static heuristic layer; the RL bandit can override/rerank these.
static SUGGESTION_MAP: &[(&str, &[(&str, &str)])] = &[
    (
        "touring_ast_find",
        &[
            ("touring_graph", "Check blast radius of found symbols"),
            ("touring_wiring", "Verify wiring status of found symbols"),
            (
                "touring_memory_recall",
                "Recall past patterns for these symbols",
            ),
        ],
    ),
    (
        "touring_ast_overview",
        &[
            ("touring_ast_find", "Drill into specific symbols"),
            ("touring_graph", "Explore dependencies"),
        ],
    ),
    (
        "touring_memory_recall",
        &[
            (
                "touring_memory_store",
                "Persist new insights from this recall",
            ),
            ("touring_suggest", "Get RL-guided next action"),
        ],
    ),
    (
        "touring_wiring",
        &[
            ("touring_wiring_audit", "Full integration audit"),
            ("touring_gotcha", "Check for known pitfalls"),
        ],
    ),
    (
        "touring_wiring_audit",
        &[
            ("touring_ast_find", "Locate orphan symbol definitions"),
            ("touring_graph", "Trace dependency chains"),
        ],
    ),
    (
        "touring_graph",
        &[
            (
                "touring_ast_find",
                "Find symbol definitions in blast radius",
            ),
            ("touring_wiring", "Check wiring of affected modules"),
            ("touring_gotcha", "Match pitfalls in affected files"),
        ],
    ),
    (
        "touring_gotcha",
        &[
            ("touring_memory_recall", "Recall fixes for matched pitfalls"),
            ("touring_ast_find", "Locate code at pitfall locations"),
        ],
    ),
    (
        "touring_suggest",
        &[
            (
                "touring_memory_recall",
                "Recall context for suggested action",
            ),
            ("touring_ast_overview", "Explore suggested target file"),
        ],
    ),
    (
        "touring_session",
        &[
            ("touring_suggest", "Get RL-guided first action"),
            ("touring_memory_recall", "Recall relevant knowledge"),
        ],
    ),
    (
        "touring_decompose",
        &[
            ("touring_suggest", "Get RL-guided task ordering"),
            ("touring_graph", "Check blast radius of planned changes"),
        ],
    ),
    (
        "touring_speculate",
        &[
            ("touring_ast_find", "Verify speculated symbols exist"),
            (
                "touring_wiring",
                "Check wiring impact of speculated changes",
            ),
        ],
    ),
    (
        "touring_classify_intent",
        &[
            ("touring_suggest", "Get suggested actions for this intent"),
            (
                "touring_memory_recall",
                "Recall patterns for this intent type",
            ),
        ],
    ),
    (
        "touring_minimal_context",
        &[
            (
                "touring_ast_overview",
                "Explore files identified in context",
            ),
            ("touring_wiring_audit", "Audit identified risk areas"),
        ],
    ),
    (
        "touring_ctx_execute",
        &[
            (
                "touring_ast_overview",
                "Analyze files mentioned in execution output",
            ),
            (
                "touring_wiring_audit",
                "Check wiring impact of code changes",
            ),
            ("touring_memory_recall", "Recall similar execution patterns"),
        ],
    ),
];

/// Get next-tool suggestions for a given tool name.
///
/// Returns up to `max_suggestions` recommendations based on:
/// 1. Static heuristic mapping (always available)
/// 2. Optionally augmented by RL bandit scores (when available)
pub fn get_suggestions(tool_name: &str, max_suggestions: usize) -> Vec<NextToolSuggestion> {
    let base_suggestions: Vec<NextToolSuggestion> = SUGGESTION_MAP
        .iter()
        .find(|(name, _)| *name == tool_name)
        .map(|(_, suggestions)| {
            suggestions
                .iter()
                .enumerate()
                .map(|(i, (tool, reason))| NextToolSuggestion {
                    tool: tool.to_string(),
                    reason: reason.to_string(),
                    // Confidence decays with position (first suggestion = highest confidence)
                    confidence: 0.9 - (i as f64 * 0.15),
                })
                .collect()
        })
        .unwrap_or_else(|| {
            // Fallback: generic exploration suggestions
            vec![
                NextToolSuggestion {
                    tool: "touring_ast_overview".to_string(),
                    reason: "Explore file structure".to_string(),
                    confidence: 0.5,
                },
                NextToolSuggestion {
                    tool: "touring_memory_recall".to_string(),
                    reason: "Check for relevant patterns".to_string(),
                    confidence: 0.4,
                },
            ]
        });

    base_suggestions.into_iter().take(max_suggestions).collect()
}

/// Append `_next_tools` field to an MCP tool response JSON.
///
/// This is the middleware function: call it after each tool handler
/// to inject inline suggestions into the response.
/// Uses the static suggestion map as fallback.
pub fn append_to_response(
    response: &mut serde_json::Value,
    tool_name: &str,
    max_suggestions: usize,
) {
    let suggestions = get_suggestions(tool_name, max_suggestions);
    if let Some(obj) = response.as_object_mut() {
        obj.insert("_next_tools".to_string(), json!(suggestions));
    }
}

/// Append `_next_tools` using the session-aware hint engine.
///
/// Records the tool call in the engine's history, then generates
/// context-aware hints based on session intent + tool history.
/// Falls back to static suggestions if engine produces none.
pub fn append_with_session(
    response: &mut serde_json::Value,
    tool_name: &str,
    engine: &mut super::session_hints::SessionHintEngine,
    max_suggestions: usize,
) {
    engine.record_call(tool_name);
    let hints = engine.get_hints(tool_name, max_suggestions);

    let suggestions: Vec<serde_json::Value> = if hints.is_empty() {
        // Fallback to static
        get_suggestions(tool_name, max_suggestions)
            .into_iter()
            .map(|s| json!(s))
            .collect()
    } else {
        hints.into_iter().map(|h| json!(h)).collect()
    };

    if let Some(obj) = response.as_object_mut() {
        obj.insert("_next_tools".to_string(), json!(suggestions));
        obj.insert(
            "_session_intent".to_string(),
            json!(engine.intent().label()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_suggestions_known_tool() {
        let suggestions = get_suggestions("touring_ast_find", 3);
        assert!(
            !suggestions.is_empty(),
            "Should return suggestions for ast_find"
        );
        assert!(suggestions.len() <= 3);
        assert_eq!(suggestions[0].tool, "touring_graph");
        assert!(suggestions[0].confidence > suggestions[1].confidence);
    }

    #[test]
    fn test_get_suggestions_unknown_tool() {
        let suggestions = get_suggestions("nonexistent_tool", 3);
        assert!(
            !suggestions.is_empty(),
            "Should return fallback suggestions"
        );
        assert_eq!(suggestions[0].tool, "touring_ast_overview");
    }

    #[test]
    fn test_get_suggestions_limit() {
        let suggestions = get_suggestions("touring_ast_find", 1);
        assert_eq!(suggestions.len(), 1, "Should respect max_suggestions limit");
    }

    #[test]
    fn test_append_to_response() {
        let mut response = json!({"status": "ok", "data": [1, 2, 3]});
        append_to_response(&mut response, "touring_ast_find", 2);

        let next = response
            .get("_next_tools")
            .expect("Should have _next_tools field");
        assert!(next.is_array());
        let arr = next.as_array().expect("_next_tools should be array");
        assert!(arr.len() <= 2);
    }

    #[test]
    fn test_append_to_non_object() {
        let mut response = json!("just a string");
        append_to_response(&mut response, "touring_ast_find", 2);
        // Should not panic, just skip appending
        assert!(response.get("_next_tools").is_none());
    }

    #[test]
    fn test_confidence_decay() {
        let suggestions = get_suggestions("touring_graph", 3);
        for i in 1..suggestions.len() {
            assert!(
                suggestions[i - 1].confidence > suggestions[i].confidence,
                "Confidence should decay: {} > {}",
                suggestions[i - 1].confidence,
                suggestions[i].confidence,
            );
        }
    }

    #[test]
    fn test_all_mapped_tools_have_suggestions() {
        for (tool_name, suggestions) in SUGGESTION_MAP {
            let result = get_suggestions(tool_name, 10);
            assert!(
                !result.is_empty(),
                "Tool {} should have suggestions",
                tool_name
            );
            assert_eq!(
                result.len(),
                suggestions.len().min(10),
                "Tool {} suggestion count mismatch",
                tool_name
            );
        }
    }
}
