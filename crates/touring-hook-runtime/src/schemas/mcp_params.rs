//! MCP tool parameter schemas with validator derive.
//!
//! Feature D (2026-04-24) — Schema Validation Layer
//!
//! Provides type-safe parameter structs for MCP tool handlers.

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Parameters for `decompose create` MCP tool / CLI.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DecomposeCreateParams {
    /// Category of the task being created.
    #[validate(length(min = 1, max = 100, message = "task_type must be 1-100 chars"))]
    pub task_type: String,

    /// Human-readable description of the task.
    #[validate(length(min = 1, max = 1000, message = "description must be 1-1000 chars"))]
    pub description: String,

    /// Source that originated the task (e.g. `"claude-code"`).
    pub origin: Option<String>,
    /// CILA routing level to assign to the task.
    pub cila_level: Option<u8>,
}

/// Parameters for `decompose add` MCP tool / CLI.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DecomposeAddParams {
    /// Identifier of the parent task in the DAG.
    #[validate(length(min = 1, message = "task_id cannot be empty"))]
    pub task_id: String,

    /// Identifier of the subtask being added.
    #[validate(length(min = 1, message = "subtask_id cannot be empty"))]
    pub subtask_id: String,

    /// Human-readable description of the subtask.
    #[validate(length(min = 1, max = 1000, message = "description must be 1-1000 chars"))]
    pub description: String,

    /// Identifiers of subtasks this one depends on.
    pub depends_on: Option<Vec<String>>,
    /// Scheduling priority of the subtask.
    pub priority: Option<u8>,
}

/// Parameters for `memory store` MCP tool / CLI.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemoryStoreParams {
    /// Lookup key under which to store the entry.
    #[validate(length(min = 1, message = "key cannot be empty"))]
    pub key: String,

    /// Content of the memory entry.
    #[validate(length(min = 1, message = "value cannot be empty"))]
    pub value: String,

    /// Tier: "semantic" | "local"
    pub tier: Option<String>,
    /// Memory type: "lesson" | "pattern" | "insight" | "gotcha"
    pub memory_type: Option<String>,
}

/// Parameters for `memory recall` MCP tool / CLI.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemoryRecallParams {
    /// Search query to match against stored memory entries.
    #[validate(length(min = 1, message = "query cannot be empty"))]
    pub query: String,

    /// Maximum number of results to return.
    pub limit: Option<usize>,
    /// Restrict recall to a specific memory tier.
    pub tier: Option<String>,
}

/// Parameters for `decompose update` MCP tool / CLI.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DecomposeUpdateParams {
    /// Identifier of the task or subtask to update.
    #[validate(length(min = 1, message = "task_id cannot be empty"))]
    pub task_id: String,

    /// New status to set.
    pub status: Option<String>,
    /// Updated quality score, in `[0.0, 1.0]`.
    pub quality_score: Option<f64>,
    /// Error message to record, if the task failed.
    pub error: Option<String>,
}

/// Parameters for `decompose finalize` MCP tool / CLI.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DecomposeFinalizeParams {
    /// Identifier of the task to finalize.
    #[validate(length(min = 1, message = "task_id cannot be empty"))]
    pub task_id: String,

    /// Minimum quality score required to finalize the task.
    pub quality_threshold: Option<f64>,
}

/// Parameters for `decompose ready` MCP tool / CLI.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DecomposeReadyParams {
    /// Identifier of the task whose ready subtasks to list; omit for all tasks.
    pub task_id: Option<String>,
}

/// Parameters for `touring index find` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IndexFindParams {
    /// Symbol name to look up in the index.
    #[validate(length(min = 1, message = "symbol cannot be empty"))]
    pub symbol: String,

    /// Restrict the lookup to a specific file.
    pub file_path: Option<String>,
    /// Maximum number of matches to return.
    pub limit: Option<usize>,
}

/// Parameters for `touring index search` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IndexSearchParams {
    /// Search query to match against indexed symbols.
    #[validate(length(min = 1, message = "query cannot be empty"))]
    pub query: String,

    /// Maximum number of results to return.
    pub limit: Option<usize>,
}

/// Parameters for `touring tantivy search` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TantivySearchParams {
    /// Full-text query string for the Tantivy BM25 search.
    #[validate(length(min = 1, message = "query cannot be empty"))]
    pub query: String,

    /// Number of top-ranked hits to return.
    pub top_k: Option<usize>,
}

/// Parameters for `touring tantivy fuzzy` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TantivyFuzzyParams {
    /// Query string for the fuzzy Tantivy search.
    #[validate(length(min = 1, message = "query cannot be empty"))]
    pub query: String,

    /// Maximum Levenshtein edit distance for fuzzy matching.
    pub distance: Option<usize>,
    /// Number of top-ranked hits to return.
    pub top_k: Option<usize>,
}

/// Parameters for `touring gotcha add` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct GotchaAddParams {
    /// Pattern that identifies the pitfall.
    #[validate(length(min = 1, message = "pattern cannot be empty"))]
    pub pattern: String,

    /// Human-readable description of the gotcha.
    #[validate(length(min = 1, message = "description cannot be empty"))]
    pub description: String,

    /// Severity bucket (e.g. low, medium, high).
    pub severity: Option<String>,
}

/// Parameters for `touring learning reward` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LearningRewardParams {
    /// Name of the tool receiving the RL reward.
    #[validate(length(min = 1, message = "tool cannot be empty"))]
    pub tool: String,

    /// Reward value, in `[-1.0, 1.0]`.
    pub value: Option<f64>,
    /// Free-form context describing the outcome.
    pub context: Option<String>,
}

/// Parameters for `touring e2e` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct E2eParams {
    /// Depth of the end-to-end check (e.g. quick, standard, deep).
    pub depth: Option<String>,
    /// Restrict the check to a specific file.
    pub file_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompose_create_valid() {
        let p = DecomposeCreateParams {
            task_type: "intent".to_string(),
            description: "Implement feature X".to_string(),
            origin: Some("claude-code".to_string()),
            cila_level: Some(3),
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn decompose_create_empty_type_fails() {
        let p = DecomposeCreateParams {
            task_type: "".to_string(),
            description: "Implement feature X".to_string(),
            origin: None,
            cila_level: None,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn memory_store_valid() {
        let p = MemoryStoreParams {
            key: "lesson:error_handling".to_string(),
            value: "Always use ? instead of unwrap in prod".to_string(),
            tier: Some("semantic".to_string()),
            memory_type: Some("lesson".to_string()),
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn memory_store_empty_key_fails() {
        let p = MemoryStoreParams {
            key: "".to_string(),
            value: "Some value".to_string(),
            tier: None,
            memory_type: None,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn index_find_valid() {
        let p = IndexFindParams {
            symbol: "HookContext".to_string(),
            file_path: None,
            limit: Some(10),
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn tantivy_search_valid() {
        let p = TantivySearchParams {
            query: "hook context".to_string(),
            top_k: Some(5),
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn learning_reward_valid() {
        let p = LearningRewardParams {
            tool: "edit".to_string(),
            value: Some(1.0),
            context: Some("checkpoint_passed".to_string()),
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn e2e_valid() {
        let p = E2eParams {
            depth: Some("standard".to_string()),
            file_path: Some("src/main.rs".to_string()),
        };
        assert!(p.validate().is_ok());
    }
}
