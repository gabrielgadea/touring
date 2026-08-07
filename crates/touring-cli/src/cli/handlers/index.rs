//! CLI index and AST handlers — extracted from cli_handlers.rs (lines 1778-2135)
//!
//! This module contains:
//! - **Index handlers** (5): cli_index_status, cli_index_search, cli_index_find,
//!   cli_index_files, cli_index_rebuild
//! - **AST handlers** (11): cli_ast_find, cli_ast_overview, cli_ast_blast,
//!   cli_ast_semantic, cli_ast_quality, cli_ast_calls, cli_ast_heat,
//!   cli_ast_modules, cli_ast_scope, cli_ast_detail, cli_ast_imports
//!
//! The 8 new handlers (semantic/quality/calls/heat/modules/scope/detail/imports)
//! provide deep AST-level analysis using the touring-ast crate APIs.

use crate::runtime::HookRuntime;
use rusqlite::params;
use std::path::Path;
use touring_analysis::e2e::schema_guard;
use touring_code::ast::{
    CallGraph, FileHeat, HeatMap, ImportResolver, Lang, QualityReport, ScopeMap, SymbolDetail,
    SymbolLocation, analyze_quality, build_call_graph, build_scope_map, extract_imports_resolved,
    extract_symbol_details, module_tree::ModuleTree, semantic_search::SemanticSymbolIndex,
    symbols::SymbolKind,
};

// ─────────────────────────────────────────────────────────────────────────────
// Index handlers (5)
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-index-status` — returns symbol store health and statistics.
///
/// Wave 22 (S-Q4a): wrapped in `query_cache` with a global key — the
/// dashboard (`touring status`) calls this repeatedly in hot paths.
/// TTL is 60 s (global cache default); explicit invalidation happens in
/// `cli_index_rebuild` (S-Q4b) so post-rebuild calls always return fresh data.
pub fn cli_index_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let cache_key = crate::shared::query_cache::make_key("cli_index_status", "global");
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    let (symbol_count, file_count) = if let Some(ref store) = rt.infra.symbol_store {
        match store.stats() {
            Ok(stats) => (stats.symbol_count, stats.file_count),
            Err(_) => (0, 0),
        }
    } else {
        (0, 0)
    };

    let out = serde_json::json!({
        "initialized": rt.infra.symbol_store.is_some(),
        "symbol_count": symbol_count,
        "file_count": file_count
    })
    .to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

/// `cli-index-search` — prefix-search symbols in the store.
pub fn cli_index_search(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");

    if query.is_empty() {
        return serde_json::json!({"results": [], "count": 0}).to_string();
    }

    // Wave 18: cache prefix lookup results for 60s — CC discovery
    // (`touring index search Foo`) often re-runs the same prefix.
    let cache_key = crate::shared::query_cache::make_key("cli_index_search", query);
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    let results: Vec<serde_json::Value> = if let Some(ref store) = rt.infra.symbol_store {
        store
            .search_symbols(query)
            .map(|locations| {
                locations
                    .into_iter()
                    .take(20)
                    .map(|loc| {
                        serde_json::json!({
                            "file_path": loc.file_path,
                            "symbol_name": loc.symbol_name,
                            "line": loc.line,
                            "column": loc.column,
                            "is_definition": loc.is_definition,
                            "kind": loc.kind
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    let out = serde_json::json!({
        "query": query,
        "results": results,
        "count": results.len()
    })
    .to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

/// `cli-index-find` — exact-match symbol lookup.
///
/// Wave 17 (2026-04-18): wrapped in `query_cache::get_or_compute` so
/// repeated VGP verifications of the same symbol within a generation
/// pass return in ~1µs instead of round-tripping the symbol store
/// (~50–200µs). 60s TTL keeps results fresh while the daemon may
/// re-index.
pub fn cli_index_find(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol_name = payload
        .get("symbol_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let definitions_only = payload
        .get("definitions_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if symbol_name.is_empty() {
        return serde_json::json!({"definitions": [], "count": 0}).to_string();
    }

    let cache_key = crate::shared::query_cache::make_key(
        "cli_index_find",
        &format!("{symbol_name}|defs={definitions_only}"),
    );
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    // A2/B: `definitions` (`@definition.*`) comes from `find_symbol` (which is
    // definition-only by construction — see store.rs); `references`
    // (`@reference.call`, kind="call") comes from the explicit `find_references`.
    // References are suppressed under `definitions_only` (VGP / collision
    // checks). `count` / `reference_count` are the true totals before output caps.
    fn map_loc(loc: SymbolLocation) -> serde_json::Value {
        serde_json::json!({
            "file_path": loc.file_path,
            "symbol_name": loc.symbol_name,
            "line": loc.line,
            "column": loc.column,
            "is_definition": loc.is_definition,
            "kind": loc.kind
        })
    }

    let store_opt = rt.infra.symbol_store.as_ref();

    let mut definitions: Vec<serde_json::Value> = store_opt
        .and_then(|s| s.find_symbol(symbol_name).ok())
        .unwrap_or_default()
        .into_iter()
        .map(map_loc)
        .collect();
    let def_count = definitions.len();
    definitions.truncate(10);

    let (mut references, symbol_store_ref_count): (Vec<serde_json::Value>, usize) =
        if definitions_only {
            (Vec::new(), 0)
        } else {
            let refs: Vec<serde_json::Value> = store_opt
                .and_then(|s| s.find_references(symbol_name).ok())
                .unwrap_or_default()
                .into_iter()
                .map(map_loc)
                .collect();
            let n = refs.len();
            (refs, n)
        };
    // Augment reference_count with wiring_map consumer count — the symbol store
    // call graph may undercount callers that are tracked only in the wiring layer
    // (cross-module imports). Take the max so the count never goes backwards.
    let wiring_consumer_count: usize = if definitions_only {
        0
    } else {
        rt.ctx
            .knowledge
            .conn_ref()
            .query_row(
                "SELECT COUNT(DISTINCT consumer_file) FROM wiring_map \
                 WHERE symbol_name = ?1 AND consumer_file IS NOT NULL",
                params![symbol_name],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .unwrap_or(0)
    };
    let ref_count = symbol_store_ref_count.max(wiring_consumer_count);
    references.truncate(50);

    let out = serde_json::json!({
        "symbol_name": symbol_name,
        "definitions": definitions,
        "references": references,
        "count": def_count,
        "reference_count": ref_count
    })
    .to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

/// `cli-index-files` — list indexed files (from knowledge DB top-accessed).
pub fn cli_index_files(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let pattern = payload
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Bug 1 fix (2026-05-02): honor payload.limit instead of hardcoded 50.
    // Default 100, max 10_000 to bound memory. Pool oversized for pattern filtering.
    let limit = payload
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n.min(10_000) as usize)
        .unwrap_or(100);
    let pool_size = std::cmp::max(limit.saturating_mul(4), 1000);

    let files: Vec<String> = rt
        .ctx
        .knowledge
        .top_accessed_files(pool_size)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| pattern.is_empty() || f.contains(pattern))
        .take(limit)
        .collect();

    serde_json::json!({
        "pattern": pattern,
        "limit": limit,
        "files": files,
        "count": files.len()
    })
    .to_string()
}

/// Bug 4 fix (2026-05-02): re-entrance guard for index rebuild.
/// Concurrent rebuild attempts are rejected with a structured error instead of
/// causing daemon-level lock contention / crash on large workspaces.
static REBUILD_IN_PROGRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// P-H — emit Go package-aware wiring rows for one `.go` source file during a
/// rebuild, keyed by the file's package import-path (`go:<import-path>`) rather
/// than its `.go` file path. Producers are the file's exported top-level
/// declarations; consumers are `alias.Symbol` selectors resolved to the imported
/// package's key. Returns the number of producer rows registered (for the
/// `wiring_entries` tally).
///
/// The generic per-file loop keys Go producers by the `.go` path, which the
/// storage gate rejects (only `go:` keys wire for Go), and the Go arm of
/// `resolve_import_path_with_source` returns `None` — so without this pass Go
/// contributes nothing. This is the *package-aware* half that makes Go wire.
///
/// No-op for `_test.go` files (test code is not the package's public API) and
/// files with no derivable `go.mod` import-path. Inert unless
/// `TOURING_POLYGLOT_WIRING` is set — the storage gate rejects `go:` keys
/// otherwise — so default daemon behavior is byte-identical.
fn feed_go_package_wiring(
    rt: &HookRuntime,
    rel_path: &str,
    abs_path_str: &str,
    content: &str,
    cleared_pkgs: &mut std::collections::HashSet<String>,
) -> u32 {
    // Zero-overhead when the opt-in is off: skip extraction entirely (the
    // storage gate would reject every `go:` row anyway) so both DB state AND the
    // reported `wiring_entries` tally stay byte-identical to pre-polyglot runs.
    // Single source of truth for the flag (same process `OnceLock`).
    if !touring_hooks_core::knowledge_wiring::polyglot_wiring_enabled() {
        return 0;
    }
    if rel_path.ends_with("_test.go") {
        return 0;
    }
    let Some(pkg_key) = touring_code::ast::go_wiring::go_package_key_for_file(abs_path_str) else {
        return 0;
    };
    // Clear the package's producer rows exactly once per pass. A per-file clear
    // would wipe exports a sibling `.go` file already registered under the same
    // package key; tracking cleared keys keeps a partial `--dir` reindex from
    // wiping packages it never re-walks. Consumer rows are keyed by the `.go`
    // consumer file and already cleared by the generic `clear_consumer_entries`.
    if cleared_pkgs.insert(pkg_key.clone()) {
        let _ = rt.ctx.knowledge.clear_wiring(&pkg_key);
    }
    let mut registered = 0u32;
    for export in touring_code::ast::go_wiring::extract_go_exports(content) {
        let _ = rt
            .ctx
            .knowledge
            .register_pub_symbol(&pkg_key, &export.name, export.kind, "public");
        registered += 1;
    }
    for edge in touring_code::ast::go_wiring::extract_go_consumer_edges(content) {
        let _ = rt
            .ctx
            .knowledge
            .record_consumer(&edge.package_key, &edge.symbol, rel_path, None);
    }
    registered
}

/// `cli-index-rebuild` — walk the project root and symbol-index all supported files.
pub fn cli_index_rebuild(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use std::sync::atomic::Ordering;
    // Bug 4: prevent concurrent rebuilds (would cause daemon crash on large workspaces).
    if REBUILD_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return serde_json::json!({
            "error": "rebuild already in progress",
            "hint": "wait for current rebuild to finish before retrying",
            "files_indexed": 0
        })
        .to_string();
    }
    // RAII drop guard so panic / early-return always clears the flag.
    struct ResetGuard;
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            REBUILD_IN_PROGRESS.store(false, Ordering::SeqCst);
        }
    }
    let _reset = ResetGuard;

    let root = if let Some(dir) = payload.get("dir").and_then(|v| v.as_str()) {
        std::path::PathBuf::from(dir)
    } else {
        rt.project_root.clone()
    };

    const SUPPORTED_EXTS: &[&str] = &[
        "rs", "py", "pyi", "ts", "tsx", "js", "jsx", "mjs", "cjs", "sh", "bash", "html", "css",
        "scss", "md", "mdx", "json", "toml", "yaml", "yml",
    ];
    const SKIP_DIRS: &[&str] = &[
        // Build / dependency artifacts
        "target",
        ".git",
        "node_modules",
        ".cargo",
        ".venv",
        "venv",
        "__pycache__",
        "dist",
        "build",
        ".tox",
        ".mypy_cache",
        ".pytest_cache",
        ".ruff_cache",
        ".eggs",
        ".nox",
        ".cache",
        // Data / output directories (avoid walking huge data lakes)
        "data",
        "datasets",
        "dataset",
        "raw_data",
        "processed_data",
        "downloaded_files",
        "downloads",
        "uploads",
        "attachments",
        "coverage",
        "coverage_html",
        "htmlcov",
        "lcov-report",
        "migrations",
        "generated",
        "benchmarks",
        "tmp",
        "temp",
        "logs",
        "log",
        // Touring daemon state — JSON checkpoint files are NOT code
        // (knowledge_sync_*.json would be parsed as modules with fake symbols)
        "checkpoints",
    ];

    // Non-touring subprojects to skip entirely (same filter as post_read + cli_e2e)
    const SKIP_SUBPROJECTS: &[&str] = &[
        "agent-harness",
        "holon-wasm-components",
        "holon-wasm-runner",
        "pln2",
    ];

    let mut files_indexed: u32 = 0;
    let mut symbols_added: u32 = 0;
    let mut wiring_entries: u32 = 0;
    let mut errors: u32 = 0;
    // Wave 2026-05-14 — root-cause fix for the "rebuild is additive only"
    // gotcha that forced manual SQL purges after every `rm -rf crates/X`.
    // `walked_rel_paths` captures every file successfully processed in the
    // main loop so the post-loop sweep can compute
    // `(db_files - walked_files) = stale`. See sweep block after the loop.
    let mut walked_rel_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stale_files_purged: u32 = 0;
    // P-H (Go package-aware wiring): Go producers key by `go:<import-path>`, so
    // many `.go` files map to one package key. Producer rows must be cleared
    // once per package (not per file — a per-file clear of the shared key would
    // wipe the exports registered by a sibling file). This set tracks the
    // package keys already cleared in THIS pass, so a partial `--dir` reindex
    // only clears the packages it actually re-walks (never a global wipe).
    let mut cleared_go_pkgs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stale_paths_sample: Vec<String> = Vec::new();
    let project_root = rt.project_root.clone();
    let t0 = std::time::Instant::now();

    fn should_skip_dir(name: &str, skip_dirs: &[&str]) -> bool {
        // Exact match
        if skip_dirs.contains(&name) {
            return true;
        }
        // Prefix match for venv variants (.venv-test, .venv.bak, venv_cipher, etc.)
        if name.starts_with(".venv") || name.starts_with("venv") {
            return true;
        }
        // Hidden directories starting with .
        if name.starts_with('.') {
            return true;
        }
        false
    }

    fn walk(
        dir: &std::path::Path,
        skip_dirs: &[&str],
        skip_subprojects: &[&str],
        supported_exts: &[&str],
        acc: &mut Vec<std::path::PathBuf>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if !should_skip_dir(name, skip_dirs) {
                    // Skip entire non-touring subproject directories
                    if !skip_subprojects.contains(&name) {
                        walk(&path, skip_dirs, skip_subprojects, supported_exts, acc);
                    }
                }
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if supported_exts.contains(&ext) {
                    acc.push(path);
                }
            }
        }
    }

    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    walk(
        &root,
        SKIP_DIRS,
        SKIP_SUBPROJECTS,
        SUPPORTED_EXTS,
        &mut paths,
    );

    // OOM-prevention gate (Wave 2026-05-12).
    //
    // Previously this loop processed every file in `paths` without checking
    // process RSS, which caused daemon SIGKILL by the kernel OOM-killer on
    // large workspaces (2517+ files / 58k+ symbols) when system swap was
    // already saturated. We now snapshot RSS every CHUNK_SIZE files and:
    //   * SOFT threshold → sleep 50 ms (let the allocator/kernel reclaim)
    //   * HARD threshold → break out gracefully, return partial result,
    //     bump `memory_pressure_red_count` so operators see the signal in
    //     `touring gate-metrics -j`.
    //
    // The chunked check is `chunk_idx % CHUNK_SIZE == 0` rather than a real
    // `paths.chunks()` envelope to avoid re-indenting the existing loop body.
    const CHUNK_SIZE: usize = 100;
    const MEMORY_SOFT_MB: f64 = 2000.0;
    const MEMORY_HARD_MB: f64 = 3000.0;
    let mut max_rss_mb: f64 = 0.0;
    let mut aborted_memory_pressure: bool = false;

    for (chunk_idx, path) in paths.iter().enumerate() {
        // RSS probe at chunk boundaries (incl. the very first iteration).
        if chunk_idx % CHUNK_SIZE == 0 {
            let mem = crate::shared::memory_stats_probe::snapshot();
            if mem.physical_mb > max_rss_mb {
                max_rss_mb = mem.physical_mb;
            }
            if mem.physical_mb > MEMORY_HARD_MB {
                crate::shared::gate_metrics::record_pressure_red();
                aborted_memory_pressure = true;
                break;
            }
            if mem.physical_mb > MEMORY_SOFT_MB {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }

        let rel_path = crate::runtime::make_relative(path.to_str().unwrap_or(""), &project_root);
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        match rt.process_file(&rel_path, &content) {
            Ok(result) => {
                let sym_count = result.symbols_added.len() as u32;
                if let Some(store) = rt.symbol_store() {
                    // B (2026-06-21): polyglot `@reference.call`. Persists
                    // call-sites as `is_definition=false` rows (`kind="call"`)
                    // so `index find` doubles as find-references (GitHub-semantic
                    // tags model). DEFAULT ON (Gabriel 2026-06-21) — disable
                    // per-project with `TOURING_INDEX_REFERENCES=0`. Cost measured
                    // on the touring workspace: ~2.2× rows / +61% DB / +12%
                    // reindex. Safe by construction: references are invisible to
                    // every definition query (`find_symbol`/`search_symbols`/
                    // `load_into_index` filter `is_definition=1`); only the
                    // explicit `find_references` returns them. `build_call_graph`
                    // is polyglot (Rust/Python/TS/JS); other langs yield no sites.
                    if std::env::var("TOURING_INDEX_REFERENCES")
                        .map(|v| v != "0" && v != "false")
                        .unwrap_or(true)
                    {
                        let mut combined = result.symbols_added.clone();
                        if let Some(lang) = Lang::from_path(Path::new(&rel_path)) {
                            for cs in build_call_graph(&content, lang).sites {
                                combined.push(
                                    SymbolLocation::new(
                                        rel_path.as_str(),
                                        cs.callee,
                                        cs.line,
                                        0,
                                        false,
                                    )
                                    .with_kind(Some("call".to_string())),
                                );
                            }
                        }
                        let _ = store.replace_file_symbols(&rel_path, &combined);
                    } else {
                        let _ = store.replace_file_symbols(&rel_path, &result.symbols_added);
                    }
                }
                files_indexed += 1;
                symbols_added += sym_count;
                // Wave 2026-05-14: record this path so the post-loop sweep
                // can tell it apart from files that vanished from disk.
                walked_rel_paths.insert(rel_path.clone());

                let abs_path_str = path.to_str().unwrap_or("");
                if let Some(symbols) =
                    crate::ast_bridge::extract_enriched_symbols(&content, abs_path_str)
                {
                    let language =
                        crate::shared::detect_language::detect_language_or_unknown(&rel_path);
                    let is_code_lang = !matches!(
                        language,
                        "toml" | "json" | "yaml" | "markdown" | "html" | "css"
                    );

                    // ALWAYS clear old wiring entries so we start fresh for this file.
                    // This prevents stale entries from accumulating across rebuilds.
                    let _ = rt.ctx.knowledge.clear_wiring(&rel_path);
                    let _ = rt.ctx.knowledge.clear_consumer_entries(&rel_path);

                    if is_code_lang {
                        for sym in &symbols {
                            if sym.is_public {
                                let kind_str = sym.kind.as_str();
                                let _ = rt
                                    .ctx
                                    .knowledge
                                    .register_pub_symbol(&rel_path, &sym.name, kind_str, "public");
                                wiring_entries += 1;
                            }
                        }

                        let imports =
                            crate::ast_bridge::extract_file_imports(&content, abs_path_str);
                        for (module_path, imported_symbols) in &imports {
                            if let Some(module_file) =
                                touring_hooks_core::symbol_extractors::resolve_import_path_with_source(
                                    module_path,
                                    language,
                                    Some(abs_path_str),
                                )
                            {
                                for symbol_name in imported_symbols {
                                    let _ = rt.ctx.knowledge.record_consumer(
                                        &module_file,
                                        symbol_name,
                                        &rel_path,
                                        None,
                                    );
                                }
                            }
                        }

                        // F9 (2026-05-11): record dynamic-dispatch consumers.
                        // `.method()` and `Type::assoc_fn()` are invisible to
                        // `use`-statement scraping above — those produced ~2925
                        // false-positive orphans (43% of orphan_count). Walk the
                        // AST for call expressions, look up which producer rows
                        // match by symbol_name, and record this file as their
                        // consumer (capped to 4 producers per name to prevent
                        // fan-out blow-up on generic names like `clone`).
                        let method_names =
                            crate::ast_bridge::extract_file_method_calls(&content, abs_path_str);
                        if !method_names.is_empty()
                            && let Ok(producers) = rt
                                .ctx
                                .knowledge
                                .find_producer_modules_for_methods(&method_names, 4)
                        {
                            for (module_file, symbol_name) in &producers {
                                let _ = rt.ctx.knowledge.record_consumer(
                                    module_file,
                                    symbol_name,
                                    &rel_path,
                                    None,
                                );
                            }
                        }

                        // P-H: Go package-aware wiring. Go producers key by the
                        // package import-path (`go:<path>`), not the `.go` file
                        // (which the generic loop above registered — and the
                        // storage gate rejected), and consumers key by selector
                        // (`pkg.Foo()`), not `use`-import. Inert unless
                        // `TOURING_POLYGLOT_WIRING` is set (gate rejects `go:`).
                        if language == "go" {
                            wiring_entries += feed_go_package_wiring(
                                rt,
                                &rel_path,
                                abs_path_str,
                                &content,
                                &mut cleared_go_pkgs,
                            );
                        }
                    }
                }
            }
            Err(_) => {
                errors += 1;
            }
        }
    }

    // ── Wave 2026-05-14: sweep stale entries ───────────────────────────
    // Historically `cli_index_rebuild` was ADDITIVE-only: it called
    // `replace_file_symbols` for every walked file, but never deleted
    // entries for files that had been removed from disk (e.g. after a
    // `rm -rf crates/foo` refactor). The result was an ever-growing
    // pool of stale symbols + wiring rows that produced phantom orphans,
    // ghost-cycles, and incorrect cross-caller analyses — forcing
    // operators to issue manual `DELETE FROM symbols WHERE file_path
    // LIKE ...` against `symbols.db` after every absorption wave.
    //
    // This sweep closes the loop: for every file_path in the DB that
    // is NOT in `walked_rel_paths` AND whose absolute path is missing
    // on disk, we purge symbols + wiring + consumer + tantivy rows.
    //
    // SAFETY: skip the sweep when the walk aborted early due to memory
    // pressure (`aborted_memory_pressure`). Without a complete walk,
    // `walked_rel_paths` is a strict subset of "should exist", and we
    // would catastrophically delete legitimate entries.
    //
    // DEFENSE-IN-DEPTH: even when not aborted, we double-check the
    // absolute path with `Path::exists()` before purging — covers the
    // case where a single file failed to be read but is still on disk.
    if !aborted_memory_pressure
        && let Some(store) = rt.symbol_store()
        && let Ok(db_files) = store.get_indexed_files(usize::MAX)
    {
        for db_path in &db_files {
            if walked_rel_paths.contains(db_path) {
                continue;
            }
            // Belt-and-suspenders existence check.
            let abs_path = project_root.join(db_path);
            if abs_path.exists() {
                continue;
            }
            let _ = store.remove_file(db_path);
            let _ = rt.ctx.knowledge.clear_wiring(db_path);
            let _ = rt.ctx.knowledge.clear_consumer_entries(db_path);
            #[cfg(feature = "tantivy-fts")]
            if let Some(tantivy_idx) = crate::tantivy_index::tantivy_for(Some(&rt.project_root)) {
                let _ = tantivy_idx.delete_by_file(db_path);
            }
            if stale_paths_sample.len() < 5 {
                stale_paths_sample.push(db_path.clone());
            }
            stale_files_purged += 1;
        }
    }

    // Wave H+1 (2026-06-11): re-resolve consumer rows frozen at
    // symbol_kind='unknown' — walk-order races (consumer indexed before its
    // producer) and facade re-export imports both leave recoverable rows;
    // this closes the doctor wiring_diagnostic pollution warning.
    let kinds_backfilled = rt
        .ctx
        .knowledge
        .backfill_unknown_consumer_kinds()
        .unwrap_or(0);
    if kinds_backfilled > 0 {
        tracing::info!(kinds_backfilled, "wiring_map unknown-kind backfill");
    }

    // Wave 22 (S-Q4b): invalidate cli_index_status cache after rebuild so
    // the next `touring index status` / `touring status` call returns fresh counts.
    crate::shared::query_cache::invalidate(&crate::shared::query_cache::make_key(
        "cli_index_status",
        "global",
    ));

    let abort_hint = if aborted_memory_pressure {
        Some(format!(
            "daemon RSS {} MB exceeded {} MB hard threshold; rebuild stopped at {}/{} files. \
             Hint: free system memory (close rust-analyzer, IDEs) or rebuild incrementally \
             via `touring index ingest <file>`.",
            max_rss_mb as u64,
            MEMORY_HARD_MB as u64,
            files_indexed,
            paths.len()
        ))
    } else {
        None
    };

    serde_json::json!({
        "files_indexed": files_indexed,
        "symbols_added": symbols_added,
        "wiring_entries": wiring_entries,
        "errors": errors,
        "total_files_scanned": paths.len(),
        "duration_ms": t0.elapsed().as_millis(),
        // Wave 2026-05-12: OOM-prevention telemetry.
        "max_rss_mb": max_rss_mb as u64,
        "aborted_memory_pressure": aborted_memory_pressure,
        "abort_hint": abort_hint,
        // Wave 2026-05-14: sweep telemetry — catches `rm -rf crates/X`
        // regressions automatically. Operators can grep this field in
        // CI to assert the workspace is "clean" after a refactor.
        "stale_files_purged": stale_files_purged,
        "stale_paths_sample": stale_paths_sample,
    })
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// AST handlers (3 existing + 8 new = 11 total)
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-ast-find` — AST-level symbol lookup with optional file filter.
pub fn cli_ast_find(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol_name = payload
        .get("symbol_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let definitions_only = payload
        .get("definitions_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let file_path = payload.get("file_path").and_then(|v| v.as_str());

    if symbol_name.is_empty() {
        return serde_json::json!({"definitions": [], "count": 0}).to_string();
    }

    let results: Vec<serde_json::Value> = if let Some(ref store) = rt.infra.symbol_store {
        store
            .find_symbol(symbol_name)
            .map(|locations| {
                locations
                    .into_iter()
                    .filter(|loc| {
                        let file_match =
                            file_path.map(|f| loc.file_path.contains(f)).unwrap_or(true);
                        let def_match = !definitions_only || loc.is_definition;
                        file_match && def_match
                    })
                    .take(10)
                    .map(|loc| {
                        serde_json::json!({
                            "file_path": loc.file_path,
                            "symbol_name": loc.symbol_name,
                            "line": loc.line,
                            "column": loc.column,
                            "is_definition": loc.is_definition,
                            "kind": loc.kind
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    serde_json::json!({
        "symbol_name": symbol_name,
        "definitions": results,
        "count": results.len()
    })
    .to_string()
}

/// `cli-ast-overview` — list all symbols defined in a file.
///
/// Wave 22 (S-Q3): wrapped in `query_cache` — mirrors the existing pattern
/// used by `cli_ast_blast` (line ~427). Path-scoped invalidation via
/// `invalidate_by_path` in `post_edit`/`post_write` ensures freshness after edits.
///
/// Bug 3 fix (2026-05-02): now emits `language` + symbol `kind`/`name` (was null).
/// Tries to read the file and use `extract_enriched_symbols` (provides kind).
/// Falls back to symbol_store SymbolLocation (kind=null) if file unreadable.
pub fn cli_ast_overview(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let cache_key = crate::shared::query_cache::make_key("cli_ast_overview", file_path);
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    // Bug 3 fix: detect language from file extension (was null in JSON).
    let language = crate::shared::detect_language::detect_language_or_unknown(file_path);

    // Resolve absolute path to read file content for enriched extraction.
    let abs_path: std::path::PathBuf = if std::path::Path::new(file_path).is_absolute() {
        std::path::PathBuf::from(file_path)
    } else {
        rt.project_root.join(file_path)
    };

    // Primary path: read source + extract enriched symbols (have kind + is_public).
    // Fallback: bare SymbolLocation from symbol_store (kind/is_public unavailable).
    let symbols: Vec<serde_json::Value> = match std::fs::read_to_string(&abs_path) {
        Ok(content) => match crate::ast_bridge::extract_enriched_symbols(&content, file_path) {
            Some(enriched) if !enriched.is_empty() => enriched
                .into_iter()
                .map(|sym| {
                    serde_json::json!({
                        "symbol_name": sym.name.clone(),
                        "name": sym.name,
                        "kind": sym.kind.as_str(),
                        "line": sym.line,
                        "column": sym.column,
                        "is_definition": true,
                        "is_public": sym.is_public,
                    })
                })
                .collect(),
            _ => fallback_overview_from_store(rt, file_path),
        },
        Err(_) => fallback_overview_from_store(rt, file_path),
    };

    let out = serde_json::json!({
        "file_path": file_path,
        "language": language,
        "symbols": symbols,
        "symbol_count": symbols.len()
    })
    .to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

/// Fallback: query symbol_store for SymbolLocation (no kind/is_public available).
fn fallback_overview_from_store(rt: &HookRuntime, file_path: &str) -> Vec<serde_json::Value> {
    if let Some(ref store) = rt.infra.symbol_store {
        store
            .find_symbols_in_file(file_path)
            .map(|locations| {
                locations
                    .into_iter()
                    .map(|loc| {
                        serde_json::json!({
                            "symbol_name": loc.symbol_name.clone(),
                            "name": loc.symbol_name,
                            "kind": loc.kind.clone(),
                            "line": loc.line,
                            "column": loc.column,
                            "is_definition": loc.is_definition,
                            "is_public": serde_json::Value::Null,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// `cli-ast-blast` — compute blast radius (number of consumers) for a file.
///
/// Wave 18: cached for 60s — pre_edit calls this on every L3+ edit
/// (regra de ouro "blast radius first"). path-scoped invalidation in
/// post_edit ensures freshness after the edit completes.
pub fn cli_ast_blast(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let cache_key = crate::shared::query_cache::make_key("cli_ast_blast", file_path);
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    let db = &rt.ctx.knowledge;

    let consumers: Vec<String> = {
        let mut stmt = match db.conn_ref().prepare(&format!(
            "SELECT DISTINCT consumer_file FROM {} WHERE module_file = ?1 AND consumer_file IS NOT NULL",
            schema_guard::TABLE_WIRING_MAP
        )) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("ast blast prepare failed: {}", e);
                return serde_json::json!({
                    "file_path": file_path,
                    "blast_radius": 0,
                    "consumers": [],
                    "error": format!("{}", e)
                })
                .to_string();
            }
        };
        stmt.query_map(params![file_path], |r| r.get::<_, String>(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };

    let out = serde_json::json!({
        "file_path": file_path,
        "blast_radius": consumers.len(),
        "consumers": consumers
    })
    .to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// 8 NEW AST handlers (semantic / quality / calls / heat / modules / scope / detail / imports)
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-ast-semantic` — find semantically similar symbols using cosine similarity.
/// Uses touring_code::ast::SemanticSymbolIndex with 16-dim feature vectors.
pub fn cli_ast_semantic(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let threshold = payload
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    if query.is_empty() {
        return serde_json::json!({"error": "query is required"}).to_string();
    }

    let index = SemanticSymbolIndex::new(2048);

    // Build query symbol
    let query_sym = touring_code::ast::symbols::Symbol::new(
        query,
        SymbolKind::Function,
        0,
        1,
        0,
        0,
        0,
        query,
        true,
    );

    let results = index.find_similar_symbols(&query_sym, threshold as f64, limit);
    let count = results.len();

    // S-25: RL reward, weighted by how much of the requested limit was
    // actually matched — a constant carries no signal to learn from.
    let reward = crate::cli::shared::retrieval_coverage_reward(count, limit);
    rt.learning
        .inject_reward("cli-ast-semantic", reward, "similarity_coverage");

    serde_json::json!({
        "query": query,
        "threshold": threshold,
        "results": results,
        "count": count
    })
    .to_string()
}

/// `cli-ast-quality` — analyze code quality (anti-patterns, complexity, severity).
/// Uses touring_code::ast::analyze_quality.
pub fn cli_ast_quality(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let lang = Lang::from_path(Path::new(&file_path)).unwrap_or(Lang::Rust);
    let report: QualityReport = analyze_quality(&source, lang);

    // S-25: RL reward = the quality actually measured, not a constant. This
    // call site is the luckiest of the five: it already computed a real scalar
    // in [0, 1] and was throwing it away in favour of a literal `1.0`.
    let reward = f64::from(report.overall_score).clamp(0.0, 1.0);
    rt.learning
        .inject_reward("cli-ast-quality", reward, "measured_overall_score");

    serde_json::json!({
        "file_path": file_path,
        "language": lang.as_str(),
        "report": {
            "overall_score": report.overall_score,
            "complexity_score": report.complexity_score,
            "antipattern_score": report.antipattern_score,
            "max_complexity": report.max_complexity,
            "avg_complexity": report.avg_complexity,
            "complex_symbols": report.complex_symbols,
            "antipatterns": report.antipatterns,
        }
    })
    .to_string()
}

/// `cli-ast-calls` — build and query the call graph for a file.
/// Uses touring_code::ast::build_call_graph.
pub fn cli_ast_calls(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol = payload.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let lang = Lang::from_path(Path::new(&file_path)).unwrap_or(Lang::Rust);
    let graph: CallGraph = build_call_graph(&source, lang);

    // If a symbol is specified, return callers/callees for that symbol
    let callers: Vec<String> = if !symbol.is_empty() {
        graph
            .callers_of(symbol)
            .into_iter()
            .map(|c| c.caller.clone())
            .collect()
    } else {
        vec![]
    };
    let callees: Vec<String> = if !symbol.is_empty() {
        graph
            .callees_of(symbol)
            .into_iter()
            .map(|c| c.callee.clone())
            .collect()
    } else {
        vec![]
    };

    serde_json::json!({
        "file_path": file_path,
        "symbol": symbol,
        "callers": callers,
        "callees": callees,
        "total_calls": graph.sites.len()
    })
    .to_string()
}

/// `cli-ast-heat` — compute file heat score (edits * recency_decay * blast_radius).
/// Uses touring_code::ast::{HeatMap, FileHeat}.
pub fn cli_ast_heat(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    // Use knowledge DB to get heat data
    let db = &rt.ctx.knowledge;
    let edits: u32 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE module_file = ?1",
                schema_guard::TABLE_WIRING_MAP
            ),
            params![file_path],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let access_count: u32 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            params![file_path],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let last_edit_epoch: f64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT last_edit FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            params![file_path],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let blast_radius_weight: f64 = {
        let consumers: i64 = db
            .conn_ref()
            .query_row(
                &format!(
                    "SELECT COUNT(DISTINCT consumer_file) FROM {} WHERE module_file = ?1",
                    schema_guard::TABLE_WIRING_MAP
                ),
                params![file_path],
                |r| r.get(0),
            )
            .unwrap_or(0);
        (consumers as f64) * 0.1
    };

    let file_heat =
        FileHeat::from_existing(edits, last_edit_epoch, access_count, blast_radius_weight);

    let now = chrono::Utc::now().timestamp() as f64;
    let heat_score = file_heat.heat_score(now, HeatMap::DEFAULT_HALF_LIFE_SECS);

    serde_json::json!({
        "file_path": file_path,
        "heat_score": heat_score,
        "edits": edits,
        "access_count": access_count,
        "last_edit_epoch": last_edit_epoch,
        "blast_radius_weight": blast_radius_weight
    })
    .to_string()
}

/// `cli-ast-modules` — build module tree for a directory.
/// Uses touring_code::ast::ModuleTree.
pub fn cli_ast_modules(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let dir = payload.get("dir").and_then(|v| v.as_str()).unwrap_or("");

    if dir.is_empty() {
        return serde_json::json!({"error": "dir required"}).to_string();
    }

    let root_path = Path::new(&dir);
    if !root_path.is_dir() {
        return serde_json::json!({"error": "dir does not exist or is not a directory"})
            .to_string();
    }

    let mut module_nodes: Vec<serde_json::Value> = Vec::new();

    fn walk_dir(dir: &Path, acc: &mut Vec<serde_json::Value>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_dir(&path, acc);
            } else if path.extension().map(|e| e == "rs").unwrap_or(false)
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                let tree = ModuleTree::build_from_source(&content, &filename);
                acc.push(serde_json::json!({
                    "file": filename,
                    "path": path.to_string_lossy(),
                    "root": tree.root.name,
                    "children": tree.root.children.len()
                }));
            }
        }
    }

    walk_dir(root_path, &mut module_nodes);

    serde_json::json!({
        "dir": dir,
        "modules": module_nodes,
        "count": module_nodes.len()
    })
    .to_string()
}

/// `cli-ast-scope` — build scope map (local variable definitions) for a file.
/// Uses touring_code::ast::build_scope_map.
pub fn cli_ast_scope(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let lang = Lang::from_path(Path::new(&file_path)).unwrap_or(Lang::Rust);
    let scope_map: ScopeMap = build_scope_map(&source, lang);

    let entries: Vec<serde_json::Value> = scope_map
        .entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "name": e.name,
                "type": e.type_str,
                "kind": format!("{:?}", e.kind),
                "line": e.line
            })
        })
        .collect();

    serde_json::json!({
        "file_path": file_path,
        "entries": entries,
        "count": entries.len()
    })
    .to_string()
}

/// `cli-ast-detail` — extract detailed information about a symbol (fields, methods, variants).
/// Uses touring_code::ast::extract_symbol_details.
pub fn cli_ast_detail(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol = payload.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if symbol.is_empty() {
        return serde_json::json!({"error": "symbol is required"}).to_string();
    }
    if file_path.is_empty() {
        return serde_json::json!({"error": "file_path required"}).to_string();
    }

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let details: Vec<SymbolDetail> = extract_symbol_details(&source, symbol);

    serde_json::json!({
        "symbol": symbol,
        "file_path": file_path,
        "details": details,
        "count": details.len()
    })
    .to_string()
}

/// `cli-ast-imports` — extract and resolve imports for a file.
/// Uses touring_code::ast::extract_imports_resolved.
pub fn cli_ast_imports(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let lang = Lang::from_path(Path::new(&file_path)).unwrap_or(Lang::Rust);
    let resolver: ImportResolver = extract_imports_resolved(&source, lang);

    let imports: Vec<serde_json::Value> = resolver
        .imports
        .iter()
        .map(|i| {
            serde_json::json!({
                "path": i.path,
                "alias": i.alias,
                "is_glob": i.is_glob,
                "line": i.line
            })
        })
        .collect();

    serde_json::json!({
        "file_path": file_path,
        "language": lang.as_str(),
        "imports": imports,
        "count": imports.len()
    })
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// `cli-index-ingest` — single-file incremental reindex (B3, 2026-05-10)
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-index-ingest` — reindex one file on demand.
///
/// Public, on-demand counterpart to the `post_edit`/`post_write` reindex pass.
/// Use when:
///
/// - The hook reindex silently failed in a prior edit
///   (`reindex_failure_count > 0` in `touring gate-metrics`).
/// - You want to refresh a single file without paying for a 30 s full
///   `touring index rebuild`.
/// - Automation needs a deterministic, scriptable single-file reindex.
///
/// Internally delegates to [`crate::shared::reindex::reindex_file_with_old`]
/// with `old_content = None`, which forces the O(file) full-parse path
/// (the O(edit_region) fast path requires the old content captured at the
/// edit boundary, which we don't have here).
///
/// # Payload
///
/// ```json
/// { "path": "crates/touring-core/src/lib.rs" }   // relative to project_root, OR
/// { "path": "/absolute/path/to/file.rs" }        // absolute path
/// ```
///
/// # Response
///
/// ```json
/// // Success:
/// { "status": "ok", "file_path": "<rel>", "abs_path": "<abs>" }
///
/// // Missing/empty path argument:
/// { "error": "missing required field 'path'", "hint": "usage: touring index ingest <file>" }
///
/// // File does not exist on disk:
/// { "status": "error", "file_path": "<rel>", "error": "file not found" }
///
/// // Reindex pipeline failed (also bumps `reindex_failure_count`):
/// { "status": "error", "file_path": "<rel>", "error": "<reason>" }
/// ```
pub fn cli_index_ingest(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let path_arg = match payload.get("path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => {
            return serde_json::json!({
                "error": "missing required field 'path'",
                "hint": "usage: touring index ingest <file>"
            })
            .to_string();
        }
    };

    // Resolve abs/rel against the runtime's project_root.
    // Mirrors `reindex_file_with_old`'s own resolution, but we precompute
    // both so we can return them in the response.
    let abs_path = if Path::new(&path_arg).is_absolute() {
        path_arg.clone()
    } else {
        rt.project_root
            .join(&path_arg)
            .to_string_lossy()
            .to_string()
    };

    let rel_path = Path::new(&abs_path)
        .strip_prefix(&rt.project_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path_arg.clone());

    if !Path::new(&abs_path).exists() {
        return serde_json::json!({
            "status": "error",
            "file_path": rel_path,
            "error": "file not found",
            "abs_path": abs_path,
        })
        .to_string();
    }

    match crate::shared::reindex::reindex_file_with_old(rt, &abs_path, &rel_path, None) {
        Ok(()) => serde_json::json!({
            "status": "ok",
            "file_path": rel_path,
            "abs_path": abs_path,
        })
        .to_string(),
        Err(e) => {
            // Mirror post_edit's observability: bump the failure counter so
            // `touring gate-metrics` reflects manual ingest attempts that fail.
            crate::shared::gate_metrics::record_reindex_failure();
            serde_json::json!({
                "status": "error",
                "file_path": rel_path,
                "abs_path": abs_path,
                "error": e.to_string(),
            })
            .to_string()
        }
    }
}
