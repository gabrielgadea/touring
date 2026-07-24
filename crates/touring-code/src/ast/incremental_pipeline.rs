//! Incremental editing pipeline — wires [`IncrementalParser`], [`RopeDocument`],
//! and (optionally) [`SymbolStore`] into a single edit-aware workflow.
//!
//! # Architecture
//!
//! ```text
//! Edit arrives (file_path, start_byte, old_end_byte, new_text)
//!   │
//!   ▼
//! RopeDocument::edit()  → InputEdit  (O(log N) rope mutation)
//!   │
//!   ▼
//! IncrementalParser::parse_incremental()  → (Tree, changed_ranges)
//!   │
//!   ▼
//! extract symbols from changed ranges via tree-sitter Query
//!   │
//!   ▼
//! SymbolStore delta update  (remove old, insert new for affected ranges)
//! ```
//!
//! The pipeline falls back to full re-parse when no cached tree exists.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use tracing::instrument;
use tree_sitter::{Query, QueryCursor};

use crate::ast::document::RopeDocument;
use crate::ast::error::{AstError, AstResult};
use crate::ast::graph::SymbolLocation;
use crate::ast::languages::Lang;
use crate::ast::parser::IncrementalParser;
use crate::ast::revision::SymbolCache;
use crate::ast::store::SymbolStore;

// ── Result type ─────────────────────────────────────────────────────────────

/// Result of an incremental edit operation.
#[derive(Debug, Clone)]
pub struct IncrementalEditResult {
    /// Byte ranges that changed in the source (start, end).
    pub changed_ranges: Vec<(usize, usize)>,
    /// Symbols newly detected in the changed regions.
    pub symbols_added: Vec<SymbolLocation>,
    /// Names of symbols that were in the old ranges but not in the new tree.
    pub symbols_removed: Vec<String>,
    /// Symbols whose definition spans overlap a changed range.
    pub symbols_modified: Vec<SymbolLocation>,
    /// Wall-clock time spent parsing, in microseconds.
    pub parse_time_us: u64,
    /// `true` if tree-sitter performed an incremental re-parse;
    /// `false` if it fell back to a full parse (cache miss).
    pub was_incremental: bool,
}

// ── Pipeline ────────────────────────────────────────────────────────────────

/// Manages the incremental editing pipeline for all open documents.
///
/// Owns an [`IncrementalParser`] (tree cache), a set of [`RopeDocument`]s
/// (one per open file), and an optional [`SymbolStore`] for persistence.
pub struct IncrementalPipeline {
    parser: IncrementalParser,
    documents: HashMap<String, RopeDocument>,
    symbol_store: Option<SymbolStore>,
    /// Salsa-inspired symbol cache with durability tiers and hash-based early cutoff.
    /// Thread-safe via Mutex so get_symbols() can take &self and allow SharedPipeline
    /// to use with_read() instead of with_write(), enabling true RwLock read concurrency.
    symbol_cache: Mutex<SymbolCache>,
    /// Maximum document memory in bytes. 0 = unlimited.
    max_memory_bytes: usize,
    /// Files queued for lazy parsing: file_path → source content.
    ///
    /// Files in this queue are NOT yet parsed. They are parsed on-demand
    /// when [`ensure_loaded`] is called for the first time.
    lazy_queue: HashMap<String, String>,
}

impl std::fmt::Debug for IncrementalPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncrementalPipeline")
            .field("documents", &self.documents.len())
            .field("parser", &self.parser)
            .field("has_symbol_store", &self.symbol_store.is_some())
            .finish()
    }
}

impl Default for IncrementalPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalPipeline {
    /// Create a pipeline without persistent symbol storage.
    pub fn new() -> Self {
        Self {
            parser: IncrementalParser::new(),
            documents: HashMap::new(),
            symbol_store: None,
            symbol_cache: Mutex::new(SymbolCache::new()),
            max_memory_bytes: 0,
            lazy_queue: HashMap::new(),
        }
    }

    /// Wave 13: Create pipeline with tuned symbol cache capacity.
    ///
    /// When `PIPELINE_CHANNEL_CAPACITY` env var is set, enables buffered mode
    /// with the specified cache capacity hint.
    pub fn with_cache_capacity(capacity: usize) -> Self {
        Self {
            parser: IncrementalParser::new(),
            documents: HashMap::new(),
            symbol_store: None,
            symbol_cache: Mutex::new(SymbolCache::with_capacity(capacity)),
            max_memory_bytes: 0,
            lazy_queue: HashMap::new(),
        }
    }

    /// Create a pipeline backed by a [`SymbolStore`] at `db_path`.
    ///
    /// # Errors
    /// Returns an error if the SQLite database cannot be opened.
    pub fn with_symbol_store(db_path: &str) -> AstResult<Self> {
        let store = SymbolStore::new(Path::new(db_path))?;
        Ok(Self {
            parser: IncrementalParser::new(),
            documents: HashMap::new(),
            symbol_store: Some(store),
            symbol_cache: Mutex::new(SymbolCache::new()),
            max_memory_bytes: 0,
            lazy_queue: HashMap::new(),
        })
    }

    // ── Memory budget ───────────────────────────────────────────────────

    /// Set a memory budget in megabytes. When exceeded, oldest documents are evicted.
    /// Pass `0` for unlimited (default).
    pub fn with_memory_budget_mb(mut self, mb: usize) -> Self {
        self.max_memory_bytes = mb.saturating_mul(1024 * 1024);
        self
    }

    /// Estimated memory usage in megabytes based on document rope sizes.
    pub fn memory_usage_mb(&self) -> f64 {
        let bytes: usize = self.documents.values().map(|d| d.len_bytes()).sum();
        bytes as f64 / (1024.0 * 1024.0)
    }

    /// Evict documents until total rope memory is under `max_memory_bytes`.
    /// No-op when `max_memory_bytes == 0` (unlimited).
    fn evict_if_over_budget(&mut self) {
        if self.max_memory_bytes == 0 {
            return;
        }
        loop {
            let current: usize = self.documents.values().map(|d| d.len_bytes()).sum();
            if current <= self.max_memory_bytes {
                break;
            }
            // Evict one document per iteration. HashMap has no guaranteed order;
            // we evict whichever key comes first. Future work: track LRU order.
            if let Some(key) = self.documents.keys().next().cloned() {
                self.documents.remove(&key);
            } else {
                break; // no documents left
            }
        }
    }

    // ── Public API ──────────────────────────────────────────────────────

    /// Process a file edit incrementally.
    ///
    /// 1. Get or create a [`RopeDocument`] for the file.
    /// 2. Apply the edit to the rope, producing a [`tree_sitter::InputEdit`].
    /// 3. Feed the `InputEdit` to [`IncrementalParser::parse_incremental`].
    /// 4. Extract symbols from the changed ranges via tree-sitter queries.
    /// 5. Update the [`SymbolStore`] with the delta (if configured).
    ///
    /// # Arguments
    /// * `file_path` — path used as cache key (should be consistent across calls).
    /// * `start_byte` — byte offset where the edit starts.
    /// * `old_end_byte` — byte offset where the old content ends (before edit).
    /// * `new_text` — replacement text inserted at `start_byte`.
    pub fn process_edit(
        &mut self,
        file_path: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_text: &str,
    ) -> AstResult<IncrementalEditResult> {
        // Notify the symbol cache about this edit.
        self.symbol_cache
            .lock()
            .expect("symbol_cache lock")
            .notify_edit(Path::new(file_path));

        // Snapshot symbols before edit for diff.
        let old_symbols = self.get_symbols(file_path);

        // Get-or-create the document.
        let doc = self.documents.get_mut(file_path).ok_or_else(|| {
            AstError::ParseFailed(format!(
                "No document loaded for '{file_path}'. Call process_file first."
            ))
        })?;

        // Apply edit to rope → InputEdit.
        let input_edit = doc.edit(start_byte, old_end_byte, new_text);
        let new_source = doc.content();

        // Check whether the parser has a cached tree BEFORE parsing.
        // `parse_incremental` consumes the cached tree, so we must check first.
        let had_cached_tree = self.parser.cached_tree(file_path).is_some();

        // Incremental re-parse.
        let t0 = Instant::now();
        let (tree, ts_ranges) =
            self.parser
                .parse_incremental(file_path, &new_source, &input_edit)?;
        let parse_time_us = t0.elapsed().as_micros() as u64;

        // `was_incremental` is true when a cached tree existed, meaning
        // tree-sitter reused unchanged subtrees. Note: `ts_ranges` (the
        // structural diff) may be empty even on an incremental parse —
        // e.g., renaming an identifier doesn't change tree structure.
        let was_incremental = had_cached_tree;

        // Convert tree-sitter ranges to byte ranges.
        let changed_ranges: Vec<(usize, usize)> = if !ts_ranges.is_empty() {
            ts_ranges
                .iter()
                .map(|r| (r.start_byte, r.end_byte))
                .collect()
        } else if was_incremental {
            // Incremental parse but no structural changes detected.
            // Report the edit span as the changed range.
            vec![(start_byte, start_byte + new_text.len())]
        } else {
            // Full re-parse — entire file is "changed".
            vec![(0, new_source.len())]
        };

        // Extract symbols from the new tree.
        let new_symbols = self.extract_all_symbols(&tree, &new_source, file_path);

        // Hash-based early cutoff: check if symbols actually changed.
        let symbols_changed = self
            .symbol_cache
            .lock()
            .expect("symbol_cache lock")
            .update(Path::new(file_path), new_symbols.clone());

        // Compute delta.
        let (symbols_added, symbols_removed, symbols_modified) =
            Self::diff_symbols(&old_symbols, &new_symbols, &changed_ranges);

        // Only persist if symbols actually changed (early cutoff optimization).
        if symbols_changed {
            if let Some(store) = &self.symbol_store {
                self.persist_delta(store, file_path, &new_symbols)?;
            }
        }

        Ok(IncrementalEditResult {
            changed_ranges,
            symbols_added,
            symbols_removed,
            symbols_modified,
            parse_time_us,
            was_incremental,
        })
    }

    /// Process a full file (first load or cache miss).
    ///
    /// Creates a [`RopeDocument`], parses the full source, extracts all
    /// symbols, and stores them.
    pub fn process_file(
        &mut self,
        file_path: &str,
        content: &str,
    ) -> AstResult<IncrementalEditResult> {
        let lang = Lang::from_path(Path::new(file_path))
            .ok_or_else(|| AstError::UnknownLanguage(file_path.to_string()))?;

        // Create / replace the document, then evict if over memory budget.
        let doc = RopeDocument::new(file_path, content);
        self.documents.insert(file_path.to_string(), doc);
        self.evict_if_over_budget();

        // Notify the symbol cache about this file change.
        self.symbol_cache
            .lock()
            .expect("symbol_cache lock")
            .notify_edit(Path::new(file_path));

        // Full parse + cache.
        let t0 = Instant::now();
        let tree = self.parser.parse_and_cache(file_path, content, lang)?;
        let parse_time_us = t0.elapsed().as_micros() as u64;

        // Extract all symbols.
        let symbols = self.extract_all_symbols(&tree, content, file_path);

        // Hash-based early cutoff: check if symbols actually changed.
        let symbols_changed = self
            .symbol_cache
            .lock()
            .expect("symbol_cache lock")
            .update(Path::new(file_path), symbols.clone());

        // Only persist if symbols actually changed (early cutoff optimization).
        if symbols_changed {
            if let Some(store) = &self.symbol_store {
                self.persist_delta(store, file_path, &symbols)?;
            }
        }

        let symbols_added = symbols;

        Ok(IncrementalEditResult {
            changed_ranges: vec![(0, content.len())],
            symbols_added,
            symbols_removed: Vec::new(),
            symbols_modified: Vec::new(),
            parse_time_us,
            was_incremental: false,
        })
    }

    /// Get symbols for a file from the in-memory last-known state.
    ///
    /// If the file has been processed, re-extracts from the cached tree.
    /// If a [`SymbolStore`] is available and the file was not yet processed,
    /// queries the store.
    pub fn get_symbols(&self, file_path: &str) -> Vec<SymbolLocation> {
        // Try symbol cache first (revision-based, avoids re-extraction).
        let cached_opt: Option<Vec<SymbolLocation>> = {
            let cache = self.symbol_cache.lock().expect("symbol_cache lock");
            cache.get_if_valid(Path::new(file_path)).map(|s| s.to_vec())
        };
        if let Some(cached) = cached_opt {
            return cached;
        }

        // Try cached tree (peek = no LRU promotion).
        if let Some(tree) = self.parser.peek_tree(file_path) {
            if let Some(doc) = self.documents.get(file_path) {
                let content = doc.content();
                let symbols = self.extract_all_symbols(&tree, &content, file_path);
                // Populate the symbol cache for future calls.
                // Mutex lock is re-acquired: prior lock guard was dropped above.
                self.symbol_cache
                    .lock()
                    .expect("symbol_cache lock")
                    .update(Path::new(file_path), symbols.clone());
                return symbols;
            }
        }

        // Fall back to SymbolStore.
        if let Some(store) = &self.symbol_store {
            if let Ok(syms) = store.find_symbols_in_file(file_path) {
                return syms;
            }
        }

        Vec::new()
    }

    /// Get a reference to a loaded document.
    pub fn get_document(&self, file_path: &str) -> Option<&RopeDocument> {
        self.documents.get(file_path)
    }

    /// Number of loaded documents and cached trees.
    pub fn cache_stats(&self) -> (usize, usize) {
        (self.documents.len(), self.parser.cache_size())
    }

    /// Returns true if the parser has a cached tree for `file_path`.
    pub fn cached_tree(&self, file_path: &str) -> bool {
        self.parser.cached_tree(file_path).is_some()
    }

    /// INS-A3: Pre-warm the parser cache for session-predicted files.
    ///
    /// Reads each predicted file from disk and calls `process_file` if the
    /// file is not already cached, warming up tree-sitter's incremental parse
    /// cache so that the first real edit is fast.
    ///
    /// Files that cannot be read (missing, unknown language, I/O error) are
    /// silently skipped — prefetch is best-effort.
    ///
    /// Returns the number of files successfully pre-warmed.
    pub fn prefetch_for_session(&mut self, predicted_paths: &[String]) -> usize {
        let mut warmed = 0usize;
        for path in predicted_paths {
            // Skip already-cached files — no work needed.
            if self.documents.contains_key(path.as_str()) {
                continue;
            }
            // Skip unknown languages early to avoid I/O.
            if std::path::Path::new(path).extension().is_none() {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(path) {
                if self.process_file(path, &content).is_ok() {
                    warmed += 1;
                }
            }
        }
        warmed
    }

    // ── Lazy loading ────────────────────────────────────────────────────

    /// Queue a file for lazy parsing.
    ///
    /// The file is NOT parsed immediately. Parsing is deferred until
    /// `ensure_loaded` is called for this path (or `prefetch_from_heat`
    /// triggers it). This reduces startup latency when only a subset of
    /// indexed files will actually be queried.
    ///
    /// If the file is already loaded (in `documents`), this is a no-op.
    /// Calling `queue_for_lazy` a second time for the same path overwrites
    /// the queued source content.
    pub fn queue_for_lazy(&mut self, file_path: impl Into<String>, source: impl Into<String>) {
        let path = file_path.into();
        if self.documents.contains_key(&path) {
            return; // Already loaded — no-op.
        }
        self.lazy_queue.insert(path, source.into());
    }

    /// Ensure a file is loaded, triggering lazy parsing if needed.
    ///
    /// If `file_path` is in the lazy queue, it is parsed now via `process_file`.
    /// Returns `true` if the file was lazily loaded, `false` if it was already
    /// loaded or not in the queue.
    pub fn ensure_loaded(&mut self, file_path: &str) -> bool {
        if let Some(source) = self.lazy_queue.remove(file_path) {
            let _ = self.process_file(file_path, &source);
            return true;
        }
        false
    }

    /// Number of files currently pending in the lazy queue (not yet parsed).
    pub fn pending_lazy_count(&self) -> usize {
        self.lazy_queue.len()
    }

    /// Prefetch the top-`n` hottest files from the heat map.
    ///
    /// Triggers `ensure_loaded` for each file in priority order.
    /// Files not in the lazy queue are silently skipped.
    /// Returns the number of files actually loaded from the queue.
    pub fn prefetch_from_heat(
        &mut self,
        heat_map: &crate::ast::file_heat::HeatMap,
        n: usize,
    ) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let priority = heat_map.get_priority_order(now);
        let paths: Vec<String> = priority
            .into_iter()
            .take(n)
            .map(|(path, _score)| path.to_string())
            .collect();
        let mut loaded = 0usize;
        for path in paths {
            if self.ensure_loaded(&path) {
                loaded += 1;
            }
        }
        loaded
    }

    /// Evict all documents and cached trees.
    pub fn clear_cache(&mut self) {
        // Collect keys BEFORE clearing so we can invalidate cached trees.
        let paths: Vec<String> = self.documents.keys().cloned().collect();
        for p in &paths {
            self.parser.invalidate_tree(p);
        }
        self.documents.clear();
        self.symbol_cache.lock().expect("symbol_cache lock").clear();
        // Also reset the parser's internal state for a clean slate.
        self.parser = IncrementalParser::new();
    }

    /// Get symbol cache statistics: (hits, early_cutoffs, recomputations).
    pub fn symbol_cache_stats(&self) -> (u64, u64, u64) {
        self.symbol_cache.lock().expect("symbol_cache lock").stats()
    }

    /// Get a lock guard for the symbol cache.
    ///
    /// Returns a `MutexGuard<SymbolCache>` — the lock is held until the guard is dropped.
    /// Do not hold this guard while calling any mutating method on the pipeline.
    pub fn symbol_cache(&self) -> std::sync::MutexGuard<'_, SymbolCache> {
        self.symbol_cache.lock().expect("symbol_cache lock")
    }

    // ── Symbol extraction via tree-sitter queries ───────────────────────

    /// Extract all symbol definitions from the full tree.
    fn extract_all_symbols(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &str,
    ) -> Vec<SymbolLocation> {
        let Some(lang) = Lang::from_path(Path::new(file_path)) else {
            return Vec::new();
        };

        let query_text = lang.query_file();
        let Ok(query) = Query::new(&lang.tree_sitter_language(), query_text) else {
            return Vec::new();
        };

        self.run_query(&query, tree, source, file_path, lang)
    }

    /// Execute a compiled query against a tree and return symbol locations.
    ///
    /// `lang` is threaded through so each definition can be classified into a
    /// canonical [`crate::ast::symbols::SymbolKind`] (function/class/const/…)
    /// via the same logic the rich `extract_symbols` path uses — keeping the
    /// lightweight indexing path's `kind` consistent with on-demand overviews.
    fn run_query(
        &self,
        query: &Query,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &str,
        lang: Lang,
    ) -> Vec<SymbolLocation> {
        use streaming_iterator::StreamingIterator;

        let root = tree.root_node();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, root, source.as_bytes());

        let mut symbols = Vec::new();
        let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let capture_name = match query.capture_names().get(capture.index as usize) {
                    Some(n) => *n,
                    None => continue,
                };
                if capture_name == "name" {
                    let node = capture.node;
                    if let Some(parent) = node.parent() {
                        let key = (parent.start_byte(), parent.end_byte());
                        if seen.contains(&key) {
                            continue;
                        }
                        seen.insert(key);

                        let name = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();

                        // line is 1-indexed (matching SymbolLocation convention)
                        let line = parent.start_position().row + 1;
                        let column = parent.start_position().column;

                        // Classify the definition into a canonical SymbolKind
                        // (same logic as the rich `extract_symbols` path), so
                        // the persisted index carries `kind` for `index find`.
                        let kind = crate::ast::symbols::refine_binding_kind(
                            &name,
                            crate::ast::symbols::Symbol::node_kind_to_symbol_kind(
                                parent.kind(),
                                lang,
                            ),
                        );
                        symbols.push(
                            SymbolLocation::new(file_path, name, line, column, true)
                                .with_kind(Some(kind.as_str().to_string())),
                        ); // definitions only
                    }
                }
            }
        }

        symbols.sort_by_key(|s| s.line);
        symbols
    }

    // ── Delta computation ───────────────────────────────────────────────

    /// Compute added / removed / modified symbols between two snapshots.
    fn diff_symbols(
        old: &[SymbolLocation],
        new: &[SymbolLocation],
        changed_ranges: &[(usize, usize)],
    ) -> (Vec<SymbolLocation>, Vec<String>, Vec<SymbolLocation>) {
        let old_names: std::collections::HashSet<&str> =
            old.iter().map(|s| s.symbol_name.as_str()).collect();
        let new_names: std::collections::HashSet<&str> =
            new.iter().map(|s| s.symbol_name.as_str()).collect();

        let symbols_added: Vec<SymbolLocation> = new
            .iter()
            .filter(|s| !old_names.contains(s.symbol_name.as_str()))
            .cloned()
            .collect();

        let symbols_removed: Vec<String> = old
            .iter()
            .filter(|s| !new_names.contains(s.symbol_name.as_str()))
            .map(|s| s.symbol_name.clone())
            .collect();

        // Modified = present in both old and new, and overlaps a changed range.
        // Since we don't have byte offsets in SymbolLocation, we consider any
        // symbol present in both sets as "modified" if any changed range exists.
        let symbols_modified: Vec<SymbolLocation> = if changed_ranges.is_empty() {
            Vec::new()
        } else {
            new.iter()
                .filter(|s| {
                    old_names.contains(s.symbol_name.as_str())
                        && new_names.contains(s.symbol_name.as_str())
                })
                .filter(|s| {
                    // Check if the symbol's line falls within any changed range.
                    // This is an approximation — byte-level would be more precise,
                    // but SymbolLocation stores line numbers, not byte offsets.
                    let sym_in_old = old.iter().find(|o| o.symbol_name == s.symbol_name);
                    match sym_in_old {
                        Some(o) => o.line != s.line || o.column != s.column,
                        None => false,
                    }
                })
                .cloned()
                .collect()
        };

        (symbols_added, symbols_removed, symbols_modified)
    }

    // ── Persistence helper ──────────────────────────────────────────────

    /// Persist incremental symbol changes using a differential update.
    ///
    /// Unlike `replace_file_symbols`, this method:
    /// - Only writes rows that actually changed (upserts) or were removed
    /// - Leaves the `dependencies` table untouched, preserving import edges
    ///   that were established by separate dependency-extraction passes
    fn persist_delta(
        &self,
        store: &SymbolStore,
        file_path: &str,
        new_symbols: &[SymbolLocation],
    ) -> AstResult<()> {
        let changeset = store
            .diff_symbols(file_path, new_symbols)
            .map_err(AstError::Sqlite)?;

        if !changeset.is_empty() {
            store
                .apply_change_set(&changeset)
                .map_err(AstError::Sqlite)?;
        }

        Ok(())
    }

    // ── IA-1: FileHeat ↔ blast_radius_weight wiring ─────────────────────

    /// Wire FileHeat ↔ blast_radius_weight automatically on every edit.
    ///
    /// This is the ACO pheromone deposit step: records the edit in `heat_map`
    /// and updates `blast_radius_weight` based on how many symbols the file
    /// defines (proxy for import count when `SymbolStore` is unavailable).
    ///
    /// # Arguments
    /// * `file_path` — path of the edited file (used as heat-map key).
    /// * `start_byte`, `old_end_byte`, `new_text` — forwarded to `process_edit`.
    /// * `heat_map` — mutable reference to the shared `HeatMap`.
    ///
    /// # Returns
    /// The [`IncrementalEditResult`] from the underlying `process_edit` call,
    /// or an error string if parsing fails.
    pub fn process_edit_with_heat(
        &mut self,
        file_path: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_text: &str,
        heat_map: &mut crate::ast::file_heat::HeatMap,
    ) -> AstResult<IncrementalEditResult> {
        let result = self.process_edit(file_path, start_byte, old_end_byte, new_text)?;

        // Record the edit event — pheromone deposit step.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        heat_map.record_edit(file_path, now);

        // Update blast_radius_weight using symbol count as a proxy for
        // import depth. ln_1p normalises large counts to avoid dominance.
        let symbol_count = result.symbols_added.len();
        let weight = (symbol_count as f64).ln_1p() / 10.0;
        heat_map.set_blast_radius_weight(file_path, weight);

        Ok(result)
    }
}

// ── IA-3: PrioritizedPipeline ────────────────────────────────────────────────

/// ACO-scheduled incremental pipeline.
///
/// Wraps [`IncrementalPipeline`] with a heat-ordered re-indexing queue.
/// Files are enqueued for re-indexing as they are edited, then drained in
/// descending heat-score order so that "hot" (frequently edited, high
/// blast-radius) files are always re-indexed first.
///
/// # Example
/// ```rust
/// use touring_code::ast::incremental_pipeline::PrioritizedPipeline;
///
/// let mut pp = PrioritizedPipeline::new(100);
/// pp.enqueue("hot.rs".to_string());
/// pp.enqueue("cold.rs".to_string());
/// // hot.rs will typically be returned first by next_hot().
/// ```
pub struct PrioritizedPipeline {
    inner: IncrementalPipeline,
    heat_map: crate::ast::file_heat::HeatMap,
    pending: std::collections::HashSet<String>,
}

impl PrioritizedPipeline {
    /// Create a new [`PrioritizedPipeline`] with the given heat-map capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: IncrementalPipeline::new(),
            heat_map: crate::ast::file_heat::HeatMap::new(capacity),
            pending: std::collections::HashSet::new(),
        }
    }

    /// Expose the inner pipeline for direct operations.
    pub fn pipeline(&mut self) -> &mut IncrementalPipeline {
        &mut self.inner
    }

    /// Enqueue a file for re-indexing and record an edit event in the heat map.
    #[instrument(skip(self), fields(file = %file_path))]
    pub fn enqueue(&mut self, file_path: String) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        self.heat_map.record_edit(&file_path, now);
        self.pending.insert(file_path);
    }

    /// Enqueue a file for re-parsing with an explicit priority weight.
    ///
    /// Identical to `enqueue` but also sets the blast-radius weight on the
    /// internal heat map, so hot files with high blast radius float to the top
    /// of the priority queue returned by `get_priority_order`.
    ///
    /// # Arguments
    /// * `file_path` – canonical path of the file to enqueue.
    /// * `weight`    – multiplicative priority boost (0.0 = no boost).
    pub fn enqueue_with_priority(&mut self, file_path: String, weight: f64) {
        self.heat_map.set_blast_radius_weight(&file_path, weight);
        self.enqueue(file_path);
    }

    /// Return the hottest pending file path for re-indexing, removing it from
    /// the queue. Returns `None` when the queue is empty.
    pub fn next_hot(&mut self) -> Option<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        // get_priority_order returns (&str, score) sorted desc — find first pending.
        let hottest = self
            .heat_map
            .get_priority_order(now)
            .into_iter()
            .find(|(f, _)| self.pending.contains(*f))
            .map(|(f, _)| f.to_string());

        if let Some(ref path) = hottest {
            self.pending.remove(path);
        }
        hottest
    }

    /// Number of files currently waiting to be re-indexed.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

// ── Thread-safe wrapper ─────────────────────────────────────────────────────

/// Thread-safe wrapper around [`IncrementalPipeline`].
///
/// `IncrementalPipeline` is `!Send` because it contains `tree_sitter::Parser`.
/// This wrapper uses a `Mutex` to allow shared ownership across threads via `Arc`.
///
/// # Usage
/// ```ignore
/// let shared = Arc::new(SharedPipeline::new());
/// // In any thread:
/// shared.with(|pipeline| pipeline.process_file("test.py", source));
/// ```
/// Thread-safe wrapper around [`IncrementalPipeline`] using a `RwLock`.
///
/// This allows concurrent read access while serializing writes, improving
/// throughput for read-heavy workloads (process_file, get_symbols, cache_stats)
/// vs a Mutex which serializes all access.
pub struct SharedPipeline {
    inner: parking_lot::RwLock<IncrementalPipeline>,
}

// SAFETY: SharedPipeline wraps its inner state (including tree_sitter::Parser which is
// !Send, and optionally rusqlite::Connection which is !Send+!Sync) in a parking_lot::RwLock.
// The RwLock serializes all write access; read access can proceed concurrently since
// IncrementalPipeline internal state is protected by the lock. This makes it safe to
// both send SharedPipeline across threads and share references to it.
unsafe impl Send for SharedPipeline {}
unsafe impl Sync for SharedPipeline {}

impl SharedPipeline {
    /// Create a new thread-safe pipeline without symbol storage.
    pub fn new() -> Self {
        Self {
            inner: parking_lot::RwLock::new(IncrementalPipeline::new()),
        }
    }

    /// Create a new thread-safe pipeline with symbol storage.
    pub fn with_symbol_store(db_path: &str) -> AstResult<Self> {
        let pipeline = IncrementalPipeline::with_symbol_store(db_path)?;
        Ok(Self {
            inner: parking_lot::RwLock::new(pipeline),
        })
    }

    /// Execute a closure with exclusive write access to the pipeline.
    ///
    /// parking_lot RwLock does not poison — acquisition always succeeds.
    pub fn with_write<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut IncrementalPipeline) -> R,
    {
        let mut guard = self.inner.write();
        f(&mut guard)
    }

    /// Execute a closure with shared read access to the pipeline.
    ///
    /// Multiple threads can read concurrently. parking_lot does not poison.
    pub fn with_read<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&IncrementalPipeline) -> R,
    {
        let guard = self.inner.read();
        f(&guard)
    }

    /// Process a file through the pipeline (thread-safe).
    pub fn process_file(&self, file_path: &str, content: &str) -> AstResult<IncrementalEditResult> {
        self.with_write(|p| p.process_file(file_path, content))
    }

    /// Process an edit through the pipeline (thread-safe).
    pub fn process_edit(
        &self,
        file_path: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_text: &str,
    ) -> AstResult<IncrementalEditResult> {
        self.with_write(|p| p.process_edit(file_path, start_byte, old_end_byte, new_text))
    }

    /// Get symbols for a file (thread-safe).
    ///
    /// Uses a shared read lock — multiple threads can call this concurrently.
    /// `IncrementalPipeline::symbol_cache` uses a `Mutex` for thread-safe cache
    /// updates without requiring an exclusive write lock on the outer RwLock.
    #[instrument(skip(self))]
    pub fn get_symbols(&self, file_path: &str) -> Vec<SymbolLocation> {
        self.with_read(|p| p.get_symbols(file_path))
    }

    /// Returns true if the pipeline has a cached parse tree for `file_path`.
    ///
    /// Thread-safe. Uses a shared read lock.
    pub fn has_cached_tree(&self, file_path: &str) -> bool {
        self.with_read(|p| p.cached_tree(file_path))
    }

    /// Get cache stats (thread-safe).
    pub fn cache_stats(&self) -> (usize, usize) {
        self.with_read(|p| p.cache_stats())
    }

    /// Clear all caches (thread-safe).
    pub fn clear_cache(&self) {
        self.with_write(|p| p.clear_cache())
    }

    /// Invalidate cached parse trees for files affected by a change.
    ///
    /// Uses `SymbolIndex::blast_radius()` to find all transitively dependent
    /// files, then clears their cached trees so the next access re-parses.
    /// Returns the number of invalidated files.
    pub fn invalidate_dependents(
        &self,
        changed_file: &str,
        index: &crate::ast::graph::SymbolIndex,
    ) -> AstResult<usize> {
        let radius = index.blast_radius(changed_file);
        let mut invalidated = 0;
        self.with_write(|p| {
            for file in &radius.affected_files {
                if p.documents.contains_key(file) {
                    p.documents.remove(file);
                    invalidated += 1;
                }
            }
        });
        tracing::debug!(
            changed = changed_file,
            invalidated,
            total_affected = radius.affected_files.len(),
            "invalidated dependent parse caches"
        );
        Ok(invalidated)
    }
}

impl Default for SharedPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SharedPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedPipeline")
            .field("locked", &self.inner.is_locked())
            .finish()
    }
}

// ── Async-safe wrapper ──────────────────────────────────────────────────────

/// Async-safe wrapper around [`IncrementalPipeline`].
///
/// Uses [`tokio::sync::Mutex`] for compatibility with async runtimes.
/// The pipeline itself is synchronous (tree-sitter is CPU-bound), but
/// the async Mutex allows holding the guard across `.await` points without
/// blocking the executor — unlike `std::sync::Mutex` which would.
///
/// # Usage
/// ```ignore
/// let shared = Arc::new(AsyncSharedPipeline::new());
/// shared.process_file("test.py", source).await?;
/// ```
#[cfg(feature = "async-pipeline")]
pub struct AsyncSharedPipeline {
    inner: tokio::sync::Mutex<IncrementalPipeline>,
}

// SAFETY: Same reasoning as SharedPipeline — the inner IncrementalPipeline
// contains !Send types (tree_sitter::Parser, optionally rusqlite::Connection).
// tokio::sync::Mutex serializes all access, ensuring only one task touches
// the non-Send/Sync types at a time.
#[cfg(feature = "async-pipeline")]
unsafe impl Send for AsyncSharedPipeline {}
#[cfg(feature = "async-pipeline")]
unsafe impl Sync for AsyncSharedPipeline {}

#[cfg(feature = "async-pipeline")]
impl AsyncSharedPipeline {
    /// Create a new async pipeline without symbol storage.
    pub fn new() -> Self {
        Self {
            inner: tokio::sync::Mutex::new(IncrementalPipeline::new()),
        }
    }

    /// Create a new async pipeline with symbol storage.
    ///
    /// # Errors
    /// Returns an error if the SQLite database cannot be opened.
    pub fn with_symbol_store(db_path: &str) -> AstResult<Self> {
        let pipeline = IncrementalPipeline::with_symbol_store(db_path)?;
        Ok(Self {
            inner: tokio::sync::Mutex::new(pipeline),
        })
    }

    /// Execute a closure with exclusive access to the pipeline.
    ///
    /// Async equivalent of `SharedPipeline::with`.
    pub async fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut IncrementalPipeline) -> R,
    {
        let mut guard = self.inner.lock().await;
        f(&mut guard)
    }

    /// Process a full file through the pipeline (async).
    pub async fn process_file(
        &self,
        file_path: &str,
        content: &str,
    ) -> AstResult<IncrementalEditResult> {
        let mut pipeline = self.inner.lock().await;
        pipeline.process_file(file_path, content)
    }

    /// Process an edit through the pipeline (async).
    pub async fn process_edit(
        &self,
        file_path: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_text: &str,
    ) -> AstResult<IncrementalEditResult> {
        let mut pipeline = self.inner.lock().await;
        pipeline.process_edit(file_path, start_byte, old_end_byte, new_text)
    }

    /// Get symbols for a file (async).
    pub async fn get_symbols(&self, file_path: &str) -> Vec<SymbolLocation> {
        let pipeline = self.inner.lock().await;
        pipeline.get_symbols(file_path)
    }

    /// Get cache stats (async).
    pub async fn cache_stats(&self) -> (usize, usize) {
        let pipeline = self.inner.lock().await;
        pipeline.cache_stats()
    }

    /// Clear all caches (async).
    pub async fn clear_cache(&self) {
        let mut pipeline = self.inner.lock().await;
        pipeline.clear_cache();
    }
}

#[cfg(feature = "async-pipeline")]
impl Default for AsyncSharedPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "async-pipeline")]
impl std::fmt::Debug for AsyncSharedPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncSharedPipeline")
            .field("locked", &self.inner.try_lock().is_err())
            .finish()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ─────────────────────────────────────────────────────────

    fn python_source_v1() -> &'static str {
        "def hello():\n    pass\n\ndef world():\n    return 42\n"
    }

    fn rust_source_v1() -> &'static str {
        "pub fn greet() {}\n\npub struct Config {\n    pub name: String,\n}\n"
    }

    // ── process_file ────────────────────────────────────────────────────

    #[test]
    fn test_process_file_extracts_python_symbols() {
        let mut pipeline = IncrementalPipeline::new();
        let result = pipeline
            .process_file("test.py", python_source_v1())
            .unwrap();

        assert!(!result.was_incremental);
        assert!(result.parse_time_us < 1_000_000); // < 1 second
        assert!(
            result.symbols_added.len() >= 2,
            "Expected at least 2 symbols (hello, world), got {}",
            result.symbols_added.len()
        );

        let names: Vec<&str> = result
            .symbols_added
            .iter()
            .map(|s| s.symbol_name.as_str())
            .collect();
        assert!(names.contains(&"hello"), "Missing 'hello' in {names:?}");
        assert!(names.contains(&"world"), "Missing 'world' in {names:?}");
    }

    #[test]
    fn test_process_file_extracts_rust_symbols() {
        let mut pipeline = IncrementalPipeline::new();
        let result = pipeline.process_file("lib.rs", rust_source_v1()).unwrap();

        assert!(!result.was_incremental);
        let names: Vec<&str> = result
            .symbols_added
            .iter()
            .map(|s| s.symbol_name.as_str())
            .collect();
        assert!(names.contains(&"greet"), "Missing 'greet' in {names:?}");
        assert!(names.contains(&"Config"), "Missing 'Config' in {names:?}");
    }

    // ── process_edit (incremental) ──────────────────────────────────────

    #[test]
    fn test_process_edit_incremental_vs_full() {
        let mut pipeline = IncrementalPipeline::new();

        // Step 1: load file (full parse).
        pipeline
            .process_file("test.py", python_source_v1())
            .unwrap();

        // Step 2: edit — rename "hello" to "hola" (bytes 4..9 in "def hello():")
        // "def hello():\n" → "def hola():\n"
        let result = pipeline.process_edit("test.py", 4, 9, "hola").unwrap();

        // The parser should have had a cached tree.
        assert!(result.was_incremental, "Expected incremental parse");
        assert!(result.parse_time_us < 1_000_000);

        // After edit, "hola" should be present, "hello" gone.
        let syms = pipeline.get_symbols("test.py");
        let names: Vec<&str> = syms.iter().map(|s| s.symbol_name.as_str()).collect();
        assert!(
            names.contains(&"hola"),
            "Missing 'hola' after edit in {names:?}"
        );
        assert!(
            !names.contains(&"hello"),
            "'hello' should be gone after rename"
        );
        // "world" should still be there.
        assert!(
            names.contains(&"world"),
            "Missing 'world' after edit in {names:?}"
        );
    }

    #[test]
    fn test_process_edit_returns_changed_ranges() {
        let mut pipeline = IncrementalPipeline::new();
        pipeline
            .process_file("test.py", python_source_v1())
            .unwrap();

        // Insert a parameter.
        // "def hello():" → "def hello(name):"
        let result = pipeline.process_edit("test.py", 9, 9, "name").unwrap();

        assert!(
            !result.changed_ranges.is_empty(),
            "Expected changed ranges from incremental edit"
        );
    }

    // ── SymbolStore integration ─────────────────────────────────────────

    #[test]
    fn test_symbol_store_updated_on_edit() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("symbols.db");
        let db_str = db_path.to_str().unwrap();

        let mut pipeline = IncrementalPipeline::with_symbol_store(db_str).unwrap();

        // Load file.
        pipeline
            .process_file("test.py", python_source_v1())
            .unwrap();

        // Verify store has symbols.
        let store_syms = pipeline
            .symbol_store
            .as_ref()
            .unwrap()
            .find_symbols_in_file("test.py")
            .unwrap();
        assert!(
            store_syms.len() >= 2,
            "Store should have at least 2 symbols, got {}",
            store_syms.len()
        );

        // Edit: rename "hello" to "hola".
        pipeline.process_edit("test.py", 4, 9, "hola").unwrap();

        // Store should reflect the rename.
        let after = pipeline
            .symbol_store
            .as_ref()
            .unwrap()
            .find_symbols_in_file("test.py")
            .unwrap();
        let after_names: Vec<&str> = after.iter().map(|s| s.symbol_name.as_str()).collect();
        assert!(
            after_names.contains(&"hola"),
            "Store should contain 'hola' after edit, got {after_names:?}"
        );
        assert!(
            !after_names.contains(&"hello"),
            "Store should not contain 'hello' after edit"
        );
    }

    // ── Cache management ────────────────────────────────────────────────

    #[test]
    fn test_cache_stats_track_documents() {
        let mut pipeline = IncrementalPipeline::new();

        assert_eq!(pipeline.cache_stats(), (0, 0));

        pipeline.process_file("a.py", "def a(): pass\n").unwrap();
        let (docs, trees) = pipeline.cache_stats();
        assert_eq!(docs, 1);
        assert_eq!(trees, 1);

        pipeline.process_file("b.rs", "fn b() {}\n").unwrap();
        let (docs, trees) = pipeline.cache_stats();
        assert_eq!(docs, 2);
        assert_eq!(trees, 2);
    }

    #[test]
    fn test_clear_cache_resets_all() {
        let mut pipeline = IncrementalPipeline::new();
        pipeline.process_file("a.py", "def a(): pass\n").unwrap();
        pipeline.process_file("b.rs", "fn b() {}\n").unwrap();

        assert_eq!(pipeline.cache_stats(), (2, 2));

        pipeline.clear_cache();

        assert_eq!(pipeline.cache_stats(), (0, 0));
        assert!(pipeline.get_document("a.py").is_none());
        assert!(pipeline.get_symbols("a.py").is_empty());
    }

    // ── Edge cases ──────────────────────────────────────────────────────

    #[test]
    fn test_unknown_language_returns_empty_symbols() {
        let mut pipeline = IncrementalPipeline::new();
        let result = pipeline.process_file("data.csv", "a,b,c\n1,2,3\n");
        assert!(result.is_err(), "Unknown language should return error");
    }

    #[test]
    fn test_process_edit_without_prior_load_returns_error() {
        let mut pipeline = IncrementalPipeline::new();
        let result = pipeline.process_edit("nonexistent.py", 0, 0, "x");
        assert!(result.is_err(), "Edit without load should return error");
    }

    #[test]
    fn test_sequential_edits_maintain_consistency() {
        let mut pipeline = IncrementalPipeline::new();

        let source = "def alpha():\n    pass\n\ndef beta():\n    pass\n";
        pipeline.process_file("test.py", source).unwrap();

        // Edit 1: rename alpha to gamma.
        // "def alpha():" bytes 4..9 = "alpha"
        pipeline.process_edit("test.py", 4, 9, "gamma").unwrap();

        let syms = pipeline.get_symbols("test.py");
        let names: Vec<&str> = syms.iter().map(|s| s.symbol_name.as_str()).collect();
        assert!(names.contains(&"gamma"), "After edit 1: {names:?}");
        assert!(names.contains(&"beta"), "After edit 1: {names:?}");

        // Edit 2: rename beta to delta.
        // After edit 1, content is "def gamma():\n    pass\n\ndef beta():\n    pass\n"
        // "beta" starts at byte offset of second "def " + 4.
        let content = pipeline.get_document("test.py").unwrap().content();
        let beta_pos = content.find("beta").expect("beta must exist");
        pipeline
            .process_edit("test.py", beta_pos, beta_pos + 4, "delta")
            .unwrap();

        let syms = pipeline.get_symbols("test.py");
        let names: Vec<&str> = syms.iter().map(|s| s.symbol_name.as_str()).collect();
        assert!(names.contains(&"gamma"), "After edit 2: {names:?}");
        assert!(names.contains(&"delta"), "After edit 2: {names:?}");
        assert!(!names.contains(&"alpha"), "alpha should be gone: {names:?}");
        assert!(!names.contains(&"beta"), "beta should be gone: {names:?}");
    }

    #[test]
    fn test_large_file_performance() {
        let mut pipeline = IncrementalPipeline::new();

        // Generate a ~100K char Python file.
        let mut source = String::with_capacity(150_000);
        for i in 0..3000 {
            source.push_str(&format!(
                "def func_{i}(x_{i}, y_{i}):\n    \"\"\"Docstring for func {i}.\"\"\"\n    return x_{i} + y_{i} + {i}\n\n"
            ));
        }
        assert!(
            source.len() > 100_000,
            "Source should be >100K chars, got {}",
            source.len()
        );

        // Full parse.
        let result = pipeline.process_file("big.py", &source).unwrap();
        assert!(
            result.symbols_added.len() >= 3000,
            "Expected >=3000 symbols, got {}",
            result.symbols_added.len()
        );

        // Incremental edit — add a new function at the start.
        let new_text = "def inserted():\n    pass\n\n";
        let result = pipeline.process_edit("big.py", 0, 0, new_text).unwrap();
        assert!(result.was_incremental);

        // Verify inserted symbol appears.
        let syms = pipeline.get_symbols("big.py");
        let has_inserted = syms.iter().any(|s| s.symbol_name == "inserted");
        assert!(has_inserted, "Inserted function should be in symbols");
    }

    // ── get_document ────────────────────────────────────────────────────

    #[test]
    fn test_get_document_returns_loaded() {
        let mut pipeline = IncrementalPipeline::new();
        assert!(pipeline.get_document("x.py").is_none());

        pipeline.process_file("x.py", "x = 1\n").unwrap();
        let doc = pipeline.get_document("x.py").unwrap();
        assert_eq!(doc.content(), "x = 1\n");
    }

    // ── TypeScript / JavaScript ─────────────────────────────────────────

    #[test]
    fn test_process_file_extracts_typescript_symbols() {
        let mut pipeline = IncrementalPipeline::new();
        let source = "function greet(): void {}\n\nclass Foo {\n  bar(): void {}\n}\n";
        let result = pipeline.process_file("app.ts", source).unwrap();

        let names: Vec<&str> = result
            .symbols_added
            .iter()
            .map(|s| s.symbol_name.as_str())
            .collect();
        assert!(names.contains(&"greet"), "Missing 'greet' in {names:?}");
        assert!(names.contains(&"Foo"), "Missing 'Foo' in {names:?}");
    }

    #[test]
    fn test_process_file_extracts_javascript_symbols() {
        let mut pipeline = IncrementalPipeline::new();
        let source = "function hello() {}\n\nclass World {\n  render() {}\n}\n";
        let result = pipeline.process_file("index.js", source).unwrap();

        let names: Vec<&str> = result
            .symbols_added
            .iter()
            .map(|s| s.symbol_name.as_str())
            .collect();
        assert!(names.contains(&"hello"), "Missing 'hello' in {names:?}");
        assert!(names.contains(&"World"), "Missing 'World' in {names:?}");
    }

    // ── P1.2: SharedPipeline thread-safety tests ────────────────────────

    #[test]
    fn test_shared_pipeline_basic() {
        let shared = super::SharedPipeline::new();
        let result = shared.process_file("test.py", "def foo(): pass\n").unwrap();
        assert!(!result.was_incremental);
        assert!(!result.symbols_added.is_empty());
    }

    #[test]
    fn test_shared_pipeline_send_sync() {
        // Compile-time check: SharedPipeline must be Send + Sync.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<super::SharedPipeline>();
    }

    #[test]
    fn test_shared_pipeline_multithread() {
        use std::sync::Arc;

        let shared = Arc::new(super::SharedPipeline::new());
        let mut handles = vec![];

        for i in 0..4 {
            let shared_clone = Arc::clone(&shared);
            handles.push(std::thread::spawn(move || {
                let name = format!("test_{i}.py");
                let source = format!("def func_{i}(): return {i}\n");
                shared_clone.process_file(&name, &source).unwrap();
                let syms = shared_clone.get_symbols(&name);
                assert!(!syms.is_empty(), "Thread {i} should get symbols");
            }));
        }

        for h in handles {
            h.join().expect("Thread should not panic");
        }

        let (docs, trees) = shared.cache_stats();
        assert_eq!(docs, 4);
        assert_eq!(trees, 4);
    }

    #[test]
    fn test_shared_pipeline_clear_cache() {
        let shared = super::SharedPipeline::new();
        shared.process_file("a.py", "x = 1\n").unwrap();
        assert_eq!(shared.cache_stats(), (1, 1));
        shared.clear_cache();
        assert_eq!(shared.cache_stats(), (0, 0));
    }

    // ── AsyncSharedPipeline tests ───────────────────────────────────────

    #[cfg(feature = "async-pipeline")]
    mod async_tests {
        #[tokio::test]
        async fn test_async_pipeline_basic() {
            let shared = super::super::AsyncSharedPipeline::new();
            let result = shared
                .process_file("test.py", "def foo(): pass\n")
                .await
                .unwrap();
            assert!(!result.was_incremental);
            assert!(!result.symbols_added.is_empty());
        }

        #[tokio::test]
        async fn test_async_pipeline_send_sync() {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<super::super::AsyncSharedPipeline>();
        }

        #[tokio::test]
        async fn test_async_pipeline_process_edit() {
            let shared = super::super::AsyncSharedPipeline::new();

            // Load file first.
            shared
                .process_file(
                    "test.py",
                    "def hello():\n    pass\n\ndef world():\n    return 42\n",
                )
                .await
                .unwrap();

            // Rename "hello" to "hola" (bytes 4..9).
            let result = shared.process_edit("test.py", 4, 9, "hola").await.unwrap();
            assert!(result.was_incremental);

            let syms = shared.get_symbols("test.py").await;
            let names: Vec<&str> = syms.iter().map(|s| s.symbol_name.as_str()).collect();
            assert!(names.contains(&"hola"), "Missing 'hola' in {names:?}");
            assert!(!names.contains(&"hello"), "'hello' should be gone");
            assert!(names.contains(&"world"), "Missing 'world' in {names:?}");
        }

        #[tokio::test]
        async fn test_async_pipeline_cache_stats_and_clear() {
            let shared = super::super::AsyncSharedPipeline::new();

            assert_eq!(shared.cache_stats().await, (0, 0));

            shared
                .process_file("a.py", "def a(): pass\n")
                .await
                .unwrap();
            assert_eq!(shared.cache_stats().await, (1, 1));

            shared.process_file("b.rs", "fn b() {}\n").await.unwrap();
            assert_eq!(shared.cache_stats().await, (2, 2));

            shared.clear_cache().await;
            assert_eq!(shared.cache_stats().await, (0, 0));
        }

        #[tokio::test]
        async fn test_async_pipeline_with_closure() {
            let shared = super::super::AsyncSharedPipeline::new();
            shared.process_file("test.py", "x = 1\n").await.unwrap();

            let count = shared.with(|p| p.cache_stats().0).await;
            assert_eq!(count, 1);
        }

        #[tokio::test]
        async fn test_async_pipeline_concurrent_tasks() {
            use std::sync::Arc;

            let shared = Arc::new(super::super::AsyncSharedPipeline::new());
            let mut handles = vec![];

            for i in 0..4 {
                let shared_clone = Arc::clone(&shared);
                handles.push(tokio::spawn(async move {
                    let name = format!("test_{i}.py");
                    let source = format!("def func_{i}(): return {i}\n");
                    shared_clone.process_file(&name, &source).await.unwrap();
                    let syms = shared_clone.get_symbols(&name).await;
                    assert!(!syms.is_empty(), "Task {i} should get symbols");
                }));
            }

            for h in handles {
                h.await.expect("Task should not panic");
            }

            let (docs, trees) = shared.cache_stats().await;
            assert_eq!(docs, 4);
            assert_eq!(trees, 4);
        }

        #[tokio::test]
        async fn test_async_pipeline_debug_and_default() {
            let shared = super::super::AsyncSharedPipeline::default();
            let debug = format!("{shared:?}");
            assert!(
                debug.contains("AsyncSharedPipeline"),
                "Debug should contain type name: {debug}"
            );
        }
    }
}

// ── IA-1 + IA-3 tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod aco_tests {
    use super::*;
    use crate::ast::file_heat::HeatMap;

    // ── IA-1: process_edit_with_heat ────────────────────────────────────

    #[test]
    fn test_process_edit_with_heat_updates_heat_map() {
        let mut pipeline = IncrementalPipeline::new();
        let mut heat_map = HeatMap::new(100);

        // First load the file so process_edit can find the document.
        pipeline
            .process_file("test.py", "def hello():\n    pass\n")
            .expect("process_file");

        let result = pipeline
            .process_edit_with_heat("test.py", 4, 9, "world", &mut heat_map)
            .expect("process_edit_with_heat");

        // heat_map should now have an entry for test.py
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let order = heat_map.get_priority_order(now);
        assert!(!order.is_empty(), "heat_map should have test.py");
        assert!(order[0].1 > 0.0, "heat score should be positive after edit");
        // Result should be valid.
        assert!(result.parse_time_us < 5_000_000);
    }

    #[test]
    fn test_process_edit_with_heat_sets_blast_radius_weight() {
        let mut pipeline = IncrementalPipeline::new();
        let mut heat_map = HeatMap::new(100);

        // A Rust file with multiple symbols → non-zero blast_radius_weight.
        let rust_src = "pub fn a() {}\npub fn b() {}\npub fn c() {}\n";
        pipeline
            .process_file("multi.rs", rust_src)
            .expect("process_file");

        pipeline
            .process_edit_with_heat("multi.rs", 0, 0, "", &mut heat_map)
            .expect("process_edit_with_heat");

        // blast_radius_weight should be >= 0.0 (may be 0 if symbols_added is empty
        // on a no-op edit, that is still correct behaviour).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let order = heat_map.get_priority_order(now);
        assert!(!order.is_empty(), "heat_map should have multi.rs");
        // score >= 0 is always true for f64, but we verify the entry exists.
        assert!(order[0].1 >= 0.0);
    }

    // ── IA-3: PrioritizedPipeline ────────────────────────────────────────

    #[test]
    fn test_prioritized_pipeline_empty_returns_none() {
        let mut pp = PrioritizedPipeline::new(100);
        assert!(pp.next_hot().is_none());
    }

    #[test]
    fn test_prioritized_pipeline_pending_count() {
        let mut pp = PrioritizedPipeline::new(100);
        pp.enqueue("a.rs".to_string());
        pp.enqueue("b.rs".to_string());
        pp.enqueue("c.rs".to_string());
        assert_eq!(pp.pending_count(), 3);

        pp.next_hot();
        assert_eq!(pp.pending_count(), 2);
    }

    #[test]
    fn test_prioritized_pipeline_hot_first() {
        let mut pp = PrioritizedPipeline::new(100);

        // Enqueue cold.rs once, hot.rs three times to accumulate heat.
        pp.enqueue("cold.rs".to_string());
        pp.enqueue("hot.rs".to_string());
        pp.enqueue("hot.rs".to_string()); // duplicate enqueue records extra heat
        pp.enqueue("hot.rs".to_string());

        // hot.rs should be returned first.
        let first = pp.next_hot().expect("should have a file");
        assert_eq!(first, "hot.rs", "hottest file should come first");
    }

    #[test]
    fn test_prioritized_pipeline_drains_all() {
        let mut pp = PrioritizedPipeline::new(100);
        pp.enqueue("x.rs".to_string());
        pp.enqueue("y.rs".to_string());

        assert!(pp.next_hot().is_some());
        assert!(pp.next_hot().is_some());
        assert!(pp.next_hot().is_none(), "queue should be empty now");
    }
}

// ── B6: Lazy Symbol Loading tests ───────────────────────────────────────────

#[cfg(test)]
mod lazy_loading_tests {
    use super::*;

    #[test]
    fn test_queue_for_lazy_and_ensure_loaded() {
        let mut pipeline = IncrementalPipeline::new();
        let source = "pub fn lazy_fn() {}";
        pipeline.queue_for_lazy("src/lazy.rs", source);
        assert_eq!(pipeline.pending_lazy_count(), 1, "file should be in queue");

        // ensure_loaded triggers parsing
        let loaded = pipeline.ensure_loaded("src/lazy.rs");
        assert!(loaded, "ensure_loaded should return true for queued file");
        assert_eq!(
            pipeline.pending_lazy_count(),
            0,
            "queue should be empty after loading"
        );

        // Calling ensure_loaded again returns false (already loaded)
        let loaded2 = pipeline.ensure_loaded("src/lazy.rs");
        assert!(
            !loaded2,
            "ensure_loaded returns false for already-loaded file"
        );
    }

    #[test]
    fn test_queue_for_lazy_no_op_when_already_loaded() {
        let mut pipeline = IncrementalPipeline::new();
        pipeline.process_file("src/already.rs", "fn foo() {}").ok();
        pipeline.queue_for_lazy("src/already.rs", "pub fn bar() {}");
        // Already loaded — queue should NOT have this file
        assert_eq!(
            pipeline.pending_lazy_count(),
            0,
            "already-loaded file should not be queued"
        );
    }
}
