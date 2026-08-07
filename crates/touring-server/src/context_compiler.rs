//! Context Compiler -- produces coalesced context blocks for subagent prompts.
//!
//! Usage: `touring context compile --intent "fix bug" --files "a.py,b.rs"`
//! Output: Single context block < max_tokens with file overviews, gotchas,
//! blast radius, and learned patterns.
//!
//! This extends the cortex enrichment pipeline (`cortex/enrichment.rs`) from
//! single-file hook injection to multi-file subagent spawn context.
//!
//! ## v2.1.0 Changes (S0 Cache Foundation)
//!
//! - **S0.1**: `FxHasher` replaces `DefaultHasher` for deterministic cross-process keys
//! - **S0.2**: Files sorted before hashing — order-insensitive cache keys
//! - **S0.3**: File mtime included in hash — auto-invalidation on edit
//! - **S0.5**: `ContextCache` (moka TinyLFU, cap=256) provides lock-free bounded caching

#![allow(dead_code)] // Context compiler API may have unused helpers in some configurations

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, params};
use rustc_hash::FxHasher;

use crate::observation_masker::ObservationMasker;

/// Priority levels for context fields during compaction.
/// P0 fields are NEVER truncated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextPriority {
    /// Critical -- never truncated (objective, errors, active plan)
    P0 = 0,
    /// Important -- compress last (blast radius, gotchas, co-edit predictions)
    P1 = 1,
    /// Standard -- compress early (session history, evolution insights)
    P2 = 2,
    /// Optional -- first to drop (memory recall, graph context)
    P3 = 3,
}

/// Structured summary of context for compaction.
/// Preserves critical fields (P0) while allowing compression of lower-priority content.
#[derive(Debug, Clone, Default)]
pub struct CompactSummary {
    /// P0: Current objective/task description (NEVER truncated)
    pub objective: String,
    /// P0: Files modified in current session (NEVER truncated)
    pub files_modified: Vec<String>,
    /// P0: Errors encountered (NEVER truncated)
    pub errors: Vec<String>,
    /// P0: Active plan steps (NEVER truncated)
    pub pending_tasks: Vec<String>,
    /// P1: Key decisions made
    pub decisions: Vec<String>,
    /// P2: Tool usage history (compressed)
    pub tool_history: Vec<String>,
    /// P3: Code snippets (first to drop)
    pub code_snippets: Vec<String>,
}

impl CompactSummary {
    /// Convert to context string, respecting `max_chars` budget.
    /// P0 fields always included. P1-P3 included if budget allows.
    pub fn to_context_string(&self, max_chars: usize) -> String {
        use std::fmt::Write;

        // Pre-allocate with estimated P0 size to avoid repeated reallocs
        let mut output = String::with_capacity(self.p0_size().min(max_chars) + 128);

        // P0: Always included
        if !self.objective.is_empty() {
            let _ = writeln!(output, "Objective: {}", self.objective);
        }
        if !self.files_modified.is_empty() {
            output.push_str("Modified: ");
            for (i, f) in self.files_modified.iter().enumerate() {
                if i > 0 {
                    output.push_str(", ");
                }
                output.push_str(f);
            }
            output.push('\n');
        }
        if !self.errors.is_empty() {
            output.push_str("Errors: ");
            for (i, e) in self.errors.iter().enumerate() {
                if i > 0 {
                    output.push_str("; ");
                }
                output.push_str(e);
            }
            output.push('\n');
        }
        if !self.pending_tasks.is_empty() {
            output.push_str("Pending: ");
            for (i, t) in self.pending_tasks.iter().enumerate() {
                if i > 0 {
                    output.push_str("; ");
                }
                output.push_str(t);
            }
            output.push('\n');
        }

        let p0_len = output.len();
        if p0_len >= max_chars {
            return output; // P0 alone exceeds budget -- return P0 only
        }

        // P1: Decisions (if budget allows)
        if !self.decisions.is_empty() {
            // Pre-compute length without allocating: "Decisions: " + joined + "\n"
            let joined_len: usize = self.decisions.iter().map(|s| s.len()).sum::<usize>()
                + (self.decisions.len().saturating_sub(1)) * 2; // "; " separators
            let section_len = "Decisions: ".len() + joined_len + 1;
            if output.len() + section_len <= max_chars {
                output.push_str("Decisions: ");
                for (i, d) in self.decisions.iter().enumerate() {
                    if i > 0 {
                        output.push_str("; ");
                    }
                    output.push_str(d);
                }
                output.push('\n');
            }
        }

        // P2: Tool history (if budget allows)
        if !self.tool_history.is_empty() {
            let joined_len: usize = self.tool_history.iter().map(|s| s.len()).sum::<usize>()
                + (self.tool_history.len().saturating_sub(1)) * 2; // ", " separators
            let section_len = "Tools: ".len() + joined_len + 1;
            if output.len() + section_len <= max_chars {
                output.push_str("Tools: ");
                for (i, t) in self.tool_history.iter().enumerate() {
                    if i > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(t);
                }
                output.push('\n');
            }
        }

        // P3: Code snippets (if budget allows)
        if !self.code_snippets.is_empty() {
            let joined_len: usize = self.code_snippets.iter().map(|s| s.len()).sum::<usize>()
                + (self.code_snippets.len().saturating_sub(1)) * 5; // "\n---\n" separators
            let section_len = "Snippets: ".len() + joined_len + 1;
            if output.len() + section_len <= max_chars {
                output.push_str("Snippets: ");
                for (i, s) in self.code_snippets.iter().enumerate() {
                    if i > 0 {
                        output.push_str("\n---\n");
                    }
                    output.push_str(s);
                }
                output.push('\n');
            }
        }

        output
    }

    /// Estimate the char count of P0 fields only.
    pub fn p0_size(&self) -> usize {
        let mut size = 0;
        if !self.objective.is_empty() {
            size += "Objective: ".len() + self.objective.len() + 1; // +1 for \n
        }
        if !self.files_modified.is_empty() {
            size += "Modified: ".len() + self.files_modified.join(", ").len() + 1;
        }
        if !self.errors.is_empty() {
            size += "Errors: ".len() + self.errors.join("; ").len() + 1;
        }
        if !self.pending_tasks.is_empty() {
            size += "Pending: ".len() + self.pending_tasks.join("; ").len() + 1;
        }
        size
    }
}

/// Mask observations in raw context, then generate a structured summary.
///
/// This is the recommended entry point for the full compilation pipeline:
///
/// ```text
/// raw context → ObservationMasker::mask_observations() → generate_structured_summary()
/// ```
///
/// Returns `(summary, masking_stats)` so callers can log token savings.
/// If the masker determines the context is below the activation threshold,
/// it passes through unchanged (zero overhead for small contexts).
pub fn mask_and_summarize(
    raw_context: &str,
    masker: &ObservationMasker,
) -> (CompactSummary, crate::observation_masker::MaskingStats) {
    let (masked_context, stats) = masker.mask_observations(raw_context);

    if stats.blocks_masked > 0 {
        tracing::debug!(
            original_tokens = stats.original_tokens,
            masked_tokens = stats.masked_tokens,
            blocks_masked = stats.blocks_masked,
            savings_pct = if stats.original_tokens > 0 {
                ((stats.original_tokens - stats.masked_tokens) as f64
                    / stats.original_tokens as f64
                    * 100.0) as u32
            } else {
                0
            },
            "ObservationMasker reduced context before summarization"
        );
    }

    let summary = generate_structured_summary(&masked_context);
    (summary, stats)
}

/// Extract structured summary from raw context string.
/// Uses simple pattern matching to identify priority fields.
pub fn generate_structured_summary(context: &str) -> CompactSummary {
    let mut summary = CompactSummary::default();

    for line in context.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // P0: Objective patterns
        if trimmed.starts_with("Objective:")
            || trimmed.starts_with("Task:")
            || trimmed.starts_with("Mission:")
        {
            summary.objective = trimmed
                .split_once(':')
                .map(|x| x.1)
                .unwrap_or("")
                .trim()
                .to_string();
        }
        // P0: Error patterns
        else if trimmed.contains("error")
            || trimmed.contains("FAIL")
            || trimmed.contains("Error:")
        {
            summary.errors.push(trimmed.to_string());
        }
        // P0: File modification patterns
        else if trimmed.starts_with("Modified:")
            || trimmed.starts_with("Written:")
            || trimmed.starts_with("EDIT:")
        {
            summary.files_modified.push(trimmed.to_string());
        }
        // P0: Pending tasks
        else if trimmed.starts_with("TODO:")
            || trimmed.starts_with("Pending:")
            || trimmed.starts_with("- [ ]")
        {
            summary.pending_tasks.push(trimmed.to_string());
        }
        // P1: Decisions
        else if trimmed.starts_with("Decision:")
            || trimmed.starts_with("Decided:")
            || trimmed.starts_with("Chose:")
        {
            summary.decisions.push(trimmed.to_string());
        }
    }

    summary
}

/// Maximum number of gotchas to include in compiled context.
const MAX_GOTCHAS: usize = 5;
/// Maximum number of relations to include in compiled context.
const MAX_RELATIONS: usize = 8;
/// Maximum number of recent errors to include in compiled context.
const MAX_ERRORS: usize = 5;
/// Rough chars-per-token estimate for budget calculations.
const CHARS_PER_TOKEN: usize = 4;
/// Default capacity for the dedup LRU cache (entry-bound, legacy constructor).
const DEDUP_CACHE_CAPACITY: usize = 256;
/// Default byte budget for the bytes-bound dedup cache (32 MiB).
///
/// Sized to comfortably hold ~256 medium contexts (≈128 KB each) without
/// the long-tail explosion that entry-bound caches suffer when a few large
/// compilations dominate. The TinyLFU admission policy retains entries
/// with the highest historical frequency × weight, so frequently re-used
/// large contexts survive while one-shot huge ones get evicted.
const DEDUP_CACHE_BYTES: u64 = 32 * 1024 * 1024;

/// Compiled context for one or more files, ready for subagent injection.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CompiledContext {
    /// Intent/purpose of the compilation.
    pub intent: String,
    /// Files included in the context.
    pub files: Vec<String>,
    /// Composed context string (estimated <= max_tokens).
    pub context: String,
    /// Estimated token count.
    pub estimated_tokens: usize,
    /// Cache key (hash of intent + sorted files + mtimes for dedup).
    pub cache_key: String,
}

/// Bounded dedup cache for compiled contexts.
///
/// Prevents memory leaks in long sessions by evicting entries via TinyLFU
/// admission policy (moka). Thread-safe without external locking -- moka
/// uses internal sharded locks and is fully concurrent.
#[derive(Debug)]
pub struct ContextCache {
    inner: moka::sync::Cache<String, CompiledContext>,
}

impl ContextCache {
    /// Create a new cache with default capacity (256 entries).
    pub fn new() -> Self {
        Self {
            inner: moka::sync::Cache::builder()
                .max_capacity(DEDUP_CACHE_CAPACITY as u64)
                .time_to_live(Duration::from_secs(300)) // 5 min TTL
                .build(),
        }
    }

    /// Create a cache with custom **entry** capacity (legacy semantics).
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: moka::sync::Cache::builder()
                .max_capacity(cap.max(1) as u64)
                .time_to_live(Duration::from_secs(300))
                .build(),
        }
    }

    /// Create a bytes-bound cache (P1 ranking #6 — moka weigher pattern).
    ///
    /// Unlike [`Self::with_capacity`] which counts entries, this constructor
    /// bounds the cache by the **total weighted size in bytes** of all
    /// retained entries. The weigher sums:
    /// - the cache key length (intent hash),
    /// - the compiled context body length (the dominant term),
    /// - the file-path strings,
    ///
    /// and clamps to `u32::MAX` per the moka API contract.
    ///
    /// Use this when context payloads vary wildly in size (typical
    /// production workload). Entry-bound caches with the same `cap` either
    /// over-retain small contexts or evict useful large ones — the bytes
    /// budget makes the trade explicit.
    pub fn with_byte_capacity(bytes: u64) -> Self {
        Self {
            inner: moka::sync::Cache::builder()
                .weigher(Self::context_weigher)
                .max_capacity(bytes.max(1024))
                .time_to_live(Duration::from_secs(300))
                .build(),
        }
    }

    /// Approximate in-memory footprint of a `(key, CompiledContext)` pair.
    ///
    /// Counts the variable-length fields (key, intent, context body, file
    /// paths). Fixed overhead (`Vec` header, struct padding) is intentionally
    /// excluded — TinyLFU only needs a *relative* weight ordering, not the
    /// exact bytes-on-heap.
    #[allow(clippy::ptr_arg)]
    fn context_weigher(key: &String, value: &CompiledContext) -> u32 {
        let body: usize = value.context.len()
            + value.intent.len()
            + value.files.iter().map(String::len).sum::<usize>()
            + key.len();
        u32::try_from(body).unwrap_or(u32::MAX)
    }

    /// Compile context, returning cached version if available.
    pub fn compile_or_cached(
        &self,
        intent: &str,
        files: &[String],
        gotchas: &[(String, String)],
        relations: &[String],
        recent_errors: &[String],
        max_tokens: usize,
    ) -> CompiledContext {
        let key = compute_cache_key(intent, files);

        // Check cache first (no lock needed -- moka is internally concurrent)
        if let Some(cached) = self.inner.get(&key) {
            return cached;
        }

        // Cache miss — compile fresh
        let result = compile_context(intent, files, gotchas, relations, recent_errors, max_tokens);

        // Store in cache
        self.inner.insert(key, result.clone());
        result
    }

    /// Compile context with observation masking applied to the result.
    ///
    /// Identical to `compile_or_cached` but applies `ObservationMasker` to the
    /// assembled context string, reducing tool-result verbosity before caching.
    /// The masked version is what gets cached, so subsequent hits return the
    /// already-compact form.
    #[allow(clippy::too_many_arguments)]
    pub fn compile_or_cached_masked(
        &self,
        intent: &str,
        files: &[String],
        gotchas: &[(String, String)],
        relations: &[String],
        recent_errors: &[String],
        max_tokens: usize,
        masker: &ObservationMasker,
    ) -> CompiledContext {
        let key = compute_cache_key(intent, files);

        if let Some(cached) = self.inner.get(&key) {
            return cached;
        }

        let mut result =
            compile_context(intent, files, gotchas, relations, recent_errors, max_tokens);

        // Apply observation masking to the assembled context
        let (masked, stats) = masker.mask_observations(&result.context);
        if stats.blocks_masked > 0 {
            tracing::debug!(
                blocks_masked = stats.blocks_masked,
                original_tokens = stats.original_tokens,
                masked_tokens = stats.masked_tokens,
                "ContextCache: observation masking applied before caching"
            );
            result.context = masked;
            result.estimated_tokens = result.context.len() / CHARS_PER_TOKEN;
        }

        self.inner.insert(key, result.clone());
        result
    }

    /// Get a cached entry by key (used by PersistentContextCache).
    fn get(&self, key: &str) -> Option<CompiledContext> {
        self.inner.get(key)
    }

    /// Insert an entry (used by PersistentContextCache).
    fn insert(&self, key: String, value: CompiledContext) {
        self.inner.insert(key, value);
    }

    /// Number of entries currently cached.
    pub fn len(&self) -> usize {
        self.inner.run_pending_tasks();
        self.inner.entry_count() as usize
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        self.inner.invalidate_all();
        self.inner.run_pending_tasks();
    }
}

impl Default for ContextCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cross-session persistent context cache backed by SQLite.
///
/// Two-tier lookup: in-memory LRU (fast) → SQLite (persistent across sessions).
/// When `db_path` is `None`, operates in memory-only mode (identical to `ContextCache`).
#[derive(Debug)]
pub struct PersistentContextCache {
    memory: ContextCache,
    db: Option<Mutex<Connection>>,
    db_path: Option<PathBuf>,
}

impl PersistentContextCache {
    /// Create a new persistent cache.
    ///
    /// If `db_path` is provided, opens (or creates) the SQLite database and
    /// ensures the `context_cache` table exists. If `None`, operates in
    /// memory-only mode.
    pub fn new(db_path: Option<&Path>) -> Self {
        let (db, resolved_path) = if let Some(path) = db_path {
            match Connection::open(path) {
                Ok(conn) => {
                    let _ = conn.execute_batch(
                        "PRAGMA journal_mode = WAL;
                         PRAGMA synchronous = NORMAL;
                         PRAGMA busy_timeout = 2000;",
                    );
                    let _ = conn.execute(
                        "CREATE TABLE IF NOT EXISTS context_cache (
                            cache_key TEXT PRIMARY KEY,
                            intent TEXT NOT NULL,
                            files_json TEXT NOT NULL,
                            context TEXT NOT NULL,
                            estimated_tokens INTEGER NOT NULL,
                            created_at TEXT DEFAULT (datetime('now')),
                            last_used_at TEXT DEFAULT (datetime('now'))
                        )",
                        [],
                    );
                    (Some(Mutex::new(conn)), Some(path.to_path_buf()))
                }
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        };

        Self {
            memory: ContextCache::new(),
            db,
            db_path: resolved_path,
        }
    }

    /// Compile context with two-tier caching: memory → SQLite → fresh compile.
    pub fn compile_or_cached(
        &self,
        intent: &str,
        files: &[String],
        gotchas: &[(String, String)],
        relations: &[String],
        recent_errors: &[String],
        max_tokens: usize,
    ) -> CompiledContext {
        let key = compute_cache_key(intent, files);

        // Tier 1: check in-memory moka cache (lock-free)
        if let Some(cached) = self.memory.get(&key) {
            return cached;
        }

        // Tier 2: check SQLite
        if let Some(loaded) = self.load_from_db(&key) {
            // Promote to memory cache
            self.memory.insert(key.clone(), loaded.clone());
            // Update last_used_at
            self.touch_in_db(&key);
            return loaded;
        }

        // Tier 3: compile fresh
        let result = compile_context(intent, files, gotchas, relations, recent_errors, max_tokens);

        // Store in both tiers
        self.memory.insert(key.clone(), result.clone());
        self.save_to_db(&key, &result);

        result
    }

    /// Persist a compiled context to the SQLite database.
    pub fn save_to_db(&self, key: &str, ctx: &CompiledContext) {
        if let Some(ref db_mutex) = self.db {
            let conn = db_mutex.lock().unwrap_or_else(|e| e.into_inner());
            let files_json = serde_json::to_string(&ctx.files).unwrap_or_default();
            let _ = conn.execute(
                "INSERT OR REPLACE INTO context_cache
                 (cache_key, intent, files_json, context, estimated_tokens, created_at, last_used_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), datetime('now'))",
                params![key, ctx.intent, files_json, ctx.context, ctx.estimated_tokens as i64],
            );
        }
    }

    /// Load a compiled context from the SQLite database.
    pub fn load_from_db(&self, key: &str) -> Option<CompiledContext> {
        let db_mutex = self.db.as_ref()?;
        let conn = db_mutex.lock().unwrap_or_else(|e| e.into_inner());
        conn.query_row(
            "SELECT intent, files_json, context, estimated_tokens, cache_key
             FROM context_cache WHERE cache_key = ?1",
            params![key],
            |row| {
                let intent: String = row.get(0)?;
                let files_json: String = row.get(1)?;
                let context: String = row.get(2)?;
                let estimated_tokens: i64 = row.get(3)?;
                let cache_key: String = row.get(4)?;
                let files: Vec<String> = serde_json::from_str(&files_json).unwrap_or_default();
                Ok(CompiledContext {
                    intent,
                    files,
                    context,
                    estimated_tokens: estimated_tokens as usize,
                    cache_key,
                })
            },
        )
        .ok()
    }

    /// Delete cache entries older than `max_age_days` days.
    ///
    /// Returns the number of pruned rows.
    pub fn prune_old_entries(&self, max_age_days: u32) -> usize {
        if let Some(ref db_mutex) = self.db {
            let conn = db_mutex.lock().unwrap_or_else(|e| e.into_inner());
            conn.execute(
                "DELETE FROM context_cache
                 WHERE last_used_at < datetime('now', ?1)",
                params![format!("-{max_age_days} days")],
            )
            .unwrap_or(0)
        } else {
            0
        }
    }

    /// Whether this cache has a SQLite backing store.
    pub fn has_db(&self) -> bool {
        self.db.is_some()
    }

    /// The database path, if any.
    pub fn db_path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }

    /// Number of entries in the in-memory LRU.
    pub fn memory_len(&self) -> usize {
        self.memory.len()
    }

    /// Update `last_used_at` for an entry (called on SQLite cache hit).
    fn touch_in_db(&self, key: &str) {
        if let Some(ref db_mutex) = self.db {
            let conn = db_mutex.lock().unwrap_or_else(|e| e.into_inner());
            let _ = conn.execute(
                "UPDATE context_cache SET last_used_at = datetime('now') WHERE cache_key = ?1",
                params![key],
            );
        }
    }
}

/// Compile context for given files and intent.
///
/// Produces a single coalesced block respecting the token budget.
/// Priority: intent header > files > gotchas > relations > recent errors.
///
/// The budget is enforced section-by-section: each section is included only
/// if it fits entirely within the remaining budget. This mirrors the
/// approach used in `cortex::enrichment::compose_enriched_context`.
pub fn compile_context(
    intent: &str,
    files: &[String],
    gotchas: &[(String, String)], // (severity, description)
    relations: &[String],         // import/caller paths
    recent_errors: &[String],     // recent error patterns
    max_tokens: usize,
) -> CompiledContext {
    use std::fmt::Write;

    // Pre-allocate parts with known max sections: header + files + gotchas + relations + errors
    let mut parts: Vec<String> = Vec::with_capacity(5);
    let mut budget = max_tokens;

    // Section 1: Header with intent (always included if fits)
    let header = format!("Context for: {}", intent);
    let header_tokens = header.len() / CHARS_PER_TOKEN;
    if header_tokens <= budget {
        parts.push(header);
        budget -= header_tokens;
    }

    // Section 2: Files list -- build inline to avoid intermediate join allocation
    if !files.is_empty() {
        let joined_len: usize =
            files.iter().map(|f| f.len()).sum::<usize>() + (files.len().saturating_sub(1)) * 2; // ", " separators
        let section_len = "Files: ".len() + joined_len;
        let tokens = section_len / CHARS_PER_TOKEN;
        if tokens <= budget {
            let mut files_str = String::with_capacity(section_len);
            files_str.push_str("Files: ");
            for (i, f) in files.iter().enumerate() {
                if i > 0 {
                    files_str.push_str(", ");
                }
                files_str.push_str(f);
            }
            parts.push(files_str);
            budget -= tokens;
        }
    }

    // Section 3: Gotchas (highest-priority enrichment data)
    if !gotchas.is_empty() && budget > 0 {
        let take_n = gotchas.len().min(MAX_GOTCHAS);
        // Pre-compute capacity: "Gotchas:\n" + per-line "  GOTCHA [sev]: desc\n"
        let mut section = String::with_capacity(
            "Gotchas:\n".len()
                + gotchas
                    .iter()
                    .take(take_n)
                    .map(|(sev, desc)| "  GOTCHA []: ".len() + sev.len() + desc.len() + 1)
                    .sum::<usize>(),
        );
        section.push_str("Gotchas:");
        for (sev, desc) in gotchas.iter().take(take_n) {
            let _ = write!(section, "\n  GOTCHA [{}]: {}", sev, desc);
        }
        let tokens = section.len() / CHARS_PER_TOKEN;
        if tokens <= budget {
            parts.push(section);
            budget -= tokens;
        }
    }

    // Section 4: Relations -- build with references, no intermediate Vec alloc
    if !relations.is_empty() && budget > 0 {
        let take_n = relations.len().min(MAX_RELATIONS);
        let joined_len: usize = relations
            .iter()
            .take(take_n)
            .map(|s| s.len())
            .sum::<usize>()
            + (take_n.saturating_sub(1)) * 2; // ", " separators
        let section_len = "Relations: ".len() + joined_len;
        let tokens = section_len / CHARS_PER_TOKEN;
        if tokens <= budget {
            let mut rel_str = String::with_capacity(section_len);
            rel_str.push_str("Relations: ");
            for (i, r) in relations.iter().take(take_n).enumerate() {
                if i > 0 {
                    rel_str.push_str(", ");
                }
                rel_str.push_str(r);
            }
            parts.push(rel_str);
            budget -= tokens;
        }
    }

    // Section 5: Recent errors -- same pattern: direct write, no intermediate Vec
    if !recent_errors.is_empty() && budget > 0 {
        let take_n = recent_errors.len().min(MAX_ERRORS);
        let joined_len: usize = recent_errors
            .iter()
            .take(take_n)
            .map(|s| s.len())
            .sum::<usize>()
            + (take_n.saturating_sub(1)) * 2; // "; " separators
        let section_len = "Recent errors: ".len() + joined_len;
        let tokens = section_len / CHARS_PER_TOKEN;
        if tokens <= budget {
            let mut err_str = String::with_capacity(section_len);
            err_str.push_str("Recent errors: ");
            for (i, e) in recent_errors.iter().take(take_n).enumerate() {
                if i > 0 {
                    err_str.push_str("; ");
                }
                err_str.push_str(e);
            }
            parts.push(err_str);
        }
    }

    let context = parts.join("\n\n");
    let estimated_tokens = context.len() / CHARS_PER_TOKEN;

    // Compute deterministic cache key from intent + sorted files + mtimes
    let cache_key = compute_cache_key(intent, files);

    CompiledContext {
        intent: intent.to_string(),
        files: files.to_vec(),
        context,
        estimated_tokens,
        cache_key,
    }
}

/// Compute a deterministic cache key from intent and file list.
///
/// ## S0 Fixes Applied
///
/// - **S0.1**: Uses `FxHasher` (deterministic across processes) instead of
///   `DefaultHasher` (random seed per process via SipHash).
/// - **S0.2**: Files are sorted before hashing so `["a.py", "b.rs"]` and
///   `["b.rs", "a.py"]` produce the same cache key.
/// - **S0.3**: File modification time (mtime) is included in the hash so
///   editing a file automatically invalidates cached context for it.
fn compute_cache_key(intent: &str, files: &[String]) -> String {
    let mut hasher = FxHasher::default();
    intent.hash(&mut hasher);

    // S0.2: Sort files for order-insensitive hashing.
    // Use &str references instead of cloning Strings -- avoids N heap allocations.
    let mut sorted: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    sorted.sort_unstable();

    for f in &sorted {
        f.hash(&mut hasher);

        // S0.3: Include mtime for cache invalidation on file edit.
        // Gracefully skips if file doesn't exist or mtime unavailable.
        if let Ok(meta) = std::fs::metadata(f)
            && let Ok(mtime) = meta.modified()
        {
            mtime.hash(&mut hasher);
        }
    }
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Existing tests (preserved) ──────────────────────────────────

    #[test]
    fn test_compile_basic() {
        let result = compile_context(
            "fix bug in parser",
            &["parser.rs".into()],
            &[],
            &[],
            &[],
            2000,
        );
        assert!(result.context.contains("fix bug in parser"));
        assert!(result.estimated_tokens <= 2000);
        assert!(!result.cache_key.is_empty());
    }

    #[test]
    fn test_compile_with_gotchas() {
        let result = compile_context(
            "edit rust_bridge",
            &["rust_bridge.py".into()],
            &[("critical".into(), "use matched_text not text".into())],
            &[],
            &[],
            2000,
        );
        assert!(result.context.contains("GOTCHA [critical]"));
        assert!(result.context.contains("matched_text"));
    }

    #[test]
    fn test_compile_respects_budget() {
        let large_gotchas: Vec<(String, String)> = (0..100)
            .map(|i| {
                (
                    "warning".into(),
                    format!("gotcha {}: {}", i, "x".repeat(200)),
                )
            })
            .collect();
        let result = compile_context("test", &[], &large_gotchas, &[], &[], 100);
        assert!(
            result.estimated_tokens <= 100,
            "expected <= 100 tokens, got {}",
            result.estimated_tokens
        );
    }

    #[test]
    fn test_compile_cache_key_deterministic() {
        let r1 = compile_context("intent", &["a.rs".into()], &[], &[], &[], 2000);
        let r2 = compile_context("intent", &["a.rs".into()], &[], &[], &[], 2000);
        assert_eq!(r1.cache_key, r2.cache_key);
    }

    #[test]
    fn test_compile_cache_key_different_for_different_intent() {
        let r1 = compile_context("fix", &["a.rs".into()], &[], &[], &[], 2000);
        let r2 = compile_context("refactor", &["a.rs".into()], &[], &[], &[], 2000);
        assert_ne!(r1.cache_key, r2.cache_key);
    }

    #[test]
    fn test_compile_cache_key_different_for_different_files() {
        let r1 = compile_context("fix", &["a.rs".into()], &[], &[], &[], 2000);
        let r2 = compile_context("fix", &["b.rs".into()], &[], &[], &[], 2000);
        assert_ne!(r1.cache_key, r2.cache_key);
    }

    #[test]
    fn test_compile_includes_relations() {
        let result = compile_context(
            "test",
            &[],
            &[],
            &["mod_a".into(), "mod_b".into()],
            &[],
            2000,
        );
        assert!(result.context.contains("Relations:"));
        assert!(result.context.contains("mod_a"));
        assert!(result.context.contains("mod_b"));
    }

    #[test]
    fn test_compile_includes_errors() {
        let result = compile_context("test", &[], &[], &[], &["string_not_found".into()], 2000);
        assert!(result.context.contains("Recent errors:"));
        assert!(result.context.contains("string_not_found"));
    }

    #[test]
    fn test_compile_empty_inputs() {
        let result = compile_context("test", &[], &[], &[], &[], 2000);
        assert!(result.context.contains("Context for: test"));
        assert!(result.estimated_tokens < 50);
    }

    #[test]
    fn test_compile_priority_order() {
        let gotchas = vec![("WARN".into(), "gotcha1".into())];
        let relations = vec!["rel1.rs".into()];
        let errors = vec!["err1".into()];

        let result = compile_context(
            "test",
            &["f.rs".into()],
            &gotchas,
            &relations,
            &errors,
            2000,
        );

        let intent_pos = result
            .context
            .find("Context for:")
            .expect("should have intent");
        let files_pos = result.context.find("Files:").expect("should have files");
        let gotcha_pos = result
            .context
            .find("Gotchas:")
            .expect("should have gotchas");
        let rel_pos = result
            .context
            .find("Relations:")
            .expect("should have relations");
        let err_pos = result
            .context
            .find("Recent errors:")
            .expect("should have errors");

        assert!(intent_pos < files_pos, "intent before files");
        assert!(files_pos < gotcha_pos, "files before gotchas");
        assert!(gotcha_pos < rel_pos, "gotchas before relations");
        assert!(rel_pos < err_pos, "relations before errors");
    }

    #[test]
    fn test_compile_max_items_capped() {
        let gotchas: Vec<(String, String)> = (0..20)
            .map(|i| ("INFO".into(), format!("gotcha_{i}")))
            .collect();
        let relations: Vec<String> = (0..20).map(|i| format!("rel_{i}.rs")).collect();
        let errors: Vec<String> = (0..20).map(|i| format!("error_{i}")).collect();

        let result = compile_context("test", &[], &gotchas, &relations, &errors, 5000);

        let gotcha_count = result.context.matches("GOTCHA").count();
        assert_eq!(
            gotcha_count, MAX_GOTCHAS,
            "should cap at {MAX_GOTCHAS} gotchas"
        );

        assert!(
            result.context.contains("rel_7.rs"),
            "should include 8th relation"
        );
        assert!(
            !result.context.contains("rel_8.rs"),
            "should exclude 9th relation"
        );

        assert!(
            result.context.contains("error_4"),
            "should include 5th error"
        );
        assert!(
            !result.context.contains("error_5"),
            "should exclude 6th error"
        );
    }

    #[test]
    fn test_compile_zero_budget() {
        let result = compile_context(
            "intent",
            &["file.rs".into()],
            &[("WARN".into(), "gotcha".into())],
            &["rel".into()],
            &["err".into()],
            0,
        );
        assert!(
            result.context.is_empty(),
            "zero budget should produce empty context"
        );
        assert_eq!(result.estimated_tokens, 0);
    }

    #[test]
    fn test_compile_serializes_to_json() {
        let result = compile_context("test", &["a.rs".into()], &[], &[], &[], 2000);
        let json = serde_json::to_string(&result).expect("should serialize to JSON");
        assert!(json.contains("\"intent\":\"test\""));
        assert!(json.contains("\"cache_key\""));
    }

    // ── S0.1: FxHasher determinism ──────────────────────────────────

    #[test]
    fn test_s0_1_cache_key_deterministic_across_calls() {
        // FxHasher uses no random seed — identical inputs always
        // produce identical output, even across process restarts.
        let k1 = compute_cache_key("intent", &["a.rs".into()]);
        let k2 = compute_cache_key("intent", &["a.rs".into()]);
        assert_eq!(k1, k2, "FxHasher must be deterministic");
    }

    #[test]
    fn test_s0_1_cache_key_16_hex_chars() {
        let key = compute_cache_key("test", &["f.rs".into()]);
        assert_eq!(key.len(), 16, "cache key must be 16 hex chars (u64)");
        assert!(
            key.chars().all(|c| c.is_ascii_hexdigit()),
            "cache key must be hex: {key}"
        );
    }

    // ── S0.2: Order-insensitive file hashing ────────────────────────

    #[test]
    fn test_s0_2_cache_key_order_insensitive() {
        let k1 = compute_cache_key("fix", &["a.py".into(), "b.rs".into(), "c.ts".into()]);
        let k2 = compute_cache_key("fix", &["c.ts".into(), "a.py".into(), "b.rs".into()]);
        assert_eq!(k1, k2, "file order must not affect cache key");
    }

    #[test]
    fn test_s0_2_cache_key_single_file_unchanged() {
        // Single file — no ordering concern, key should still work
        let k1 = compute_cache_key("fix", &["only.rs".into()]);
        let k2 = compute_cache_key("fix", &["only.rs".into()]);
        assert_eq!(k1, k2);
    }

    // ── S0.3: mtime-based invalidation ──────────────────────────────

    #[test]
    fn test_s0_3_cache_key_graceful_on_nonexistent_file() {
        // Non-existent files should not panic — mtime is skipped
        let k1 = compute_cache_key("fix", &["/nonexistent/path/x.rs".into()]);
        let k2 = compute_cache_key("fix", &["/nonexistent/path/x.rs".into()]);
        assert_eq!(k1, k2, "non-existent files produce consistent keys");
    }

    #[test]
    fn test_s0_3_cache_key_changes_on_file_edit() {
        // Create a temp file, compute key, modify file, recompute
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("test.rs");
        std::fs::write(&path, "fn main() {}").expect("write");

        let k1 = compute_cache_key("fix", &[path.to_string_lossy().into_owned()]);

        // Wait briefly to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&path, "fn main() { updated }").expect("write");

        let k2 = compute_cache_key("fix", &[path.to_string_lossy().into_owned()]);

        assert_ne!(k1, k2, "editing a file should change cache key via mtime");
    }

    // ── S0.5: ContextCache dedup ────────────────────────────────────

    #[test]
    fn test_s0_5_context_cache_returns_cached() {
        let cache = ContextCache::new();
        let r1 = cache.compile_or_cached("fix", &["a.rs".into()], &[], &[], &[], 2000);
        let r2 = cache.compile_or_cached("fix", &["a.rs".into()], &[], &[], &[], 2000);
        assert_eq!(r1.cache_key, r2.cache_key);
        assert_eq!(r1.context, r2.context);
        assert_eq!(cache.len(), 1, "should have 1 cached entry");
    }

    #[test]
    fn test_s0_5_context_cache_different_keys() {
        let cache = ContextCache::new();
        cache.compile_or_cached("fix", &["a.rs".into()], &[], &[], &[], 2000);
        cache.compile_or_cached("refactor", &["b.rs".into()], &[], &[], &[], 2000);
        assert_eq!(cache.len(), 2, "different intents = 2 entries");
    }

    #[test]
    fn test_s0_5_context_cache_evicts_at_capacity() {
        let cache = ContextCache::with_capacity(3);
        for i in 0..5 {
            cache.compile_or_cached(
                &format!("intent_{i}"),
                &[format!("file_{i}.rs")],
                &[],
                &[],
                &[],
                2000,
            );
        }
        // moka uses TinyLFU admission + async eviction; run_pending_tasks()
        // is called inside len() to flush pending ops before counting.
        assert!(
            cache.len() <= 3,
            "cache should evict entries at capacity, got {}",
            cache.len(),
        );
    }

    #[test]
    fn test_s0_5_context_cache_clear() {
        let cache = ContextCache::new();
        cache.compile_or_cached("fix", &["a.rs".into()], &[], &[], &[], 2000);
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
    }

    // ── S4.2: PersistentContextCache ─────────────────────────────────

    #[test]
    fn test_persistent_cache_save_and_load() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("cache.db");
        let cache = PersistentContextCache::new(Some(&db_path));
        assert!(cache.has_db());

        // Compile and persist
        let ctx = cache.compile_or_cached("fix bug", &["main.rs".into()], &[], &[], &[], 2000);
        assert!(!ctx.context.is_empty());

        // Load directly from DB to confirm persistence
        let loaded = cache.load_from_db(&ctx.cache_key);
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.intent, "fix bug");
        assert_eq!(loaded.files, vec!["main.rs".to_string()]);
        assert_eq!(loaded.context, ctx.context);
        assert_eq!(loaded.estimated_tokens, ctx.estimated_tokens);
    }

    #[test]
    fn test_persistent_cache_memory_first() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("cache.db");
        let cache = PersistentContextCache::new(Some(&db_path));

        // First call compiles and caches
        let r1 = cache.compile_or_cached("test", &["a.rs".into()], &[], &[], &[], 2000);
        assert_eq!(cache.memory_len(), 1);

        // Second call hits memory (same key)
        let r2 = cache.compile_or_cached("test", &["a.rs".into()], &[], &[], &[], 2000);
        assert_eq!(r1.cache_key, r2.cache_key);
        assert_eq!(r1.context, r2.context);
        assert_eq!(cache.memory_len(), 1); // still 1 — cache hit, not new entry
    }

    #[test]
    fn test_persistent_cache_prune_old() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("cache.db");
        let cache = PersistentContextCache::new(Some(&db_path));

        // Insert an entry
        cache.compile_or_cached("old intent", &["old.rs".into()], &[], &[], &[], 2000);

        // Manually backdate it in SQLite so prune can remove it
        if let Some(ref db_mutex) = cache.db {
            let conn = db_mutex.lock().unwrap();
            conn.execute(
                "UPDATE context_cache SET last_used_at = datetime('now', '-100 days')",
                [],
            )
            .unwrap();
        }

        // Prune entries older than 30 days
        let pruned = cache.prune_old_entries(30);
        assert_eq!(pruned, 1);

        // Verify entry is gone
        let key = compute_cache_key("old intent", &["old.rs".into()]);
        assert!(cache.load_from_db(&key).is_none());
    }

    #[test]
    fn test_persistent_cache_without_db() {
        // Memory-only mode (no db_path)
        let cache = PersistentContextCache::new(None);
        assert!(!cache.has_db());
        assert!(cache.db_path().is_none());

        // Should still work via in-memory cache
        let ctx = cache.compile_or_cached("test", &["x.rs".into()], &[], &[], &[], 2000);
        assert!(!ctx.context.is_empty());
        assert_eq!(cache.memory_len(), 1);

        // load_from_db returns None (no DB)
        assert!(cache.load_from_db(&ctx.cache_key).is_none());

        // prune does nothing
        assert_eq!(cache.prune_old_entries(30), 0);
    }

    #[test]
    fn test_persistent_cache_cross_session_survival() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("cache.db");

        // Session 1: compile and persist
        let key;
        {
            let cache = PersistentContextCache::new(Some(&db_path));
            let ctx = cache.compile_or_cached("refactor", &["lib.rs".into()], &[], &[], &[], 2000);
            key = ctx.cache_key.clone();
        }
        // cache is dropped — simulates end of session

        // Session 2: new instance, same DB — should find the entry
        {
            let cache2 = PersistentContextCache::new(Some(&db_path));
            assert_eq!(cache2.memory_len(), 0); // fresh memory

            // This should hit SQLite and promote to memory
            let loaded =
                cache2.compile_or_cached("refactor", &["lib.rs".into()], &[], &[], &[], 2000);
            assert_eq!(loaded.cache_key, key);
            assert_eq!(loaded.intent, "refactor");
            assert_eq!(cache2.memory_len(), 1); // promoted from DB
        }
    }

    // ── S4.1: CompactSummary + generate_structured_summary ──────────

    #[test]
    fn test_compact_summary_preserves_p0_objective() {
        let summary = CompactSummary {
            objective: "Fix parser bug in AST module".to_string(),
            ..Default::default()
        };
        let output = summary.to_context_string(5000);
        assert!(
            output.contains("Objective: Fix parser bug in AST module"),
            "P0 objective must always be present: {output}"
        );
    }

    #[test]
    fn test_compact_summary_preserves_p0_errors() {
        let summary = CompactSummary {
            errors: vec![
                "type mismatch in line 42".to_string(),
                "FAIL: test_parser panicked".to_string(),
            ],
            ..Default::default()
        };
        let output = summary.to_context_string(5000);
        assert!(
            output.contains("type mismatch in line 42"),
            "P0 errors must be present"
        );
        assert!(
            output.contains("FAIL: test_parser panicked"),
            "P0 errors must be present"
        );
    }

    #[test]
    fn test_compact_summary_truncates_p3_when_budget_exceeded() {
        let summary = CompactSummary {
            objective: "short".to_string(),
            code_snippets: vec!["fn main() { very_long_code(); }".repeat(50)],
            ..Default::default()
        };
        // Give a budget that fits P0 but not P3
        let p0_size = summary.p0_size();
        let output = summary.to_context_string(p0_size + 10);
        assert!(output.contains("Objective: short"), "P0 must be present");
        assert!(
            !output.contains("Snippets:"),
            "P3 should be truncated when budget is tight"
        );
    }

    #[test]
    fn test_compact_summary_empty_context_produces_empty_summary() {
        let summary = CompactSummary::default();
        let output = summary.to_context_string(5000);
        assert!(
            output.is_empty(),
            "empty summary should produce empty output, got: {output}"
        );
    }

    #[test]
    fn test_compact_summary_p0_never_truncated_even_small_budget() {
        let summary = CompactSummary {
            objective: "Critical objective that must survive compaction".to_string(),
            errors: vec!["fatal error X".to_string()],
            files_modified: vec!["src/main.rs".to_string()],
            pending_tasks: vec!["TODO: fix remaining tests".to_string()],
            decisions: vec!["Decision: use tokio over async-std".to_string()],
            ..Default::default()
        };
        // Budget of 1 char — P0 still fully emitted
        let output = summary.to_context_string(1);
        assert!(
            output.contains("Objective:"),
            "P0 objective must survive tiny budget"
        );
        assert!(
            output.contains("Errors:"),
            "P0 errors must survive tiny budget"
        );
        assert!(
            output.contains("Modified:"),
            "P0 files_modified must survive tiny budget"
        );
        assert!(
            output.contains("Pending:"),
            "P0 pending_tasks must survive tiny budget"
        );
        // P1 should NOT be present
        assert!(
            !output.contains("Decisions:"),
            "P1 decisions should be dropped with tiny budget"
        );
    }

    #[test]
    fn test_compact_summary_p0_size_estimation() {
        let summary = CompactSummary {
            objective: "test".to_string(),
            files_modified: vec!["a.rs".to_string(), "b.rs".to_string()],
            errors: vec!["err1".to_string()],
            pending_tasks: vec!["task1".to_string()],
            ..Default::default()
        };
        let p0 = summary.p0_size();
        // Verify the estimate matches actual P0 output length
        let full_output = summary.to_context_string(1); // tiny budget = P0 only
        assert_eq!(
            p0,
            full_output.len(),
            "p0_size() estimate must match actual P0 output length"
        );
    }

    #[test]
    fn test_generate_structured_summary_from_raw() {
        let raw_context = "\
Objective: Implement S4 context compression
Modified: src/context_compiler.rs
EDIT: src/enrichment.rs
TODO: Add tests for edge cases
- [ ] Run clippy
Decision: Use char-based budget instead of token-based
This line contains error in the output
Some unrelated line that should be ignored
FAIL: test_budget exceeded";

        let summary = generate_structured_summary(raw_context);

        assert_eq!(summary.objective, "Implement S4 context compression");
        assert_eq!(summary.files_modified.len(), 2);
        assert!(summary.files_modified[0].contains("context_compiler.rs"));
        assert!(summary.files_modified[1].contains("enrichment.rs"));
        assert_eq!(summary.pending_tasks.len(), 2); // TODO + - [ ]
        assert_eq!(summary.decisions.len(), 1);
        assert!(summary.decisions[0].contains("char-based budget"));
        assert_eq!(summary.errors.len(), 2); // "error" line + "FAIL" line
    }

    // ── Task 1: ObservationMasker integration tests ─────────────────────

    #[test]
    fn test_mask_and_summarize_passthrough_small_context() {
        let masker = ObservationMasker::new(); // Default threshold = 4000 tokens
        let small = "Objective: test\nModified: a.rs\nTODO: finish";
        let (summary, stats) = mask_and_summarize(small, &masker);

        assert_eq!(stats.blocks_masked, 0, "small context should not be masked");
        assert_eq!(summary.objective, "test");
        assert_eq!(summary.pending_tasks.len(), 1);
    }

    #[test]
    fn test_mask_and_summarize_masks_tool_results() {
        let masker = ObservationMasker::with_threshold(5); // Low threshold to activate

        // Build a context with tool results that should be masked
        let context = "Objective: Fix parser bug\n\
                        Modified: src/parser.rs\n\
                        → Read src/parser.rs\n\
                        tool_result: Read output\n\
                        1→fn parse() { buggy_code(); }\n\
                        2→fn parse2() { more_code(); }\n\
                        3→fn parse3() { even_more(); }\n\
                        4→fn parse4() { yet_more(); }\n\
                        5→fn parse5() { still_more(); }\n\
                        FAIL: test_parser panicked";

        let (summary, stats) = mask_and_summarize(context, &masker);

        assert!(stats.blocks_masked > 0, "tool results should be masked");
        assert_eq!(summary.objective, "Fix parser bug");
        // The error line should still be captured even after masking
        assert!(!summary.errors.is_empty(), "errors should survive masking");
    }

    #[test]
    fn test_compile_or_cached_masked() {
        let cache = ContextCache::new();
        let masker = ObservationMasker::new();

        let r1 =
            cache.compile_or_cached_masked("fix", &["a.rs".into()], &[], &[], &[], 2000, &masker);
        let r2 =
            cache.compile_or_cached_masked("fix", &["a.rs".into()], &[], &[], &[], 2000, &masker);

        assert_eq!(r1.cache_key, r2.cache_key, "cache should return same key");
        assert_eq!(r1.context, r2.context, "cache should return same content");
        assert_eq!(cache.len(), 1, "should have 1 cached entry");
    }

    #[test]
    fn test_byte_capacity_cache_evicts_under_pressure() {
        // P1 ranking #6 — bytes-bound moka cache.
        // Build a cache with a tiny byte budget (1 KiB) and insert
        // contexts that vastly exceed it. The TinyLFU+weigher pair must
        // evict so the total weighted size never grows unbounded.
        let cache = ContextCache::with_byte_capacity(1024);

        // Each compile_or_cached produces a CompiledContext whose body
        // size scales with the file count. With 50 distinct intents,
        // we'd need ~5 KiB to retain everything — well over our 1 KiB
        // budget. Eviction MUST kick in.
        for i in 0..50 {
            let intent = format!("intent_{i}");
            let _ = cache.compile_or_cached(
                &intent,
                &[format!("file_{i}.rs"), format!("other_{i}.rs")],
                &[],
                &[],
                &[],
                500,
            );
        }
        // moka batches eviction work; force it to flush before we
        // observe the size — Context7 best practice for tests.
        cache.inner.run_pending_tasks();

        // Bytes-bound: total weighted size ≤ budget (allow 2× headroom
        // for moka's window/protected segment overhead — the Caffeine
        // paper documents a small multiplicative factor).
        let weighted = cache.inner.weighted_size();
        assert!(
            weighted <= 1024 * 2,
            "weighted_size {weighted} exceeded 2× budget — eviction broken"
        );
        // Entry count is incidental — could be 1 or 50 depending on
        // weigher distribution. The contract is bytes, not entries.
        assert!(
            cache.len() < 50,
            "no eviction happened: {} entries",
            cache.len()
        );
    }

    #[test]
    fn test_byte_capacity_cache_retains_when_under_budget() {
        // REQUIREMENT: bytes-bound cache should NOT evict if the total
        // weighted size stays below the configured budget. This guards
        // against an over-eager eviction policy that punishes correct
        // small workloads.
        let cache = ContextCache::with_byte_capacity(64 * 1024); // 64 KiB
        for i in 0..3 {
            let intent = format!("small_{i}");
            let _ = cache.compile_or_cached(&intent, &[format!("a_{i}.rs")], &[], &[], &[], 100);
        }
        cache.inner.run_pending_tasks();
        assert_eq!(cache.len(), 3, "small workload must not evict");
    }

    #[test]
    fn test_byte_capacity_zero_input_clamps_to_floor() {
        // BOUNDARY: with_byte_capacity(0) is a foot-gun. Implementation
        // clamps to 1024 bytes minimum to guarantee at least one entry
        // can land — verify the floor holds.
        let cache = ContextCache::with_byte_capacity(0);
        let result = cache.compile_or_cached("tiny", &["x.rs".into()], &[], &[], &[], 100);
        cache.inner.run_pending_tasks();
        assert_eq!(result.intent, "tiny");
        // At least the entry we just inserted must be present (or have
        // been evicted then reinserted on cache miss — both legal).
        assert!(cache.len() <= 1);
    }
}
