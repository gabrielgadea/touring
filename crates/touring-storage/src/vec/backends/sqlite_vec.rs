//! SQLite-backed vector store using the sqlite-vec schema.
//!
//! Uses rusqlite for persistence with the same table schema as the sqlite-vec
//! C extension: `vec_<collection>(id TEXT PRIMARY KEY, vector BLOB, metadata JSON)`.

use async_trait::async_trait;
use rusqlite::{Connection, params};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::super::VectorStoreError;
use super::super::{CollectionSchema, Point, SearchHit, SearchQuery, VectorStore};

/// SQLite-backed vector store.
///
/// Wraps an `Arc<Mutex<Connection>>` so the `Connection` can be safely
/// shared across async tasks. All sync SQLite operations are dispatched via
/// `tokio::task::spawn_blocking`.
#[derive(Clone)]
pub struct SqliteVecStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteVecStore {
    /// Open (or create) a SQLite database at `db_path`.
    pub fn new(db_path: &str) -> Result<Self, VectorStoreError> {
        let conn = Connection::open(db_path)
            .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn table_name(collection: &str) -> String {
        format!("vec_{}", collection)
    }

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

    fn vector_to_blob(vector: &[f32]) -> Vec<u8> {
        let mut blob = Vec::with_capacity(vector.len() * 4);
        for &v in vector {
            blob.extend_from_slice(&v.to_le_bytes());
        }
        blob
    }

    fn blob_to_vector(blob: &[u8]) -> Vec<f32> {
        debug_assert!(blob.len() % 4 == 0);
        blob.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    // ------------------------------------------------------------------
    // Sync functions — must be called from within spawn_blocking.
    // ------------------------------------------------------------------

    fn sync_collection_exists(conn: &Connection, name: &str) -> Result<bool, VectorStoreError> {
        let table = Self::table_name(name);
        let count: i32 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name = ?",
                params![table],
                |row| row.get(0),
            )
            .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?;
        Ok(count > 0)
    }

    fn sync_create_collection(
        conn: &Connection,
        schema: &CollectionSchema,
    ) -> Result<(), VectorStoreError> {
        let table = Self::table_name(&schema.name);
        let ddl = format!(
            "CREATE TABLE IF NOT EXISTS {} (id TEXT PRIMARY KEY, vector BLOB, metadata JSON)",
            table
        );
        conn.execute(&ddl, [])
            .map_err(|e| VectorStoreError::UpsertFailed(e.to_string()))?;
        Ok(())
    }

    fn sync_delete_collection(conn: &Connection, name: &str) -> Result<(), VectorStoreError> {
        let table = Self::table_name(name);
        conn.execute(&format!("DROP TABLE IF EXISTS {}", table), [])
            .map_err(|e| VectorStoreError::DeleteFailed(e.to_string()))?;
        Ok(())
    }

    /// Upsert points into the collection using `INSERT OR REPLACE`.
    ///
    /// This is atomic and idempotent: if the point already exists, the old row is
    /// replaced entirely. No prior read is required, eliminating a race window.
    fn sync_upsert(
        conn: &Connection,
        collection: &str,
        points: Vec<Point>,
    ) -> Result<(), VectorStoreError> {
        let table = Self::table_name(collection);
        let sql = format!(
            "INSERT OR REPLACE INTO {} (id, vector, metadata) VALUES (?, ?, ?)",
            table
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| VectorStoreError::UpsertFailed(e.to_string()))?;
        for point in points {
            let blob = Self::vector_to_blob(&point.vector);
            let metadata = serde_json::to_string(&point.metadata)
                .map_err(|e| VectorStoreError::UpsertFailed(e.to_string()))?;
            stmt.execute(params![point.id, blob, metadata])
                .map_err(|e| VectorStoreError::UpsertFailed(e.to_string()))?;
        }
        Ok(())
    }

    fn sync_delete(
        conn: &Connection,
        collection: &str,
        ids: Vec<String>,
    ) -> Result<(), VectorStoreError> {
        let table = Self::table_name(collection);
        for id in ids {
            conn.execute(&format!("DELETE FROM {} WHERE id = ?", table), params![id])
                .map_err(|e| VectorStoreError::DeleteFailed(e.to_string()))?;
        }
        Ok(())
    }

    fn sync_search(
        conn: &Connection,
        collection: &str,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<(String, f32, serde_json::Value)>, VectorStoreError> {
        let table = Self::table_name(collection);
        let sql = format!("SELECT id, vector, metadata FROM {}", table);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| VectorStoreError::SearchFailed(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                let blob: Vec<u8> = row.get(1).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Blob,
                        Box::new(e),
                    )
                })?;
                let metadata_json: String = row.get(2).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                let vec = Self::blob_to_vector(&blob);
                let metadata: serde_json::Value =
                    serde_json::from_str(&metadata_json).unwrap_or(serde_json::Value::Null);
                Ok((id, vec, metadata))
            })
            .map_err(|e| VectorStoreError::SearchFailed(e.to_string()))?;
        let mut hits = Vec::new();
        for row in rows {
            let (id, v, metadata) =
                row.map_err(|e| VectorStoreError::SearchFailed(e.to_string()))?;
            let score = Self::cosine_similarity(vector, &v);
            hits.push((id, score, metadata));
        }
        hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(top_k);
        Ok(hits)
    }
}

#[async_trait]
impl VectorStore for SqliteVecStore {
    async fn collection_exists(&self, name: &str) -> Result<bool, VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        let name = name.to_string();
        tokio::task::spawn_blocking(move || {
            let guard = conn.blocking_lock();
            Self::sync_collection_exists(&guard, &name)
        })
        .await
        .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?
    }

    async fn create_collection(&self, schema: CollectionSchema) -> Result<(), VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        let schema = Arc::new(schema);
        let schema_clone = Arc::clone(&schema);
        tokio::task::spawn_blocking(move || {
            let guard = conn.blocking_lock();
            Self::sync_create_collection(&guard, &schema_clone)
        })
        .await
        .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?
    }

    async fn delete_collection(&self, name: &str) -> Result<(), VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        let name = name.to_string();
        tokio::task::spawn_blocking(move || {
            let guard = conn.blocking_lock();
            Self::sync_delete_collection(&guard, &name)
        })
        .await
        .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?
    }

    async fn upsert(
        &self,
        collection_name: &str,
        points: Vec<Point>,
    ) -> Result<(), VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        let collection_name = collection_name.to_string();
        tokio::task::spawn_blocking(move || {
            let guard = conn.blocking_lock();
            Self::sync_upsert(&guard, &collection_name, points)
        })
        .await
        .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?
    }

    async fn search(
        &self,
        collection_name: &str,
        query: SearchQuery,
    ) -> Result<Vec<SearchHit>, VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        let collection_name = collection_name.to_string();
        let vector = query.vector.clone();
        let top_k = query.top_k;
        let with_metadata = query.with_metadata;

        let hits = tokio::task::spawn_blocking(move || {
            let guard = conn.blocking_lock();
            Self::sync_search(&guard, &collection_name, &vector, top_k)
        })
        .await
        .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))??;

        let out = hits
            .into_iter()
            .map(|(id, score, metadata)| SearchHit {
                id,
                score,
                metadata: if with_metadata {
                    metadata
                } else {
                    serde_json::Value::Null
                },
            })
            .collect();
        Ok(out)
    }

    async fn delete(
        &self,
        collection_name: &str,
        ids: Vec<String>,
    ) -> Result<(), VectorStoreError> {
        let conn = Arc::clone(&self.conn);
        let collection_name = collection_name.to_string();
        tokio::task::spawn_blocking(move || {
            let guard = conn.blocking_lock();
            Self::sync_delete(&guard, &collection_name, ids)
        })
        .await
        .map_err(|e| VectorStoreError::ConnectionFailed(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vec::DistanceMetric;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_sqlite_vec_basic() {
        let tmp = NamedTempFile::new().expect("create temp file");
        let store =
            SqliteVecStore::new(tmp.path().to_str().expect("path to str")).expect("open store");

        let schema = CollectionSchema {
            name: "test".to_string(),
            dimension: 3,
            distance: DistanceMetric::Cosine,
        };
        store
            .create_collection(schema.clone())
            .await
            .expect("create collection");
        assert!(
            store
                .collection_exists("test")
                .await
                .expect("collection_exists")
        );

        store
            .upsert(
                "test",
                vec![Point {
                    id: "1".to_string(),
                    vector: vec![0.1, 0.2, 0.3],
                    metadata: serde_json::json!({"a": 1}),
                }],
            )
            .await
            .expect("upsert");

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
            .expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "1");
        assert!(hits[0].score > 0.99);

        store
            .delete("test", vec!["1".to_string()])
            .await
            .expect("delete");
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
            .expect("search after delete");
        assert_eq!(hits.len(), 0);
    }

    #[tokio::test]
    async fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];
        assert!((SqliteVecStore::cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
        assert!((SqliteVecStore::cosine_similarity(&a, &c) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_vector_blob_roundtrip() {
        let v = vec![0.1, 0.2, 0.3, 0.4];
        let blob = SqliteVecStore::vector_to_blob(&v);
        let recovered = SqliteVecStore::blob_to_vector(&blob);
        assert_eq!(v, recovered);
    }
}
