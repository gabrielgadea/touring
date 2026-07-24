//! CLI wiring handlers (`cli_wiring_*`) — extracted from cli_handlers.rs (A-W2.P3).
//!
//! Dependency/orphan/cycle/impact/suggest analysis over the wiring graph.

use crate::cli_handlers::{WiringModuleStatus, WiringOrphan, WiringStatus, normalize_to_relative};
use crate::rfc100_emission::Rfc100Emitter;
use crate::runtime::HookRuntime;
use rusqlite::params;
use touring_analysis::e2e::schema_guard;

/// Reports overall wiring status (orphan, module, symbol, and consumer counts) as JSON.
pub fn cli_wiring_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let orphans_result: Result<Vec<crate::wiring::WiringEntry>, _> = db.orphan_symbols();
    let orphan_count: usize = orphans_result.map(|v| v.len()).unwrap_or(0);
    let module_count: usize = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT module_file) FROM {}",
                schema_guard::TABLE_WIRING_MAP
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0) as usize;
    let total_pub: i64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT symbol_name) FROM {} WHERE visibility = 'public' AND consumer_file IS NULL",
                schema_guard::TABLE_WIRING_MAP
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let total_cons: i64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT symbol_name) FROM {} WHERE consumer_file IS NOT NULL",
                schema_guard::TABLE_WIRING_MAP
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let status = WiringStatus {
        orphan_count,
        module_count,
        total_pub_symbols: total_pub as usize,
        total_consumers: total_cons as usize,
    };
    let access_count: i64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_FILE_ACCESS_LOG
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let bash_count: i64 = db
        .conn_ref()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_BASH_OUTCOMES),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let edit_count: i64 = db
        .conn_ref()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_EDIT_HISTORY),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let gotcha_count: i64 = db
        .conn_ref()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_GOTCHAS),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let coedit_pairs: i64 = db
        .conn_ref()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_FILE_COEDITS),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let relation_count: i64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_FILE_RELATIONS
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let task_metrics_count: i64 = db
        .conn_ref()
        .query_row("SELECT COUNT(*) FROM task_decompositions", [], |r| r.get(0))
        .unwrap_or(0);
    let mut result = serde_json::to_value(&status).unwrap_or_default();
    if let Some(obj) = result.as_object_mut() {
        obj.insert(
            "knowledge_activity".to_string(),
            serde_json::json!(
                { "access_count" : access_count, "bash_count" : bash_count, "edit_count"
                : edit_count, "gotcha_count" : gotcha_count, "coedit_pairs" :
                coedit_pairs, "relation_count" : relation_count, "task_metrics_count" :
                task_metrics_count, }
            ),
        );
        let (hg_count, hg_labels) = crate::wiring::hypergraph_cycle_detection(db);
        let (_hg, multi_imports) = crate::wiring::build_multi_import_hypergraph(db);
        obj.insert(
            "hypergraph_cycles".to_string(),
            serde_json::json!({ "count" : hg_count, "detail" : hg_labels, }),
        );
        obj.insert(
            "multi_import_hyperedges".to_string(),
            serde_json::json!({ "count" : multi_imports.len(), }),
        );
    }
    serde_json::to_string(&result)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
/// Lists orphan public symbols (pub items with no consumers) from the wiring graph as JSON.
pub fn cli_wiring_orphans(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let orphans: Vec<WiringOrphan> = db
        .orphan_symbols()
        .map(|entries| {
            entries
                .into_iter()
                .map(|e| WiringOrphan {
                    module_file: e.module_file,
                    symbol_name: e.symbol_name,
                    symbol_kind: e.symbol_kind,
                    visibility: e.visibility,
                })
                .collect()
        })
        .unwrap_or_default();
    let symbol_names: Vec<String> = orphans.iter().map(|o| o.symbol_name.clone()).collect();
    let dead_patterns = touring_analysis::scan_dead_patterns(&symbol_names);
    let want_diagnostics = payload
        .get("diagnostics")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if want_diagnostics {
        use touring_analysis::wiring::WiringFinding;
        use touring_foundation::diagnostic::DiagnosticCode;
        let mut diagnostics = Vec::with_capacity(orphans.len() + dead_patterns.len());
        for o in &orphans {
            let f = WiringFinding::OrphanSymbol {
                module_file: o.module_file.clone(),
                symbol: o.symbol_name.clone(),
            };
            diagnostics.push(
                f.to_diagnostic()
                    .try_attach_source_from_file(&o.module_file, 4096),
            );
        }
        for sym in &dead_patterns {
            let f = WiringFinding::CouldBePublic {
                symbol: sym.clone(),
                file: String::from("(unknown)"),
            };
            diagnostics.push(f.to_diagnostic());
        }
        for _ in &diagnostics {
            crate::shared::gate_metrics::record_diagnostic_wiring_finding_emitted();
        }
        return serde_json::json!(
            { "orphans" : orphans, "dead_patterns" : dead_patterns, "orphan_count" :
            orphans.len(), "diagnostics" : diagnostics, "diagnostic_count" : diagnostics
            .len(), }
        )
        .to_string();
    }
    serde_json::json!(
        { "orphans" : orphans, "dead_patterns" : dead_patterns, "orphan_count" : orphans
        .len(), }
    )
    .to_string()
}
/// `cli-wiring-modules` — list integration scores for all modules.
///
/// Wave 22 (S-Q1b + S-Q2): rewritten to use `wiring_modules_aggregate()`
/// (single GROUP BY SQL — O(1) instead of O(N*3)) and wrapped in a
/// `query_cache` for the common read-only dashboard path.
///
/// Invariant C-2: integration_score = wired_count/total_pub (1.0 when total_pub=0).
/// Orphan symbol names are fetched only for modules with wired_count < total_pub.
pub fn cli_wiring_modules(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let cache_key = crate::shared::query_cache::make_key("cli_wiring_modules", "v1");
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }
    let db = &rt.ctx.knowledge;
    let agg_rows = match db.wiring_modules_aggregate() {
        Ok(rows) => rows,
        Err(e) => {
            return serde_json::json!(
                { "error" : format!("aggregate query failed: {e}") }
            )
            .to_string();
        }
    };
    let modules: Vec<WiringModuleStatus> = agg_rows
        .into_iter()
        .map(|row| {
            let score = row.integration_score();
            Rfc100Emitter::emit_w101_low_integration(&row.module_file, score);
            let orphan_symbols: Vec<String> = if row.wired_count < row.total_pub {
                db.orphan_symbols_for_module(&row.module_file)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|e| e.symbol_name)
                    .collect()
            } else {
                vec![]
            };
            let orphan_count = orphan_symbols.len();
            WiringModuleStatus {
                file_path: row.module_file,
                integration_score: score,
                total_pub_symbols: row.total_pub as usize,
                orphan_count,
                orphan_symbols,
            }
        })
        .collect();
    let out = serde_json::to_string(&modules)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string());
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}
/// Handle `cli-wiring-impact` — transitive impact analysis for a symbol.
///
/// Computes all symbols (direct + transitive) that would be affected by changes
/// to the given symbol. Uses BFS walk on consumer edges.
///
/// Args from payload:
///   - `symbol` (str): symbol name to analyze
///   - `depth` (u32, default 5): maximum traversal depth
///   - `format` (str, default "text"): output format "text" or "json"
pub fn cli_wiring_impact(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol = payload.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
    let max_depth = payload.get("depth").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let format = payload
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    if symbol.is_empty() {
        return serde_json::json!({ "error" : "symbol is required" }).to_string();
    }
    let impact = crate::wiring::compute_impact(&rt.ctx.knowledge, symbol, max_depth);
    if format == "json" {
        return serde_json::to_string(&impact)
            .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string());
    }
    let mut lines = vec![
        format!("Impact Analysis: {}", symbol),
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string(),
        format!("Direct consumers: {}", impact.direct_consumers),
        format!("Total transitive impacted: {}", impact.total_transitive),
        format!("Max depth: {}", impact.max_depth),
        String::new(),
        "Path                           Type        Depth  Fan-out".to_string(),
        "──────────────────────────────────────────────────────────".to_string(),
    ];
    for path in &impact.paths {
        let indent = "  ".repeat(path.depth.saturating_sub(1));
        let prefix = if path.depth == 1 {
            "└─►"
        } else {
            "  └─►"
        };
        lines.push(format!(
            "{}{} {:20} {:8} {:5} {}",
            indent,
            prefix,
            path.consumer_symbol.chars().take(20).collect::<String>(),
            path.path_type,
            path.depth,
            path.fan_out
        ));
    }
    lines.join("\n")
}
/// Handle `cli-wiring-cycles` — detect dependency cycles in the wiring graph.
///
/// Uses Tarjan's SCC algorithm to find strongly-connected components (cycles)
/// in the module dependency graph.
///
/// Args from payload:
///   - `min_depth` (u32, default 2): minimum cycle length to report
///   - `format` (str, default "text"): output format "text" or "json"
pub fn cli_wiring_cycles(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let min_depth = payload
        .get("min_depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(2) as usize;
    let format = payload
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    // PLT-2026-06-02: callers can opt into per-workspace filtering via
    // `{"workspace_root": "..."}` to suppress false-positive cycles that
    // leak from sibling projects (e.g., `analise/kazuba-rust-core` rows
    // polluting konverter's view). Legacy callers that don't set the
    // key get the unfiltered behaviour (back-compat).
    let workspace_root_filter: Option<String> = payload
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let all_cycles =
        crate::wiring::find_all_cycles(&rt.ctx.knowledge, workspace_root_filter.as_deref(), true);
    let cycles: Vec<_> = all_cycles
        .into_iter()
        .filter(|c| c.depth >= min_depth)
        .collect();
    for cycle in &cycles {
        Rfc100Emitter::emit_w110_dependency_cycle(&cycle.modules.join(" -> "), cycle.depth);
    }
    if format == "json" {
        return serde_json::json!({ "cycle_count" : cycles.len(), "cycles" : cycles }).to_string();
    }
    let mut lines = vec![
        format!("Dependency Cycles Detected: {}", cycles.len()),
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string(),
        String::new(),
    ];
    for cycle in &cycles {
        lines.push(format!(
            "Cycle #{} (depth: {}, modules: {}, severity: {})",
            cycle.id,
            cycle.depth,
            cycle.modules.len(),
            cycle.severity
        ));
        for (i, module) in cycle.modules.iter().enumerate() {
            let arrow = if i < cycle.modules.len() - 1 {
                " → "
            } else {
                " [CYCLE CLOSES]"
            };
            lines.push(format!("  {}{}", module, arrow));
        }
        lines.push(String::new());
    }
    lines.join("\n")
}
/// Handle `cli-wiring-suggest` — returns wiring suggestions for an orphan symbol.
/// Supports bulk mode when `orphan_symbols` (array) is provided.
pub fn cli_wiring_suggest(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    if let Some(symbols_arr) = payload.get("orphan_symbols").and_then(|v| v.as_array()) {
        let symbols: Vec<String> = symbols_arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if symbols.is_empty() {
            return serde_json::json!(
                { "error" : "orphan_symbols array is empty", "count" : 0, "results" : []
                }
            )
            .to_string();
        }
        let symbols = if symbols.len() > 1000 {
            symbols.into_iter().take(1000).collect()
        } else {
            symbols
        };
        let mut results: Vec<serde_json::Value> = Vec::with_capacity(symbols.len());
        let mut total_count = 0;
        for symbol in &symbols {
            let result = process_single_wiring_suggest(rt, symbol);
            if let Some(count) = result.get("count").and_then(|c| c.as_u64()) {
                total_count += count;
            }
            let mut clean = result.clone();
            if let Some(obj) = clean.as_object_mut() {
                obj.remove("orphan_symbol");
            }
            results.push(serde_json::json!({ "symbol" : symbol, "result" : clean }));
        }
        return serde_json::json!(
            { "orphan_symbols" : symbols, "symbol_count" : symbols.len(), "total_count" :
            total_count, "results" : results }
        )
        .to_string();
    }
    let orphan_symbol = payload
        .get("orphan_symbol")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    process_single_wiring_suggest(rt, orphan_symbol).to_string()
}
/// Process a single orphan symbol (helper for bulk mode).
fn process_single_wiring_suggest(rt: &mut HookRuntime, orphan_symbol: &str) -> serde_json::Value {
    let cached = rt
        .ctx
        .knowledge
        .get_pending_wiring_suggestions(orphan_symbol);
    match cached {
        Err(e) => {
            return serde_json::json!(
                { "error" : format!("wiring_suggest query failed: {e}"), "orphan_symbol"
                : orphan_symbol }
            );
        }
        Ok(rows) if !rows.is_empty() => {
            let suggestions: Vec<serde_json::Value> = rows
                .iter()
                .map(|(id, orphan_file, suggested_consumer, score)| {
                    serde_json::json!(
                        { "id" : id, "orphan_file" : orphan_file, "suggested_consumer" :
                        suggested_consumer, "similarity_score" : score }
                    )
                })
                .collect();
            return serde_json::json!(
                { "orphan_symbol" : orphan_symbol, "count" : suggestions.len(),
                "suggestions" : suggestions, "source" : "cached" }
            );
        }
        Ok(_) => {}
    }
    let orphan_file: Option<String> = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            &format!(
                "SELECT DISTINCT module_file FROM {} WHERE symbol_name = ?1 \
                 AND visibility = 'public' LIMIT 1",
                schema_guard::TABLE_WIRING_MAP
            ),
            params![orphan_symbol],
            |r| r.get(0),
        )
        .ok();
    let Some(ref file) = orphan_file else {
        return serde_json::json!(
            { "orphan_symbol" : orphan_symbol, "count" : 0, "suggestions" : [], "note" :
            "orphan_symbol not found in wiring_map — run `touring index rebuild` first"
            }
        );
    };
    let neighbors = rt.ctx.knowledge.get_coedit_neighbors(file, 10);
    let max_count = neighbors.iter().map(|(_, c)| *c).max().unwrap_or(1) as f64;
    let suggestions: Vec<serde_json::Value> = neighbors
        .iter()
        .map(|(neighbor, count)| {
            let score = *count as f64 / max_count;
            let _ = rt.ctx.knowledge.upsert_wiring_suggestion(
                orphan_symbol,
                file,
                Some(neighbor.as_str()),
                score,
                None,
            );
            serde_json::json!(
                { "orphan_file" : file, "suggested_consumer" : neighbor,
                "similarity_score" : score }
            )
        })
        .collect();
    serde_json::json!(
        { "orphan_symbol" : orphan_symbol, "count" : suggestions.len(), "suggestions" :
        suggestions, "source" : "computed" }
    )
}
/// Query module purpose and export/import profile from the wiring graph.
///
/// Payload: `{"file_path": "..."}`
pub fn cli_wiring_purpose(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };
    let file_path = normalize_to_relative(file_path, &rt.project_root);
    let conn = rt.ctx.knowledge.conn_ref();
    let ecosystem: Option<(String, f64, i64, i64)> = conn
        .query_row(
            &format!(
                "SELECT module_role, integration_score, pub_symbol_count, import_count \
                 FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_MODULE_ECOSYSTEM
            ),
            params![file_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .ok();
    let (module_role, integration_score, pub_symbol_count, import_count) =
        ecosystem.unwrap_or_else(|| ("unknown".to_string(), 0.0, 0, 0));
    let notes: Option<String> = conn
        .query_row(
            &format!(
                "SELECT notes FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            params![file_path],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    let imports: Vec<String> = {
        let sql = format!(
            "SELECT target_path FROM {} WHERE source_path = ?1 ORDER BY target_path",
            schema_guard::TABLE_FILE_RELATIONS
        );
        match conn.prepare(&sql) {
            Ok(mut stmt) => stmt
                .query_map(params![file_path], |row| row.get::<_, String>(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    };
    let purpose = notes.unwrap_or_else(|| module_role.clone());
    serde_json::json!(
        { "file_path" : file_path, "purpose" : purpose, "module_role" : module_role,
        "integration_score" : integration_score, "exports" : pub_symbol_count,
        "imports_count" : import_count, "imports" : imports }
    )
    .to_string()
}
/// Handler: cli-wiring-community
/// Payload: {file_path: str}
/// Returns the Louvain/Leiden community assignment for a file (community_id, modularity_score).
pub fn cli_wiring_community(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };
    let file_path = normalize_to_relative(file_path, &rt.project_root);
    let conn = rt.ctx.knowledge.conn_ref();
    let sql = format!(
        "SELECT community_id, modularity_score FROM {} WHERE file_path = ?1",
        schema_guard::TABLE_FILE_COMMUNITIES
    );
    match conn.query_row(&sql, params![file_path], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
    }) {
        Ok((community_id, modularity_score)) => serde_json::json!(
            { "file_path" : file_path, "community_id" : community_id,
            "modularity_score" : modularity_score, "assigned" : true }
        )
        .to_string(),
        Err(rusqlite::Error::QueryReturnedNoRows) => serde_json::json!(
            { "file_path" : file_path, "assigned" : false, "community_id" : null,
            "modularity_score" : null }
        )
        .to_string(),
        Err(e) => serde_json::json!(
            { "file_path" : file_path, "error" : format!("{e}"), "assigned" : false }
        )
        .to_string(),
    }
}
/// `cli-wiring-chains` — Query or rebuild functional chains in the wiring graph.
///
/// Payload:
/// - `{"file_path": "src/foo.rs"}` → return chains where this module is source/sink
/// - `{"rebuild": true}` → rebuild all chains from registered functional signatures
/// - `{}` → rebuild + return chain count
pub fn cli_wiring_chains(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let _ = db.conn_ref().execute_batch(
        "CREATE TABLE IF NOT EXISTS functional_signatures (
            file_path TEXT PRIMARY KEY,
            module_purpose TEXT,
            domain TEXT,
            symbols_json TEXT DEFAULT '[]',
            content_hash TEXT,
            updated_at TEXT DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS functional_chains (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_module TEXT NOT NULL,
            source_symbol TEXT NOT NULL,
            source_output TEXT,
            sink_module TEXT NOT NULL,
            sink_symbol TEXT NOT NULL,
            sink_input TEXT,
            chain_type TEXT NOT NULL DEFAULT 'Sequential',
            confidence REAL DEFAULT 0.0,
            created_at TEXT DEFAULT (datetime('now'))
        );",
    );
    if payload
        .get("rebuild")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return match db.rebuild_functional_chains() {
            Ok(count) => {
                serde_json::json!({ "rebuilt" : true, "chain_count" : count, }).to_string()
            }
            Err(e) => {
                serde_json::json!({ "rebuilt" : false, "error" : format!("{e}"), }).to_string()
            }
        };
    }
    if let Some(file_path) = payload.get("file_path").and_then(|v| v.as_str()) {
        if !file_path.is_empty() {
            let file_path = normalize_to_relative(file_path, &rt.project_root);
            return match db.chains_for_module(&file_path) {
                Ok(chains) => {
                    let chains_json: Vec<serde_json::Value> = chains
                        .iter()
                        .map(|c| {
                            serde_json::json!(
                                { "id" : c.id, "source_module" : c.source_module,
                                "source_symbol" : c.source_symbol, "source_output" : c
                                .source_output, "sink_module" : c.sink_module, "sink_symbol"
                                : c.sink_symbol, "sink_input" : c.sink_input, "chain_type" :
                                c.chain_type, "confidence" : c.confidence, }
                            )
                        })
                        .collect();
                    serde_json::json!(
                        { "file_path" : file_path, "chain_count" : chains_json.len(),
                        "chains" : chains_json, }
                    )
                    .to_string()
                }
                Err(e) => serde_json::json!(
                    { "file_path" : file_path, "error" : format!("{e}"), }
                )
                .to_string(),
            };
        }
    }
    match db.rebuild_functional_chains() {
        Ok(count) => serde_json::json!({ "rebuilt" : true, "chain_count" : count, }).to_string(),
        Err(e) => serde_json::json!({ "rebuilt" : false, "error" : format!("{e}"), }).to_string(),
    }
}
