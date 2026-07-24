//! CLI gotcha handlers (`cli_gotcha_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::cli::params::{str_or, str_or_empty};
use crate::cli_handlers::GotchaEntry;
use crate::knowledge::Gotcha;
use crate::runtime::HookRuntime;
use rusqlite::params;
use touring_analysis::e2e::schema_guard;

/// Lists recorded gotcha entries (known pitfalls) from the gotcha database as JSON.
pub fn cli_gotcha_list(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let gotchas: Vec<GotchaEntry> = rt
        .ctx
        .knowledge
        .list_gotchas()
        .into_iter()
        .map(|g: Gotcha| {
            let resolved = rt
                .ctx
                .knowledge
                .conn_ref()
                .query_row(
                    "SELECT resolved_at FROM gotchas WHERE id = ?1 AND resolved_at IS NOT NULL",
                    params![g.id],
                    |_r| Ok(true),
                )
                .unwrap_or(false);
            GotchaEntry {
                id: g.id,
                pattern: g.pattern,
                gotcha: g.gotcha,
                severity: g.severity,
                hit_count: g.hit_count,
                prevented_errors: g.prevented_errors,
                decay_score: None,
                resolved,
            }
        })
        .collect();
    serde_json::to_string(&gotchas)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
// Carve R (2026-06-10): runtime-service handler moved to touring-hook-runtime::ceg_impls
// (it is a pure HookRuntime capability); re-exported at the historical path.
pub use touring_hook_runtime::ceg_impls::cli_gotcha_add;
/// Matches a file or context against the gotcha database, returning applicable pitfalls as JSON.
pub fn cli_gotcha_match(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = str_or_empty(payload, "file_path");
    if file_path.is_empty() {
        return serde_json::json!({ "matches" : [], "count" : 0 }).to_string();
    }
    let gotchas: Vec<GotchaEntry> = rt
        .ctx
        .knowledge
        .get_gotchas_for_file(file_path)
        .into_iter()
        .map(|g: Gotcha| GotchaEntry {
            id: g.id,
            pattern: g.pattern,
            gotcha: g.gotcha,
            severity: g.severity,
            hit_count: g.hit_count,
            prevented_errors: g.prevented_errors,
            decay_score: None,
            resolved: false,
        })
        .collect();
    let count = gotchas.len();
    serde_json::json!({ "file_path" : file_path, "matches" : gotchas, "count" : count }).to_string()
}
/// Reports aggregate gotcha statistics (total, resolved, and unresolved counts) as JSON.
pub fn cli_gotcha_stats(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let (total, hits, prevented) = rt.ctx.knowledge.gotcha_stats();
    let resolved: i64 = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM gotchas WHERE resolved_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    serde_json::json!(
        { "total" : total, "resolved" : resolved, "unresolved" : total as i64 - resolved,
        "total_hits" : hits, "total_prevented" : prevented }
    )
    .to_string()
}
/// Wave Q3: Sync gotchas from YAML rule library to SQLite cache.
///
/// Payload: `{"dir": "/path/to/gotchas/"}` (default: `~/.claude/rust/docs/gotchas/`)
pub fn cli_gotcha_sync(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    // Productization Fase 0: the gotcha library follows the canonical workspace
    // (`TOURING_WORKSPACE_ROOT` override → historical global default).
    let default_dir = std::env::var("TOURING_WORKSPACE_ROOT")
        .map(|r| format!("{}/docs/gotchas", r.trim_end_matches('/')))
        .unwrap_or_else(|_| "/home/gabrielgadea/projects/touring/docs/gotchas".to_string());
    let dir_str = str_or(payload, "dir", &default_dir);
    let dir = std::path::Path::new(dir_str);
    if !dir.is_dir() {
        return serde_json::json!(
            { "error" : format!("dir '{dir_str}' is not a directory") }
        )
        .to_string();
    }
    let report = crate::gotcha_loader::sync_to_sqlite(rt, dir);
    let mut out = serde_json::to_value(&report).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = out.as_object_mut() {
        obj.insert("dir".to_string(), serde_json::json!(dir_str));
    }
    out.to_string()
}
/// Wave Q3: Bootstrap from existing SQLite gotchas → YAML files (one-shot
/// reverse migration). Useful when adopting the YAML workflow on a project
/// that already has gotchas in the cache.
///
/// Payload: `{"output_dir": "/path/to/output"}` (required)
///
/// Returns: count of YAML files written + list of generated paths.
pub fn cli_gotcha_init(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let output_dir = str_or_empty(payload, "output_dir");
    if output_dir.is_empty() {
        return serde_json::json!({ "error" : "output_dir required" }).to_string();
    }
    let out_path = std::path::Path::new(output_dir);
    if let Err(e) = std::fs::create_dir_all(out_path) {
        return serde_json::json!({ "error" : format!("create_dir_all failed: {e}") }).to_string();
    }
    let conn = rt.ctx.knowledge.conn_ref();
    let sql = format!(
        "SELECT id, pattern, gotcha, severity, language FROM {} ORDER BY id",
        schema_guard::TABLE_GOTCHAS
    );
    let rows: Vec<(i64, String, String, String, Option<String>)> = match conn.prepare(&sql) {
        Ok(mut stmt) => stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default(),
        Err(e) => {
            return serde_json::json!({ "error" : format!("query failed: {e}") }).to_string();
        }
    };
    let mut written: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for (id, pattern, gotcha, severity, language) in rows {
        let lang = language.unwrap_or_else(|| "multi-lang".to_string());
        let stable_id = format!("{lang}:imported-{id}");
        let yaml = format!(
            "id: {stable_id}\nlanguage: {lang}\npattern: |\n  {p}\ndescription: |\n  {d}\nseverity: {severity}\nmetadata:\n  imported_from: sqlite\n  imported_id: {id}\n",
            p = pattern.replace('\n', "\n  "),
            d = gotcha.replace('\n', "\n  "),
        );
        let safe_name = stable_id.replace([':', '/'], "-");
        let file_path = out_path.join(format!("{safe_name}.yaml"));
        match std::fs::write(&file_path, &yaml) {
            Ok(()) => written.push(file_path.display().to_string()),
            Err(e) => failed.push(format!("{}: {e}", file_path.display())),
        }
    }
    serde_json::json!(
        { "output_dir" : output_dir, "written_count" : written.len(), "written_files" :
        written, "failed" : failed, }
    )
    .to_string()
}
