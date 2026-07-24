//! CLI AST analysis handlers (`cli_ast_*`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Callgraph/todos/rationale/features/meta/skeleton/tdg/blast queries. Uses
//! fully-qualified `crate::ast_bridge::*` and `crate::shared::query_cache::*`
//! paths; the exclusive helpers `compute_churn_score` and
//! `detect_language_from_ext` move alongside the handlers. The shared
//! `normalize_to_relative` helper stays in cli_handlers.rs and is imported.

use crate::cli_handlers::normalize_to_relative;
use crate::runtime::HookRuntime;
use rusqlite::params;
use touring_analysis::e2e::schema_guard;
use touring_foundation::diagnostic::DiagnosticCode;

/// Query symbol-level call-graph data for a file.
///
/// Uses the wiring_map table (which contains pub symbols per file with kind info)
/// as the symbol registry for the given file.
///
/// Payload: `{"file_path": "..."}`
pub fn cli_ast_callgraph(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };
    let conn = rt.ctx.knowledge.conn_ref();
    let mut stmt = match conn.prepare(&format!(
        "SELECT symbol_name, symbol_kind, consumer_file \
         FROM {} WHERE module_file = ?1 ORDER BY symbol_name",
        schema_guard::TABLE_WIRING_MAP
    )) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({ "error" : format!("query failed: {e}") }).to_string();
        }
    };
    let symbols: Vec<serde_json::Value> = stmt
        .query_map(params![file_path], |row| {
            let consumer: Option<String> = row.get(2)?;
            Ok(serde_json::json!(
                { "name" : row.get::< _, String > (0) ?, "kind" : row.get::< _,
                String > (1) ?, "consumer" : consumer }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    let count = symbols.len();
    serde_json::json!({ "file_path" : file_path, "symbols" : symbols, "count" : count }).to_string()
}
/// Query TODOs and FIXMEs stored in the file_todos table.
///
/// Payload: `{"file_path": "..."}` — if empty, returns all todos.
pub fn cli_ast_todos(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let conn = rt.ctx.knowledge.conn_ref();
    let todos: Vec<serde_json::Value> = if file_path.is_empty() {
        let mut stmt = match conn.prepare(&format!(
            "SELECT file_path, line_num, kind, content FROM {} ORDER BY file_path, line_num",
            schema_guard::TABLE_FILE_TODOS
        )) {
            Ok(s) => s,
            Err(e) => {
                return serde_json::json!({ "error" : format!("query failed: {e}") }).to_string();
            }
        };
        stmt.query_map([], |row| {
            Ok(serde_json::json!(
                { "file_path" : row.get::< _, String > (0) ?, "line_number" :
                row.get::< _, i64 > (1) ?, "kind" : row.get::< _, String >
                (2) ?, "text" : row.get::< _, String > (3) ? }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    } else {
        let mut stmt = match conn
            .prepare(
                &format!(
                    "SELECT file_path, line_num, kind, content FROM {} WHERE file_path = ?1 ORDER BY line_num",
                    schema_guard::TABLE_FILE_TODOS
                ),
            )
        {
            Ok(s) => s,
            Err(e) => {
                return serde_json::json!({ "error" : format!("query failed: {e}") })
                    .to_string();
            }
        };
        stmt.query_map(params![file_path], |row| {
            Ok(serde_json::json!(
                { "file_path" : row.get::< _, String > (0) ?, "line_number" :
                row.get::< _, i64 > (1) ?, "kind" : row.get::< _, String >
                (2) ?, "text" : row.get::< _, String > (3) ? }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };
    let count = todos.len();
    serde_json::json!({ "todos" : todos, "count" : count }).to_string()
}
/// Query file rationale / notes from the file_knowledge table.
///
/// The `notes` column in file_knowledge stores contextual annotations.
///
/// Payload: `{"file_path": "..."}`
pub fn cli_ast_rationale(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };
    let conn = rt.ctx.knowledge.conn_ref();
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            &format!(
                "SELECT notes, language FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            params![file_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    match row {
        Some((notes, lang)) => serde_json::json!(
            { "file_path" : file_path, "language" : lang, "rationale" : notes
            .unwrap_or_default(), "source" : "file_knowledge.notes" }
        )
        .to_string(),
        None => serde_json::json!(
            { "file_path" : file_path, "rationale" : serde_json::Value::Null, "note"
            : "file not found in knowledge index" }
        )
        .to_string(),
    }
}
/// Query feature flags for a file (or all files) from the file_feature_flags table.
///
/// Payload: `{"file_path": "..."}` — optional.
pub fn cli_ast_features(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let conn = rt.ctx.knowledge.conn_ref();
    let features: Vec<serde_json::Value> = if file_path.is_empty() {
        let mut stmt = match conn.prepare(&format!(
            "SELECT file_path, feature_name, lang FROM {} ORDER BY file_path, feature_name",
            schema_guard::TABLE_FILE_FEATURE_FLAGS
        )) {
            Ok(s) => s,
            Err(e) => {
                return serde_json::json!({ "error" : format!("query failed: {e}") }).to_string();
            }
        };
        stmt.query_map([], |row| {
            Ok(serde_json::json!(
                { "file_path" : row.get::< _, String > (0) ?, "feature_name"
                : row.get::< _, String > (1) ?, "feature_kind" : row.get::<
                _, String > (2) ? }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    } else {
        let mut stmt = match conn
            .prepare(
                &format!(
                    "SELECT file_path, feature_name, lang FROM {} WHERE file_path = ?1 ORDER BY feature_name",
                    schema_guard::TABLE_FILE_FEATURE_FLAGS
                ),
            )
        {
            Ok(s) => s,
            Err(e) => {
                return serde_json::json!({ "error" : format!("query failed: {e}") })
                    .to_string();
            }
        };
        stmt.query_map(params![file_path], |row| {
            Ok(serde_json::json!(
                { "file_path" : row.get::< _, String > (0) ?, "feature_name"
                : row.get::< _, String > (1) ?, "feature_kind" : row.get::<
                _, String > (2) ? }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };
    let count = features.len();
    serde_json::json!({ "features" : features, "count" : count }).to_string()
}
/// Consolidated file metadata at skeleton / summary / full depth.
///
/// Payload: `{"file_path": "...", "depth": "skeleton|summary|full"}`
///
/// - skeleton: file_path, language, line_count, pub symbol names
/// - summary: skeleton + fan_in/fan_out signals, cognitive_score, integration_score
/// - full: summary + imports, todos, feature_flags
pub fn cli_ast_meta(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let depth = payload
        .get("depth")
        .and_then(|v| v.as_str())
        .unwrap_or("skeleton");
    if file_path.is_empty() {
        return serde_json::json!({ "error" : "file_path required" }).to_string();
    }
    let file_path = normalize_to_relative(file_path, &rt.project_root);
    let cache_key =
        crate::shared::query_cache::make_key("cli_ast_meta", &format!("{file_path}|{depth}"));
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }
    let conn = rt.ctx.knowledge.conn_ref();
    let fk: Option<(Option<String>, i64, i64, Option<String>)> = conn
        .query_row(
            &format!(
                "SELECT language, line_count, symbol_count, notes FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            params![file_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .ok();
    let (language, line_count, _symbol_count, notes, enrichment_source) = match fk {
        Some((lang, lc, sc, n)) => (lang, lc, sc, n, "knowledge_db"),
        None => {
            let abs_path = if std::path::Path::new(&file_path).is_absolute() {
                std::path::PathBuf::from(&file_path)
            } else {
                rt.project_root.join(&file_path)
            };
            let lang_str =
                touring_code::ast::Lang::from_path(&abs_path).map(|l| l.as_str().to_string());
            let lc = std::fs::read_to_string(&abs_path)
                .map(|c| c.lines().count() as i64)
                .unwrap_or(0);
            (lang_str, lc, 0i64, None::<String>, "on_disk_fallback")
        }
    };
    let pub_symbols: Vec<String> = {
        let sql = format!(
            "SELECT DISTINCT symbol_name FROM {} WHERE module_file = ?1 AND visibility = 'public' ORDER BY symbol_name",
            schema_guard::TABLE_WIRING_MAP
        );
        let from_db: Vec<String> = match conn.prepare(&sql) {
            Ok(mut stmt) => stmt
                .query_map(params![file_path], |row| row.get::<_, String>(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        if !from_db.is_empty() {
            from_db
        } else if enrichment_source == "on_disk_fallback" {
            let abs_path = if std::path::Path::new(&file_path).is_absolute() {
                std::path::PathBuf::from(&file_path)
            } else {
                rt.project_root.join(&file_path)
            };
            std::fs::read_to_string(&abs_path)
                .ok()
                .and_then(|c| crate::ast_bridge::extract_enriched_symbols(&c, &file_path))
                .map(|syms| {
                    syms.into_iter()
                        .filter(|s| s.is_public)
                        .map(|s| s.name)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    let mut result = serde_json::json!(
        { "file_path" : file_path, "depth" : depth, "language" : language, "line_count" :
        line_count, "pub_symbols" : pub_symbols, "enrichment_source" : enrichment_source
        }
    );
    if depth == "skeleton" {
        return result.to_string();
    }
    let cog = rt
        .ctx
        .knowledge
        .get_cognitive_enrichment(&file_path)
        .unwrap_or(None);
    let (cognitive_score, fan_in, fan_out, summary_source) = match cog {
        Some((cs, _, fi, fo, _)) => (cs, fi, fo, "knowledge_db"),
        None => {
            let abs_path = if std::path::Path::new(&file_path).is_absolute() {
                std::path::PathBuf::from(&file_path)
            } else {
                rt.project_root.join(&file_path)
            };
            let score = touring_code::ast::Lang::from_path(&abs_path)
                .and_then(|lang| {
                    std::fs::read_to_string(&abs_path)
                        .ok()
                        .map(|c| touring_code::ast::analyze_quality(&c, lang).overall_score as f64)
                })
                .unwrap_or(0.0);
            (score, 0.0, 0.0, "on_disk_fallback")
        }
    };
    let integration_score: f64 = conn
        .query_row(
            &format!(
                "SELECT integration_score FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_MODULE_ECOSYSTEM
            ),
            params![file_path],
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
        obj.insert("notes".to_string(), serde_json::json!(notes));
        obj.insert(
            "summary_source".to_string(),
            serde_json::json!(summary_source),
        );
    }
    if depth == "summary" {
        return result.to_string();
    }
    let imports_json: Option<String> = conn
        .query_row(
            &format!(
                "SELECT imports_json FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            params![file_path],
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
                .query_map(params![file_path], |row| {
                    Ok(serde_json::json!(
                        { "line" : row.get::< _, i64 > (0) ?, "kind" : row.get::< _,
                        String > (1) ?, "text" : row.get::< _, String > (2) ? }
                    ))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    };
    let feature_flags: Vec<String> = rt
        .ctx
        .knowledge
        .get_feature_flags_for_file(&file_path)
        .unwrap_or_default();
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
    let out = result.to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}
/// Detect language from file extension. Inline helper for cli_ast_tdg
/// (avoids dragging touring-ast::Lang::detect into this hot path).
fn detect_language_from_ext(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" => "typescript",
        "tsx" => "typescript",
        "js" => "javascript",
        "jsx" => "javascript",
        "go" => "go",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" | "hh" => "cpp",
        "java" => "java",
        "kt" => "kotlin",
        "rb" => "ruby",
        "sh" | "bash" => "bash",
        _ => "unknown",
    }
}
/// Map per-file edit count → churn score in `[0.0, 1.0]`.
/// 0 edits → 1.0 (pristine), 50 edits → 0.5, 200 edits → ~0.2.
/// Asymptotic: never reaches 0 even for very high churn.
fn compute_churn_score(edit_count: i64) -> f64 {
    let n = edit_count.max(0) as f64;
    1.0 / (1.0 + (n / 50.0))
}
/// Skeleton-depth metadata for a file (alias for cli_ast_meta at skeleton depth).
///
/// Payload: `{"file_path": "..."}`
pub fn cli_ast_skeleton(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let mut enriched = payload.clone();
    if let Some(obj) = enriched.as_object_mut() {
        obj.insert("depth".to_string(), serde_json::json!("skeleton"));
    }
    cli_ast_meta(rt, &enriched)
}
/// Technical Debt Grade for a file — runs QualityPipeline + optional
/// Rust syn-based signals + computes [`TdgReport`] with letter grade.
///
/// Payload:
/// ```json
/// {"file_path": "path/to/file.rs", "grade_only": false}
/// ```
///
/// Returns:
/// - When `grade_only=true`: `{"file_path": "...", "grade": "B+"}`
/// - Otherwise: `{"file_path": "...", "language": "...", "tdg": {...}}`
///   where `tdg` is the full [`TdgReport::to_json`] output.
///
/// Reads source from disk (resolving relative to `project_root`). Pulls
/// per-file churn from `file_access_log` table when available; defaults
/// to `churn=1.0` (pristine) when no edit history exists.
///
/// [`TdgReport`]: touring_analysis::quality::TdgReport
/// [`TdgReport::to_json`]: touring_analysis::quality::TdgReport::to_json
pub fn cli_ast_tdg(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path_in = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let grade_only = payload
        .get("grade_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if file_path_in.is_empty() {
        return serde_json::json!({ "error" : "file_path required" }).to_string();
    }
    let file_path = normalize_to_relative(file_path_in, &rt.project_root);
    let abs_path = rt.project_root.join(&file_path);
    let source = match std::fs::read_to_string(&abs_path) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!(
                { "error" : format!("read failed: {e}"), "file_path" : file_path, }
            )
            .to_string();
        }
    };
    let language = detect_language_from_ext(&file_path);
    let pipeline = touring_analysis::quality::QualityPipeline::new(
        touring_analysis::engine::AnalysisConfig::standard(),
    );
    let report = pipeline.analyze_file(&file_path, &source, language);
    let rust_signals = if language == "rust" {
        touring_analysis::quality::RustQualitySignals::from_source(&source)
    } else {
        None
    };
    let edit_count: i64 = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_ACCESS_LOG
            ),
            params![file_path],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let churn = compute_churn_score(edit_count);
    let duplication = 1.0;
    let tdg = touring_analysis::quality::TdgReport::from_quality_report(
        &report,
        rust_signals.as_ref(),
        duplication,
        churn,
    );
    {
        let gl = tdg.grade_letter();
        if gl == "D" || gl == "F" {
            crate::shared::gate_metrics::record_diagnostic_tdg_emitted();
        }
    }
    if grade_only {
        return serde_json::json!(
            { "file_path" : file_path, "grade" : tdg.grade_letter(), }
        )
        .to_string();
    }
    serde_json::json!(
        { "file_path" : file_path, "language" : language, "edit_count" : edit_count,
        "tdg" : tdg.to_json(), }
    )
    .to_string()
}
/// Enriched blast radius: wiring integration score + cognitive signals.
///
/// Payload: `{"file_path": "..."}`
pub fn cli_ast_blast_enriched(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };
    let conn = rt.ctx.knowledge.conn_ref();
    let integration_score: f64 = conn
        .query_row(
            &format!(
                "SELECT integration_score FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_MODULE_ECOSYSTEM
            ),
            params![file_path],
            |row| row.get(0),
        )
        .unwrap_or(0.0);
    let pub_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT symbol_name) FROM {} WHERE module_file = ?1 AND visibility = 'public'",
                schema_guard::TABLE_WIRING_MAP
            ),
            params![file_path],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let consumer_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT consumer_file) FROM {} WHERE module_file = ?1 AND consumer_file IS NOT NULL",
                schema_guard::TABLE_WIRING_MAP
            ),
            params![file_path],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let cog = rt
        .ctx
        .knowledge
        .get_cognitive_enrichment(file_path)
        .unwrap_or(None);
    let (cognitive_score, complexity_signal, fan_in, fan_out, doc_signal) =
        cog.unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0));
    serde_json::json!(
        { "file_path" : file_path, "blast_radius" : pub_count, "consumer_count" :
        consumer_count, "integration_score" : integration_score, "cognitive_score" :
        cognitive_score, "complexity_signal" : complexity_signal, "fan_in_signal" :
        fan_in, "fan_out_signal" : fan_out, "doc_signal" : doc_signal }
    )
    .to_string()
}
/// `cli-ast-blast-cross-feature` — Compute cfg-gated pub item blast radius for a Rust file.
///
/// Scans the file for `#[cfg(...)]` public items and reports how many consumers
/// are potentially impacted when those cfg conditions hold (feature flags, OS targets, etc.).
///
/// Payload: `{"file_path": "src/lib.rs"}`
pub fn cli_ast_blast_cross_feature(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match payload.get("file_path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p,
        _ => {
            return serde_json::json!(
                { "error" : "file_path is required", "usage" :
                "cli-ast-blast-cross-feature {\"file_path\": \"src/lib.rs\"}" }
            )
            .to_string();
        }
    };
    let abs_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        rt.project_root
            .join(file_path)
            .to_string_lossy()
            .to_string()
    };
    let source = match std::fs::read_to_string(&abs_path) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!(
                { "file_path" : file_path, "error" : format!("read failed: {e}"), }
            )
            .to_string();
        }
    };
    match crate::ast_bridge::compute_blast_radius_cross_feature(&source, &abs_path, None) {
        Some(radius) => {
            if radius.gated_item_count > 0 {
                use touring_analysis::blast_radius::BlastWarning;
                let w = BlastWarning::CrossFeatureBlast {
                    features: radius.cfg_conditions.clone(),
                    gated_symbol_count: radius.gated_item_count,
                };
                let diag = w.to_diagnostic();
                tracing::warn!(
                    code = % diag.code, severity = % diag.severity, message = % diag
                    .message, file_path = % file_path, gated_item_count = radius
                    .gated_item_count, "B-320 CrossFeatureBlast emitted"
                );
            }
            serde_json::to_string(&radius)
                .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
        }
        None => serde_json::json!(
            { "file_path" : file_path, "cfg_analysis" : "skipped", "reason" :
            "non-Rust file — cfg detection is Rust-specific", }
        )
        .to_string(),
    }
}
