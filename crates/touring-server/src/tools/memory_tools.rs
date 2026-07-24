//! Memory Tools - MCP tools for memory operations
//!
//! Implements:
//! - touring_memory_recall: Query memory (RLM + semantic)
//! - touring_memory_store: Store a memory entry

use crate::memory_store::{MemoryEntry, MemoryQuery, MemoryStore, MemoryStoreBuilder};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use touring_foundation::config::TouringConfig;

/// Input for touring_memory_recall tool
#[derive(Debug, Deserialize)]
pub struct MemoryRecallInput {
    /// Query string to search for
    pub query: String,
    /// Optional tier filter (ephemeral, working, reference, core)
    pub tier: Option<String>,
    /// Maximum number of results to return
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Minimum score threshold
    #[serde(default = "default_min_score")]
    pub min_score: f32,
    /// Use full-text search in addition to RLM
    #[serde(default = "default_use_fts")]
    pub use_fts: bool,
}

fn default_top_k() -> usize {
    10
}
fn default_min_score() -> f32 {
    0.0
}
fn default_use_fts() -> bool {
    true
}

/// Match result from memory recall
#[derive(Debug, Serialize)]
pub struct MemoryMatchResult {
    /// Lookup key of the matched entry.
    pub key: String,
    /// Memory tier the matched entry belongs to.
    pub tier: String,
    /// Stored value of the matched entry.
    pub value: String,
    /// Relevance score of the match.
    pub score: f32,
    /// Optional classification of the entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_type: Option<String>,
    /// Optional number of times the entry has been accessed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_count: Option<i64>,
}

/// Output from touring_memory_recall tool
#[derive(Debug, Serialize)]
pub struct MemoryRecallOutput {
    /// Matched entries ordered by relevance.
    pub matches: Vec<MemoryMatchResult>,
    /// Total number of matches returned.
    pub total_matches: usize,
    /// The query that produced these matches.
    pub query: String,
}

/// Input for touring_memory_store tool
#[derive(Debug, Deserialize)]
pub struct MemoryStoreInput {
    /// Key for the memory entry
    pub key: String,
    /// Value to store
    pub value: String,
    /// Tier (ephemeral, working, reference, core)
    #[serde(default = "default_tier")]
    pub tier: String,
    /// Optional entry type
    pub entry_type: Option<String>,
    /// Optional embedding vector
    pub embedding: Option<Vec<f32>>,
}

fn default_tier() -> String {
    "working".to_string()
}

/// Output from touring_memory_store tool
#[derive(Debug, Serialize)]
pub struct MemoryStoreOutput {
    /// Whether the entry was stored successfully.
    pub stored: bool,
    /// Key under which the entry was stored.
    pub key: String,
    /// Tier the entry was stored in.
    pub tier: String,
}

/// Memory tools handler
#[derive(Debug)]
pub struct MemoryTools {
    store: Arc<Mutex<MemoryStore>>,
}

impl MemoryTools {
    /// Convert a semantic match to MemoryMatchResult, applying tier filter.
    fn convert_semantic_match(
        m: &touring_intelligence::rl::memory::ChunkMatch,
        tier_filter: &Option<String>,
    ) -> Option<MemoryMatchResult> {
        let key = m
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("key"))
            .and_then(|k| k.as_str())
            .unwrap_or("<semantic>")
            .to_string();

        let tier = m
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("tier"))
            .and_then(|t| t.as_str())
            .unwrap_or("semantic")
            .to_string();

        if let Some(filter) = tier_filter {
            if tier != **filter {
                return None;
            }
        }

        let entry_type = m
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("entry_type"))
            .and_then(|e| e.as_str())
            .map(|s| s.to_string());

        Some(MemoryMatchResult {
            key,
            tier,
            value: m.content.clone(),
            score: m.score,
            entry_type,
            access_count: None,
        })
    }

    /// Create new memory tools handler
    pub fn new(config: &TouringConfig) -> Result<Self, touring_foundation::TouringError> {
        let store = MemoryStoreBuilder::new()
            .rlm_path(&config.rlm_db_path)
            .recall_path(&config.semantic_db_path)
            .build()
            .map_err(|e| {
                touring_foundation::TouringError::Memory(format!(
                    "Failed to create memory store: {}",
                    e
                ))
            })?;

        Ok(Self {
            store: Arc::new(Mutex::new(store)),
        })
    }

    /// Handle touring_memory_recall tool
    pub async fn recall(
        &self,
        input: MemoryRecallInput,
    ) -> Result<MemoryRecallOutput, touring_foundation::TouringError> {
        let store = self.store.lock().await;

        // Capture tier filter before moving input.tier
        let tier_filter = input.tier.clone();

        let query = MemoryQuery::new(&input.query)
            .with_top_k(input.top_k)
            .with_min_score(input.min_score)
            .with_fts(input.use_fts);

        let query = if let Some(tier) = input.tier {
            query.with_tier(tier)
        } else {
            query
        };

        let result = store.query(query).map_err(|e| {
            touring_foundation::TouringError::Memory(format!("Query failed: {}", e))
        })?;

        let mut matches: Vec<MemoryMatchResult> = Vec::new();

        // Convert RLM matches (with tier filter applied)
        for m in result.rlm_matches {
            // Skip if tier filter is specified and doesn't match
            if let Some(ref filter) = tier_filter {
                if m.tier != **filter {
                    continue;
                }
            }
            matches.push(MemoryMatchResult {
                key: m.key,
                tier: m.tier,
                value: m.value,
                score: m.score,
                entry_type: m.entry_type,
                access_count: Some(m.access_count),
            });
        }

        // Convert semantic matches (from FTS)
        for m in result.semantic_matches {
            if let Some(result) = Self::convert_semantic_match(&m, &tier_filter) {
                matches.push(result);
            }
        }

        let total_matches = matches.len();

        Ok(MemoryRecallOutput {
            matches,
            total_matches,
            query: input.query,
        })
    }

    /// Handle touring_memory_store tool
    pub async fn store(
        &self,
        input: MemoryStoreInput,
    ) -> Result<MemoryStoreOutput, touring_foundation::TouringError> {
        let store = self.store.lock().await;

        let entry = MemoryEntry {
            key: input.key.clone(),
            tier: input.tier.clone(),
            value: input.value,
            entry_type: input.entry_type,
            embedding: input.embedding,
        };

        store.store(entry).map_err(|e| {
            touring_foundation::TouringError::Memory(format!("Store failed: {}", e))
        })?;

        Ok(MemoryStoreOutput {
            stored: true,
            key: input.key,
            tier: input.tier,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config() -> (TouringConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        let config = TouringConfig {
            project_root: temp_dir.path().to_path_buf(),
            symbols_db_path: data_dir.join("symbols.db"),
            rlm_db_path: data_dir.join("rlm_memory.db"),
            semantic_db_path: data_dir.join("semantic_recall.db"),
            cache_size: 1000,
            watcher_debounce_ms: 100,
            max_file_size: 1024 * 1024,
            debug: false,
            gpu_service_url: "http://localhost:8200".to_string(),
            embedding_dim: 384,
            auto_embed: false,
            ..Default::default()
        };

        (config, temp_dir)
    }

    #[tokio::test]
    async fn test_memory_store_and_recall() {
        let config = create_test_config();
        let tools = MemoryTools::new(&config.0).unwrap();

        // Store a memory
        let store_input = MemoryStoreInput {
            key: "test_key".to_string(),
            value: "test value content".to_string(),
            tier: "working".to_string(),
            entry_type: Some("test".to_string()),
            embedding: None,
        };

        let store_result = tools.store(store_input).await.unwrap();
        assert!(store_result.stored);
        assert_eq!(store_result.key, "test_key");

        // Recall it
        let recall_input = MemoryRecallInput {
            query: "test".to_string(),
            tier: None,
            top_k: 10,
            min_score: 0.0,
            use_fts: true,
        };

        let recall_result = tools.recall(recall_input).await.unwrap();
        assert!(recall_result.total_matches > 0);
        assert!(recall_result.matches.iter().any(|m| m.key == "test_key"));
    }

    #[tokio::test]
    async fn test_tier_filtering() {
        let config = create_test_config();
        let tools = MemoryTools::new(&config.0).unwrap();

        // Store in different tiers
        tools
            .store(MemoryStoreInput {
                key: "key1".to_string(),
                value: "working value".to_string(),
                tier: "working".to_string(),
                entry_type: None,
                embedding: None,
            })
            .await
            .unwrap();

        tools
            .store(MemoryStoreInput {
                key: "key2".to_string(),
                value: "reference value".to_string(),
                tier: "reference".to_string(),
                entry_type: None,
                embedding: None,
            })
            .await
            .unwrap();

        // Query with tier filter
        let recall_input = MemoryRecallInput {
            query: "value".to_string(),
            tier: Some("working".to_string()),
            top_k: 10,
            min_score: 0.0,
            use_fts: true,
        };

        let result = tools.recall(recall_input).await.unwrap();
        // Should return 2 matches: 1 from RLM + 1 from semantic search
        // Both are key1 from working tier (filter excludes key2 from reference tier)
        assert_eq!(result.matches.len(), 2);
        assert!(result.matches.iter().all(|m| m.tier == "working"));
        assert!(result.matches.iter().any(|m| m.key == "key1"));
    }
}
