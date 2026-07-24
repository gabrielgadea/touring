//! GoT Snapshot Store — synchronous SQLite persistence for GoT snapshots.
//!
//! Provides [`GoTSnapshotStore`] for saving and loading [`GoTSnapshot`] data
//! using rusqlite (sync). This complements the async `SessionPersistence` in
//! touring-cognitive with a sync interface suitable for use in session hooks
//! (which run synchronously in the hook pipeline).
//!
//! Snapshots are stored as rkyv-serialized BLOBs keyed by session_id.

use rusqlite::{Connection, params};
use std::path::Path;
use touring_intelligence::reasoning::GoTSnapshot;

/// Errors returned by [`GoTSnapshotStore`] operations.
#[derive(Debug, thiserror::Error)]
pub enum GoTSnapshotStoreError {
    /// A SQLite operation failed (open, pragma, schema init, query, or write).
    ///
    /// `context` names the operation that failed; `source` carries the
    /// underlying `rusqlite` error for full diagnostics.
    #[error("GoTSnapshotStore: {context}: {source}")]
    Sqlite {
        /// Human-readable name of the SQLite operation that failed.
        context: &'static str,
        /// The underlying `rusqlite` error.
        #[source]
        source: rusqlite::Error,
    },
    /// Snapshot (de)serialization failed (rkyv encode/decode or schema mismatch).
    #[error("GoTSnapshotStore: snapshot serialization: {0}")]
    Serialization(String),
}

/// Synchronous SQLite store for GoT snapshots.
///
/// Uses the same schema as `SessionPersistence` (touring-cognitive) but with
/// a synchronous rusqlite connection, suitable for hook contexts where async
/// runtimes may not be available.
pub struct GoTSnapshotStore {
    conn: Connection,
}

impl GoTSnapshotStore {
    /// Open or create the snapshot database at `db_path`.
    ///
    /// Initializes the `got_sessions` table if it doesn't exist.
    /// Uses WAL journal mode for concurrent read access.
    ///
    /// # Errors
    ///
    /// Returns error if the database cannot be opened or schema creation fails.
    pub fn new(db_path: &Path) -> Result<Self, GoTSnapshotStoreError> {
        let conn = Connection::open(db_path).map_err(|source| GoTSnapshotStoreError::Sqlite {
            context: "cannot open DB",
            source,
        })?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|source| GoTSnapshotStoreError::Sqlite {
                context: "WAL pragma failed",
                source,
            })?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS got_sessions (
                session_id TEXT PRIMARY KEY,
                snapshot   BLOB NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .map_err(|source| GoTSnapshotStoreError::Sqlite {
            context: "schema init failed",
            source,
        })?;

        Ok(Self { conn })
    }

    /// Save a GoT snapshot for the given session (upsert semantics).
    ///
    /// # Errors
    ///
    /// Returns error on serialization or SQLite failure.
    pub fn save(
        &self,
        session_id: &str,
        snapshot: &GoTSnapshot,
    ) -> Result<(), GoTSnapshotStoreError> {
        let bytes = snapshot
            .to_bytes()
            .map_err(|e| GoTSnapshotStoreError::Serialization(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();

        self.conn
            .execute(
                "INSERT INTO got_sessions (session_id, snapshot, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT(session_id) DO UPDATE SET snapshot=?2, updated_at=?3",
                params![session_id, bytes, now],
            )
            .map_err(|source| GoTSnapshotStoreError::Sqlite {
                context: "save failed",
                source,
            })?;

        Ok(())
    }

    /// Load the most recent GoT snapshot (by updated_at).
    ///
    /// Returns `Ok(None)` if no snapshots exist.
    ///
    /// # Errors
    ///
    /// Returns error on SQLite or deserialization failure.
    pub fn load_latest(&self) -> Result<Option<(String, GoTSnapshot)>, GoTSnapshotStoreError> {
        let result: Option<(String, Vec<u8>)> = self
            .conn
            .query_row(
                "SELECT session_id, snapshot FROM got_sessions ORDER BY updated_at DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|source| GoTSnapshotStoreError::Sqlite {
                context: "load_latest query failed",
                source,
            })?;

        match result {
            Some((sid, bytes)) => {
                let snapshot = GoTSnapshot::from_bytes(&bytes)
                    .map_err(|e| GoTSnapshotStoreError::Serialization(e.to_string()))?;
                Ok(Some((sid, snapshot)))
            }
            None => Ok(None),
        }
    }

    /// Load a specific session's snapshot by ID.
    ///
    /// Returns `Ok(None)` if the session doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns error on SQLite or deserialization failure.
    pub fn load_by_session(
        &self,
        session_id: &str,
    ) -> Result<Option<GoTSnapshot>, GoTSnapshotStoreError> {
        let result: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT snapshot FROM got_sessions WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| GoTSnapshotStoreError::Sqlite {
                context: "load_by_session query failed",
                source,
            })?;

        match result {
            Some(bytes) => GoTSnapshot::from_bytes(&bytes)
                .map(Some)
                .map_err(|e| GoTSnapshotStoreError::Serialization(e.to_string())),
            None => Ok(None),
        }
    }

    /// List all session IDs, ordered by most recently updated first.
    ///
    /// # Errors
    ///
    /// Returns error on SQLite failure.
    pub fn list_sessions(&self) -> Result<Vec<String>, GoTSnapshotStoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT session_id FROM got_sessions ORDER BY updated_at DESC")
            .map_err(|source| GoTSnapshotStoreError::Sqlite {
                context: "prepare failed",
                source,
            })?;

        let ids = stmt
            .query_map([], |row| row.get(0))
            .map_err(|source| GoTSnapshotStoreError::Sqlite {
                context: "query failed",
                source,
            })?
            .collect::<Result<Vec<String>, _>>()
            .map_err(|source| GoTSnapshotStoreError::Sqlite {
                context: "collect failed",
                source,
            })?;

        Ok(ids)
    }

    /// Count the number of stored snapshots.
    ///
    /// # Errors
    ///
    /// Returns error on SQLite failure.
    pub fn count(&self) -> Result<usize, GoTSnapshotStoreError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM got_sessions", [], |row| row.get(0))
            .map_err(|source| GoTSnapshotStoreError::Sqlite {
                context: "count failed",
                source,
            })?;
        Ok(n as usize)
    }
}

// ── Use rusqlite::OptionalExtension for .optional() ──────────────────────
use rusqlite::OptionalExtension;

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use touring_intelligence::reasoning::GoTSnapshot;
    use touring_intelligence::reasoning::got::{GotEngine, GotNode};

    fn build_test_snapshot(session_id: &str) -> GoTSnapshot {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "plan", 1.0));
        engine.add_node(GotNode::new(2, "execute", 0.9));
        engine.add_edge(1, 2);
        GoTSnapshot::from_engine(&engine, session_id)
    }

    #[test]
    fn test_store_new_creates_schema() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("got_test.db");

        let store = GoTSnapshotStore::new(&db_path).expect("new should succeed");
        let sessions = store.list_sessions().expect("list");
        assert!(sessions.is_empty());
        assert_eq!(store.count().expect("count"), 0);
    }

    #[test]
    fn test_store_save_and_load_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("got_roundtrip.db");

        let store = GoTSnapshotStore::new(&db_path).expect("new");
        let snapshot = build_test_snapshot("sess-1");

        store.save("sess-1", &snapshot).expect("save");

        // Load by session ID
        let loaded = store
            .load_by_session("sess-1")
            .expect("load")
            .expect("should exist");
        assert_eq!(loaded.session_id, "sess-1");
        assert_eq!(loaded.nodes.len(), 2);
        assert_eq!(loaded.max_depth, 3);

        // Load latest
        let (sid, latest) = store
            .load_latest()
            .expect("load_latest")
            .expect("should exist");
        assert_eq!(sid, "sess-1");
        assert_eq!(latest.nodes.len(), 2);
    }

    #[test]
    fn test_store_load_nonexistent_returns_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("got_noexist.db");

        let store = GoTSnapshotStore::new(&db_path).expect("new");
        assert!(store.load_by_session("ghost").expect("load").is_none());
        assert!(store.load_latest().expect("load_latest").is_none());
    }

    #[test]
    fn test_store_save_overwrites_existing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("got_upsert.db");

        let store = GoTSnapshotStore::new(&db_path).expect("new");

        // Save initial
        let snap1 = build_test_snapshot("up-1");
        store.save("up-1", &snap1).expect("save 1");

        // Save updated (different engine state)
        let mut engine2 = GotEngine::new(5);
        engine2.add_node(GotNode::new(10, "revised", 2.0));
        let snap2 = GoTSnapshot::from_engine(&engine2, "up-1");
        store.save("up-1", &snap2).expect("save 2");

        // Load should return the updated version
        let loaded = store
            .load_by_session("up-1")
            .expect("load")
            .expect("should exist");
        assert_eq!(loaded.max_depth, 5);
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.nodes[0].label, "revised");

        // Should still be only one session
        assert_eq!(store.count().expect("count"), 1);
    }

    #[test]
    fn test_store_multiple_sessions_ordered() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("got_multi.db");

        let store = GoTSnapshotStore::new(&db_path).expect("new");

        store
            .save("alpha", &build_test_snapshot("alpha"))
            .expect("save alpha");
        store
            .save("beta", &build_test_snapshot("beta"))
            .expect("save beta");

        let sessions = store.list_sessions().expect("list");
        assert_eq!(sessions.len(), 2);

        // beta was saved last, so it should be first (most recent)
        assert_eq!(sessions[0], "beta");
        assert_eq!(sessions[1], "alpha");

        // load_latest should return beta
        let (sid, _) = store.load_latest().expect("latest").expect("should exist");
        assert_eq!(sid, "beta");

        assert_eq!(store.count().expect("count"), 2);
    }
}
