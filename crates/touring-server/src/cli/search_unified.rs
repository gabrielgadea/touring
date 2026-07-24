//! `touring search unified|exact|fuzzy|bm25|index|overlay` — Unified search with RRF fusion.
//!
//! RRF (Reciprocal Rank Fusion) combines multiple result rankings using the formula:
//! `score(d) = sum_i(1 / (k + rank_i(d)))` where k=60 is the standard constant.
//!
//! Wave P3-1.3 W5b (2026-06-11): migrated from manual `arg_or` / `flag_value` parsing
//! to clap derive. The `run` signature is unchanged. All business logic (rrf_fuse,
//! parse_symbols_response, hybrid pipeline, VFS overlay, etc.) is preserved verbatim (G6).

use super::daemon_query;
use clap::{Parser, Subcommand, ValueEnum};
use ignore::WalkBuilder;
use std::sync::Arc;
use touring_storage::embeddings::{FastEmbedModel, FastEmbedProvider};
use touring_storage::hybrid_search::{
    HybridConfig, HybridQuery, HybridQueryIntent, IntentQueryIntent, SearchPipeline, detect_intent,
};
use touring_storage::vec::InMemoryVectorStore;
use touring_storage::vfs::{AbsPath, FileSet, VfsOverlay};

const RRF_K: f32 = 60.0;

/// A search result from a single backend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackendResult {
    /// 1-based rank of this hit within its originating backend's result list.
    pub rank: usize,
    /// Path of the file containing the hit.
    pub file_path: String,
    /// Line number of the hit, when the backend reports one.
    pub line: Option<usize>,
    /// Column number of the hit, when the backend reports one.
    pub col: Option<usize>,
    /// Matched symbol name, populated for symbol backends.
    pub symbol: Option<String>,
    /// Surrounding context snippet, populated for document backends.
    pub context: Option<String>,
    /// Identifier of the backend that produced this result (e.g. `symbols`, `docs`, `hybrid`).
    pub backend: String,
}

impl BackendResult {
    /// Unique key for RRF score aggregation: "file_path:line:col"
    fn rrf_key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.file_path,
            self.line.unwrap_or(0),
            self.col.unwrap_or(0)
        )
    }
}

/// Final fused search result with RRF score.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    /// 1-based rank in the fused result list, ordered by descending `rrf_score`.
    pub rank: usize,
    /// Path of the file containing the hit.
    pub file_path: String,
    /// Line number of the hit, parsed from the RRF aggregation key when present.
    pub line: Option<usize>,
    /// Column number of the hit, parsed from the RRF aggregation key when present.
    pub col: Option<usize>,
    /// Matched symbol name; currently always `None` for fused results.
    pub symbol: Option<String>,
    /// Surrounding context snippet; currently always `None` for fused results.
    pub context: Option<String>,
    /// Origin label for fused results (always `unified`).
    pub backend: String,
    /// Aggregated Reciprocal Rank Fusion score summed across all backends.
    pub rrf_score: f32,
}

// ─────────────────────────────────────────────────────────────────────────────
// clap derive types (Wave P3-1.3 W5b)
// ─────────────────────────────────────────────────────────────────────────────

/// Search intent hint for the `unified` subcommand.
#[derive(Debug, Clone, ValueEnum)]
enum SearchIntent {
    Understand,
    Debug,
    Implement,
    Refactor,
    Document,
    Explore,
}

#[derive(Parser, Debug)]
#[command(
    name = "touring search",
    bin_name = "touring search",
    about = "Unified search: unified (default), exact, fuzzy, bm25, index, overlay",
    disable_help_subcommand = true
)]
struct SearchCli {
    #[command(subcommand)]
    cmd: Option<SearchCmd>,
}

#[derive(Subcommand, Debug)]
enum SearchCmd {
    /// Multi-backend RRF-fused search (default).
    Unified {
        /// Query string.
        query: String,
        /// Maximum number of results (default: 20).
        #[arg(long, default_value_t = 20usize)]
        limit: usize,
        /// Optional intent hint for ranking.
        #[arg(long, value_enum)]
        intent: Option<SearchIntent>,
    },
    /// Exact symbol search via daemon `cli-search-symbols`.
    Exact {
        /// Query string.
        query: String,
        /// Maximum number of results (default: 20).
        #[arg(long, default_value_t = 20usize)]
        limit: usize,
    },
    /// Fuzzy document search via daemon `cli-search-docs`.
    Fuzzy {
        /// Query string.
        query: String,
        /// Maximum number of results (default: 20).
        #[arg(long, default_value_t = 20usize)]
        limit: usize,
    },
    /// BM25 document search via daemon `cli-search-docs`.
    Bm25 {
        /// Query string.
        query: String,
        /// Maximum number of results (default: 20).
        #[arg(long, default_value_t = 20usize)]
        limit: usize,
    },
    /// Index files in a directory into the in-memory vector store.
    Index {
        /// Root directory to index (default: ".").
        #[arg(default_value = ".")]
        path: String,
    },
    /// Overlay-based glob search via FileSet.
    Overlay {
        /// Root directory (default: ".").
        #[arg(default_value = ".")]
        root: String,
        /// Glob pattern (default: "**/*").
        #[arg(default_value = "**/*")]
        pattern: String,
    },
    /// Tool catalog search: find CLI tools by intent description.
    ///
    /// Delegates to the in-process tool catalog (no daemon required). Returns
    /// the top-k tools ranked by BM25 relevance to the supplied intent string.
    ///
    /// # Examples
    ///
    /// ```text
    /// touring search tools wiring orphans
    /// touring search tools "cognitive metrics" --top 5
    /// ```
    // The variant name already derives the `tools` subcommand; a redundant
    // `alias = "tools"` collides with the command's own name and trips clap's
    // debug assertion at startup (panics every invocation). (A5 fix)
    Tools {
        /// Intent or keyword(s) to match against tool descriptions.
        intent: Vec<String>,
        /// Maximum number of tools to return (default: 10).
        #[arg(long, default_value_t = 10usize)]
        top: usize,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Run the search subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match SearchCli::try_parse_from(args.iter().skip(1)) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(SearchCmd::Unified {
        query: String::new(),
        limit: 20,
        intent: None,
    }) {
        SearchCmd::Unified {
            query,
            limit,
            intent,
        } => run_unified(&query, limit, intent),
        SearchCmd::Exact { query, limit } => {
            let payload = serde_json::json!({ "query": query, "top": limit });
            let output = daemon_query("cli-search-symbols", payload)?;
            println!("{output}");
            Ok(())
        }
        SearchCmd::Fuzzy { query, limit } => {
            let payload = serde_json::json!({ "query": query, "top": limit });
            let output = daemon_query("cli-search-docs", payload)?;
            println!("{output}");
            Ok(())
        }
        SearchCmd::Bm25 { query, limit } => {
            let payload = serde_json::json!({ "query": query, "top": limit });
            let output = daemon_query("cli-search-docs", payload)?;
            println!("{output}");
            Ok(())
        }
        SearchCmd::Index { path } => run_index(&path),
        SearchCmd::Overlay { root, pattern } => run_overlay(&root, &pattern),
        SearchCmd::Tools { intent, top } => {
            let query = intent.join(" ");
            let result = crate::tool_catalog::search_as_json(&query, top);
            println!(
                "{}",
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            );
            Ok(())
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers (G6: logic preserved verbatim from pre-migration)
// ─────────────────────────────────────────────────────────────────────────────

/// Map the clap `SearchIntent` value to the string form used by the daemon.
fn intent_to_daemon_str(intent: &SearchIntent, _query: &str) -> String {
    match intent {
        SearchIntent::Understand | SearchIntent::Implement => "understand".to_string(),
        SearchIntent::Debug => "lookup".to_string(),
        SearchIntent::Refactor => "navigate".to_string(),
        SearchIntent::Document | SearchIntent::Explore => "explore".to_string(),
    }
}

/// Run the unified multi-backend RRF search.
fn run_unified(query: &str, limit: usize, intent_opt: Option<SearchIntent>) -> anyhow::Result<()> {
    let query_str = query.to_string();
    let intent_str: String = if let Some(ref intent_val) = intent_opt {
        intent_to_daemon_str(intent_val, &query_str)
    } else {
        let result = detect_intent(&query_str);
        match result.intent {
            IntentQueryIntent::Understand | IntentQueryIntent::Implement => {
                "understand".to_string()
            }
            IntentQueryIntent::Debug => "lookup".to_string(),
            IntentQueryIntent::Refactor => "navigate".to_string(),
            IntentQueryIntent::Document | IntentQueryIntent::Explore => "explore".to_string(),
        }
    };

    // Query daemon backends.
    let symbols_out = daemon_query(
        "cli-search-symbols",
        serde_json::json!({
            "query": query_str, "top": limit, "intent": intent_str
        }),
    );
    let docs_out = daemon_query(
        "cli-search-docs",
        serde_json::json!({
            "query": query_str, "top": limit, "intent": intent_str
        }),
    );

    let exact_results = symbols_out
        .as_ref()
        .ok()
        .map(|s| parse_symbols_response(s, "symbols"))
        .unwrap_or_default();
    let fuzzy_results = docs_out
        .as_ref()
        .ok()
        .map(|s| parse_docs_response(s, "docs"))
        .unwrap_or_default();

    // Also call the hybrid search fusion pipeline for semantic results.
    let hybrid_out = std::thread::spawn({
        let query = query_str.clone();
        let intent_clone = intent_str.clone();
        let limit_inner = limit;
        move || {
            let intent_h = match intent_clone.as_str() {
                "understand" => HybridQueryIntent::Understand,
                "lookup" => HybridQueryIntent::Lookup,
                "navigate" => HybridQueryIntent::Navigate,
                _ => HybridQueryIntent::Explore,
            };
            let provider = FastEmbedProvider::with_model(FastEmbedModel::BgeSmall);
            let store = Arc::new(InMemoryVectorStore::default());
            let config = HybridConfig::default();
            let pipeline =
                SearchPipeline::with_provider_and_store(config, Arc::new(provider), store);
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime for hybrid search");
            let hybrid_query = HybridQuery {
                query: query.to_string(),
                intent: intent_h,
                top_k: limit_inner,
                rerank: false,
            };
            rt.block_on(pipeline.search(hybrid_query)).0
        }
    })
    .join()
    .unwrap_or_default();

    let hybrid_results: Vec<BackendResult> = hybrid_out
        .into_iter()
        .map(|sr| BackendResult {
            rank: 0,
            file_path: sr.doc_id.clone(),
            line: None,
            col: None,
            symbol: None,
            context: None,
            backend: "hybrid".to_string(),
        })
        .collect();

    let all_backends: Vec<Vec<BackendResult>> =
        if exact_results.is_empty() && fuzzy_results.is_empty() {
            vec![hybrid_results]
        } else {
            let mut backs = Vec::new();
            if !exact_results.is_empty() {
                backs.push(exact_results);
            }
            if !fuzzy_results.is_empty() {
                backs.push(fuzzy_results);
            }
            backs
        };

    let fused = rrf_fuse(all_backends);
    let limited: Vec<_> = fused
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(i, mut sr)| {
            sr.rank = i + 1;
            sr
        })
        .collect();
    println!("{}", serde_json::to_string(&limited).unwrap_or_default());
    Ok(())
}

/// Run the `index` subcommand — index files under `path` into the in-memory vector store.
fn run_index(path: &str) -> anyhow::Result<()> {
    use std::path::Path;
    use touring_storage::vfs::{FileManifest, content_hash};

    let cache_dir = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join(".cache")
        .join("touring-search-fusion");
    let manifest_path = cache_dir.join("manifest.json");

    let manifest: FileManifest = if manifest_path.exists() {
        let data = std::fs::read(&manifest_path).unwrap_or_default();
        serde_json::from_slice(&data).unwrap_or_default()
    } else {
        FileManifest::new()
    };

    let all_paths: Vec<std::path::PathBuf> = WalkBuilder::new(Path::new(path))
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .max_depth(Some(20))
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map_or(false, |ext| {
                matches!(
                    ext.to_str(),
                    Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "go" | "toml")
                )
            })
        })
        .take(5000)
        .map(|e| e.path().to_path_buf())
        .collect();

    if all_paths.is_empty() {
        println!(
            "{}",
            serde_json::json!({ "indexed": 0, "message": "no files found" })
        );
        return Ok(());
    }

    let moves_result = manifest.detect_moves(&all_paths);
    let move_count = moves_result.moves.len();

    let mut updated_manifest = manifest.clone();
    for mv in &moves_result.moves {
        updated_manifest.apply_move(mv);
    }
    for np in &moves_result.new_files {
        if let Ok(content) = std::fs::read(np) {
            let hash = content_hash(&content);
            updated_manifest.insert(np.clone(), hash);
        }
    }
    let _ = std::fs::create_dir_all(&cache_dir);
    let manifest_json = serde_json::to_vec(&updated_manifest).unwrap_or_default();
    let _ = std::fs::write(&manifest_path, &manifest_json);

    let to_index: Vec<(String, String)> = all_paths
        .into_iter()
        .filter(|p| !moves_result.duplicates.contains(p))
        .filter_map(|entry| {
            std::fs::read_to_string(&entry).ok().map(|content| {
                let doc_id = entry.to_string_lossy().to_string();
                let chunk = content.chars().take(8000).collect::<String>();
                (doc_id, chunk)
            })
        })
        .collect();

    if to_index.is_empty() {
        println!(
            "{}",
            serde_json::json!({
                "indexed": 0,
                "moves_detected": move_count,
                "duplicates": moves_result.duplicates.len(),
                "message": "nothing to index (all duplicates)"
            })
        );
        return Ok(());
    }

    let count = std::thread::spawn({
        let provider = Arc::new(FastEmbedProvider::with_model(FastEmbedModel::BgeSmall));
        let store = Arc::new(InMemoryVectorStore::default());
        let config = HybridConfig::default();
        let pipeline = SearchPipeline::with_provider_and_store(config, provider, store);
        let to_index = to_index.clone();
        move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime for indexing");
            rt.block_on(pipeline.upsert_documents(to_index))
                .unwrap_or_else(|e| {
                    eprintln!("index error: {}", e);
                    0
                })
        }
    })
    .join()
    .unwrap_or(0);

    println!(
        "{}",
        serde_json::json!({
            "indexed": count,
            "files": to_index.len(),
            "moves_detected": move_count,
            "duplicates": moves_result.duplicates.len()
        })
    );
    Ok(())
}

/// Run the `overlay` subcommand — overlay-based glob search via FileSet.
fn run_overlay(root: &str, pattern: &str) -> anyhow::Result<()> {
    use std::path::Path;

    let base_vfs = touring_storage::vfs::Vfs::new();
    let mut overlay = VfsOverlay::with_base(base_vfs);

    let search_paths: Vec<std::path::PathBuf> = WalkBuilder::new(Path::new(root))
        .hidden(false)
        .git_ignore(true)
        .max_depth(Some(20))
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map_or(false, |ext| {
                matches!(
                    ext.to_str(),
                    Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "go" | "toml")
                )
            })
        })
        .take(5000)
        .map(|e| e.path().to_path_buf())
        .collect();

    for p in &search_paths {
        if let Ok(content) = std::fs::read(p) {
            let path_str = p.to_string_lossy().into_owned();
            let abs = AbsPath::from_absolute(&path_str).unwrap_or_else(|_| {
                AbsPath::from_absolute("/dev/null").expect("/dev/null is absolute")
            });
            overlay.set(abs, content);
        }
    }

    let vfs = touring_storage::vfs::Vfs::new();
    let mut file_set = FileSet::new(vfs);
    for p in &search_paths {
        let path_str = p.to_string_lossy().into_owned();
        let abs = AbsPath::from_absolute(&path_str).ok();
        if let Some(path) = abs {
            let id = touring_storage::vfs::FileId::new(file_set.len() as u32 + 1);
            file_set.add_path(path, id);
        }
    }

    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| std::path::PathBuf::from(root));
    let root_str = canonical.to_string_lossy().into_owned();
    let root_abs = AbsPath::from_absolute(&root_str)
        .unwrap_or_else(|_| AbsPath::from_absolute("/").expect("/ is absolute"));

    let matched = file_set.glob(root_abs, pattern);
    let results: Vec<String> = matched.iter().map(|p| p.as_str().to_string()).collect();

    println!(
        "{}",
        serde_json::json!({
            "pattern": pattern,
            "root": root,
            "matched": results.len(),
            "files": results
        })
    );
    Ok(())
}

/// Fuse multiple ranked result lists using Reciprocal Rank Fusion.
/// k=60 is the standard constant.
fn rrf_fuse(backend_results: Vec<Vec<BackendResult>>) -> Vec<SearchResult> {
    use std::collections::HashMap;
    let mut scored: HashMap<String, f32> = HashMap::new();
    for backend in &backend_results {
        for (rank, result) in backend.iter().enumerate() {
            let key = result.rrf_key();
            let contribution = 1.0 / (RRF_K + rank as f32);
            *scored.entry(key).or_insert(0.0) += contribution;
        }
    }
    let mut sorted: Vec<_> = scored.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Less));
    sorted
        .into_iter()
        .enumerate()
        .map(|(i, (key, rrf_score))| {
            let parts: Vec<&str> = key.split(':').collect();
            SearchResult {
                rank: i + 1,
                file_path: parts.first().unwrap_or(&"").to_string(),
                line: parts.get(1).and_then(|s| s.parse().ok()),
                col: parts.get(2).and_then(|s| s.parse().ok()),
                symbol: None,
                context: None,
                backend: "unified".to_string(),
                rrf_score,
            }
        })
        .collect()
}

/// Parse daemon response from `cli_search_symbols` into BackendResult list.
fn parse_symbols_response(raw: &str, backend: &str) -> Vec<BackendResult> {
    let parsed: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let results = parsed.get("results").and_then(|v| v.as_array());
    let Some(results_arr) = results else {
        return Vec::new();
    };
    results_arr
        .iter()
        .enumerate()
        .map(|(i, obj)| BackendResult {
            rank: i + 1,
            file_path: obj
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            line: None,
            col: None,
            symbol: obj
                .get("symbol_name")
                .and_then(|v| v.as_str())
                .map(String::from),
            context: None,
            backend: backend.to_string(),
        })
        .collect()
}

/// Parse daemon response from `cli_search_docs` into BackendResult list.
fn parse_docs_response(raw: &str, backend: &str) -> Vec<BackendResult> {
    let parsed: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let results = parsed.get("results").and_then(|v| v.as_array());
    let Some(results_arr) = results else {
        return Vec::new();
    };
    results_arr
        .iter()
        .enumerate()
        .map(|(i, obj)| BackendResult {
            rank: i + 1,
            file_path: obj
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            line: None,
            col: None,
            symbol: None,
            context: obj
                .get("context_value")
                .and_then(|v| v.as_str())
                .map(String::from),
            backend: backend.to_string(),
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    SearchCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── clap parse smoke tests (W5b) ─────────────────────────────────────────

    #[test]
    fn parses_unified_subcommand_with_query() {
        let cli = SearchCli::try_parse_from(["search", "unified", "my query"]).unwrap();
        let SearchCmd::Unified {
            query,
            limit,
            intent,
        } = cli.cmd.unwrap()
        else {
            panic!("expected Unified")
        };
        assert_eq!(query, "my query");
        assert_eq!(limit, 20);
        assert!(intent.is_none());
    }

    #[test]
    fn parses_unified_with_limit_and_intent() {
        let cli = SearchCli::try_parse_from([
            "search", "unified", "foo", "--limit", "50", "--intent", "debug",
        ])
        .unwrap();
        let SearchCmd::Unified {
            query,
            limit,
            intent,
        } = cli.cmd.unwrap()
        else {
            panic!("expected Unified")
        };
        assert_eq!(query, "foo");
        assert_eq!(limit, 50);
        assert!(matches!(intent, Some(SearchIntent::Debug)));
    }

    #[test]
    fn parses_exact_subcommand() {
        let cli = SearchCli::try_parse_from(["search", "exact", "MySymbol"]).unwrap();
        let SearchCmd::Exact { query, limit } = cli.cmd.unwrap() else {
            panic!("expected Exact")
        };
        assert_eq!(query, "MySymbol");
        assert_eq!(limit, 20);
    }

    #[test]
    fn parses_fuzzy_subcommand() {
        let cli = SearchCli::try_parse_from(["search", "fuzzy", "partial"]).unwrap();
        assert!(matches!(cli.cmd, Some(SearchCmd::Fuzzy { .. })));
    }

    #[test]
    fn parses_bm25_subcommand() {
        let cli = SearchCli::try_parse_from(["search", "bm25", "term"]).unwrap();
        assert!(matches!(cli.cmd, Some(SearchCmd::Bm25 { .. })));
    }

    #[test]
    fn parses_index_with_default_path() {
        let cli = SearchCli::try_parse_from(["search", "index"]).unwrap();
        let SearchCmd::Index { path } = cli.cmd.unwrap() else {
            panic!("expected Index")
        };
        assert_eq!(path, ".");
    }

    #[test]
    fn parses_index_with_explicit_path() {
        let cli = SearchCli::try_parse_from(["search", "index", "/some/dir"]).unwrap();
        let SearchCmd::Index { path } = cli.cmd.unwrap() else {
            panic!("expected Index")
        };
        assert_eq!(path, "/some/dir");
    }

    #[test]
    fn parses_overlay_with_defaults() {
        let cli = SearchCli::try_parse_from(["search", "overlay"]).unwrap();
        let SearchCmd::Overlay { root, pattern } = cli.cmd.unwrap() else {
            panic!("expected Overlay")
        };
        assert_eq!(root, ".");
        assert_eq!(pattern, "**/*");
    }

    #[test]
    fn parses_overlay_with_explicit_args() {
        let cli = SearchCli::try_parse_from(["search", "overlay", "/root", "**/*.rs"]).unwrap();
        let SearchCmd::Overlay { root, pattern } = cli.cmd.unwrap() else {
            panic!("expected Overlay")
        };
        assert_eq!(root, "/root");
        assert_eq!(pattern, "**/*.rs");
    }

    #[test]
    fn unknown_subcommand_errors() {
        let result = SearchCli::try_parse_from(["search", "invalid_sub"]);
        assert!(result.is_err());
    }

    #[test]
    fn exact_limit_flag() {
        let cli = SearchCli::try_parse_from(["search", "exact", "foo", "--limit", "50"]).unwrap();
        let SearchCmd::Exact { limit, .. } = cli.cmd.unwrap() else {
            panic!()
        };
        assert_eq!(limit, 50);
    }

    // ── RRF logic (pre-existing tests, unchanged) ─────────────────────────────

    #[test]
    fn test_rrf_fuse_empty() {
        let results: Vec<Vec<BackendResult>> = vec![];
        let fused = rrf_fuse(results);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_rrf_fuse_single_backend() {
        let backend = vec![
            BackendResult {
                rank: 1,
                file_path: "a.rs".to_string(),
                line: Some(10),
                col: Some(5),
                symbol: Some("foo".to_string()),
                context: None,
                backend: "exact".to_string(),
            },
            BackendResult {
                rank: 2,
                file_path: "b.rs".to_string(),
                line: Some(20),
                col: None,
                symbol: Some("bar".to_string()),
                context: None,
                backend: "exact".to_string(),
            },
        ];
        let fused = rrf_fuse(vec![backend]);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].file_path, "a.rs");
        assert_eq!(fused[0].rank, 1);
        assert!(fused[0].rrf_score > fused[1].rrf_score);
    }

    #[test]
    fn test_rrf_fuse_two_backends_same_doc() {
        let exact = vec![BackendResult {
            rank: 1,
            file_path: "shared.rs".to_string(),
            line: Some(10),
            col: None,
            symbol: Some("func_a".to_string()),
            context: None,
            backend: "exact".to_string(),
        }];
        let fuzzy = vec![BackendResult {
            rank: 2,
            file_path: "shared.rs".to_string(),
            line: Some(10),
            col: None,
            symbol: None,
            context: Some("documentation".to_string()),
            backend: "fuzzy".to_string(),
        }];
        let fused = rrf_fuse(vec![exact, fuzzy]);
        assert_eq!(fused.len(), 1);
        let expected = 2.0 / RRF_K;
        assert!((fused[0].rrf_score - expected).abs() < 0.0001);
    }

    #[test]
    fn test_rrf_fuse_different_docs() {
        let exact = vec![BackendResult {
            rank: 0,
            file_path: "a.rs".to_string(),
            line: Some(1),
            col: None,
            symbol: Some("foo".to_string()),
            context: None,
            backend: "exact".to_string(),
        }];
        let fuzzy = vec![BackendResult {
            rank: 0,
            file_path: "b.rs".to_string(),
            line: Some(2),
            col: None,
            symbol: None,
            context: Some("doc".to_string()),
            backend: "fuzzy".to_string(),
        }];
        let fused = rrf_fuse(vec![exact, fuzzy]);
        assert_eq!(fused.len(), 2);
        assert!((fused[0].rrf_score - fused[1].rrf_score).abs() < 0.0001);
    }

    #[test]
    fn test_rrf_k_constant() {
        assert_eq!(RRF_K, 60.0);
    }

    #[test]
    fn test_parse_symbols_response_valid() {
        let raw = r#"{"query":"test","results":[{"symbol_name":"Foo","file_path":"a.rs","symbol_kind":"fn"}],"count":1}"#;
        let results = parse_symbols_response(raw, "exact");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "a.rs");
        assert_eq!(results[0].symbol, Some("Foo".to_string()));
        assert_eq!(results[0].backend, "exact");
    }

    #[test]
    fn test_parse_symbols_response_empty() {
        let raw = r#"{"query":"test","results":[],"count":0}"#;
        let results = parse_symbols_response(raw, "exact");
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_docs_response_valid() {
        let raw = r#"{"query":"test","results":[{"file_path":"docs.rs","context_value":"some doc"}],"count":1}"#;
        let results = parse_docs_response(raw, "fuzzy");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "docs.rs");
        assert_eq!(results[0].context, Some("some doc".to_string()));
        assert_eq!(results[0].backend, "fuzzy");
    }

    #[test]
    fn test_parse_invalid_json() {
        let results = parse_symbols_response("not json", "exact");
        assert!(results.is_empty());
    }

    #[test]
    fn test_rrf_key_uniqueness() {
        let r1 = BackendResult {
            rank: 1,
            file_path: "a.rs".to_string(),
            line: Some(10),
            col: Some(5),
            symbol: None,
            context: None,
            backend: "exact".to_string(),
        };
        let r2 = BackendResult {
            rank: 1,
            file_path: "a.rs".to_string(),
            line: Some(10),
            col: Some(5),
            symbol: None,
            context: None,
            backend: "fuzzy".to_string(),
        };
        assert_eq!(r1.rrf_key(), r2.rrf_key());
    }

    #[test]
    fn test_vfs_overlay_wired() {
        use touring_storage::vfs::AbsPath;
        use touring_storage::vfs::VfsOverlay;

        let mut overlay = VfsOverlay::new();
        let path = AbsPath::from_absolute("/test/overlay.txt").unwrap();
        overlay.set(path, b"test content".as_slice());
        assert!(overlay.exists(path));
        let content = overlay.read(path).unwrap();
        assert_eq!(content.as_ref(), b"test content");
    }

    #[test]
    fn test_file_set_wired() {
        use touring_storage::vfs::{AbsPath, FileId, FileSet, Vfs};

        let vfs = Vfs::new();
        let mut file_set = FileSet::new(vfs);
        let path = AbsPath::from_absolute("/src/test.rs").unwrap();
        file_set.add_path(path, FileId::new(42));
        assert_eq!(file_set.get(path), Some(FileId::new(42)));
    }

    #[test]
    fn test_file_set_glob() {
        use touring_storage::vfs::{AbsPath, FileId, FileSet, Vfs};

        let vfs = Vfs::new();
        let mut file_set = FileSet::new(vfs);
        file_set.add_path(
            AbsPath::from_absolute("/src/foo.rs").unwrap(),
            FileId::new(1),
        );
        file_set.add_path(
            AbsPath::from_absolute("/src/bar.rs").unwrap(),
            FileId::new(2),
        );
        let base = AbsPath::from_absolute("/src").unwrap();
        let results = file_set.glob(base, "*.rs");
        assert_eq!(results.len(), 2);
    }

    // ── A5: SearchCmd::Tools (tool catalog search) ────────────────────────

    #[test]
    fn test_search_tools_empty_query_returns_valid_json() {
        // search_as_json with empty query must return a JSON array or object without panicking.
        let result = crate::tool_catalog::search_as_json("", 5);
        // Must serialise without error.
        let _s = serde_json::to_string(&result)
            .expect("search_as_json result must be JSON-serialisable");
        // Must be array or object (not a bare string/null).
        assert!(
            result.is_array() || result.is_object(),
            "expected JSON array or object, got: {result}"
        );
    }

    #[test]
    fn test_search_tools_known_intent_wiring() {
        let result = crate::tool_catalog::search_as_json("wiring", 3);
        let _s = serde_json::to_string(&result).expect("must serialise");
        // Result is non-null (wiring is a known domain).
        assert!(!result.is_null());
    }

    #[test]
    fn test_search_tools_known_intent_cognitive() {
        let result = crate::tool_catalog::search_as_json("cognitive metrics", 3);
        let _s = serde_json::to_string(&result).expect("must serialise");
        assert!(!result.is_null());
    }

    #[test]
    fn test_search_tools_top_k_zero_does_not_panic() {
        // Asking for 0 results must not panic; behaviour is implementation-defined.
        let result = crate::tool_catalog::search_as_json("anything", 0);
        let _s = serde_json::to_string(&result).expect("must serialise");
    }
}
