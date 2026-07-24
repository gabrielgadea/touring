//! Pln2 file metadata MCP tools.
//!
//! 9 tools that expose the new CLI handlers (registered in touring-hooks v30.3+)
//! via MCP. Each tool opens a read-only rusqlite connection to knowledge.db and
//! replicates the query logic from the corresponding `cli_*` handler.
//!
//! Table names from `touring_analysis::e2e::schema_guard` (single source of truth).

#![allow(dead_code)] // MCP tools defined for schema exposure, internal helpers may not all be used

use super::*;
use touring_analysis::e2e::schema_guard;

/// Helper: open a read-only rusqlite connection to knowledge.db.
/// Returns an Err McpError with a descriptive message on failure.
fn open_knowledge_db(
    config: &touring_foundation::TouringConfig,
) -> Result<rusqlite::Connection, McpError> {
    let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(&config.project_root);
    if !db_path.exists() {
        return Err(McpError::internal_error(
            "knowledge.db not found — run `touring index rebuild` first",
            None,
        ));
    }
    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| McpError::internal_error(format!("knowledge.db open failed: {e}"), None))?;
    let _ = conn.execute_batch("PRAGMA busy_timeout = 1000;");
    Ok(conn)
}

#[tool_router(router = router_metadata, vis = "pub(crate)")]
impl TouringServer {
    // ── Pln2 Metadata Tools ───────────────────────────────────────────────

    #[tool(
        name = "touring_ast_callgraph",
        description = "Get call graph (pub symbols + their consumers) for a file. \
                       Returns symbol names, kinds, and consumer file paths from wiring_map."
    )]
    async fn ast_callgraph(
        &self,
        params: Parameters<AstCallgraphParams>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = params.0.file_path;
        if file_path.is_empty() {
            return Err(McpError::invalid_params("file_path required", None));
        }

        let conn = open_knowledge_db(&self.config)?;
        let sql = format!(
            "SELECT symbol_name, symbol_kind, consumer_file \
             FROM {} WHERE module_file = ?1 ORDER BY symbol_name",
            schema_guard::TABLE_WIRING_MAP
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

        let symbols: Vec<serde_json::Value> = stmt
            .query_map(rusqlite::params![file_path], |row| {
                let consumer: Option<String> = row.get(2)?;
                Ok(serde_json::json!({
                    "name": row.get::<_, String>(0)?,
                    "kind": row.get::<_, String>(1)?,
                    "consumer": consumer
                }))
            })
            .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?
            .filter_map(|r| r.ok())
            .collect();

        let output = serde_json::json!({
            "file_path": file_path,
            "symbols": symbols,
            "count": symbols.len()
        });
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ast_todos",
        description = "List TODO/FIXME annotations stored in the knowledge index. \
                       Pass an absolute file_path to filter to one file, or leave empty \
                       to return all todos across the project."
    )]
    async fn ast_todos(
        &self,
        params: Parameters<AstTodosParams>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = params.0.file_path;
        let conn = open_knowledge_db(&self.config)?;

        let row_mapper = |row: &rusqlite::Row<'_>| {
            Ok(serde_json::json!({
                "file_path": row.get::<_, String>(0)?,
                "line_number": row.get::<_, i64>(1)?,
                "kind": row.get::<_, String>(2)?,
                "text": row.get::<_, String>(3)?
            }))
        };
        let todos: Vec<serde_json::Value> = if file_path.is_empty() {
            let sql = format!(
                "SELECT file_path, line_num, kind, content FROM {} ORDER BY file_path, line_num",
                schema_guard::TABLE_FILE_TODOS
            );
            match conn.prepare(&sql) {
                Ok(mut stmt) => stmt
                    .query_map([], row_mapper)
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        } else {
            let sql = format!(
                "SELECT file_path, line_num, kind, content FROM {} WHERE file_path = ?1 ORDER BY line_num",
                schema_guard::TABLE_FILE_TODOS
            );
            match conn.prepare(&sql) {
                Ok(mut stmt) => stmt
                    .query_map(rusqlite::params![file_path], row_mapper)
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        };

        let output = serde_json::json!({"todos": todos, "count": todos.len()});
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ast_features",
        description = "List feature flags per file from the knowledge index. \
                       Pass an absolute file_path to filter to one file, or leave empty \
                       to return all feature flags across the project."
    )]
    async fn ast_features(
        &self,
        params: Parameters<AstFeaturesParams>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = params.0.file_path;
        let conn = open_knowledge_db(&self.config)?;

        let feat_mapper = |row: &rusqlite::Row<'_>| {
            Ok(serde_json::json!({
                "file_path": row.get::<_, String>(0)?,
                "feature_name": row.get::<_, String>(1)?,
                "feature_kind": row.get::<_, String>(2)?
            }))
        };
        let features: Vec<serde_json::Value> = if file_path.is_empty() {
            let sql = format!(
                "SELECT file_path, feature_name, lang FROM {} ORDER BY file_path, feature_name",
                schema_guard::TABLE_FILE_FEATURE_FLAGS
            );
            match conn.prepare(&sql) {
                Ok(mut stmt) => stmt
                    .query_map([], feat_mapper)
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        } else {
            let sql = format!(
                "SELECT file_path, feature_name, lang FROM {} WHERE file_path = ?1 ORDER BY feature_name",
                schema_guard::TABLE_FILE_FEATURE_FLAGS
            );
            match conn.prepare(&sql) {
                Ok(mut stmt) => stmt
                    .query_map(rusqlite::params![file_path], feat_mapper)
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        };

        let output = serde_json::json!({"features": features, "count": features.len()});
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        annotations(read_only_hint = true, title = "File metadata + blast radius"),
        name = "touring_ast_meta",
        description = "Consolidated file metadata at skeleton/summary/full depth. \
                       skeleton: file_path, language, line_count, pub symbol names. \
                       summary: skeleton + cognitive_score, fan_in, fan_out, integration_score. \
                       full: summary + imports, todos, feature_flags."
    )]
    async fn ast_meta(
        &self,
        params: Parameters<AstMetaParams>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = params.0.file_path;
        let depth = params.0.depth.unwrap_or_else(|| "summary".to_string());
        if file_path.is_empty() {
            return Err(McpError::invalid_params("file_path required", None));
        }

        let conn = open_knowledge_db(&self.config)?;

        // Skeleton layer — always present
        let fk: Option<(Option<String>, i64, i64)> = conn
            .query_row(
                &format!(
                    "SELECT language, line_count, symbol_count FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_FILE_KNOWLEDGE
                ),
                rusqlite::params![file_path],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        let (language, line_count, _symbol_count) = match fk {
            Some(v) => v,
            None => {
                let output = serde_json::json!({
                    "file_path": file_path,
                    "depth": depth,
                    "error": "file not found in knowledge index"
                });
                let text = serde_json::to_string_pretty(&output)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                return Ok(CallToolResult::success(vec![Content::text(text)]));
            }
        };

        // Pub symbol names (skeleton)
        let pub_symbols: Vec<String> = {
            let sql = format!(
                "SELECT DISTINCT symbol_name FROM {} WHERE module_file = ?1 \
                 AND visibility = 'public' ORDER BY symbol_name",
                schema_guard::TABLE_WIRING_MAP
            );
            match conn.prepare(&sql) {
                Ok(mut stmt) => stmt
                    .query_map(rusqlite::params![file_path], |row| row.get::<_, String>(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        };

        let mut result = serde_json::json!({
            "file_path": file_path,
            "depth": depth,
            "language": language,
            "line_count": line_count,
            "pub_symbols": pub_symbols
        });

        if depth == "skeleton" {
            let text = serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }

        // Summary layer: cognitive + integration scores
        let cognitive_score: f64 = conn
            .query_row(
                &format!(
                    "SELECT cognitive_score FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_COGNITIVE_ENRICHMENT
                ),
                rusqlite::params![file_path],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let (fan_in, fan_out): (f64, f64) = conn
            .query_row(
                &format!(
                    "SELECT fan_in_signal, fan_out_signal FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_COGNITIVE_ENRICHMENT
                ),
                rusqlite::params![file_path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((0.0, 0.0));

        let integration_score: f64 = conn
            .query_row(
                &format!(
                    "SELECT integration_score FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_MODULE_ECOSYSTEM
                ),
                rusqlite::params![file_path],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        if let Some(obj) = result.as_object_mut() {
            obj.insert(
                "cognitive_score".to_string(),
                serde_json::json!(cognitive_score),
            );
            obj.insert("fan_in_signal".to_string(), serde_json::json!(fan_in));
            obj.insert("fan_out_signal".to_string(), serde_json::json!(fan_out));
            obj.insert(
                "integration_score".to_string(),
                serde_json::json!(integration_score),
            );
        }

        if depth == "summary" {
            let text = serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }

        // Full layer: imports, todos, feature_flags
        let imports_json: Option<String> = conn
            .query_row(
                &format!(
                    "SELECT imports_json FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_FILE_KNOWLEDGE
                ),
                rusqlite::params![file_path],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();

        let todos: Vec<serde_json::Value> = {
            let sql = format!(
                "SELECT line_num, kind, content FROM {} WHERE file_path = ?1 ORDER BY line_num",
                schema_guard::TABLE_FILE_TODOS
            );
            match conn.prepare(&sql) {
                Ok(mut stmt) => stmt
                    .query_map(rusqlite::params![file_path], |row| {
                        Ok(serde_json::json!({
                            "line": row.get::<_, i64>(0)?,
                            "kind": row.get::<_, String>(1)?,
                            "text": row.get::<_, String>(2)?
                        }))
                    })
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        };

        let feature_flags: Vec<String> = {
            let sql = format!(
                "SELECT feature_name FROM {} WHERE file_path = ?1 ORDER BY feature_name",
                schema_guard::TABLE_FILE_FEATURE_FLAGS
            );
            match conn.prepare(&sql) {
                Ok(mut stmt) => stmt
                    .query_map(rusqlite::params![file_path], |row| row.get::<_, String>(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        };

        if let Some(obj) = result.as_object_mut() {
            let imports: serde_json::Value = imports_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::json!([]));
            obj.insert("imports".to_string(), imports);
            obj.insert("todos".to_string(), serde_json::json!(todos));
            obj.insert(
                "feature_flags".to_string(),
                serde_json::json!(feature_flags),
            );
        }

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ast_grep",
        description = "Polyglot structural search + rewrite over JS/TS/Python/Go/etc source files \
                       via ast-grep. Pattern metavariables: $VAR matches one node, $$$VAR matches \
                       zero or more. Provide `rewrite` to transform the source instead of just \
                       searching. Language is auto-detected from file extension; use `lang` to \
                       override. Runs in-process (no daemon hop)."
    )]
    async fn ast_grep(
        &self,
        params: Parameters<AstGrepParams>,
    ) -> Result<CallToolResult, McpError> {
        use std::str::FromStr;
        use touring_code::polyglot::{
            Lang, detect_lang, rewrite as poly_rewrite, search as poly_search,
        };

        let p = params.0;
        if p.file_path.is_empty() {
            return Err(McpError::invalid_params("file_path required", None));
        }
        if p.pattern.is_empty() {
            return Err(McpError::invalid_params("pattern required", None));
        }

        let lang = match p.lang.as_deref().filter(|s| !s.is_empty()) {
            Some(explicit) => Lang::from_str(explicit).map_err(|e| {
                McpError::invalid_params(format!("unknown lang '{explicit}': {e}"), None)
            })?,
            None => detect_lang(&p.file_path).ok_or_else(|| {
                McpError::invalid_params(
                    format!(
                        "could not auto-detect lang for '{}' — pass explicit `lang`",
                        p.file_path
                    ),
                    None,
                )
            })?,
        };

        let source = std::fs::read_to_string(&p.file_path)
            .map_err(|e| McpError::internal_error(format!("read failed: {e}"), None))?;

        let output = if let Some(repl) = p.rewrite.as_deref() {
            let out = poly_rewrite(lang, &source, &p.pattern, repl)
                .map_err(|e| McpError::internal_error(format!("rewrite failed: {e}"), None))?;
            serde_json::json!({
                "file_path": p.file_path,
                "lang": lang.name(),
                "rewritten": out != source,
                "source": out
            })
        } else {
            let top = p.top.unwrap_or(50).clamp(1, 10_000) as usize;
            let mut hits = poly_search(lang, &source, &p.pattern)
                .map_err(|e| McpError::internal_error(format!("search failed: {e}"), None))?;
            hits.truncate(top);
            serde_json::json!({
                "file_path": p.file_path,
                "lang": lang.name(),
                "count": hits.len(),
                "matches": hits
            })
        };

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_search_symbols",
        description = "Search for symbols by name using LIKE pattern matching against wiring_map. \
                       Returns ranked results with file paths and symbol kinds. \
                       Use % as wildcard, e.g. '%Handler' matches all handler symbols."
    )]
    async fn search_symbols(
        &self,
        params: Parameters<SearchSymbolsParams>,
    ) -> Result<CallToolResult, McpError> {
        let query = params.0.query;
        if query.is_empty() {
            return Err(McpError::invalid_params("query required", None));
        }
        let top = params.0.top.unwrap_or(10).clamp(1, 100);

        let conn = open_knowledge_db(&self.config)?;
        let pattern = format!("%{}%", query);
        let sql = format!(
            "SELECT DISTINCT symbol_name, module_file, symbol_kind \
             FROM {} WHERE symbol_name LIKE ?1 ORDER BY symbol_name LIMIT ?2",
            schema_guard::TABLE_WIRING_MAP
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

        let results: Vec<serde_json::Value> = stmt
            .query_map(rusqlite::params![pattern, top], |row| {
                Ok(serde_json::json!({
                    "symbol_name": row.get::<_, String>(0)?,
                    "file_path": row.get::<_, String>(1)?,
                    "symbol_kind": row.get::<_, String>(2)?
                }))
            })
            .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?
            .filter_map(|r| r.ok())
            .collect();

        let output =
            serde_json::json!({"query": query, "results": results, "count": results.len()});
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_search_docs",
        description = "Full-text search in file documentation and knowledge context. \
                       Searches file_knowledge.notes and symbols_json for the query string. \
                       Returns file paths, language, and matching context snippets."
    )]
    async fn search_docs(
        &self,
        params: Parameters<SearchDocsParams>,
    ) -> Result<CallToolResult, McpError> {
        let query = params.0.query;
        if query.is_empty() {
            return Err(McpError::invalid_params("query required", None));
        }
        let top = params.0.top.unwrap_or(10).clamp(1, 100);

        let conn = open_knowledge_db(&self.config)?;
        let pattern = format!("%{}%", query);
        let sql = format!(
            "SELECT file_path, language, notes FROM {} \
             WHERE notes LIKE ?1 OR symbols_json LIKE ?1 ORDER BY file_path LIMIT ?2",
            schema_guard::TABLE_FILE_KNOWLEDGE
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

        let results: Vec<serde_json::Value> = stmt
            .query_map(rusqlite::params![pattern, top], |row| {
                Ok(serde_json::json!({
                    "file_path": row.get::<_, String>(0)?,
                    "language": row.get::<_, Option<String>>(1)?,
                    "context_value": row.get::<_, Option<String>>(2)?
                }))
            })
            .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?
            .filter_map(|r| r.ok())
            .collect();

        let output =
            serde_json::json!({"query": query, "results": results, "count": results.len()});
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_query_dsl",
        description = "Query file metadata with a simple DSL. \
                       Syntax: 'field op value [AND field op value ...]'. \
                       Supported fields: lang, language, loc, line_count, symbol_count, read_count. \
                       Supported operators: =, !=, >, <, >=, <=, LIKE. \
                       Example: 'lang = rust AND loc > 100'. Returns up to 50 matching files."
    )]
    async fn query_dsl(
        &self,
        params: Parameters<QueryDslParams>,
    ) -> Result<CallToolResult, McpError> {
        let dsl_query = params.0.query;
        let dsl_query = dsl_query.trim();
        if dsl_query.is_empty() {
            return Err(McpError::invalid_params("query required", None));
        }

        // Parse DSL: split on AND, each clause is "field op value"
        let clauses: Vec<&str> = dsl_query.split(" AND ").collect();
        let mut where_parts: Vec<String> = Vec::new();
        let mut params_vec: Vec<String> = Vec::new();
        let mut param_idx = 1usize;

        // Allowed columns (prevent SQL injection via allowlist)
        let allowed_fields = [
            ("lang", "language"),
            ("language", "language"),
            ("line_count", "line_count"),
            ("loc", "line_count"),
            ("symbol_count", "symbol_count"),
            ("read_count", "read_count"),
            ("content_hash", "content_hash"),
        ];
        let allowed_ops = [">=", "<=", "!=", ">", "<", "=", "LIKE"];

        for clause in &clauses {
            let clause = clause.trim();
            let found = allowed_ops.iter().find_map(|op| {
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
                            let output = serde_json::json!({
                                "error": format!("unknown field '{field}'. Allowed: lang, loc, line_count, symbol_count, read_count")
                            });
                            let text = serde_json::to_string_pretty(&output)
                                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                            return Ok(CallToolResult::success(vec![Content::text(text)]));
                        }
                        (_, false) => {
                            let output = serde_json::json!({
                                "error": format!("invalid operator '{op}'. Allowed: =, !=, >, <, >=, <=, LIKE")
                            });
                            let text = serde_json::to_string_pretty(&output)
                                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                            return Ok(CallToolResult::success(vec![Content::text(text)]));
                        }
                    }
                }
                None => {
                    let output = serde_json::json!({
                        "error": format!("cannot parse clause '{clause}'. Expected: field op value")
                    });
                    let text = serde_json::to_string_pretty(&output)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    return Ok(CallToolResult::success(vec![Content::text(text)]));
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

        let conn = open_knowledge_db(&self.config)?;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| McpError::internal_error(format!("SQL prepare failed: {e}"), None))?;

        let results: Result<Vec<serde_json::Value>, rusqlite::Error> = stmt
            .query_and_then(rusqlite::params_from_iter(params_vec.iter()), |row| {
                Ok(serde_json::json!({
                    "file_path": row.get::<_, String>(0)?,
                    "language": row.get::<_, Option<String>>(1)?,
                    "line_count": row.get::<_, i64>(2)?,
                    "symbol_count": row.get::<_, i64>(3)?,
                    "read_count": row.get::<_, i64>(4)?
                }))
            })
            .and_then(|iter| iter.collect());

        let output = match results {
            Ok(rows) => serde_json::json!({
                "query": dsl_query,
                "sql": sql,
                "results": rows,
                "count": rows.len()
            }),
            Err(e) => serde_json::json!({
                "error": format!("query execution failed: {e}"),
                "sql": sql
            }),
        };
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_session_summary",
        description = "Get session file summaries for a specific file path. \
                       Returns skeleton JSON, purpose, top gotchas, and blast severity \
                       from previous sessions that touched this file."
    )]
    async fn session_summary(
        &self,
        params: Parameters<SessionSummaryParams>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = params.0.file_path;
        if file_path.is_empty() {
            return Err(McpError::invalid_params("file_path required", None));
        }

        let conn = open_knowledge_db(&self.config)?;
        let sql = format!(
            "SELECT file_path, session_id, skeleton_json, purpose, \
             top_gotchas_json, blast_severity, created_at \
             FROM {} WHERE file_path = ?1 ORDER BY created_at DESC",
            schema_guard::TABLE_SESSION_FILE_SUMMARY
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

        let summaries: Vec<serde_json::Value> = stmt
            .query_map(rusqlite::params![file_path], |row| {
                Ok(serde_json::json!({
                    "file_path": row.get::<_, String>(0)?,
                    "session_id": row.get::<_, String>(1)?,
                    "skeleton_json": row.get::<_, Option<String>>(2)?,
                    "purpose": row.get::<_, Option<String>>(3)?,
                    "top_gotchas_json": row.get::<_, Option<String>>(4)?,
                    "blast_severity": row.get::<_, Option<String>>(5)?,
                    "created_at": row.get::<_, Option<String>>(6)?
                }))
            })
            .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?
            .filter_map(|r| r.ok())
            .collect();

        let output = serde_json::json!({
            "file_path": file_path,
            "summaries": summaries,
            "count": summaries.len()
        });
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_wiring_purpose",
        description = "Get the purpose of a module — its role, exports, imports, \
                       and integration score from the dependency graph. \
                       Returns module_role, integration_score, pub symbol count, \
                       import list, and notes from the knowledge index."
    )]
    async fn wiring_purpose(
        &self,
        params: Parameters<WiringPurposeParams>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = params.0.file_path;
        if file_path.is_empty() {
            return Err(McpError::invalid_params("file_path required", None));
        }

        let conn = open_knowledge_db(&self.config)?;

        // Module role and integration score from ecosystem
        let ecosystem: Option<(String, f64, i64, i64)> = conn
            .query_row(
                &format!(
                    "SELECT module_role, integration_score, pub_symbol_count, import_count \
                     FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_MODULE_ECOSYSTEM
                ),
                rusqlite::params![file_path],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .ok();

        let (module_role, integration_score, pub_symbol_count, import_count) =
            ecosystem.unwrap_or_else(|| ("unknown".to_string(), 0.0, 0, 0));

        // Notes as purpose narrative
        let notes: Option<String> = conn
            .query_row(
                &format!(
                    "SELECT notes FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_FILE_KNOWLEDGE
                ),
                rusqlite::params![file_path],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();

        // Imports from file_relations
        let imports: Vec<String> = {
            let sql = format!(
                "SELECT target_path FROM {} WHERE source_path = ?1 ORDER BY target_path",
                schema_guard::TABLE_FILE_RELATIONS
            );
            match conn.prepare(&sql) {
                Ok(mut stmt) => stmt
                    .query_map(rusqlite::params![file_path], |row| row.get::<_, String>(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        };

        let purpose = notes.unwrap_or_else(|| module_role.clone());
        let output = serde_json::json!({
            "file_path": file_path,
            "purpose": purpose,
            "module_role": module_role,
            "integration_score": integration_score,
            "exports": pub_symbol_count,
            "imports_count": import_count,
            "imports": imports
        });
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── D.3.5 — Fix Apply MCP tool ────────────────────────────────────────

    #[tool(
        name = "touring_fix_apply",
        description = "Apply a named assist to fix a diagnostic. Given a diagnostic \
                       code (e.g. Q-201, W-100, M-500) and a file path, looks up the \
                       applicable assist and applies it via SourceChange. Returns \
                       the applied fix details including label, kind, and edit indels. \
                       Use `touring diagnostics --with-fixes <file>` to discover available \
                       fixes for a file."
    )]
    async fn fix_apply(
        &self,
        params: Parameters<FixApplyParams>,
    ) -> Result<CallToolResult, McpError> {
        use std::path::Path;
        use touring_assists::{ALL_HANDLERS, AssistContext, Assists};
        use touring_generator::source_change::TextEdit;

        let p = params.0;

        if p.code.is_empty() {
            return Err(McpError::invalid_params("code required", None));
        }
        if p.file_path.is_empty() {
            return Err(McpError::invalid_params("file_path required", None));
        }
        if !Path::new(&p.file_path).exists() {
            return Err(McpError::invalid_params(
                format!("file not found: {}", p.file_path),
                None,
            ));
        }

        // Map RFC-100 code → assist_id
        let assist_id: &str = match &p.code[..] {
            "Q-201" | "W-100" | "W-101" | "W-102" | "W-103" => "auto_wire",
            "Q-220" => "format_rust_preserve",
            "M-500" | "M-510" | "M-520" | "M-530" => "auto_import",
            "B-301" => "extract_function",
            other => {
                return Err(McpError::invalid_params(
                    format!("no fix known for code: {other}"),
                    None,
                ));
            }
        };

        // Read file content and build AssistContext
        let content = std::fs::read_to_string(&p.file_path)
            .map_err(|e| McpError::internal_error(format!("read file: {e}"), None))?;
        let range = 0..content.len();

        let ctx = AssistContext::new(1usize, &p.file_path, &content, range.clone());
        let mut assists = Assists::new();

        let handler = ALL_HANDLERS
            .iter()
            .find(|(id, _)| *id == assist_id)
            .map(|(_, h)| *h)
            .ok_or_else(|| {
                McpError::internal_error(format!("assist not registered: {assist_id}"), None)
            })?;

        handler(&mut assists, &ctx);
        let finished = assists.finish();

        if finished.is_empty() {
            let output = serde_json::json!({
                "applied": false,
                "reason": "no assist applicable",
                "code": p.code,
                "file_path": p.file_path,
            });
            let text = serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }

        let assist = &finished[0];
        let source_change = assist.source_change.evaluate();

        let changes: Vec<(usize, TextEdit)> = source_change
            .edits()
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        let mut edits: Vec<serde_json::Value> = Vec::new();
        for (file_id, edit) in &changes {
            let indels_json: Vec<_> = edit
                .indels()
                .map(|i| {
                    serde_json::json!({
                        "delete": [i.delete.start, i.delete.end],
                        "insert": i.insert,
                    })
                })
                .collect();
            edits.push(serde_json::json!({
                "file_id": file_id,
                "indels": indels_json,
            }));
        }

        let output = serde_json::json!({
            "applied": true,
            "kind": assist.id,
            "label": assist.label,
            "code": p.code,
            "file_path": p.file_path,
            "edits": edits,
        });
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── D.3 — Assist MCP tools ────────────────────────────────────────────────

    /// D.3.1 — List all available assist kinds (e.g. auto_wire, extract_function).
    #[tool(
        name = "touring_assist_list_kinds",
        description = "List all available refactor assist kinds. Returns each assist's \
                       id and group. Use this before `touring_assist_applicable` to \
                       discover what assists are available in this codebase."
    )]
    async fn assist_list_kinds(
        &self,
        params: Parameters<AssistListKindsParams>,
    ) -> Result<CallToolResult, McpError> {
        use touring_assists::ALL_HANDLERS;

        let p = params.0;
        let kinds: Vec<_> = ALL_HANDLERS
            .iter()
            .filter(|(_, _)| {
                // Filter by group if specified
                if let Some(ref g) = p.group {
                    // All handlers currently in "Refactoring" group; extend when groups are per-handler
                    g == "Refactoring"
                } else {
                    true
                }
            })
            .map(|(id, _)| {
                serde_json::json!({
                    "id": *id,
                    "group": "Refactoring",
                })
            })
            .collect();

        let text = serde_json::to_string_pretty(&kinds)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// D.3.2 — Show applicable assists at a cursor position.
    #[tool(
        name = "touring_assist_applicable",
        description = "Show which assists are applicable at a cursor position in a file. \
                       Takes a cursor spec in file:line:col format (e.g. src/main.rs:10:5). \
                       Returns each applicable assist's id, label, group, file_id, and byte range. \
                       Use before `touring_assist_apply` to discover which assists can run."
    )]
    async fn assist_applicable(
        &self,
        params: Parameters<AssistApplicableParams>,
    ) -> Result<CallToolResult, McpError> {
        use touring_assists::{ALL_HANDLERS, AssistContext, Assists};

        let p = params.0;

        if p.cursor.is_empty() {
            return Err(McpError::invalid_params(
                "cursor required (file:line:col)",
                None,
            ));
        }

        let (file_path, line, col) = parse_cursor_for_mcp(&p.cursor)?;
        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| McpError::internal_error(format!("read file: {e}"), None))?;
        // cursor_to_byte_range not needed here — all handlers inspect ctx.content
        // which contains the full file. This is a "what assists exist" discovery pass.
        let range = 0..content.len();

        let ctx = AssistContext::new(1usize, &file_path, &content, range.clone());
        let mut assists = Assists::new();
        for (_, handler) in ALL_HANDLERS {
            handler(&mut assists, &ctx);
        }

        let results: Vec<_> = assists
            .finish()
            .into_iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.id,
                    "label": a.label,
                    "group": a.group.0,
                    "file_id": a.target.file_id,
                    "range": [a.target.range.start, a.target.range.end],
                })
            })
            .collect();

        let output = serde_json::json!({
            "cursor": p.cursor,
            "file_path": file_path,
            "line": line + 1,
            "col": col + 1,
            "applicable": results,
        });
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// D.3.3 — Apply a specific assist at a position.
    #[tool(
        name = "touring_assist_apply",
        description = "Apply a named assist at a cursor position in a file. Takes an assist kind \
                       (e.g. auto_wire, extract_function), a cursor spec (file:line:col), and a byte \
                       range (start..end). Returns applied=true with the edit indels on success, \
                       or applied=false with a reason if the assist cannot be applied. \
                       Use `touring_assist_applicable` first to discover applicable assists."
    )]
    async fn assist_apply(
        &self,
        params: Parameters<AssistApplyParams>,
    ) -> Result<CallToolResult, McpError> {
        use touring_assists::{ALL_HANDLERS, AssistContext, Assists};
        use touring_generator::source_change::TextEdit;

        let p = params.0;

        if p.kind.is_empty() {
            return Err(McpError::invalid_params("kind required", None));
        }
        if p.cursor.is_empty() {
            return Err(McpError::invalid_params(
                "cursor required (file:line:col)",
                None,
            ));
        }
        if p.range.is_empty() {
            return Err(McpError::invalid_params(
                "range required (start..end)",
                None,
            ));
        }

        let (file_path, _line, _col) = parse_cursor_for_mcp(&p.cursor)?;
        let (start, end) = parse_range_for_mcp(&p.range)?;
        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| McpError::internal_error(format!("read file: {e}"), None))?;

        // Use the explicitly provided byte range — not cursor-derived range.
        // The range param (start..end) is the actual target for the assist.
        // cursor (file:line:col) is used only for file_path resolution.
        let target_range = start..end;
        let ctx = AssistContext::new(1usize, &file_path, &content, target_range);

        let handler = ALL_HANDLERS
            .iter()
            .find(|(id, _)| *id == p.kind)
            .map(|(_, h)| *h)
            .ok_or_else(|| {
                McpError::invalid_params(format!("unknown assist kind: {}", p.kind), None)
            })?;

        let mut assists = Assists::new();
        handler(&mut assists, &ctx);
        let finished = assists.finish();

        if finished.is_empty() {
            let output = serde_json::json!({
                "applied": false,
                "reason": "assist not applicable at this location",
                "kind": p.kind,
                "file_path": file_path,
            });
            let text = serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }

        let assist = &finished[0];
        let source_change = assist.source_change.evaluate();
        let changes: Vec<(usize, TextEdit)> = source_change
            .edits()
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        let mut edits: Vec<serde_json::Value> = Vec::new();
        for (file_id, edit) in &changes {
            let indels_json: Vec<_> = edit
                .indels()
                .map(|i| {
                    serde_json::json!({
                        "delete": [i.delete.start, i.delete.end],
                        "insert": i.insert,
                    })
                })
                .collect();
            edits.push(serde_json::json!({
                "file_id": file_id,
                "indels": indels_json,
            }));
        }

        let output = serde_json::json!({
            "applied": true,
            "kind": assist.id,
            "label": assist.label,
            "file_path": file_path,
            "edits": edits,
        });
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

// ── D.3 MCP helpers (mirrors cli/assist.rs logic) ──────────────────────────────

/// Parse a cursor spec "file:line:col" → (file_path, line_0-indexed, col_0-indexed).
/// Handles Unix paths with colons by looking for the last colon where both
/// trailing parts are numeric.
fn parse_cursor_for_mcp(spec: &str) -> Result<(String, usize, usize), McpError> {
    let spec = spec.trim();
    let spec = spec.strip_prefix("file://").unwrap_or(spec);

    // Find the last colon where the after part is fully numeric (col)
    let last_colon = spec
        .rfind(':')
        .ok_or_else(|| McpError::invalid_params("cursor must be file:line:col", None))?;
    let after_last = &spec[last_colon + 1..];
    if after_last.parse::<usize>().is_err() {
        return Err(McpError::invalid_params(
            format!("invalid cursor format (col not numeric): {}", spec),
            None,
        ));
    }

    // Find the second-to-last colon where the part between it and last colon is numeric (line)
    let before_last = &spec[..last_colon];
    let prev_colon = before_last.rfind(':');

    let (path, line, col) = if let Some(pos) = prev_colon {
        let between = &before_last[pos + 1..];
        if between.parse::<usize>().is_ok() {
            // This is file:line:col
            let path = &spec[..pos];
            let line: usize = between.parse().expect("checked is_ok above");
            let col: usize = after_last.parse().expect("checked is_ok above");
            (
                path.to_string(),
                line.saturating_sub(1),
                col.saturating_sub(1),
            )
        } else {
            // Only col is numeric — treat whole thing as path with line=0, col=col
            (
                before_last.to_string(),
                0usize,
                after_last
                    .parse::<usize>()
                    .expect("checked is_ok above")
                    .saturating_sub(1),
            )
        }
    } else {
        // Only one colon — treat as path:col
        (
            before_last.to_string(),
            0usize,
            after_last
                .parse::<usize>()
                .expect("checked is_ok above")
                .saturating_sub(1),
        )
    };

    if path.is_empty() {
        return Err(McpError::invalid_params("empty file path in cursor", None));
    }
    Ok((path, line, col))
}

/// Parse a range spec "start..end" → (start, end) as byte offsets.
fn parse_range_for_mcp(spec: &str) -> Result<(usize, usize), McpError> {
    let spec = spec.trim();
    let Some((start_s, end_s)) = spec.split_once("..") else {
        return Err(McpError::invalid_params(
            "range must be start..end (e.g. 10..25)",
            None,
        ));
    };
    let start: usize = start_s
        .parse()
        .map_err(|_| McpError::invalid_params(format!("invalid range start: {start_s}"), None))?;
    let end: usize = end_s
        .parse()
        .map_err(|_| McpError::invalid_params(format!("invalid range end: {end_s}"), None))?;
    if start > end {
        return Err(McpError::invalid_params(
            format!("range start ({start}) > end ({end})"),
            None,
        ));
    }
    Ok((start, end))
}

/// Convert (line, col) 0-indexed to a byte range covering that position.
fn cursor_to_byte_range(content: &str, line: usize, col: usize) -> std::ops::Range<usize> {
    let mut offset: usize = 0;
    for (i, line_str) in content.lines().enumerate() {
        if i == line {
            let start = offset.saturating_add(col);
            let end = start.saturating_add(line_str.len().saturating_sub(col));
            return start..end;
        }
        offset = offset.saturating_add(line_str.len()).saturating_add(1);
    }
    0..content.len()
}
