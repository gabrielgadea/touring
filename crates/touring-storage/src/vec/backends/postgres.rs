//! PostgreSQL backend for touring-vector-store using tokio-postgres.
//!
//! Uses a single `touring_vectors` table with schema:
//! ```sql
//! CREATE TABLE IF NOT EXISTS touring_vectors (
//!     collection TEXT NOT NULL,
//!     id TEXT NOT NULL,
//!     vector REAL[] NOT NULL,
//!     metadata JSONB,
//!     PRIMARY KEY (collection, id)
//! );
//! ```

use crate::vec::VectorStoreError;
use crate::vec::{CollectionSchema, Point, SearchHit, SearchQuery, VectorStore};
use async_trait::async_trait;
use std::sync::Arc;
use tokio_postgres::{Client, NoTls};

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

/// Convert raw cosine to a [0, 1] score.
fn cosine_score(normalized: f32) -> f32 {
    (normalized + 1.0) / 2.0
}

/// PostgreSQL backend for vector storage.
///
/// Uses a shared `Client` from tokio-postgres and a single `touring_vectors` table.
/// Each collection is differentiated by the `collection` column.
#[derive(Clone)]
pub struct PostgresBackend {
    client: Arc<Client>,
    table_name: String,
}

impl PostgresBackend {
    /// Create a new PostgresBackend from a DSN (Data Source Name).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let backend = PostgresBackend::new("postgres://user:pass@localhost/touring").await?;
    /// ```
    pub async fn new(dsn: &str) -> Result<Self, VectorStoreError> {
        let (client, connection) = tokio_postgres::connect(dsn, NoTls)
            .await
            .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?;

        // Spawn the connection handler so the client stays alive.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("PostgreSQL connection error: {}", e);
            }
        });

        let backend = Self {
            client: Arc::new(client),
            table_name: "touring_vectors".to_string(),
        };

        backend.ensure_table().await?;

        Ok(backend)
    }

    /// Create a new PostgresBackend with a custom table name.
    pub async fn with_table(dsn: &str, table_name: &str) -> Result<Self, VectorStoreError> {
        let (client, connection) = tokio_postgres::connect(dsn, NoTls)
            .await
            .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("PostgreSQL connection error: {}", e);
            }
        });

        let backend = Self {
            client: Arc::new(client),
            table_name: table_name.to_string(),
        };

        backend.ensure_table().await?;

        Ok(backend)
    }

    /// Ensure the vector table exists.
    async fn ensure_table(&self) -> Result<(), VectorStoreError> {
        let ddl = format!(
            "CREATE TABLE IF NOT EXISTS {} (\
                collection TEXT NOT NULL,\
                id TEXT NOT NULL,\
                vector REAL[] NOT NULL,\
                metadata JSONB,\
                PRIMARY KEY (collection, id)\
            )",
            self.table_name
        );

        self.client
            .execute(&ddl, &[])
            .await
            .map_err(|e| VectorStoreError::PersistenceError(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl VectorStore for PostgresBackend {
    async fn collection_exists(&self, name: &str) -> Result<bool, VectorStoreError> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "SELECT 1 FROM {} WHERE collection = $1 LIMIT 1",
                    self.table_name
                ),
                &[&name],
            )
            .await
            .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn create_collection(&self, _schema: CollectionSchema) -> Result<(), VectorStoreError> {
        // tokio-postgres stores dimension per-vector; no pre-creation needed.
        self.ensure_table().await?;
        Ok(())
    }

    async fn delete_collection(&self, name: &str) -> Result<(), VectorStoreError> {
        self.client
            .execute(
                &format!("DELETE FROM {} WHERE collection = $1", self.table_name),
                &[&name],
            )
            .await
            .map_err(|e| VectorStoreError::DeleteFailed(e.to_string()))?;

        Ok(())
    }

    async fn upsert(
        &self,
        collection_name: &str,
        points: Vec<Point>,
    ) -> Result<(), VectorStoreError> {
        for point in points {
            let metadata = serde_json::to_value(&point.metadata)
                .map_err(|e| VectorStoreError::UpsertFailed(e.to_string()))?;

            self.client
                .execute(
                    &format!(
                        "INSERT INTO {} (collection, id, vector, metadata)\
                        VALUES ($1, $2, $3, $4)\
                        ON CONFLICT (collection, id) DO UPDATE SET\
                        vector = EXCLUDED.vector, metadata = EXCLUDED.metadata",
                        self.table_name
                    ),
                    &[&collection_name, &point.id, &point.vector, &metadata],
                )
                .await
                .map_err(|e| VectorStoreError::UpsertFailed(e.to_string()))?;
        }

        Ok(())
    }

    async fn search(
        &self,
        collection_name: &str,
        query: SearchQuery,
    ) -> Result<Vec<SearchHit>, VectorStoreError> {
        let rows = self
            .client
            .query(
                &format!(
                    "SELECT id, vector, metadata FROM {} WHERE collection = $1",
                    self.table_name
                ),
                &[&collection_name],
            )
            .await
            .map_err(|e| VectorStoreError::SearchFailed(e.to_string()))?;

        let mut hits: Vec<SearchHit> = rows
            .iter()
            .map(|row| {
                let vector: Vec<f32> = row.get("vector");
                let sim = cosine_similarity(&query.vector, &vector);
                let metadata: serde_json::Value = row.get("metadata");
                SearchHit {
                    id: row.get("id"),
                    score: cosine_score(sim),
                    metadata: if query.with_metadata {
                        metadata
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
        for id in ids {
            self.client
                .execute(
                    &format!(
                        "DELETE FROM {} WHERE collection = $1 AND id = $2",
                        self.table_name
                    ),
                    &[&collection_name, &id],
                )
                .await
                .map_err(|e| VectorStoreError::DeleteFailed(e.to_string()))?;
        }

        Ok(())
    }
}
