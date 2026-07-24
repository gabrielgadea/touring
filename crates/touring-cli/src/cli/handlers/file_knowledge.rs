//! CLI file-knowledge query handlers — `cli-file-knowledge-*` daemon handlers.
//!
//! This module contains 3 handlers for file_knowledge population and auditing:
//! - **cli-file-knowledge-stats** — aggregate statistics over file_knowledge table
//! - **cli-file-knowledge-populate** — walk workspace and upsert all files
//! - **cli-file-knowledge-audit** — identify empty/enriched anomalies
//!
//! These are daemon-side handlers invoked via the dispatch table in hook_registry.rs.
//! The CLI client is in `touring-server/src/cli/file_knowledge.rs` and dispatches
//! to these handlers via `daemon_query()`.

use crate::runtime::HookRuntime;
use crate::shared::reindex::reindex_file_with_old;
use std::path::Path;
use touring_analysis::e2e::schema_guard;

/// `cli-file-knowledge-stats` — return aggregate statistics over file_knowledge.
///
/// Returns:
/// ```json
/// {
///   "row_count": 1234,
///   "language_distribution": {"rust": 500, "python": 300, ...},
///   "avg_line_count": 245.6,
///   "total_symbols": 45678,
///   "files_with_symbols": 980,
///   "files_with_imports": 800,
///   "orphan_files": 50
/// }
/// ```
pub fn cli_file_knowledge_stats(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let conn = db.conn_ref();

    // Total row count
    let row_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Language distribution
    let lang_dist: std::collections::HashMap<String, i64> = conn
        .prepare(&format!(
            "SELECT language, COUNT(*) FROM {} WHERE language IS NOT NULL GROUP BY language",
            schema_guard::TABLE_FILE_KNOWLEDGE
        ))
        .ok()
        .map(|mut stmt| {
            let mut map = std::collections::HashMap::new();
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0).unwrap_or_default(),
                    row.get::<_, i64>(1).unwrap_or(0),
                ))
            });
            if let Ok(rows) = rows {
                for r in rows.flatten() {
                    map.insert(r.0, r.1);
                }
            }
            map
        })
        .unwrap_or_default();

    // Average line count
    let avg_line_count: f64 = conn
        .query_row(
            &format!(
                "SELECT AVG(line_count) FROM {} WHERE line_count > 0",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0.0);

    // Total and files-with-symbols
    let (total_symbols, files_with_symbols): (i64, i64) = conn
        .query_row(
            &format!(
                "SELECT COALESCE(SUM(symbol_count),0), COUNT(*) FROM {} WHERE symbol_count > 0",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |row| Ok((row.get(0).unwrap_or(0), row.get(1).unwrap_or(0))),
        )
        .unwrap_or((0, 0));

    // Files with imports
    let files_with_imports: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE imports_json IS NOT NULL AND imports_json != '[]' AND imports_json != 'null'",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    serde_json::json!({
        "row_count": row_count,
        "language_distribution": lang_dist,
        "avg_line_count": avg_line_count,
        "total_symbols": total_symbols,
        "files_with_symbols": files_with_symbols,
        "files_with_imports": files_with_imports,
    })
    .to_string()
}

/// `cli-file-knowledge-populate` — walk all source files in workspace and upsert them.
///
/// Walks `project_root` recursively, filters to known extensions, and calls
/// `reindex_file_with_old` for each file found. Returns a summary of files processed.
pub fn cli_file_knowledge_populate(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let walk_hidden = payload
        .get("include_hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let extensions: Vec<String> = payload
        .get("extensions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                "rs".to_string(),
                "py".to_string(),
                "ts".to_string(),
                "tsx".to_string(),
                "js".to_string(),
                "jsx".to_string(),
                "go".to_string(),
                "java".to_string(),
                "toml".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
                "md".to_string(),
            ]
        });

    let root = rt.project_root.clone();
    let mut processed = 0i64;
    let mut failed = 0i64;
    let mut errors: Vec<String> = Vec::new();

    walk_dir_recursive(
        &root,
        &root,
        &extensions,
        walk_hidden,
        rt,
        &mut processed,
        &mut failed,
        &mut errors,
    );

    serde_json::json!({
        "processed": processed,
        "failed": failed,
        "root": root.to_string_lossy(),
        "errors": errors,
    })
    .to_string()
}

/// Recursive directory walker that calls reindex_file_with_old for each matching file.
///
/// Takes `rt` as a mutable reference parameter to avoid nested function borrow issues.
/// The 8-argument signature is intentional: splitting into a context struct would add
/// indirection without clarity benefit for this single private recursive helper.
#[allow(clippy::too_many_arguments)]
fn walk_dir_recursive(
    dir: &Path,
    root: &Path,
    extensions: &[String],
    walk_hidden: bool,
    rt: &mut HookRuntime,
    processed: &mut i64,
    failed: &mut i64,
    errors: &mut Vec<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !walk_hidden && name.starts_with('.') {
                continue;
            }
            walk_dir_recursive(
                &path,
                root,
                extensions,
                walk_hidden,
                rt,
                processed,
                failed,
                errors,
            );
        } else if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !extensions.iter().any(|e| e == ext) {
                continue;
            }
            let rel_path = match path.strip_prefix(root) {
                Ok(r) => r.to_string_lossy().to_string(),
                Err(_) => continue,
            };
            let abs_path = path.to_string_lossy().to_string();

            match reindex_file_with_old(rt, &abs_path, &rel_path, None) {
                Ok(()) => *processed += 1,
                Err(e) => {
                    *failed += 1;
                    if errors.len() < 10 {
                        errors.push(format!("{}: {}", rel_path, e));
                    }
                }
            }
        }
    }
}

/// `cli-file-knowledge-audit` — report anomalies in file_knowledge table.
///
/// Identifies:
/// - Files with `line_count = 0` or `symbol_count = 0`
/// - Files missing from enrichment tables
/// - coedit_pairs_count = 0 (never co-edited)
/// - edit_history_count = 0 (never edited)
pub fn cli_file_knowledge_audit(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let conn = db.conn_ref();

    // Files with zero line_count
    let empty_line_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE line_count = 0",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Files with zero symbol_count
    let no_symbols: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE symbol_count = 0",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Files missing cognitive_enrichment (join check)
    let missing_enrichment: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_knowledge fk
             LEFT JOIN cognitive_enrichment ce ON fk.file_path = ce.file_path
             WHERE ce.file_path IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Files missing module_ecosystem
    let missing_ecosystem: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_knowledge fk
             LEFT JOIN module_ecosystem me ON fk.file_path = me.file_path
             WHERE me.file_path IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // coedit_pairs = 0 via file_relations count
    let no_coedit: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_knowledge fk
             LEFT JOIN (
                 SELECT source_path, COUNT(*) as pair_count FROM file_relations GROUP BY source_path
             ) fr ON fk.file_path = fr.source_path
             WHERE fr.source_path IS NULL OR fr.pair_count = 0",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let total_files: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    serde_json::json!({
        "empty_line_count": empty_line_count,
        "no_symbols": no_symbols,
        "missing_enrichment": missing_enrichment,
        "missing_ecosystem": missing_ecosystem,
        "no_coedit_pairs": no_coedit,
        "total_files": total_files,
    })
    .to_string()
}
