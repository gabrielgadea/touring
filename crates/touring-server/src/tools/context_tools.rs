//! Context Tools — Token-efficient entry points for MCP consumers.
//!
//! Inspired by code-review-graph's `get_minimal_context` tool (~100 tokens).
//! Provides ultra-compact context aggregation from multiple Touring subsystems.

use serde::Serialize;
use serde_json::json;

use crate::server::params::DetailLevel;

/// Compact project context (~100-150 tokens at minimal detail level).
#[derive(Debug, Clone, Serialize)]
pub struct MinimalContext {
    /// High-level statistics
    pub stats: ContextStats,
    /// Risk assessment (0.0-1.0) for the given scope
    pub risk_score: f64,
    /// Risk level label
    pub risk_level: String,
    /// Top issues (gotchas, orphans, etc.)
    pub top_issues: Vec<String>,
    /// Suggested next tools to call
    pub suggested_tools: Vec<ToolSuggestion>,
}

/// Aggregate statistics from index + wiring + knowledge subsystems.
#[derive(Debug, Clone, Serialize)]
pub struct ContextStats {
    /// Total number of symbols in the index.
    pub symbol_count: u64,
    /// Total number of source files tracked by the index.
    pub file_count: u64,
    /// Number of orphan `pub` symbols with no consumers.
    pub orphan_count: u64,
    /// End-to-end composite health score (0.0-1.0).
    pub e2e_health: f64,
    /// Current EMA reinforcement-learning reward.
    pub rl_reward: f64,
}

/// A tool suggestion with rationale.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSuggestion {
    /// Name of the suggested MCP/CLI tool to call next.
    pub tool_name: String,
    /// Human-readable rationale for the suggestion.
    pub reason: String,
}

/// Input bundle for [`build_minimal_context`] — groups the numeric subsystem stats
/// to keep the argument count within the 7-arg Clippy limit.
pub struct MinimalContextInput<'a> {
    /// Total number of symbols in the index.
    pub symbol_count: u64,
    /// Total number of source files tracked by the index.
    pub file_count: u64,
    /// Number of orphan `pub` symbols with no consumers.
    pub orphan_count: u64,
    /// End-to-end composite health score (0.0-1.0).
    pub e2e_health: f64,
    /// Current EMA reinforcement-learning reward.
    pub rl_reward: f64,
    /// Most relevant gotcha messages for the current scope.
    pub top_gotchas: Vec<String>,
    /// Optional task description used to bias tool suggestions.
    pub task_hint: Option<&'a str>,
    /// Verbosity level controlling how much context is emitted.
    pub detail_level: DetailLevel,
}

/// Build minimal context from available subsystem data.
///
/// This is the core aggregation function. It composes data from:
/// - Symbol index (file/symbol counts)
/// - Wiring (orphan counts, integration score)
/// - E2E health score
/// - RL reward signal
/// - Gotcha database (top issues)
///
/// The result is a compact JSON object designed to cost ≤150 tokens
/// when serialized, giving the AI assistant a complete picture of
/// the project state in a single call.
pub fn build_minimal_context(input: MinimalContextInput<'_>) -> serde_json::Value {
    let MinimalContextInput {
        symbol_count,
        file_count,
        orphan_count,
        e2e_health,
        rl_reward,
        top_gotchas,
        task_hint,
        detail_level,
    } = input;
    let risk_score = compute_risk_score(orphan_count, file_count, e2e_health);
    let risk_level = classify_risk(risk_score);
    let suggestions = generate_suggestions(task_hint, risk_score, orphan_count);

    let max_issues = detail_level.max_items();
    let max_suggestions = detail_level.max_items().min(5);

    let issues: Vec<&String> = top_gotchas.iter().take(max_issues).collect();
    let suggestions: Vec<&ToolSuggestion> = suggestions.iter().take(max_suggestions).collect();

    let mut result = json!({
        "stats": {
            "symbols": symbol_count,
            "files": file_count,
            "orphans": orphan_count,
            "health": format!("{:.2}", e2e_health),
            "rl_reward": format!("{:.3}", rl_reward),
        },
        "risk": {
            "score": format!("{:.2}", risk_score),
            "level": risk_level,
        },
        "top_issues": issues,
        "suggested_tools": suggestions,
    });

    if detail_level.include_summaries()
        && let Some(hint) = task_hint
        && let Some(obj) = result.as_object_mut()
    {
        obj.insert("task_context".to_string(), json!(hint));
    }

    // Add estimated token cost (chars/4 approximation from CRG)
    let serialized = serde_json::to_string(&result).unwrap_or_default();
    let estimated_tokens = serialized.len() / 4;
    if let Some(obj) = result.as_object_mut() {
        obj.insert("_estimated_tokens".to_string(), json!(estimated_tokens));
    }

    result
}

/// Estimate token count from a JSON value (chars/4 approximation).
///
/// Uses character count / 4 as rough approximation for English + code.
/// This matches the heuristic used by code-review-graph's token_benchmark.py.
pub fn estimate_tokens(value: &serde_json::Value) -> usize {
    serde_json::to_string(value)
        .map(|s| s.len() / 4)
        .unwrap_or(0)
}

/// Standardized compact response builder for Touring MCP tools.
///
/// Inspired by code-review-graph's `compact_response()`. Provides a consistent
/// structure that all tools can optionally use:
/// - summary: 1-2 sentence overview
/// - key_entities: top-N relevant symbols (capped at 10)
/// - risk: low/medium/high/critical
/// - next_tool_suggestions: up to 3 recommended tools
/// - _estimated_tokens: chars/4 cost estimate
pub fn compact_response(
    summary: &str,
    key_entities: Option<&[String]>,
    risk: Option<&str>,
    next_suggestions: Option<&[&str]>,
    data: Option<serde_json::Value>,
    detail_level: super::super::server::params::DetailLevel,
) -> serde_json::Value {
    let mut resp = json!({
        "status": "ok",
        "summary": summary,
    });

    if let Some(entities) = key_entities {
        let capped: Vec<&String> = entities.iter().take(10).collect();
        resp.as_object_mut()
            .map(|o| o.insert("key_entities".to_string(), json!(capped)));
    }

    if let Some(r) = risk {
        resp.as_object_mut()
            .map(|o| o.insert("risk".to_string(), json!(r)));
    }

    if let Some(suggestions) = next_suggestions {
        let capped: Vec<&&str> = suggestions.iter().take(3).collect();
        resp.as_object_mut()
            .map(|o| o.insert("next_tool_suggestions".to_string(), json!(capped)));
    }

    if detail_level.include_bodies()
        && let Some(d) = data
    {
        resp.as_object_mut()
            .map(|o| o.insert("data".to_string(), d));
    }

    // Add token cost estimate
    let tokens = estimate_tokens(&resp);
    resp.as_object_mut()
        .map(|o| o.insert("_estimated_tokens".to_string(), json!(tokens)));

    resp
}

/// Compute a risk score (0.0-1.0) from project health signals.
fn compute_risk_score(orphan_count: u64, file_count: u64, e2e_health: f64) -> f64 {
    if file_count == 0 {
        return 0.5; // no data
    }

    let orphan_ratio = (orphan_count as f64) / (file_count as f64).max(1.0);
    let orphan_penalty = (orphan_ratio * 0.3).min(0.3);
    let health_penalty = (1.0 - e2e_health) * 0.5;

    (orphan_penalty + health_penalty).min(1.0)
}

/// Classify risk score into human-readable level.
fn classify_risk(score: f64) -> String {
    match score {
        s if s < 0.2 => "low".to_string(),
        s if s < 0.5 => "medium".to_string(),
        s if s < 0.8 => "high".to_string(),
        _ => "critical".to_string(),
    }
}

/// Generate tool suggestions based on context.
fn generate_suggestions(
    task_hint: Option<&str>,
    risk_score: f64,
    orphan_count: u64,
) -> Vec<ToolSuggestion> {
    let mut suggestions = Vec::new();

    // Always suggest based on risk
    if risk_score > 0.5 {
        suggestions.push(ToolSuggestion {
            tool_name: "touring_wiring_audit".to_string(),
            reason: "High risk score — audit wiring integrity".to_string(),
        });
    }

    if orphan_count > 1000 {
        suggestions.push(ToolSuggestion {
            tool_name: "touring_wiring".to_string(),
            reason: format!("{} orphan symbols — review wiring status", orphan_count),
        });
    }

    // Task-aware suggestions
    if let Some(hint) = task_hint {
        let hint_lower = hint.to_lowercase();
        if hint_lower.contains("debug") || hint_lower.contains("fix") || hint_lower.contains("bug")
        {
            suggestions.push(ToolSuggestion {
                tool_name: "touring_gotcha".to_string(),
                reason: "Debugging task — check known pitfalls".to_string(),
            });
            suggestions.push(ToolSuggestion {
                tool_name: "touring_memory_recall".to_string(),
                reason: "Recall past fixes for similar issues".to_string(),
            });
        } else if hint_lower.contains("refactor") || hint_lower.contains("rename") {
            suggestions.push(ToolSuggestion {
                tool_name: "touring_ast_find".to_string(),
                reason: "Refactoring task — locate symbol definitions".to_string(),
            });
            suggestions.push(ToolSuggestion {
                tool_name: "touring_graph".to_string(),
                reason: "Check blast radius before refactoring".to_string(),
            });
        } else if hint_lower.contains("review") || hint_lower.contains("audit") {
            suggestions.push(ToolSuggestion {
                tool_name: "touring_wiring_audit".to_string(),
                reason: "Review task — audit integration completeness".to_string(),
            });
        } else {
            // General task
            suggestions.push(ToolSuggestion {
                tool_name: "touring_ast_overview".to_string(),
                reason: "Explore file structure".to_string(),
            });
            suggestions.push(ToolSuggestion {
                tool_name: "touring_memory_recall".to_string(),
                reason: "Check for relevant past patterns".to_string(),
            });
        }
    } else {
        // No task hint — suggest exploration
        suggestions.push(ToolSuggestion {
            tool_name: "touring_ast_overview".to_string(),
            reason: "Start by exploring file structure".to_string(),
        });
    }

    suggestions
}

// ── Context Compiler bridge ─────────────────────────────────────────────────

use crate::context_compiler::{CompiledContext, ContextCache, PersistentContextCache};
use crate::observation_masker::ObservationMasker;

/// Input for `compile_context` — wraps context_compiler::compile_or_cached.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CompileContextInput<'a> {
    /// Task intent driving what context to compile.
    pub intent: &'a str,
    /// Source file paths to include in the compiled context.
    pub files: &'a [String],
    /// Relevant gotchas as `(key, message)` pairs.
    pub gotchas: &'a [(String, String)],
    /// Symbol/module relations to surface in the context.
    pub relations: &'a [String],
    /// Recent error messages to inform the context.
    pub recent_errors: &'a [String],
    /// Token budget cap for the compiled output.
    pub max_tokens: usize,
}

/// Input for `summarize_context_masked` — wraps compile_or_cached_masked.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummarizeMaskedInput<'a> {
    /// Task intent driving what context to compile.
    pub intent: &'a str,
    /// Source file paths to include in the compiled context.
    pub files: &'a [String],
    /// Relevant gotchas as `(key, message)` pairs.
    pub gotchas: &'a [(String, String)],
    /// Symbol/module relations to surface in the context.
    pub relations: &'a [String],
    /// Recent error messages to inform the context.
    pub recent_errors: &'a [String],
    /// Token budget cap for the compiled output.
    pub max_tokens: usize,
    /// Minimum token count above which observation masking is applied.
    pub masking_threshold: usize,
}

/// Cache statistics returned by `persistent_cache_stats`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    /// Number of entries currently held in the in-memory cache.
    pub memory_entries: usize,
    /// Maximum number of entries the in-memory cache can hold.
    pub memory_capacity: u64,
    /// Whether a SQLite-backed persistent tier is configured.
    pub has_sqlite: bool,
    /// Filesystem path to the SQLite cache file, when present.
    pub sqlite_path: Option<String>,
}

/// Bridge to `context_compiler::compile_or_cached`.
///
/// Creates a `PersistentContextCache` in memory-only mode and delegates to it.
pub fn compile_context_mcp(input: CompileContextInput<'_>) -> CompiledContext {
    let cache = PersistentContextCache::new(None);
    cache.compile_or_cached(
        input.intent,
        input.files,
        input.gotchas,
        input.relations,
        input.recent_errors,
        input.max_tokens,
    )
}

/// Bridge to `context_compiler::compile_or_cached_masked`.
///
/// Uses `ContextCache` directly (which has `compile_or_cached_masked`), then
/// applies observation masking to the result.
pub fn summarize_context_masked_mcp(input: SummarizeMaskedInput<'_>) -> CompiledContext {
    let cache = ContextCache::new();
    let masker = ObservationMasker::with_threshold(input.masking_threshold);
    cache.compile_or_cached_masked(
        input.intent,
        input.files,
        input.gotchas,
        input.relations,
        input.recent_errors,
        input.max_tokens,
        &masker,
    )
}

/// Return cache statistics from a transient in-memory `PersistentContextCache`.
pub fn persistent_cache_stats_mcp() -> CacheStats {
    let cache = PersistentContextCache::new(None);
    CacheStats {
        memory_entries: cache.memory_len(),
        memory_capacity: 256, // ContextCache::new() default capacity
        has_sqlite: cache.has_db(),
        sqlite_path: cache.db_path().map(|p| p.to_string_lossy().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_risk_score_healthy() {
        let score = compute_risk_score(0, 1000, 0.95);
        assert!(
            score < 0.1,
            "Healthy project should have low risk: {}",
            score
        );
    }

    #[test]
    fn test_compute_risk_score_degraded() {
        let score = compute_risk_score(500, 1000, 0.5);
        assert!(
            score > 0.3,
            "Degraded project should have medium+ risk: {}",
            score
        );
    }

    #[test]
    fn test_compute_risk_score_empty() {
        let score = compute_risk_score(0, 0, 0.0);
        assert_eq!(score, 0.5, "Empty project should return 0.5 (no data)");
    }

    #[test]
    fn test_classify_risk_levels() {
        assert_eq!(classify_risk(0.1), "low");
        assert_eq!(classify_risk(0.3), "medium");
        assert_eq!(classify_risk(0.6), "high");
        assert_eq!(classify_risk(0.9), "critical");
    }

    #[test]
    fn test_generate_suggestions_debug() {
        let suggestions = generate_suggestions(Some("fix the auth bug"), 0.3, 100);
        let tool_names: Vec<&str> = suggestions.iter().map(|s| s.tool_name.as_str()).collect();
        assert!(
            tool_names.contains(&"touring_gotcha"),
            "Debug task should suggest gotcha"
        );
        assert!(
            tool_names.contains(&"touring_memory_recall"),
            "Debug task should suggest memory recall"
        );
    }

    #[test]
    fn test_generate_suggestions_refactor() {
        let suggestions = generate_suggestions(Some("refactor the parser module"), 0.3, 100);
        let tool_names: Vec<&str> = suggestions.iter().map(|s| s.tool_name.as_str()).collect();
        assert!(
            tool_names.contains(&"touring_ast_find"),
            "Refactor task should suggest ast_find"
        );
        assert!(
            tool_names.contains(&"touring_graph"),
            "Refactor task should suggest graph"
        );
    }

    #[test]
    fn test_generate_suggestions_high_risk() {
        let suggestions = generate_suggestions(None, 0.7, 5000);
        let tool_names: Vec<&str> = suggestions.iter().map(|s| s.tool_name.as_str()).collect();
        assert!(
            tool_names.contains(&"touring_wiring_audit"),
            "High risk should suggest wiring audit"
        );
        assert!(
            tool_names.contains(&"touring_wiring"),
            "Many orphans should suggest wiring"
        );
    }

    #[test]
    fn test_build_minimal_context_structure() {
        let result = build_minimal_context(MinimalContextInput {
            symbol_count: 1000,
            file_count: 50,
            orphan_count: 200,
            e2e_health: 0.75,
            rl_reward: 0.5,
            top_gotchas: vec!["gotcha1".to_string(), "gotcha2".to_string()],
            task_hint: Some("debug auth"),
            detail_level: DetailLevel::Standard,
        });
        assert!(result.get("stats").is_some());
        assert!(result.get("risk").is_some());
        assert!(result.get("top_issues").is_some());
        assert!(result.get("suggested_tools").is_some());
        assert!(result.get("task_context").is_some());
    }

    #[test]
    fn test_build_minimal_context_minimal_level() {
        let result = build_minimal_context(MinimalContextInput {
            symbol_count: 1000,
            file_count: 50,
            orphan_count: 200,
            e2e_health: 0.75,
            rl_reward: 0.5,
            top_gotchas: vec![
                "g1".to_string(),
                "g2".to_string(),
                "g3".to_string(),
                "g4".to_string(),
                "g5".to_string(),
            ],
            task_hint: Some("debug"),
            detail_level: DetailLevel::Minimal,
        });
        // Minimal should limit issues to 3
        let issues = result.get("top_issues").unwrap().as_array().unwrap();
        assert!(
            issues.len() <= 3,
            "Minimal should limit to 3 issues, got {}",
            issues.len()
        );
        // Minimal should NOT include task_context (no summaries)
        assert!(
            result.get("task_context").is_none(),
            "Minimal should not include task_context"
        );
    }

    #[test]
    fn test_estimate_tokens() {
        let small = json!({"key": "value"});
        let tokens = estimate_tokens(&small);
        assert!(
            tokens > 0 && tokens < 10,
            "Small JSON ~3-5 tokens: {}",
            tokens
        );

        let large = json!({"data": "x".repeat(400)});
        let tokens = estimate_tokens(&large);
        assert!(tokens > 90, "400-char string ~100 tokens: {}", tokens);
    }

    #[test]
    fn test_estimated_tokens_in_minimal_context() {
        let result = build_minimal_context(MinimalContextInput {
            symbol_count: 100,
            file_count: 10,
            orphan_count: 5,
            e2e_health: 0.9,
            rl_reward: 0.5,
            top_gotchas: vec![],
            task_hint: None,
            detail_level: DetailLevel::Minimal,
        });
        assert!(
            result.get("_estimated_tokens").is_some(),
            "Should include token estimate"
        );
        let tokens = result["_estimated_tokens"].as_u64().unwrap_or(0);
        assert!(tokens > 0, "Tokens should be > 0");
    }

    #[test]
    fn test_compact_response_minimal() {
        let resp = compact_response(
            "Test summary",
            Some(&["sym1".to_string(), "sym2".to_string()]),
            Some("low"),
            Some(&["touring_ast_find", "touring_wiring"]),
            Some(json!({"detail": "full data here"})),
            DetailLevel::Minimal,
        );
        assert_eq!(resp["summary"], "Test summary");
        assert!(resp.get("key_entities").is_some());
        assert_eq!(resp["risk"], "low");
        assert!(resp.get("next_tool_suggestions").is_some());
        // Minimal should NOT include data
        assert!(resp.get("data").is_none(), "Minimal excludes data");
        assert!(resp.get("_estimated_tokens").is_some());
    }

    #[test]
    fn test_compact_response_full_includes_data() {
        let resp = compact_response(
            "Full output",
            None,
            None,
            None,
            Some(json!({"detail": "everything"})),
            DetailLevel::Full,
        );
        assert!(resp.get("data").is_some(), "Full should include data");
    }
}
