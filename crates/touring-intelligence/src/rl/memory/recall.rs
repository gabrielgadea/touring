//! Semantic Recall — FTS5 + vector similarity search.
//!
//! Unified from touring/src/memory/recall.rs (717 LOC)

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use thiserror::Error;

use touring_simd::learning::simd_knn_search;

/// Errors that can occur in semantic recall operations.
#[derive(Error, Debug)]
pub enum RecallError {
    /// Underlying SQLite failure.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// An embedding's dimension did not match the expected dimension.
    #[error("Embedding dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// Expected embedding dimension.
        expected: usize,
        /// Actual embedding dimension received.
        actual: usize,
    },
    /// The embedding bytes could not be decoded.
    #[error("Invalid embedding data")]
    InvalidEmbedding,
}

/// Convenience result type for recall operations.
pub type Result<T> = std::result::Result<T, RecallError>;

/// A matched chunk with relevance score.
#[derive(Debug, Clone)]
pub struct ChunkMatch {
    /// Row identifier of the matched chunk.
    pub id: i64,
    /// Text content of the chunk.
    pub content: String,
    /// Optional metadata stored alongside the chunk.
    pub metadata: Option<serde_json::Value>,
    /// Relevance score of the match.
    pub score: f32,
}

/// Which schema the existing DB uses.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SchemaKind {
    Touring,
    Python,
}

/// Semantic recall system with FTS5 and cosine similarity.
#[derive(Debug)]
pub struct SemanticRecall {
    conn: Connection,
    embedding_dim: usize,
    schema: SchemaKind,
}

impl SemanticRecall {
    /// Opens (or creates) the recall database with the given embedding dimension.
    pub fn new(db_path: &Path, embedding_dim: usize) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;",
        )?;
        let schema = Self::detect_schema(&conn);
        let recall = Self {
            conn,
            embedding_dim,
            schema,
        };
        recall.ensure_schema()?;
        Ok(recall)
    }

    fn detect_schema(conn: &Connection) -> SchemaKind {
        let has_text_col = conn
            .prepare("PRAGMA table_info(chunks)")
            .and_then(|mut stmt| {
                let cols: Vec<String> = stmt
                    .query_map([], |row| row.get::<_, String>(1))?
                    .filter_map(|r| r.ok())
                    .collect();
                Ok(cols)
            })
            .map(|cols| cols.iter().any(|c| c == "text"))
            .unwrap_or(false);

        if has_text_col {
            SchemaKind::Python
        } else {
            SchemaKind::Touring
        }
    }

    fn text_col(&self) -> &'static str {
        match self.schema {
            SchemaKind::Touring => "content",
            SchemaKind::Python => "text",
        }
    }

    fn ensure_schema(&self) -> Result<()> {
        match self.schema {
            SchemaKind::Touring => self.ensure_touring_schema(),
            SchemaKind::Python => self.ensure_fts_for_python(),
        }
    }

    fn ensure_touring_schema(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                embedding BLOB,
                metadata_json TEXT
            )",
            [],
        )?;
        // Idempotent: add u4 quantization columns if they don't exist yet.
        // SQLite does not support ADD COLUMN IF NOT EXISTS, so we ignore the
        // "duplicate column name" error (SQLITE_ERROR) which means they already exist.
        for col in &[
            "ALTER TABLE chunks ADD COLUMN embedding_u4 BLOB",
            "ALTER TABLE chunks ADD COLUMN quant_scale REAL",
            "ALTER TABLE chunks ADD COLUMN quant_zero REAL",
        ] {
            if let Err(e) = self.conn.execute(col, []) {
                // SQLite returns SQLITE_ERROR for "duplicate column name" when the
                // column already exists. This is the expected idempotency path.
                if !e.to_string().contains("duplicate column name") {
                    return Err(e.into());
                }
            }
        }
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                content,
                content='chunks',
                content_rowid='id'
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
                INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
            END",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
                INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES ('delete', old.id, old.content);
            END",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
                INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES ('delete', old.id, old.content);
                INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
            END",
            [],
        )?;
        Ok(())
    }

    fn ensure_fts_for_python(&self) -> Result<()> {
        let fts_exists: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='chunks_fts'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);

        if !fts_exists {
            self.conn.execute(
                "CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                    content,
                    content='chunks',
                    content_rowid='rowid'
                )",
                [],
            )?;
        }
        Ok(())
    }

    /// Stores a text chunk with an optional embedding and metadata, returning its id.
    pub fn store_chunk(
        &self,
        content: &str,
        embedding: Option<&[f32]>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<i64> {
        if let Some(emb) = embedding
            && emb.len() != self.embedding_dim
        {
            return Err(RecallError::DimensionMismatch {
                expected: self.embedding_dim,
                actual: emb.len(),
            });
        }

        let embedding_bytes: Option<Vec<u8>> =
            embedding.map(|emb| emb.iter().flat_map(|f| f.to_le_bytes()).collect());

        match self.schema {
            SchemaKind::Touring => {
                let metadata_str = metadata.map(|m| m.to_string());
                #[cfg(feature = "u4-quantization")]
                {
                    use touring_simd::quantization::EmbeddingU4;
                    if let Some(emb) = embedding {
                        let q = EmbeddingU4::from_f32(emb);
                        self.conn.execute(
                            "INSERT INTO chunks \
                             (content, embedding, embedding_u4, quant_scale, quant_zero, metadata_json) \
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![
                                content,
                                embedding_bytes,
                                q.to_bytes(),
                                q.scale,
                                q.zero,
                                metadata_str
                            ],
                        )?;
                        return Ok(self.conn.last_insert_rowid());
                    }
                }
                self.conn.execute(
                    "INSERT INTO chunks (content, embedding, metadata_json) VALUES (?1, ?2, ?3)",
                    params![content, embedding_bytes, metadata_str],
                )?;
            }
            SchemaKind::Python => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let key = metadata
                    .and_then(|m| m.get("key").and_then(|k| k.as_str()))
                    .unwrap_or("touring");
                let hash = format!("{:x}", fnv_hash(content.as_bytes()));
                let id = format!("touring_{}", now);

                self.conn.execute(
                    "INSERT INTO chunks (id, key, source, hash, text, embedding, created_at, accessed_at)
                     VALUES (?1, ?2, 'touring', ?3, ?4, ?5, ?6, ?7)",
                    params![id, key, hash, content, embedding_bytes, now, now],
                )?;
            }
        }

        let id = self.conn.last_insert_rowid();
        Ok(id)
    }

    #[must_use = "search results should be used or explicitly discarded"]
    /// Full-text search over chunks via FTS5, returning the top `top_k` matches.
    pub fn fts_search(&self, query: &str, top_k: usize) -> Result<Vec<ChunkMatch>> {
        // FTS5 has many operator characters that cause "syntax error" when
        // they appear unescaped in user input (paths, slugs, IDs):
        //   / : tokenizer separator (e.g. "pattern:touring-native tooling:...")
        //   - : NOT operator (e.g. "tf-20260501-...")
        //   : : column qualifier
        //   ( ) : grouping
        //   * : prefix wildcard
        //   ^ : column anchor
        //   " : phrase delimiter
        // When the input contains ANY of these, wrap the whole string in a
        // phrase query (double quotes) so FTS5 treats it as a literal token
        // sequence. Inner double quotes are doubled to escape per FTS5 rules.
        // Pure-alphanumeric queries pass through unchanged so the bm25 ranker
        // can do tokenized matching as before.
        const SPECIAL: &[char] = &['"', '*', '^', '/', '-', ':', '(', ')'];
        let escaped_query = if query.chars().any(|c| SPECIAL.contains(&c)) {
            format!("\"{}\"", query.replace('"', "\"\""))
        } else {
            query.to_string()
        };

        let text_col = self.text_col();
        let sql = match self.schema {
            SchemaKind::Touring => format!(
                "SELECT c.id, c.{text_col}, c.metadata_json, rank
                 FROM chunks_fts AS fts
                 JOIN chunks AS c ON c.id = fts.rowid
                 WHERE chunks_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2"
            ),
            SchemaKind::Python => format!(
                "SELECT c.rowid, c.{text_col}, NULL, rank
                 FROM chunks_fts AS fts
                 JOIN chunks AS c ON c.rowid = fts.rowid
                 WHERE chunks_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2"
            ),
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![escaped_query, top_k as i64], |row| {
                let metadata_str: Option<String> = row.get(2)?;
                let metadata = metadata_str.and_then(|s| serde_json::from_str(&s).ok());
                Ok(ChunkMatch {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    metadata,
                    score: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Fetches a single chunk by its row id, if present.
    pub fn get_chunk(&self, id: i64) -> Result<Option<ChunkMatch>> {
        let text_col = self.text_col();
        let sql = match self.schema {
            SchemaKind::Touring => {
                format!("SELECT id, {text_col}, metadata_json FROM chunks WHERE id = ?1")
            }
            SchemaKind::Python => {
                format!("SELECT rowid, {text_col}, NULL FROM chunks WHERE rowid = ?1")
            }
        };
        let result = self
            .conn
            .query_row(&sql, params![id], |row| {
                let metadata_str: Option<String> = row.get(2)?;
                let metadata = metadata_str.and_then(|s| serde_json::from_str(&s).ok());
                Ok(ChunkMatch {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    metadata,
                    score: 1.0,
                })
            })
            .optional()?;
        Ok(result)
    }

    /// Returns aggregate statistics about the recall database.
    pub fn stats(&self) -> Result<RecallStats> {
        let total_chunks: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;
        let chunks_with_embeddings: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE embedding IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        let total_fts_rows: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM chunks_fts", [], |row| row.get(0))
            .unwrap_or(0);

        Ok(RecallStats {
            total_chunks,
            chunks_with_embeddings,
            total_fts_rows,
            embedding_dim: self.embedding_dim,
        })
    }

    /// ANN search — rank stored chunks by cosine similarity to `query_embedding`.
    ///
    /// Reads all non-null embeddings from SQLite, decodes them from their
    /// little-endian byte representation, then runs SIMD-accelerated KNN search.
    ///
    /// Returns up to `top_k` [`ChunkMatch`] entries ordered by similarity score
    /// (highest first). Entries without an embedding are skipped.
    pub fn ann_search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<ChunkMatch>> {
        if query_embedding.is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }

        // When u4-quantization is enabled, prefer reading from embedding_u4
        // (8x smaller, faster I/O) with fallback to full f32 embedding.
        #[cfg(feature = "u4-quantization")]
        {
            if let Ok(results) = self.ann_search_u4(query_embedding, top_k)
                && !results.is_empty()
            {
                return Ok(results);
            }
        }

        // Fallback: read full f32 embeddings
        self.ann_search_f32(query_embedding, top_k)
    }

    /// ANN search using full f32 embeddings (original path).
    fn ann_search_f32(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<ChunkMatch>> {
        let text_col = self.text_col();
        let sql = match self.schema {
            SchemaKind::Touring => format!(
                "SELECT id, {text_col}, metadata_json, embedding \
                 FROM chunks WHERE embedding IS NOT NULL"
            ),
            SchemaKind::Python => format!(
                "SELECT rowid, {text_col}, NULL, embedding \
                 FROM chunks WHERE embedding IS NOT NULL"
            ),
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let mut ids: Vec<i64> = Vec::new();
        let mut contents: Vec<String> = Vec::new();
        let mut metadatas: Vec<Option<serde_json::Value>> = Vec::new();
        let mut embeddings: Vec<Vec<f32>> = Vec::new();

        let rows = stmt.query_map([], |row| {
            let raw: Vec<u8> = row.get(3)?;
            let meta_str: Option<String> = row.get(2)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                meta_str,
                raw,
            ))
        })?;

        for row in rows.flatten() {
            let (id, content, meta_str, raw) = row;
            let emb = decode_embedding_bytes(&raw);
            if emb.is_empty() {
                continue;
            }
            let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
            ids.push(id);
            contents.push(content);
            metadatas.push(metadata);
            embeddings.push(emb);
        }

        if embeddings.is_empty() {
            return Ok(Vec::new());
        }

        let k = top_k.min(embeddings.len());
        let results = simd_knn_search(query_embedding, &embeddings, k);

        Ok(results
            .into_iter()
            .filter_map(|r| {
                let idx = r.index;
                Some(ChunkMatch {
                    id: *ids.get(idx)?,
                    content: contents.get(idx)?.clone(),
                    metadata: metadatas.get(idx)?.clone(),
                    score: r.score as f32,
                })
            })
            .collect())
    }

    /// ANN search using u4-quantized embeddings (8x less I/O, ~90% recall).
    ///
    /// Reads `embedding_u4 + quant_scale + quant_zero` from rows that have been
    /// quantized, decodes to f32, and runs SIMD KNN search. Falls back to empty
    /// vec if no u4 embeddings exist yet (pre-migration state).
    #[cfg(feature = "u4-quantization")]
    fn ann_search_u4(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<ChunkMatch>> {
        use touring_simd::quantization::EmbeddingU4;

        let text_col = self.text_col();
        let sql = format!(
            "SELECT id, {text_col}, metadata_json, embedding_u4, quant_scale, quant_zero \
             FROM chunks WHERE embedding_u4 IS NOT NULL"
        );

        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Ok(Vec::new()), // column doesn't exist yet
        };

        let mut ids: Vec<i64> = Vec::new();
        let mut contents: Vec<String> = Vec::new();
        let mut metadatas: Vec<Option<serde_json::Value>> = Vec::new();
        let mut embeddings: Vec<Vec<f32>> = Vec::new();

        let rows = stmt.query_map([], |row| {
            let u4_blob: Vec<u8> = row.get(3)?;
            let scale: f64 = row.get(4)?;
            let zero: f64 = row.get(5)?;
            let meta_str: Option<String> = row.get(2)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                meta_str,
                u4_blob,
                scale as f32,
                zero as f32,
            ))
        })?;

        for row in rows.flatten() {
            let (id, content, meta_str, u4_blob, _scale, _zero) = row;
            // Decode u4 → f32 using the EmbeddingU4 format (header + nibbles)
            let emb = match EmbeddingU4::from_bytes(&u4_blob) {
                Some(q) => q.to_f32(),
                None => continue,
            };
            if emb.is_empty() {
                continue;
            }
            let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
            ids.push(id);
            contents.push(content);
            metadatas.push(metadata);
            embeddings.push(emb);
        }

        if embeddings.is_empty() {
            return Ok(Vec::new());
        }

        let k = top_k.min(embeddings.len());
        let results = simd_knn_search(query_embedding, &embeddings, k);

        Ok(results
            .into_iter()
            .filter_map(|r| {
                let idx = r.index;
                Some(ChunkMatch {
                    id: *ids.get(idx)?,
                    content: contents.get(idx)?.clone(),
                    metadata: metadatas.get(idx)?.clone(),
                    score: r.score as f32,
                })
            })
            .collect())
    }

    /// Hybrid search — fuse FTS5 text-match results with ANN embedding results
    /// using Reciprocal Rank Fusion (RRF).
    ///
    /// # Arguments
    ///
    /// * `query_text`      — keyword query for FTS5
    /// * `query_embedding` — vector query for ANN similarity search
    /// * `k`               — number of results to return
    ///
    /// # Returns
    ///
    /// Top-`k` [`ChunkMatch`] entries ranked by RRF score (combines both signals).
    /// Chunks that appear in both result sets are boosted.
    pub fn hybrid_search(
        &self,
        query_text: &str,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<ChunkMatch>> {
        if k == 0 {
            return Ok(Vec::new());
        }

        let fetch_k = k.saturating_mul(2).max(10);

        // 1. FTS5 results
        let fts_results = self.fts_search(query_text, fetch_k)?;
        let fts_list: Vec<(String, f64)> = fts_results
            .iter()
            .map(|c| (c.id.to_string(), c.score as f64))
            .collect();

        // 2. ANN results
        let ann_results = self.ann_search(query_embedding, fetch_k)?;
        let ann_list: Vec<(String, f64)> = ann_results
            .iter()
            .map(|c| (c.id.to_string(), c.score as f64))
            .collect();

        if fts_list.is_empty() && ann_list.is_empty() {
            return Ok(Vec::new());
        }

        // 3. RRF fusion
        let fused = rrf_fuse(&[fts_list, ann_list], 60.0, k);

        // 4. Build a lookup map (id → ChunkMatch) from both result sets
        let mut lookup: std::collections::HashMap<i64, ChunkMatch> =
            std::collections::HashMap::new();
        for c in fts_results.into_iter().chain(ann_results) {
            lookup.entry(c.id).or_insert(c);
        }

        // 5. Return top-k in RRF order with the fused RRF score
        let mut out = Vec::with_capacity(fused.len());
        for (id_str, rrf_score) in fused {
            if let Ok(id) = id_str.parse::<i64>()
                && let Some(mut chunk) = lookup.remove(&id)
            {
                chunk.score = rrf_score as f32;
                out.push(chunk);
            }
        }
        Ok(out)
    }
}

/// Decode a stored embedding blob back to `Vec<f32>`.
///
/// Bytes are stored as little-endian f32 (4 bytes per value).
/// Incomplete trailing bytes are silently discarded.
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

/// Simple FNV-1a hash for chunk dedup (not cryptographic).
fn fnv_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Statistics about recall database.
#[derive(Debug, Clone)]
pub struct RecallStats {
    /// Total number of stored chunks.
    pub total_chunks: i64,
    /// Number of chunks that have an embedding.
    pub chunks_with_embeddings: i64,
    /// Number of rows in the FTS5 index.
    pub total_fts_rows: i64,
    /// Configured embedding dimension.
    pub embedding_dim: usize,
}

/// Fuse multiple ranked result lists using Reciprocal Rank Fusion.
///
/// RRF(doc) = Σ 1/(k + rank_i) where rank_i is the 0-indexed position
/// in each ranked list. This combines FTS5 text search and embedding-based
/// search into a single ranked list that benefits from both signal types.
///
/// # Arguments
///
/// * `ranked_lists` — Each inner Vec is a ranked list of `(doc_id, score)`.
///   The original scores are ignored; only position (rank) matters.
/// * `k` — RRF constant (standard default is 60.0). Higher values dampen
///   the effect of high ranks.
/// * `top_n` — Maximum number of results to return.
///
/// # Returns
///
/// A Vec of `(doc_id, rrf_score)` sorted by RRF score descending,
/// truncated to `top_n`.
#[must_use]
pub fn rrf_fuse(ranked_lists: &[Vec<(String, f64)>], k: f64, top_n: usize) -> Vec<(String, f64)> {
    use std::collections::HashMap;

    let mut scores: HashMap<&str, f64> = HashMap::new();

    for list in ranked_lists {
        for (rank, (doc_id, _score)) in list.iter().enumerate() {
            *scores.entry(doc_id.as_str()).or_insert(0.0) += 1.0 / (k + rank as f64);
        }
    }

    let mut result: Vec<(String, f64)> = scores
        .into_iter()
        .map(|(id, score)| (id.to_string(), score))
        .collect();

    // Sort by RRF score descending, break ties alphabetically by doc_id
    result.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    result.truncate(top_n);
    result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;
    use tempfile::TempDir;

    // ── S4.1: RRF Fusion tests ──────────────────────────────────────

    #[test]
    fn test_rrf_single_list_preserves_order() {
        let list = vec![
            ("doc_a".to_string(), 0.9),
            ("doc_b".to_string(), 0.7),
            ("doc_c".to_string(), 0.5),
        ];
        let result = rrf_fuse(&[list], 60.0, 10);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].0, "doc_a");
        assert_eq!(result[1].0, "doc_b");
        assert_eq!(result[2].0, "doc_c");

        // Verify scores: 1/(60+0) > 1/(60+1) > 1/(60+2)
        assert!(result[0].1 > result[1].1);
        assert!(result[1].1 > result[2].1);

        // Verify exact RRF score for first: 1/(60+0) = 1/60
        let expected = 1.0 / 60.0;
        assert!((result[0].1 - expected).abs() < 1e-10);
    }

    #[test]
    fn test_rrf_two_lists_fuses_correctly() {
        // List 1: A at rank 0, B at rank 1
        let list1 = vec![("doc_a".to_string(), 0.9), ("doc_b".to_string(), 0.7)];
        // List 2: B at rank 0, A at rank 1
        let list2 = vec![("doc_b".to_string(), 0.8), ("doc_a".to_string(), 0.6)];

        let result = rrf_fuse(&[list1, list2], 60.0, 10);

        assert_eq!(result.len(), 2);

        // Both A and B appear in both lists at symmetric positions
        // A: 1/(60+0) + 1/(60+1) = 1/60 + 1/61
        // B: 1/(60+1) + 1/(60+0) = 1/61 + 1/60
        // Scores should be equal, tie-break alphabetically: doc_a first
        assert!((result[0].1 - result[1].1).abs() < 1e-10);
        assert_eq!(result[0].0, "doc_a");
        assert_eq!(result[1].0, "doc_b");
    }

    #[test]
    fn test_rrf_handles_disjoint_lists() {
        let list1 = vec![("doc_a".to_string(), 1.0)];
        let list2 = vec![("doc_b".to_string(), 1.0)];

        let result = rrf_fuse(&[list1, list2], 60.0, 10);

        assert_eq!(result.len(), 2);
        // Both appear only once at rank 0: 1/(60+0) = 1/60
        assert!((result[0].1 - result[1].1).abs() < 1e-10);
        // Tied scores, alphabetical: doc_a before doc_b
        assert_eq!(result[0].0, "doc_a");
        assert_eq!(result[1].0, "doc_b");
    }

    #[test]
    fn test_rrf_empty_input() {
        let result = rrf_fuse(&[], 60.0, 10);
        assert!(result.is_empty());

        // Empty inner lists
        let empty_list: Vec<(String, f64)> = vec![];
        let result = rrf_fuse(&[empty_list], 60.0, 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rrf_top_n_truncation() {
        let list = vec![
            ("a".to_string(), 1.0),
            ("b".to_string(), 0.9),
            ("c".to_string(), 0.8),
            ("d".to_string(), 0.7),
        ];
        let result = rrf_fuse(&[list], 60.0, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "a");
        assert_eq!(result[1].0, "b");
    }

    #[test]
    fn test_rrf_three_lists_boosted_doc() {
        // doc_x appears in all 3 lists at rank 0 — should have highest score
        let list1 = vec![("doc_x".to_string(), 1.0), ("doc_y".to_string(), 0.5)];
        let list2 = vec![("doc_x".to_string(), 1.0), ("doc_z".to_string(), 0.5)];
        let list3 = vec![("doc_x".to_string(), 1.0), ("doc_w".to_string(), 0.5)];

        let result = rrf_fuse(&[list1, list2, list3], 60.0, 5);

        assert_eq!(result[0].0, "doc_x");
        // doc_x: 3 * 1/(60+0) = 3/60 = 0.05
        let expected = 3.0 / 60.0;
        assert!((result[0].1 - expected).abs() < 1e-10);
    }

    // ── Existing recall tests ───────────────────────────────────────

    #[test]
    fn test_store_and_get_chunk() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_recall.db");
        let recall = SemanticRecall::new(&db_path, 384).unwrap();

        let id = recall
            .store_chunk("This is test content about Rust", None, None)
            .unwrap();
        assert!(id > 0);

        let chunk = recall.get_chunk(id).unwrap().unwrap();
        assert_eq!(chunk.content, "This is test content about Rust");
    }

    #[test]
    fn test_dimension_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_recall.db");
        let recall = SemanticRecall::new(&db_path, 384).unwrap();

        let wrong_emb = vec![1.0; 100];
        let result = recall.store_chunk("Test", Some(&wrong_emb), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_touring_schema_idempotent() {
        // Calling ensure_schema twice must not error (covers the ADD COLUMN idempotency path).
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("idem.db");
        let recall = SemanticRecall::new(&db_path, 384).unwrap();
        // Calling the private method again via a second open on the same file
        let recall2 = SemanticRecall::new(&db_path, 384).unwrap();
        let stats = recall2.stats().unwrap();
        // DB previously written by recall must be visible
        let _ = recall.store_chunk("idempotency probe", None, None).unwrap();
        let stats2 = recall2.stats().unwrap();
        assert_eq!(stats2.total_chunks, stats.total_chunks + 1);
    }

    #[cfg(feature = "u4-quantization")]
    #[test]
    fn test_store_chunk_writes_u4_columns() {
        use rusqlite::Connection;
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("u4_test.db");
        let recall = SemanticRecall::new(&db_path, 4).unwrap();

        let emb: Vec<f32> = vec![0.1, 0.5, 0.9, 0.3];
        let id = recall
            .store_chunk("u4 test chunk", Some(&emb), None)
            .unwrap();
        assert!(id > 0);

        // Verify u4 columns were written
        let conn = Connection::open(&db_path).unwrap();
        let (u4_blob, scale, zero): (Option<Vec<u8>>, Option<f64>, Option<f64>) = conn
            .query_row(
                "SELECT embedding_u4, quant_scale, quant_zero FROM chunks WHERE id = ?1",
                rusqlite::params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert!(u4_blob.is_some(), "embedding_u4 must be written");
        assert!(scale.is_some(), "quant_scale must be written");
        assert!(zero.is_some(), "quant_zero must be written");
        // scale and zero must be finite numbers
        assert!(scale.unwrap().is_finite());
        assert!(zero.unwrap().is_finite());
    }

    /// E2E: store_chunk writes u4 → ann_search_u4 reads u4 and returns results.
    /// Proves the full store→search pipeline works with quantized embeddings.
    #[cfg(feature = "u4-quantization")]
    #[test]
    fn test_ann_search_u4_finds_stored_chunks() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("u4_search_test.db");
        let recall = SemanticRecall::new(&db_path, 4).unwrap();

        // Store 3 chunks with embeddings
        let e1 = vec![1.0, 0.0, 0.0, 0.0]; // pointing in dim 0
        let e2 = vec![0.0, 1.0, 0.0, 0.0]; // pointing in dim 1
        let e3 = vec![0.9, 0.1, 0.0, 0.0]; // close to e1

        recall
            .store_chunk("chunk about rust", Some(&e1), None)
            .unwrap();
        recall
            .store_chunk("chunk about python", Some(&e2), None)
            .unwrap();
        recall
            .store_chunk("chunk about cargo", Some(&e3), None)
            .unwrap();

        // Search with query close to e1 — should find "rust" and "cargo" first
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let results = recall.ann_search(&query, 3).unwrap();

        assert_eq!(results.len(), 3, "should return all 3 chunks");

        // First result should be "rust" (exact match to query)
        assert!(
            results[0].content.contains("rust"),
            "first result should be 'rust' (closest to query), got: {}",
            results[0].content
        );

        // Second should be "cargo" (close to query)
        assert!(
            results[1].content.contains("cargo"),
            "second result should be 'cargo' (next closest), got: {}",
            results[1].content
        );
    }

    /// Prove ann_search falls back to f32 when no u4 embeddings exist.
    #[cfg(feature = "u4-quantization")]
    #[test]
    fn test_ann_search_fallback_to_f32_when_no_u4() {
        use rusqlite::Connection;
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("fallback_test.db");

        // Create recall and store chunks, then DELETE the u4 columns
        let recall = SemanticRecall::new(&db_path, 4).unwrap();
        recall
            .store_chunk("fallback chunk", Some(&[0.5, 0.5, 0.5, 0.5]), None)
            .unwrap();

        // Null out u4 to force fallback
        let conn = Connection::open(&db_path).unwrap();
        conn.execute("UPDATE chunks SET embedding_u4 = NULL", [])
            .unwrap();
        drop(conn);

        // Reopen and search — should fall back to f32 path
        let recall2 = SemanticRecall::new(&db_path, 4).unwrap();
        let results = recall2.ann_search(&[0.5, 0.5, 0.5, 0.5], 1).unwrap();

        assert_eq!(
            results.len(),
            1,
            "fallback to f32 should still find the chunk"
        );
        assert!(results[0].content.contains("fallback"));
    }
}
