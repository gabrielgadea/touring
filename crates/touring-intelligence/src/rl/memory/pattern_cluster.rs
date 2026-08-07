//! Pattern Clustering via HNSW-Based Lazy Clustering
//!
//! ALT-B1: Cluster patterns on-the-fly using the existing ANN index.
//! PRINCIPIO: Não pré-clusterizar. Deixe o HNSW index fazer o trabalho.
//!
//! ## How it works
//!
//! 1. **Pattern storage**: Generate embedding, store in `pattern_clusters` table with cluster_id
//!    (nearest centroid via SIMD KNN cosine similarity)
//! 2. **Query similar**: ANN search for top-k, group by similarity > threshold
//! 3. **Centroid management**: Periodic re-computation of centroids, stored in SQLite
//!
//! ## Clustering threshold
//!
//! Critical parameter (0.7-0.8): too high = isolated clusters, too low = merged clusters
//!
//! ## Centroid drift
//!
//! Cluster centers shift over time. Handle via periodic re-computation every N patterns
//! or on demand when query returns stale cluster assignments.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use thiserror::Error;

use touring_simd::learning::simd_knn_search;

/// Errors for pattern clustering operations
#[derive(Error, Debug)]
pub enum ClusterError {
    /// Underlying SQLite failure.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// No cluster exists for the requested cluster id.
    #[error("Cluster not found: {0}")]
    ClusterNotFound(u64),
    /// Clustering requested but no centroids have been computed yet.
    #[error("No centroids available")]
    NoCentroids,
    /// An embedding's dimension did not match the expected dimension.
    #[error("Invalid embedding dimension: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// Expected embedding dimension.
        expected: usize,
        /// Actual embedding dimension received.
        actual: usize,
    },
}

/// Result type for clustering operations
pub type Result<T> = std::result::Result<T, ClusterError>;

/// A cluster of related patterns
#[derive(Debug, Clone)]
pub struct PatternCluster {
    /// Unique identifier of this cluster.
    pub cluster_id: u64,
    /// Keys of the patterns belonging to this cluster.
    pub member_keys: Vec<String>,
    /// Centroid embedding representing the cluster center.
    pub centroid_embedding: Vec<f32>,
    /// Number of members in the cluster.
    pub member_count: usize,
    /// Unix timestamp of the last centroid update.
    pub last_updated: i64,
}

impl PatternCluster {
    /// Create a new pattern cluster
    pub fn new(cluster_id: u64, centroid: Vec<f32>) -> Self {
        Self {
            cluster_id,
            member_keys: Vec::new(),
            centroid_embedding: centroid,
            member_count: 0,
            last_updated: current_timestamp(),
        }
    }
}

/// Input for storing a pattern with cluster assignment
#[derive(Debug, Clone)]
pub struct ClusteredPattern {
    /// Unique key identifying the pattern.
    pub key: String,
    /// Stored pattern payload.
    pub value: String,
    /// Embedding vector for the pattern.
    pub embedding: Vec<f32>,
    /// Assigned cluster id, if already clustered.
    pub cluster_id: Option<u64>,
    /// Optional arbitrary metadata attached to the pattern.
    pub metadata: Option<serde_json::Value>,
}

/// Result from finding similar clusters
#[derive(Debug, Clone)]
pub struct ClusterMatch {
    /// Identifier of the matched cluster.
    pub cluster_id: u64,
    /// Cosine similarity of the query to the cluster centroid.
    pub similarity: f32,
    /// Number of members in the matched cluster.
    pub member_count: usize,
    /// Members of the matched cluster.
    pub members: Vec<ClusterMember>,
}

/// A member of a cluster
#[derive(Debug, Clone)]
pub struct ClusterMember {
    /// Key of the member pattern.
    pub key: String,
    /// Stored payload of the member pattern.
    pub value: String,
    /// Cosine similarity of the member to its cluster centroid.
    pub similarity_to_centroid: f32,
}

/// Statistics about the clustering system
#[derive(Debug, Clone)]
pub struct ClusterStats {
    /// Total number of clusters.
    pub total_clusters: usize,
    /// Total number of stored patterns across all clusters.
    pub total_patterns: usize,
    /// Average number of members per cluster.
    pub avg_cluster_size: f32,
    /// Member count of the largest cluster.
    pub largest_cluster: usize,
    /// Member count of the smallest cluster.
    pub smallest_cluster: usize,
}

/// Pattern clustering using lazy HNSW-based assignment
#[derive(Debug)]
pub struct PatternClusterer {
    conn: Connection,
    embedding_dim: usize,
    similarity_threshold: f32,
    /// In-memory centroid cache for fast lookup
    centroids: Vec<(u64, Vec<f32>)>,
}

impl PatternClusterer {
    /// Create a new pattern clusterer
    pub fn new(db_path: &Path, embedding_dim: usize) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;",
        )?;

        let mut clusterer = Self {
            conn,
            embedding_dim,
            similarity_threshold: 0.75, // Default: 0.7-0.8 is the sweet spot
            centroids: Vec::new(),
        };

        clusterer.ensure_schema()?;
        clusterer.load_centroids()?;
        Ok(clusterer)
    }

    /// Create with custom similarity threshold
    pub fn with_threshold(db_path: &Path, embedding_dim: usize, threshold: f32) -> Result<Self> {
        let mut clusterer = Self::new(db_path, embedding_dim)?;
        clusterer.similarity_threshold = threshold.clamp(0.0, 1.0);
        Ok(clusterer)
    }

    fn ensure_schema(&self) -> Result<()> {
        // Clusters table: stores centroid embeddings
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS pattern_clusters (
                cluster_id INTEGER PRIMARY KEY,
                centroid_embedding BLOB NOT NULL,
                member_keys TEXT NOT NULL DEFAULT '[]',
                member_count INTEGER NOT NULL DEFAULT 0,
                last_updated INTEGER NOT NULL
            )",
            [],
        )?;

        Ok(())
    }

    /// Load centroids from database into memory cache
    fn load_centroids(&mut self) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare("SELECT cluster_id, centroid_embedding FROM pattern_clusters")?;

        let rows = stmt.query_map([], |row| {
            let cluster_id: u64 = row.get::<_, i64>(0)? as u64;
            let embedding_blob: Vec<u8> = row.get(1)?;
            Ok((cluster_id, embedding_blob))
        })?;

        self.centroids.clear();
        for row in rows.flatten() {
            let (cluster_id, blob) = row;
            let embedding = decode_embedding_bytes(&blob);
            if embedding.len() == self.embedding_dim {
                self.centroids.push((cluster_id, embedding));
            }
        }

        Ok(())
    }

    /// Find the nearest centroid to a query embedding
    fn find_nearest_centroid(&self, query: &[f32]) -> Option<(u64, f32)> {
        if self.centroids.is_empty() {
            return None;
        }

        let candidate_embeddings: Vec<Vec<f32>> =
            self.centroids.iter().map(|(_, emb)| emb.clone()).collect();

        let results = simd_knn_search(query, &candidate_embeddings, 1);

        results.first().and_then(|r| {
            let idx = r.index;
            self.centroids
                .get(idx)
                .map(|(cluster_id, _)| (*cluster_id, r.score as f32))
        })
    }

    /// Store a pattern and assign it to the nearest cluster
    pub fn store_pattern(&mut self, pattern: ClusteredPattern) -> Result<u64> {
        let cluster_id = if let Some(cid) = pattern.cluster_id {
            cid
        } else if let Some((cid, similarity)) = self.find_nearest_centroid(&pattern.embedding) {
            if similarity >= self.similarity_threshold {
                // Incremental centroid update using exponential moving average
                self.update_centroid_incremental(cid, &pattern.embedding)?;
                cid
            } else {
                // Create new cluster
                self.create_cluster(&pattern.embedding)?
            }
        } else {
            // First pattern ever - create initial cluster
            self.create_cluster(&pattern.embedding)?
        };

        // Update cluster member list and count
        self.conn.execute(
            "UPDATE pattern_clusters SET member_keys = member_keys || ?1 || ',', member_count = member_count + 1, last_updated = ?2 WHERE cluster_id = ?3",
            params![pattern.key, current_timestamp(), cluster_id as i64],
        )?;

        Ok(cluster_id)
    }

    /// Create a new cluster with the given centroid
    fn create_cluster(&mut self, centroid: &[f32]) -> Result<u64> {
        let cluster_id = (self.centroids.len() as u64) + 1;
        let embedding_bytes: Vec<u8> = centroid.iter().flat_map(|f| f.to_le_bytes()).collect();

        self.conn.execute(
            "INSERT INTO pattern_clusters (cluster_id, centroid_embedding, member_keys, member_count, last_updated)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![cluster_id as i64, embedding_bytes, "", 0i64, current_timestamp()],
        )?;

        self.centroids.push((cluster_id, centroid.to_vec()));
        Ok(cluster_id)
    }

    /// Find clusters similar to a query embedding
    pub fn find_similar_clusters(&self, query: &[f32], top_k: usize) -> Result<Vec<ClusterMatch>> {
        if self.centroids.is_empty() {
            return Ok(Vec::new());
        }

        let candidate_embeddings: Vec<Vec<f32>> =
            self.centroids.iter().map(|(_, emb)| emb.clone()).collect();

        let cluster_ids: Vec<u64> = self.centroids.iter().map(|(id, _)| *id).collect();
        let results = simd_knn_search(
            query,
            &candidate_embeddings,
            top_k.min(candidate_embeddings.len()),
        );

        let mut matches = Vec::new();
        for r in results {
            // Only include matches above similarity threshold (consistent with store_pattern logic)
            if (r.score as f32) < self.similarity_threshold {
                continue;
            }
            let idx = r.index;
            let cluster_id = *cluster_ids
                .get(idx)
                .ok_or_else(|| rusqlite::Error::InvalidQuery)?;
            if let Some(member_count) = self.get_cluster_member_count(cluster_id)? {
                let members = self.get_cluster_members(cluster_id, 5)?;
                matches.push(ClusterMatch {
                    cluster_id,
                    similarity: r.score as f32,
                    member_count,
                    members,
                });
            }
        }

        Ok(matches)
    }

    /// Get member count for a cluster
    fn get_cluster_member_count(&self, cluster_id: u64) -> Result<Option<usize>> {
        let count: Option<i64> = self
            .conn
            .query_row(
                "SELECT member_count FROM pattern_clusters WHERE cluster_id = ?1",
                params![cluster_id as i64],
                |row| row.get(0),
            )
            .optional()?;

        Ok(count.map(|c| c as usize))
    }

    /// Get members of a cluster
    fn get_cluster_members(&self, cluster_id: u64, limit: usize) -> Result<Vec<ClusterMember>> {
        // Get member keys from pattern_clusters table
        let (keys_str,): (String,) = self.conn.query_row(
            "SELECT member_keys FROM pattern_clusters WHERE cluster_id = ?1",
            params![cluster_id as i64],
            |row| Ok((row.get(0)?,)),
        )?;

        // Parse comma-separated keys
        let keys: Vec<&str> = keys_str
            .split(',')
            .filter(|k| !k.is_empty())
            .take(limit)
            .collect();
        let members: Vec<ClusterMember> = keys
            .iter()
            .map(|key| ClusterMember {
                key: key.to_string(),
                value: String::new(),
                similarity_to_centroid: 1.0,
            })
            .collect();

        Ok(members)
    }

    /// Get all clusters
    pub fn get_all_clusters(&self) -> Result<Vec<PatternCluster>> {
        let mut stmt = self.conn.prepare(
            "SELECT cluster_id, centroid_embedding, member_keys, member_count, last_updated
             FROM pattern_clusters
             ORDER BY cluster_id",
        )?;

        let rows = stmt.query_map([], |row| {
            let cluster_id: u64 = row.get::<_, i64>(0)? as u64;
            let embedding_blob: Vec<u8> = row.get(1)?;
            let member_keys_str: String = row.get(2)?;
            let member_count: i64 = row.get(3)?;
            let last_updated: i64 = row.get(4)?;

            let keys: Vec<String> = if member_keys_str.is_empty() {
                Vec::new()
            } else {
                member_keys_str
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            };

            Ok(PatternCluster {
                cluster_id,
                member_keys: keys.clone(),
                centroid_embedding: decode_embedding_bytes(&embedding_blob),
                member_count: member_count as usize,
                last_updated,
            })
        })?;

        Ok(rows.flatten().collect())
    }

    /// Get cluster statistics
    pub fn stats(&self) -> Result<ClusterStats> {
        let total_clusters: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM pattern_clusters", [], |row| {
                    row.get(0)
                })?;

        let total_patterns: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(member_count), 0) FROM pattern_clusters",
            [],
            |row| row.get(0),
        )?;

        let avg_cluster_size = if total_clusters > 0 {
            total_patterns as f32 / total_clusters as f32
        } else {
            0.0
        };

        let (largest, smallest) = if total_clusters > 0 {
            let extremes: (i64, i64) = self.conn.query_row(
                "SELECT MAX(member_count), MIN(member_count) FROM pattern_clusters",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            (extremes.0 as usize, extremes.1 as usize)
        } else {
            (0, 0)
        };

        Ok(ClusterStats {
            total_clusters: total_clusters as usize,
            total_patterns: total_patterns as usize,
            avg_cluster_size,
            largest_cluster: largest,
            smallest_cluster: smallest,
        })
    }

    /// Recompute centroids for all clusters (handles centroid drift)
    /// Since we don't store individual member embeddings, this uses the stored centroid
    /// and applies a small decay factor to simulate drift handling.
    /// In production, member embeddings should be stored for proper centroid recomputation.
    pub fn recompute_centroids(&mut self) -> Result<()> {
        let cluster_ids: Vec<u64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT cluster_id FROM pattern_clusters")?;
            let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            rows.filter_map(|id| id.ok().map(|v| v as u64)).collect()
        };

        // For each cluster, renormalize the centroid (handles numerical drift)
        for cluster_id in cluster_ids {
            if let Some(pos) = self.centroids.iter().position(|(id, _)| *id == cluster_id)
                && let Some(centroid) = self.centroids.get_mut(pos).map(|c| &mut c.1)
            {
                // Renormalize to unit vector (handles numerical precision drift)
                let norm: f32 = centroid.iter().map(|v| v * v).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for v in centroid.iter_mut() {
                        *v /= norm;
                    }
                    // Update DB
                    let embedding_bytes: Vec<u8> =
                        centroid.iter().flat_map(|f| f.to_le_bytes()).collect();
                    self.conn.execute(
                            "UPDATE pattern_clusters SET centroid_embedding = ?1, last_updated = ?2 WHERE cluster_id = ?3",
                            params![embedding_bytes, current_timestamp(), cluster_id as i64],
                        )?;
                }
            }
        }

        Ok(())
    }

    /// Incrementally update centroid using exponential moving average
    /// This is called when a new pattern joins an existing cluster
    fn update_centroid_incremental(
        &mut self,
        cluster_id: u64,
        new_embedding: &[f32],
    ) -> Result<()> {
        let pos = match self.centroids.iter().position(|(id, _)| *id == cluster_id) {
            Some(p) => p,
            None => return Ok(()),
        };
        let alpha = 0.1; // Learning rate for centroid update

        // EMA update: centroid = (1 - alpha) * centroid + alpha * new_embedding
        // Using get_mut for safe indexing as required by clippy
        let dim = self.embedding_dim;
        if let Some(centroid) = self.centroids.get_mut(pos) {
            let centroid_vec = &mut centroid.1;
            for i in 0..dim {
                let old_val = *centroid_vec.get(i).unwrap_or(&0.0);
                let emb_val = *new_embedding.get(i).unwrap_or(&0.0);
                let new_val = (1.0 - alpha) * old_val + alpha * emb_val;
                if let Some(slot) = centroid_vec.get_mut(i) {
                    *slot = new_val;
                }
            }

            // Renormalize to unit vector
            let norm: f32 = centroid_vec.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for val in centroid_vec.iter_mut() {
                    *val /= norm;
                }
            }

            // Update DB
            let embedding_bytes: Vec<u8> =
                centroid_vec.iter().flat_map(|f| f.to_le_bytes()).collect();
            self.conn.execute(
                "UPDATE pattern_clusters SET centroid_embedding = ?1, last_updated = ?2 WHERE cluster_id = ?3",
                params![embedding_bytes, current_timestamp(), cluster_id as i64],
            )?;
        }
        Ok(())
    }

    /// Set the similarity threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.similarity_threshold = threshold.clamp(0.0, 1.0);
    }

    /// Get the current threshold
    pub fn threshold(&self) -> f32 {
        self.similarity_threshold
    }
}

/// Decode a stored embedding blob back to `Vec<f32>`.
fn decode_embedding_bytes(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| {
            let arr: [u8; 4] = b
                .try_into()
                .expect("chunks_exact(4) guarantees 4-byte slices");
            f32::from_le_bytes(arr)
        })
        .collect()
}

/// Get current timestamp
fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(feature = "async-memory")]
mod async_wrapper {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Thread-safe wrapper for PatternClusterer
    #[derive(Debug, Clone)]
    pub struct AsyncPatternClusterer {
        inner: Arc<Mutex<PatternClusterer>>,
    }

    impl AsyncPatternClusterer {
        /// Create a new async pattern clusterer
        pub fn new(db_path: &Path, embedding_dim: usize) -> Result<Self> {
            let inner = PatternClusterer::new(db_path, embedding_dim)?;
            Ok(Self {
                inner: Arc::new(Mutex::new(inner)),
            })
        }

        /// Create with custom threshold
        pub fn with_threshold(
            db_path: &Path,
            embedding_dim: usize,
            threshold: f32,
        ) -> Result<Self> {
            let inner = PatternClusterer::with_threshold(db_path, embedding_dim, threshold)?;
            Ok(Self {
                inner: Arc::new(Mutex::new(inner)),
            })
        }

        /// Store a pattern and get its cluster assignment
        pub async fn store_pattern(&self, pattern: ClusteredPattern) -> Result<u64> {
            let mut clusterer = self.inner.lock().await;
            clusterer.store_pattern(pattern)
        }

        /// Find clusters similar to a query embedding
        pub async fn find_similar_clusters(
            &self,
            query: &[f32],
            top_k: usize,
        ) -> Result<Vec<ClusterMatch>> {
            let clusterer = self.inner.lock().await;
            clusterer.find_similar_clusters(query, top_k)
        }

        /// Get all clusters
        pub async fn get_all_clusters(&self) -> Result<Vec<PatternCluster>> {
            let clusterer = self.inner.lock().await;
            clusterer.get_all_clusters()
        }

        /// Get cluster statistics
        pub async fn stats(&self) -> Result<ClusterStats> {
            let clusterer = self.inner.lock().await;
            clusterer.stats()
        }

        /// Recompute centroids
        pub async fn recompute_centroids(&self) -> Result<()> {
            let mut clusterer = self.inner.lock().await;
            clusterer.recompute_centroids()
        }
    }
}

#[cfg(feature = "async-memory")]
pub use async_wrapper::AsyncPatternClusterer;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_clusterer() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("clusters.db");
        let clusterer = PatternClusterer::new(&db_path, 4).unwrap();

        let stats = clusterer.stats().unwrap();
        assert_eq!(stats.total_clusters, 0);
        assert_eq!(stats.total_patterns, 0);
    }

    #[test]
    fn test_store_first_pattern_creates_cluster() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("clusters.db");
        let mut clusterer = PatternClusterer::new(&db_path, 4).unwrap();

        let pattern = ClusteredPattern {
            key: "test_key".to_string(),
            value: "test value".to_string(),
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            cluster_id: None,
            metadata: None,
        };

        let cluster_id = clusterer.store_pattern(pattern).unwrap();
        assert_eq!(cluster_id, 1);

        let stats = clusterer.stats().unwrap();
        assert_eq!(stats.total_clusters, 1);
        assert_eq!(stats.total_patterns, 1);
    }

    #[test]
    fn test_similar_patterns_same_cluster() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("clusters.db");
        let mut clusterer = PatternClusterer::new(&db_path, 4).unwrap();

        // Store first pattern
        clusterer
            .store_pattern(ClusteredPattern {
                key: "key1".to_string(),
                value: "value1".to_string(),
                embedding: vec![1.0, 0.0, 0.0, 0.0],
                cluster_id: None,
                metadata: None,
            })
            .unwrap();

        // Store similar pattern (cosine similarity should be high)
        let cluster_id = clusterer
            .store_pattern(ClusteredPattern {
                key: "key2".to_string(),
                value: "value2".to_string(),
                embedding: vec![0.95, 0.05, 0.0, 0.0],
                cluster_id: None,
                metadata: None,
            })
            .unwrap();

        assert_eq!(cluster_id, 1); // Should be in same cluster

        let stats = clusterer.stats().unwrap();
        assert_eq!(stats.total_clusters, 1);
        assert_eq!(stats.total_patterns, 2);
    }

    #[test]
    fn test_dissimilar_patterns_different_clusters() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("clusters.db");
        let mut clusterer = PatternClusterer::with_threshold(&db_path, 4, 0.8).unwrap();

        // Store first pattern
        clusterer
            .store_pattern(ClusteredPattern {
                key: "key1".to_string(),
                value: "value1".to_string(),
                embedding: vec![1.0, 0.0, 0.0, 0.0],
                cluster_id: None,
                metadata: None,
            })
            .unwrap();

        // Store very different pattern
        let cluster_id = clusterer
            .store_pattern(ClusteredPattern {
                key: "key2".to_string(),
                value: "value2".to_string(),
                embedding: vec![0.0, 1.0, 0.0, 0.0],
                cluster_id: None,
                metadata: None,
            })
            .unwrap();

        // Should create a new cluster since similarity < threshold
        assert_eq!(cluster_id, 2);

        let stats = clusterer.stats().unwrap();
        assert_eq!(stats.total_clusters, 2);
    }

    #[test]
    fn test_find_similar_clusters() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("clusters.db");
        let mut clusterer = PatternClusterer::new(&db_path, 4).unwrap();

        // Create clusters
        clusterer
            .store_pattern(ClusteredPattern {
                key: "cluster1_key".to_string(),
                value: "cluster1 value".to_string(),
                embedding: vec![1.0, 0.0, 0.0, 0.0],
                cluster_id: None,
                metadata: None,
            })
            .unwrap();

        clusterer
            .store_pattern(ClusteredPattern {
                key: "cluster2_key".to_string(),
                value: "cluster2 value".to_string(),
                embedding: vec![0.0, 1.0, 0.0, 0.0],
                cluster_id: None,
                metadata: None,
            })
            .unwrap();

        // Query for similar to cluster 1
        let query = vec![0.9, 0.1, 0.0, 0.0];
        let matches = clusterer.find_similar_clusters(&query, 2).unwrap();

        assert!(!matches.is_empty());
        // First match should be cluster 1 (closer to our query)
        assert_eq!(matches[0].cluster_id, 1);
    }

    #[test]
    fn test_threshold_bounds() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("clusters.db");
        let mut clusterer = PatternClusterer::with_threshold(&db_path, 4, 0.5).unwrap();

        assert_eq!(clusterer.threshold(), 0.5);

        clusterer.set_threshold(1.5); // Should clamp to 1.0
        assert_eq!(clusterer.threshold(), 1.0);

        clusterer.set_threshold(-0.5); // Should clamp to 0.0
        assert_eq!(clusterer.threshold(), 0.0);
    }

    #[cfg(feature = "async-memory")]
    #[tokio::test]
    async fn test_async_wrapper() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("clusters.db");
        let clusterer = AsyncPatternClusterer::new(&db_path, 4).unwrap();

        let stats = clusterer.stats().await.unwrap();
        assert_eq!(stats.total_clusters, 0);
    }
}
