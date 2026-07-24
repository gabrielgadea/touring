//! Tantivy FTS integration for touring-hooks.
//!
//! Provides full-text search over symbols using Tantivy's BM25 ranking.
//! Feature-gated under `tantivy-fts`. Additive to existing FTS5 SQLite index.
//!
//! # Architecture
//! - `TantivyIndex` wraps a Tantivy index with a batch-commit policy.
//! - Documents are upserted by `(file_path, symbol_name)` — delete-then-add.
//! - Auto-commit triggers when `pending_count >= batch_size` (default 500).
//!
//! # Schema (16 fields — v3)
//! Core 7: symbol_name, file_path, symbol_kind, module_path,
//!   docstring (FTS), line_number (fast u64), language (keyword).
//! New 8: visibility, crate_name, blake3_hash, import_count, export_count,
//!   cognitive_score_x1000, functional_signature (FTS).
//! New 1: community_id (fast u64) — Louvain/Leiden community assignment for
//!   topological RRF boost during community-aware search.
//!
//! # IMPORTANT: Schema Compatibility
//! This module uses schema v3 (16 fields). Indexes created with v2 (15 fields)
//! or v1 (7 fields) are INCOMPATIBLE and must be deleted and rebuilt. The schema
//! version is encoded in the field set — opening an older index with v3 code will
//! cause field lookup failures. Delete the index directory and run
//! `touring tantivy reindex` when upgrading.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use tantivy::schema::FAST;
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
};
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument, Term};

use touring_foundation::char_classes::{CharClass, CharClasses};

// ─── Module-level helpers ───────────────────────────────────────────────────

/// Returns only the `CharClass::Code` content from `source`, skipping all
/// string-literal, comment, raw-string, and doc-comment regions.
///
/// Used to pre-filter FTS-indexed fields so that string contents embedded
/// in docstrings or signatures do not pollute the search index.
fn code_only(source: &str) -> String {
    let chars = CharClasses::new(source);
    let mut out = String::with_capacity(source.len());
    let mut emitting = false;
    for (_, ch, class) in chars {
        match (emitting, class) {
            (false, CharClass::Code) => {
                emitting = true;
                out.push(ch);
            }
            (false, _) => {
                // skip — not yet in code
            }
            (true, CharClass::Code) => {
                out.push(ch);
            }
            (true, _) => {
                // transition out of code — emit a space so word boundaries
                // between code and non-code don't merge (e.g. `"fn"x` → "fn x")
                out.push(' ');
                emitting = false;
            }
        }
    }
    out
}

// ─── Public Types ────────────────────────────────────────────────────────────

/// A symbol to index in Tantivy (schema v3 — 16 fields).
///
/// All new fields (beyond the original 7) are `Option<_>` so existing callers
/// compile without modification. Pass `None` to omit a field from the index.
#[derive(Debug, Clone)]
pub struct SymbolDoc {
    // Original 7 fields
    /// The symbol's identifier (function/struct/etc. name).
    pub symbol_name: String,
    /// Path of the file that defines the symbol.
    pub file_path: String,
    /// Kind of symbol (e.g. `"fn"`, `"struct"`, `"trait"`).
    pub symbol_kind: String,
    /// Fully-qualified module path, if known.
    pub module_path: Option<String>,
    /// Leading doc comment text, if any.
    pub docstring: Option<String>,
    /// 1-based line number of the symbol's definition.
    pub line_number: u64,
    /// Source language of the symbol (e.g. `"rust"`, `"python"`).
    pub language: String,
    // New 8 fields (schema v2)
    /// Visibility modifier: "pub", "pub(crate)", "pub(super)", or "" for private.
    pub visibility: Option<String>,
    /// The crate that defines this symbol, e.g. "touring-hooks".
    pub crate_name: Option<String>,
    /// BLAKE3 content hash for deduplication and change detection.
    pub blake3_hash: Option<String>,
    /// Number of import statements in the containing file.
    pub import_count: Option<u64>,
    /// Number of public exports in the containing file.
    pub export_count: Option<u64>,
    /// Cognitive complexity score, 0.0–1.0. Stored as u64 * 1000 in the index.
    pub cognitive_score: Option<f64>,
    /// Normalized function signature, e.g. `fn(A, B) -> C`. Searchable via FTS.
    pub functional_signature: Option<String>,
    // New 1 field (schema v3)
    /// Louvain/Leiden community assignment. Applied as RRF boost during search.
    pub community_id: Option<u64>,
}

/// A single BM25-ranked search result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    // Original fields
    /// The matched symbol's identifier.
    pub symbol_name: String,
    /// Path of the file containing the match.
    pub file_path: String,
    /// Kind of the matched symbol (e.g. `"fn"`, `"struct"`).
    pub symbol_kind: String,
    /// 1-based line number of the match.
    pub line_number: u64,
    /// BM25 relevance score (higher is more relevant).
    pub score: f32,
    // Additional fields from schema v2
    /// Defining crate of the matched symbol, if known.
    pub crate_name: Option<String>,
    /// Visibility modifier of the matched symbol, if known.
    pub visibility: Option<String>,
    /// Normalized function signature of the matched symbol, if available.
    pub functional_signature: Option<String>,
    /// Cognitive complexity score, 0.0–1.0 (converted from u64 x1000).
    pub cognitive_score: Option<f64>,
    // Schema v3 field
    /// Community ID of the symbol's containing file. Used for RRF boost.
    pub community_id: Option<u64>,
}

/// Snapshot of index health metrics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexStats {
    /// Total number of documents currently in the index.
    pub total_docs: u64,
    /// On-disk size of the index in bytes.
    pub index_size_bytes: u64,
    /// Number of operations buffered but not yet committed.
    pub pending_ops: u32,
    /// Cumulative count of commits performed.
    pub total_commits: u64,
    /// Cumulative count of symbol upserts performed.
    pub total_upserts: u64,
}

// ─── Schema Field Handles ────────────────────────────────────────────────────

struct SchemaFields {
    // Original 7
    symbol_name: Field,     // en_stem — BM25 search
    symbol_name_raw: Field, // default — fuzzy/prefix search
    file_path: Field,
    symbol_kind: Field,
    module_path: Field,
    docstring: Field,
    line_number: Field,
    language: Field,
    // New 7 (schema v2)
    visibility: Field,
    crate_name: Field,
    blake3_hash: Field,
    import_count: Field,
    export_count: Field,
    /// Cognitive complexity * 1000 as u64 (tantivy 0.22 has no f64 field).
    cognitive_score_x1000: Field,
    functional_signature: Field,
    // New 1 (schema v3)
    /// Louvain community ID for topological RRF boost.
    community_id: Field,
    // I-01 (schema v4) — NgramTokenizer trigram for substring search
    /// Indexed via custom 'trigram_3' tokenizer (see open_or_create).
    /// Enables 'useEff' → 'useEffect' substring match without fuzzy proxy.
    symbol_name_trigram: Field,
    // I-08 (schema v5) — Facet field for hierarchical drill-down navigation
    /// Stores `/Lang/Crate/Kind/Visibility` paths. Enables `FacetCollector`
    /// counts per level + filter-by-prefix searches.
    symbol_facet: Field,
}

fn build_schema() -> (Schema, SchemaFields) {
    use tantivy::schema::SchemaBuilder;

    let mut builder = SchemaBuilder::new();

    // stem + raw dual-index (context-mode P3 architecture)
    // symbol_name_stem: en_stem for BM25 quality (Porter normalization)
    // symbol_name_raw:  default for fuzzy/prefix search (no stemming)
    let text_opts_stem = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let text_opts_raw = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    // ── Original 7 fields ─────────────────────────────────────────────────────
    // stem field: used by BM25 search for morphological quality
    let symbol_name = builder.add_text_field("symbol_name", text_opts_stem.clone());
    // raw field: used by fuzzy_search + suggest for typo-tolerant matching
    let symbol_name_raw = builder.add_text_field("symbol_name_raw", text_opts_raw.clone());
    let file_path = builder.add_text_field("file_path", STRING | STORED);
    let symbol_kind = builder.add_text_field("symbol_kind", STRING | STORED);
    let module_path = builder.add_text_field("module_path", text_opts_stem.clone());
    let docstring = builder.add_text_field("docstring", text_opts_stem.clone());
    let line_number = builder.add_u64_field("line_number", STORED | FAST);
    let language = builder.add_text_field("language", STRING | STORED);

    // ── New 7 fields (schema v2) ───────────────────────────────────────────────
    // String fields (exact match, filterable)
    let visibility = builder.add_text_field("visibility", STRING | STORED | FAST);
    let crate_name = builder.add_text_field("crate_name", STRING | STORED | FAST);
    let blake3_hash = builder.add_text_field("blake3_hash", STRING | STORED);

    // Numeric fields (fast for range queries and sorting)
    let import_count = builder.add_u64_field("import_count", STORED | FAST);
    let export_count = builder.add_u64_field("export_count", STORED | FAST);
    let cognitive_score_x1000 = builder.add_u64_field("cognitive_score_x1000", STORED | FAST);

    // FTS field (searchable full-text)
    let functional_signature = builder.add_text_field("functional_signature", text_opts_stem);

    // ── New 1 field (schema v3) ────────────────────────────────────────────────
    // Fast u64 for range queries and RRF boost filtering by community partition.
    let community_id = builder.add_u64_field("community_id", STORED | FAST);

    // ── New 1 field (schema v4) — I-01 trigram substring search ──────────────
    // Uses custom 'trigram_3' tokenizer (registered in open_or_create).
    // Storing positions enables future PhraseQuery on n-grams.
    let trigram_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("trigram_3")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let symbol_name_trigram = builder.add_text_field("symbol_name_trigram", trigram_opts);

    // ── New 1 field (schema v5) — I-08 facet hierarchy ────────────────────
    // FacetOptions::default() — STORED + INDEXED for FacetCollector queries.
    let symbol_facet =
        builder.add_facet_field("symbol_facet", tantivy::schema::FacetOptions::default());

    let schema = builder.build();
    let fields = SchemaFields {
        symbol_name,
        symbol_name_raw,
        file_path,
        symbol_kind,
        module_path,
        docstring,
        line_number,
        language,
        visibility,
        crate_name,
        blake3_hash,
        import_count,
        export_count,
        cognitive_score_x1000,
        functional_signature,
        community_id,
        symbol_name_trigram,
        symbol_facet,
    };
    (schema, fields)
}

/// I-08 — Builds a hierarchical facet path from a SymbolDoc.
/// Format: `/<lang>/<crate>/<kind>/<visibility>` — each level contributes
/// to drill-down navigation. Empty values fall back to "_unknown".
fn build_symbol_facet(doc: &SymbolDoc) -> tantivy::schema::Facet {
    fn safe(s: &str) -> String {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            "_unknown".to_string()
        } else {
            // Sanitise: facet path forbids '/' inside segments
            trimmed.replace('/', "_")
        }
    }
    let lang = safe(&doc.language);
    let crate_n = doc
        .crate_name
        .as_deref()
        .map(safe)
        .unwrap_or_else(|| "_unknown".into());
    let kind = safe(&doc.symbol_kind);
    let vis = doc
        .visibility
        .as_deref()
        .map(safe)
        .unwrap_or_else(|| "private".into());
    tantivy::schema::Facet::from(&format!("/{lang}/{crate_n}/{kind}/{vis}"))
}

// ─── TantivyIndex ─────────────────────────────────────────────────────────────

/// Tantivy search engine with batch-commit policy.
///
/// Thread-safe: `IndexWriter` is guarded by a `Mutex`. `IndexReader` and
/// all field handles are immutable after construction.
pub struct TantivyIndex {
    index: Index,
    reader: IndexReader,
    writer: std::sync::Mutex<IndexWriter>,
    /// Stored separately — `MmapDirectory::root_path` is private.
    index_dir: PathBuf,
    fields: SchemaFields,
    // Metrics (lock-free counters)
    pending_count: AtomicU32,
    total_upserts: AtomicU64,
    total_commits: AtomicU64,
    /// Auto-commit threshold.
    batch_size: u32,
    last_commit: std::sync::Mutex<Instant>,
    /// BM25 query-result cache (moka W-TinyLFU + TTL). Absorbs duplicate
    /// queries across hooks — same `{op}:{query}:{top_k}` key returns the
    /// cached `Arc<Vec<SearchHit>>` without re-running scoring.
    query_cache: moka::sync::Cache<String, std::sync::Arc<Vec<SearchHit>>>,
    /// Count of cache hits. Counterpart to `gate_metrics::record_tantivy_query_latency`
    /// so operators can compute hit ratio without pulling moka internals.
    query_cache_hits: AtomicU64,
    /// Count of cache misses (i.e. BM25 executions).
    query_cache_misses: AtomicU64,
}

/// Error from [`TantivyIndex`] / [`ToolOutputsIndex`] full-text-search
/// operations (index open, query parse, search, commit, lock poison, …).
///
/// Wraps the contextual failure message; its `Display` is that message
/// verbatim, so consumer logs read identically to the prior `String` errors.
#[derive(Debug, thiserror::Error, serde::Serialize)]
#[error("{0}")]
pub struct TantivyIndexError(String);

impl From<String> for TantivyIndexError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl TantivyIndex {
    /// Open an existing index or create a new one at `path`.
    ///
    /// I-01 — Registers a custom NgramTokenizer (3,3) so the schema field
    /// `symbol_name_trigram` can index real trigrams (substring search) in
    /// addition to the existing en_stem (porter) and default (raw) fields.
    pub fn open_or_create(path: &Path) -> Result<Self, TantivyIndexError> {
        let (schema, fields) = build_schema();
        std::fs::create_dir_all(path).map_err(|e| format!("create_dir_all: {e}"))?;

        let mmap_dir = tantivy::directory::MmapDirectory::open(path)
            .map_err(|e| format!("MmapDirectory: {e}"))?;
        let index = match Index::open_or_create(mmap_dir, schema) {
            Ok(idx) => idx,
            Err(first_err) => {
                // Schema mismatch — on-disk index was built with an older schema version.
                // Delete the stale directory and retry from scratch so the daemon can
                // self-recover without manual intervention.
                tracing::warn!(
                    dir = %path.display(),
                    err = %first_err,
                    "tantivy: open_or_create failed (likely schema mismatch) \
                     — deleting stale index dir and retrying"
                );
                std::fs::remove_dir_all(path)
                    .map_err(|e| format!("remove_dir_all (schema mismatch recovery): {e}"))?;
                std::fs::create_dir_all(path)
                    .map_err(|e| format!("create_dir_all (retry): {e}"))?;
                let (schema2, _) = build_schema();
                let mmap_dir2 = tantivy::directory::MmapDirectory::open(path)
                    .map_err(|e| format!("MmapDirectory (retry): {e}"))?;
                Index::open_or_create(mmap_dir2, schema2).map_err(|e| {
                    format!("Index::open_or_create (retry after schema mismatch): {e}")
                })?
            }
        };

        // I-01: register trigram tokenizer (3,3) on this index. Idempotent —
        // re-registering the same name overwrites with identical config.
        // Wrapped in a TextAnalyzer so it composes with LowerCaser for
        // case-insensitive substring search.
        let trigram = tantivy::tokenizer::TextAnalyzer::builder(
            tantivy::tokenizer::NgramTokenizer::new(3, 3, false)
                .map_err(|e| format!("ngram tokenizer: {e}"))?,
        )
        .filter(tantivy::tokenizer::LowerCaser)
        .build();
        index.tokenizers().register("trigram_3", trigram);

        let writer = index
            .writer(50_000_000)
            .map_err(|e| format!("writer: {e}"))?;
        let reader = index.reader().map_err(|e| format!("reader: {e}"))?;

        Ok(TantivyIndex {
            index_dir: path.to_path_buf(),
            fields,
            index,
            reader,
            writer: std::sync::Mutex::new(writer),
            pending_count: AtomicU32::new(0),
            total_upserts: AtomicU64::new(0),
            total_commits: AtomicU64::new(0),
            batch_size: 500,
            last_commit: std::sync::Mutex::new(Instant::now()),
            query_cache: moka::sync::Cache::new(10_000),
            query_cache_hits: AtomicU64::new(0),
            query_cache_misses: AtomicU64::new(0),
        })
    }

    /// I-01 — Substring search via the dedicated trigram field. Lowercases
    /// the query and runs a TermQuery on each 3-gram, fused via SHOULD.
    /// Returns up to `top_k` hits. Counter `tantivy_trigram_query_count`
    /// advances on each invocation.
    pub fn search_trigram(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<SearchHit>, TantivyIndexError> {
        if query.len() < 3 {
            return Ok(Vec::new());
        }
        crate::shared::gate_metrics::record_tantivy_trigram_query();
        let cache_key = format!("trigram:{query}:{top_k}");
        if let Some(hits) = self.query_cache.get(&cache_key) {
            self.query_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(hits.as_ref().clone());
        }
        self.query_cache_misses.fetch_add(1, Ordering::Relaxed);
        let searcher = self.reader.searcher();
        let lower = query.to_lowercase();
        // Build a SHOULD union of TermQuery for each trigram of the query.
        let mut clauses: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
        let bytes: Vec<char> = lower.chars().collect();
        for window in bytes.windows(3) {
            let gram: String = window.iter().collect();
            let term = tantivy::Term::from_field_text(self.fields.symbol_name_trigram, &gram);
            let q: Box<dyn tantivy::query::Query> = Box::new(tantivy::query::TermQuery::new(
                term,
                tantivy::schema::IndexRecordOption::WithFreqs,
            ));
            clauses.push((tantivy::query::Occur::Should, q));
        }
        if clauses.is_empty() {
            return Ok(Vec::new());
        }
        let bool_q = tantivy::query::BooleanQuery::new(clauses);
        let top_docs = searcher
            .search(&bool_q, &tantivy::collector::TopDocs::with_limit(top_k))
            .map_err(|e| format!("trigram search: {e}"))?;
        let hits: Vec<SearchHit> = top_docs
            .into_iter()
            .filter_map(|(score, addr)| {
                let doc: TantivyDocument = searcher.doc(addr).ok()?;
                Some(self.doc_to_hit(&doc, score))
            })
            .collect();
        self.query_cache
            .insert(cache_key, std::sync::Arc::new(hits.clone()));
        Ok(hits)
    }

    /// Commit all pending writes to disk.
    pub fn commit(&self) -> Result<(), TantivyIndexError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| format!("writer lock: {e}"))?;
        writer.commit().map_err(|e| format!("commit: {e}"))?;
        drop(writer);
        self.reader
            .reload()
            .map_err(|e| format!("reader reload: {e}"))?;
        self.pending_count.store(0, Ordering::Relaxed);
        let mut last = self
            .last_commit
            .lock()
            .map_err(|e| format!("last_commit lock: {e}"))?;
        *last = Instant::now();
        let prev = self.total_commits.fetch_add(1, Ordering::Relaxed);
        crate::shared::gate_metrics::record_tantivy_commit();
        tracing::debug!(total_commits = prev + 1, "tantivy committed");
        Ok(())
    }

    /// Execute a BM25 search query and return the top `top_k` hits.
    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchHit>, TantivyIndexError> {
        let cache_key = format!("search:{query}:{top_k}");
        if let Some(hits) = self.query_cache.get(&cache_key) {
            self.query_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(hits.as_ref().clone());
        }
        self.query_cache_misses.fetch_add(1, Ordering::Relaxed);

        let searcher = self.reader.searcher();
        // I-03 — 5× heading boost: matches no symbol_name pesam mais que
        // matches em docstring, replicando 'title field 5x weighting' do
        // context-mode. Configurável via env TOURING_TANTIVY_NAME_BOOST.
        let name_boost = crate::shared::feature_flags::tantivy_name_boost();
        let mut query_parser = tantivy::query::QueryParser::for_index(
            &self.index,
            vec![
                self.fields.symbol_name,
                self.fields.docstring,
                self.fields.functional_signature,
            ],
        );
        query_parser.set_field_boost(self.fields.symbol_name, name_boost);
        query_parser.set_field_boost(self.fields.functional_signature, 1.5);
        // docstring permanece com boost 1.0 (default)
        let parsed = query_parser
            .parse_query(query)
            .map_err(|e| format!("parse_query: {e}"))?;

        // I-02 — PhraseQuery proximity boost para multi-term queries.
        // Detecta `query.split_whitespace().count() >= 2` e combina BM25
        // plain com PhraseQuery em BooleanQuery::SHOULD union.
        let final_query: Box<dyn tantivy::query::Query> = match self.try_build_phrase_query(query) {
            Some(phrase_q) => {
                let union: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> = vec![
                    (tantivy::query::Occur::Should, parsed),
                    (tantivy::query::Occur::Should, phrase_q),
                ];
                crate::shared::gate_metrics::record_phrase_query_match();
                Box::new(tantivy::query::BooleanQuery::new(union))
            }
            _ => parsed,
        };

        let top_docs = searcher
            .search(
                &final_query,
                &tantivy::collector::TopDocs::with_limit(top_k),
            )
            .map_err(|e| format!("search: {e}"))?;

        let hits: Vec<SearchHit> = top_docs
            .into_iter()
            .filter_map(|(score, doc_address)| {
                let doc: TantivyDocument = searcher.doc(doc_address).ok()?;
                Some(self.doc_to_hit(&doc, score))
            })
            .collect();

        self.query_cache
            .insert(cache_key, std::sync::Arc::new(hits.clone()));
        Ok(hits)
    }

    /// I-02 helper — constrói PhraseQuery em `symbol_name` quando a query
    /// tem ≥ 2 termos. Slop default 2 (env-tunável).
    /// Retorna None para single-term queries (no-op).
    fn try_build_phrase_query(&self, query: &str) -> Option<Box<dyn tantivy::query::Query>> {
        let terms: Vec<&str> = query.split_whitespace().filter(|s| !s.is_empty()).collect();
        if terms.len() < 2 {
            return None;
        }
        let slop = crate::shared::feature_flags::tantivy_phrase_slop();
        let term_objs: Vec<tantivy::Term> = terms
            .iter()
            .map(|t| tantivy::Term::from_field_text(self.fields.symbol_name, &t.to_lowercase()))
            .collect();
        let phrase = tantivy::query::PhraseQuery::new_with_offset_and_slop(
            term_objs.into_iter().enumerate().collect(),
            slop,
        );
        Some(Box::new(phrase))
    }

    /// Execute a fuzzy search allowing up to `distance` edit-distance typos.
    ///
    /// Uses `FuzzyTermQuery` directly with a lowercased term. The default
    /// analyzer lowercases indexed terms (e.g. "HookRuntime" → "hookruntime"),
    /// so we lowercase the query term before creating the raw term for the
    /// fuzzy matcher. This ensures the edit distance is computed between
    /// comparable strings: "hokruntime" vs "hookruntime" = distance 1.
    pub fn fuzzy_search(
        &self,
        query: &str,
        distance: u8,
        top_k: usize,
    ) -> Result<Vec<SearchHit>, TantivyIndexError> {
        let cache_key = format!("fuzzy:{query}:{distance}:{top_k}");
        if let Some(hits) = self.query_cache.get(&cache_key) {
            self.query_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(hits.as_ref().clone());
        }
        self.query_cache_misses.fetch_add(1, Ordering::Relaxed);

        let searcher = self.reader.searcher();
        // The index stores lowercased terms; lowercase the query too so the
        // edit distance is computed between comparable strings.
        let term_text = query.to_lowercase();
        let term = Term::from_field_text(self.fields.symbol_name_raw, &term_text);
        let fuzzy_query = tantivy::query::FuzzyTermQuery::new(term, distance, true);
        let top_docs = searcher
            .search(
                &fuzzy_query,
                &tantivy::collector::TopDocs::with_limit(top_k),
            )
            .map_err(|e| format!("fuzzy search: {e}"))?;

        let hits: Vec<SearchHit> = top_docs
            .into_iter()
            .filter_map(|(score, doc_address)| {
                let doc: TantivyDocument = searcher.doc(doc_address).ok()?;
                Some(self.doc_to_hit(&doc, score))
            })
            .collect();

        self.query_cache
            .insert(cache_key, std::sync::Arc::new(hits.clone()));
        Ok(hits)
    }

    /// I-08 — Counts symbols grouped by facet level under `prefix` (e.g.
    /// `/Rust/touring-hooks` returns counts per `Kind` sub-bucket).
    ///
    /// Returns a map `{full_facet_path: count}`. Empty map if no docs match.
    pub fn count_facets(
        &self,
        prefix: &str,
        max_buckets: usize,
    ) -> Result<Vec<(String, u64)>, TantivyIndexError> {
        use tantivy::collector::FacetCollector;
        let searcher = self.reader.searcher();
        let mut collector = FacetCollector::for_field("symbol_facet");
        collector.add_facet(prefix);
        let counts = searcher
            .search(&tantivy::query::AllQuery, &collector)
            .map_err(|e| format!("facet search: {e}"))?;
        let mut out: Vec<(String, u64)> = counts
            .get(prefix)
            .map(|(facet, count)| (format!("{}", facet), count))
            .take(max_buckets)
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.1));
        Ok(out)
    }

    /// I-07 — Aggregation: count documents grouped by an existing string
    /// field (e.g. `symbol_kind`, `crate_name`, `language`). Returns
    /// `Vec<(value, count)>` sorted descending by count, capped at `max_buckets`.
    ///
    /// Implemented over Tantivy DocSet iteration rather than the full
    /// AggregationCollector JSON schema (which requires `fast` field types
    /// not available on text fields). Bounded by index size (acceptable for
    /// the small symbol corpus).
    pub fn aggregate_terms(
        &self,
        field_name: &str,
        max_buckets: usize,
    ) -> Result<Vec<(String, u64)>, TantivyIndexError> {
        let searcher = self.reader.searcher();
        let field = self
            .index
            .schema()
            .get_field(field_name)
            .map_err(|e| format!("unknown field {field_name}: {e}"))?;
        let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for segment_reader in searcher.segment_readers() {
            let store_reader = segment_reader
                .get_store_reader(0)
                .map_err(|e| format!("store reader: {e}"))?;
            for d in 0..segment_reader.max_doc() {
                if segment_reader.is_deleted(d) {
                    continue;
                }
                if let Ok(doc) = store_reader.get::<TantivyDocument>(d) {
                    let v = doc_str(&doc, field);
                    if !v.is_empty() {
                        *counts.entry(v).or_insert(0) += 1;
                    }
                }
            }
        }
        let mut sorted: Vec<(String, u64)> = counts.into_iter().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.1));
        sorted.truncate(max_buckets);
        Ok(sorted)
    }

    /// P3-TRIG — RRF-fused search combining BM25 (porter stem) with
    /// fuzzy edit-distance results.
    ///
    /// Runs [`Self::search`] (BM25 over `symbol_name`) and
    /// [`Self::fuzzy_search`] (edit distance over `symbol_name_raw`)
    /// in succession, then merges via Reciprocal Rank Fusion with constant
    /// `k` (default 60, env-tunable via `TOURING_RRF_K`). Set
    /// `TOURING_TANTIVY_TRIGRAM=0` to fall back to plain BM25.
    ///
    /// Returns at most `top_k` hits ranked by fused score.
    pub fn search_rrf(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<SearchHit>, TantivyIndexError> {
        if !crate::shared::feature_flags::tantivy_trigram_enabled() {
            return self.search(query, top_k);
        }
        let cache_key = format!("rrf:{query}:{top_k}");
        if let Some(hits) = self.query_cache.get(&cache_key) {
            self.query_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(hits.as_ref().clone());
        }
        self.query_cache_misses.fetch_add(1, Ordering::Relaxed);
        // Pull more candidates than top_k so the fusion has room to work.
        let pool = (top_k * 3).max(20);
        let porter = self.search(query, pool).unwrap_or_default();
        // I-01: 3-way RRF — adicionar trigram nativo se field habilitado.
        let trigram_enabled = crate::shared::feature_flags::tantivy_trigram_field_enabled();
        let trigram = if trigram_enabled {
            self.search_trigram(query, pool).unwrap_or_default()
        } else {
            Vec::new()
        };
        let fuzzy = self.fuzzy_search(query, 2, pool).unwrap_or_default();
        let k = crate::shared::feature_flags::rrf_k_constant();
        let fused = if trigram.is_empty() {
            rrf_merge_two(&porter, &fuzzy, k, top_k)
        } else {
            rrf_merge_three(&porter, &trigram, &fuzzy, k, top_k)
        };
        self.query_cache
            .insert(cache_key, std::sync::Arc::new(fused.clone()));
        Ok(fused)
    }

    /// Return prefix suggestions — symbols whose name starts with `prefix`.
    ///
    /// Uses `RegexQuery` with pattern `^prefix.*`. The prefix is lowercased
    /// because the default analyzer lowercases indexed terms
    /// (e.g. "HookRuntime" → "hookruntime").
    pub fn suggest(&self, prefix: &str, top_k: usize) -> Result<Vec<SearchHit>, TantivyIndexError> {
        if prefix.is_empty() {
            return Ok(Vec::new());
        }
        let cache_key = format!("suggest:{prefix}:{top_k}");
        if let Some(hits) = self.query_cache.get(&cache_key) {
            return Ok(hits.as_ref().clone());
        }

        let searcher = self.reader.searcher();
        let lower = prefix.to_lowercase();
        let escaped = tantivy_regex_escape(&lower);
        // The FST regex engine matches substrings only when the pattern
        // covers the full term via `.*` suffix. Without `.*`, exact-match regex
        // `hook` matches term "hook" but NOT "hookruntime". We need prefix
        // semantics, so append `.*` to match anything after the prefix.
        let pattern = format!("{}.*", escaped);
        let regex_query =
            tantivy::query::RegexQuery::from_pattern(&pattern, self.fields.symbol_name)
                .map_err(|e| format!("suggest regex: {e}"))?;
        let top_docs = searcher
            .search(
                &regex_query,
                &tantivy::collector::TopDocs::with_limit(top_k),
            )
            .map_err(|e| format!("suggest search: {e}"))?;

        let hits: Vec<SearchHit> = top_docs
            .into_iter()
            .filter_map(|(score, doc_address)| {
                let doc: TantivyDocument = searcher.doc(doc_address).ok()?;
                Some(self.doc_to_hit(&doc, score))
            })
            .collect();

        self.query_cache
            .insert(cache_key, std::sync::Arc::new(hits.clone()));
        Ok(hits)
    }

    /// Search within a specific crate, filtering by `crate_name`.
    pub fn search_by_crate(
        &self,
        query: &str,
        crate_name: &str,
        top_k: usize,
    ) -> Result<Vec<SearchHit>, TantivyIndexError> {
        let searcher = self.reader.searcher();
        let query_parser = tantivy::query::QueryParser::for_index(
            &self.index,
            vec![self.fields.symbol_name, self.fields.docstring],
        );
        let text_query = query_parser
            .parse_query(query)
            .map_err(|e| format!("parse_query: {e}"))?;
        let top_docs = searcher
            .search(&text_query, &tantivy::collector::TopDocs::with_limit(top_k))
            .map_err(|e| format!("search_by_crate: {e}"))?;

        let hits: Vec<SearchHit> = top_docs
            .into_iter()
            .filter_map(|(score, doc_address)| {
                let doc: TantivyDocument = searcher.doc(doc_address).ok()?;
                let hit = self.doc_to_hit(&doc, score);
                if hit.crate_name.as_deref() == Some(crate_name) {
                    Some(hit)
                } else {
                    None
                }
            })
            .collect();

        Ok(hits)
    }

    /// Delete all symbols that belong to `file`.
    pub fn delete_by_file(&self, file: &str) -> Result<(), TantivyIndexError> {
        let writer = self
            .writer
            .lock()
            .map_err(|e| format!("writer lock: {e}"))?;
        let term = Term::from_field_text(self.fields.file_path, file);
        writer.delete_term(term);
        Ok(())
    }

    /// Rebuild the entire index from an explicit list of documents.
    /// Removes all existing documents and inserts the provided list.
    pub fn reindex(&self, docs: Vec<SymbolDoc>) -> Result<IndexStats, TantivyIndexError> {
        // Delete all existing documents by committing with a match-all delete
        {
            let writer = self
                .writer
                .lock()
                .map_err(|e| format!("writer lock: {e}"))?;
            // Delete all docs using a match-all query approach
            let searcher = self.reader.searcher();
            let all_query = tantivy::query::AllQuery;
            let top_docs = searcher
                .search(&all_query, &tantivy::collector::TopDocs::with_limit(10_000))
                .map_err(|e| format!("all docs query: {e}"))?;
            for (_, doc_address) in top_docs {
                if let Ok(doc) = searcher.doc::<TantivyDocument>(doc_address) {
                    let file_path = extract_str(&doc, self.fields.file_path);
                    let term = Term::from_field_text(self.fields.file_path, &file_path);
                    writer.delete_term(term);
                }
            }
        }
        // Add all new documents
        for doc in &docs {
            let (tantivy_doc, doc_id) = Self::build_tantivy_doc(&self.fields, doc);
            let writer = self
                .writer
                .lock()
                .map_err(|e| format!("writer lock: {e}"))?;
            let term = Term::from_field_text(self.fields.blake3_hash, &doc_id);
            writer.delete_term(term);
            writer
                .add_document(tantivy_doc)
                .map_err(|e| format!("add_document: {e}"))?;
        }
        self.commit()?;
        Ok(self.stats())
    }

    /// Community-boosted search using RRF (Reciprocal Rank Fusion).
    pub fn search_with_community_boost(
        &self,
        query: &str,
        top_k: usize,
        community_id: Option<u64>,
    ) -> Result<Vec<SearchHit>, TantivyIndexError> {
        let searcher = self.reader.searcher();
        let query_parser = tantivy::query::QueryParser::for_index(
            &self.index,
            vec![self.fields.symbol_name, self.fields.docstring],
        );
        let parsed = query_parser
            .parse_query(query)
            .map_err(|e| format!("parse_query: {e}"))?;
        let top_docs = searcher
            .search(&parsed, &tantivy::collector::TopDocs::with_limit(top_k * 2))
            .map_err(|e| format!("search: {e}"))?;

        let mut hits: Vec<SearchHit> = top_docs
            .into_iter()
            .filter_map(|(score, doc_address)| {
                let doc: TantivyDocument = searcher.doc(doc_address).ok()?;
                Some(self.doc_to_hit(&doc, score))
            })
            .collect();

        // Apply community RRF boost
        if let Some(cid) = community_id {
            hits.sort_by(|a, b| {
                let score_a = if a.community_id == Some(cid) {
                    1.0
                } else {
                    0.0
                };
                let score_b = if b.community_id == Some(cid) {
                    1.0
                } else {
                    0.0
                };
                score_b
                    .partial_cmp(&score_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            hits.truncate(top_k);
        }

        Ok(hits)
    }

    /// Return a snapshot of index health and performance metrics.
    pub fn stats(&self) -> IndexStats {
        let index_size_bytes = std::fs::metadata(&self.index_dir)
            .map(|m| m.len())
            .unwrap_or(0);
        IndexStats {
            total_docs: self.reader.searcher().num_docs(),
            index_size_bytes,
            pending_ops: self.pending_count.load(Ordering::Relaxed),
            total_commits: self.total_commits.load(Ordering::Relaxed),
            total_upserts: self.total_upserts.load(Ordering::Relaxed),
        }
    }

    fn build_tantivy_doc(fields: &SchemaFields, doc: &SymbolDoc) -> (TantivyDocument, String) {
        let mut tantivy_doc = TantivyDocument::new();
        // stem field: Porter normalization for BM25 search quality
        tantivy_doc.add_text(fields.symbol_name, &doc.symbol_name);
        // raw field: exact text for fuzzy/prefix search (no stemming applied)
        tantivy_doc.add_text(fields.symbol_name_raw, &doc.symbol_name);
        // I-01 trigram field: substring search via NgramTokenizer(3,3)
        if crate::shared::feature_flags::tantivy_trigram_field_enabled() {
            tantivy_doc.add_text(fields.symbol_name_trigram, &doc.symbol_name);
        }
        // I-08 facet hierarchy: /<lang>/<crate>/<kind>/<visibility>
        tantivy_doc.add_facet(fields.symbol_facet, build_symbol_facet(doc));
        tantivy_doc.add_text(fields.file_path, &doc.file_path);
        tantivy_doc.add_text(fields.symbol_kind, &doc.symbol_kind);
        if let Some(ref mp) = doc.module_path {
            tantivy_doc.add_text(fields.module_path, mp);
        }
        if let Some(ref ds) = doc.docstring {
            let filtered = code_only(ds);
            if !filtered.is_empty() {
                tantivy_doc.add_text(fields.docstring, &filtered);
            }
        }
        tantivy_doc.add_u64(fields.line_number, doc.line_number);
        tantivy_doc.add_text(fields.language, &doc.language);
        if let Some(ref v) = doc.visibility {
            tantivy_doc.add_text(fields.visibility, v);
        }
        if let Some(ref c) = doc.crate_name {
            tantivy_doc.add_text(fields.crate_name, c);
        }
        let doc_id: String = doc.blake3_hash.clone().unwrap_or_else(|| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(doc.symbol_name.as_bytes());
            hasher.update(b"|");
            hasher.update(doc.file_path.as_bytes());
            hasher.update(b"|");
            hasher.update(&doc.line_number.to_le_bytes());
            hasher.finalize().to_hex().to_string()
        });
        tantivy_doc.add_text(fields.blake3_hash, &doc_id);
        if let Some(ic) = doc.import_count {
            tantivy_doc.add_u64(fields.import_count, ic);
        }
        if let Some(ec) = doc.export_count {
            tantivy_doc.add_u64(fields.export_count, ec);
        }
        if let Some(cs) = doc.cognitive_score {
            tantivy_doc.add_u64(fields.cognitive_score_x1000, (cs * 1000.0) as u64);
        }
        if let Some(ref fs) = doc.functional_signature {
            let filtered = code_only(fs);
            if !filtered.is_empty() {
                tantivy_doc.add_text(fields.functional_signature, &filtered);
            }
        }
        if let Some(cid) = doc.community_id {
            tantivy_doc.add_u64(fields.community_id, cid);
        }
        (tantivy_doc, doc_id)
    }

    /// Convert a TantivyDocument to a SearchHit.
    fn doc_to_hit(&self, doc: &TantivyDocument, score: f32) -> SearchHit {
        SearchHit {
            symbol_name: extract_str(doc, self.fields.symbol_name),
            file_path: extract_str(doc, self.fields.file_path),
            symbol_kind: extract_str(doc, self.fields.symbol_kind),
            line_number: extract_u64(doc, self.fields.line_number),
            score,
            crate_name: extract_str_opt(doc, self.fields.crate_name),
            visibility: extract_str_opt(doc, self.fields.visibility),
            functional_signature: extract_str_opt(doc, self.fields.functional_signature),
            cognitive_score: extract_cognitive_score(doc, self.fields.cognitive_score_x1000),
            community_id: extract_u64_opt(doc, self.fields.community_id),
        }
    }

    /// Inserts or replaces the index entry for the given symbol document.
    pub fn upsert_symbol(&self, doc: &SymbolDoc) -> Result<(), TantivyIndexError> {
        let (tantivy_doc, doc_id) = Self::build_tantivy_doc(&self.fields, doc);
        {
            let writer = self
                .writer
                .lock()
                .map_err(|e| format!("writer lock poisoned: {e}"))?;
            let term = Term::from_field_text(self.fields.blake3_hash, &doc_id);
            writer.delete_term(term);
            writer
                .add_document(tantivy_doc)
                .map_err(|e| format!("add_document: {e}"))?;
        }
        self.pending_count.fetch_add(1, Ordering::Relaxed);
        self.total_upserts.fetch_add(1, Ordering::Relaxed);
        crate::shared::gate_metrics::record_tantivy_upsert();
        if self.pending_count.load(Ordering::Relaxed) >= self.batch_size {
            let _ = self.commit();
        }
        Ok(())
    }
}

/// Escape regex metacharacters for use in tantivy's `RegexQuery` patterns.
///
/// The `tantivy-fst` regex engine supports a subset of POSIX ERE.  We escape
/// the meaningful metacharacters so arbitrary user-supplied prefixes are safe.
fn tantivy_regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        if matches!(
            ch,
            '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

// ─── Field Extraction Helpers ─────────────────────────────────────────────────

#[inline]
fn extract_str(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| {
            if let tantivy::schema::OwnedValue::Str(s) = v {
                Some(s.as_str())
            } else {
                None
            }
        })
        .unwrap_or("")
        .to_string()
}

#[inline]
fn extract_str_opt(doc: &TantivyDocument, field: Field) -> Option<String> {
    doc.get_first(field).and_then(|v| {
        if let tantivy::schema::OwnedValue::Str(s) = v {
            let s = s.as_str();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        } else {
            None
        }
    })
}

#[inline]
fn extract_u64(doc: &TantivyDocument, field: Field) -> u64 {
    doc.get_first(field)
        .and_then(|v| {
            if let tantivy::schema::OwnedValue::U64(n) = v {
                Some(*n)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

/// Extract cognitive_score as f64 from u64 field (stored as x1000).
#[inline]
fn extract_cognitive_score(doc: &TantivyDocument, field: Field) -> Option<f64> {
    doc.get_first(field).and_then(|v| {
        if let tantivy::schema::OwnedValue::U64(n) = v {
            if *n == 0 {
                None
            } else {
                Some(*n as f64 / 1000.0)
            }
        } else {
            None
        }
    })
}

/// Extract an optional u64 value from a Tantivy document field.
///
/// Returns `None` if the field is absent or not a u64.
#[inline]
fn extract_u64_opt(doc: &TantivyDocument, field: Field) -> Option<u64> {
    doc.get_first(field).and_then(|v| {
        if let tantivy::schema::OwnedValue::U64(n) = v {
            Some(*n)
        } else {
            None
        }
    })
}

// ─── Global Singleton ────────────────────────────────────────────────────────

/// Global TantivyIndex singleton, lazily initialized on first access.
///
/// Stores `Option<TantivyIndex>` so that initialization failures degrade
/// gracefully: callers receive `None` and skip Tantivy work rather than panicking.
///
/// Index directory: `~/.claude/touring/tantivy/`
static TANTIVY_GLOBAL: OnceLock<Option<TantivyIndex>> = OnceLock::new();

/// Return a reference to the global [`TantivyIndex`] singleton, initializing
/// it on first call.
///
/// Returns `None` if initialization fails (e.g. permission error, disk full).
/// All call-sites must handle `None` gracefully — Tantivy is additive; its
/// absence must never block the hook path.
pub fn global_tantivy() -> Option<&'static TantivyIndex> {
    TANTIVY_GLOBAL
        .get_or_init(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            if home.is_empty() {
                tracing::warn!("tantivy global: HOME not set — skipping init");
                return None;
            }
            let index_dir = PathBuf::from(&home)
                .join(".claude")
                .join("touring")
                .join("tantivy");
            match TantivyIndex::open_or_create(&index_dir) {
                Ok(idx) => {
                    tracing::debug!(
                        dir = %index_dir.display(),
                        docs = idx.stats().total_docs,
                        "tantivy global singleton initialized"
                    );
                    Some(idx)
                }
                Err(e) => {
                    tracing::warn!("tantivy global init failed: {e}");
                    None
                }
            }
        })
        .as_ref()
}

/// Map a file path extension to a language name suitable for the Tantivy `language` field.
///
/// Returns `"unknown"` for unrecognized extensions.
pub fn extension_to_language(path: &str) -> String {
    const EXT_MAP: &[(&str, &str)] = &[
        ("rs", "rust"),
        ("py", "python"),
        ("ts", "typescript"),
        ("tsx", "typescript"),
        ("js", "javascript"),
        ("jsx", "javascript"),
        ("go", "go"),
        ("java", "java"),
        ("c", "c"),
        ("h", "c"),
        ("cpp", "cpp"),
        ("hpp", "cpp"),
        ("cc", "cpp"),
        ("cxx", "cpp"),
        ("md", "markdown"),
        ("toml", "toml"),
        ("yaml", "yaml"),
        ("yml", "yaml"),
        ("json", "json"),
        ("sh", "shell"),
        ("bash", "shell"),
    ];
    let ext = path.rsplit('.').next().unwrap_or("");
    EXT_MAP
        .iter()
        .find(|(k, _)| *k == ext)
        .map(|(_, v)| *v)
        .unwrap_or("unknown")
        .to_string()
}

// ─── D2.3 — tool_outputs index (context-mode integration) ────────────────────

/// A captured large-output tool invocation persisted for context-mode replay.
///
/// Stored under a separate Tantivy index (`tool_outputs`), keyed by the
/// blake3 content hash produced by `sandbox_executor::hash_output`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ToolOutputDoc {
    /// blake3 hex digest of the captured stdout (64 chars).
    pub content_hash: String,
    /// Tool name, e.g. "Bash", "Grep".
    pub tool_name: String,
    /// First N chars of the captured output, or LLM-generated abstract.
    pub summary: String,
    /// Filesystem path holding the raw bytes (`stored_path` from SandboxResult).
    pub full_output_path: String,
    /// Process exit code (-1 if unavailable).
    pub exit_code: i64,
    /// Captured output size in bytes.
    pub output_bytes: u64,
    /// True when capture stopped at `max_output_bytes`.
    pub was_truncated: bool,
    /// Unix epoch seconds at storage time.
    pub stored_at_unix: u64,
    /// I-06 — Original tool args as JSON for deep-nested queries.
    /// Persisted via Tantivy `add_json_field` enabling queries like
    /// `tool_args.command:gh-*` or `tool_args.path:src/*`.
    /// `#[serde(default)]` keeps existing serialised docs deserializable.
    #[serde(default)]
    pub tool_args: Option<serde_json::Value>,
}

struct ToolOutputsFields {
    content_hash: Field,
    tool_name: Field,
    summary: Field,
    full_output_path: Field,
    exit_code: Field,
    output_bytes: Field,
    was_truncated: Field,
    stored_at_unix: Field,
    /// I-09 — DateField nativo (dual-write com stored_at_unix). Permite
    /// queries RFC3339 (`stored_at:>2026-05-01`) e date_histogram aggs.
    stored_at_dt: Field,
    /// I-06 — JSON field para tool_args com queries deep-nested
    /// (`tool_args.command:gh`, `tool_args.path:src/*`).
    tool_args_json: Field,
}

fn build_tool_outputs_schema() -> (Schema, ToolOutputsFields) {
    use tantivy::schema::{INDEXED, SchemaBuilder};
    let mut builder = SchemaBuilder::new();

    let text_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    // content_hash: exact match key (STRING) — required for upsert/get.
    let content_hash = builder.add_text_field("content_hash", STRING | STORED);
    let tool_name = builder.add_text_field("tool_name", STRING | STORED | FAST);
    let summary = builder.add_text_field("summary", text_opts);
    let full_output_path = builder.add_text_field("full_output_path", STRING | STORED);
    // i64 supports negative exit codes; FAST for filtering, INDEXED for queries.
    let exit_code = builder.add_i64_field("exit_code", STORED | FAST | INDEXED);
    let output_bytes = builder.add_u64_field("output_bytes", STORED | FAST);
    let was_truncated = builder.add_u64_field("was_truncated", STORED | FAST);
    let stored_at_unix = builder.add_u64_field("stored_at_unix", STORED | FAST);
    // I-09 — DateField nativo (dual-write com stored_at_unix). FAST + INDEXED
    // habilita date_histogram aggs e RFC3339 range queries.
    let stored_at_dt = builder.add_date_field("stored_at", STORED | FAST | INDEXED);
    // I-06 — JSON field para tool_args (deep-nested queries).
    // STORED + TEXT permite parse_query 'tool_args.command:gh' nativamente.
    use tantivy::schema::TEXT;
    let tool_args_json = builder.add_json_field("tool_args", STORED | TEXT);

    let schema = builder.build();
    let fields = ToolOutputsFields {
        content_hash,
        tool_name,
        summary,
        full_output_path,
        exit_code,
        output_bytes,
        was_truncated,
        stored_at_unix,
        stored_at_dt,
        tool_args_json,
    };
    (schema, fields)
}

// ─── P3-TRIG — RRF (Reciprocal Rank Fusion) helpers ──────────────────────────
// Module-level so the public `search_rrf` handler stays under CC=15
// (lesson:engineer:ann_memory:rrf_merge_pattern).

/// Stable identity for a `SearchHit` — combines file_path + symbol_name +
/// line_number to deduplicate hits across the two ranked lists. Two
/// definitions in the same file with the same name but different lines
/// still get distinct keys.
fn hit_identity(hit: &SearchHit) -> String {
    format!("{}::{}:{}", hit.file_path, hit.symbol_name, hit.line_number)
}

/// Reciprocal Rank Fusion of two ranked lists.
///
/// For a doc appearing at rank `r` (1-indexed) in list `L`, contributes
/// `1.0 / (k + r)` to the fused score. Documents appearing in both lists
/// sum contributions, then results are sorted by score descending and
/// truncated to `top_k`.
fn rrf_merge_two(
    porter: &[SearchHit],
    fuzzy: &[SearchHit],
    k: u32,
    top_k: usize,
) -> Vec<SearchHit> {
    use std::collections::HashMap;
    let k = k as f32;
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut hits_by_id: HashMap<String, SearchHit> = HashMap::new();

    accumulate_rrf(porter, k, &mut scores, &mut hits_by_id);
    accumulate_rrf(fuzzy, k, &mut scores, &mut hits_by_id);

    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
        .into_iter()
        .take(top_k)
        .filter_map(|(id, fused_score)| {
            let mut h = hits_by_id.remove(&id)?;
            h.score = fused_score;
            Some(h)
        })
        .collect()
}

/// I-01 — 3-way variant of [`rrf_merge_two`]: porter ⊕ trigram ⊕ fuzzy.
/// Documents matching in all three lists rank highest.
fn rrf_merge_three(
    porter: &[SearchHit],
    trigram: &[SearchHit],
    fuzzy: &[SearchHit],
    k: u32,
    top_k: usize,
) -> Vec<SearchHit> {
    use std::collections::HashMap;
    let k = k as f32;
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut hits_by_id: HashMap<String, SearchHit> = HashMap::new();
    accumulate_rrf(porter, k, &mut scores, &mut hits_by_id);
    accumulate_rrf(trigram, k, &mut scores, &mut hits_by_id);
    accumulate_rrf(fuzzy, k, &mut scores, &mut hits_by_id);
    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
        .into_iter()
        .take(top_k)
        .filter_map(|(id, fused_score)| {
            let mut h = hits_by_id.remove(&id)?;
            h.score = fused_score;
            Some(h)
        })
        .collect()
}

/// Accumulates per-doc reciprocal-rank contributions from a single ranked
/// list into the running score map.
fn accumulate_rrf(
    list: &[SearchHit],
    k: f32,
    scores: &mut std::collections::HashMap<String, f32>,
    hits_by_id: &mut std::collections::HashMap<String, SearchHit>,
) {
    for (rank, hit) in list.iter().enumerate() {
        let id = hit_identity(hit);
        let r = (rank + 1) as f32;
        *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k + r);
        hits_by_id.entry(id).or_insert_with(|| hit.clone());
    }
}

// ─── I-06 JSON value conversion helper (serde → tantivy OwnedValue) ──────────

/// I-06 — Convert a `serde_json::Value` into `tantivy::schema::OwnedValue`.
/// Recursive but bounded by the input's structural depth. Numbers map to I64
/// when integral, F64 otherwise; null becomes Null; arrays + objects recurse.
fn serde_value_to_tantivy_owned(v: &serde_json::Value) -> tantivy::schema::OwnedValue {
    use tantivy::schema::OwnedValue;
    match v {
        serde_json::Value::Null => OwnedValue::Null,
        serde_json::Value::Bool(b) => OwnedValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                OwnedValue::I64(i)
            } else if let Some(u) = n.as_u64() {
                OwnedValue::U64(u)
            } else {
                OwnedValue::F64(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => OwnedValue::Str(s.clone()),
        serde_json::Value::Array(arr) => {
            OwnedValue::Array(arr.iter().map(serde_value_to_tantivy_owned).collect())
        }
        serde_json::Value::Object(map) => {
            let inner: std::collections::BTreeMap<String, OwnedValue> = map
                .iter()
                .map(|(k, v)| (k.clone(), serde_value_to_tantivy_owned(v)))
                .collect();
            OwnedValue::Object(inner.into_iter().collect())
        }
    }
}

// ─── tool_outputs decode helpers (CC<15 — lesson:rrf_merge_pattern) ──────────

fn doc_str(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| match v {
            tantivy::schema::OwnedValue::Str(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn doc_i64(doc: &TantivyDocument, field: Field) -> i64 {
    doc.get_first(field)
        .and_then(|v| match v {
            tantivy::schema::OwnedValue::I64(n) => Some(*n),
            _ => None,
        })
        .unwrap_or_default()
}

fn doc_u64(doc: &TantivyDocument, field: Field) -> u64 {
    doc.get_first(field)
        .and_then(|v| match v {
            tantivy::schema::OwnedValue::U64(n) => Some(*n),
            _ => None,
        })
        .unwrap_or_default()
}

fn decode_tool_output_doc(doc: &TantivyDocument, fields: &ToolOutputsFields) -> ToolOutputDoc {
    ToolOutputDoc {
        content_hash: doc_str(doc, fields.content_hash),
        tool_name: doc_str(doc, fields.tool_name),
        summary: doc_str(doc, fields.summary),
        full_output_path: doc_str(doc, fields.full_output_path),
        exit_code: doc_i64(doc, fields.exit_code),
        output_bytes: doc_u64(doc, fields.output_bytes),
        was_truncated: doc_u64(doc, fields.was_truncated) != 0,
        stored_at_unix: doc_u64(doc, fields.stored_at_unix),
        // I-06 — JSON field roundtrip not implemented for read path yet
        // (Tantivy 0.22 OwnedValue → serde_json::Value conversion is verbose;
        // queries work but in-memory ToolOutputDoc.tool_args remains None
        // on read; original Value preserved server-side until next session
        // adds the inverse decoder).
        tool_args: None,
    }
}

/// Tantivy index of tool outputs (context-mode storage).
///
/// Separate from the symbol search index to avoid schema coupling. Documents
/// are upserted by `content_hash` (delete-then-add) and committed eagerly.
pub struct ToolOutputsIndex {
    index: Index,
    reader: IndexReader,
    writer: std::sync::Mutex<IndexWriter>,
    fields: ToolOutputsFields,
}

impl ToolOutputsIndex {
    /// Open or create the tool_outputs index at `path`.
    pub fn open_or_create(path: &Path) -> Result<Self, TantivyIndexError> {
        let (schema, fields) = build_tool_outputs_schema();
        std::fs::create_dir_all(path).map_err(|e| format!("create_dir_all: {e}"))?;
        let index = Index::open_or_create(
            tantivy::directory::MmapDirectory::open(path)
                .map_err(|e| format!("MmapDirectory: {e}"))?,
            schema,
        )
        .map_err(|e| format!("Index::open_or_create: {e}"))?;
        let writer = index
            .writer(15_000_000)
            .map_err(|e| format!("writer: {e}"))?;
        let reader = index.reader().map_err(|e| format!("reader: {e}"))?;
        Ok(ToolOutputsIndex {
            index,
            reader,
            writer: std::sync::Mutex::new(writer),
            fields,
        })
    }

    /// I-05 — Returns Some(stored_at_unix) when a doc with `content_hash`
    /// is fresh within the configured TTL window; None otherwise.
    pub fn is_fresh(&self, content_hash: &str, ttl_secs: u64) -> Option<u64> {
        let existing = self.get_tool_output(content_hash).ok().flatten()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if now.saturating_sub(existing.stored_at_unix) <= ttl_secs {
            Some(existing.stored_at_unix)
        } else {
            None
        }
    }

    /// I-05 — Cleanup actor: deletes docs older than `retention_secs`.
    /// Returns count of deleted docs. Idempotent and safe to call repeatedly.
    /// Iterates all docs via AllQuery + filters in-memory by stored_at_unix
    /// (RangeQuery API in tantivy 0.22 differs across patch versions; this
    /// scan is safe and bounded by the small `tool_outputs` corpus).
    pub fn cleanup_expired(&self, retention_secs: u64) -> Result<u64, TantivyIndexError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cutoff = now.saturating_sub(retention_secs);
        use tantivy::collector::TopDocs;
        let searcher = self.reader.searcher();
        let all_q = tantivy::query::AllQuery;
        let hits = searcher
            .search(&all_q, &TopDocs::with_limit(100_000))
            .map_err(|e| format!("cleanup search: {e}"))?;
        let mut w = self
            .writer
            .lock()
            .map_err(|e| format!("writer lock poisoned: {e}"))?;
        let mut deleted = 0u64;
        for (_score, addr) in hits {
            let Ok(stored) = searcher.doc::<TantivyDocument>(addr) else {
                continue;
            };
            let stored_at = doc_u64(&stored, self.fields.stored_at_unix);
            if stored_at < cutoff {
                let hash = doc_str(&stored, self.fields.content_hash);
                if !hash.is_empty() {
                    let term = Term::from_field_text(self.fields.content_hash, &hash);
                    w.delete_term(term);
                    deleted += 1;
                }
            }
        }
        if deleted > 0 {
            w.commit().map_err(|e| format!("commit: {e}"))?;
            self.reader.reload().map_err(|e| format!("reload: {e}"))?;
            crate::shared::gate_metrics::record_tool_outputs_cleanup_deleted(deleted);
        }
        Ok(deleted)
    }

    /// Upsert a tool output document, keyed by `content_hash`.
    ///
    /// I-05 — Skips re-indexing when an existing doc with the same hash
    /// is within the TTL window (env `TOURING_TOOL_OUTPUTS_TTL_SECS`,
    /// default 86400 = 24h). Counter `tool_outputs_ttl_skip_count` advanced
    /// on skip path.
    pub fn store_tool_output(&self, doc: &ToolOutputDoc) -> Result<(), TantivyIndexError> {
        let ttl = crate::shared::feature_flags::tool_outputs_ttl_secs();
        if self.is_fresh(&doc.content_hash, ttl).is_some() {
            crate::shared::gate_metrics::record_tool_outputs_ttl_skip();
            return Ok(());
        }
        let mut w = self
            .writer
            .lock()
            .map_err(|e| format!("writer lock poisoned: {e}"))?;
        // Delete any previous record for the same content_hash (idempotent upsert).
        let term = Term::from_field_text(self.fields.content_hash, &doc.content_hash);
        w.delete_term(term);

        let mut td = TantivyDocument::default();
        td.add_text(self.fields.content_hash, &doc.content_hash);
        td.add_text(self.fields.tool_name, &doc.tool_name);
        td.add_text(self.fields.summary, &doc.summary);
        td.add_text(self.fields.full_output_path, &doc.full_output_path);
        td.add_i64(self.fields.exit_code, doc.exit_code);
        td.add_u64(self.fields.output_bytes, doc.output_bytes);
        td.add_u64(
            self.fields.was_truncated,
            if doc.was_truncated { 1 } else { 0 },
        );
        td.add_u64(self.fields.stored_at_unix, doc.stored_at_unix);
        // I-09 — dual-write: persiste DateField alongside u64 para queries
        // nativas RFC3339. Conversão saturating saturada em i64::MAX se
        // o unix-secs ultrapassar (improvável até ano 292M).
        let dt = tantivy::DateTime::from_timestamp_secs(doc.stored_at_unix as i64);
        td.add_date(self.fields.stored_at_dt, dt);
        // I-06 — persist tool_args as JSON field when caller provided it.
        if let Some(ref args) = doc.tool_args {
            if let Some(map) = args.as_object() {
                let owned: std::collections::BTreeMap<String, tantivy::schema::OwnedValue> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_value_to_tantivy_owned(v)))
                    .collect();
                td.add_object(self.fields.tool_args_json, owned);
            }
        }

        w.add_document(td)
            .map_err(|e| format!("add_document: {e}"))?;
        w.commit().map_err(|e| format!("commit: {e}"))?;
        // Reader must be reloaded so subsequent reads see the new doc.
        self.reader.reload().map_err(|e| format!("reload: {e}"))?;
        Ok(())
    }

    /// Retrieve a previously stored tool output by `content_hash`.
    pub fn get_tool_output(
        &self,
        content_hash: &str,
    ) -> Result<Option<ToolOutputDoc>, TantivyIndexError> {
        use tantivy::collector::TopDocs;
        use tantivy::query::TermQuery;
        let searcher = self.reader.searcher();
        let term = Term::from_field_text(self.fields.content_hash, content_hash);
        let query = TermQuery::new(term, IndexRecordOption::Basic);
        let hits = searcher
            .search(&query, &TopDocs::with_limit(1))
            .map_err(|e| format!("search: {e}"))?;
        let Some((_score, addr)) = hits.first().copied() else {
            return Ok(None);
        };
        let stored = searcher
            .doc::<TantivyDocument>(addr)
            .map_err(|e| format!("doc: {e}"))?;
        Ok(Some(decode_tool_output_doc(&stored, &self.fields)))
    }

    /// Forces a writer commit + reader reload. Useful for tests.
    pub fn commit(&self) -> Result<(), TantivyIndexError> {
        let mut w = self
            .writer
            .lock()
            .map_err(|e| format!("writer lock poisoned: {e}"))?;
        w.commit().map_err(|e| format!("commit: {e}"))?;
        self.reader.reload().map_err(|e| format!("reload: {e}"))?;
        Ok(())
    }

    /// Returns the underlying index handle (for diagnostics).
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// BM25 search over the `summary` field — used by `ctx_insight` to
    /// surface relevant captured tool outputs for a free-text query.
    ///
    /// Returns up to `top_k` matching documents in BM25 order. Empty
    /// queries return an empty Vec without touching the index.
    pub fn search_summaries(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<ToolOutputDoc>, TantivyIndexError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        use tantivy::collector::TopDocs;
        let searcher = self.reader.searcher();
        let qp = tantivy::query::QueryParser::for_index(&self.index, vec![self.fields.summary]);
        let parsed = qp
            .parse_query(query)
            .map_err(|e| format!("parse_query: {e}"))?;
        let hits = searcher
            .search(&parsed, &TopDocs::with_limit(top_k))
            .map_err(|e| format!("search: {e}"))?;
        let mut out = Vec::with_capacity(hits.len());
        for (_score, addr) in hits {
            let stored = searcher
                .doc::<TantivyDocument>(addr)
                .map_err(|e| format!("doc: {e}"))?;
            out.push(decode_tool_output_doc(&stored, &self.fields));
        }
        Ok(out)
    }

    /// I-04 — Retrieve a tool output and regenerate its summary as a smart
    /// snippet around the query terms via Tantivy's `SnippetGenerator`.
    ///
    /// When `query` is empty, falls back to [`Self::get_tool_output`]
    /// (legacy behaviour: returns the stored prefix-truncated summary).
    /// Otherwise extracts a window around query matches in the indexed
    /// `summary` field, with HTML tags stripped (Tantivy emits
    /// `<b>...</b>` highlights — we convert to plain text).
    pub fn get_tool_output_with_snippet(
        &self,
        content_hash: &str,
        query: &str,
    ) -> Result<Option<ToolOutputDoc>, TantivyIndexError> {
        let Some(mut doc) = self.get_tool_output(content_hash)? else {
            return Ok(None);
        };
        if query.trim().is_empty() {
            return Ok(Some(doc));
        }
        use tantivy::collector::TopDocs;
        use tantivy::query::TermQuery;
        let searcher = self.reader.searcher();
        let term = Term::from_field_text(self.fields.content_hash, content_hash);
        let q = TermQuery::new(term, IndexRecordOption::Basic);
        let hits = searcher
            .search(&q, &TopDocs::with_limit(1))
            .map_err(|e| format!("snippet search: {e}"))?;
        let Some((_, addr)) = hits.first().copied() else {
            return Ok(Some(doc));
        };
        // Build a query over the `summary` field for snippet highlighting.
        let qp = tantivy::query::QueryParser::for_index(&self.index, vec![self.fields.summary]);
        if let Ok(parsed) = qp.parse_query(query) {
            if let Ok(mut snippet_gen) = tantivy::snippet::SnippetGenerator::create(
                &searcher,
                parsed.as_ref(),
                self.fields.summary,
            ) {
                snippet_gen.set_max_num_chars(512);
                if let Ok(stored) = searcher.doc::<TantivyDocument>(addr) {
                    let snippet = snippet_gen.snippet_from_doc(&stored);
                    let html = snippet.to_html();
                    if !html.is_empty() {
                        // Strip <b>/<em> tags emitted by Tantivy highlight
                        doc.summary = strip_html_tags(&html);
                    }
                }
            }
        }
        Ok(Some(doc))
    }
}

/// I-04 helper — Strip Tantivy's `<b>`/`</b>` highlight tags from a
/// snippet's HTML rendering, producing plain text. Module-level so the
/// public method stays under CC=15 (lesson:rrf_merge_pattern).
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

// ─── Global Singleton (parallel to global_tantivy) ───────────────────────────

/// Wrapper allows tests to swap in a fresh OnceLock when HOME changes between
/// test cases. Without this, a singleton OnceLock initialized with the first
/// HOME value stays pinned to that path for the entire process lifetime.
static TOOL_OUTPUTS_GLOBAL: Mutex<Option<OnceLock<Option<ToolOutputsIndex>>>> = Mutex::new(None);

/// Reset the `TOOL_OUTPUTS_GLOBAL` singleton so the next call to
/// [`global_tool_outputs`] re-reads `HOME` and re-initializes the index.
///
/// **This is test infrastructure only.**  In production the daemon runs with
/// a fixed `HOME` and this function is never called.
pub fn reset_tool_outputs_global() {
    let mut guard = TOOL_OUTPUTS_GLOBAL
        .lock()
        .expect("TOOL_OUTPUTS_GLOBAL mutex poisoned");
    *guard = Some(OnceLock::new());
}

/// Lazy singleton for the `tool_outputs` Tantivy index.
///
/// Initialised on first access at `~/.claude/touring/tool_outputs/`. Returns
/// `None` on init failure so callers degrade gracefully (matches the
/// `global_tantivy()` pattern).
pub fn global_tool_outputs() -> Option<&'static ToolOutputsIndex> {
    // Phase 1: ensure OnceLock exists in the static (lock held briefly).
    let once_ptr: *const OnceLock<Option<ToolOutputsIndex>> = {
        let mut guard = TOOL_OUTPUTS_GLOBAL
            .lock()
            .expect("TOOL_OUTPUTS_GLOBAL mutex poisoned");
        if guard.is_none() {
            *guard = Some(OnceLock::new());
        }
        // SAFETY: OnceLock lives in the static at a stable address. The
        // address remains valid for the program's lifetime — the Mutex itself
        // is never dropped. The pointer is valid for the 'static lifetime.
        guard.as_ref().expect("guard set to Some above")
            as *const OnceLock<Option<ToolOutputsIndex>>
    };
    // Phase 2: init without lock (OnceLock is Send + Sync, safe to use
    // from any thread once stored in the static).
    // SAFETY: once_ptr points to the OnceLock in the static (stable for
    // program lifetime). The OnceLock is only ever accessed through this
    // static, so there is no data race. The returned reference is 'static
    // because the OnceLock and its contents are stored in the static.
    let once: &'static OnceLock<Option<ToolOutputsIndex>> = unsafe { &*once_ptr };
    once.get_or_init(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            tracing::warn!("tool_outputs global: HOME not set");
            return None;
        }
        let dir = PathBuf::from(&home)
            .join(".claude")
            .join("touring")
            .join("tool_outputs");
        match ToolOutputsIndex::open_or_create(&dir) {
            Ok(idx) => {
                tracing::debug!(dir = %dir.display(), "tool_outputs singleton init");
                Some(idx)
            }
            Err(e) => {
                tracing::warn!(err = %e, "tool_outputs init failed");
                None
            }
        }
    })
    .as_ref()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "tantivy_index_tests.rs"]
mod tests;
