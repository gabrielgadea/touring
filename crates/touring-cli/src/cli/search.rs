//! CLI search handlers (`cli_search_*`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! LIKE-based symbol + doc search over the wiring_map / file_knowledge tables.

use crate::cli::params;
use crate::runtime::HookRuntime;
use rusqlite::params as sql_params;
use touring_analysis::e2e::schema_guard;

/// Search symbols by name pattern using LIKE matching against wiring_map.
///
/// Payload: `{"query": "...", "top": 10}`
pub fn cli_search_symbols(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = params::str_or_empty(payload, "query");
    if query.is_empty() {
        return serde_json::json!({ "error" : "query required" }).to_string();
    }
    let top = params::i64_or(payload, "top", 10).clamp(1, 100);
    let conn = rt.ctx.knowledge.conn_ref();
    let pattern = format!("%{}%", query);
    let mut stmt = match conn.prepare(&format!(
        "SELECT DISTINCT symbol_name, module_file, symbol_kind \
         FROM {} WHERE symbol_name LIKE ?1 ORDER BY symbol_name LIMIT ?2",
        schema_guard::TABLE_WIRING_MAP
    )) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({ "error" : format!("query failed: {e}") }).to_string();
        }
    };
    let results: Vec<serde_json::Value> = stmt
        .query_map(sql_params![pattern, top], |row| {
            Ok(serde_json::json!(
                { "symbol_name" : row.get::< _, String > (0) ?, "file_path" : row
                .get::< _, String > (1) ?, "symbol_kind" : row.get::< _, String >
                (2) ? }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    let count = results.len();
    serde_json::json!({ "query" : query, "results" : results, "count" : count }).to_string()
}
/// Full-text search in file knowledge notes / documentation.
///
/// Searches `file_knowledge.notes` and `file_knowledge.symbols_json` for the query.
///
/// Payload: `{"query": "...", "top": 10}`
pub fn cli_search_docs(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = params::str_or_empty(payload, "query");
    if query.is_empty() {
        return serde_json::json!({ "error" : "query required" }).to_string();
    }
    let top = params::i64_or(payload, "top", 10).clamp(1, 100);
    let conn = rt.ctx.knowledge.conn_ref();
    let pattern = format!("%{}%", query);
    let mut stmt = match conn.prepare(&format!(
        "SELECT file_path, language, notes FROM {} \
         WHERE notes LIKE ?1 OR symbols_json LIKE ?1 ORDER BY file_path LIMIT ?2",
        schema_guard::TABLE_FILE_KNOWLEDGE
    )) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({ "error" : format!("query failed: {e}") }).to_string();
        }
    };
    let results: Vec<serde_json::Value> = stmt
        .query_map(sql_params![pattern, top], |row| {
            Ok(serde_json::json!(
                { "file_path" : row.get::< _, String > (0) ?, "language" : row
                .get::< _, Option < String >> (1) ?, "context_value" : row.get::<
                _, Option < String >> (2) ? }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    let count = results.len();
    serde_json::json!({ "query" : query, "results" : results, "count" : count }).to_string()
}
