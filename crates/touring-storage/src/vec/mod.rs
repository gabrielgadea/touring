//! Vector store abstraction for Touring — pluggable backends (sqlite-vec, Qdrant, InMemory).

pub mod backends;
mod error;

pub use error::VectorStoreError;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A single point in a vector collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    /// Unique identifier for the point.
    pub id: String,
    /// The embedding vector.
    pub vector: Vec<f32>,
    /// Arbitrary metadata associated with the point.
    pub metadata: serde_json::Value,
}

/// A search hit returned from a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// Point identifier.
    pub id: String,
    /// Similarity score.
    pub score: f32,
    /// Point metadata (only populated if `with_metadata` is true).
    pub metadata: serde_json::Value,
}

/// A search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Query vector.
    pub vector: Vec<f32>,
    /// Number of results to return.
    pub top_k: usize,
    /// Whether to include metadata in results.
    pub with_metadata: bool,
    /// Optional JSON filter predicate.
    pub filter: Option<serde_json::Value>,
}

/// Distance metric used to measure similarity between vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistanceMetric {
    /// Cosine similarity — measures the angle between vectors, ignoring magnitude.
    Cosine,
    /// Euclidean (L2) distance — straight-line distance between vectors.
    Euclidean,
    /// Dot product — inner product, sensitive to both angle and magnitude.
    DotProduct,
}

/// Schema definition for a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSchema {
    /// Unique name of the collection.
    pub name: String,
    /// Dimensionality (length) of every vector stored in the collection.
    pub dimension: usize,
    /// Distance metric used when searching the collection.
    pub distance: DistanceMetric,
}

/// In-memory collection state.
struct InMemoryCollection {
    schema: CollectionSchema,
    points: HashMap<String, Point>,
}

impl InMemoryCollection {
    fn new(schema: CollectionSchema) -> Self {
        Self {
            schema,
            points: HashMap::new(),
        }
    }
}

/// In-memory vector store backend.
#[derive(Clone)]
pub struct InMemoryVectorStore {
    collections: Arc<RwLock<HashMap<String, InMemoryCollection>>>,
}

impl InMemoryVectorStore {
    /// Create a new in-memory store.
    pub fn new() -> Self {
        Self {
            collections: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Compute Euclidean distance (returned as negative similarity for ranking).
fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    let sum: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum();
    -sum.sqrt()
}

/// Compute dot product.
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn collection_exists(&self, name: &str) -> Result<bool, VectorStoreError> {
        let collections = self.collections.read().await;
        Ok(collections.contains_key(name))
    }

    async fn create_collection(&self, schema: CollectionSchema) -> Result<(), VectorStoreError> {
        let mut collections = self.collections.write().await;
        if collections.contains_key(&schema.name) {
            return Err(VectorStoreError::UpsertFailed(format!(
                "collection '{}' already exists",
                schema.name
            )));
        }
        collections.insert(schema.name.clone(), InMemoryCollection::new(schema));
        Ok(())
    }

    async fn delete_collection(&self, name: &str) -> Result<(), VectorStoreError> {
        let mut collections = self.collections.write().await;
        collections
            .remove(name)
            .ok_or(VectorStoreError::CollectionNotFound(name.to_string()))?;
        Ok(())
    }

    async fn upsert(
        &self,
        collection_name: &str,
        points: Vec<Point>,
    ) -> Result<(), VectorStoreError> {
        let dim =
            {
                let collections = self.collections.read().await;
                let collection = collections.get(collection_name).ok_or(
                    VectorStoreError::CollectionNotFound(collection_name.to_string()),
                )?;
                collection.schema.dimension
            };

        let mut collections = self.collections.write().await;
        let collection =
            collections
                .get_mut(collection_name)
                .ok_or(VectorStoreError::CollectionNotFound(
                    collection_name.to_string(),
                ))?;

        for point in points {
            if point.vector.len() != dim {
                return Err(VectorStoreError::InvalidDimension {
                    expected: dim,
                    actual: point.vector.len(),
                });
            }
            collection.points.insert(point.id.clone(), point);
        }
        Ok(())
    }

    async fn search(
        &self,
        collection_name: &str,
        query: SearchQuery,
    ) -> Result<Vec<SearchHit>, VectorStoreError> {
        let (dim, distance) =
            {
                let collections = self.collections.read().await;
                let collection = collections.get(collection_name).ok_or(
                    VectorStoreError::CollectionNotFound(collection_name.to_string()),
                )?;
                (collection.schema.dimension, collection.schema.distance)
            };

        if query.vector.len() != dim {
            return Err(VectorStoreError::InvalidDimension {
                expected: dim,
                actual: query.vector.len(),
            });
        }

        let collections = self.collections.read().await;
        let collection =
            collections
                .get(collection_name)
                .ok_or(VectorStoreError::CollectionNotFound(
                    collection_name.to_string(),
                ))?;

        let mut hits: Vec<SearchHit> = collection
            .points
            .values()
            .map(|point| {
                let score = match distance {
                    DistanceMetric::Cosine => cosine_similarity(&query.vector, &point.vector),
                    DistanceMetric::Euclidean => euclidean_distance(&query.vector, &point.vector),
                    DistanceMetric::DotProduct => dot_product(&query.vector, &point.vector),
                };
                SearchHit {
                    id: point.id.clone(),
                    score,
                    metadata: if query.with_metadata {
                        point.metadata.clone()
                    } else {
                        serde_json::Value::Null
                    },
                }
            })
            .collect();

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(query.top_k);
        Ok(hits)
    }

    async fn delete(
        &self,
        collection_name: &str,
        ids: Vec<String>,
    ) -> Result<(), VectorStoreError> {
        {
            let collections = self.collections.read().await;
            if !collections.contains_key(collection_name) {
                return Err(VectorStoreError::CollectionNotFound(
                    collection_name.to_string(),
                ));
            }
        }

        let mut collections = self.collections.write().await;
        let collection =
            collections
                .get_mut(collection_name)
                .ok_or(VectorStoreError::CollectionNotFound(
                    collection_name.to_string(),
                ))?;

        for id in ids {
            collection.points.remove(&id);
        }
        Ok(())
    }
}

/// Unified vector store trait — implemented by all backends.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Check if a collection exists.
    async fn collection_exists(&self, name: &str) -> Result<bool, VectorStoreError>;

    /// Create a new collection.
    async fn create_collection(&self, schema: CollectionSchema) -> Result<(), VectorStoreError>;

    /// Delete a collection and all its points.
    async fn delete_collection(&self, name: &str) -> Result<(), VectorStoreError>;

    /// Insert or update points in a collection.
    async fn upsert(&self, collection: &str, points: Vec<Point>) -> Result<(), VectorStoreError>;

    /// Search for similar points.
    async fn search(
        &self,
        collection: &str,
        query: SearchQuery,
    ) -> Result<Vec<SearchHit>, VectorStoreError>;

    /// Delete points by ID.
    async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<(), VectorStoreError>;
}

/// Construct a backend from the `TOURING_VECTOR_BACKEND` environment variable.
///
/// Supported values:
/// - `"in-memory"` (default) — in-process HashMap backend
/// - `"sqlite-vec"` — requires the `sqlite-vec` feature
/// - `"qdrant"` — requires the `qdrant` feature
pub fn backend_from_env() -> Result<Arc<dyn VectorStore>, VectorStoreError> {
    let backend =
        std::env::var("TOURING_VECTOR_BACKEND").unwrap_or_else(|_| "in-memory".to_string());

    match backend.as_str() {
        "in-memory" => Ok(Arc::new(InMemoryVectorStore::new()) as Arc<dyn VectorStore>),
        #[cfg(feature = "sqlite-vec")]
        "sqlite-vec" => {
            use crate::vec::backends::sqlite_vec::SqliteVecStore;
            let db_path = std::env::var("TOURING_VECTOR_DB_PATH")
                .unwrap_or_else(|_| "touring_vectors.db".to_string());
            Ok(Arc::new(
                SqliteVecStore::new(&db_path)
                    .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?,
            ) as Arc<dyn VectorStore>)
        }
        #[cfg(feature = "qdrant")]
        "qdrant" => {
            use crate::vec::backends::qdrant::QdrantStore;
            // qdrant-client speaks gRPC, which Qdrant serves on port 6334;
            // 6333 is the REST port and cannot accept a gRPC connection.
            let url = std::env::var("TOURING_QDRANT_URL")
                .unwrap_or_else(|_| "http://localhost:6334".to_string());
            Ok(Arc::new(
                QdrantStore::new(&url)
                    .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?,
            ) as Arc<dyn VectorStore>)
        }
        _ => Err(VectorStoreError::BackendNotAvailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_basic() {
        let store = InMemoryVectorStore::new();

        let schema = CollectionSchema {
            name: "test".to_string(),
            dimension: 3,
            distance: DistanceMetric::Cosine,
        };
        store.create_collection(schema.clone()).await.unwrap();
        store.collection_exists("test").await.unwrap();

        store
            .upsert(
                "test",
                vec![Point {
                    id: "1".to_string(),
                    vector: vec![0.1, 0.2, 0.3],
                    metadata: serde_json::json!("{}"),
                }],
            )
            .await
            .unwrap();

        let hits = store
            .search(
                "test",
                SearchQuery {
                    vector: vec![0.1, 0.2, 0.3],
                    top_k: 3,
                    with_metadata: false,
                    filter: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "1");
        assert!(hits[0].score > 0.99);

        store.delete("test", vec!["1".to_string()]).await.unwrap();
        let hits = store
            .search(
                "test",
                SearchQuery {
                    vector: vec![0.1, 0.2, 0.3],
                    top_k: 3,
                    with_metadata: false,
                    filter: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 0);
    }

    #[tokio::test]
    async fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 1e-6);
    }
}
