//! Tool parameter structs — JsonSchema for auto inputSchema generation.
//!
//! All 32 MCP tool parameter types live here, extracted from the monolithic server.rs.
//! Each struct derives `Deserialize` + `JsonSchema` for rmcp auto-schema generation.

#![allow(dead_code)] // MCP params defined for schema generation — not all struct fields are used by every handler

use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashMap;

// ── Detail Level (Token-Efficiency) ─────────────────────────────────────
//
// Inspired by code-review-graph's `detail_level` parameter.
// Controls output verbosity across all MCP tools to reduce token consumption.

/// Output verbosity for token-efficient MCP responses.
///
/// Controls how much detail is included in tool output:
/// - `minimal`: ~20-50 tokens — IDs, counts, scores only
/// - `standard`: ~100-200 tokens — top-N items, summaries (default)
/// - `full`: complete output (backward compatible with pre-v31 behavior)
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DetailLevel {
    /// Compact output: IDs, counts, and scores only (~20-50 tokens)
    Minimal,
    /// Balanced output: top-N items and summaries (~100-200 tokens)
    #[default]
    Standard,
    /// Complete output: all data, backward compatible with pre-v31 behavior
    Full,
}

impl DetailLevel {
    /// Maximum number of items to include in list responses.
    pub fn max_items(&self) -> usize {
        match self {
            Self::Minimal => 3,
            Self::Standard => 10,
            Self::Full => usize::MAX,
        }
    }

    /// Whether to include full symbol bodies and content.
    pub fn include_bodies(&self) -> bool {
        matches!(self, Self::Full)
    }

    /// Whether to include textual summaries.
    pub fn include_summaries(&self) -> bool {
        !matches!(self, Self::Minimal)
    }

    /// Maximum string length for truncated values.
    pub fn max_value_len(&self) -> usize {
        match self {
            Self::Minimal => 50,
            Self::Standard => 200,
            Self::Full => usize::MAX,
        }
    }
}

/// Truncate a JSON value's string fields and lists according to the detail level.
pub fn apply_detail_level(value: &mut serde_json::Value, level: DetailLevel) {
    match level {
        DetailLevel::Full => {} // no-op
        _ => truncate_json_value(value, level),
    }
}

fn truncate_json_value(value: &mut serde_json::Value, level: DetailLevel) {
    match value {
        serde_json::Value::Array(arr) => {
            let max = level.max_items();
            if arr.len() > max {
                let total = arr.len();
                arr.truncate(max);
                arr.push(serde_json::json!({
                    "_truncated": true,
                    "_total": total,
                    "_showing": max
                }));
            }
            for item in arr.iter_mut() {
                truncate_json_value(item, level);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                truncate_json_value(v, level);
            }
        }
        serde_json::Value::String(s) => {
            let max_len = level.max_value_len();
            if s.len() > max_len {
                let truncated = s.chars().take(max_len).collect::<String>();
                *s = format!("{}…", truncated);
            }
        }
        _ => {}
    }
}

/// Parameters for the `touring_audit` master workflow tool.
///
/// Orchestrates multi-engine failure/gap detection on a single source file
/// (coupling backlog — master tools). See `crate::server::tools_workflow`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AuditParams {
    /// Path to the source file to audit for vulnerabilities, quality blockers
    /// and gaps (e.g. "crates/foo/src/lib.rs").
    pub path: String,
    /// Detection layers to run — any of "vuln" (offensive CWE/OWASP patterns)
    /// and "quality" (6 P0 BLOCK dims). Omit or use ["all"] to run every layer.
    #[serde(default)]
    pub layers: Option<Vec<String>>,
    /// Output verbosity (minimal | standard | full). Default: standard.
    #[serde(default)]
    pub detail: DetailLevel,
}

/// D.3.5 — MCP tool: apply a fix assist for a diagnostic code.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FixApplyParams {
    /// RFC-100 diagnostic code (e.g. "Q-201", "W-100").
    pub code: String,
    /// File path to apply the fix in.
    pub file_path: String,
    /// Optional line:column range within the file.
    pub range: Option<String>,
}

/// D.3 — MCP tool: list all available assist kinds.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssistListKindsParams {
    /// Optional group filter (e.g. "Refactoring", "Code Generation").
    #[serde(default)]
    pub group: Option<String>,
}

/// D.3 — MCP tool: show applicable assists at a cursor position.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssistApplicableParams {
    /// File path with cursor position in file:line:col format.
    pub cursor: String,
}

/// D.3 — MCP tool: apply a specific assist at a position.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssistApplyParams {
    /// Assist kind id (e.g. "auto_wire", "extract_function").
    pub kind: String,
    /// File path with cursor position in file:line:col format.
    pub cursor: String,
    /// Byte range to apply the assist (format: start..end).
    pub range: String,
}

/// C3 (coupling backlog) — MCP tool: discover Touring tools by intent.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchToolsParams {
    /// Natural-language description of what you want to do (e.g. "find who
    /// calls a function").
    pub intent: String,
    /// Max number of ranked tools to return (default 8, clamped to 1..=50).
    #[serde(default)]
    pub top_k: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_verbose_output() -> serde_json::Value {
        serde_json::json!({
            "total_matches": 150,
            "rlm_matches": (0..50).map(|i| serde_json::json!({
                "key": format!("lesson:pattern:error_handling_{}", i),
                "tier": "semantic",
                "value": format!("This is a long memory entry value that contains detailed information about pattern {} and its implications for the codebase architecture and design decisions that were made during development.", i),
                "entry_type": "lesson",
                "score": 0.95 - (i as f64 * 0.01),
                "access_count": 10 - i,
            })).collect::<Vec<_>>(),
            "semantic_matches": (0..20).map(|i| serde_json::json!({
                "id": i,
                "content": format!("Semantic content block {} with substantial text that describes something important about the codebase.", i),
                "metadata": {"source": "auto", "timestamp": "2026-04-08"},
                "score": 0.88 - (i as f64 * 0.02),
            })).collect::<Vec<_>>(),
            "graph_neighbors": {
                "file": "/home/user/project/src/main.rs",
                "imports": (0..15).map(|i| format!("dep_{}.rs", i)).collect::<Vec<_>>(),
                "imported_by": (0..8).map(|i| format!("consumer_{}.rs", i)).collect::<Vec<_>>(),
            },
        })
    }

    #[test]
    fn test_detail_level_defaults() {
        assert_eq!(DetailLevel::default(), DetailLevel::Standard);
        assert_eq!(DetailLevel::Minimal.max_items(), 3);
        assert_eq!(DetailLevel::Standard.max_items(), 10);
        assert_eq!(DetailLevel::Full.max_items(), usize::MAX);
    }

    #[test]
    fn test_detail_level_token_savings_minimal() {
        let mut full = sample_verbose_output();
        let full_size = serde_json::to_string(&full)
            .expect("sample_verbose_output is always serializable")
            .len();

        apply_detail_level(&mut full, DetailLevel::Minimal);
        let minimal_size = serde_json::to_string(&full)
            .expect("minimal output is always serializable")
            .len();

        let ratio = full_size as f64 / minimal_size as f64;
        println!(
            "Full: {} bytes, Minimal: {} bytes, Ratio: {:.1}x",
            full_size, minimal_size, ratio
        );
        assert!(
            ratio > 3.0,
            "Minimal should be at least 3x smaller than full: {:.1}x",
            ratio
        );
    }

    #[test]
    fn test_detail_level_token_savings_standard() {
        let mut full_copy = sample_verbose_output();
        let full_size = serde_json::to_string(&full_copy)
            .expect("sample_verbose_output is always serializable")
            .len();

        apply_detail_level(&mut full_copy, DetailLevel::Standard);
        let standard_size = serde_json::to_string(&full_copy)
            .expect("standard output is always serializable")
            .len();

        let ratio = full_size as f64 / standard_size as f64;
        println!(
            "Full: {} bytes, Standard: {} bytes, Ratio: {:.1}x",
            full_size, standard_size, ratio
        );
        assert!(
            ratio > 1.5,
            "Standard should be at least 1.5x smaller than full: {:.1}x",
            ratio
        );
    }

    #[test]
    fn test_detail_level_full_is_noop() {
        let original = sample_verbose_output();
        let mut copy = original.clone();
        apply_detail_level(&mut copy, DetailLevel::Full);
        assert_eq!(original, copy, "Full should not modify output");
    }

    #[test]
    fn test_truncate_string_values() {
        let mut val = serde_json::json!({
            "short": "ok",
            "long": "a".repeat(300),
        });
        apply_detail_level(&mut val, DetailLevel::Minimal);
        let long_val = val["long"].as_str().expect("long field should be a string");
        assert!(
            long_val.len() <= 55,
            "Should truncate to ~50 chars: {}",
            long_val.len()
        );
        assert!(long_val.ends_with('…'), "Should end with ellipsis");
    }

    #[test]
    fn test_truncate_arrays() {
        let mut val = serde_json::json!({
            "items": (0..100).collect::<Vec<_>>(),
        });
        apply_detail_level(&mut val, DetailLevel::Minimal);
        let arr = val["items"]
            .as_array()
            .expect("items field should be an array");
        assert!(
            arr.len() <= 4,
            "Minimal should truncate to 3 items + truncation marker: {}",
            arr.len()
        );
        // Last item should be truncation marker
        let last = arr
            .last()
            .expect("truncated array should have at least one item");
        assert!(
            last.get("_truncated").is_some(),
            "Should have truncation marker"
        );
    }
}

// ── AST Tools ────────────────────────────────────────────────────────────

/// Parameters for touring_ast_overview tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct AstOverviewParams {
    /// Source code content to analyze
    pub content: Option<String>,
    /// File path for language detection (alternative to content)
    pub file_path: Option<String>,
    /// Language hint (python, rust, typescript, javascript)
    pub language: Option<String>,
    /// Output format: toon (default), compact, brief, json
    pub format: Option<String>,
    /// Show token savings comparison
    pub show_savings: Option<bool>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_ast_find tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AstFindParams {
    /// Symbol name to search for
    pub symbol_name: String,
    /// File paths to search in (structured format)
    pub files: Option<Vec<AstFindFileParam>>,
    /// Single file path (flat alias)
    pub file_path: Option<String>,
    /// Alias for file_path
    pub path: Option<String>,
    /// File content (flat alias)
    pub content: Option<String>,
    /// Alias for content
    pub source: Option<String>,
    /// Language for flat mode
    pub language: Option<String>,
    /// Only return definitions (not references)
    #[serde(default = "default_true")]
    pub definitions_only: bool,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AstFindFileParam {
    /// File path (relative to project root)
    pub path: String,
    /// File content
    pub content: String,
    /// Language: python, rust, typescript, javascript
    pub language: String,
}

/// Parameters for touring_ast_edit tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AstEditParams {
    /// Action: replace_body or validate_syntax
    pub action: Option<String>,
    /// Source code content
    pub content: String,
    /// Symbol to edit (for replace_body)
    pub symbol_name: Option<String>,
    /// New body content (for replace_body)
    pub new_body: Option<String>,
    /// Language (for validate_syntax)
    pub language: Option<String>,
    /// File path for graph context (blast radius warning on hub files)
    pub file_path: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_ast_grep tool — polyglot structural search + rewrite.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AstGrepParams {
    /// Absolute path of the source file to operate on.
    pub file_path: String,
    /// ast-grep pattern. Use `$VAR` for single-node capture, `$$$VAR` for variadic.
    pub pattern: String,
    /// Optional replacement. When present the tool runs in rewrite mode.
    pub rewrite: Option<String>,
    /// Optional language override (e.g. `python`, `typescript`).
    /// Auto-detected from the file extension when omitted.
    pub lang: Option<String>,
    /// Max number of matches returned in search mode. Default 50.
    pub top: Option<u64>,
}

// ── Classification & PII ─────────────────────────────────────────────────

/// Parameters for touring_classify_intent tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClassifyIntentParams {
    /// User prompt text to classify
    pub text: String,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_scan_pii tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScanPiiParams {
    /// Text content to scan for PII
    pub text: String,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Memory Tools ─────────────────────────────────────────────────────────

/// Parameters for touring_memory_store tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryStoreParams {
    /// Unique key for the memory entry
    pub key: Option<String>,
    /// Alias for key
    pub title: Option<String>,
    /// Memory tier: ephemeral, working, reference, core (aliases: reflexive→ephemeral, session→working, project→reference)
    pub tier: Option<String>,
    /// Alias for tier
    pub memory_type: Option<String>,
    /// Memory content to store
    pub value: Option<String>,
    /// Alias for value
    pub content: Option<String>,
    /// Optional entry type tag
    pub entry_type: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_memory_recall tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryRecallParams {
    /// Search query
    pub query: String,
    /// Filter by tier (optional)
    pub tier: Option<String>,
    /// Max results (default: 10)
    pub limit: Option<u64>,
    /// Alias for limit
    pub top_k: Option<u64>,
    /// File path for graph-neighborhood expansion: returns files that import/are imported by this file
    pub file_path: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Learning Tools ───────────────────────────────────────────────────────

/// Parameters for touring_learn_pattern tool
///
/// Note: For backwards compatibility, use `action_id` field for integer action IDs
/// and `action` field for string operation names.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LearnPatternParams {
    /// Action: update, get_q, best_action, reset_traces
    pub action: Option<String>,
    /// Alias for action (backwards compat)
    pub operation: Option<String>,
    /// State ID
    pub state: Option<u64>,
    /// Action ID (for update/get_q)
    pub action_id: Option<u64>,
    /// Reward signal (for update)
    pub reward: Option<f64>,
    /// Next state (for update)
    pub next_state: Option<u64>,
    /// Terminal state flag (for update)
    pub terminal: Option<bool>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_cluster_skills tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClusterSkillsParams {
    /// Action: record, cluster, find_similar, get_clusters
    pub action: Option<String>,
    /// Alias for action (backwards compat)
    pub operation: Option<String>,
    /// Skill identifier (for record/find_similar)
    pub skill_id: Option<String>,
    /// Usage context (for record)
    pub context: Option<String>,
    /// Whether usage was successful (for record)
    pub success: Option<bool>,
    /// Number of similar skills to return (for find_similar)
    pub top_k: Option<u64>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── D.2 Semantic primitives ────────────────────────────────────────────────

/// Parameters for touring_resolve_def tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveDefParams {
    /// File path to resolve (absolute or relative)
    pub file_path: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
    /// Optional source content (if not provided, file is read from disk)
    pub source: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_find_references tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FindReferencesParams {
    /// File path to resolve (absolute or relative)
    pub file_path: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
    /// Search scope: "workspace" (default) or "project"
    #[serde(default = "default_workspace_scope")]
    pub scope: String,
    /// Optional source content (if not provided, file is read from disk)
    pub source: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

fn default_workspace_scope() -> String {
    "workspace".to_string()
}

/// Parameters for touring_rename tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameParams {
    /// File path to resolve (absolute or relative)
    pub file_path: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
    /// New name for the symbol
    pub new_name: String,
    /// Apply the rename (default false — dry run)
    #[serde(default)]
    pub apply: bool,
    /// Optional source content (if not provided, file is read from disk)
    pub source: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Evolution & Insights ─────────────────────────────────────────────────

/// Parameters for touring_insights tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InsightsParams {
    /// Filter by axis: self_improvement, project_evolution (optional)
    pub axis: Option<String>,
    /// Filter by category: tool_effectiveness, cila_progression, cost_efficiency, drift_detection
    pub category: Option<String>,
    /// Minimum severity: info, warning, critical (optional)
    pub min_severity: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_evolution_status tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvolutionStatusParams {
    /// Include detailed breakdowns per wilson item and drift metric
    pub detailed: Option<bool>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_evolve tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvolveParams {
    /// Action: extract_patterns, update_qtable, auto_learn, consolidate_memory, drift_report, recommend
    pub action: String,
    /// Session ID (for "extract_patterns", "update_qtable")
    pub session_id: Option<String>,
    /// Memory key (for "consolidate_memory")
    pub key: Option<String>,
    /// Current tier (for "consolidate_memory")
    pub current_tier: Option<String>,
    /// New tier (for "consolidate_memory")
    pub new_tier: Option<String>,
    /// Metric name (for "drift_report", if omitted: all metrics)
    pub metric: Option<String>,
    /// State ID (for "recommend")
    pub state: Option<u64>,
    /// Top-k items (for "recommend", default: 5)
    pub top_k: Option<u64>,
    /// Reward (for "update_qtable")
    pub reward: Option<f64>,
    /// Action ID (for "update_qtable")
    pub action_id: Option<u64>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_evolution_drift tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DriftParams {
    /// Optional metric name to filter results. If None, returns all drift metrics.
    pub metric_name: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── File & Project Tools ─────────────────────────────────────────────────

/// Parameters for touring_index_status tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IndexStatusParams {
    /// Project path (default: config.project_root)
    pub project_path: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_checkpoint tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckpointParams {
    /// Checkpoint description
    pub description: Option<String>,
    /// Optional tags
    pub tags: Option<Vec<String>>,
    /// EC65: backwards-compat serde field — legacy clients send `operation`; we must
    /// deserialize it to avoid parse errors, but the field is intentionally ignored.
    /// cargo check cannot see serde's field read, so dead_code annotation is necessary.
    #[allow(dead_code)]
    pub operation: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_file_ops tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileOpsParams {
    /// Operation: read, write, append, delete, delete_dir, find, search, stat, exists,
    ///            mkdir, copy, move, rename, glob, tree, list
    pub operation: Option<String>,
    /// Alias for operation (backwards compat)
    pub action: Option<String>,
    /// Source file/directory path (required for all operations)
    pub path: String,
    /// Content (for write/append)
    pub content: Option<String>,
    /// Destination path (for copy, move, rename)
    pub dest: Option<String>,
    /// Pattern for find/search/glob operations (glob: *.rs, **/*.py; or regex with use_regex=true)
    pub pattern: Option<String>,
    /// Max directory depth for find/search/tree (default: find=10, tree=5)
    pub max_depth: Option<usize>,
    /// Search within file contents matching this pattern (for find/search operation)
    pub content_pattern: Option<String>,
    /// Include hidden files/directories (default: false)
    pub include_hidden: Option<bool>,
    /// Interpret pattern as regex instead of glob (default: false — glob is default)
    pub use_regex: Option<bool>,
    /// Force recursive deletion for delete_dir (default: false)
    pub force: Option<bool>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_project tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectParams {
    /// Project path (default: config.project_root)
    pub project_path: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_resolve_project tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectResolverParams {
    /// File path to resolve to a project
    pub file_path: String,
}

// ── Graph & Decomposition ────────────────────────────────────────────────

/// Parameters for touring_graph tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphParams {
    /// Action: index, blast_radius, dependency_path, imports, query, reload, neighbors
    pub action: String,
    /// File paths with content for indexing (for "index" action)
    pub files: Option<Vec<GraphFileParam>>,
    /// Symbol name (for "blast_radius" action) or file path (for "blast_radius")
    pub symbol: Option<String>,
    /// Source file (for "dependency_path")
    pub from: Option<String>,
    /// Target file (for "dependency_path")
    pub to: Option<String>,
    /// File content (for "imports" action)
    pub content: Option<String>,
    /// Language: python, rust, typescript, javascript
    pub language: Option<String>,
    /// Name pattern (for "query" action)
    pub pattern: Option<String>,
    /// Kind filter for query: definition, reference
    pub kind: Option<String>,
    /// File path for graph context (Focus Tracker update)
    pub file_path: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// A file entry for graph indexing
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphFileParam {
    /// File path
    pub path: String,
    /// File content
    pub content: String,
    /// Language
    pub language: String,
}

/// Parameters for touring_decompose tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DecomposeParams {
    /// Action: create, add_subtask, update_status, get_plan, validate_order
    pub action: String,
    /// Task ID (for add_subtask, update_status, get_plan, validate_order)
    pub task_id: Option<String>,
    /// Task type: refactor, debug, feature, analysis, pipeline (for "create")
    pub task_type: Option<String>,
    /// Description (for "create" and "add_subtask")
    pub description: Option<String>,
    /// Subtask ID (for "update_status")
    pub subtask_id: Option<String>,
    /// Dependency subtask IDs (for "add_subtask")
    pub depends_on: Option<Vec<String>>,
    /// Priority 0-255 (for "add_subtask", default: 0)
    pub priority: Option<u8>,
    /// New status: pending, in_progress, completed, blocked (for "update_status")
    pub status: Option<String>,
    /// CILA level 0-6 (for "create", default: 3)
    pub cila_level: Option<u8>,
    /// Minimum quality threshold 0.0-1.0 (for "validate_completion", "finalize")
    pub quality_threshold: Option<f64>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
    /// Wave C3-D3: auto-scaffold subtasks from GranularityBandit hint (L3+ only).
    #[serde(default)]
    pub auto_decompose: Option<bool>,
}

// ── Session Management ───────────────────────────────────────────────────

/// Parameters for touring_session tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionParams {
    /// Action: start, checkpoint, assess, end, list, get
    pub action: String,
    /// Session ID (for checkpoint, assess, end, get)
    pub session_id: Option<String>,
    /// Task type (for "start")
    pub task_type: Option<String>,
    /// Objective (for "start")
    pub objective: Option<String>,
    /// Notes (for "checkpoint")
    pub notes: Option<String>,
    /// Metrics as key-value pairs (for "checkpoint")
    pub metrics: Option<HashMap<String, f64>>,
    /// Max sessions to list (for "list", default: 10)
    pub limit: Option<u64>,
    /// End status: completed, abandoned (for "end")
    pub status: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Suggestion & Refactoring ─────────────────────────────────────────────

/// Parameters for touring_suggest tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestParams {
    /// Action: next_action, similar_patterns, skill_recommendation, code_pattern
    pub action: String,
    /// State ID (for "next_action")
    pub state: Option<u64>,
    /// Query text (for "similar_patterns")
    pub query: Option<String>,
    /// Skill ID (for "skill_recommendation")
    pub skill_id: Option<String>,
    /// File content (for "code_pattern")
    pub content: Option<String>,
    /// Language (for "code_pattern")
    pub language: Option<String>,
    /// Tier filter (for "similar_patterns")
    pub tier: Option<String>,
    /// Top-k results (default: 5)
    pub top_k: Option<u64>,
    /// File path for graph-aware confidence scaling (blast_radius modulates suggestion confidence)
    pub file_path: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_refactor tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RefactorParams {
    /// Action: analyze, rename, validate, preview
    pub action: String,
    /// Source code content
    pub content: String,
    /// Symbol name to target
    pub symbol_name: Option<String>,
    /// New name (for "rename")
    pub new_name: Option<String>,
    /// Language (python, rust, typescript, javascript)
    pub language: Option<String>,
    /// File path hint (for blast_radius context)
    pub file_path: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── v9.0 Advanced Tools ──────────────────────────────────────────────────

/// Parameters for touring_mask_context tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaskContextParams {
    /// Context text to mask (tool result observations will be summarized)
    pub text: String,
    /// Token threshold below which masking is skipped (default: 4000)
    pub threshold: Option<usize>,
}

/// Parameters for touring_mcts_search tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MctsSearchParams {
    /// Root state ID for the search tree
    pub root_state: u64,
    /// Comma-separated candidate action IDs (e.g. "1,2,3,4,5")
    pub candidate_actions: String,
    /// Number of MCTS rollout iterations (default: 50)
    pub num_rollouts: Option<usize>,
    /// Maximum search depth per rollout (default: 5)
    pub max_depth: Option<usize>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_incremental_status tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncrementalStatusParams {
    /// Reserved for future use (no parameters required)
    pub _unused: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_speculate tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SpeculateParams {
    /// File path to apply the speculative edit to
    pub file_path: String,
    /// New file content for the speculative branch
    pub content: String,
    /// Base directory for the shadow workspace (default: ".")
    pub base_dir: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_online_learn tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OnlineLearnParams {
    /// Reserved for future use (no parameters required)
    pub _unused: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_streaming_mcts tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamingMctsParams {
    /// Root state for the search tree (string encoded for flexibility)
    pub search_state: String,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Wiring & Gotcha ─────────────────────────────────────────────────────

/// Parameters for touring_wiring tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WiringParams {
    /// Action: status (summary), orphans (unused pub symbols), modules (per-module scores)
    pub action: String,
    /// Filter by file path (optional, for modules/orphans actions)
    pub file_path: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_wiring_audit tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WiringAuditParams {
    /// Optional module path filter — if provided, only audit that module
    pub module_filter: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for the touring_wiring_suggest tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WiringSuggestParams {
    /// Optional symbol name to filter suggestions — if omitted returns all pending suggestions
    pub orphan_symbol: Option<String>,
    /// Output verbosity level
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_gotcha tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GotchaParams {
    /// Action: list (all gotchas), stats (counts by file)
    pub action: Option<String>,
    /// Filter by file path (optional)
    pub file_path: Option<String>,
    /// Max results (default: 50)
    pub limit: Option<u64>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_memory_clusters tool
///
/// Note: fields `cluster_id`, `query_embedding`, `top_k` are only accessed when
/// async-memory feature is enabled. Without the feature, only `action` is used for validation.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryClustersParams {
    /// Action to perform: "list" | "stats" | "members" | "similar"
    pub action: String,
    /// Optional cluster_id for "members" action
    // EC65: only read under #[cfg(feature = "async-memory")] — annotation required.
    #[allow(dead_code)]
    pub cluster_id: Option<u64>,
    /// Optional query embedding for "similar" action
    // EC65: only read under #[cfg(feature = "async-memory")] — annotation required.
    #[allow(dead_code)]
    pub query_embedding: Option<Vec<f32>>,
    /// Maximum number of results (default: 10)
    // EC65: only read under #[cfg(feature = "async-memory")] — annotation required.
    #[allow(dead_code)]
    pub top_k: Option<usize>,
}

// ── Profile Query ────────────────────────────────────────────────────────

/// Parameters for touring_profile_query MCP tool (Wave A A.4)
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileQueryParams {
    /// Optional label filter — if provided, only entries matching this label
    /// are returned. If None, all tracked labels are included.
    pub section: Option<String>,
    /// Maximum number of entries to return (default: 10).
    #[serde(default = "default_profile_top_n")]
    pub top_n: u32,
    /// Which percentile latencies to include. Each value must be 0-100.
    /// Common values: 50 (p50), 90 (p90), 99 (p99), 999 (p999).
    /// Defaults to [50, 90, 99].
    #[serde(default)]
    pub include_percentiles: Option<Vec<u8>>,
}

fn default_profile_top_n() -> u32 {
    10
}

// ── SSR Apply ─────────────────────────────────────────────────────────────

/// Parameters for touring_ssr_apply MCP tool (Wave B B.1)
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SsrApplyParams {
    /// Pattern in ast-grep syntax: `foo($X) => $X.foo()`.
    pub pattern: String,
    /// Replacement template.
    pub replacement: String,
    /// Source code to apply SSR on.
    pub source: String,
    /// Language hint (rust, javascript, typescript, python, go, java, bash).
    /// Defaults to "rust".
    #[serde(default = "default_ssr_lang")]
    pub lang: String,
}

fn default_ssr_lang() -> String {
    "rust".to_string()
}

// ── Rename Symbol ───────────────────────────────────────────────────────────

/// Parameters for `touring_rename_symbol` MCP tool (D7).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameSymbolParams {
    /// The symbol to rename.
    pub symbol: String,
    /// The new name for the symbol.
    pub new_name: String,
    /// Scope: file, dir, or project. Defaults to project.
    #[serde(default)]
    pub scope: Option<String>,
}

/// Response struct for `touring_rename_symbol` tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenameSymbolResponse {
    /// Original symbol name.
    pub old_symbol: String,
    /// New symbol name.
    pub new_symbol: String,
    /// List of edit sites found.
    pub results: Vec<RenameSymbolResult>,
    /// Number of files affected.
    pub blast_radius: usize,
    /// Risk tier based on blast radius: low, medium, high.
    pub risk_tier: String,
    /// Hash of the plan for confirmation.
    pub plan_hash: String,
}

/// A single edit site for rename.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenameSymbolResult {
    /// Absolute file path.
    pub file_path: String,
    /// Line number (1-indexed).
    pub line: usize,
    /// Column number (1-indexed).
    pub col: usize,
    /// Kind of usage: definition, import, call_site, type_ref.
    pub kind: String,
}

// ── WASM Plugin Tools ───────────────────────────────────────────────────

/// Parameters for touring_wasm_plugin tool
#[cfg(feature = "wasm-plugins")]
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmPluginParams {
    /// Path to the WASM plugin file (.wasm or .wat)
    pub file_path: String,
    /// Optional JSON input to pass to the plugin
    #[serde(default)]
    pub input: Option<String>,
    /// Maximum fuel (instructions) to allow (uses DEFAULT_FUEL if not specified)
    #[serde(default)]
    pub max_fuel: Option<u64>,
}

// ── Analysis Report ─────────────────────────────────────────────────────

/// Parameters for the `touring_analysis_report` MCP tool.
#[derive(Deserialize, JsonSchema, Debug)]
pub(crate) struct AnalysisReportParams {
    /// Analysis depth: "quick", "standard", or "deep". Defaults to "standard".
    #[serde(default)]
    pub depth: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Health Check ─────────────────────────────────────────────────────────

/// Parameters for the `touring_health` MCP tool.
#[derive(Deserialize, JsonSchema, Debug, Default)]
pub(crate) struct HealthCheckParams {
    /// Reserved for future use.
    #[serde(default)]
    pub _unused: Option<String>,
}

// ── Blast Radius Analysis ─────────────────────────────────────────────────

/// Parameters for the `touring_blast_radius_analysis` MCP tool.
#[derive(Deserialize, JsonSchema, Debug)]
pub(crate) struct BlastRadiusAnalysisParams {
    /// File path to analyze blast radius for (relative to project root).
    pub file_path: String,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Quality Check ─────────────────────────────────────────────────────────

/// Parameters for the `touring_quality_check` MCP tool.
#[derive(Deserialize, JsonSchema, Debug)]
pub(crate) struct QualityCheckParams {
    /// Analysis depth: "quick", "standard", or "deep". Defaults to "quick".
    #[serde(default)]
    pub depth: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Metrics ──────────────────────────────────────────────────────────────

/// Parameters for the `touring_metrics` MCP tool.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub(crate) struct MetricsParams {
    /// Optional filter prefix: if set, only metrics whose name starts with this
    /// string are included in the output (e.g. `"analysis"` returns only
    /// `analysis_*` counters). When absent, all counters are exported.
    #[serde(default)]
    pub filter: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Change Detection & Risk Scoring ─────────────────────────────────────

/// Parameters for touring_detect_changes tool.
///
/// Unified risk-scored change impact analysis combining blast radius,
/// wiring scores, gotcha matches, and test coverage gaps.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DetectChangesParams {
    /// Changed file paths to analyze
    pub file_paths: Vec<String>,
    /// Git base ref for diff context (default: HEAD~1)
    pub base: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Minimal Context (Token-Efficiency Entry Point) ──────────────────────

/// Parameters for touring_minimal_context tool.
///
/// Ultra-compact entry point (~100-150 tokens). Call this first before
/// any other touring tool to get a project overview with risk assessment
/// and tool suggestions.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MinimalContextParams {
    /// Optional task description for context-aware suggestions
    /// (e.g. "debug auth bug", "refactor parser", "review PR")
    pub task: Option<String>,
    /// Optional file paths for targeted risk assessment
    pub file_paths: Option<Vec<String>>,
    /// Output verbosity: minimal (default for this tool), standard, full
    #[serde(default = "default_minimal")]
    pub detail_level: Option<DetailLevel>,
}

fn default_minimal() -> Option<DetailLevel> {
    Some(DetailLevel::Minimal)
}

// ── L7-B Delta: Jobs MCP tools ───────────────────────────────────────────

/// Parameters for `touring_spawn_worker` tool.
///
/// Spawns a background program (no shell invocation — uses execve semantics).
/// Returns a `job_id` that can be passed to `touring_poll_worker` later.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobsSpawnParams {
    /// Logical tool name used as the `job_id` prefix (e.g., "cargo-test")
    pub tool_name: String,
    /// Executable program name or path (e.g., "cargo", "/usr/bin/touring")
    pub program: String,
    /// Individual argument strings — each passed as a distinct argv entry
    #[serde(default)]
    pub args: Vec<String>,
}

/// Parameters for `touring_poll_worker` tool.
///
/// Polls a previously-spawned job and returns its current status:
/// `running` (non-terminal), `completed` (with result), `failed` (with error),
/// or `not_found`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobsPollParams {
    /// Job id returned by `touring_spawn_worker`
    pub job_id: String,
}

/// Parameters for `touring_list_jobs` tool. Empty — no required inputs.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobsListParams {}

/// Parameters for `touring_drop_job` tool.
///
/// Removes a job from the registry. If the job is still running, its
/// `JoinHandle` is aborted. Returns `{dropped: bool}` indicating whether
/// the job_id was found and removed.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobsDropParams {
    /// Job id returned by `touring_spawn_worker`
    pub job_id: String,
}

// ── Wave 16 (2026-04-18) — health_delta MCP tool params ────────────────

/// Parameters for `touring_health_delta_status` tool.
///
/// When `file_path` is omitted/empty, returns aggregate counters from
/// `gate_metrics` (record/compute/regression/improvement/streak_alert/
/// recovery/outstanding). When provided, returns per-path streak state
/// + warning/improvement hints.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthDeltaStatusParams {
    /// Optional absolute file path. Omit for aggregate snapshot.
    #[serde(default)]
    pub file_path: String,
}

/// Parameters for `touring_health_delta_reset` tool.
///
/// Clears both the streak counters and any pending pre-record entry
/// for the given path. Useful after a deliberate refactor checkpoint
/// where the operator wants to start tracking from a known-good baseline.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthDeltaResetParams {
    /// Absolute file path to reset. Required.
    pub file_path: String,
}

// ── Generator Tools (20 MCP tools) ──────────────────────────────────────

/// Parameters for `touring_generator_submit_plan` / `touring_generator_commit_plan`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorSubmitParams {
    /// GeneratorPlan serialized as a JSON string.
    pub plan_json: String,
    /// If true, stop after render (skip speculate+commit). Default: false.
    #[serde(default)]
    pub dry_run: bool,
}

/// Parameters for tools that take only a plan JSON string.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorPlanParams {
    /// GeneratorPlan serialized as a JSON string.
    pub plan_json: String,
}

/// Parameters for `touring_generator_schema_dump`.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorSchemaDumpParams {
    /// Schema version string (informational, default "v1.0").
    pub version: Option<String>,
}

/// Parameters for `touring_generator_recall_similar`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorRecallParams {
    /// Free-text search query forwarded to `touring memory recall`.
    pub query: String,
    /// Maximum number of results to return. Default: 10.
    #[serde(default = "default_recall_limit")]
    pub limit: i64,
}

fn default_recall_limit() -> i64 {
    10
}

/// Parameters for `touring_generator_diff_plans`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorDiffParams {
    /// First plan, serialized as a JSON string.
    pub plan_a_json: String,
    /// Second plan, serialized as a JSON string.
    pub plan_b_json: String,
}

/// Parameters for `touring_generator_suggest_plan`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorSuggestParams {
    /// Natural-language intent for the skeleton plan.
    pub intent: String,
    /// Optional GeneratorKind name (e.g. "RustModule", "McpTool").
    pub kind: Option<String>,
}

/// Parameters for `touring_generator_template_validate`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorTemplateValidateParams {
    /// Absolute path to the Tera template file to validate.
    pub template_file: String,
}

/// Parameters for `touring_generator_template_test`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorTemplateTestParams {
    /// Built-in template name (e.g. "rust_module.tera"). Use template_list to enumerate.
    pub template_name: String,
    /// Optional JSON object of template variables (e.g. `{"name":"MyModule"}`).
    pub vars_json: Option<String>,
}

/// Parameters for tools that take no inputs (template_list, kinds_list, capacity).
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorEmptyParams {}

/// Parameters for `touring_generator_consumer_wiring`.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorConsumerWiringParams {
    /// Maximum number of orphan symbols to process. Default: 10.
    #[serde(default = "default_consumer_wiring_limit")]
    pub limit: usize,
}

fn default_consumer_wiring_limit() -> usize {
    10
}

// ── Pln2 Metadata Tools (9 MCP tools) ────────────────────────────────────

/// Parameters for `touring_ast_callgraph`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AstCallgraphParams {
    /// Absolute path to the file to query.
    pub file_path: String,
}

/// Parameters for `touring_ast_todos`.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AstTodosParams {
    /// Absolute path to the file. If empty, returns todos for all files.
    #[serde(default)]
    pub file_path: String,
}

/// Parameters for `touring_ast_features`.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AstFeaturesParams {
    /// Absolute path to the file. If empty, returns features for all files.
    #[serde(default)]
    pub file_path: String,
}

/// Parameters for `touring_ast_meta`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AstMetaParams {
    /// Absolute path to the file.
    pub file_path: String,
    /// Depth: "skeleton" (symbols+LOC), "summary" (+quality+blast+fan), "full" (+imports+todos).
    pub depth: Option<String>,
}

/// Parameters for `touring_search_symbols`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchSymbolsParams {
    /// Symbol name pattern to search for (LIKE matching).
    pub query: String,
    /// Maximum results to return. Default: 10.
    pub top: Option<i64>,
}

/// Parameters for `touring_search_docs`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchDocsParams {
    /// Text to search in file documentation and knowledge context.
    pub query: String,
    /// Maximum results to return. Default: 10.
    pub top: Option<i64>,
}

/// Parameters for `touring_query_dsl`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QueryDslParams {
    /// DSL query string. Example: "lang = rust AND loc > 100".
    /// Supported fields: lang, language, loc, line_count, symbol_count, read_count.
    /// Supported operators: =, !=, >, <, >=, <=, LIKE.
    pub query: String,
}

/// Parameters for `touring_session_summary`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionSummaryParams {
    /// Absolute path to the file to query session summaries for.
    pub file_path: String,
}

/// Parameters for `touring_wiring_purpose`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WiringPurposeParams {
    /// Absolute path to the file to describe.
    pub file_path: String,
}

/// Parameters for the `touring_generator_bundle` MCP tool.
/// Accepts multiple GeneratorPlan JSON strings to execute as a sequential
/// bundle transaction. Returns a manifest with per-plan results.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorBundleParams {
    /// Vec of GeneratorPlan JSON strings to execute sequentially.
    pub plans_json: Vec<String>,
    /// If true, stop each plan after render (no commit).
    #[serde(default)]
    pub dry_run: bool,
}

/// Parameters for the `touring_generator_schema_check` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaVersionParams {
    /// Schema version string to check compatibility for.
    pub version: String,
}

/// Parameters for `touring_generator_registry_status`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorRegistryParams {
    /// Optional filter by plan_id prefix.
    pub filter: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ─── Tantivy MCP Params ─────────────────────────────────────────────────────

/// Parameters for `touring_tantivy_search` — BM25 full-text search over symbols.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TantivySearchParams {
    /// Search query string for BM25 full-text search.
    pub query: String,
    /// Maximum results to return (default: 10).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Parameters for `touring_tantivy_fuzzy` — fuzzy search with edit-distance tolerance.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TantivyFuzzyParams {
    /// Fuzzy search query string.
    pub query: String,
    /// Maximum edit distance for fuzzy matching (default: 2).
    #[serde(default)]
    pub distance: Option<u8>,
    /// Maximum results to return (default: 10).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Parameters for `touring_tantivy_stats` — no inputs required.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TantivyStatsParams {}

/// Parameters for `touring_tantivy_suggest` — autocomplete prefix suggestions.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TantivySuggestParams {
    /// Prefix string for autocomplete suggestions.
    pub prefix: String,
    /// Maximum results to return (default: 10).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Parameters for `touring_tantivy_reindex` — full reindex from symbol store.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TantivyReindexParams {}

// ── Context Router (NEW-2/NEW-3 Wave 2026-05-08 post-wave) ──────────────

/// Parameters for `touring_ctx_gain` — token-savings dashboard. No inputs.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxGainParams {}

/// Parameters for `touring_ctx_discover` — compression-profile catalog. No inputs.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxDiscoverParams {}

/// Parameters for `touring_ctx_tee_retrieve` — fetch full unredacted output
/// for a sandbox failure stored via tee mode.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CtxTeeRetrieveParams {
    /// blake3-64-hex content hash from a stored ToolOutputDoc with exit_code != 0.
    pub content_hash: String,
}

// ── Wave 3 INTELLIGENCE — 15 T1 MCP tool params ─────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxReplayParams {
    #[serde(default)]
    pub n: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CtxPurgeParams {
    #[serde(default)]
    pub tee_logs: Option<bool>,
    #[serde(default)]
    pub tool_outputs_index: Option<bool>,
    #[serde(default)]
    pub expired_memory: Option<bool>,
    #[serde(default)]
    pub all: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxDoctorParams {}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxGainHistoryParams {
    #[serde(default)]
    pub days: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxGainGraphParams {
    #[serde(default)]
    pub days: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxSessionAdoptionParams {}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxInitAgentParams {
    pub agent: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CtxSmartParams {
    pub file_path: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CtxChunkReadParams {
    pub file_path: String,
    #[serde(default)]
    pub threshold: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CtxExplainParams {
    pub counter_name: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CtxBudgetParams {
    #[serde(default)]
    pub used_tokens: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxBatchExecuteParams {
    pub items: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CtxExecuteFileParams {
    pub file_path: String,
    pub language: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CtxUpgradeParams {
    #[serde(default)]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CtxDiscoverSessionParams {}

// ── Wave 3 Extended (T2 + T3) — generic shared params ───────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct Wave3EmptyParams {}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct Wave3StringParams {
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct Wave3StringNParams {
    pub value: String,
    #[serde(default)]
    pub n: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct Wave3NParams {
    #[serde(default)]
    pub n: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct Wave3HitsNParams {
    pub hits: Vec<String>,
    #[serde(default)]
    pub n: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Wave3RawExitParams {
    pub raw: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct Wave3TierParams {
    #[serde(default)]
    pub tier: Option<u8>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Wave3PrParams {
    #[serde(default)]
    pub pr_number: Option<u64>,
}

// ── Source Change ───────────────────────────────────────────────────────

/// Parameters for `find_code` — unified code search super-tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FindCodeParams {
    /// The search query string.
    pub query: String,
    /// Optional intent override: understand, debug, lookup, refactor, explore, navigate, document
    pub intent_override: Option<String>,
    /// Maximum number of results to return (default: 20, max: 100).
    #[serde(default)]
    pub max_results: Option<usize>,
}

/// Response struct for `find_code` tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FindCodeResponse {
    /// List of search results sorted by fused score.
    pub results: Vec<FindCodeResult>,
    /// The detected intent from the query.
    pub detected_intent: String,
    /// Confidence score of the intent detection.
    pub confidence: f32,
}

/// A single search result from `find_code`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FindCodeResult {
    /// Absolute file path of the result.
    pub file_path: String,
    /// Line number (1-indexed) if available.
    pub line: Option<usize>,
    /// Column number (1-indexed) if available.
    pub col: Option<usize>,
    /// Symbol name if available.
    pub symbol: Option<String>,
    /// Code context snippet if available.
    pub context: Option<String>,
    /// Which backend produced this result (e.g., "hybrid-search-fusion").
    pub backend: String,
    /// Fused RRF score for ranking.
    pub rrf_score: f32,
    /// Confidence tier: high, medium, low, unknown
    pub confidence_tier: String,
}

// ── Source Change ───────────────────────────────────────────────────────

/// Parameters for `touring_source_change` — apply or preview transactional multi-file changes.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct SourceChangeParams {
    /// Operation: "preview" (dry-run shadow-validate), "apply" (atomic commit), or "validate" (alias for preview).
    pub operation: String,
    /// JSON string containing the SourceChange payload (edits, fs_edits, snippet).
    pub source_change_json: String,
    /// Output format: "json" or "text" (default: json).
    #[serde(default)]
    pub format: Option<String>,
}

// ── Clone Detection ─────────────────────────────────────────────────────

/// Parameters for `touring_detect_clones` — detect structural clone groups in a symbol collection.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct DetectClonesParams {
    /// Path to scan (optional, defaults to workspace root).
    pub path: Option<String>,
    /// Minimum similarity threshold (0.0-1.0, default 0.5).
    #[serde(default)]
    pub min_similarity: Option<f32>,
    /// Output verbosity level.
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// D2.4 — MCP tool: touring_ctx_execute sandboxed multi-language execution.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CtxExecuteParams {
    /// Programming language (js, python, ts, ruby, go, rust, shell, etc.)
    pub language: String,
    /// Code to execute in the sandbox.
    pub code: String,
    /// Optional JSON array of arguments (exposed as `sys.argv` / `process.argv`).
    #[serde(default)]
    pub args: Option<serde_json::Value>,
    /// Timeout in milliseconds (default: 30000, max: 120000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Working directory for execution (default: project root).
    #[serde(default)]
    pub cwd: Option<String>,
    /// P1.4 — Override the forbidden-call policy for this call.
    /// When `true`, forbidden calls are allowed even in Block mode (trusted caller).
    /// When `false` or absent, the environment-level policy applies.
    #[serde(default)]
    pub allow_forbidden: Option<bool>,
}

// ── Entity Identity Registry Tools (D5.4) ───────────────────────────────

/// Parameters for touring_entity_define tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityDefineParams {
    /// Unique identifier for the entity (e.g., "touring-hooks::cli_inferlets_exec")
    pub id: String,
    /// Canonical name of the entity
    pub name: String,
    /// Kind of entity: function, type, module, constant, trait, macro, file, config
    pub kind: String,
    /// Crate or module name this entity belongs to
    pub crate_name: String,
    /// Source file path (optional)
    pub source_path: Option<String>,
    /// Definition line number (optional)
    pub definition_line: Option<u32>,
    /// Doc comment summary (optional)
    pub doc_summary: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_entity_resolve tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityResolveParams {
    /// Name to resolve
    pub name: String,
    /// Maximum edit distance for fuzzy matching (default: 2)
    #[serde(default)]
    pub max_edit_distance: Option<u8>,
    /// If true, only return exact matches (default: false)
    #[serde(default)]
    pub exact_only: Option<bool>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_entity_relate tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityRelateParams {
    /// Source entity ID
    pub from: String,
    /// Relation kind: derived_from, refines, supersedes, equivalent, see_also, wraps
    pub kind: String,
    /// Target entity ID
    pub to: String,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_entity_list tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityListParams {
    /// Filter by crate name (optional)
    pub crate_name: Option<String>,
    /// Filter by entity kind (optional)
    pub kind: Option<String>,
    /// Maximum results (default: 50)
    #[serde(default)]
    pub limit: Option<u32>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for touring_entity_delete tool
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityDeleteParams {
    /// Entity ID to delete
    pub id: String,
    /// Reason for deletion (optional)
    pub reason: Option<String>,
    /// Output verbosity: minimal, standard (default), full
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Wave 2 P5 (Sentrux master plan, 2026-05-09) — Quality signal MCP tools ──

/// Parameters for `touring_quality_signal_compute`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QualitySignalComputeParams {
    /// Workspace root to walk for `.rs` files. Defaults to the daemon's project root.
    #[serde(default)]
    pub root: Option<String>,
    /// Drop the bulky `diagnostics` block from the response.
    #[serde(default)]
    pub no_diagnostics: Option<bool>,
    /// Output verbosity: minimal, standard (default), full.
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for `touring_quality_rules_evaluate`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QualityRulesEvaluateParams {
    /// Workspace root to walk. Defaults to the daemon's project root.
    #[serde(default)]
    pub root: Option<String>,
    /// Path to a TOML rules file. Mutually exclusive with `rules_toml`.
    #[serde(default)]
    pub rules_path: Option<String>,
    /// Inline TOML rules content. Mutually exclusive with `rules_path`.
    #[serde(default)]
    pub rules_toml: Option<String>,
    /// Output verbosity: minimal, standard (default), full.
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Parameters for `touring_quality_signal_diff`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QualitySignalDiffParams {
    /// Previous-snapshot workspace root.
    pub previous_root: String,
    /// Current-snapshot workspace root.
    pub current_root: String,
    /// Trend epsilon on the 0..=10000 Sentrux scale (default 50).
    #[serde(default)]
    pub trend_epsilon: Option<i32>,
    /// Output verbosity: minimal, standard (default), full.
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Single workspace entry for `touring_quality_federation_aggregate`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FederationWorkspaceParam {
    /// Stable identifier (typically the directory leaf or org+repo tag).
    pub workspace_id: String,
    /// Filesystem root scanned to compute the signal.
    pub root: String,
}

/// Parameters for `touring_quality_federation_aggregate`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QualityFederationAggregateParams {
    /// List of workspaces (1..=64) to compute signals for and aggregate
    /// into a single federated summary.
    pub workspaces: Vec<FederationWorkspaceParam>,
    /// Output verbosity: minimal, standard (default), full.
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

// ── Helpers ──────────────────────────────────────────────────────────────

pub(crate) fn default_true() -> bool {
    true
}
