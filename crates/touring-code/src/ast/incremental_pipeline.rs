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
use crate::ast::file_heat::HeatMap;
use crate::ast::graph::SymbolLocation;
use crate::ast::languages::Lang;
use crate::ast::parser::IncrementalParser;
use crate::ast::revision::SymbolCache;
use crate::ast::store::SymbolStore;

/// Seconds since the Unix epoch — the clock the heat map is read against.
fn unix_now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

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
    /// What the budget and the heat did, for [`IncrementalPipeline::stats`].
    counters: BudgetCounters,
}

/// Counters behind [`PipelineMemoryStats`]; plain integers, the pipeline is `&mut` when they move.
#[derive(Debug, Default, Clone, Copy)]
struct BudgetCounters {
    evictions: u64,
    heat_consulted: u64,
    heat_prefetched: u64,
}

/// A reading of the pipeline's memory and of what the budget did — what
/// `touring status -j` reports under `pipeline`. (Not `PipelineStats`: that name is
/// the NLP pipeline's in `touring-intelligence`.)
///
/// `memory_mb` counts the rope bytes of the loaded documents (the unit the budget
/// is enforced in); the parse tree each file also holds is released together with
/// its document on eviction but is not part of this number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PipelineMemoryStats {
    /// Documents currently loaded.
    pub documents: usize,
    /// Parse trees currently cached.
    pub trees: usize,
    /// Rope bytes of the loaded documents, in megabytes.
    pub memory_mb: f64,
    /// Budget in whole megabytes; `0` = unlimited.
    pub budget_mb: usize,
    /// Documents evicted because the budget was exceeded.
    pub evictions: u64,
    /// Evictions decided with a non-empty heat map to rank the victims.
    pub heat_consulted: u64,
    /// Files loaded from disk by a heat-driven prefetch.
    pub heat_prefetched: u64,
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
            counters: BudgetCounters::default(),
        }
    }

    /// Create a pipeline backed by a [`SymbolStore`] at `db_path`.
    ///
    /// # Errors
    /// Returns an error if the SQLite database cannot be opened.
    fn with_symbol_store(db_path: &str) -> AstResult<Self> {
        let store = SymbolStore::new(Path::new(db_path))?;
        Ok(Self {
            parser: IncrementalParser::new(),
            documents: HashMap::new(),
            symbol_store: Some(store),
            symbol_cache: Mutex::new(SymbolCache::new()),
            max_memory_bytes: 0,
            counters: BudgetCounters::default(),
        })
    }

    // ── Memory budget ───────────────────────────────────────────────────

    /// Set a memory budget in megabytes for the loaded documents; `0` = unlimited
    /// (default). The budget is applied by [`enforce_budget`](Self::enforce_budget),
    /// which the caller runs after each file it feeds in: the caller owns the heat
    /// map that ranks the victims.
    pub fn with_memory_budget_mb(mut self, mb: usize) -> Self {
        self.max_memory_bytes = mb.saturating_mul(1024 * 1024);
        self
    }

    /// Rope bytes of the loaded documents, in megabytes — the unit the budget uses.
    pub fn memory_usage_mb(&self) -> f64 {
        let bytes: usize = self.documents.values().map(|d| d.len_bytes()).sum();
        bytes as f64 / (1024.0 * 1024.0)
    }

    /// Memory use and what the budget did so far.
    pub fn stats(&self) -> PipelineMemoryStats {
        PipelineMemoryStats {
            documents: self.documents.len(),
            trees: self.parser.cache_size(),
            memory_mb: self.memory_usage_mb(),
            budget_mb: self.max_memory_bytes / (1024 * 1024),
            evictions: self.counters.evictions,
            heat_consulted: self.counters.heat_consulted,
            heat_prefetched: self.counters.heat_prefetched,
        }
    }

    /// Evict documents, coldest first, until the loaded rope bytes fit the budget.
    /// Returns how many were evicted; `0` when the budget is unlimited or already met.
    ///
    /// Contract (the one policy — nothing else in the pipeline evicts):
    /// - `keep` — the file the caller just processed, `None` when there is none — is
    ///   never the victim, even when it alone exceeds the budget;
    /// - order is deterministic: a file with no entry in `heat` is colder than any
    ///   tracked one, then lower heat score first, then path order;
    /// - a victim loses its document, its parse tree and its symbol-cache entry, so
    ///   its next edit is a full re-parse (`process_file`), never a stale increment.
    pub fn enforce_budget(&mut self, heat: &HeatMap, keep: Option<&str>) -> usize {
        if self.max_memory_bytes == 0 {
            return 0;
        }
        let mut current: usize = self.documents.values().map(|d| d.len_bytes()).sum();
        if current <= self.max_memory_bytes {
            return 0;
        }
        let scores: HashMap<&str, f64> = heat
            .get_priority_order(unix_now_secs())
            .into_iter()
            .collect();
        let mut evicted = 0usize;
        while current > self.max_memory_bytes {
            let Some((path, bytes)) = self.coldest_document(&scores, keep) else {
                break; // only `keep` is left
            };
            self.evict_document(&path);
            current = current.saturating_sub(bytes);
            evicted += 1;
        }
        self.counters.evictions += evicted as u64;
        if evicted > 0 && !heat.is_empty() {
            self.counters.heat_consulted += 1;
        }
        evicted
    }

    /// The coldest loaded document other than `keep`, with its rope bytes: no heat
    /// entry first, then the lowest score, then path order (deterministic).
    fn coldest_document(
        &self,
        scores: &HashMap<&str, f64>,
        keep: Option<&str>,
    ) -> Option<(String, usize)> {
        let coldness = |path: &str| scores.get(path).copied().unwrap_or(f64::NEG_INFINITY);
        self.documents
            .iter()
            .filter(|(path, _)| Some(path.as_str()) != keep)
            .min_by(|(a, _), (b, _)| coldness(a).total_cmp(&coldness(b)).then_with(|| a.cmp(b)))
            .map(|(path, doc)| (path.clone(), doc.len_bytes()))
    }

    /// Drop everything the pipeline holds for `path`: document, parse tree, cached symbols.
    fn evict_document(&mut self, path: &str) {
        self.documents.remove(path);
        self.parser.invalidate_tree(path);
        self.symbol_cache
            .lock()
            .expect("symbol_cache lock")
            .remove(Path::new(path));
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
        if symbols_changed && let Some(store) = &self.symbol_store {
            self.persist_delta(store, file_path, &new_symbols)?;
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

        // Create / replace the document. The memory budget is NOT applied here: the
        // caller that owns the heat map runs `enforce_budget` after this returns.
        let doc = RopeDocument::new(file_path, content);
        self.documents.insert(file_path.to_string(), doc);

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
        if symbols_changed && let Some(store) = &self.symbol_store {
            self.persist_delta(store, file_path, &symbols)?;
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
        if let Some(tree) = self.parser.peek_tree(file_path)
            && let Some(doc) = self.documents.get(file_path)
        {
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

        // Fall back to SymbolStore.
        if let Some(store) = &self.symbol_store
            && let Ok(syms) = store.find_symbols_in_file(file_path)
        {
            return syms;
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
    fn cached_tree(&self, file_path: &str) -> bool {
        self.parser.cached_tree(file_path).is_some()
    }

    /// INS-A3: Pre-warm the parser cache for session-predicted files.
    ///
    /// `predicted` holds pipeline KEYS — the project-relative spelling the index
    /// uses. Each one not yet loaded is read from `root.join(key)` (an absolute key
    /// joins to itself) and processed under that same key, warming tree-sitter's
    /// incremental parse cache so that the first real edit is fast.
    ///
    /// Files that cannot be read (missing, unknown language, I/O error) are
    /// silently skipped — prefetch is best-effort.
    ///
    /// Returns the number of files successfully pre-warmed.
    pub fn prefetch_for_session(&mut self, root: &Path, predicted: &[String]) -> usize {
        let mut warmed = 0usize;
        for key in predicted {
            // Skip already-cached files — no work needed.
            if self.documents.contains_key(key.as_str()) {
                continue;
            }
            // Skip unknown languages early to avoid I/O.
            if Path::new(key).extension().is_none() {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(root.join(key))
                && self.process_file(key, &content).is_ok()
            {
                warmed += 1;
            }
        }
        warmed
    }

    /// Prefetch the top-`n` hottest files from the heat map.
    ///
    /// Ranks the keys by `heat_map` priority (heat × blast-radius weight, decayed to
    /// now) and warms the cache for the first `n` through
    /// [`prefetch_for_session`](Self::prefetch_for_session). Only keys `admit` accepts
    /// count toward the `n` (the heat map also holds files the index never admits —
    /// outside the project, excluded directories — and those must not be loaded). A
    /// budget that the prefetch overshoots is enforced right after, so it can only
    /// displace files colder than the ones it just loaded. Returns the number loaded.
    pub fn prefetch_from_heat(
        &mut self,
        root: &Path,
        heat_map: &HeatMap,
        n: usize,
        admit: impl Fn(&str) -> bool,
    ) -> usize {
        let hottest: Vec<String> = heat_map
            .get_priority_order(unix_now_secs())
            .into_iter()
            .filter(|(key, _score)| admit(key))
            .take(n)
            .map(|(key, _score)| key.to_string())
            .collect();
        let loaded = self.prefetch_for_session(root, &hottest);
        self.counters.heat_prefetched += loaded as u64;
        self.enforce_budget(heat_map, None);
        loaded
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

        // A markdown file is also ONE searchable document (its frontmatter name,
        // else its first heading, else its stem) — a memory with no heading is
        // otherwise invisible to every search (`ast::markdown_text`).
        crate::ast::markdown_text::with_document_symbol(
            file_path,
            source,
            self.run_query(&query, tree, source, file_path, lang),
        )
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
                    if let Some((parent, key)) = crate::ast::symbols::defining_node(node) {
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
}

// ── Thread-safe wrapper ─────────────────────────────────────────────────────

/// Thread-safe wrapper around [`IncrementalPipeline`] using a `RwLock`: shared
/// ownership across threads via `Arc`, concurrent readers, serialized writers —
/// better throughput than a `Mutex` for read-heavy workloads (`get_symbols`,
/// `cache_stats`, `stats`).
///
/// `Send + Sync` are proven by the compiler, not promised: `IncrementalPipeline`
/// owns no `tree_sitter::Parser` (those live in the per-thread cache of
/// `ast::grammar`), only trees, ropes, a `Mutex`-guarded symbol cache and a
/// `SymbolStore` whose connection sits behind a `Mutex`. The test
/// `test_shared_pipeline_send_sync` stops compiling the day a field breaks that.
///
/// # Usage
/// ```ignore
/// let shared = Arc::new(SharedPipeline::new());
/// // In any thread:
/// shared.with_write(|pipeline| pipeline.process_file("test.py", source));
/// ```
pub struct SharedPipeline {
    inner: parking_lot::RwLock<IncrementalPipeline>,
}

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

    /// Give the pipeline a memory budget in megabytes (`0` = unlimited); see
    /// [`IncrementalPipeline::enforce_budget`] for what the budget means.
    pub fn with_memory_budget_mb(self, mb: usize) -> Self {
        Self {
            inner: parking_lot::RwLock::new(self.inner.into_inner().with_memory_budget_mb(mb)),
        }
    }

    /// Memory use and what the budget did so far (thread-safe, shared read lock).
    pub fn stats(&self) -> PipelineMemoryStats {
        self.with_read(|p| p.stats())
    }

    /// Evict the coldest documents until the budget is met, never `keep` (the file
    /// the caller just processed, if any). Returns how many were evicted. See
    /// [`IncrementalPipeline::enforce_budget`].
    pub fn enforce_budget(&self, heat: &HeatMap, keep: Option<&str>) -> usize {
        self.with_write(|p| p.enforce_budget(heat, keep))
    }

    /// Warm the cache with the `n` hottest files of `heat` that `admit` accepts, read
    /// from under `root`. Returns how many were loaded. See
    /// [`IncrementalPipeline::prefetch_from_heat`].
    pub fn prefetch_from_heat(
        &self,
        root: &Path,
        heat: &HeatMap,
        n: usize,
        admit: impl Fn(&str) -> bool,
    ) -> usize {
        self.with_write(|p| p.prefetch_from_heat(root, heat, n, admit))
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

    /// The index path must agree with `extract_symbols`: a tuple-unpacked module
    /// binding defines EVERY name, each with the kind of the binding — not the
    /// first name alone typed as `pattern_list` (26/09/2026: `index find SENADOR`
    /// came back empty while `file_knowledge` already had it).
    #[test]
    fn test_process_file_indexes_every_name_of_a_module_tuple_unpacking() {
        let mut pipeline = IncrementalPipeline::new();
        let source = "PRESIDENTE, GOVERNADOR, SENADOR = \"1\", \"3\", \"5\"\n(low, high) = 0, 9\n";
        let result = pipeline.process_file("teto.py", source).unwrap();

        let got: Vec<(&str, Option<&str>)> = result
            .symbols_added
            .iter()
            .map(|s| (s.symbol_name.as_str(), s.kind.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("PRESIDENTE", Some("const")),
                ("GOVERNADOR", Some("const")),
                ("SENADOR", Some("const")),
                ("low", Some("variable")),
                ("high", Some("variable")),
            ]
        );
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
        // Compile-time check: no `unsafe impl` backs this — the compiler derives it, so the
        // day a field of the pipeline is not thread-safe, this stops compiling.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<super::IncrementalPipeline>();
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
}

// ── IA-1 + C7 tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod aco_tests {
    use super::*;
    use crate::ast::file_heat::HeatMap;

    /// A source of at least `bytes` bytes of valid Rust.
    fn rust_source_of(bytes: usize) -> String {
        "fn f() {}\n".repeat(bytes / 10 + 1)
    }

    /// A heat map where each `(key, edits)` was edited `edits` times just now.
    fn heat_of(entries: &[(&str, u32)]) -> HeatMap {
        let mut heat_map = HeatMap::new(100);
        for (key, edits) in entries {
            for _ in 0..*edits {
                heat_map.record_edit(key, unix_now_secs());
            }
        }
        heat_map
    }

    // ── Heat → cache warm-up ─────────────────────────────────────────────

    #[test]
    fn test_prefetch_from_heat_loads_only_the_hottest_files_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, body) in [
            ("hot.rs", "pub fn hot() {}\n"),
            ("warm.rs", "pub fn warm() {}\n"),
            ("cold.rs", "pub fn cold() {}\n"),
        ] {
            std::fs::write(dir.path().join(name), body).expect("write fixture");
        }
        // Keys are project-relative, the way the index spells them.
        let heat_map = heat_of(&[("hot.rs", 3), ("warm.rs", 2), ("cold.rs", 1)]);

        let mut pipeline = IncrementalPipeline::new();
        assert_eq!(
            pipeline.prefetch_from_heat(dir.path(), &heat_map, 2, |_| true),
            2,
            "the two hottest files are read from disk and parsed"
        );
        assert!(pipeline.get_document("hot.rs").is_some(), "hottest loaded");
        assert!(
            pipeline.get_document("warm.rs").is_some(),
            "second hottest loaded"
        );
        assert!(
            pipeline.get_document("cold.rs").is_none(),
            "the coldest stays unloaded"
        );
        assert_eq!(pipeline.stats().heat_prefetched, 2, "the counter moved");

        assert_eq!(
            pipeline.prefetch_from_heat(dir.path(), &heat_map, 2, |_| true),
            0,
            "files already loaded are not read again"
        );
    }

    #[test]
    fn test_prefetch_from_heat_counts_only_admitted_keys_toward_n() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["refused.rs", "ok_a.rs", "ok_b.rs"] {
            std::fs::write(dir.path().join(name), "pub fn f() {}\n").expect("write fixture");
        }
        // refused.rs is the hottest: without the filter it would take the only slot.
        let heat_map = heat_of(&[("refused.rs", 9), ("ok_a.rs", 3), ("ok_b.rs", 1)]);
        let mut pipeline = IncrementalPipeline::new();
        let loaded =
            pipeline.prefetch_from_heat(dir.path(), &heat_map, 1, |key| key != "refused.rs");
        assert_eq!(loaded, 1);
        assert!(pipeline.get_document("refused.rs").is_none(), "never read");
        assert!(
            pipeline.get_document("ok_a.rs").is_some(),
            "the hottest ADMITTED file"
        );
        assert!(pipeline.get_document("ok_b.rs").is_none(), "n = 1 is spent");
    }

    #[test]
    fn test_prefetch_from_heat_skips_a_hot_file_that_is_gone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let heat_map = heat_of(&[("gone.rs", 1)]);

        let mut pipeline = IncrementalPipeline::new();
        assert_eq!(
            pipeline.prefetch_from_heat(dir.path(), &heat_map, 5, |_| true),
            0,
            "prefetch is best-effort: an unreadable file is skipped, not an error"
        );
    }

    // ── C7: the memory budget, ranked by heat ────────────────────────────

    /// A pipeline with `files` loaded (400 KiB each) under a 1 MiB budget.
    fn loaded_pipeline(files: &[&str]) -> IncrementalPipeline {
        let mut pipeline = IncrementalPipeline::new().with_memory_budget_mb(1);
        let body = rust_source_of(400 * 1024);
        for file in files {
            pipeline.process_file(file, &body).expect("process_file");
        }
        pipeline
    }

    #[test]
    fn test_enforce_budget_never_evicts_the_file_it_was_told_to_keep() {
        let mut pipeline = IncrementalPipeline::new().with_memory_budget_mb(1);
        pipeline
            .process_file("big.rs", &rust_source_of(2 * 1024 * 1024))
            .expect("process_file");
        assert_eq!(
            pipeline.enforce_budget(&HeatMap::new(10), Some("big.rs")),
            0
        );
        assert!(
            pipeline.get_document("big.rs").is_some(),
            "the file just processed is never its own victim, even alone over the budget"
        );
    }

    #[test]
    fn test_enforce_budget_evicts_the_untracked_before_the_hottest() {
        // 3 x 400 KiB > 1 MiB: exactly one victim. cold1/cold2 have no heat entry.
        let mut pipeline = loaded_pipeline(&["hot.rs", "cold1.rs", "cold2.rs"]);
        let heat = heat_of(&[("hot.rs", 3)]);
        assert_eq!(pipeline.enforce_budget(&heat, Some("cold2.rs")), 1);
        assert!(
            pipeline.get_document("hot.rs").is_some(),
            "the hot file stays"
        );
        assert!(
            pipeline.get_document("cold1.rs").is_none(),
            "untracked, first in path order: the victim"
        );
        assert!(pipeline.get_document("cold2.rs").is_some(), "kept");
    }

    #[test]
    fn test_enforce_budget_among_tracked_files_evicts_the_lowest_score() {
        let mut pipeline = loaded_pipeline(&["a.rs", "b.rs", "c.rs"]);
        let heat = heat_of(&[("a.rs", 5), ("b.rs", 3), ("c.rs", 1)]);
        assert_eq!(pipeline.enforce_budget(&heat, None), 1);
        assert!(
            pipeline.get_document("c.rs").is_none(),
            "c.rs is the coldest"
        );
        assert!(pipeline.get_document("a.rs").is_some());
        assert!(pipeline.get_document("b.rs").is_some());
    }

    #[test]
    fn test_enforce_budget_without_heat_is_deterministic_by_path() {
        for _ in 0..4 {
            let mut pipeline = loaded_pipeline(&["z.rs", "m.rs", "a.rs"]);
            pipeline.enforce_budget(&HeatMap::new(10), Some("z.rs"));
            assert!(
                pipeline.get_document("a.rs").is_none(),
                "the same victim every time (no HashMap order)"
            );
        }
    }

    #[test]
    fn test_enforce_budget_releases_tree_and_symbols_with_the_document() {
        let mut pipeline = loaded_pipeline(&["a.rs", "b.rs", "c.rs"]);
        let heat = heat_of(&[("a.rs", 5), ("b.rs", 3), ("c.rs", 1)]);
        assert!(pipeline.cached_tree("c.rs") && !pipeline.get_symbols("c.rs").is_empty());
        pipeline.enforce_budget(&heat, None);
        assert!(
            !pipeline.cached_tree("c.rs"),
            "the tree goes with the document"
        );
        assert!(
            pipeline.get_symbols("c.rs").is_empty(),
            "so do the cached symbols"
        );
        assert!(
            pipeline.cached_tree("a.rs"),
            "the survivors keep their tree"
        );
    }

    #[test]
    fn test_enforce_budget_is_a_noop_when_unlimited_or_within_budget() {
        let mut unlimited = IncrementalPipeline::new();
        unlimited
            .process_file("a.rs", &rust_source_of(2 * 1024 * 1024))
            .expect("process_file");
        assert_eq!(unlimited.enforce_budget(&HeatMap::new(10), None), 0);
        let mut within = IncrementalPipeline::new().with_memory_budget_mb(8);
        within
            .process_file("a.rs", &rust_source_of(1024))
            .expect("process_file");
        assert_eq!(within.enforce_budget(&HeatMap::new(10), None), 0);
        assert!(within.get_document("a.rs").is_some());
    }

    #[test]
    fn test_stats_report_memory_budget_evictions_and_heat_use() {
        let mut pipeline = loaded_pipeline(&["a.rs", "b.rs", "c.rs"]);
        let before = pipeline.stats();
        assert_eq!(
            (
                before.documents,
                before.trees,
                before.budget_mb,
                before.evictions
            ),
            (3, 3, 1, 0)
        );
        assert!(
            before.memory_mb > 1.0,
            "the three documents exceed the budget"
        );

        pipeline.enforce_budget(&HeatMap::new(10), None);
        let blind = pipeline.stats();
        assert_eq!(
            (blind.evictions, blind.heat_consulted),
            (1, 0),
            "no heat to consult"
        );

        let mut again = loaded_pipeline(&["a.rs", "b.rs", "c.rs"]);
        again.enforce_budget(&heat_of(&[("a.rs", 2)]), None);
        let ranked = again.stats();
        assert_eq!((ranked.evictions, ranked.heat_consulted), (1, 1));
        assert!(ranked.memory_mb <= 1.0, "back inside the budget");
        assert_eq!((ranked.documents, ranked.trees), (2, 2));
    }

    #[test]
    fn test_prefetch_from_heat_under_a_budget_displaces_only_colder_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let body = rust_source_of(600 * 1024);
        for name in ["hot.rs", "warm.rs"] {
            std::fs::write(dir.path().join(name), &body).expect("write fixture");
        }
        let mut pipeline = IncrementalPipeline::new().with_memory_budget_mb(1);
        pipeline
            .process_file("stale.rs", &rust_source_of(600 * 1024))
            .expect("stale.rs");
        let heat = heat_of(&[("hot.rs", 4), ("warm.rs", 2)]);
        assert_eq!(
            pipeline.prefetch_from_heat(dir.path(), &heat, 1, |_| true),
            1
        );
        assert!(
            pipeline.get_document("hot.rs").is_some(),
            "the prefetched file"
        );
        assert!(
            pipeline.get_document("stale.rs").is_none(),
            "the untracked file made room for the hot one"
        );
    }

    #[test]
    fn test_shared_pipeline_budget_stats_and_enforce_round_trip() {
        let shared = SharedPipeline::new().with_memory_budget_mb(1);
        let body = rust_source_of(400 * 1024);
        for file in ["a.rs", "b.rs", "c.rs"] {
            shared.process_file(file, &body).expect("process_file");
        }
        assert_eq!(shared.stats().budget_mb, 1);
        assert_eq!(
            shared.enforce_budget(&heat_of(&[("a.rs", 3)]), Some("c.rs")),
            1
        );
        let stats = shared.stats();
        assert_eq!((stats.documents, stats.evictions), (2, 1));
    }
}
