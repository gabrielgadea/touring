//! Session persistence — async SQLite storage for GoT snapshots via deadpool-sqlite.
//!
//! [`SessionPersistence`] manages a pool of SQLite connections for saving and
//! loading [`GoTSnapshot`] data. Snapshots are stored as rkyv-serialized BLOBs
//! with session metadata (timestamps, session_id).
//!
//! # Usage
//!
//! ```ignore
//! let sp = SessionPersistence::new("/tmp/got_sessions.db").await?;
//! sp.save_snapshot("my-session", &snapshot).await?;
//! let loaded = sp.load_snapshot("my-session").await?;
//! ```

use crate::reasoning::snapshot::{GoTSnapshot, GoTSnapshotError};

/// Async session persistence backed by a deadpool-sqlite connection pool.
///
/// Stores GoT snapshots as rkyv-serialized BLOBs in a SQLite table.
/// Uses upsert semantics: saving to an existing session_id overwrites it.
pub struct SessionPersistence {
    pool: deadpool_sqlite::Pool,
}

/// Error returned by [`SessionPersistence`] async SQLite operations (RBP-03).
/// `Display` is preserved byte-for-byte via the message carried in each variant.
#[derive(Debug, thiserror::Error)]
pub enum SessionPersistenceError {
    /// deadpool connection-pool creation or checkout failed.
    #[error("{0}")]
    Pool(String),
    /// `deadpool_sqlite::interact` dispatch failed (blocking task panicked or cancelled).
    #[error("{0}")]
    Interact(String),
    /// Underlying SQLite operation (schema / save / load / list / delete) failed.
    #[error("{0}")]
    Sqlite(String),
    /// Snapshot (de)serialization failed — chains the underlying [`GoTSnapshotError`].
    #[error(transparent)]
    Snapshot(#[from] GoTSnapshotError),
}

impl SessionPersistence {
    /// Create a new persistence layer, initializing the connection pool and schema.
    ///
    /// # Arguments
    ///
    /// * `db_path` — Path to the SQLite database file. Created if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns error if pool creation or schema initialization fails.
    pub async fn new(db_path: &str) -> Result<Self, SessionPersistenceError> {
        let cfg = deadpool_sqlite::Config::new(db_path);
        let pool = cfg
            .create_pool(deadpool_sqlite::Runtime::Tokio1)
            .map_err(|e| {
                SessionPersistenceError::Pool(format!(
                    "SessionPersistence pool creation failed: {e}"
                ))
            })?;

        // Initialize schema
        let conn = pool
            .get()
            .await
            .map_err(|e| SessionPersistenceError::Pool(format!("pool.get failed: {e}")))?;
        conn.interact(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS got_sessions (
                    session_id TEXT PRIMARY KEY,
                    snapshot   BLOB NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                )",
            )
        })
        .await
        .map_err(|e| SessionPersistenceError::Interact(format!("interact failed: {e}")))?
        .map_err(|e| SessionPersistenceError::Sqlite(format!("schema init failed: {e}")))?;

        Ok(Self { pool })
    }

    /// Save a GoT snapshot for the given session.
    ///
    /// Uses upsert: if the session already exists, its snapshot and `updated_at`
    /// timestamp are overwritten.
    ///
    /// # Errors
    ///
    /// Returns error on serialization, pool, or SQLite failure.
    pub async fn save_snapshot(
        &self,
        session_id: &str,
        snapshot: &GoTSnapshot,
    ) -> Result<(), SessionPersistenceError> {
        let bytes = snapshot.to_bytes()?;
        let sid = session_id.to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| SessionPersistenceError::Pool(format!("pool.get failed: {e}")))?;

        conn.interact(move |conn| {
            conn.execute(
                "INSERT INTO got_sessions (session_id, snapshot, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT(session_id) DO UPDATE SET snapshot=?2, updated_at=?3",
                rusqlite::params![sid, bytes, now],
            )
        })
        .await
        .map_err(|e| SessionPersistenceError::Interact(format!("interact failed: {e}")))?
        .map_err(|e| SessionPersistenceError::Sqlite(format!("save failed: {e}")))?;

        Ok(())
    }

    /// Load a GoT snapshot for the given session.
    ///
    /// Returns `Ok(None)` if no snapshot exists for `session_id`.
    ///
    /// # Errors
    ///
    /// Returns error on pool, SQLite, or deserialization failure.
    pub async fn load_snapshot(
        &self,
        session_id: &str,
    ) -> Result<Option<GoTSnapshot>, SessionPersistenceError> {
        let sid = session_id.to_string();

        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| SessionPersistenceError::Pool(format!("pool.get failed: {e}")))?;

        let bytes_opt: Option<Vec<u8>> = conn
            .interact(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT snapshot FROM got_sessions WHERE session_id = ?1")?;
                let mut rows = stmt.query(rusqlite::params![sid])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(row.get::<_, Vec<u8>>(0)?))
                } else {
                    Ok(None)
                }
            })
            .await
            .map_err(|e| SessionPersistenceError::Interact(format!("interact failed: {e}")))?
            .map_err(|e: rusqlite::Error| {
                SessionPersistenceError::Sqlite(format!("load failed: {e}"))
            })?;

        match bytes_opt {
            Some(bytes) => GoTSnapshot::from_bytes(&bytes)
                .map(Some)
                .map_err(Into::into),
            None => Ok(None),
        }
    }

    /// List all session IDs, ordered by most recently updated first.
    ///
    /// # Errors
    ///
    /// Returns error on pool or SQLite failure.
    pub async fn list_sessions(&self) -> Result<Vec<String>, SessionPersistenceError> {
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| SessionPersistenceError::Pool(format!("pool.get failed: {e}")))?;

        conn.interact(|conn| {
            let mut stmt =
                conn.prepare("SELECT session_id FROM got_sessions ORDER BY updated_at DESC")?;
            let ids = stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            Ok(ids)
        })
        .await
        .map_err(|e| SessionPersistenceError::Interact(format!("interact failed: {e}")))?
        .map_err(|e: rusqlite::Error| SessionPersistenceError::Sqlite(format!("list failed: {e}")))
    }

    /// Delete a session and its snapshot.
    ///
    /// No-op if the session doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns error on pool or SQLite failure.
    pub async fn delete_session(&self, session_id: &str) -> Result<(), SessionPersistenceError> {
        let sid = session_id.to_string();

        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| SessionPersistenceError::Pool(format!("pool.get failed: {e}")))?;

        conn.interact(move |conn| {
            conn.execute(
                "DELETE FROM got_sessions WHERE session_id = ?1",
                rusqlite::params![sid],
            )
        })
        .await
        .map_err(|e| SessionPersistenceError::Interact(format!("interact failed: {e}")))?
        .map_err(|e| SessionPersistenceError::Sqlite(format!("delete failed: {e}")))?;

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasoning::got::{GotEngine, GotNode};

    fn build_test_snapshot(session_id: &str) -> GoTSnapshot {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "plan", 1.0));
        engine.add_node(GotNode::new(2, "execute", 0.9));
        engine.add_edge(1, 2);
        GoTSnapshot::from_engine(&engine, session_id)
    }

    #[tokio::test]
    async fn test_persistence_new_creates_schema() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let path_str = db_path.to_string_lossy().to_string();

        let sp = SessionPersistence::new(&path_str)
            .await
            .expect("new should succeed");

        // Schema should exist — list_sessions should return empty vec
        let sessions = sp.list_sessions().await.expect("list");
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_persistence_save_and_load_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("roundtrip.db");
        let path_str = db_path.to_string_lossy().to_string();

        let sp = SessionPersistence::new(&path_str).await.expect("new");
        let snapshot = build_test_snapshot("rt-1");

        sp.save_snapshot("rt-1", &snapshot).await.expect("save");
        let loaded = sp
            .load_snapshot("rt-1")
            .await
            .expect("load")
            .expect("should exist");

        assert_eq!(loaded.session_id, "rt-1");
        assert_eq!(loaded.nodes.len(), 2);
        assert_eq!(loaded.max_depth, 3);
    }

    #[tokio::test]
    async fn test_persistence_load_nonexistent_returns_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("noexist.db");
        let path_str = db_path.to_string_lossy().to_string();

        let sp = SessionPersistence::new(&path_str).await.expect("new");
        let result = sp.load_snapshot("does-not-exist").await.expect("load");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_persistence_list_sessions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("list.db");
        let path_str = db_path.to_string_lossy().to_string();

        let sp = SessionPersistence::new(&path_str).await.expect("new");

        sp.save_snapshot("alpha", &build_test_snapshot("alpha"))
            .await
            .expect("save alpha");
        sp.save_snapshot("beta", &build_test_snapshot("beta"))
            .await
            .expect("save beta");

        let sessions = sp.list_sessions().await.expect("list");
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&"alpha".to_string()));
        assert!(sessions.contains(&"beta".to_string()));
    }

    #[tokio::test]
    async fn test_persistence_delete_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("delete.db");
        let path_str = db_path.to_string_lossy().to_string();

        let sp = SessionPersistence::new(&path_str).await.expect("new");
        sp.save_snapshot("del-1", &build_test_snapshot("del-1"))
            .await
            .expect("save");

        sp.delete_session("del-1").await.expect("delete");

        let loaded = sp.load_snapshot("del-1").await.expect("load after delete");
        assert!(loaded.is_none());

        let sessions = sp.list_sessions().await.expect("list");
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_persistence_save_overwrites_existing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("upsert.db");
        let path_str = db_path.to_string_lossy().to_string();

        let sp = SessionPersistence::new(&path_str).await.expect("new");

        // Save initial
        let snap1 = build_test_snapshot("up-1");
        sp.save_snapshot("up-1", &snap1).await.expect("save 1");

        // Save updated (different engine state)
        let mut engine2 = GotEngine::new(5);
        engine2.add_node(GotNode::new(10, "revised", 2.0));
        let snap2 = GoTSnapshot::from_engine(&engine2, "up-1");
        sp.save_snapshot("up-1", &snap2).await.expect("save 2");

        // Load should return the updated version
        let loaded = sp
            .load_snapshot("up-1")
            .await
            .expect("load")
            .expect("should exist");
        assert_eq!(loaded.max_depth, 5);
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.nodes[0].label, "revised");

        // Should still be only one session
        let sessions = sp.list_sessions().await.expect("list");
        assert_eq!(sessions.len(), 1);
    }

    #[tokio::test]
    async fn test_persistence_e2e_snapshot_then_persist_then_restore() {
        // End-to-end: GoTEngine -> GoTSnapshot -> save -> load -> verify
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("e2e.db");
        let path_str = db_path.to_string_lossy().to_string();

        // Build engine
        let mut engine = GotEngine::new(4);
        engine.add_node(GotNode::new(1, "observe", 1.0));
        engine.add_node(GotNode::new(2, "orient", 0.8));
        engine.add_node(GotNode::new(3, "decide", 0.6));
        engine.add_node(GotNode::new(4, "act", 0.4));
        engine.add_edge(1, 2);
        engine.add_edge(2, 3);
        engine.add_edge(3, 4);

        // Snapshot
        let snapshot = GoTSnapshot::from_engine(&engine, "ooda-loop");

        // Persist
        let sp = SessionPersistence::new(&path_str).await.expect("new");
        sp.save_snapshot("ooda-loop", &snapshot)
            .await
            .expect("save");

        // Restore
        let restored = sp
            .load_snapshot("ooda-loop")
            .await
            .expect("load")
            .expect("should exist");

        assert_eq!(restored.session_id, "ooda-loop");
        assert_eq!(restored.nodes.len(), 4);
        assert_eq!(restored.max_depth, 4);

        // Verify edge structure
        let node1 = restored.nodes.iter().find(|n| n.id == 1).expect("node 1");
        assert_eq!(node1.child_ids, vec![2]);
        let node3 = restored.nodes.iter().find(|n| n.id == 3).expect("node 3");
        assert_eq!(node3.child_ids, vec![4]);
    }

    #[tokio::test]
    async fn test_persistence_delete_nonexistent_is_noop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("noop.db");
        let path_str = db_path.to_string_lossy().to_string();

        let sp = SessionPersistence::new(&path_str).await.expect("new");
        // Should not error
        sp.delete_session("ghost").await.expect("delete noop");
    }
}
