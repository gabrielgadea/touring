//! H104: IncrementalIndexing — Delta symbol indexing via touring_ast IncrementalPipeline.
//!
//! Builds on touring_ast's IncrementalPipeline to provide O(log N) incremental
//! re-parsing after edits (vs O(n) full file re-parse). On PostToolUse for
//! Edit events, this handler extracts the changed file's symbols and injects
//! a delta context line reporting new/removed/modified symbols.
//!
//! Architecture:
//! ```text
//! PostToolUse (Edit):
//!   [Step 1] Extract file_path from tool_input
//!   [Step 2] Read current file content
//!   [Step 3] Try process_edit() on shared pipeline (incremental if cached)
//!   [Step 3b] Fallback: process_file() (full parse, caches document)
//!   [Step 4] Format: "idx: +N -M ~K [incremental|full] (file.rs)"
//!   [Step 5] INJECT context_line into ctx
//! ```
//!
//! The shared pipeline maintains an in-memory cache of tree-sitter parse
//! trees per file. After a full parse (process_file), subsequent edits to
//! the same file benefit from O(log N) tree-sitter incremental re-parsing.

use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use moka::sync::Cache;

use touring_code::ast::graph::SymbolLocation;
use touring_code::ast::incremental_pipeline::{IncrementalEditResult, IncrementalPipeline};
use touring_code::ast::languages::Lang;
use touring_code::ast::symbols::{Symbol, extract_symbols};

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

/// Process-global incremental pipeline — shared across all handler invocations.
/// Initialized lazily on first PostToolUse event.
/// After `process_file` caches a document, `process_edit` becomes incremental.
static INCREMENTAL_PIPELINE: OnceLock<Arc<std::sync::Mutex<IncrementalPipeline>>> = OnceLock::new();

/// Cache for file contents to support incremental parsing.
///
/// Migrated 2026-04-16 from `Arc<RwLock<HashMap<String, String>>>` to
/// `moka::sync::Cache<String, Arc<String>>` (Moka Expansion Wave 2, 9th
/// migration). The previous implementation was unbounded — long sessions
/// could accumulate gigabytes of cached source across hundreds of edited
/// files. moka adds:
///
/// - **Byte-aware weigher** capped at 64 MiB (covers ~650 typical 100 KiB
///   files or ~13 at the 5 MiB cap).
/// - **TTL 5min + TTI 2min** — cold entries evicted automatically.
/// - **W-TinyLFU admission** — hot files survive pressure.
/// - **Lock-free reads** — no `RwLock` acquisition on the hot path.
/// - **Arc<String>** values — hits are O(1) refcount bumps instead of
///   full string clones (the old impl cloned every read).
static FILE_CONTENT_CACHE: OnceLock<Cache<String, Arc<String>>> = OnceLock::new();

/// Minimum context budget before running incremental indexing.
const MIN_BUDGET: usize = 60;

/// Maximum file size for incremental indexing (5MB).
const MAX_FILE_SIZE_BYTES: u64 = 5 * 1024 * 1024;

/// Maximum bytes retained in the file-content cache (64 MiB). Tuned for the
/// 5 MiB per-file cap × ~13 hot files, or ~650 typical 100 KiB files.
const FILE_CACHE_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// TTL for cached file content. Short enough that external edits (e.g. by
/// other tools) are not masked indefinitely.
const FILE_CACHE_TTL_SECS: u64 = 5 * 60;

/// Idle window — files not read within this window are evicted even before
/// TTL, keeping memory pressure predictable during long sessions.
const FILE_CACHE_TTI_SECS: u64 = 2 * 60;

/// Returns the shared incremental pipeline, initializing if needed.
fn shared_pipeline() -> &'static Arc<std::sync::Mutex<IncrementalPipeline>> {
    INCREMENTAL_PIPELINE.get_or_init(|| Arc::new(std::sync::Mutex::new(IncrementalPipeline::new())))
}

/// Returns the shared file content cache.
fn shared_content_cache() -> &'static Cache<String, Arc<String>> {
    FILE_CONTENT_CACHE.get_or_init(|| {
        Cache::builder()
            .max_capacity(FILE_CACHE_MAX_BYTES)
            .time_to_live(Duration::from_secs(FILE_CACHE_TTL_SECS))
            .time_to_idle(Duration::from_secs(FILE_CACHE_TTI_SECS))
            .weigher(|_k: &String, v: &Arc<String>| -> u32 {
                // UTF-8 byte length ≈ payload cost (saturate so gigantic
                // files do not overflow the u32 weight tally).
                v.len().min(u32::MAX as usize) as u32
            })
            .eviction_policy(moka::policy::EvictionPolicy::lru())
            .build()
    })
}

/// H104: Incremental indexing handler — O(log N) delta symbol extraction.
#[derive(Default)]
pub struct IncrementalIndexingHandler;

impl IncrementalIndexingHandler {
    /// Creates a new incremental-indexing handler.
    pub fn new() -> Self {
        Self
    }

    /// Extract the file path from tool input.
    fn extract_file_path<'a>(&self, tool_input: &'a serde_json::Value) -> Option<&'a str> {
        tool_input
            .pointer("/file_path")
            .and_then(|v| v.as_str())
            .or_else(|| tool_input.pointer("/path").and_then(|v| v.as_str()))
    }

    /// Extract edit metadata from tool input.
    fn extract_edit_meta(&self, tool_input: &serde_json::Value) -> Option<(usize, usize, String)> {
        let start_byte = tool_input
            .pointer("/start_byte")
            .and_then(|v| v.as_u64().map(|n| n as usize))?;
        let old_end_byte = tool_input
            .pointer("/old_end_byte")
            .and_then(|v| v.as_u64().map(|n| n as usize))?;
        let new_text = tool_input
            .pointer("/new_text")
            .or_else(|| tool_input.pointer("/content"))
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        Some((start_byte, old_end_byte, new_text))
    }

    /// Read file content, using the in-process moka cache when available.
    ///
    /// Cache hits return an `Arc<String>` clone (O(1) refcount bump); the
    /// caller receives an owned `String` only on miss paths where disk
    /// I/O dominated anyway. Preserves the historical return type.
    fn get_file_content(&self, path: &str) -> Option<String> {
        let cache = shared_content_cache();
        if let Some(hit) = cache.get(path) {
            return Some((*hit).clone());
        }
        // Miss — read from disk and populate cache via Arc<String>
        // (the Arc means subsequent hits do not re-clone the buffer).
        let content = std::fs::read_to_string(path).ok()?;
        let arc = Arc::new(content);
        cache.insert(path.to_string(), Arc::clone(&arc));
        Some((*arc).clone())
    }

    /// Update file content in cache after an edit.
    ///
    /// The new content is wrapped in `Arc<String>` so readers immediately
    /// benefit from zero-copy access. `moka::sync::Cache::insert` is
    /// lock-free (internally sharded) so this no longer blocks the hot path.
    fn update_content_cache(
        &self,
        path: &str,
        old_content: &str,
        start: usize,
        old_end: usize,
        new_text: &str,
    ) {
        let cache = shared_content_cache();
        let start_idx = start.min(old_content.len());
        let end_idx = old_end.min(old_content.len());
        let new_content = format!(
            "{}{}{}",
            &old_content[..start_idx],
            new_text,
            &old_content[end_idx..]
        );
        cache.insert(path.to_string(), Arc::new(new_content));
    }

    /// Process an incremental edit on the shared pipeline.
    fn try_incremental_edit(
        &self,
        path: &str,
        start: usize,
        old_end: usize,
        new_text: &str,
    ) -> Option<IncrementalEditResult> {
        let pipeline = shared_pipeline();
        let mut guard = pipeline.lock().ok()?;
        guard.process_edit(path, start, old_end, new_text).ok()
    }

    /// Process a full file parse (caches the document for future incremental edits).
    fn process_full(&self, path: &str, content: &str) -> IncrementalEditResult {
        let pipeline = shared_pipeline();
        let mut guard = match pipeline.lock() {
            Ok(g) => g,
            Err(_) => {
                // Poisoned lock — fall back to pure AST extraction
                return self.pure_ast_extract(path, content);
            }
        };
        match guard.process_file(path, content) {
            Ok(result) => result,
            Err(_) => self.pure_ast_extract(path, content),
        }
    }

    /// Pure AST extraction when the pipeline is unavailable.
    /// Does NOT cache the document (used as fallback only).
    fn pure_ast_extract(&self, path: &str, content: &str) -> IncrementalEditResult {
        let t0 = Instant::now();
        let lang = Lang::from_path(std::path::Path::new(path));
        let symbols: Vec<Symbol> = match lang {
            Some(l) => extract_symbols(content, l).unwrap_or_default(),
            None => Vec::new(),
        };
        // Convert Symbol -> SymbolLocation for IncrementalEditResult compatibility.
        // Preserve the rich `kind` classification (the Symbol already carries it)
        // so the persisted index keeps `kind` even on this fallback path.
        let symbols_added: Vec<SymbolLocation> = symbols
            .into_iter()
            .map(|s| {
                let kind = s.kind.as_str().to_string();
                SymbolLocation::new(path, s.name, s.line, s.column, s.is_public)
                    .with_kind(Some(kind))
            })
            .collect();
        IncrementalEditResult {
            changed_ranges: vec![(0, content.len())],
            symbols_added,
            symbols_removed: Vec::new(),
            symbols_modified: Vec::new(),
            parse_time_us: t0.elapsed().as_micros() as u64,
            was_incremental: false,
        }
    }

    /// Build a human-readable context line from an incremental edit result.
    fn format_delta(&self, path: &str, result: &IncrementalEditResult) -> String {
        let added = result.symbols_added.len();
        let removed = result.symbols_removed.len();
        let modified = result.symbols_modified.len();

        let file_name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);

        let parse_mode = if result.was_incremental {
            format!("incremental({}µs)", result.parse_time_us)
        } else {
            format!("full({}µs)", result.parse_time_us)
        };

        let parts: Vec<String> = std::iter::empty()
            .chain(if added > 0 {
                Some(format!("+{}", added))
            } else {
                None
            })
            .chain(if removed > 0 {
                Some(format!("-{}", removed))
            } else {
                None
            })
            .chain(if modified > 0 {
                Some(format!("~{}", modified))
            } else {
                None
            })
            .collect();

        match added == 0 && removed == 0 && modified == 0 {
            true if result.changed_ranges.is_empty() => {
                format!("idx: {} [{}] — no symbol changes", file_name, parse_mode)
            }
            true if !result.was_incremental => {
                // Full parse with no symbol changes — suppress range count
                format!("idx: {} [{}] — no symbol changes", file_name, parse_mode)
            }
            true => {
                // Incremental with no symbol changes but file was modified — show range count
                format!(
                    "idx: {} [{}] —  ({} ranges)",
                    file_name,
                    parse_mode,
                    result.changed_ranges.len()
                )
            }
            false => {
                format!(
                    "idx: {} [{}] — {} ({} ranges)",
                    file_name,
                    parse_mode,
                    parts.join(" "),
                    result.changed_ranges.len()
                )
            }
        }
    }
}

impl Handler for IncrementalIndexingHandler {
    fn name(&self) -> &str {
        "H104_incremental_indexing"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Edit")
    }

    fn priority(&self) -> u8 {
        60
    }

    fn dependency_tier(&self) -> u8 {
        1
    }

    fn timeout_ms(&self) -> u64 {
        15
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        if ctx.context_budget_remaining < MIN_BUDGET {
            return HandlerResult::skip(self.name());
        }

        // Only run on Edit tool
        let tool_name = ctx.tool_name.as_deref().unwrap_or("");
        if tool_name != "Edit" {
            return HandlerResult::skip(self.name());
        }

        let file_path = match self.extract_file_path(&ctx.tool_input) {
            Some(p) => p,
            None => return HandlerResult::skip(self.name()),
        };

        // Check file size
        if let Ok(metadata) = std::fs::metadata(file_path) {
            if metadata.len() > MAX_FILE_SIZE_BYTES {
                return HandlerResult::skip(self.name());
            }
        }

        // Get current content from cache or disk
        let current_content = match self.get_file_content(file_path) {
            Some(c) => c,
            None => return HandlerResult::skip(self.name()),
        };

        // Extract edit metadata
        let (start_byte, old_end_byte, new_text) = self
            .extract_edit_meta(&ctx.tool_input)
            .unwrap_or((0, 0, String::new()));

        // Check if edit has actual changes
        if start_byte == 0 && old_end_byte == 0 && new_text.is_empty() {
            // No edit metadata — just re-index the file
            let result = self.process_full(file_path, &current_content);
            let context_line = self.format_delta(file_path, &result);
            ctx.context_lines.push(context_line);
            return HandlerResult::allow(self.name(), None);
        }

        // Try incremental edit first
        let result = if let Some(r) =
            self.try_incremental_edit(file_path, start_byte, old_end_byte, &new_text)
        {
            // Update content cache
            self.update_content_cache(
                file_path,
                &current_content,
                start_byte,
                old_end_byte,
                &new_text,
            );
            r
        } else {
            // No cached document — do full parse (caches for next time)
            // First update content cache
            self.update_content_cache(
                file_path,
                &current_content,
                start_byte,
                old_end_byte,
                &new_text,
            );
            self.process_full(file_path, &current_content)
        };

        let context_line = self.format_delta(file_path, &result);
        ctx.context_lines.push(context_line);
        HandlerResult::allow(self.name(), None)
    }
}

/// Register H104 in the pipeline.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(IncrementalIndexingHandler::new()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // Serialize tests that operate on the shared content cache singleton.
    // Without this, concurrent tests race on `invalidate_all` + count assertions.
    static CACHE_LOCK: Mutex<()> = Mutex::new(());

    fn cache_guard() -> MutexGuard<'static, ()> {
        CACHE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn test_handler_name() {
        let handler = IncrementalIndexingHandler::new();
        assert_eq!(handler.name(), "H104_incremental_indexing");
    }

    #[test]
    fn test_events() {
        let handler = IncrementalIndexingHandler::new();
        assert_eq!(handler.events(), &[HookEvent::PostToolUse]);
    }

    #[test]
    fn test_tool_matcher() {
        let handler = IncrementalIndexingHandler::new();
        assert_eq!(handler.tool_matcher(), Some("Edit"));
    }

    #[test]
    fn test_priority() {
        let handler = IncrementalIndexingHandler::new();
        assert_eq!(handler.priority(), 60);
    }

    #[test]
    fn test_extract_file_path_from_file_path_key() {
        let handler = IncrementalIndexingHandler::new();
        let input = serde_json::json!({ "file_path": "/src/main.rs" });
        assert_eq!(handler.extract_file_path(&input), Some("/src/main.rs"));
    }

    #[test]
    fn test_extract_file_path_from_path_key() {
        let handler = IncrementalIndexingHandler::new();
        let input = serde_json::json!({ "path": "/src/main.rs" });
        assert_eq!(handler.extract_file_path(&input), Some("/src/main.rs"));
    }

    #[test]
    fn test_extract_file_path_missing() {
        let handler = IncrementalIndexingHandler::new();
        let input = serde_json::json!({ "content": "fn main() {}" });
        assert_eq!(handler.extract_file_path(&input), None);
    }

    #[test]
    fn test_extract_edit_meta_full() {
        let handler = IncrementalIndexingHandler::new();
        let input = serde_json::json!({
            "start_byte": 10,
            "old_end_byte": 20,
            "new_text": "hello world"
        });
        let result = handler.extract_edit_meta(&input);
        assert!(result.is_some());
        let (start, end, text) = result.unwrap();
        assert_eq!(start, 10);
        assert_eq!(end, 20);
        assert_eq!(text, "hello world");
    }

    #[test]
    fn test_extract_edit_meta_content_fallback() {
        let handler = IncrementalIndexingHandler::new();
        let input = serde_json::json!({
            "start_byte": 0,
            "old_end_byte": 0,
            "content": "full file content"
        });
        let result = handler.extract_edit_meta(&input);
        assert!(result.is_some());
        let (_, _, text) = result.unwrap();
        assert_eq!(text, "full file content");
    }

    #[test]
    fn test_extract_edit_meta_missing() {
        let handler = IncrementalIndexingHandler::new();
        let input = serde_json::json!({ "start_byte": 10 });
        assert!(handler.extract_edit_meta(&input).is_none());
    }

    #[test]
    fn test_format_delta_no_changes_incremental() {
        let handler = IncrementalIndexingHandler::new();
        let result = IncrementalEditResult {
            changed_ranges: vec![],
            symbols_added: vec![],
            symbols_removed: vec![],
            symbols_modified: vec![],
            parse_time_us: 42,
            was_incremental: true,
        };
        let msg = handler.format_delta("/src/main.rs", &result);
        assert!(msg.contains("no symbol changes"));
        assert!(msg.contains("incremental"));
        assert!(msg.contains("main.rs"));
    }

    #[test]
    fn test_format_delta_no_changes_full() {
        let handler = IncrementalIndexingHandler::new();
        // Empty changed_ranges + no symbol changes → "no symbol changes" branch
        let result = IncrementalEditResult {
            changed_ranges: vec![],
            symbols_added: vec![],
            symbols_removed: vec![],
            symbols_modified: vec![],
            parse_time_us: 1200,
            was_incremental: false,
        };
        let msg = handler.format_delta("/src/main.rs", &result);
        assert!(msg.contains("no symbol changes"), "got: {msg}");
        assert!(msg.contains("full"), "got: {msg}");
    }

    #[test]
    fn test_format_delta_with_changes() {
        let handler = IncrementalIndexingHandler::new();
        let result = IncrementalEditResult {
            changed_ranges: vec![(5, 15), (30, 35)],
            symbols_added: vec![],
            symbols_removed: vec![],
            symbols_modified: vec![],
            parse_time_us: 15,
            was_incremental: true,
        };
        let msg = handler.format_delta("/src/lib.rs", &result);
        assert!(msg.contains("lib.rs"));
        assert!(msg.contains("incremental"));
        assert!(msg.contains("2 ranges"));
    }

    #[test]
    fn test_shared_pipeline_singleton() {
        let p1 = shared_pipeline();
        let p2 = shared_pipeline();
        assert!(Arc::ptr_eq(p1, p2));
    }

    #[test]
    fn test_shared_content_cache_singleton() {
        // moka::sync::Cache is itself reference-counted (the singleton is
        // a `&'static Cache`). Two calls must return the same pointer so
        // writers and readers agree on the same cache.
        let c1 = shared_content_cache();
        let c2 = shared_content_cache();
        assert!(std::ptr::eq(c1, c2));
    }

    #[test]
    fn test_update_content_cache_inserts() {
        let _guard = cache_guard();
        let cache = shared_content_cache();
        cache.invalidate_all();
        cache.run_pending_tasks();

        let handler = IncrementalIndexingHandler::new();
        // start=4 (beginning of "content"), old_end=11 (after "content")
        // Replaces "content" with "NEW": "old " + "NEW" + " here" = "old NEW here"
        handler.update_content_cache("/test.rs", "old content here", 4, 11, "NEW");
        let cached = cache.get("/test.rs").map(|arc| (*arc).clone());
        assert_eq!(cached, Some("old NEW here".to_string()));
    }

    // ── moka-specific regression tests (Wave 2 · 9th migration, 2026-04-16) ──

    #[test]
    fn test_cache_hit_returns_arc_clone_not_deep_copy() {
        let _guard = cache_guard();
        // Proves the migration's zero-copy advantage: two hits return the
        // *same* Arc allocation rather than cloning the full String.
        let cache = shared_content_cache();
        cache.invalidate_all();
        cache.run_pending_tasks();
        cache.insert("/arcproof.rs".into(), Arc::new("fn main(){}".to_string()));
        let a = cache.get("/arcproof.rs").expect("hit a");
        let b = cache.get("/arcproof.rs").expect("hit b");
        assert!(
            Arc::ptr_eq(&a, &b),
            "moka must return clones of the same Arc — zero-copy hits"
        );
    }

    #[test]
    fn test_weigher_tracks_byte_size() {
        let _guard = cache_guard();
        let cache = shared_content_cache();
        cache.invalidate_all();
        cache.run_pending_tasks();
        cache.insert("/w1.rs".into(), Arc::new("aaaa".to_string())); // 4 bytes
        cache.insert("/w2.rs".into(), Arc::new("bbbbbbbb".to_string())); // 8 bytes
        cache.run_pending_tasks();
        assert_eq!(cache.entry_count(), 2);
        assert_eq!(cache.weighted_size(), 4 + 8, "byte-aware weigher");
    }

    #[test]
    fn test_invalidate_all_drops_every_entry() {
        let _guard = cache_guard();
        let cache = shared_content_cache();
        cache.invalidate_all();
        cache.run_pending_tasks();
        for i in 0..5 {
            cache.insert(format!("/i{i}.rs"), Arc::new(format!("content{i}")));
        }
        cache.run_pending_tasks();
        assert_eq!(cache.entry_count(), 5);
        cache.invalidate_all();
        cache.run_pending_tasks();
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn test_update_content_cache_overwrites_prior_entry() {
        let _guard = cache_guard();
        let cache = shared_content_cache();
        cache.invalidate_all();
        cache.run_pending_tasks();
        let handler = IncrementalIndexingHandler::new();

        // First insert via update_content_cache
        handler.update_content_cache("/multi.rs", "hello world", 0, 5, "HI");
        // Second update overwrites
        handler.update_content_cache("/multi.rs", "HI world", 3, 8, "Earth");
        cache.run_pending_tasks();

        assert_eq!(cache.entry_count(), 1, "same key — one entry");
        let cached = cache.get("/multi.rs").map(|a| (*a).clone());
        assert_eq!(cached, Some("HI Earth".to_string()));
    }
}
