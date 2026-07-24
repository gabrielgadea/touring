//! CLI graph/query handlers (`cli_graph_flow`, `cli_query_dsl`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! BFS path enumeration over the wiring graph + a small DSL→SQL query parser
//! over file_knowledge. Both use fully-qualified `crate::shared::*` and
//! `rusqlite::params_from_iter` paths.

use crate::cli::params::{bool_or, str_or_empty, usize_or};
use crate::runtime::HookRuntime;
use touring_analysis::e2e::schema_guard;

/// Handle `cli-graph-flow` — BFS path enumeration from symbol A to symbol B.
///
/// Uses the wiring graph (wiring_map) to find simple paths connecting two
/// symbols. This enables "how does A eventually call B?" queries.
///
/// Payload: `{"from": "symbol_a", "to": "symbol_b", "max_paths": 10, "max_depth": 8, "validate": true}`
///
/// When `validate` is true (default), the `from`/`to` values are first looked up
/// in the symbol index. If found, the resolved module_file path is used for BFS.
/// If not found, the value is used literally as a module_file path (backward-compatible
/// fallback). Pass `"validate": false` to skip index validation entirely.
pub fn cli_graph_flow(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use std::collections::{HashMap, VecDeque};
    let from_raw = str_or_empty(payload, "from");
    let to_raw = str_or_empty(payload, "to");
    if from_raw.is_empty() || to_raw.is_empty() {
        return serde_json::json!({ "error" : "from and to are required" }).to_string();
    }
    let max_paths: usize = usize_or(payload, "max_paths", 10);
    let max_depth: usize = usize_or(payload, "max_depth", 8);
    let validate = bool_or(payload, "validate", true);
    /// Resolve a user-provided identifier to a wiring-graph node (module_file path).
    /// If `validate` is true, first checks the symbol index; on miss, falls back to
    /// literal interpretation. Returns `None` when the symbol is known but has no
    /// known wiring node.
    fn resolve_node(rt: &HookRuntime, identifier: &str, validate: bool) -> Option<String> {
        if !validate {
            return Some(identifier.to_string());
        }
        if let Some(ref store) = rt.infra.symbol_store {
            if let Ok(locations) = store.find_symbol(identifier) {
                let loc = locations
                    .iter()
                    .find(|l| l.is_definition)
                    .or_else(|| locations.first());
                if let Some(loc) = loc {
                    return Some(loc.file_path.clone());
                }
            }
        }
        Some(identifier.to_string())
    }
    let from = match resolve_node(rt, from_raw, validate) {
        Some(n) => n,
        None => {
            return serde_json::json!(
                { "from" : from_raw, "to" : to_raw, "paths" : [], "count" : 0,
                "truncated" : false, "error" :
                format!("symbol '{}' not found in symbol index", from_raw) }
            )
            .to_string();
        }
    };
    let to = match resolve_node(rt, to_raw, validate) {
        Some(n) => n,
        None => {
            return serde_json::json!(
                { "from" : from_raw, "to" : to_raw, "paths" : [], "count" : 0,
                "truncated" : false, "error" :
                format!("symbol '{}' not found in symbol index", to_raw) }
            )
            .to_string();
        }
    };
    let conn = rt.ctx.knowledge.conn_ref();
    #[derive(Default)]
    struct AdjBuilder(HashMap<String, Vec<String>>);
    impl AdjBuilder {
        fn add(&mut self, module: &str, consumer: &str) {
            self.0
                .entry(module.to_string())
                .or_default()
                .push(consumer.to_string());
        }
        fn build(self) -> HashMap<String, Vec<String>> {
            self.0
        }
    }
    let mut adj = AdjBuilder::default();
    let mut stmt = match conn.prepare(
        "SELECT DISTINCT module_file, consumer_file FROM wiring_map \
         WHERE module_file IS NOT NULL AND consumer_file IS NOT NULL",
    ) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({ "error" : format!("query failed: {e}") }).to_string();
        }
    };
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    for (module, consumer) in &rows {
        adj.add(module, consumer);
    }
    let adj = adj.build();
    let nodes: Vec<&str> = adj.keys().map(|s| s.as_str()).collect();
    let idx: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let start = match idx.get(from.as_str()) {
        Some(&s) => s,
        None => {
            return serde_json::json!(
                { "from" : from_raw, "to" : to_raw, "paths" : [], "count" : 0,
                "truncated" : false, "error" :
                format!("node '{}' not found in wiring graph (try validate:false)", from)
                }
            )
            .to_string();
        }
    };
    let end = match idx.get(to.as_str()) {
        Some(&e) => e,
        None => {
            return serde_json::json!(
                { "from" : from_raw, "to" : to_raw, "paths" : [], "count" : 0,
                "truncated" : false, "error" :
                format!("node '{}' not found in wiring graph (try validate:false)", to) }
            )
            .to_string();
        }
    };
    let mut results: Vec<Vec<usize>> = Vec::new();
    let mut queue: VecDeque<(usize, Vec<usize>)> = VecDeque::new();
    queue.push_back((start, vec![start]));
    while let Some((node, path)) = queue.pop_front() {
        if path.len() > max_depth {
            continue;
        }
        if node == end {
            results.push(path);
            continue;
        }
        if path.len() >= max_depth {
            continue;
        }
        if let Some(neighbors) = adj.get(nodes[node]) {
            for next in neighbors {
                if let Some(&ni) = idx.get(next.as_str()) {
                    if !path.contains(&ni) {
                        let mut new_path = path.clone();
                        new_path.push(ni);
                        queue.push_back((ni, new_path));
                    }
                }
            }
        }
    }
    let total = results.len();
    let truncated = total > max_paths;
    let paths: Vec<serde_json::Value> = results
        .into_iter()
        .take(max_paths)
        .map(|path| {
            let nodes_str: Vec<String> = path.iter().map(|&i| nodes[i].to_string()).collect();
            serde_json::json!(
                { "nodes" : nodes_str, "depth" : path.len().saturating_sub(1) }
            )
        })
        .collect();
    serde_json::json!(
        { "from" : from, "to" : to, "paths" : paths, "count" : paths.len(), "total_found"
        : total, "truncated" : truncated }
    )
    .to_string()
}
/// Simple DSL query parser — parses "field op value [AND field op value ...]"
/// and translates to a SQL WHERE clause against file_knowledge.
///
/// Supported fields: lang, language, line_count, loc, symbol_count, read_count
/// Supported operators: =, !=, >, <, >=, <=
///
/// Payload: `{"query": "lang = rust AND loc > 100"}`
pub fn cli_query_dsl(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let dsl_query = str_or_empty(payload, "query").trim();
    if dsl_query.is_empty() {
        return serde_json::json!({ "error" : "query required" }).to_string();
    }
    let clauses: Vec<&str> = dsl_query.split(" AND ").collect();
    let mut where_parts: Vec<String> = Vec::new();
    let mut params_vec: Vec<String> = Vec::new();
    let mut param_idx = 1usize;
    let allowed_fields = [
        ("lang", "language"),
        ("language", "language"),
        ("line_count", "line_count"),
        ("loc", "line_count"),
        ("symbol_count", "symbol_count"),
        ("read_count", "read_count"),
        ("content_hash", "content_hash"),
    ];
    let allowed_ops = ["=", "!=", ">", "<", ">=", "<=", "LIKE"];
    for clause in &clauses {
        let clause = clause.trim();
        let found = [">=", "<=", "!=", ">", "<", "=", "LIKE"]
            .iter()
            .find_map(|op| {
                clause.find(op).map(|pos| {
                    let field = clause[..pos].trim();
                    let value = clause[pos + op.len()..]
                        .trim()
                        .trim_matches('\'')
                        .trim_matches('"');
                    (field, *op, value)
                })
            });
        match found {
            Some((field, op, value)) => {
                let col = allowed_fields
                    .iter()
                    .find(|(f, _)| *f == field)
                    .map(|(_, c)| *c);
                let op_valid = allowed_ops.contains(&op);
                match (col, op_valid) {
                    (Some(col), true) => {
                        where_parts.push(format!("{col} {op} ?{param_idx}"));
                        params_vec.push(value.to_string());
                        param_idx += 1;
                    }
                    (None, _) => {
                        return serde_json::json!(
                            { "error" :
                            format!("unknown field '{field}' — allowed: lang, language, loc, line_count, symbol_count, read_count")
                            }
                        )
                            .to_string();
                    }
                    (_, false) => {
                        return serde_json::json!(
                            { "error" : format!("invalid operator '{op}'") }
                        )
                        .to_string();
                    }
                }
            }
            None => {
                return serde_json::json!(
                    { "error" :
                    format!("could not parse clause '{clause}' — expected 'field op value'")
                    }
                )
                .to_string();
            }
        }
    }
    let where_sql = if where_parts.is_empty() {
        "1=1".to_string()
    } else {
        where_parts.join(" AND ")
    };
    let sql = format!(
        "SELECT file_path, language, line_count, symbol_count, read_count \
         FROM {} WHERE {} ORDER BY file_path LIMIT 50",
        schema_guard::TABLE_FILE_KNOWLEDGE,
        where_sql
    );
    let conn = rt.ctx.knowledge.conn_ref();
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({ "error" : format!("SQL prepare failed: {e}") }).to_string();
        }
    };
    let results: Result<Vec<serde_json::Value>, rusqlite::Error> = stmt
        .query_and_then(rusqlite::params_from_iter(params_vec.iter()), |row| {
            Ok(serde_json::json!(
                { "file_path" : row.get::< _, String > (0) ?, "language" : row
                .get::< _, Option < String >> (1) ?, "line_count" : row.get::< _,
                i64 > (2) ?, "symbol_count" : row.get::< _, i64 > (3) ?,
                "read_count" : row.get::< _, i64 > (4) ? }
            ))
        })
        .and_then(|iter| iter.collect());
    match results {
        Ok(rows) => {
            let count = rows.len();
            serde_json::json!(
                { "query" : dsl_query, "sql" : sql, "results" : rows, "count" : count }
            )
            .to_string()
        }
        Err(e) => serde_json::json!(
            { "error" : format!("query execution failed: {e}"), "sql" : sql }
        )
        .to_string(),
    }
}
