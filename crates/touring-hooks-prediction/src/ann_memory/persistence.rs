//! Persistence layer for AnnMemoryRecall.
//!
//! Provides SQLite-backed persistence for ANN memory index, enabling
//! warm restarts without losing embeddings.

use std::path::Path;

use rusqlite::{Connection, params};
use serde_json;

use super::{AnnMemoryRecall, MemoryEntry};

/// PersistedAnnMemoryRecall provides SQLite-backed persistence for ANN recall.
#[derive(Debug)]
pub struct PersistedAnnMemoryRecall {
    ann: AnnMemoryRecall,
    conn: Connection,
    last_rowid: u64,
}

impl PersistedAnnMemoryRecall {
    /// Load from existing SQLite file, or create new if doesn't exist.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, PersistError> {
        let conn = Connection::open(path).map_err(PersistError::DbOpen)?;
        Self::init_schema(&conn)?;
        let mut ann = AnnMemoryRecall::new();
        let last_rowid = Self::rebuild_index(&conn, &mut ann)?;
        Ok(Self {
            ann,
            conn,
            last_rowid,
        })
    }

    /// Create a new in-memory instance (no persistence).
    pub fn new() -> Self {
        Self {
            ann: AnnMemoryRecall::new(),
            conn: Connection::open_in_memory().expect("in-memory DB"),
            last_rowid: 0,
        }
    }

    fn init_schema(conn: &Connection) -> Result<(), PersistError> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS embeddings (
                rowid    INTEGER PRIMARY KEY,
                id       TEXT NOT NULL UNIQUE,
                content  TEXT NOT NULL,
                embedding BLOB NOT NULL,
                metadata_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_embeddings_id ON embeddings(id);
            CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            PRAGMA journal_mode = WAL;
            "#,
        )
        .map_err(PersistError::DbOpen)?;
        Ok(())
    }

    /// Deserialize `f32` slice from little-endian bytes.
    /// Replaces unmaintained `bincode 1.x` (RUSTSEC-2025-0141); format is
    /// width-pure LE f32, byte length must be a multiple of 4.
    fn deserialize_embedding(bytes: &[u8]) -> Result<Vec<f32>, PersistError> {
        if !bytes.len().is_multiple_of(std::mem::size_of::<f32>()) {
            return Err(PersistError::Serialize(format!(
                "embedding byte length {} is not a multiple of {}",
                bytes.len(),
                std::mem::size_of::<f32>()
            )));
        }
        let mut out = Vec::with_capacity(bytes.len() / std::mem::size_of::<f32>());
        for chunk in bytes.chunks_exact(std::mem::size_of::<f32>()) {
            out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(out)
    }

    /// Serialize `f32` slice to little-endian bytes.
    /// Replaces unmaintained `bincode 1.x` (RUSTSEC-2025-0141); bit-exact round-trip.
    fn serialize_embedding(embedding: &[f32]) -> Result<Vec<u8>, PersistError> {
        let mut out = Vec::with_capacity(std::mem::size_of_val(embedding));
        for &v in embedding {
            out.extend_from_slice(&v.to_le_bytes());
        }
        Ok(out)
    }

    fn rebuild_index(conn: &Connection, ann: &mut AnnMemoryRecall) -> Result<u64, PersistError> {
        let mut stmt = conn
            .prepare("SELECT rowid, id, content, embedding, metadata_json FROM embeddings ORDER BY rowid")
            .map_err(PersistError::Query)?;
        let mut last_rowid = 0u64;
        let rows = stmt
            .query_map([], |row| {
                let embedding_blob: Vec<u8> = row.get(3).expect("embedding blob");
                let metadata_json: Option<String> = row.get(4).expect("metadata");
                Ok((
                    row.get::<_, i64>(0).expect("rowid") as u64,
                    row.get::<_, String>(1).expect("id"),
                    row.get::<_, String>(2).expect("content"),
                    embedding_blob,
                    metadata_json,
                ))
            })
            .map_err(PersistError::Query)?;
        for row in rows.flatten() {
            let (rowid, id, content, embedding_blob, metadata_json) = row;
            let embedding = Self::deserialize_embedding(&embedding_blob)?;
            let entry = MemoryEntry {
                id,
                content,
                embedding,
                metadata: metadata_json
                    .map(|m| serde_json::from_str(&m))
                    .transpose()?,
            };
            ann.add_memory(entry);
            last_rowid = rowid;
        }
        Ok(last_rowid)
    }

    /// Add a memory entry with write-through to SQLite.
    pub fn add_memory(&mut self, entry: MemoryEntry) -> Result<(), PersistError> {
        let metadata_json = entry
            .metadata
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());
        let embedding_blob = Self::serialize_embedding(&entry.embedding)?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO embeddings (id, content, embedding, metadata_json) VALUES (?1, ?2, ?3, ?4)",
                params![entry.id, entry.content, embedding_blob, metadata_json],
            )
            .map_err(PersistError::Query)?;
        self.last_rowid = self.conn.last_insert_rowid() as u64;
        self.ann.add_memory(entry);
        Ok(())
    }

    /// Add multiple entries in batch.
    pub fn add_batch(&mut self, entries: Vec<MemoryEntry>) -> Result<(), PersistError> {
        let tx = self.conn.unchecked_transaction()?;
        for entry in entries {
            let metadata_json = entry
                .metadata
                .as_ref()
                .and_then(|m| serde_json::to_string(m).ok());
            let embedding_blob = Self::serialize_embedding(&entry.embedding)?;
            self.conn
                .execute(
                    "INSERT OR REPLACE INTO embeddings (id, content, embedding, metadata_json) VALUES (?1, ?2, ?3, ?4)",
                    params![entry.id, entry.content, embedding_blob, metadata_json],
                )
                .map_err(PersistError::Query)?;
            self.last_rowid = self.conn.last_insert_rowid() as u64;
            self.ann.add_memory(entry);
        }
        tx.commit().map_err(PersistError::Query)?;
        Ok(())
    }

    /// Search for similar memories.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<super::MemorySearchResult> {
        self.ann.search(query, k)
    }

    /// Get a specific memory by ID.
    pub fn get(&self, id: &str) -> Option<&MemoryEntry> {
        self.ann.get(id)
    }

    /// Number of memories.
    pub fn len(&self) -> usize {
        self.ann.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.ann.is_empty()
    }

    /// Explicit persist (no-op since write-through already persists).
    pub fn persist(&self) -> Result<(), PersistError> {
        Ok(())
    }

    /// Run integrity check on the database.
    pub fn integrity_check(&self) -> Result<(), PersistError> {
        let result: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(PersistError::Query)?;
        if result == "ok" {
            Ok(())
        } else {
            Err(PersistError::Integrity(result))
        }
    }

    /// Get the underlying AnnMemoryRecall for read operations.
    pub fn inner(&self) -> &AnnMemoryRecall {
        &self.ann
    }

    /// Get a mutable reference to the underlying AnnMemoryRecall.
    pub fn inner_mut(&mut self) -> &mut AnnMemoryRecall {
        &mut self.ann
    }
}

impl Default for PersistedAnnMemoryRecall {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors from persistence operations.
#[derive(Debug)]
pub enum PersistError {
    /// Failed to open or initialize the SQLite database.
    DbOpen(rusqlite::Error),
    /// A SQL query against the persistence store failed.
    Query(rusqlite::Error),
    /// Failed to serialize an entry for storage.
    Serialize(String),
    /// Failed to parse a stored value back into its in-memory form.
    ParseError(String),
    /// An integrity check on persisted data failed.
    Integrity(String),
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistError::DbOpen(e) => write!(f, "database open error: {e}"),
            PersistError::Query(e) => write!(f, "query error: {e}"),
            PersistError::Serialize(s) => write!(f, "serialization error: {s}"),
            PersistError::ParseError(s) => write!(f, "parse error: {s}"),
            PersistError::Integrity(s) => write!(f, "integrity check failed: {s}"),
        }
    }
}

impl From<rusqlite::Error> for PersistError {
    fn from(err: rusqlite::Error) -> Self {
        PersistError::Query(err)
    }
}

impl From<serde_json::Error> for PersistError {
    fn from(err: serde_json::Error) -> Self {
        PersistError::ParseError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_persisted_new_is_empty() {
        let recall = PersistedAnnMemoryRecall::new();
        assert!(recall.is_empty());
    }

    #[test]
    fn test_add_and_search() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("test_memory.db");
        let mut recall = PersistedAnnMemoryRecall::load(&path).expect("load");
        recall
            .add_memory(MemoryEntry::new(
                "mem1",
                "rust error handling",
                vec![0.1, 0.2, 0.8],
            ))
            .expect("add_memory");
        assert_eq!(recall.len(), 1);
        assert!(recall.get("mem1").is_some());
        let results = recall.search(&[0.1, 0.2, 0.9], 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "mem1");
    }

    #[test]
    fn test_persisted_warm_restart() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("test_memory.db");
        {
            let mut recall = PersistedAnnMemoryRecall::load(&path).expect("load");
            recall
                .add_batch(vec![
                    MemoryEntry::new("a", "content a", vec![1.0, 0.0, 0.0]),
                    MemoryEntry::new("b", "content b", vec![0.0, 1.0, 0.0]),
                ])
                .expect("add_batch");
        }
        let recall = PersistedAnnMemoryRecall::load(&path).expect("load");
        assert_eq!(recall.len(), 2);
        assert!(recall.get("a").is_some());
        assert!(recall.get("b").is_some());
    }

    #[test]
    fn test_integrity_check_ok() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("test_integrity.db");
        let mut recall = PersistedAnnMemoryRecall::load(&path).expect("load");
        recall
            .add_memory(MemoryEntry::new("test", "data", vec![1.0]))
            .expect("add");
        assert!(recall.integrity_check().is_ok());
    }

    /// S-04 (2026-05-29): the core invariant this wave repairs — after a memory
    /// entry is stored with a (hash) embedding, ANN search returns >= 1 result.
    /// An empty corpus gave 0 ANN hits, so `memory recall` degraded to FTS/
    /// TF-IDF only; `cli_memory_store` + `cli_memory_reindex` now populate it.
    #[test]
    fn recall_returns_ann_hits() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("recall_ann.db");
        let mut recall = PersistedAnnMemoryRecall::load(&path).expect("load");

        let query = "semantic memory lesson about ANN backfill";
        let embedding = super::super::query_hash_embedding(query);

        recall
            .add_memory(MemoryEntry::new(
                "lesson:ann-backfill",
                query,
                embedding.clone(),
            ))
            .expect("add_memory");

        let results = recall.search(&embedding, 5);
        assert!(
            !results.is_empty(),
            "ann_results must be > 0 after add_memory (S-04 invariant)"
        );
        assert_eq!(results[0].id, "lesson:ann-backfill");
    }
}
