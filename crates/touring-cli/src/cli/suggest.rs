//! CLI suggestion handlers (`cli_suggest_*`, `cli_suggestion_*`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! LinUCB-bandit next-action/skill hints + suggestion-record lifecycle
//! (mark consumed, stats, list pending, GC) over the decompose tables. The
//! shared `ensure_decompose_tables` and `keyword_skill_match` helpers stay in
//! cli_handlers.rs (the latter retains its in-place `#[cfg(test)]` coverage).

use crate::cli::params::{str_opt, str_or, str_or_empty};
use crate::cli_handlers::{ensure_decompose_tables, keyword_skill_match};
use crate::runtime::HookRuntime;
use rusqlite::params;
use touring_intelligence::rl::bandit::extract_features_rich;

/// Select a bandit arm for a query-only decision.
///
/// Two defects fixed here on 04/08/2026, both proven by running
/// `touring suggest next`, which answered with a constant fallback for every
/// input:
///
/// 1. The caller read `rt.learning.bandit` directly. That field is materialised
///    lazily by `HookRuntime::get_bandit`, so reading it first always saw `None`
///    and the trained LinUCB was never consulted.
/// 2. The feature vector was 6 elements — `[query.len()/100, 0, 0, 0, 0, 0]` —
///    against a space of `FEATURE_DIM = 25`. Worse than the rank mismatch: in
///    the trained space slot 0 is the "file type is Python" one-hot, so that
///    wrote a string length into a categorical indicator.
///
/// There is no query-text feature in the 25-dim space, so a query-only call
/// contributes the neutral context and lets the bandit answer from what it has
/// learned generally. Enriching this space with query features is a separate,
/// deliberate change — silently reusing another feature's slot is not.
fn select_query_arm(rt: &mut HookRuntime) -> (usize, f64) {
    let features = extract_features_rich(
        "other", // file type: a query names no file
        0,       // file size
        0,       // session turn
        0,       // recent errors
        0,       // cila level
        None,    // quality score
        None,    // file risk
        None,    // session error count
        None,    // recent tool success rate
        None,    // hour of day
    );
    let slice = features.as_slice().unwrap_or(&[]);
    rt.get_bandit().select_arm(slice)
}

/// Suggests the next action for a query via the LinUCB bandit, returning the hint as JSON.
pub fn cli_suggest_next(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let (arm_idx, confidence) = select_query_arm(rt);
    // `query` is echoed, not consumed: the 25-dim space carries no query-text
    // feature, so the arm reflects learned context only. Echoing it keeps the
    // caller's input visible instead of silently discarding it, and makes the
    // gap measurable — see `select_query_arm`.
    serde_json::json!(
        { "suggested_action" : arm_idx, "confidence" : confidence, "source" :
        "linucb_bandit", "query" : query, "query_informed_selection" : false }
    )
    .to_string()
}
/// Suggests a relevant skill for a query via keyword matching, returning the match as JSON.
pub fn cli_suggest_skill(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let skills: Vec<serde_json::Value> = {
        // Same fix as `cli_suggest_next`: consult the lazily-materialised bandit
        // through `get_bandit()` with a correctly-ranked 25-dim vector, instead
        // of reading a `None` field with a 6-element one.
        let (arm_idx, confidence) = select_query_arm(rt);
        let arm_skills: Vec<(&str, &str, &str)> = match arm_idx {
            0 => {
                vec![
                    (
                        "touring index find",
                        "high",
                        "Find symbol definitions in the indexed codebase",
                    ),
                    (
                        "touring ast overview",
                        "medium",
                        "Get symbol overview for a file",
                    ),
                ]
            }
            1 => {
                vec![
                    (
                        "touring ast overview",
                        "high",
                        "AST symbol overview for target file",
                    ),
                    (
                        "touring index find",
                        "medium",
                        "Verify symbol existence before use",
                    ),
                ]
            }
            2 => {
                vec![
                    (
                        "touring gotcha match",
                        "high",
                        "Check known pitfalls for this file",
                    ),
                    (
                        "touring gotcha list",
                        "medium",
                        "List all active gotchas in project",
                    ),
                ]
            }
            3 => {
                vec![
                    (
                        "touring ast blast",
                        "high",
                        "Analyze blast radius before editing",
                    ),
                    (
                        "touring wiring score",
                        "medium",
                        "Check integration score for target",
                    ),
                ]
            }
            4 => {
                vec![
                    (
                        "touring wiring orphans",
                        "high",
                        "Find orphan pub symbols needing consumers",
                    ),
                    (
                        "touring index find",
                        "medium",
                        "Trace symbol usages across codebase",
                    ),
                ]
            }
            5 => {
                vec![
                    (
                        "touring ast overview",
                        "high",
                        "AST symbols for target file",
                    ),
                    (
                        "touring gotcha match",
                        "high",
                        "Known pitfalls for this file",
                    ),
                ]
            }
            6 => {
                vec![
                    (
                        "touring ast blast",
                        "high",
                        "Impact analysis before editing",
                    ),
                    (
                        "touring index find",
                        "medium",
                        "Verify symbols before referencing",
                    ),
                ]
            }
            7 => {
                vec![
                    (
                        "touring memory store",
                        "high",
                        "Persist learned pattern after success",
                    ),
                    (
                        "touring evolution insights",
                        "high",
                        "Review learned patterns and tool effectiveness",
                    ),
                    (
                        "touring wiring audit",
                        "medium",
                        "Full integration audit after changes",
                    ),
                ]
            }
            _ => {
                vec![
                    ("touring index find", "high", "Find symbol definitions"),
                    ("touring memory recall", "medium", "Recall past patterns"),
                ]
            }
        };
        let mut merged: Vec<serde_json::Value> = arm_skills
            .into_iter()
            .map(|(skill, relevance, description)| {
                serde_json::json!(
                    { "skill" : skill, "relevance" : relevance, "description" :
                    description, "arm_idx" : arm_idx, "confidence" : confidence, "source"
                    : "linucb_bandit" }
                )
            })
            .collect();

        // Keep the keyword match: it is the only QUERY-AWARE signal here, since
        // the 25-dim bandit space carries no query text. It used to sit in an
        // `else` branch reachable only when the bandit was absent — which, given
        // the `None`-field bug, was in practice the only branch that ever ran.
        // Merging keeps both signals rather than trading one for the other.
        let already = |list: &[serde_json::Value], name: &str| {
            list.iter()
                .any(|s| s.get("skill").and_then(|v| v.as_str()) == Some(name))
        };
        for candidate in keyword_skill_match(query) {
            let name = candidate
                .get("skill")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() || !already(&merged, &name) {
                merged.push(candidate);
            }
        }
        merged
    };
    serde_json::json!({ "query" : query, "skills" : skills, "count" : skills.len() }).to_string()
}
/// Insert an action suggestion (Pln3 bidirectional action flow).
///
/// Touring-observed conditions (stuck subtask, failure threshold, CILA L4+)
/// emit a suggestion row for CC to adopt via `TaskUpdate/TaskStop/PlanMode`
/// with `suggestion_ref` in the payload. CC consumption marks `consumed=1`.
///
/// `action_type` ∈ {"update", "stop", "plan_mode"}. Validation is by convention.
pub fn cli_suggest_action(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let action_type = str_or_empty(payload, "action_type");
    let target_task_id = str_or_empty(payload, "target_task_id");
    let target_subtask_id = str_opt(payload, "target_subtask_id");
    let reason = str_or_empty(payload, "reason");
    let evidence_json = payload
        .get("evidence_json")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());
    if action_type.is_empty() || target_task_id.is_empty() || reason.is_empty() {
        return serde_json::json!(
            { "inserted" : false, "reason" :
            "missing required field (action_type|target_task_id|reason)" }
        )
        .to_string();
    }
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let suggestion_id = format!(
        "sugg_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let now = chrono::Utc::now().to_rfc3339();
    let result = db
        .conn_ref()
        .execute(
            "INSERT INTO cc_action_suggestions (suggestion_id, action_type, target_task_id, target_subtask_id, reason, evidence_json, suggested_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                suggestion_id, action_type, target_task_id, target_subtask_id, reason,
                evidence_json, now
            ],
        );
    serde_json::json!(
        { "inserted" : result.is_ok(), "suggestion_id" : suggestion_id, "action_type" :
        action_type, "target_task_id" : target_task_id, "suggested_at" : now, }
    )
    .to_string()
}
/// Mark an action suggestion as consumed (Pln3 bidirectional action flow).
///
/// Called by hook handlers (`task-sync-post-update`, `task-sync-post-stop`,
/// `enter-plan-mode`) when CC acts with `suggestion_ref` in tool_input.
pub fn cli_suggestion_mark_consumed(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let suggestion_id = str_or_empty(payload, "suggestion_id");
    let consumed_action = str_or(payload, "consumed_action", "unknown");
    if suggestion_id.is_empty() {
        return serde_json::json!(
            { "marked" : false, "reason" : "missing suggestion_id" }
        )
        .to_string();
    }
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let now = chrono::Utc::now().to_rfc3339();
    let result = db
        .conn_ref()
        .execute(
            "UPDATE cc_action_suggestions SET consumed = 1, consumed_at = ?2, consumed_action = ?3 WHERE suggestion_id = ?1",
            params![suggestion_id, now, consumed_action],
        );
    let rows = result.unwrap_or(0);
    let mut early_reactivated = false;
    let mut acceptance_rate: f64 = 0.0;
    if rows > 0 {
        let action_type: Option<String> = db
            .conn_ref()
            .query_row(
                "SELECT action_type FROM cc_action_suggestions WHERE suggestion_id = ?1",
                params![suggestion_id],
                |r| r.get(0),
            )
            .ok();
        if let Some(ref at) = action_type {
            let was_deactivated: bool = db
                .conn_ref()
                .query_row(
                    "SELECT COUNT(*) FROM action_type_deactivation \
                     WHERE action_type = ?1 AND deactivated_until > datetime('now')",
                    params![at],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0)
                > 0;
            let _ = db.conn_ref().execute(
                "INSERT INTO action_type_deactivation \
                   (action_type, consecutive_ignores, acceptance_count, total_samples) \
                 VALUES (?1, 0, 1, 1) \
                 ON CONFLICT(action_type) DO UPDATE SET \
                   consecutive_ignores = 0, \
                   deactivated_until   = NULL, \
                   acceptance_count    = acceptance_count + 1, \
                   total_samples       = total_samples + 1",
                params![at],
            );
            let (acc, total): (i64, i64) = db
                .conn_ref()
                .query_row(
                    "SELECT acceptance_count, total_samples \
                     FROM action_type_deactivation WHERE action_type = ?1",
                    params![at],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )
                .unwrap_or((1, 1));
            acceptance_rate = acc as f64 / total.max(1) as f64;
            if was_deactivated && acceptance_rate > 0.8 && total >= 5 {
                early_reactivated = true;
            }
            #[cfg(not(test))]
            {
                tracing::debug!(
                    action_type = % at, acceptance_rate,
                    "R6: suggestion accepted — positive RL signal"
                );
            }
        }
    }
    serde_json::json!(
        { "marked" : rows > 0, "suggestion_id" : suggestion_id, "consumed_action" :
        consumed_action, "rows_updated" : rows, "acceptance_rate" : acceptance_rate,
        "early_reactivated" : early_reactivated, }
    )
    .to_string()
}
/// Pln3 R6: Query acceptance statistics per action_type (observability).
///
/// Returns a JSON object with `stats` (array of per-action_type rows) and
/// `total_action_types` count. Each row includes `acceptance_rate` computed
/// as `acceptance_count / total_samples` (0.0 when no samples).
pub fn cli_suggestion_stats(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let conn = db.conn_ref();
    let mut stmt = match conn.prepare(
        "SELECT action_type, consecutive_ignores, acceptance_count, total_samples, \
                deactivated_until, \
                CASE WHEN total_samples > 0 \
                     THEN CAST(acceptance_count AS REAL) / total_samples \
                     ELSE 0.0 END AS rate \
         FROM action_type_deactivation ORDER BY action_type",
    ) {
        Ok(s) => s,
        Err(_) => {
            return serde_json::json!({ "stats" : [], "total_action_types" : 0 }).to_string();
        }
    };
    let rows: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![], |row| {
            Ok(serde_json::json!(
                { "action_type" : row.get::< _, String > (0) ?,
                "consecutive_ignores" : row.get::< _, i64 > (1) ?,
                "acceptance_count" : row.get::< _, i64 > (2) ?, "total_samples" :
                row.get::< _, i64 > (3) ?, "deactivated_until" : row.get::< _,
                Option < String >> (4) ?, "acceptance_rate" : row.get::< _, f64 >
                (5) ?, }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    let count = rows.len();
    serde_json::json!({ "stats" : rows, "total_action_types" : count }).to_string()
}
/// List pending action suggestions, optionally filtered by action_type.
///
/// Returns JSON array of pending (non-consumed) suggestions for the digest
/// pipeline to surface into `additionalContext`.
pub fn cli_suggestion_list_pending(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let action_type = payload.get("action_type").and_then(|v| v.as_str());
    let limit = payload
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(10)
        .min(50);
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let conn = db.conn_ref();
    let sql = if action_type.is_some() {
        "SELECT suggestion_id, action_type, target_task_id, target_subtask_id, reason, evidence_json, suggested_at \
         FROM cc_action_suggestions WHERE consumed = 0 AND action_type = ?1 ORDER BY suggested_at ASC LIMIT ?2"
    } else {
        "SELECT suggestion_id, action_type, target_task_id, target_subtask_id, reason, evidence_json, suggested_at \
         FROM cc_action_suggestions WHERE consumed = 0 ORDER BY suggested_at ASC LIMIT ?1"
    };
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => {
            return serde_json::json!({ "suggestions" : [], "count" : 0 }).to_string();
        }
    };
    let row_mapper = |row: &rusqlite::Row<'_>| -> rusqlite::Result<serde_json::Value> {
        Ok(serde_json::json!(
            { "suggestion_id" : row.get::< _, String > (0) ?, "action_type" : row
            .get::< _, String > (1) ?, "target_task_id" : row.get::< _, String > (2)
            ?, "target_subtask_id" : row.get::< _, Option < String >> (3) ?, "reason"
            : row.get::< _, String > (4) ?, "evidence_json" : row.get::< _, String >
            (5) ?, "suggested_at" : row.get::< _, String > (6) ?, }
        ))
    };
    let suggestions: Vec<serde_json::Value> = if let Some(at) = action_type {
        stmt.query_map(params![at, limit], row_mapper)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    } else {
        stmt.query_map(params![limit], row_mapper)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };
    let count = suggestions.len();
    serde_json::json!({ "suggestions" : suggestions, "count" : count, }).to_string()
}
/// Pln3 R4: Garbage-collect consumed suggestions older than `retention_days` days.
///
/// Returns count of deleted rows. Safe to call on daemon startup or periodically —
/// idempotent and bounded (only deletes consumed rows with a recorded `consumed_at`).
pub fn cli_suggestions_gc(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let retention_days = payload
        .get("retention_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(30)
        .max(1);
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let result = db.conn_ref().execute(
        "DELETE FROM cc_action_suggestions \
         WHERE consumed = 1 AND consumed_at IS NOT NULL \
         AND consumed_at < datetime('now', ?1)",
        params![format!("-{retention_days} days")],
    );
    let deleted = result.unwrap_or(0);
    serde_json::json!({ "deleted" : deleted, "retention_days" : retention_days }).to_string()
}
