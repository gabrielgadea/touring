//! `touring search unified|exact|fuzzy|bm25|text|index|overlay` — Unified search with RRF fusion.
//!
//! RRF (Reciprocal Rank Fusion) combines multiple result rankings using the formula:
//! `score(d) = sum_i(1 / (k + rank_i(d)))` where k=60 is the standard constant.
//!
//! Wave P3-1.3 W5b (2026-06-11): migrated from manual `arg_or` / `flag_value` parsing
//! to clap derive. The `run` signature is unchanged. 2026-09-13: `unified` fuses the
//! `tantivy search` ranking with the BM25 docs lane (weighted, one vote per file per
//! lane) — the weights and the lanes left out are measured, see `run_unified`.

use super::daemon_query;
use clap::{Parser, Subcommand, ValueEnum};
use ignore::WalkBuilder;
use std::sync::Arc;
use touring_storage::embeddings::{FastEmbedModel, FastEmbedProvider};
use touring_storage::hybrid_search::{
    HybridConfig, IntentQueryIntent, SearchPipeline, detect_intent,
};
use touring_storage::vec::InMemoryVectorStore;
use touring_storage::vfs::{AbsPath, FileSet, VfsOverlay};

const RRF_K: f32 = 60.0;

/// The `cli-search-docs` ranking the unified search fuses: `bm25` (plain) or
/// `fuzzy` (BM25 ⊕ edit-distance ⊕ trigram, RRF). Decided by the retrieval bench
/// of 2026-09-13 (docs/plans/2026-09-12-graft-analysis §29), not by taste.
const UNIFIED_DOCS_MODE: &str = "bm25";

/// `touring search text`: BM25 over document TEXT only — what a memory, a rule or
/// a skill says, never what a symbol is named.
const TEXT_MODE: &str = "text";

/// The primary lane of `search unified`: the `tantivy search` ranking.
const UNIFIED_RANKED_MODE: &str = "ranked";

/// RRF weight of the ranked lane (the reference, 1.0).
const UNIFIED_RANKED_WEIGHT: f32 = 1.0;

/// RRF weight of the BM25 docs lane: a tie-breaker (49 of 55 vs 48 without it).
const UNIFIED_DOCS_WEIGHT: f32 = 0.05;

/// Hits asked of each lane before fusion, whatever `--limit` shows: a file's
/// place in the fusion must not depend on how many rows the caller prints.
const UNIFIED_LANE_DEPTH: usize = 60;

/// The payload `cli-search-docs` takes. `mode` is what tells `search fuzzy`
/// apart from `search bm25` — until 2026-09-13 both sent the same bytes.
fn docs_payload(query: &str, limit: usize, mode: &str) -> serde_json::Value {
    serde_json::json!({ "query": query, "top": limit, "mode": mode })
}

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
    /// Fuzzy document search via daemon `cli-search-docs` in `mode = fuzzy`:
    /// BM25 ⊕ edit-distance-2 ⊕ trigram, fused by RRF (`search_rrf`). Until
    /// 2026-09-13 this was the same route as `bm25` under a different name.
    Fuzzy {
        /// Query string.
        query: String,
        /// Maximum number of results (default: 20).
        #[arg(long, default_value_t = 20usize)]
        limit: usize,
    },
    /// Text search via daemon `cli-search-docs` in `mode = text`: BM25 over what
    /// documents SAY (markdown sections and documents — memories, rules, skills),
    /// never over symbol names.
    Text {
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
            let output = daemon_query("cli-search-docs", docs_payload(&query, limit, "fuzzy"))?;
            println!("{output}");
            Ok(())
        }
        SearchCmd::Text { query, limit } => {
            let output = daemon_query("cli-search-docs", docs_payload(&query, limit, TEXT_MODE))?;
            println!("{output}");
            Ok(())
        }
        SearchCmd::Bm25 { query, limit } => {
            let output = daemon_query("cli-search-docs", docs_payload(&query, limit, "bm25"))?;
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

    // Query daemon backends. 13/09/2026: the lanes and their weights come from a
    // replay of the live lanes over 55 questions (`docs/plans/2026-09-12-graft-
    // analysis/bench/`). Fusing the symbols LIKE lane, the BM25 docs lane and the
    // text lane with equal weight scored 27; the `tantivy search` ranking alone
    // scored 48, and adding the BM25 lane as a tie-breaker (weight 0.05) 49. The
    // LIKE and text lanes lowered every mix they joined (identifiers 14 → 6 with
    // the text lane), so they are subcommands of their own (`search exact`,
    // `search text`), not fusion inputs.
    let mut ranked_payload = docs_payload(
        &query_str,
        limit.max(UNIFIED_LANE_DEPTH),
        UNIFIED_RANKED_MODE,
    );
    if let Some(obj) = ranked_payload.as_object_mut() {
        obj.insert(
            "intent".into(),
            serde_json::Value::String(intent_str.clone()),
        );
    }
    let ranked_out = daemon_query("cli-search-docs", ranked_payload);
    let bm25_out = daemon_query(
        "cli-search-docs",
        docs_payload(&query_str, limit.max(UNIFIED_LANE_DEPTH), UNIFIED_DOCS_MODE),
    );
    let ranked_results = ranked_out
        .as_ref()
        .ok()
        .map(|s| parse_docs_response(s, "ranked"))
        .unwrap_or_default();
    let bm25_results = bm25_out
        .as_ref()
        .ok()
        .map(|s| parse_docs_response(s, "docs"))
        .unwrap_or_default();

    // 2A (2026-09-13, Graft analysis §29): the "hybrid" backend that used to run
    // here was an EMPTY `InMemoryVectorStore` built per call — it could never
    // return a hit. A semantic backend returns the day a persisted, populated
    // store exists (`SqliteVecStore` is the candidate) and a paraphrase bench
    // shows the gap.
    let all_backends = vec![
        (UNIFIED_RANKED_WEIGHT, ranked_results),
        (UNIFIED_DOCS_WEIGHT, bm25_results),
    ];

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

/// Bring every backend's path into one alphabet so the RRF keys of the same file
/// fuse instead of competing: the daemon's `/project/` root alias, a leading `./`
/// (the wiring tables) and the absolute path of the current workspace all become
/// the workspace-relative form the Tantivy index already uses. Measured 12/09/2026:
/// the three backends reported the same file in three spellings.
fn normalize_path(p: &str) -> String {
    let mut s = p.strip_prefix("/project/").unwrap_or(p);
    while let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    if s.starts_with('/')
        && let Ok(cwd) = std::env::current_dir()
        && let Ok(rel) = std::path::Path::new(s).strip_prefix(&cwd)
    {
        return rel.to_string_lossy().into_owned();
    }
    s.to_string()
}

/// Weighted Reciprocal Rank Fusion: each lane adds `weight / (k + rank)` to a
/// key, where `rank` counts DISTINCT keys in that lane (k = 60). A lane lists a
/// file once per matching symbol; counting every row let a file with many weak
/// symbols outrank a file with one strong answer (measured 13/09/2026), so a
/// lane votes for each file once, at its best rank.
fn rrf_fuse(lanes: Vec<(f32, Vec<BackendResult>)>) -> Vec<SearchResult> {
    use std::collections::{HashMap, HashSet};
    let mut scored: HashMap<String, f32> = HashMap::new();
    for (weight, lane) in &lanes {
        let mut seen: HashSet<String> = HashSet::new();
        for result in lane {
            let key = result.rrf_key();
            if !seen.insert(key.clone()) {
                continue;
            }
            let rank = seen.len() - 1;
            *scored.entry(key).or_insert(0.0) += weight / (RRF_K + rank as f32);
        }
    }
    let mut sorted: Vec<_> = scored.into_iter().collect();
    sorted.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Less)
            .then_with(|| a.0.cmp(&b.0))
    });
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
            file_path: normalize_path(
                obj.get("file_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default(),
            ),
            line: None,
            col: None,
            symbol: obj
                .get("symbol_name")
                .and_then(|v| v.as_str())
                .map(String::from),
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

    // ── path alphabets (12/09/2026) ──────────────────────────────────────────

    #[test]
    fn normalize_path_strips_the_daemon_alias_and_dot_slash() {
        assert_eq!(
            normalize_path("/project/crates/x/src/a.rs"),
            "crates/x/src/a.rs"
        );
        assert_eq!(normalize_path("./crates/x/src/a.rs"), "crates/x/src/a.rs");
        assert_eq!(normalize_path("././crates/x/src/a.rs"), "crates/x/src/a.rs");
        assert_eq!(normalize_path("crates/x/src/a.rs"), "crates/x/src/a.rs");
    }

    #[test]
    fn normalize_path_makes_the_workspace_absolute_form_relative() {
        let cwd = std::env::current_dir().expect("cwd");
        let abs = cwd.join("crates/x/src/a.rs");
        assert_eq!(normalize_path(&abs.to_string_lossy()), "crates/x/src/a.rs");
        assert_eq!(
            normalize_path("/somewhere/else/a.rs"),
            "/somewhere/else/a.rs",
            "a path outside the workspace is left alone"
        );
    }

    #[test]
    fn the_same_file_from_two_backends_fuses_into_one_key() {
        let symbols = parse_docs_response(
            r#"{"results":[{"symbol_name":"run_gateway","file_path":"./crates/x/a.rs"}]}"#,
            "ranked",
        );
        let docs = parse_docs_response(
            r#"{"results":[{"file_path":"/project/crates/x/a.rs","context_value":"fn run_gateway"}]}"#,
            "docs",
        );
        assert_eq!(symbols[0].file_path, docs[0].file_path);
        let fused = rrf_fuse(vec![(1.0, symbols), (1.0, docs)]);
        assert_eq!(fused.len(), 1, "one file, one fused row");
        assert!(
            (fused[0].rrf_score - 2.0 / RRF_K).abs() < 1e-6,
            "both backends contributed"
        );
    }

    #[test]
    fn docs_response_carries_the_symbol_when_the_backend_names_one() {
        let docs = parse_docs_response(
            r#"{"results":[{"file_path":"crates/x/a.rs","context_value":"fn a()","symbol_name":"a"}]}"#,
            "docs",
        );
        assert_eq!(docs[0].symbol.as_deref(), Some("a"));
    }

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
        let fused = rrf_fuse(Vec::new());
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
        let fused = rrf_fuse(vec![(1.0, backend)]);
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
        let fused = rrf_fuse(vec![(1.0, exact), (1.0, fuzzy)]);
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
        let fused = rrf_fuse(vec![(1.0, exact), (1.0, fuzzy)]);
        assert_eq!(fused.len(), 2);
        assert!((fused[0].rrf_score - fused[1].rrf_score).abs() < 0.0001);
    }

    #[test]
    fn test_rrf_k_constant() {
        assert_eq!(RRF_K, 60.0);
    }

    /// `search fuzzy` and `search bm25` differ by the `mode` the daemon reads —
    /// the byte that was missing while both subcommands answered identically.
    #[test]
    fn docs_payload_carries_the_mode_that_tells_fuzzy_from_bm25() {
        let fuzzy = docs_payload("HokRuntime", 5, "fuzzy");
        let bm25 = docs_payload("HokRuntime", 5, "bm25");
        assert_eq!(fuzzy["mode"], "fuzzy");
        assert_eq!(bm25["mode"], "bm25");
        assert_eq!(fuzzy["query"], "HokRuntime");
        assert_eq!(fuzzy["top"], 5);
        assert_ne!(
            fuzzy, bm25,
            "the two subcommands must not send the same bytes"
        );
        assert!(
            matches!(UNIFIED_DOCS_MODE, "bm25" | "fuzzy"),
            "the unified docs backend names a mode the daemon understands"
        );
        assert_eq!(docs_payload("x", 5, TEXT_MODE)["mode"], "text");
        assert_eq!(docs_payload("x", 5, UNIFIED_RANKED_MODE)["mode"], "ranked");
    }

    fn row(file: &str, backend: &str) -> BackendResult {
        BackendResult {
            rank: 0,
            file_path: file.to_string(),
            line: None,
            col: None,
            symbol: None,
            context: None,
            backend: backend.to_string(),
        }
    }

    /// A lane votes for a file once, at its best rank: many weak rows of one file
    /// never outrank one strong row of another.
    #[test]
    fn a_lane_votes_once_per_file_and_weights_scale_the_vote() {
        let noisy = vec![
            row("a.rs", "ranked"),
            row("b.rs", "ranked"),
            row("b.rs", "ranked"),
            row("b.rs", "ranked"),
        ];
        let fused = rrf_fuse(vec![(1.0, noisy)]);
        assert_eq!(
            fused[0].file_path, "a.rs",
            "b.rs repeated three times still ranks second"
        );
        assert!(
            (fused[1].rrf_score - 1.0 / (RRF_K + 1.0)).abs() < 1e-6,
            "b.rs scored once, at distinct rank 1"
        );
        // A 0.05 lane breaks near-ties (neighbours may swap — that is the measured
        // gain) but cannot lift a file from far down the primary lane.
        let primary: Vec<BackendResult> = ["x.rs", "a.rs", "b.rs", "c.rs", "d.rs", "y.rs"]
            .iter()
            .map(|f| row(f, "ranked"))
            .collect();
        let fused = rrf_fuse(vec![(1.0, primary), (0.05, vec![row("y.rs", "docs")])]);
        assert_eq!(
            fused[0].file_path, "x.rs",
            "rank 5 plus a tie-breaker stays below rank 0"
        );
        let near = rrf_fuse(vec![
            (1.0, vec![row("x.rs", "ranked"), row("y.rs", "ranked")]),
            (0.05, vec![row("y.rs", "docs")]),
        ]);
        assert_eq!(
            near[0].file_path, "y.rs",
            "adjacent files: the second lane decides"
        );
    }

    #[test]
    fn parses_text_subcommand() {
        let cli = SearchCli::try_parse_from(["search", "text", "o pipe engole", "--limit", "7"])
            .expect("parse");
        assert!(
            matches!(cli.cmd, Some(SearchCmd::Text { ref query, limit: 7 }) if query == "o pipe engole")
        );
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
        let results = parse_docs_response("not json", "ranked");
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
