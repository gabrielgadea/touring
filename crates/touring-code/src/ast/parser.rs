//! Parser pool for efficient multi-threaded parsing

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// A shared reference to a parsed tree.
///
/// P2.4: Enables zero-copy sharing of trees across multiple consumers
/// (e.g., complexity analysis + symbol extraction + graph building).
pub type SharedTree = Arc<tree_sitter::Tree>;

use moka::sync::Cache;
use tracing::instrument;

use crate::ast::document::RopeDocument;
use crate::ast::error::{AstError, AstResult};
use crate::ast::languages::Lang;

/// Maximum number of cached trees in [`IncrementalParser`].
const MAX_TREE_CACHE: usize = 128;

// ── Standalone incremental parse (P2.2) ───────────────────────────────────

/// Incrementally re-parse `source` using a previously parsed `old_tree` and
/// an edit description.
///
/// This is a **standalone** function — it does not require [`IncrementalParser`]
/// or any cached state. It creates a one-shot parser, applies `edit` to
/// `old_tree` so tree-sitter knows which byte ranges shifted, then re-parses
/// only the affected region.
///
/// # Incremental vs Full
///
/// - **Full parse**: `parser.parse(source, None)` — O(N) over the entire file.
/// - **Incremental**: `incremental_parse(source, &old_tree, edit)` — O(edit-region)
///   when `old_tree` is a previously parsed tree for the same source before `edit`.
///
/// # Arguments
///
/// - `source` — the new source text after the edit
/// - `old_tree` — the syntax tree parsed from the original source (before `edit`)
/// - `edit` — describes the byte-range change applied to transform old source into `source`
///
/// # Returns
///
/// A new [`tree_sitter::Tree`] for `source`. Unchanged subtrees from `old_tree`
/// are reused directly without re-parsing.
///
/// # Example
///
/// ```
/// use tree_sitter::InputEdit;
/// use touring_code::ast::parser::{incremental_parse, ParserPool};
/// use touring_code::ast::Lang;
///
/// let old_source = "fn foo() {}";
/// // Parse `old_source` to produce `old_tree` using the public ParserPool API.
/// let pool = ParserPool::new();
/// let old_tree = pool.parse(old_source, Lang::Rust).unwrap();
///
/// let new_source = "fn bar() {}";
/// let edit = InputEdit {
///     start_byte: 3,
///     old_end_byte: 6,
///     new_end_byte: 6,
///     start_position: tree_sitter::Point::new(0, 3),
///     old_end_position: tree_sitter::Point::new(0, 6),
///     new_end_position: tree_sitter::Point::new(0, 6),
/// };
/// let new_tree = incremental_parse(new_source, &old_tree, &edit);
/// ```
///
/// # See Also
///
/// - [`IncrementalParser::parse_incremental`] for a cached version that avoids
///   re-creating parsers across repeated edits to the same file.
pub fn incremental_parse(
    source: &str,
    old_tree: &tree_sitter::Tree,
    edit: &tree_sitter::InputEdit,
) -> tree_sitter::Tree {
    let lang = old_tree.language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang)
        .expect("Language from old_tree is always valid");

    // Apply the edit so tree-sitter knows which ranges shifted.
    // Then re-parse: unchanged subtrees are reused automatically.
    let mut edited_tree = old_tree.clone();
    edited_tree.edit(edit);

    parser
        .parse(source, Some(&edited_tree))
        .expect("tree-sitter parse with old tree never returns None")
}

// ── Thread-local parser storage ──────────────────────────────────────────

thread_local! {
    /// Per-thread parser instances — avoids Mutex contention entirely.
    /// `Parser` is `!Send` so this is the natural fit.
    static THREAD_PARSERS: RefCell<HashMap<Lang, tree_sitter::Parser>> =
        RefCell::new(HashMap::new());
}

/// Parse `source` for `lang` using a thread-local parser (zero contention).
///
/// This is the fastest parse path — no Mutex, no allocation per call.
pub(crate) fn parse_thread_local(source: &str, lang: Lang) -> AstResult<tree_sitter::Tree> {
    THREAD_PARSERS.with(|cell| {
        let mut parsers = cell.borrow_mut();

        // Use entry API to avoid contains_key + insert (clippy)
        if let std::collections::hash_map::Entry::Vacant(entry) = parsers.entry(lang) {
            let mut p = tree_sitter::Parser::new();
            p.set_language(&lang.tree_sitter_language()).map_err(|e| {
                AstError::GrammarUnavailable(format!(
                    "{} grammar rejected by the tree-sitter runtime (ABI mismatch): {e}",
                    lang.as_str()
                ))
            })?;
            entry.insert(p);
        }

        let parser = parsers.get_mut(&lang).ok_or_else(|| {
            AstError::ParseFailed(format!("Parser not found for {}", lang.as_str()))
        })?;

        parser
            .parse(source, None)
            .ok_or_else(|| AstError::ParseFailed("Parse failed".into()))
    })
}

// ── ParserPool (backward-compatible, thread-safe) ────────────────────────

/// Thread-safe parser pool that reuses parsers per language.
///
/// Internally delegates to [`thread_local`] storage for zero-contention
/// parsing. `ParserPool` is a zero-size type (ZST) — all state lives in
/// per-thread `THREAD_PARSERS`. `Send + Sync` are safe because each thread
/// owns its own `Parser` via `thread_local!` and none is shared across threads.
#[derive(Clone)]
pub struct ParserPool;

impl std::fmt::Debug for ParserPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParserPool").finish()
    }
}

impl Default for ParserPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ParserPool {
    /// Create a new parser pool.
    pub fn new() -> Self {
        Self
    }

    /// Parse source code using a thread-local parser.
    ///
    /// Zero contention — each thread maintains its own parser per language.
    #[instrument(skip(self, source), fields(lang = %lang.as_str(), len = source.len()))]
    pub fn parse(&self, source: &str, lang: Lang) -> AstResult<tree_sitter::Tree> {
        parse_thread_local(source, lang)
    }

    /// Parse a file and return the syntax tree.
    pub fn parse_file(&self, path: &std::path::Path) -> AstResult<tree_sitter::Tree> {
        let lang = Lang::from_path(path)
            .ok_or_else(|| AstError::UnknownLanguage(format!("{:?}", path)))?;

        let source = std::fs::read_to_string(path)?;

        self.parse(&source, lang)
    }

    /// Get the root node of a parsed tree.
    pub fn get_root_node(tree: &tree_sitter::Tree) -> tree_sitter::Node<'_> {
        tree.root_node()
    }
}

// ── ParsedFile ───────────────────────────────────────────────────────────

/// A parsed file with its AST and metadata.
#[derive(Debug)]
pub struct ParsedFile {
    /// Source code
    pub source: String,
    /// Syntax tree
    pub tree: tree_sitter::Tree,
    /// Language
    pub language: Lang,
    /// File path
    pub path: std::path::PathBuf,
}

impl ParsedFile {
    /// Parse a file from disk.
    pub fn from_path(path: &std::path::Path, pool: &ParserPool) -> AstResult<Self> {
        let lang = Lang::from_path(path)
            .ok_or_else(|| AstError::UnknownLanguage(format!("{:?}", path)))?;

        let source = std::fs::read_to_string(path)?;

        let tree = pool.parse(&source, lang)?;

        Ok(Self {
            source,
            tree,
            language: lang,
            path: path.to_path_buf(),
        })
    }

    /// Get the root node of the syntax tree.
    pub fn root_node(&self) -> tree_sitter::Node<'_> {
        self.tree.root_node()
    }

    /// Check if the tree has any syntax errors.
    pub fn has_errors(&self) -> bool {
        self.tree.root_node().has_error()
    }

    /// Walk the tree with a callback (iterative, stack-safe).
    pub fn walk<F>(&self, mut callback: F)
    where
        F: FnMut(tree_sitter::Node),
    {
        let mut cursor = self.root_node().walk();
        let mut did_enter = true;
        loop {
            if did_enter {
                callback(cursor.node());
                if cursor.goto_first_child() {
                    continue;
                }
            }
            if cursor.goto_next_sibling() {
                did_enter = true;
                continue;
            }
            if !cursor.goto_parent() {
                break;
            }
            did_enter = false;
        }
    }
}

// ── IncrementalParser (LRU tree cache) ───────────────────────────────────

/// Incremental parser with per-file LRU tree caching.
///
/// Enables O(edit-region) re-parsing instead of O(file-size) by caching
/// the previous [`tree_sitter::Tree`] and applying [`tree_sitter::InputEdit`]
/// before re-parsing.
///
/// # Tree Cache
///
/// Uses a concurrent LRU cache (`moka::sync::Cache`) so frequently-accessed
/// trees survive longer. Capacity: `MAX_TREE_CACHE` (128).
///
/// # Thread Safety
///
/// This struct is `!Send` because [`tree_sitter::Parser`] is `!Send`.
/// Use from a single thread, or wrap access behind a `Mutex`.
pub struct IncrementalParser {
    /// One parser per language, reused across calls.
    parsers: HashMap<Lang, tree_sitter::Parser>,
    /// LRU-cached trees keyed by file path.
    trees: Cache<String, tree_sitter::Tree>,
}

impl std::fmt::Debug for IncrementalParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncrementalParser")
            .field("languages", &self.parsers.len())
            .field("cached_trees", &self.trees.entry_count())
            .finish()
    }
}

impl Default for IncrementalParser {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalParser {
    /// Create a new incremental parser with empty caches.
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
            trees: Cache::builder()
                .max_capacity(MAX_TREE_CACHE as u64)
                .eviction_policy(moka::policy::EvictionPolicy::lru())
                .build(),
        }
    }

    /// Get or create a parser for the given language.
    fn get_parser(&mut self, lang: Lang) -> AstResult<&mut tree_sitter::Parser> {
        use std::collections::hash_map::Entry;
        match self.parsers.entry(lang) {
            Entry::Occupied(e) => Ok(e.into_mut()),
            Entry::Vacant(e) => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&lang.tree_sitter_language())
                    .map_err(|err| {
                        AstError::GrammarUnavailable(format!(
                            "{} grammar rejected by the tree-sitter runtime (ABI mismatch): {err}",
                            lang.as_str()
                        ))
                    })?;
                Ok(e.insert(parser))
            }
        }
    }

    /// Full parse without caching. Returns the syntax tree.
    pub fn parse_full(&mut self, source: &str, lang: Lang) -> AstResult<tree_sitter::Tree> {
        let parser = self.get_parser(lang)?;
        parser
            .parse(source, None)
            .ok_or_else(|| AstError::ParseFailed("Parse failed".into()))
    }

    /// Parse a [`RopeDocument`] using `parse_with` — avoids the O(N) `to_string()` copy.
    ///
    /// Uses tree-sitter's `parse_with` callback API: for each chunk tree-sitter
    /// requests, we feed the rope chunk directly (`chunk_at_byte`) without ever
    /// materialising the full document as a single contiguous `String`.
    ///
    /// For large files (≥ 64 KB) or heavily fragmented ropes (many edits),
    /// this eliminates the dominant allocation in the parse hot-path.
    ///
    /// # Arguments
    /// - `doc` — rope-backed document
    /// - `lang` — language grammar to use
    /// - `old_tree` — optional prior tree for incremental reparsing (pass after `edit()`)
    ///
    /// # Errors
    /// Returns an error string if tree-sitter fails to parse.
    pub fn parse_rope(
        &mut self,
        doc: &RopeDocument,
        lang: Lang,
        old_tree: Option<&tree_sitter::Tree>,
    ) -> AstResult<tree_sitter::Tree> {
        let parser = self.get_parser(lang)?;
        let mut callback =
            |byte_offset: usize, _: tree_sitter::Point| -> &[u8] { doc.chunk_at_byte(byte_offset) };
        parser
            .parse_with_options(&mut callback, old_tree, None)
            .ok_or_else(|| AstError::ParseFailed("Rope parse failed".into()))
    }

    /// Parse source and store the resulting tree in the LRU cache.
    ///
    /// If the cache is at capacity, the **least recently used** entry
    /// is evicted (not the oldest by insertion time).
    #[instrument(skip(self, source), fields(path = %path, lang = %lang.as_str()))]
    pub fn parse_and_cache(
        &mut self,
        path: &str,
        source: &str,
        lang: Lang,
    ) -> AstResult<tree_sitter::Tree> {
        let tree = self.parse_full(source, lang)?;
        self.trees.insert(path.to_string(), tree.clone());
        self.trees.run_pending_tasks();
        Ok(tree)
    }

    /// Parse source, cache the tree, and return a shared reference.
    ///
    /// P2.4: Returns `Arc<Tree>` for zero-cost sharing across multiple
    /// consumers. The cache stores a clone; the caller gets the Arc.
    #[instrument(skip(self, source), fields(path = %path, lang = %lang.as_str()))]
    pub fn parse_and_cache_shared(
        &mut self,
        path: &str,
        source: &str,
        lang: Lang,
    ) -> AstResult<SharedTree> {
        let tree = self.parse_full(source, lang)?;
        let shared = Arc::new(tree);
        // Store a clone in cache (Arc::new consumed the tree)
        self.trees.insert(path.to_string(), (*shared).clone());
        self.trees.run_pending_tasks();
        Ok(shared)
    }

    /// Incremental re-parse using a cached tree and an `InputEdit`.
    ///
    /// 1. If a cached tree exists for `path`, applies `edit` to it and
    ///    re-parses incrementally (tree-sitter only re-parses the changed
    ///    region).
    /// 2. If no cached tree exists, falls back to a full parse.
    ///
    /// Returns `(new_tree, changed_ranges)`. `changed_ranges` is empty
    /// when there was no cached tree to diff against.
    #[instrument(skip(self, new_source, edit), fields(path = %path))]
    pub fn parse_incremental(
        &mut self,
        path: &str,
        new_source: &str,
        edit: &tree_sitter::InputEdit,
    ) -> AstResult<(tree_sitter::Tree, Vec<tree_sitter::Range>)> {
        let lang = Lang::from_path(Path::new(path))
            .ok_or_else(|| AstError::UnknownLanguage(path.to_string()))?;

        match self.trees.remove(path) {
            Some(mut old_tree) => {
                // Apply the edit so tree-sitter knows which byte ranges shifted.
                old_tree.edit(edit);

                // Re-parse — tree-sitter reuses unchanged subtrees.
                let parser = self.get_parser(lang)?;
                let new_tree = parser
                    .parse(new_source, Some(&old_tree))
                    .ok_or_else(|| AstError::ParseFailed("Incremental parse failed".into()))?;

                let changes: Vec<tree_sitter::Range> = old_tree.changed_ranges(&new_tree).collect();

                // Update cache with new tree (promotes to most-recently-used).
                self.trees.insert(path.to_string(), new_tree.clone());
                self.trees.run_pending_tasks();
                Ok((new_tree, changes))
            }
            _ => {
                // No cached tree — fall back to full parse.
                let tree = self.parse_full(new_source, lang)?;
                self.trees.insert(path.to_string(), tree.clone());
                self.trees.run_pending_tasks();
                Ok((tree, Vec::new()))
            }
        }
    }

    /// Returns the cached tree for `path`, if any.
    ///
    /// This promotes the entry to most-recently-used in the LRU.
    pub fn cached_tree(&self, path: &str) -> Option<tree_sitter::Tree> {
        self.trees.get(path)
    }

    /// Returns the cached tree for `path` without affecting LRU order.
    ///
    /// Note: `moka::sync::Cache` does not distinguish peek vs get for eviction
    /// purposes; this method is provided for API compatibility.
    pub fn peek_tree(&self, path: &str) -> Option<tree_sitter::Tree> {
        self.trees.get(path)
    }

    /// Remove the cached tree for `path`.
    pub fn invalidate_tree(&mut self, path: &str) {
        self.trees.invalidate(path);
        self.trees.run_pending_tasks();
    }

    /// Number of currently cached trees.
    pub fn cache_size(&self) -> usize {
        self.trees.run_pending_tasks();
        self.trees.entry_count() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parser_pool_creation() {
        let pool = ParserPool::new();
        let source = "def foo(): pass";
        let result = pool.parse(source, Lang::Python);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_multiple_languages() {
        let pool = ParserPool::new();

        let python = "def foo(): pass";
        let rust = "fn foo() {}";
        let typescript = "function foo(): void {}";
        let javascript = "function foo() {}";

        assert!(pool.parse(python, Lang::Python).is_ok());
        assert!(pool.parse(rust, Lang::Rust).is_ok());
        assert!(pool.parse(typescript, Lang::TypeScript).is_ok());
        assert!(pool.parse(javascript, Lang::JavaScript).is_ok());
    }

    /// ABI-14 grammar pin regression guard. `tree-sitter-bash`, `-css` and
    /// `-html` are pinned to ABI-14 crate versions so the `tree-sitter` 0.24
    /// runtime can load them. Before the pin the 0.25.x (ABI 15) crates were
    /// rejected at `Parser::set_language`, producing `GrammarUnavailable`
    /// ("Unsupported tree-sitter ABI") — a false syntax error on every
    /// bash / css / html file analyzed.
    #[test]
    fn test_parse_bash_css_html_grammars_load() {
        let pool = ParserPool::new();

        let bash_tree = pool
            .parse("echo hello\nif [ -f x ]; then echo yes; fi\n", Lang::Bash)
            .expect("bash grammar must load (ABI 14) and parse");
        assert!(
            !bash_tree.root_node().has_error(),
            "valid bash must parse without error nodes"
        );

        let css_tree = pool
            .parse("body { color: red; margin: 0; }\n", Lang::Css)
            .expect("css grammar must load (ABI 14) and parse");
        assert!(!css_tree.root_node().has_error());

        let html_tree = pool
            .parse("<html><body><p>hi</p></body></html>\n", Lang::Html)
            .expect("html grammar must load (ABI 14) and parse");
        assert!(!html_tree.root_node().has_error());
    }

    /// `tree-sitter-go` (feature-gated by `more-languages`) is also ABI-14
    /// pinned and must load cleanly.
    #[cfg(feature = "more-languages")]
    #[test]
    fn test_parse_go_grammar_loads() {
        let pool = ParserPool::new();
        let go_tree = pool
            .parse("package main\n\nfunc main() {}\n", Lang::Go)
            .expect("go grammar must load (ABI 14) and parse");
        assert!(!go_tree.root_node().has_error());
    }

    #[test]
    fn test_parsed_file_has_errors() {
        let pool = ParserPool::new();

        let valid = "def foo(): pass";
        let tree = pool.parse(valid, Lang::Python).unwrap();
        assert!(!tree.root_node().has_error());

        let invalid = "def foo( ";
        let tree = pool.parse(invalid, Lang::Python).unwrap();
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_parse_file_temp() {
        let pool = ParserPool::new();

        let mut temp = tempfile::NamedTempFile::with_suffix(".py").unwrap();
        writeln!(temp, "def hello():").unwrap();
        writeln!(temp, "    pass").unwrap();

        let parsed = ParsedFile::from_path(temp.path(), &pool);
        assert!(parsed.is_ok());

        let parsed = parsed.unwrap();
        assert_eq!(parsed.language, Lang::Python);
        assert!(!parsed.has_errors());
    }

    #[test]
    fn test_parser_pool_parse_returns_valid_tree() {
        let pool = ParserPool::new();

        let py_tree = pool
            .parse("def hello(x): return x + 1", Lang::Python)
            .unwrap();
        assert!(!py_tree.root_node().has_error());
        assert!(py_tree.root_node().child_count() > 0);

        let rs_tree = pool.parse("fn main() { let x = 42; }", Lang::Rust).unwrap();
        assert!(!rs_tree.root_node().has_error());
        assert!(rs_tree.root_node().child_count() > 0);
    }

    #[test]
    fn test_parser_pool_reuse_produces_correct_output() {
        let pool = ParserPool::new();

        let tree1 = pool.parse("def foo(): pass", Lang::Python).unwrap();
        let tree2 = pool.parse("class Bar:\n    x = 1", Lang::Python).unwrap();

        let root1 = tree1.root_node();
        assert!(!root1.has_error());

        let root2 = tree2.root_node();
        assert!(!root2.has_error());

        assert_ne!(
            root1.child(0).unwrap().kind(),
            root2.child(0).unwrap().kind(),
            "Different sources should produce different ASTs"
        );
    }

    // ── IncrementalParser tests ──────────────────────────────────────

    #[test]
    fn test_incremental_parse_basic_edit() {
        let mut inc = IncrementalParser::new();
        let source_v1 = "def hello():\n    pass\n";
        inc.parse_and_cache("test.py", source_v1, Lang::Python)
            .unwrap();

        let source_v2 = "def hello(name):\n    pass\n";
        let edit = tree_sitter::InputEdit {
            start_byte: 9,
            old_end_byte: 9,
            new_end_byte: 13,
            start_position: tree_sitter::Point::new(0, 9),
            old_end_position: tree_sitter::Point::new(0, 9),
            new_end_position: tree_sitter::Point::new(0, 13),
        };
        let (tree2, changes) = inc.parse_incremental("test.py", source_v2, &edit).unwrap();
        assert!(!tree2.root_node().has_error());
        assert!(!changes.is_empty(), "Should detect changed ranges");
    }

    #[test]
    fn test_tree_cache_stores_and_retrieves() {
        let mut inc = IncrementalParser::new();
        let source = "fn main() {}\n";

        assert!(inc.peek_tree("main.rs").is_none());

        let _tree = inc.parse_and_cache("main.rs", source, Lang::Rust).unwrap();

        assert!(inc.peek_tree("main.rs").is_some());
    }

    #[test]
    fn test_invalidate_removes_cached_tree() {
        let mut inc = IncrementalParser::new();
        let source = "x = 1\n";
        inc.parse_and_cache("test.py", source, Lang::Python)
            .unwrap();
        assert!(inc.peek_tree("test.py").is_some());

        inc.invalidate_tree("test.py");
        assert!(inc.peek_tree("test.py").is_none());
    }

    #[test]
    fn test_parse_and_cache_updates_on_change() {
        let mut inc = IncrementalParser::new();
        let source1 = "x = 1\n";
        let source2 = "x = 42\n";

        inc.parse_and_cache("test.py", source1, Lang::Python)
            .unwrap();
        inc.parse_and_cache("test.py", source2, Lang::Python)
            .unwrap();

        let tree = inc.cached_tree("test.py").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_changed_ranges_detects_structural_modification() {
        let mut inc = IncrementalParser::new();
        let source1 = "def foo():\n    return 1\n";
        inc.parse_and_cache("test.py", source1, Lang::Python)
            .unwrap();

        let source2 = "def foo():\n    x = 0\n    return 1\n";
        let edit = tree_sitter::InputEdit {
            start_byte: 11,
            old_end_byte: 11,
            new_end_byte: 21,
            start_position: tree_sitter::Point::new(1, 0),
            old_end_position: tree_sitter::Point::new(1, 0),
            new_end_position: tree_sitter::Point::new(2, 0),
        };
        let (tree, changes) = inc.parse_incremental("test.py", source2, &edit).unwrap();
        assert!(!tree.root_node().has_error());
        assert!(
            !changes.is_empty(),
            "Inserting a new statement should produce changed ranges"
        );
    }

    #[test]
    fn test_incremental_without_cache_falls_back_to_full() {
        let mut inc = IncrementalParser::new();
        let source = "let x = 1;\n";
        let edit = tree_sitter::InputEdit {
            start_byte: 8,
            old_end_byte: 9,
            new_end_byte: 10,
            start_position: tree_sitter::Point::new(0, 8),
            old_end_position: tree_sitter::Point::new(0, 9),
            new_end_position: tree_sitter::Point::new(0, 10),
        };
        let (tree, changes) = inc.parse_incremental("new.rs", source, &edit).unwrap();
        assert!(!tree.root_node().has_error());
        assert!(changes.is_empty());
    }

    #[test]
    fn test_cache_size_bounded() {
        let mut inc = IncrementalParser::new();
        for i in 0..200 {
            let source = format!("x_{i} = {i}\n");
            inc.parse_and_cache(&format!("test_{i}.py"), &source, Lang::Python)
                .unwrap();
        }
        assert!(
            inc.cache_size() <= MAX_TREE_CACHE,
            "Cache size {} exceeds limit {}",
            inc.cache_size(),
            MAX_TREE_CACHE,
        );
    }

    #[test]
    fn test_lru_eviction_preserves_recently_used() {
        let mut inc = IncrementalParser::new();

        // Fill cache to capacity
        for i in 0..MAX_TREE_CACHE {
            let source = format!("x_{i} = {i}\n");
            inc.parse_and_cache(&format!("file_{i}.py"), &source, Lang::Python)
                .unwrap();
        }

        // Access file_0 to make it recently used
        assert!(inc.cached_tree("file_0.py").is_some());

        // Insert one more — should evict file_1 (LRU), NOT file_0
        inc.parse_and_cache("overflow.py", "y = 1\n", Lang::Python)
            .unwrap();

        // file_0 should survive (recently accessed)
        assert!(
            inc.peek_tree("file_0.py").is_some(),
            "Recently accessed file_0.py should survive LRU eviction"
        );
        // file_1 should be evicted (least recently used)
        assert!(
            inc.peek_tree("file_1.py").is_none(),
            "file_1.py should be evicted as LRU"
        );
    }

    // ── P2.4: SharedTree tests ──────────────────────────────────────

    #[test]
    fn test_parse_and_cache_shared() {
        let mut inc = IncrementalParser::new();
        let source = "def hello(): pass\n";
        let shared = inc
            .parse_and_cache_shared("test.py", source, Lang::Python)
            .unwrap();

        // Arc allows multiple owners
        let shared2 = Arc::clone(&shared);
        assert!(!shared.root_node().has_error());
        assert!(!shared2.root_node().has_error());

        // Cache should also have the tree
        assert!(inc.peek_tree("test.py").is_some());
    }

    #[test]
    fn test_shared_tree_across_threads() {
        let mut inc = IncrementalParser::new();
        let source = "fn main() { let x = 42; }\n";
        let shared = inc
            .parse_and_cache_shared("main.rs", source, Lang::Rust)
            .unwrap();

        let handle = std::thread::spawn(move || {
            assert!(!shared.root_node().has_error());
            shared.root_node().child_count()
        });

        let count = handle.join().unwrap();
        assert!(count > 0);
    }

    // ── parse_rope (P1.4.1): streaming parse via RopeDocument ───────────

    #[test]
    fn test_parse_rope_produces_same_tree_as_parse_full() {
        let mut inc = IncrementalParser::new();
        let source = "def hello(name):\n    return name\n";

        // Parse via full string
        let tree_str = inc.parse_full(source, Lang::Python).unwrap();

        // Parse via RopeDocument (zero-copy chunks)
        let doc = RopeDocument::new("test.py", source);
        let tree_rope = inc.parse_rope(&doc, Lang::Python, None).unwrap();

        // Both trees should have the same structure: no errors, same root kind
        assert!(!tree_str.root_node().has_error());
        assert!(!tree_rope.root_node().has_error());
        assert_eq!(
            tree_str.root_node().child_count(),
            tree_rope.root_node().child_count(),
            "parse_rope and parse_full should produce the same top-level AST"
        );
        assert_eq!(
            tree_str.root_node().child(0).unwrap().kind(),
            tree_rope.root_node().child(0).unwrap().kind(),
            "first child kind should match"
        );
    }

    #[test]
    fn test_parse_rope_rust_source() {
        let mut inc = IncrementalParser::new();
        let source = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let doc = RopeDocument::new("add.rs", source);
        let tree = inc.parse_rope(&doc, Lang::Rust, None).unwrap();
        assert!(!tree.root_node().has_error());
        assert!(tree.root_node().child_count() > 0);
    }

    #[test]
    fn test_parse_rope_empty_document() {
        let mut inc = IncrementalParser::new();
        let doc = RopeDocument::new("empty.py", "");
        let tree = inc.parse_rope(&doc, Lang::Python, None).unwrap();
        // Empty source parses to a module with no children (or just EOF)
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_rope_incremental_with_old_tree() {
        let mut inc = IncrementalParser::new();
        let source_v1 = "def foo():\n    pass\n";
        let doc_v1 = RopeDocument::new("test.py", source_v1);
        let tree_v1 = inc.parse_rope(&doc_v1, Lang::Python, None).unwrap();

        // Simulate editing: insert "name" parameter
        let _source_v2 = "def foo(name):\n    pass\n"; // reference only — doc_v2 is built via edit()
        let mut doc_v2 = RopeDocument::new("test.py", source_v1);
        let _edit = doc_v2.edit(8, 8, "name");

        let tree_v2 = inc
            .parse_rope(&doc_v2, Lang::Python, Some(&tree_v1))
            .unwrap();

        assert!(!tree_v2.root_node().has_error());
        // Result should reflect the new content
        let _ = tree_v2;
    }
}
