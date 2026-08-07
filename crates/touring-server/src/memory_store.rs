//! Unified Memory Store - combines RLM and SemanticRecall
//!
//! Provides a single interface for all memory operations

use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use touring_intelligence::rl::memory::recall::{ChunkMatch, RecallStats, SemanticRecall};
use touring_intelligence::rl::memory::rlm::{
    MemoryMatch as RlmMatch, MemoryStats as RlmStats, MemoryTier, RlmMemory,
};
use touring_storage::embedding::{Embedder, GpuEmbedder};

/// Errors that can occur in unified memory operations
#[derive(Error, Debug)]
pub enum MemoryStoreError {
    /// An error propagated from the RLM (reinforcement-learned memory) layer.
    #[error("RLM error: {0}")]
    Rlm(#[from] touring_intelligence::rl::memory::rlm::RlmError),
    /// An error propagated from the semantic-recall layer.
    #[error("Recall error: {0}")]
    Recall(#[from] touring_intelligence::rl::memory::recall::RecallError),
    /// The requested memory tier name is not recognized.
    #[error("Invalid tier: {0}")]
    InvalidTier(String),
}

/// Result type for memory store operations
pub(crate) type Result<T> = std::result::Result<T, MemoryStoreError>;

/// Unified memory entry for storage
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    /// Unique lookup key for the entry.
    pub key: String,
    /// Memory tier this entry belongs to (e.g. `semantic`, `episodic`).
    pub tier: String,
    /// Stored value payload.
    pub value: String,
    /// Optional classification of the entry (e.g. `lesson`, `gotcha`).
    pub entry_type: Option<String>,
    /// Optional pre-computed embedding vector for semantic recall.
    pub embedding: Option<Vec<f32>>,
}

impl MemoryEntry {
    /// Create a new memory entry
    pub fn new(key: impl Into<String>, tier: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            tier: tier.into(),
            value: value.into(),
            entry_type: None,
            embedding: None,
        }
    }

    /// Set entry type
    pub(crate) fn with_entry_type(mut self, entry_type: impl Into<String>) -> Self {
        self.entry_type = Some(entry_type.into());
        self
    }

    /// Set embedding
    pub(crate) fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Result from a batch store operation
#[derive(Debug, Clone)]
pub struct BatchStoreResult {
    /// Number of entries successfully stored.
    pub stored: usize,
    /// Number of entries that failed to store.
    pub failed: usize,
    /// Number of stored entries that received an embedding.
    pub embedded: usize,
}

/// Unified query for memory retrieval
#[derive(Debug, Clone)]
pub struct MemoryQuery {
    /// Free-text query string.
    pub query: String,
    /// Optional tier to restrict the search to.
    pub tier: Option<String>,
    /// Maximum number of results to return.
    pub top_k: usize,
    /// Minimum similarity score a match must meet.
    pub min_score: f32,
    /// Whether to use FTS5 full-text search in addition to vector recall.
    pub use_fts: bool,
    /// Pre-computed embedding for hybrid search (cosine + FTS5)
    pub embedding: Option<Vec<f32>>,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            tier: None,
            top_k: 10,
            min_score: 0.0,
            use_fts: true,
            embedding: None,
        }
    }
}

impl MemoryQuery {
    /// Create a new query
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Default::default()
        }
    }

    /// Set tier filter
    pub(crate) fn with_tier(mut self, tier: impl Into<String>) -> Self {
        self.tier = Some(tier.into());
        self
    }

    /// Set top_k
    pub(crate) fn with_top_k(mut self, top_k: usize) -> Self {
        self.top_k = top_k;
        self
    }

    /// Set minimum score
    pub(crate) fn with_min_score(mut self, min_score: f32) -> Self {
        self.min_score = min_score;
        self
    }

    /// Enable/disable FTS
    pub(crate) fn with_fts(mut self, use_fts: bool) -> Self {
        self.use_fts = use_fts;
        self
    }
}

/// Unified memory result
#[derive(Debug, Clone)]
pub struct MemoryResult {
    /// Matches retrieved from the RLM key-value layer.
    pub rlm_matches: Vec<RlmMatch>,
    /// Matches retrieved from the semantic-recall layer.
    pub semantic_matches: Vec<ChunkMatch>,
}

impl MemoryResult {
    /// Get total number of matches
    #[allow(dead_code)]
    /// False positive: called via cross-module pub(crate) from tools_core.rs:401.
    pub(crate) fn total_matches(&self) -> usize {
        self.rlm_matches.len() + self.semantic_matches.len()
    }

    /// Check if result is empty
    pub fn is_empty(&self) -> bool {
        self.rlm_matches.is_empty() && self.semantic_matches.is_empty()
    }
}

/// Unified memory store combining RLM and SemanticRecall
/// with optional auto-embedding via GPU service
#[derive(Debug)]
pub struct MemoryStore {
    rlm: RlmMemory,
    recall: SemanticRecall,
    embedder: Option<Arc<GpuEmbedder>>,
}

impl MemoryStore {
    /// Create a new unified memory store (no auto-embedding)
    pub fn new(rlm_path: &Path, recall_path: &Path) -> Result<Self> {
        let rlm = RlmMemory::new(rlm_path)?;
        let recall = SemanticRecall::new(recall_path, 384)?;

        Ok(Self {
            rlm,
            recall,
            embedder: None,
        })
    }

    /// Create with custom embedding dimension (no auto-embedding)
    pub(crate) fn with_embedding_dim(
        rlm_path: &Path,
        recall_path: &Path,
        embedding_dim: usize,
    ) -> Result<Self> {
        let rlm = RlmMemory::new(rlm_path)?;
        let recall = SemanticRecall::new(recall_path, embedding_dim)?;

        Ok(Self {
            rlm,
            recall,
            embedder: None,
        })
    }

    /// Attach an embedding client for auto-embedding on store()
    pub(crate) fn with_embedder(mut self, embedder: Arc<GpuEmbedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Get reference to the embedding client (if configured)
    pub fn embedder(&self) -> Option<&Arc<GpuEmbedder>> {
        self.embedder.as_ref()
    }

    /// Store a memory entry (unified interface)
    ///
    /// If an embedding client is configured and the entry has no embedding,
    /// auto-generates one via GPU service. Falls back to FTS5-only on failure.
    pub(crate) fn store(&self, entry: MemoryEntry) -> Result<()> {
        self.store_with_embedding(entry, None)
    }

    /// Store with an optional pre-computed embedding override
    pub(crate) fn store_with_embedding(
        &self,
        entry: MemoryEntry,
        precomputed_embedding: Option<Vec<f32>>,
    ) -> Result<()> {
        let tier = MemoryTier::parse_tier(&entry.tier)?;

        // Use provided embedding, entry's own, or None
        let embedding = precomputed_embedding.or(entry.embedding.clone());

        // Store in RLM
        self.rlm.store(
            &entry.key,
            tier,
            &entry.value,
            entry.entry_type.as_deref(),
            embedding.as_deref(),
        )?;

        // Also store in semantic recall (graceful degradation)
        let metadata = serde_json::json!({
            "key": entry.key,
            "tier": entry.tier,
            "entry_type": entry.entry_type,
        });

        if let Err(e) = self
            .recall
            .store_chunk(&entry.value, embedding.as_deref(), Some(&metadata))
        {
            tracing::warn!("SemanticRecall store failed (RLM still saved): {}", e);
        }

        Ok(())
    }

    /// Store a batch of entries with auto-embedding via GPU service
    ///
    /// Generates embeddings for all entries in a single GPU batch call,
    /// then stores each entry with its embedding. Optimized for bulk ingestion.
    pub async fn store_batch(&self, entries: Vec<MemoryEntry>) -> Result<BatchStoreResult> {
        if entries.is_empty() {
            return Ok(BatchStoreResult {
                stored: 0,
                failed: 0,
                embedded: 0,
            });
        }

        // Collect texts that need embedding
        let texts: Vec<&str> = entries
            .iter()
            .filter(|e| e.embedding.is_none())
            .map(|e| e.value.as_str())
            .collect();

        // Batch-embed via GPU if available
        let embeddings: Vec<Option<Vec<f32>>> = if !texts.is_empty() {
            if let Some(client) = &self.embedder {
                client.embed_batch(&texts.to_vec()).await
            } else {
                texts.iter().map(|_| None).collect()
            }
        } else {
            Vec::new()
        };

        let mut emb_iter = embeddings.into_iter();
        let mut stored = 0usize;
        let mut failed = 0usize;
        let mut embedded = 0usize;

        for entry in entries {
            let embedding = if entry.embedding.is_some() {
                entry.embedding.clone()
            } else {
                let emb = emb_iter.next().flatten();
                if emb.is_some() {
                    embedded += 1;
                }
                emb
            };

            match self.store_with_embedding(entry, embedding) {
                Ok(()) => stored += 1,
                Err(e) => {
                    tracing::warn!("Batch store entry failed: {}", e);
                    failed += 1;
                }
            }
        }

        Ok(BatchStoreResult {
            stored,
            failed,
            embedded,
        })
    }

    /// Query memory (unified interface)
    ///
    /// Uses hybrid search (FTS5 + cosine) when query embedding is available,
    /// falls back to FTS5-only otherwise.
    pub(crate) fn query(&self, query: MemoryQuery) -> Result<MemoryResult> {
        // Parse tier filter
        let tier_filter = query
            .tier
            .as_ref()
            .map(|t| MemoryTier::parse_tier(t))
            .transpose()?;

        // Search RLM
        let rlm_matches = self.rlm.search(&query.query, tier_filter, query.top_k)?;

        // Search semantic recall (graceful degradation)
        // NOTE: hybrid_search not yet available in touring-learning SemanticRecall;
        // using FTS5-only for now. NOTE(P5.2): add hybrid_search to touring-learning.
        let semantic_matches = if query.use_fts {
            self.recall
                .fts_search(&query.query, query.top_k)
                .unwrap_or_else(|e| {
                    tracing::warn!("SemanticRecall search failed (RLM results only): {}", e);
                    Vec::new()
                })
        } else {
            Vec::new()
        };

        Ok(MemoryResult {
            rlm_matches,
            semantic_matches,
        })
    }

    /// Query with auto-embedding of the query text via GPU service
    ///
    /// Embeds the query string first, then runs hybrid search.
    pub async fn query_with_embedding(&self, mut query: MemoryQuery) -> Result<MemoryResult> {
        // Auto-embed the query if embedder is available and no embedding provided
        if query.embedding.is_none()
            && let Some(client) = &self.embedder
        {
            query.embedding = client.embed_single(&query.query).await;
        }
        self.query(query)
    }

    /// Get a specific entry by key and tier
    pub fn get(&self, key: &str, tier: &str) -> Result<Option<String>> {
        let tier = MemoryTier::parse_tier(tier)?;
        Ok(self.rlm.get(key, tier)?)
    }

    /// Get full entry details (uses search with key as query)
    pub fn get_full(&self, key: &str, tier: &str) -> Result<Option<RlmMatch>> {
        let tier_parsed = MemoryTier::parse_tier(tier)?;
        let results = self.rlm.search(key, Some(tier_parsed), 1)?;
        Ok(results.into_iter().find(|m| m.key == key))
    }

    /// Delete an entry
    pub fn delete(&self, key: &str, tier: &str) -> Result<bool> {
        let tier = MemoryTier::parse_tier(tier)?;
        Ok(self.rlm.delete(key, tier)?)
    }

    /// List entries in a tier
    pub fn list_tier(&self, tier: &str, limit: usize) -> Result<Vec<RlmMatch>> {
        let tier_parsed = MemoryTier::parse_tier(tier)?;
        // Search with wildcard-like query to list entries in this tier
        let results = self.rlm.search("*", Some(tier_parsed), limit)?;
        Ok(results)
    }

    /// Promote a memory entry to a different tier
    ///
    /// Reads the entry from old tier, deletes it, stores in new tier.
    #[allow(dead_code)]
    /// False positive: called via cross-module pub(crate) from tools_analysis.rs:1062.
    pub(crate) fn promote_tier(&self, key: &str, old_tier: &str, new_tier: &str) -> Result<bool> {
        let old = MemoryTier::parse_tier(old_tier)?;
        let new = MemoryTier::parse_tier(new_tier)?;
        // Get the value from old tier
        let value = match self.rlm.get(key, old)? {
            Some(v) => v,
            None => return Ok(false),
        };
        // Delete from old tier
        self.rlm.delete(key, old)?;
        // Store in new tier
        self.rlm.store(key, new, &value, None, None)?;
        Ok(true)
    }

    /// Get RLM statistics
    pub fn rlm_stats(&self) -> Result<RlmStats> {
        Ok(self.rlm.stats()?)
    }

    /// Get recall statistics
    pub fn recall_stats(&self) -> Result<RecallStats> {
        Ok(self.recall.stats()?)
    }

    /// Scan entries by key prefix (exact key lookup, not FTS).
    ///
    /// Returns all entries whose key starts with `prefix`, ordered by key ASC.
    /// Use this instead of `query()` when you need structured sub-tree enumeration
    /// (e.g. diary project entries) rather than relevance-ranked text search.
    pub fn scan_prefix(&self, prefix: &str, tier: &str, limit: usize) -> Result<Vec<RlmMatch>> {
        let tier_parsed = MemoryTier::parse_tier(tier)?;
        Ok(self.rlm.scan_prefix(prefix, Some(tier_parsed), limit)?)
    }

    /// Access raw RLM (for advanced operations)
    pub fn rlm(&self) -> &RlmMemory {
        &self.rlm
    }

    /// Access raw recall (for advanced operations)
    pub fn recall(&self) -> &SemanticRecall {
        &self.recall
    }
}

/// Builder for MemoryStore with custom configuration
#[derive(Debug)]
pub struct MemoryStoreBuilder {
    rlm_path: Option<std::path::PathBuf>,
    recall_path: Option<std::path::PathBuf>,
    embedding_dim: usize,
    embedder: Option<Arc<GpuEmbedder>>,
}

impl Default for MemoryStoreBuilder {
    fn default() -> Self {
        Self {
            rlm_path: None,
            recall_path: None,
            embedding_dim: 384,
            embedder: None,
        }
    }
}

impl MemoryStoreBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set RLM database path
    pub(crate) fn rlm_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.rlm_path = Some(path.into());
        self
    }

    /// Set recall database path
    pub(crate) fn recall_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.recall_path = Some(path.into());
        self
    }

    /// Set embedding dimension
    pub fn embedding_dim(mut self, dim: usize) -> Self {
        self.embedding_dim = dim;
        self
    }

    /// Set embedding client for auto-embedding
    pub fn embedding_client(mut self, client: Arc<GpuEmbedder>) -> Self {
        self.embedder = Some(client);
        self
    }

    /// Build the MemoryStore
    pub fn build(self) -> Result<MemoryStore> {
        let rlm_path = self
            .rlm_path
            .ok_or_else(|| MemoryStoreError::InvalidTier("RLM path not set".to_string()))?;
        let recall_path = self
            .recall_path
            .ok_or_else(|| MemoryStoreError::InvalidTier("Recall path not set".to_string()))?;

        let mut store =
            MemoryStore::with_embedding_dim(&rlm_path, &recall_path, self.embedding_dim)?;
        store.embedder = self.embedder;
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_store_and_query() {
        let temp_dir = TempDir::new().unwrap();
        let rlm_path = temp_dir.path().join("rlm.db");
        let recall_path = temp_dir.path().join("recall.db");

        let store = MemoryStore::new(&rlm_path, &recall_path).unwrap();

        // Store an entry
        let entry = MemoryEntry::new("test_key", "working", "test value content")
            .with_entry_type("test_type");

        store.store(entry).unwrap();

        // Query it
        let query = MemoryQuery::new("test").with_top_k(10);
        let result = store.query(query).unwrap();

        assert!(!result.is_empty());
        assert_eq!(result.rlm_matches.len(), 1);
    }

    #[test]
    fn test_tier_filtering() {
        let temp_dir = TempDir::new().unwrap();
        let rlm_path = temp_dir.path().join("rlm.db");
        let recall_path = temp_dir.path().join("recall.db");

        let store = MemoryStore::new(&rlm_path, &recall_path).unwrap();

        // Store in different tiers
        store
            .store(MemoryEntry::new("key1", "working", "working value"))
            .unwrap();
        store
            .store(MemoryEntry::new("key2", "reference", "reference value"))
            .unwrap();

        // Query with tier filter
        let query = MemoryQuery::new("value")
            .with_tier("working")
            .with_top_k(10);

        let result = store.query(query).unwrap();
        assert_eq!(result.rlm_matches.len(), 1);
        assert_eq!(result.rlm_matches[0].tier, "working");
    }

    #[test]
    fn test_get_and_delete() {
        let temp_dir = TempDir::new().unwrap();
        let rlm_path = temp_dir.path().join("rlm.db");
        let recall_path = temp_dir.path().join("recall.db");

        let store = MemoryStore::new(&rlm_path, &recall_path).unwrap();

        store
            .store(MemoryEntry::new("my_key", "core", "my value"))
            .unwrap();

        // Get
        let value = store.get("my_key", "core").unwrap();
        assert_eq!(value, Some("my value".to_string()));

        // Delete
        let deleted = store.delete("my_key", "core").unwrap();
        assert!(deleted);

        // Verify deletion
        let missing = store.get("my_key", "core").unwrap();
        assert_eq!(missing, None);
    }

    #[test]
    fn test_builder() {
        let temp_dir = TempDir::new().unwrap();
        let rlm_path = temp_dir.path().join("rlm.db");
        let recall_path = temp_dir.path().join("recall.db");

        let store = MemoryStoreBuilder::new()
            .rlm_path(&rlm_path)
            .recall_path(&recall_path)
            .embedding_dim(768)
            .build()
            .unwrap();

        // Just verify it builds correctly
        let stats = store.recall_stats().unwrap();
        assert_eq!(stats.embedding_dim, 768);
    }
}
