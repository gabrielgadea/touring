//! Wave M1 (reescopado) — TF-IDF retriever sobre touring memory + decompose corpus.
//!
//! **Why this exists**: the original Wave M plan called for a TF-IDF index over
//! `git log` history. Hard Rule #11 (CLAUDE.md) forbids any git invocation, so
//! this implementation indexes the touring-native equivalents instead:
//!
//! - `memory_entries` (key + value)        ← lessons, patterns, insights
//! - `task_decompositions` (description)   ← intent + bug + plan history
//! - `decomposition_subtasks` (description) ← subtask granularity
//!
//! These tables together form the "history" the touring daemon owns natively,
//! superseding `git log` for retrospective semantic search.
//!
//! **Why TF-IDF when SQL LIKE + ANN already exist** (Wave 22): the existing
//! `memory_recall_rrf_merge` fuses two rankings (SQL LIKE + ANN hash embedding).
//! TF-IDF adds a **third orthogonal signal** that catches:
//!
//! - Rare/jargon terms (where embeddings underperform — low cosine separation)
//! - Multi-word queries (where LIKE returns either too much or nothing)
//! - Document length normalisation (which neither LIKE nor 64-d hash embeddings give)
//!
//! Wave M2 (this same iteration) extends `memory_recall_rrf_merge` from 2 → N
//! lists; this module supplies the third list.
//!
//! **Zero external deps**: pure std + rusqlite (already a transitive workspace
//! dep). No tantivy/lunr/etc — TF-IDF on ≤10k entries is trivial in-memory.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// One scored hit from a TF-IDF query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfidfHit {
    /// Document key — `memory:<key>`, `decomp:<task_id>`, or `subtask:<subtask_id>`.
    pub key: String,
    /// Original document body (truncated to first 500 chars for transport).
    pub snippet: String,
    /// Cosine similarity ∈ [0.0, 1.0].
    pub score: f32,
    /// Source table (`memory`, `decomp`, `subtask`) — for diagnostic/UI.
    /// Owned `String` rather than `&'static str` so cached indexes
    /// round-trip cleanly through serde.
    pub source: String,
}

/// Errors the retriever surfaces. Conservative variants so consumers can
/// degrade per-source (e.g. `CorpusEmpty` → fall back to SQL-only).
#[derive(Debug, thiserror::Error)]
pub enum TfidfError {
    /// No documents found across any source — index has nothing to score.
    #[error("corpus is empty: no memory_entries / task_decompositions / decomposition_subtasks")]
    CorpusEmpty,
    /// SQLite connection or query failure.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Filesystem I/O for cache.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Cache (de)serialization failure.
    #[error("cache (de)serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Convenience result type for TF-IDF operations, defaulting the error to [`TfidfError`].
pub type Result<T> = std::result::Result<T, TfidfError>;

/// Compact tokenized + scored representation of one indexed document.
///
/// `term_weights` stores `term → tf-idf weight` (sparse — only terms present in
/// this doc). `norm` is the L2 norm of the weight vector, precomputed so cosine
/// similarity at query time is just `dot / (q_norm * doc.norm)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredDoc {
    /// Stable identifier of the source row this document was indexed from.
    pub key: String,
    /// Owned source label (`"memory"`, `"decomp"`, `"subtask"`); owned so the
    /// struct can round-trip through serde for cache persistence.
    pub source: String,
    /// Short excerpt of the original text, returned with query results for display.
    pub snippet: String,
    /// Sparse `term → tf-idf weight` map holding only terms present in this document.
    pub term_weights: BTreeMap<String, f32>,
    /// Precomputed L2 norm of the weight vector, used to normalize cosine similarity.
    pub norm: f32,
}

/// In-memory TF-IDF index. `idf` is the inverse-document-frequency for every
/// term seen in the corpus; `docs` are the per-document scored vectors.
///
/// Build via [`TfidfIndex::build_from_db`] (live SQLite scan) or
/// [`TfidfIndex::load_cache`] (deserialize a frozen snapshot).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TfidfIndex {
    /// Inverse-document-frequency weight for every term seen across the corpus.
    pub idf: HashMap<String, f32>,
    /// Per-document scored vectors that make up the searchable corpus.
    pub docs: Vec<ScoredDoc>,
    /// Wall-clock seconds since UNIX epoch when this index was built.
    pub built_at_secs: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tokenization (zero-dep)
// ─────────────────────────────────────────────────────────────────────────────

/// Conservative English+code stopword list. Intentionally small — TF-IDF
/// down-weights frequent terms automatically; a huge list mostly harms recall.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "into", "are", "was", "were", "has",
    "have", "had", "but", "not", "you", "your", "they", "them", "their", "its", "our", "all",
    "any", "can", "will", "would", "should", "could", "may", "might", "must", "use", "used", "via",
    "per",
];

/// Tokenize a document into lowercase, alphanumeric-only, length≥3, non-stopword
/// tokens. Idempotent: same input → same output.
#[must_use]
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for low in ch.to_lowercase() {
                buf.push(low);
            }
        } else if !buf.is_empty() {
            push_if_relevant(&mut out, &buf);
            buf.clear();
        }
    }
    if !buf.is_empty() {
        push_if_relevant(&mut out, &buf);
    }
    out
}

fn push_if_relevant(out: &mut Vec<String>, candidate: &str) {
    if candidate.len() < 3 {
        return;
    }
    if STOPWORDS.contains(&candidate) {
        return;
    }
    out.push(candidate.to_string());
}

// ─────────────────────────────────────────────────────────────────────────────
// Index build
// ─────────────────────────────────────────────────────────────────────────────

/// Raw document tuple read from SQLite, before tokenization.
struct RawDoc {
    key: String,
    text: String,
    source: &'static str,
}

impl TfidfIndex {
    /// Build a fresh index from the touring SQLite databases.
    ///
    /// `memory_db` and `knowledge_db` may be the same file or distinct — the
    /// implementation opens each separately so a missing/empty db on one side
    /// does not prevent indexing the other. Errors only surface when *both*
    /// sources are unreachable AND the corpus ends up empty.
    pub fn build_from_db(memory_db: &Path, knowledge_db: &Path) -> Result<Self> {
        let mut raws: Vec<RawDoc> = Vec::new();
        collect_memory(memory_db, &mut raws);
        collect_decompose(knowledge_db, &mut raws);

        if raws.is_empty() {
            return Err(TfidfError::CorpusEmpty);
        }

        Ok(Self::build_from_raw(raws))
    }

    /// Build from already-collected raw documents — testable without SQLite.
    fn build_from_raw(raws: Vec<RawDoc>) -> Self {
        // First pass: tokenize every doc + accumulate document frequency.
        let mut tokenized: Vec<(RawDoc, Vec<String>)> = Vec::with_capacity(raws.len());
        let mut df: HashMap<String, u32> = HashMap::new();
        for raw in raws {
            let tokens = tokenize(&raw.text);
            let mut seen_in_doc: HashMap<&str, ()> = HashMap::new();
            for tok in &tokens {
                if seen_in_doc.insert(tok.as_str(), ()).is_none() {
                    *df.entry(tok.clone()).or_insert(0) += 1;
                }
            }
            tokenized.push((raw, tokens));
        }

        // Second pass: compute IDF (with smoothing: log(1 + N/df)).
        let n_docs = tokenized.len() as f32;
        let mut idf: HashMap<String, f32> = HashMap::with_capacity(df.len());
        for (term, df_count) in &df {
            let raw_idf = (1.0 + n_docs / (*df_count as f32)).ln();
            idf.insert(term.clone(), raw_idf);
        }

        // Third pass: compute per-doc TF-IDF weights + L2 norm.
        let mut docs: Vec<ScoredDoc> = Vec::with_capacity(tokenized.len());
        for (raw, tokens) in tokenized {
            let mut tf: HashMap<String, u32> = HashMap::new();
            for tok in &tokens {
                *tf.entry(tok.clone()).or_insert(0) += 1;
            }
            let doc_len = tokens.len() as f32;
            let mut weights: BTreeMap<String, f32> = BTreeMap::new();
            for (term, count) in tf {
                let term_idf = idf.get(&term).copied().unwrap_or(0.0);
                if term_idf <= 0.0 {
                    continue;
                }
                let tf_norm = (count as f32) / doc_len.max(1.0);
                weights.insert(term, tf_norm * term_idf);
            }
            let norm = l2_norm(weights.values().copied());
            docs.push(ScoredDoc {
                key: raw.key,
                source: raw.source.to_string(),
                snippet: truncate_snippet(&raw.text, 500),
                term_weights: weights,
                norm,
            });
        }

        Self {
            idf,
            docs,
            built_at_secs: now_secs(),
        }
    }

    /// Score the index against a free-text query. Returns up to `top_k` hits
    /// ordered by descending cosine similarity. Empty queries → empty result.
    #[must_use]
    pub fn query(&self, raw_query: &str, top_k: usize) -> Vec<TfidfHit> {
        if raw_query.trim().is_empty() || self.docs.is_empty() {
            return Vec::new();
        }

        // Build query weight vector (TF × IDF, only terms present in the corpus).
        let q_tokens = tokenize(raw_query);
        if q_tokens.is_empty() {
            return Vec::new();
        }
        let q_len = q_tokens.len() as f32;
        let mut q_tf: HashMap<&str, u32> = HashMap::new();
        for tok in &q_tokens {
            *q_tf.entry(tok.as_str()).or_insert(0) += 1;
        }
        let mut q_weights: BTreeMap<String, f32> = BTreeMap::new();
        for (term, count) in q_tf {
            let term_idf = self.idf.get(term).copied().unwrap_or(0.0);
            if term_idf <= 0.0 {
                continue;
            }
            q_weights.insert(term.to_string(), (count as f32) / q_len * term_idf);
        }
        if q_weights.is_empty() {
            return Vec::new();
        }
        let q_norm = l2_norm(q_weights.values().copied());

        // Score each document via cosine similarity.
        let mut scored: Vec<TfidfHit> = self
            .docs
            .iter()
            .filter_map(|doc| {
                if doc.norm <= 0.0 {
                    return None;
                }
                let mut dot = 0.0_f32;
                for (term, qw) in &q_weights {
                    if let Some(dw) = doc.term_weights.get(term) {
                        dot += qw * dw;
                    }
                }
                if dot <= 0.0 {
                    return None;
                }
                let cos = dot / (q_norm * doc.norm).max(f32::EPSILON);
                Some(TfidfHit {
                    key: doc.key.clone(),
                    snippet: doc.snippet.clone(),
                    score: cos.clamp(0.0, 1.0),
                    source: doc.source.clone(),
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        scored
    }

    /// Persist the index to a JSON cache file. Atomic via tempfile + rename.
    pub fn save_cache(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec(self)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Load a previously cached index. Returns `Ok(None)` for missing/stale
    /// files (`max_age_secs` window applies); corrupt files surface as error.
    pub fn load_cache(path: &Path, max_age_secs: u64) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        if let Ok(meta) = fs::metadata(path)
            && let Ok(age) = meta.modified().and_then(|m| {
                m.duration_since(SystemTime::UNIX_EPOCH)
                    .map_err(|e| std::io::Error::other(e))
            })
        {
            let age_secs = now_secs().saturating_sub(age.as_secs());
            if age_secs > max_age_secs {
                return Ok(None);
            }
        }
        let bytes = fs::read(path)?;
        let idx: TfidfIndex = serde_json::from_slice(&bytes)?;
        Ok(Some(idx))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SQLite collectors (best-effort — missing tables/dbs do not abort)
// ─────────────────────────────────────────────────────────────────────────────

fn collect_memory(memory_db: &Path, out: &mut Vec<RawDoc>) {
    let conn = match Connection::open(memory_db) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(target: "touring::tfidf", "memory db open failed: {e}");
            return;
        }
    };
    let mut stmt = match conn
        .prepare("SELECT key, value FROM memory_entries WHERE value IS NOT NULL AND value != ''")
    {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(target: "touring::tfidf", "memory_entries prepare failed: {e}");
            return;
        }
    };
    let rows = stmt.query_map([], |row| {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        Ok((key, value))
    });
    if let Ok(iter) = rows {
        for r in iter.flatten() {
            let (key, value) = r;
            let combined = format!("{key} {value}");
            out.push(RawDoc {
                key: format!("memory:{key}"),
                text: combined,
                source: "memory",
            });
        }
    }
}

fn collect_decompose(knowledge_db: &Path, out: &mut Vec<RawDoc>) {
    let conn = match Connection::open(knowledge_db) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(target: "touring::tfidf", "knowledge db open failed: {e}");
            return;
        }
    };
    collect_decompose_table(
        &conn,
        "SELECT task_id, description FROM task_decompositions WHERE description IS NOT NULL",
        "decomp",
        out,
    );
    collect_decompose_table(
        &conn,
        "SELECT subtask_id, description FROM decomposition_subtasks WHERE description IS NOT NULL",
        "subtask",
        out,
    );
}

fn collect_decompose_table(
    conn: &Connection,
    sql: &str,
    source: &'static str,
    out: &mut Vec<RawDoc>,
) {
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let desc: String = row.get(1)?;
        Ok((id, desc))
    });
    if let Ok(iter) = rows {
        for (id, desc) in iter.flatten() {
            out.push(RawDoc {
                key: format!("{source}:{id}"),
                text: desc,
                source,
            });
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cache helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Default cache window: 1 hour. Invalidated by `--rebuild` flag at the CLI edge.
pub const CACHE_TTL_SECS: u64 = 60 * 60;

/// Resolve the canonical cache file under the touring cache root.
#[must_use]
pub fn default_cache_path(workspace: &Path) -> PathBuf {
    workspace
        .join(".touring-cache")
        .join("tfidf-memory")
        .join("index.json")
}

// ─────────────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────────────

fn l2_norm<I: IntoIterator<Item = f32>>(values: I) -> f32 {
    let sum: f32 = values.into_iter().map(|v| v * v).sum();
    sum.sqrt()
}

fn truncate_snippet(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn raw(key: &str, text: &str, source: &'static str) -> RawDoc {
        RawDoc {
            key: key.to_string(),
            text: text.to_string(),
            source,
        }
    }

    #[test]
    fn tokenize_lowercases_and_drops_short() {
        let toks = tokenize("Hello, World! AB cdefg");
        assert_eq!(toks, vec!["hello", "world", "cdefg"]);
    }

    #[test]
    fn tokenize_drops_stopwords() {
        let toks = tokenize("The cat sat on the mat with patience");
        assert!(!toks.contains(&"the".to_string()));
        assert!(!toks.contains(&"with".to_string()));
        assert!(toks.contains(&"patience".to_string()));
    }

    #[test]
    fn tokenize_preserves_alphanumeric_runs() {
        let toks = tokenize("blake3 hash and tfidf-retriever");
        assert!(toks.contains(&"blake3".to_string()));
        assert!(toks.contains(&"tfidf".to_string()));
        assert!(toks.contains(&"retriever".to_string()));
    }

    #[test]
    fn build_index_assigns_idf_to_every_term() {
        let raws = vec![
            raw("a", "rust async patterns help", "memory"),
            raw("b", "async runtime tokio scheduler", "memory"),
            raw("c", "graph algorithms for wiring", "decomp"),
        ];
        let idx = TfidfIndex::build_from_raw(raws);
        assert!(idx.idf.contains_key("rust"));
        assert!(idx.idf.contains_key("async"));
        // "async" appears in 2/3 docs → lower IDF than "rust" (1/3 docs).
        assert!(idx.idf["rust"] > idx.idf["async"]);
    }

    #[test]
    fn query_returns_top_match() {
        let raws = vec![
            raw("a", "rust async runtime tokio", "memory"),
            raw("b", "graph wiring orphan symbols", "memory"),
            raw("c", "tfidf retriever cosine similarity", "decomp"),
        ];
        let idx = TfidfIndex::build_from_raw(raws);
        let hits = idx.query("tfidf cosine", 5);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].key, "c");
        assert!(hits[0].score > 0.0);
        assert_eq!(hits[0].source, "decomp");
    }

    #[test]
    fn query_empty_returns_empty() {
        let raws = vec![raw("a", "anything", "memory")];
        let idx = TfidfIndex::build_from_raw(raws);
        assert!(idx.query("", 5).is_empty());
        assert!(idx.query("   ", 5).is_empty());
    }

    #[test]
    fn query_no_overlap_returns_empty() {
        let raws = vec![raw("a", "alpha beta gamma", "memory")];
        let idx = TfidfIndex::build_from_raw(raws);
        // Query tokens absent from corpus → no hits.
        assert!(idx.query("zebra octopus", 5).is_empty());
    }

    #[test]
    fn query_top_k_caps_results() {
        let raws: Vec<RawDoc> = (0..20)
            .map(|i| raw(&format!("k{i}"), "shared common token corpus", "memory"))
            .collect();
        let idx = TfidfIndex::build_from_raw(raws);
        let hits = idx.query("shared common", 5);
        assert!(hits.len() <= 5);
    }

    #[test]
    fn snippet_truncated_with_ellipsis() {
        let long = "x".repeat(800);
        let raws = vec![raw("a", &long, "memory")];
        let idx = TfidfIndex::build_from_raw(raws);
        let s = &idx.docs[0].snippet;
        // 500 'x' + 1 ellipsis char.
        assert_eq!(s.chars().count(), 501);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn cache_save_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cache.json");
        let raws = vec![
            raw("a", "rust async runtime", "memory"),
            raw("b", "graph wiring orphan", "decomp"),
        ];
        let idx = TfidfIndex::build_from_raw(raws);
        idx.save_cache(&path).unwrap();
        let loaded = TfidfIndex::load_cache(&path, 3600).unwrap().unwrap();
        assert_eq!(loaded.docs.len(), idx.docs.len());
        assert_eq!(loaded.idf.len(), idx.idf.len());
        // Query should produce identical top hit.
        let h1 = idx.query("rust async", 1);
        let h2 = loaded.query("rust async", 1);
        assert_eq!(h1, h2);
    }

    #[test]
    fn cache_load_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("absent.json");
        assert!(TfidfIndex::load_cache(&path, 3600).unwrap().is_none());
    }

    #[test]
    fn cache_load_stale_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("stale.json");
        let idx = TfidfIndex::build_from_raw(vec![raw("a", "anything", "memory")]);
        idx.save_cache(&path).unwrap();
        // Wait long enough for the file to be at least 2 seconds old.
        // (filetime crate is not a workspace dep — sleeping is the portable
        // way to exercise the staleness branch without mocking SystemTime.)
        std::thread::sleep(std::time::Duration::from_millis(2100));
        // max_age=1 → file age (≥2s) > max_age → must be reported stale.
        let loaded = TfidfIndex::load_cache(&path, 1).unwrap();
        assert!(
            loaded.is_none(),
            "2s-old cache should be stale at max_age=1s"
        );
    }

    #[test]
    fn build_from_db_empty_returns_corpus_empty() {
        let dir = TempDir::new().unwrap();
        // Two empty paths → both `collect_*` no-op → CorpusEmpty.
        let mem = dir.path().join("memory.db");
        let kn = dir.path().join("knowledge.db");
        let err = TfidfIndex::build_from_db(&mem, &kn).unwrap_err();
        assert!(matches!(err, TfidfError::CorpusEmpty));
    }

    #[test]
    fn build_from_db_with_real_memory_entries() {
        let dir = TempDir::new().unwrap();
        let mem = dir.path().join("memory.db");
        {
            let conn = Connection::open(&mem).unwrap();
            conn.execute(
                "CREATE TABLE memory_entries (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    tier TEXT NOT NULL DEFAULT 'local',
                    entry_type TEXT NOT NULL DEFAULT 'insight',
                    access_count INTEGER NOT NULL DEFAULT 0,
                    last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO memory_entries (key, value) VALUES (?1, ?2)",
                rusqlite::params!["lesson:tfidf", "Use TF-IDF for rare-term retrieval queries"],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO memory_entries (key, value) VALUES (?1, ?2)",
                rusqlite::params!["lesson:wiring", "Always grep before claiming orphan"],
            )
            .unwrap();
        }
        // knowledge_db absent (no decompose tables) — should not abort.
        let kn = dir.path().join("knowledge.db");
        let idx = TfidfIndex::build_from_db(&mem, &kn).unwrap();
        assert_eq!(idx.docs.len(), 2);
        let hits = idx.query("tfidf retrieval", 1);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].key, "memory:lesson:tfidf");
        assert_eq!(hits[0].source, "memory");
    }

    #[test]
    fn default_cache_path_under_workspace() {
        let p = default_cache_path(Path::new("/ws"));
        assert_eq!(
            p,
            PathBuf::from("/ws/.touring-cache/tfidf-memory/index.json")
        );
    }

    #[test]
    fn l2_norm_basic() {
        let n = l2_norm(vec![3.0_f32, 4.0_f32]);
        assert!((n - 5.0).abs() < f32::EPSILON);
    }
}
