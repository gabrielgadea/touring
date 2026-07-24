//! CLI knowledge/metadata handlers (`cli_metadata_backfill`, `cli_session_summary`, `cli_bench_run`, `cli_file_knowledge_extended`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Metadata backfill, session summary, benchmark recording, and the extended
//! 23-field file-knowledge view. Uses fully-qualified `crate::health_delta::*`
//! and `crate::shared::*` paths; the shared `normalize_to_relative` helper and
//! the one-shot `FK_EXTENDED_DDL_DONE` DDL guard stay in cli_handlers.rs and are
//! imported.

use crate::cli_handlers::{FK_EXTENDED_DDL_DONE, normalize_to_relative};
use crate::runtime::HookRuntime;
use rusqlite::params;
use touring_analysis::e2e::schema_guard;

/// Populate `file_knowledge` for every supported source file in the project.
///
/// Walks `rt.project_root` recursively (same skip_dirs/extension policy as
/// `cli_index_rebuild`) and calls [`crate::shared::reindex::reindex_file`]
/// per file — which upserts into `file_knowledge`, `file_blake3_registry`,
/// `file_feature_flags`, `file_todos`, plus wiring and relations.
///
/// Previous versions of this handler only read the top-level directory and
/// wrote a `'pending'` row into `file_blake3_registry`, leaving
/// `file_knowledge` empty and starving `touring ast meta`, the pre_edit
/// size/complexity gates, and the gotcha matcher of ground truth. That bug
/// allowed critical files to grow unbounded (e.g. `lifecycle.rs` → 22k LOC)
/// because no quality gate could observe them.
///
/// Files already present in `file_knowledge` are skipped unless
/// `"force": true` is set in the payload.
///
/// Payload: `{"force": false}` — when true, reindex every file regardless
/// of existing `file_knowledge` row.
pub fn cli_metadata_backfill(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let force = payload
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    const SUPPORTED_EXTS: &[&str] = &[
        "rs", "py", "pyi", "ts", "tsx", "js", "jsx", "mjs", "cjs", "sh", "bash", "html", "css",
        "scss", "md", "mdx", "json", "toml", "yaml", "yml",
    ];
    const SKIP_DIRS: &[&str] = &[
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
    ];
    fn should_skip_dir(name: &str) -> bool {
        if SKIP_DIRS.contains(&name) {
            return true;
        }
        if name.starts_with(".venv") || name.starts_with("venv") {
            return true;
        }
        if name.starts_with('.') && name != ".claude" {
            return true;
        }
        false
    }
    fn walk(dir: &std::path::Path, acc: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if !should_skip_dir(name) {
                    walk(&path, acc);
                }
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if SUPPORTED_EXTS.contains(&ext) {
                    acc.push(path);
                }
            }
        }
    }
    let project_root = rt.project_root.clone();
    let start = std::time::Instant::now();
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    walk(&project_root, &mut paths);
    let existing: std::collections::HashSet<String> = if force {
        std::collections::HashSet::new()
    } else {
        let sql = format!(
            "SELECT file_path FROM {}",
            schema_guard::TABLE_FILE_KNOWLEDGE
        );
        let conn = rt.ctx.knowledge.conn_ref();
        match conn.prepare(&sql) {
            Ok(mut stmt) => stmt
                .query_map([], |row| row.get::<_, String>(0))
                .ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            Err(_) => std::collections::HashSet::new(),
        }
    };
    let mut files_processed: u64 = 0;
    let mut files_skipped: u64 = 0;
    let mut errors: u64 = 0;
    for path in &paths {
        let abs_path_str = match path.to_str() {
            Some(s) => s,
            None => {
                errors += 1;
                continue;
            }
        };
        let rel_path = crate::runtime::make_relative(abs_path_str, &project_root);
        if !force && existing.contains(&rel_path) {
            files_skipped += 1;
            continue;
        }
        match crate::shared::reindex::reindex_file(rt, abs_path_str, &rel_path) {
            Ok(()) => files_processed += 1,
            Err(_) => errors += 1,
        }
    }
    let elapsed_ms = start.elapsed().as_millis() as u64;
    serde_json::json!(
        { "files_processed" : files_processed, "files_skipped" : files_skipped, "errors"
        : errors, "files_discovered" : paths.len(), "elapsed_ms" : elapsed_ms,
        "project_root" : project_root.display().to_string(), "force" : force, }
    )
    .to_string()
}
/// Query session-file summary from session_file_summary table.
///
/// Payload: `{"file_path": "..."}`
pub fn cli_session_summary(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };
    let conn = rt.ctx.knowledge.conn_ref();
    let mut stmt = match conn
        .prepare(
            &format!(
                "SELECT file_path, session_id, skeleton_json, purpose, top_gotchas_json, blast_severity, created_at \
         FROM {} WHERE file_path = ?1 ORDER BY created_at DESC",
                schema_guard::TABLE_SESSION_FILE_SUMMARY
            ),
        )
    {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({ "error" : format!("query failed: {e}") })
                .to_string();
        }
    };
    let summaries: Vec<serde_json::Value> = stmt
        .query_map(params![file_path], |row| {
            Ok(serde_json::json!(
                { "file_path" : row.get::< _, String > (0) ?, "session_id" : row
                .get::< _, String > (1) ?, "skeleton_json" : row.get::< _, Option
                < String >> (2) ?, "purpose" : row.get::< _, Option < String >>
                (3) ?, "top_gotchas_json" : row.get::< _, Option < String >> (4)
                ?, "blast_severity" : row.get::< _, Option < String >> (5) ?,
                "created_at" : row.get::< _, Option < String >> (6) ? }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    let count = summaries.len();
    let health_delta_str = crate::health_delta::status_json(Some(file_path));
    let health_delta: serde_json::Value =
        serde_json::from_str(&health_delta_str).unwrap_or(serde_json::Value::Null);
    serde_json::json!(
        { "file_path" : file_path, "summaries" : summaries, "count" : count,
        "health_delta" : health_delta, }
    )
    .to_string()
}
/// Store a benchmark run result in metadata_benchmark_runs.
///
/// Payload: `{"bench_name": "...", "p50_ms": 1.0, "p95_ms": 2.0, "p99_ms": 3.0, "samples": 100}`
pub fn cli_bench_run(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let bench_name = payload
        .get("bench_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if bench_name.is_empty() {
        return serde_json::json!({ "error" : "bench_name required" }).to_string();
    }
    let p50_ms = payload
        .get("p50_ms")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let p95_ms = payload
        .get("p95_ms")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let p99_ms = payload
        .get("p99_ms")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let samples = payload.get("samples").and_then(|v| v.as_i64()).unwrap_or(0);
    let commit_hash = payload
        .get("commit_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("head");
    match rt.ctx.knowledge.insert_benchmark_run(
        commit_hash,
        bench_name,
        p50_ms,
        p95_ms,
        p99_ms,
        samples,
    ) {
        Ok(run_id) => serde_json::json!(
            { "stored" : true, "bench_name" : bench_name, "run_id" : run_id,
            "commit_hash" : commit_hash, "p50_ms" : p50_ms, "p95_ms" : p95_ms,
            "p99_ms" : p99_ms, "samples" : samples }
        )
        .to_string(),
        Err(e) => serde_json::json!(
            { "stored" : false, "error" : format!("insert failed: {e}"), "bench_name"
            : bench_name }
        )
        .to_string(),
    }
}
/// Returns the extended knowledge metadata (23 enriched fields: cognitive score, community, modularity, etc.) for a file as JSON.
pub fn cli_file_knowledge_extended(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    FK_EXTENDED_DDL_DONE.get_or_init(|| {
        if let Err(e) = db.conn_ref().execute_batch(
            "CREATE TABLE IF NOT EXISTS cognitive_enrichment (
                file_path TEXT PRIMARY KEY,
                cognitive_score REAL NOT NULL DEFAULT 0.0,
                complexity_signal REAL NOT NULL DEFAULT 0.0,
                fan_in_signal REAL NOT NULL DEFAULT 0.0,
                fan_out_signal REAL NOT NULL DEFAULT 0.0,
                doc_signal REAL NOT NULL DEFAULT 0.0,
                updated_at TEXT DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS module_ecosystem (
                file_path TEXT PRIMARY KEY,
                pub_symbol_count INTEGER DEFAULT 0,
                import_count INTEGER DEFAULT 0,
                re_export_count INTEGER DEFAULT 0,
                integration_score REAL DEFAULT 0.0,
                last_scanned_at TEXT
            );
            CREATE TABLE IF NOT EXISTS file_blake3_registry (
                file_path TEXT PRIMARY KEY,
                blake3_hash TEXT NOT NULL,
                updated_at TEXT DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS file_test_coverage (
                file_path TEXT PRIMARY KEY,
                coverage_pct REAL DEFAULT 0.0,
                updated_at TEXT DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS file_communities (
                file_path TEXT PRIMARY KEY,
                community_id INTEGER DEFAULT 0,
                modularity_score REAL DEFAULT 0.0,
                updated_at TEXT DEFAULT (datetime('now'))
            );",
        ) {
            tracing::warn!("cli_file_knowledge_extended DDL failed: {}", e);
        }
    });
    let file_path = match payload.get("file_path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => normalize_to_relative(p, &rt.project_root),
        _ => {
            return serde_json::json!(
                { "error" : "file_path is required", "usage" :
                "cli-file-knowledge-extended {\"file_path\": \"src/lib.rs\"}" }
            )
            .to_string();
        }
    };
    match rt.ctx.knowledge.query_extended(&file_path) {
        Ok(Some(enriched)) => serde_json::to_string(&enriched)
            .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string()),
        Ok(None) => serde_json::json!(
            { "file_path" : file_path, "found" : false, "message" :
            "file not in knowledge index — run post-read on this file first", }
        )
        .to_string(),
        Err(e) => {
            serde_json::json!({ "file_path" : file_path, "error" : format!("{e}"), }).to_string()
        }
    }
}
