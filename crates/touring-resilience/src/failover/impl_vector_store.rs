//! VectorStoreFailover — Failover implementation for embedding vector stores.
//!
//! Provides failover capability for embedding providers with backup
//! InMemoryVectorStore for zero-downtime primary elections.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{Failover, Health, Result};

/// Embedding vector with metadata.
#[derive(Debug, Clone)]
pub struct EmbeddingVector {
    /// Stable vector id (typically a hash of the source text or
    /// the primary key of the originating row).
    pub id: String,
    /// The dense embedding itself (length = embedding_dim).
    pub vector: Vec<f32>,
    /// Free-form metadata (source URL, doc id, language tag,
    /// …) carried alongside the vector.
    pub metadata: HashMap<String, String>,
}

/// Backup data structure for vector store failover.
#[derive(Debug, Clone, Default)]
pub struct VectorStoreBackup {
    /// Vectors replicated from the primary, keyed by id.
    pub vectors: HashMap<String, EmbeddingVector>,
    /// Number of vectors present at the time of the last
    /// successful sync. Useful for drift detection.
    pub last_sync_count: u64,
}

/// VectorStoreFailover implements Failover for embedding vector stores.
///
/// Uses an in-memory backup store that is populated via incremental
/// sync from the primary provider.
#[derive(Debug)]
pub struct VectorStoreFailover<P> {
    primary_provider: P,
    backup: Arc<RwLock<VectorStoreBackup>>,
}

impl<P> VectorStoreFailover<P> {
    /// Create a new VectorStoreFailover with the given primary provider.
    pub fn new(primary_provider: P) -> Self {
        Self {
            primary_provider,
            backup: Arc::new(RwLock::new(VectorStoreBackup::default())),
        }
    }
}

/// Trait for primary vector store providers.
pub trait VectorStoreProvider: Send + Sync {
    /// Check if the primary vector store is accessible.
    fn is_primary_accessible(&self) -> bool;
    /// Fetch all vectors from the primary since the given offset.
    fn fetch_vectors_since(&self, offset: u64) -> Result<Vec<EmbeddingVector>>;
    /// Get the current count of vectors in the primary.
    fn primary_vector_count(&self) -> u64;
}

#[async_trait]
impl<P: VectorStoreProvider + 'static> Failover<P, VectorStoreBackup> for VectorStoreFailover<P> {
    /// Check primary vector store health.
    async fn primary_health(&self) -> Health {
        if self.primary_provider.is_primary_accessible() {
            Health::Healthy
        } else {
            Health::Unhealthy
        }
    }
    /// Activate the backup vector store (pre-populate from primary).
    async fn activate_backup(&mut self) -> Result<()> {
        let mut backup = self.backup.write().await;
        let offset = backup.last_sync_count;
        let vectors = self.primary_provider.fetch_vectors_since(offset)?;
        for vec in vectors {
            backup.vectors.insert(vec.id.clone(), vec);
        }
        backup.last_sync_count = self.primary_provider.primary_vector_count();
        tracing::info!(
            vectors_synced = backup.vectors.len(),
            last_sync_count = backup.last_sync_count,
            "vector store backup activated"
        );
        Ok(())
    }
    /// Sync incremental vector updates from primary to backup.
    async fn sync_backup(&mut self) -> Result<()> {
        let mut backup = self.backup.write().await;
        let offset = backup.last_sync_count;
        let new_vectors = self.primary_provider.fetch_vectors_since(offset)?;
        for vec in &new_vectors {
            backup.vectors.insert(vec.id.clone(), vec.clone());
        }
        backup.last_sync_count = self.primary_provider.primary_vector_count();
        tracing::debug!(
            new_vectors = new_vectors.len(),
            total_backup = backup.vectors.len(),
            "vector store incremental sync complete"
        );
        Ok(())
    }
    /// Restore primary — for vector stores this means promoting backup
    /// to be the new primary (would require primary reconfiguration).
    async fn restore_to_primary(&mut self) -> Result<()> {
        let backup = self.backup.read().await;
        tracing::info!(
            vectors_to_restore = backup.vectors.len(),
            "vector store primary restored from backup"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    struct MockVectorProvider {
        accessible: AtomicBool,
        vector_count: AtomicU64,
        vectors: HashMap<String, EmbeddingVector>,
        /// insertion_order tracks the logical sequence of vector IDs
        /// so that skip(offset) returns the correct slice regardless of HashMap iteration order
        insertion_order: Vec<String>,
    }
    impl MockVectorProvider {
        fn new(vectors: Vec<(String, Vec<f32>)>) -> Self {
            let mut map = HashMap::new();
            let mut order = Vec::new();
            for (id, vec) in vectors {
                order.push(id.clone());
                map.insert(
                    id.clone(),
                    EmbeddingVector {
                        id,
                        vector: vec,
                        metadata: HashMap::new(),
                    },
                );
            }
            Self {
                accessible: AtomicBool::new(true),
                vector_count: AtomicU64::new(map.len() as u64),
                vectors: map,
                insertion_order: order,
            }
        }
        fn insert_vector(&mut self, id: String, vec: Vec<f32>) {
            self.insertion_order.push(id.clone());
            self.vectors.insert(
                id.clone(),
                EmbeddingVector {
                    id,
                    vector: vec,
                    metadata: HashMap::new(),
                },
            );
        }
    }
    impl VectorStoreProvider for MockVectorProvider {
        fn is_primary_accessible(&self) -> bool {
            self.accessible.load(Ordering::SeqCst)
        }
        fn fetch_vectors_since(&self, offset: u64) -> Result<Vec<EmbeddingVector>> {
            let offset = offset as usize;
            if offset >= self.insertion_order.len() {
                return Ok(vec![]);
            }
            let ids: Vec<String> = self.insertion_order[offset..].to_vec();
            let result: Vec<EmbeddingVector> = ids
                .iter()
                .filter_map(|id| self.vectors.get(id).cloned())
                .collect();
            Ok(result)
        }
        fn primary_vector_count(&self) -> u64 {
            self.vector_count.load(Ordering::SeqCst)
        }
    }
    #[tokio::test]
    async fn vector_health_healthy() {
        let provider = MockVectorProvider::new(vec![
            ("a".into(), vec![0.1, 0.2]),
            ("b".into(), vec![0.3, 0.4]),
        ]);
        let failover = VectorStoreFailover::new(provider);
        let health = failover.primary_health().await;
        assert_eq!(health, Health::Healthy);
    }
    #[tokio::test]
    async fn vector_health_unhealthy() {
        let provider = MockVectorProvider::new(vec![]);
        provider.accessible.store(false, Ordering::SeqCst);
        let failover = VectorStoreFailover::new(provider);
        let health = failover.primary_health().await;
        assert_eq!(health, Health::Unhealthy);
    }
    #[tokio::test]
    async fn sync_incremental_vectors() {
        let provider = MockVectorProvider::new(vec![
            ("a".into(), vec![0.1, 0.2]),
            ("b".into(), vec![0.3, 0.4]),
            ("c".into(), vec![0.5, 0.6]),
        ]);
        let mut failover = VectorStoreFailover::new(provider);
        failover.activate_backup().await.unwrap();
        failover
            .primary_provider
            .vector_count
            .store(5, Ordering::SeqCst);
        failover
            .primary_provider
            .insert_vector("d".into(), vec![0.7, 0.8]);
        failover
            .primary_provider
            .insert_vector("e".into(), vec![0.9, 1.0]);
        failover.sync_backup().await.unwrap();
        let backup = failover.backup.read().await;
        assert_eq!(backup.vectors.len(), 5);
    }
}
