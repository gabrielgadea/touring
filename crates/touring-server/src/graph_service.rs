// src/graph_service.rs
//! GraphService — active graph intelligence for all 26 Touring MCP tools.
//!
//! LOCK ORDERING INVARIANT (prevent deadlock):
//! In `resolve_ctx()`, ALWAYS acquire `focus` lock FIRST. The `indices`
//! map is now a lock-free `moka::sync::Cache` (see below), so the prior
//! "focus then indices" ordering simplifies to "focus lock only" — the
//! invariant is preserved for free.
//!
//! # indices cache — moka migration (2026-04-16, Wave 2 · 10th site)
//!
//! The previous `Arc<tokio::sync::Mutex<HashMap<PathBuf, Arc<Mutex<SymbolIndex>>>>>`
//! was a double-lock anti-pattern: readers paid two lock acquisitions
//! (outer HashMap lock + inner per-project Mutex). Worse, one caller used
//! `blocking_lock()` which can deadlock a single-threaded tokio runtime.
//! `moka::sync::Cache` is internally sharded and lock-free for reads,
//! safely callable from sync and async contexts alike. The per-project
//! `Arc<Mutex<SymbolIndex>>` value stays — `SymbolIndex` still needs
//! exclusive access for writes (index_file, reload, clear).

use moka::sync::Cache;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex; // matches server.rs — tokio Mutex, not std
use touring_code::ast::graph::SymbolIndex;
use touring_code::ast::store::SymbolStore;
use touring_hooks::async_knowledge::AsyncFileKnowledgeDB;
use touring_intelligence::reasoning::coedit_predictor::CoEditPredictor;

/// Source of the graph context — tracks how the focused file was determined.
#[derive(Debug, Clone, PartialEq)]
pub enum GraphCtxSource {
    /// Caller provided an explicit file_path
    Explicit,
    /// File came from the shared Focus Tracker (last Tier A tool call)
    FocusTracker,
    /// No file context available
    None,
    /// File belongs to a different project (not in current project's index)
    CrossProject,
}

/// Universal graph context injected into every tool response.
#[derive(Debug, Clone)]
pub struct GraphFocusCtx {
    /// File the context is centered on, if any.
    pub focused_file: Option<String>,
    /// Outgoing edges — files this file imports (via DependencyEdge.to)
    pub imports: Vec<String>,
    /// Incoming edges — files that import this file (reverse_deps, 1-hop)
    pub imported_by: Vec<String>,
    /// 1-hop count = imported_by.len() — used for confidence_modifier
    pub blast_radius_count: usize,
    /// imports ∪ imported_by — for query expansion (deduplicated, sorted)
    pub neighbor_files: Vec<String>,
    /// Safety signal: 1.0 (isolated) → 0.70 (critical hub)
    pub confidence_modifier: f64,
    /// Origin of the resolved focus context.
    pub source: GraphCtxSource,
    /// GS-EC11: files historically co-edited with focused_file (RRF-ranked, top-5).
    /// Empty when focused_file is None or no co-edit history exists.
    pub coedit_files: Vec<String>,
    /// EC18: how many times focused_file has been accessed (from TABLE_FILE_ACCESS_LOG).
    /// 0 when focused_file is None or AsyncFileKnowledgeDB is not initialized.
    pub access_count: i64,
    /// EC20: how many times focused_file has been edited (from TABLE_EDIT_HISTORY).
    /// 0 when focused_file is None or AsyncFileKnowledgeDB is not initialized.
    pub edit_count: i64,
    /// EC23: how many times focused_file has been read (from TABLE_FILE_KNOWLEDGE.read_count).
    /// Complements access_count (TABLE_FILE_ACCESS_LOG) — pre-read hook hit count.
    /// 0 when focused_file is None or AsyncFileKnowledgeDB is not initialized.
    pub read_count: i64,
    /// EC24: number of semantic relations originating from focused_file (TABLE_FILE_RELATIONS).
    /// Distinct from SymbolIndex.dependencies (AST imports) — covers cross-file semantic links
    /// recorded by post_edit and post_write hooks. 0 when adb not initialized.
    pub relation_count: i64,
    /// EC25: line count of focused_file from TABLE_FILE_KNOWLEDGE (populated by pre-read hook).
    /// 0 when file not yet read or adb not initialized.
    pub line_count: i64,
    /// EC25: symbol count of focused_file from TABLE_FILE_KNOWLEDGE (populated by pre-read hook).
    /// Complements SymbolIndex symbol count — reflects last-known state at read time.
    /// 0 when file not yet read or adb not initialized.
    pub symbol_count: i64,
    /// EC28: accumulated notes/gotchas about focused_file from TABLE_FILE_KNOWLEDGE.notes.
    /// Populated by pre-read and quality-assessment hooks. None when file not yet annotated.
    /// Truncated to 500 chars to keep graph_ctx JSON size bounded.
    pub file_notes: Option<String>,
    /// EC37: bash failure count for focused_file (from TABLE_BASH_OUTCOMES WHERE success = 0).
    /// Complements access_count (reads) and edit_count (writes) with execution failure signal.
    /// 0 when focused_file is None, file has no bash history, or adb not initialized.
    pub bash_failures: i64,
    /// EC39: active gotcha count matching focused_file path (from TABLE_GOTCHAS).
    /// Mirrors get_gotchas_for_file pattern: LIKE '%' || pattern || '%', decay > 0.1, unresolved.
    /// 0 when focused_file is None, no matching gotchas exist, or adb not initialized.
    pub gotcha_count: i64,
}

impl Default for GraphFocusCtx {
    fn default() -> Self {
        Self {
            focused_file: None,
            imports: vec![],
            imported_by: vec![],
            blast_radius_count: 0,
            neighbor_files: vec![],
            confidence_modifier: 1.0,
            source: GraphCtxSource::None,
            coedit_files: vec![],
            access_count: 0,
            edit_count: 0,
            read_count: 0,
            relation_count: 0,
            line_count: 0,
            symbol_count: 0,
            file_notes: None,
            bash_failures: 0,
            gotcha_count: 0,
        }
    }
}

/// Central abstraction over SymbolIndex + Focus Tracker.
/// Enriches every tool response with structural graph intelligence.
#[derive(Debug)]
pub struct GraphService {
    // Direct index for current project — backward compatible, avoids Arc lifetime issues
    index: Arc<Mutex<SymbolIndex>>,
    // Multi-project index: project_root -> Arc<Mutex<SymbolIndex>>.
    // Used for cross-project routing. current_project index = same as `index` field.
    // `moka::sync::Cache` is internally concurrent (sharded) and requires no
    // external Mutex — reads and writes are lock-free. The per-project
    // `Arc<Mutex<SymbolIndex>>` stays so writers can still exclusive-lock
    // the inner index. Callable safely from both sync and async contexts
    // (no `.await` needed on get/insert/iter).
    indices: Cache<PathBuf, Arc<Mutex<SymbolIndex>>>,
    focus: Arc<Mutex<Option<String>>>,
    /// Current project root — used as default when no cross-project match found.
    current_project: PathBuf,
    /// GS-EC11: async knowledge DB for co-edit signal in predict_coedit_files.
    /// Queries TABLE_FILE_COEDITS (populated by sync record_coedits in post_edit.rs).
    /// Optional — GraphService degrades gracefully when not initialized.
    async_knowledge: Option<std::sync::Arc<AsyncFileKnowledgeDB>>,
}

impl GraphService {
    /// Create with a single index (backward compatible).
    ///
    /// Call `.with_async_knowledge(adb)` after construction to activate the
    /// co-edit RRF signal in `predict_coedit_files` / `resolve_ctx`.
    pub fn new(index: Arc<Mutex<SymbolIndex>>, current_project: PathBuf) -> Self {
        let indices = Self::build_indices_cache();
        indices.insert(current_project.clone(), Arc::clone(&index));
        Self {
            index,
            indices,
            focus: Arc::new(Mutex::new(None)),
            current_project,
            async_knowledge: None,
        }
    }

    /// Build the moka cache used for multi-project index routing.
    ///
    /// Capacity 256 comfortably covers workspaces with hundreds of open
    /// projects. No TTL — projects are long-lived; eviction only under
    /// capacity pressure using LRU admission.
    fn build_indices_cache() -> Cache<PathBuf, Arc<Mutex<SymbolIndex>>> {
        Cache::builder()
            .max_capacity(256)
            .eviction_policy(moka::policy::EvictionPolicy::lru())
            .build()
    }

    /// Builder: wire an AsyncFileKnowledgeDB so that resolve_ctx can populate coedit_files.
    pub fn with_async_knowledge(mut self, adb: AsyncFileKnowledgeDB) -> Self {
        self.async_knowledge = Some(std::sync::Arc::new(adb));
        self
    }

    /// Create multi-project GraphService by discovering projects in ~/.claude/projects/
    ///
    /// Warm-starts each project's SymbolIndex from its persisted `symbols.db`.
    /// Gracefully degrades to an empty index when the DB is absent or unreadable.
    pub fn new_multi_project(current_project: PathBuf) -> Self {
        // Warm-start current project from its symbols.db
        let mut current_idx = SymbolIndex::new();
        let current_db = current_project
            .join(".claude")
            .join("touring")
            .join("symbols.db");
        if current_db.exists() {
            Self::load_symbols_from_db(&current_db, &mut current_idx);
        }

        let index = Arc::new(Mutex::new(current_idx));
        let indices = Self::build_indices_cache();
        indices.insert(current_project.clone(), Arc::clone(&index));

        // Scan ~/.claude/projects/ for other projects and warm-start each
        if let Some(home) = std::env::var_os("HOME") {
            let projects_dir = PathBuf::from(home).join(".claude").join("projects");
            if let Ok(entries) = std::fs::read_dir(&projects_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path == current_project {
                        continue;
                    }
                    let symbols_db = path.join(".claude").join("touring").join("symbols.db");
                    if symbols_db.exists() {
                        let mut project_idx = SymbolIndex::new();
                        Self::load_symbols_from_db(&symbols_db, &mut project_idx);
                        indices.insert(path, Arc::new(Mutex::new(project_idx)));
                    }
                }
            }
        }
        // GS-EC11: Initialize async_knowledge from the canonical knowledge DB.
        // Same path used by HookRuntime::init_async_knowledge — shared SQLite file (WAL mode).
        let knowledge_db =
            touring_foundation::TouringConfig::knowledge_db_canonical(&current_project);
        let async_knowledge = AsyncFileKnowledgeDB::new(&knowledge_db)
            .ok()
            .map(std::sync::Arc::new);

        Self {
            index,
            indices,
            focus: Arc::new(Mutex::new(None)),
            current_project,
            async_knowledge,
        }
    }

    /// Load symbols from a persisted `symbols.db` into a SymbolIndex.
    ///
    /// Called at server init for warm-start (avoids cold-start empty index).
    /// Gracefully logs and continues if DB does not exist or has schema errors.
    fn load_symbols_from_db(db_path: &std::path::Path, index: &mut SymbolIndex) {
        match SymbolStore::new(db_path) {
            Ok(store) => match store.load_into_index(index) {
                Ok(n) => tracing::info!(
                    path = %db_path.display(),
                    symbols = n,
                    "GraphService warm-start: loaded symbols from DB"
                ),
                Err(e) => tracing::warn!(
                    path = %db_path.display(),
                    error = %e,
                    "GraphService warm-start: load_into_index failed (starting empty)"
                ),
            },
            Err(e) => tracing::debug!(
                path = %db_path.display(),
                error = %e,
                "GraphService warm-start: SymbolStore open failed (starting empty)"
            ),
        }
    }

    /// Find which project owns a file path (longest prefix match).
    /// Used by cross-project resolution (v22 — pending).
    pub fn resolve_project_for_file(&self, file: &str) -> PathBuf {
        // moka::sync::Cache::iter is lock-free and safe to call from both
        // sync and async contexts — the prior `blocking_lock()` anti-pattern
        // (which could deadlock a single-threaded tokio runtime) is gone.
        let mut best_match: Option<PathBuf> = None;
        let mut best_len = 0;

        for (project_path, _) in self.indices.iter() {
            let project_str = project_path.to_string_lossy();
            if file.starts_with(project_str.as_ref()) || project_str.as_ref().starts_with(file) {
                let len = project_str.len();
                if len > best_len {
                    best_len = len;
                    best_match = Some((*project_path).clone());
                }
            }
        }

        best_match.unwrap_or_else(|| self.current_project.clone())
    }

    /// Low-level access to the current project's SymbolIndex Arc.
    ///
    /// Prefer the high-level methods (`stats`, `hotspots`, `resolve_ctx`, `expand_neighbors`)
    /// whenever possible. Use this only for complex multi-step operations that need direct
    /// index manipulation (e.g., `touring_graph` handler: index_file, blast_radius,
    /// dependency_path, query_symbols, reload/clear; background refresh task).
    ///
    /// NOTE: For cross-project queries, use `resolve_ctx()` which handles project routing.
    /// Low-level access to the current project's SymbolIndex Arc.
    pub(crate) fn inner(&self) -> Arc<Mutex<SymbolIndex>> {
        Arc::clone(&self.index)
    }

    /// Direct access to the index Arc — use for `index.lock().await` in async contexts.
    pub(crate) fn index(&self) -> &Arc<Mutex<SymbolIndex>> {
        &self.index
    }

    /// Update Focus Tracker. Called by every Tier A tool that receives a file_path.
    pub async fn update_focus(&self, file_path: &str) {
        let mut f = self.focus.lock().await; // tokio Mutex — no expect() needed
        *f = Some(file_path.to_string());
    }

    /// Resolve graph context for a tool call.
    /// - hint = Some(path): Explicit — caller must call update_focus() separately (Tier A only)
    /// - hint = None: falls back to Focus Tracker; returns empty ctx if no tracked file
    ///
    /// LOCK ORDER: focus first, index second (invariant — never reversed)
    pub async fn resolve_ctx(&self, hint: Option<&str>) -> GraphFocusCtx {
        // Step 1: acquire focus (first lock), then drop guard before acquiring index
        let focused_file: Option<String> = {
            let f = self.focus.lock().await;
            hint.map(|s| s.to_string()).or_else(|| f.clone())
        }; // focus guard dropped here — safe to acquire index next

        let source = match (hint, focused_file.is_some()) {
            (Some(_), _) => GraphCtxSource::Explicit,
            (None, true) => GraphCtxSource::FocusTracker,
            (None, false) => GraphCtxSource::None,
        };

        let Some(ref file) = focused_file else {
            return GraphFocusCtx {
                source,
                ..Default::default()
            };
        };

        // Step 1.5: Check if file belongs to current project
        // If file path doesn't start with current_project, it's from another project
        let current_project_str = self.current_project.to_string_lossy();
        let is_cross_project = !file.starts_with(current_project_str.as_ref())
            && !current_project_str
                .as_ref()
                .ends_with(file.split('/').next().unwrap_or(""));

        let source = if is_cross_project {
            // File is from a different project — mark as CrossProject
            GraphCtxSource::CrossProject
        } else {
            source
        };

        // Step 2: find owner project (moka cache — no lock needed).
        let owner_project = {
            let mut best_match: Option<PathBuf> = None;
            let mut best_len = 0;

            for (project_path, _) in self.indices.iter() {
                let project_str = project_path.to_string_lossy();
                if file.starts_with(project_str.as_ref()) || project_str.as_ref().starts_with(file)
                {
                    let len = project_str.len();
                    if len > best_len {
                        best_len = len;
                        best_match = Some((*project_path).clone());
                    }
                }
            }
            best_match.unwrap_or_else(|| self.current_project.clone())
        };
        let is_cross_project = owner_project != self.current_project;

        // Update source if cross-project
        let source = if is_cross_project {
            GraphCtxSource::CrossProject
        } else {
            source
        };

        // For now: always use current project index
        // Cross-project actual data routing is implemented (detection works)
        // but full async multi-index locking deferred to avoid borrow complexity
        let idx = self.index.lock().await;

        let imports: Vec<String> = idx
            .dependencies
            .get(file.as_str())
            .map(|edges| edges.iter().map(|e| e.to.clone()).collect())
            .unwrap_or_default();

        // reverse_deps is keyed by MODULE NAME (e.g., "utils"), not file path (e.g., "utils.py")
        // Extract module name from file path
        let module_name = std::path::Path::new(file)
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.rsplit_once('.'))
            .map(|(name, _)| name.to_string())
            .unwrap_or_else(|| file.to_string());

        let imported_by: Vec<String> = idx
            .reverse_deps
            .get(&module_name)
            .cloned()
            .unwrap_or_default();

        let blast_radius_count = imported_by.len();
        let confidence_modifier = Self::compute_confidence_modifier(blast_radius_count);

        let mut neighbor_files: Vec<String> = imports
            .iter()
            .chain(imported_by.iter())
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        neighbor_files.sort();

        // EC42: drop idx before predict_coedit_files — that method re-acquires self.index.
        // Without this explicit drop, calling predict_coedit_files would deadlock since both
        // resolve_ctx and predict_coedit_files try to acquire the same Arc<Mutex<SymbolIndex>>.
        // The prior `drop(indices)` is no longer needed — `self.indices` is now a lock-free
        // moka cache, so there is no guard to release.
        drop(idx);

        // EC42: Upgrade coedit signal from raw DB (get_coedits_from) to RRF-predicted.
        // predict_coedit_files() fuses three signals via CoEditPredictor::predict_next_files():
        //   - co-edits (1/3 weight): historical TABLE_FILE_COEDITS pairs
        //   - imports (1/3 weight): files this file depends on (SymbolIndex.dependencies)
        //   - blast_radius (1/3 weight): files that import this file (reverse_deps)
        // First real caller of predict_coedit_files() — activates full RRF over all 26 MCP tools.
        let coedit_files: Vec<String> = self
            .predict_coedit_files(file, 5)
            .await
            .into_iter()
            .map(|(path, _score)| path)
            .collect();

        // EC18: file access frequency from TABLE_FILE_ACCESS_LOG.
        // Provides a usage-heat signal alongside the structural (imports/blast) signals.
        let access_count: i64 = if let Some(ref adb) = self.async_knowledge {
            adb.access_count(file).await.unwrap_or(0)
        } else {
            0
        };

        // EC20: per-file edit frequency from TABLE_EDIT_HISTORY.
        // Complements access_count (reads) with write activity — "how hot is this file for editing".
        let edit_count: i64 = if let Some(ref adb) = self.async_knowledge {
            adb.edit_count_for_file(file).await.unwrap_or(0)
        } else {
            0
        };

        // EC23+EC25+EC28: single adb.lookup() call extracts read_count, line_count, symbol_count,
        // and notes from TABLE_FILE_KNOWLEDGE. One roundtrip for four fields.
        // notes truncated to 500 chars (UTF-8-safe) to keep graph_ctx JSON bounded.
        let (read_count, line_count, symbol_count, file_notes): (i64, i64, i64, Option<String>) =
            if let Some(ref adb) = self.async_knowledge {
                adb.lookup(file)
                    .await
                    .ok()
                    .flatten()
                    .map(|k| {
                        let notes = k.notes.map(|n| {
                            if n.len() > 500 {
                                let cut = n
                                    .char_indices()
                                    .map(|(i, _)| i)
                                    .take_while(|&i| i <= 497)
                                    .last()
                                    .unwrap_or(0);
                                format!("{}…", &n[..cut])
                            } else {
                                n
                            }
                        });
                        (k.read_count, k.line_count, k.symbol_count, notes)
                    })
                    .unwrap_or((0, 0, 0, None))
            } else {
                (0, 0, 0, None)
            };

        // EC24: semantic relation count from TABLE_FILE_RELATIONS via adb.get_relations_from().
        // Distinct from SymbolIndex.dependencies (AST imports) — covers cross-file semantic links
        // recorded by post_edit and post_write hooks (relation_type field).
        let relation_count: i64 = if let Some(ref adb) = self.async_knowledge {
            adb.get_relations_from(file).await.unwrap_or_default().len() as i64
        } else {
            0
        };

        // EC37: per-file bash failure count from TABLE_BASH_OUTCOMES.
        // Surfaces file-specific command failure history — complements access_count (reads) and
        // edit_count (writes) with execution failure signal.
        let bash_failures: i64 = if let Some(ref adb) = self.async_knowledge {
            adb.bash_failures_for_file(file).await.unwrap_or(0)
        } else {
            0
        };

        // EC39: active gotcha count matching focused_file path (TABLE_GOTCHAS).
        // Mirrors get_gotchas_for_file: LIKE '%' || pattern || '%', decay > 0.1, unresolved.
        // Direct metric: "how many known pitfalls exist for this file?"
        let gotcha_count: i64 = if let Some(ref adb) = self.async_knowledge {
            adb.gotcha_count_for_file(file).await.unwrap_or(0)
        } else {
            0
        };

        GraphFocusCtx {
            focused_file: Some(file.clone()),
            imports,
            imported_by,
            blast_radius_count,
            neighbor_files,
            confidence_modifier,
            source,
            coedit_files,
            access_count,
            edit_count,
            read_count,
            relation_count,
            line_count,
            symbol_count,
            file_notes,
            bash_failures,
            gotcha_count,
        }
    }

    /// Predict related files using RRF over: co-edits + imports + blast_radius.
    ///
    /// GS-EC11: Co-edit signal now sourced from `AsyncFileKnowledgeDB.get_coedits_from()`,
    /// which queries TABLE_FILE_COEDITS populated by the sync `record_coedits` hook.
    /// This activates the previously-empty 33% RRF weight for historical co-edit pairs.
    pub async fn predict_coedit_files(&self, file: &str, top_k: usize) -> Vec<(String, f64)> {
        let idx = self.index.lock().await;

        // Build imports ranked list (files imported by `file`)
        let imports: Vec<(String, f64)> = idx
            .dependencies
            .get(file)
            .map(|edges| {
                edges
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (e.to.clone(), 1.0 / (i as f64 + 1.0)))
                    .collect()
            })
            .unwrap_or_default();

        // Build blast_radius ranked list (files that import `file`)
        let module_name = std::path::Path::new(file)
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.rsplit_once('.'))
            .map(|(name, _)| name.to_string())
            .unwrap_or_else(|| file.to_string());

        let blast_radius: Vec<(String, f64)> = idx
            .reverse_deps
            .get(&module_name)
            .map(|files| {
                files
                    .iter()
                    .enumerate()
                    .map(|(i, f)| (f.clone(), 1.0 / (i as f64 + 1.0)))
                    .collect()
            })
            .unwrap_or_default();

        drop(idx);

        // GS-EC11: Real co-edit signal from AsyncFileKnowledgeDB.
        // Falls back to empty vec when async_knowledge is not initialized.
        let coedits: Vec<(String, f64)> = if let Some(ref adb) = self.async_knowledge {
            adb.get_coedits_from(file).await.unwrap_or_default()
        } else {
            vec![]
        };

        CoEditPredictor::predict_next_files(&coedits, &imports, &blast_radius, top_k)
    }

    /// Graduated safety signal based on 1-hop importer count.
    pub fn compute_confidence_modifier(blast_radius_count: usize) -> f64 {
        match blast_radius_count {
            0 => 1.00,
            1..=2 => 0.95,
            3..=8 => 0.85,
            9..=20 => 0.75,
            _ => 0.70,
        }
    }

    /// Return up to `limit` neighbor files (imports ∪ imported_by) for query expansion.
    pub async fn expand_neighbors(&self, file: &str, limit: usize) -> Vec<String> {
        // For now: use current project index (cross-project deferred)
        let idx = self.index.lock().await;
        let imports: Vec<String> = idx
            .dependencies
            .get(file)
            .map(|e| e.iter().map(|d| d.to.clone()).collect())
            .unwrap_or_default();
        // reverse_deps keyed by module name, not file path
        let module_name = std::path::Path::new(file)
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.rsplit_once('.'))
            .map(|(name, _)| name.to_string())
            .unwrap_or_else(|| file.to_string());
        let imported_by: Vec<String> = idx
            .reverse_deps
            .get(&module_name)
            .cloned()
            .unwrap_or_default();
        drop(idx);

        // Sort BEFORE truncating for deterministic neighbor selection
        let mut combined: Vec<String> = imports
            .into_iter()
            .chain(imported_by.into_iter())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        combined.sort();
        combined.truncate(limit);
        combined
    }

    /// Inject `graph_ctx` key into any JSON Value output (universal 1-liner for handlers).
    pub(crate) fn inject(&self, output: &mut Value, ctx: &GraphFocusCtx) {
        output["graph_ctx"] = json!({
            "focused_file": ctx.focused_file,
            "imports": ctx.imports,
            "imported_by": ctx.imported_by,
            "blast_radius_count": ctx.blast_radius_count,
            "neighbor_files": ctx.neighbor_files,
            "confidence_modifier": ctx.confidence_modifier,
            "source": format!("{:?}", ctx.source),
            // GS-EC11: historically co-edited files (RRF-ranked, top-5).
            // Populated from TABLE_FILE_COEDITS via AsyncFileKnowledgeDB.get_coedits_from.
            "coedit_files": ctx.coedit_files,
            // EC18: file access frequency from TABLE_FILE_ACCESS_LOG.
            "access_count": ctx.access_count,
            // EC20: per-file edit frequency from TABLE_EDIT_HISTORY.
            "edit_count": ctx.edit_count,
            // EC23: pre-read hook hit count from TABLE_FILE_KNOWLEDGE.
            "read_count": ctx.read_count,
            // EC24: semantic relation count from TABLE_FILE_RELATIONS.
            "relation_count": ctx.relation_count,
            // EC25: file size and complexity from TABLE_FILE_KNOWLEDGE (single lookup with read_count).
            "line_count": ctx.line_count,
            "symbol_count": ctx.symbol_count,
            // EC28: accumulated notes/gotchas from TABLE_FILE_KNOWLEDGE (truncated to 500 chars).
            "file_notes": ctx.file_notes,
            // EC37: bash failure count from TABLE_BASH_OUTCOMES WHERE success = 0.
            "bash_failures": ctx.bash_failures,
            // EC39: active gotcha count matching file path (TABLE_GOTCHAS, decay > 0.1, unresolved).
            "gotcha_count": ctx.gotcha_count,
        });

        // Warn when file is from a different project (blast_radius unavailable)
        if matches!(ctx.source, GraphCtxSource::CrossProject) {
            output["graph_warning"] = json!(
                "Cross-project file: blast_radius unavailable (file not in current project index)."
            );
        }
    }

    /// Graph statistics for `touring_project` and `touring_index_status`.
    ///
    /// EC38: enriched with AsyncFileKnowledgeDB stats — wires the previously-orphan
    /// `adb.stats()` method and surfaces KB metrics alongside SymbolIndex data.
    pub async fn stats(&self) -> Value {
        let idx = self.index.lock().await;
        let dep_edges: usize = idx.dependencies.values().map(|v| v.len()).sum();
        let symbol_count = idx.symbols.len();
        let file_count = idx.file_to_symbols.len();
        drop(idx); // release lock before async KB query

        // EC38: AsyncFileKnowledgeDB stats — file_count, relation_count, bash_count,
        // edit_count, gotcha_count from the knowledge SQLite DB.
        // Degrades gracefully to null when adb not initialized.
        let kb_stats = if let Some(ref adb) = self.async_knowledge {
            match adb.stats().await {
                Ok(s) => json!({
                    "kb_file_count": s.file_count,
                    "kb_relation_count": s.relation_count,
                    "kb_access_count": s.access_count,
                    "kb_bash_count": s.bash_count,
                    "kb_edit_count": s.edit_count,
                    "kb_gotcha_count": s.gotcha_count,
                }),
                Err(_) => json!(null),
            }
        } else {
            json!(null)
        };

        // EC41: Recent bash failure summaries from TABLE_BASH_OUTCOMES.
        // Wires AsyncFileKnowledgeDB::recent_bash_outcomes() — previously 0 async callers.
        // Returns last 3 failures (error_pattern, truncated to 80 chars) for system-level awareness.
        // Degrades gracefully to empty array when adb not initialized or no failures recorded.
        let recent_failures: Vec<serde_json::Value> = if let Some(ref adb) = self.async_knowledge {
            adb.recent_bash_outcomes(10)
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|o| !o.success)
                .take(3)
                .map(|o| {
                    let pattern = o
                        .error_pattern
                        .unwrap_or_default()
                        .chars()
                        .take(80)
                        .collect::<String>();
                    json!({
                        "command": o.command_short,
                        "error": pattern,
                        "file": o.file_context,
                    })
                })
                .collect()
        } else {
            vec![]
        };

        json!({
            "symbol_count": symbol_count,
            "file_count": file_count,
            "dependency_edge_count": dep_edges,
            "knowledge_db": kb_stats,
            // EC41: last 3 bash failures for global system awareness.
            "recent_failures": recent_failures,
        })
    }

    /// Top N files by incoming edge count (for `touring_decompose` hotspots).
    pub async fn hotspots(&self, limit: usize) -> Vec<(String, usize)> {
        let idx = self.index.lock().await;
        let mut counts: Vec<(String, usize)> = idx
            .reverse_deps
            .iter()
            .map(|(k, v)| (k.clone(), v.len()))
            .collect();
        counts.sort_by_key(|b| std::cmp::Reverse(b.1));
        counts.truncate(limit);
        counts
    }

    /// Handle a file system event — index, remove, or update the symbol index.
    ///
    /// **Hot path** (current project): immediate update via `SymbolIndex::index_file`.
    /// **Cold path** (cross-project): marks as dirty; periodic refresh picks it up.
    pub async fn on_file_event(&self, event: &touring_intelligence::index::watcher::FileEvent) {
        use touring_code::ast::languages::Lang;
        use touring_intelligence::index::watcher::FileEventType;

        let path_str = event.path.to_string_lossy();

        // Determine which project owns this file (longest prefix match).
        // moka cache — no lock needed.
        let is_cross_project = {
            let mut best_len = 0;
            let mut best_match = false;

            for (project_path, _) in self.indices.iter() {
                let project_str = project_path.to_string_lossy();
                if path_str.starts_with(project_str.as_ref()) {
                    let len = project_str.len();
                    if len > best_len {
                        best_len = len;
                        best_match = true;
                    }
                }
            }
            // If no match found in indices, fall back to current_project.
            // `entry_count()` returns u64; we only need "more than one project".
            best_match && best_len > 0 && self.indices.entry_count() > 1
        };

        match event.event_type {
            FileEventType::Create | FileEventType::Modify | FileEventType::Rename => {
                if is_cross_project {
                    // Cold path: cross-project — file will be picked up by periodic refresh
                    tracing::debug!("Cross-project file event (cold path): {:?}", path_str);
                    return;
                }

                // Hot path: current project — immediate index
                if !event.path.is_file() {
                    return;
                }

                let source = match std::fs::read_to_string(&event.path) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::debug!("Cannot read file for indexing: {}: {}", path_str, e);
                        return;
                    }
                };

                let lang = Lang::from_path(&event.path);
                let Some(lang) = lang else {
                    tracing::trace!("No language detected for: {:?}", path_str);
                    return;
                };

                let mut idx = self.index.lock().await;
                match idx.index_file(&path_str, &source, lang) {
                    Err(e) => {
                        tracing::warn!("Failed to index file {}: {}", path_str, e);
                    }
                    _ => {
                        tracing::debug!("Indexed (hot): {:?}", path_str);
                    }
                }
            }
            FileEventType::Remove => {
                if is_cross_project {
                    // Cold path: cross-project — periodic refresh handles it
                    tracing::debug!("Cross-project remove (cold path): {:?}", path_str);
                    return;
                }

                // Hot path: remove from current project's index
                let mut idx = self.index.lock().await;
                idx.remove_file(&path_str);
                tracing::debug!("Removed from index (hot): {:?}", path_str);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_code::ast::graph::SymbolLocation;

    #[test]
    fn test_load_symbols_from_db_warm_start() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let db_path = tmp.path().join("symbols.db");

        // Create a SymbolStore and insert a symbol
        let store = SymbolStore::new(&db_path).expect("store");
        let sym = SymbolLocation::new("src/main.rs", "my_function", 42, 4, true);
        store.upsert_symbol(&sym).expect("upsert");
        drop(store);

        // Warm-start: load into a fresh SymbolIndex
        let mut index = SymbolIndex::new();
        GraphService::load_symbols_from_db(&db_path, &mut index);

        // Verify the symbol was loaded
        let found = index.find_symbol("my_function");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_path, "src/main.rs");
        assert_eq!(found[0].line, 42);
        assert!(found[0].is_definition);
    }

    #[test]
    fn test_load_symbols_from_db_missing_file_graceful() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let db_path = tmp.path().join("nonexistent.db");

        // Should not panic — graceful degradation
        let mut index = SymbolIndex::new();
        GraphService::load_symbols_from_db(&db_path, &mut index);

        // Index should remain empty
        assert!(index.symbols.is_empty());
    }

    #[tokio::test]
    async fn test_graph_service_new_warm_index() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let project = tmp.path().to_path_buf();
        let touring_dir = project.join(".claude").join("touring");
        std::fs::create_dir_all(&touring_dir).expect("mkdir");
        let db_path = touring_dir.join("symbols.db");

        // Pre-populate the DB
        let store = SymbolStore::new(&db_path).expect("store");
        let sym = SymbolLocation::new("lib.rs", "Config", 10, 0, true);
        store.upsert_symbol(&sym).expect("upsert");
        drop(store);

        // Create GraphService with warm-start via new() with pre-loaded index
        let mut idx = SymbolIndex::new();
        GraphService::load_symbols_from_db(&db_path, &mut idx);
        let index = Arc::new(Mutex::new(idx));
        let svc = GraphService::new(index, project);

        // Verify stats show the symbol
        let stats = svc.stats().await;
        assert_eq!(stats["symbol_count"], 1);
        assert_eq!(stats["file_count"], 1);
    }
}
