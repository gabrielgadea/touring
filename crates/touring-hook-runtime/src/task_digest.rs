//! Touring → Claude Code task digest (bidirectional task flow).
//!
//! Surfaces Touring-originated tasks (origin != `claude-code`, `mirrored_to_cc = 0`)
//! so Claude Code can adopt them via `TaskCreate(..., external_ref=<task_id>)`.
//! When CC adopts a task, the `task-sync-post-create` hook detects `external_ref`
//! and calls `cli_decompose_mark_mirrored` to close the loop — breaking any
//! potential CC↔Touring cycle.
//!
//! Wired from `instructions_loaded::run_returning` so the digest appears once
//! per session at CLAUDE.md load time. Budget: <5ms (single SQLite query + light
//! formatting).
//!
//! Added: 2026-04-13 (Pln2 bidirectional task flow)

use crate::runtime::HookRuntime;
use rusqlite::params;

/// Max tasks surfaced per digest (prevents overflow when many external tasks queued).
const DIGEST_LIMIT: usize = 5;

/// Truncate `s` to at most `max` bytes (ASCII-safe).
#[inline]
fn truncate(s: &str, max: usize) -> &str {
    if s.len() > max { &s[..max] } else { s }
}

/// Format a single pending-task row as a bullet entry.
#[inline]
fn format_task_bullet(id: &str, desc: &str, origin: &str) -> String {
    format!("[{id}] {} ({origin})", truncate(desc, 60))
}

/// Query pending Touring-originated tasks and format as a human-readable digest
/// suitable for injection into `additionalContext`.
///
/// Returns `None` if:
///   - `task_decompositions` table missing (no tasks yet)
///   - No rows match the filter (all CC-originated or all already mirrored)
///   - SQLite query fails (graceful degradation)
///
/// Output format (single line with bullet separator for compactness):
/// ```text
/// Touring tasks (N pending): [id1] subject1 (origin) • [id2] subject2 (origin) • ...
/// Adopt via TaskCreate(subject="...", external_ref="<task_id>")
/// ```
pub fn digest_pending_tasks(runtime: &HookRuntime) -> Option<String> {
    let conn = runtime.ctx.knowledge.conn_ref();

    // Guard: only query if the schema columns exist. Silences error on fresh DBs
    // that haven't run cli_decompose_create yet (ensure_decompose_tables + ALTER).
    let schema_ready: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('task_decompositions') WHERE name IN ('origin','mirrored_to_cc')",
            rusqlite::params![],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if schema_ready < 2 {
        return None;
    }

    let mut stmt = conn
        .prepare(
            "SELECT task_id, description, origin \
             FROM task_decompositions \
             WHERE mirrored_to_cc = 0 \
               AND status IN ('created', 'active', 'ready') \
             ORDER BY created_at ASC \
             LIMIT ?1",
        )
        .ok()?;
    let rows = stmt
        .query_map(rusqlite::params![DIGEST_LIMIT as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .ok()?;
    let tasks: Vec<(String, String, String)> = rows.filter_map(|r| r.ok()).collect();
    if tasks.is_empty() {
        return None;
    }

    let n = tasks.len();
    let items: Vec<String> = tasks
        .iter()
        .map(|(id, desc, origin)| format_task_bullet(id, desc, origin))
        .collect();

    let (first_id, first_desc, _) = tasks.first().expect("tasks non-empty (checked above)");
    Some(format!(
        "Touring tasks ({n} pending): {} | Adopt via TaskCreate(subject=\"{}\", external_ref=\"{first_id}\") to close bidirectional loop",
        items.join(" • "),
        truncate(first_desc, 40),
    ))
}

/// Max pending action suggestions surfaced per digest call.
const SUGGESTION_DIGEST_LIMIT: usize = 5;

// ---------------------------------------------------------------------------
// R5 helpers — deactivation check + surface_count bookkeeping (CC ≤ 4 each)
// ---------------------------------------------------------------------------

/// Returns `true` if a table named `table` exists in the connection's schema.
fn table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        params![table],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/// Returns `true` if the `action_type_deactivation` table exists AND the given
/// `action_type` has an active cooldown (`deactivated_until > now`).
fn is_action_type_deactivated(conn: &rusqlite::Connection, action_type: &str) -> bool {
    if !table_exists(conn, "action_type_deactivation") {
        return false;
    }
    conn.query_row(
        "SELECT COUNT(*) FROM action_type_deactivation \
         WHERE action_type = ?1 AND deactivated_until > datetime('now')",
        params![action_type],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/// Increment `surface_count` for each surfaced suggestion, then activate a
/// cooldown for `action_type` when ≥ 3 non-consumed rows reach `surface_count ≥ 3`
/// (RL ignore-streak detected).
///
/// Pln3 R6: cooldown duration is adaptive:
/// - Default: +24 hours
/// - Extended to +72 hours when `total_samples >= 10` AND acceptance_rate < 0.2
///   (chronic non-responder pattern — harsher backoff applied).
///
/// Also increments `total_samples` on each surface call so the acceptance rate
/// denominator reflects all observed opportunities, not just consumed ones.
fn update_surface_counts_and_maybe_deactivate(
    conn: &rusqlite::Connection,
    action_type: &str,
    suggestion_ids: &[&str],
) {
    for sid in suggestion_ids {
        let _ = conn.execute(
            "UPDATE cc_action_suggestions SET surface_count = surface_count + 1 \
             WHERE suggestion_id = ?1",
            params![sid],
        );
    }
    // Deactivation gate: only proceed if the table exists.
    if !table_exists(conn, "action_type_deactivation") {
        return;
    }

    // R6: increment total_samples for this surface event (each surface = one sample).
    let _ = conn.execute(
        "INSERT INTO action_type_deactivation \
           (action_type, consecutive_ignores, acceptance_count, total_samples) \
         VALUES (?1, 0, 0, 1) \
         ON CONFLICT(action_type) DO UPDATE SET \
           total_samples = total_samples + 1",
        params![action_type],
    );

    let ignore_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cc_action_suggestions \
             WHERE action_type = ?1 AND consumed = 0 AND surface_count >= 3",
            params![action_type],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if ignore_count >= 3 {
        // R6: determine cooldown duration — extended if chronic low-acceptance pattern.
        let cooldown = compute_deactivation_cooldown(conn, action_type);
        let _ = conn.execute(
            &format!(
                "INSERT INTO action_type_deactivation \
                   (action_type, consecutive_ignores, deactivated_until) \
                 VALUES (?1, ?2, datetime('now', '{cooldown}')) \
                 ON CONFLICT(action_type) DO UPDATE SET \
                   consecutive_ignores = consecutive_ignores + 1, \
                   deactivated_until   = datetime('now', '{cooldown}')"
            ),
            params![action_type, ignore_count],
        );
    }
}

/// Pln3 R6: compute the deactivation cooldown string for SQLite `datetime('now', ...)`.
///
/// Returns `'+72 hours'` when `total_samples >= 10` AND `acceptance_rate < 0.2`
/// (chronic low-acceptance), otherwise returns `'+24 hours'` (default).
fn compute_deactivation_cooldown(conn: &rusqlite::Connection, action_type: &str) -> &'static str {
    let result: Option<(i64, i64)> = conn
        .query_row(
            "SELECT acceptance_count, total_samples \
             FROM action_type_deactivation WHERE action_type = ?1",
            params![action_type],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .ok();
    if let Some((acc, total)) = result {
        if total >= 10 {
            let rate = acc as f64 / total as f64;
            if rate < 0.2 {
                return "+72 hours";
            }
        }
    }
    "+24 hours"
}

/// Query pending `cc_action_suggestions` for a given `action_type` and format
/// as a human-readable digest suitable for injection into `additionalContext`.
///
/// Returns `None` if:
///   - `cc_action_suggestions` table is absent (schema not yet initialised)
///   - No unconsumed suggestions exist for the requested `action_type`
///   - SQLite query fails (graceful degradation)
///
/// Output format:
/// ```text
/// Touring suggests update on N tasks: • [sugg_id] reason (task=task_id). Use TaskUpdate with suggestion_ref=<id> to consume.
/// ```
pub fn digest_pending_action_suggestions(
    runtime: &HookRuntime,
    action_type: &str,
) -> Option<String> {
    let conn = runtime.ctx.knowledge.conn_ref();

    // Guard: suggestions table must exist.
    let table_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type='table' AND name='cc_action_suggestions'",
            params![],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if table_exists == 0 {
        return None;
    }

    // Pln3 R5: skip surfacing if action_type is in 24h cooldown.
    if is_action_type_deactivated(conn, action_type) {
        return None;
    }

    // Pln3 R3: rank by priority (stop > update > plan_mode) then most-recent first.
    let suggestions = query_ranked_suggestions(conn, action_type)?;

    // Pln3 R5: track surface counts + maybe trigger 24h deactivation.
    let ids: Vec<&str> = suggestions.iter().map(|(id, _, _)| id.as_str()).collect();
    update_surface_counts_and_maybe_deactivate(conn, action_type, &ids);

    let n = suggestions.len();
    let items: Vec<String> = suggestions
        .iter()
        .map(|(sugg_id, task_id, reason)| {
            let short_reason = if reason.len() > 80 {
                &reason[..80]
            } else {
                reason.as_str()
            };
            format!("[{sugg_id}] {short_reason} (task={task_id})")
        })
        .collect();

    Some(format!(
        "Touring suggests {action_type} on {n} task(s): {} | \
         Use TaskUpdate with suggestion_ref=<id> to consume",
        items.join(" • ")
    ))
}

/// Query ranked pending suggestions for a given action_type (R3: stop > update > plan_mode).
fn query_ranked_suggestions(
    conn: &rusqlite::Connection,
    action_type: &str,
) -> Option<Vec<(String, String, String)>> {
    let mut stmt = conn
        .prepare(
            "SELECT suggestion_id, target_task_id, reason \
             FROM cc_action_suggestions \
             WHERE consumed = 0 AND action_type = ?1 \
             ORDER BY \
               CASE action_type \
                 WHEN 'stop'      THEN 3 \
                 WHEN 'update'    THEN 2 \
                 WHEN 'plan_mode' THEN 1 \
                 ELSE 0 \
               END DESC, \
               suggested_at DESC \
             LIMIT ?2",
        )
        .ok()?;
    let rows = stmt
        .query_map(
            params![action_type, SUGGESTION_DIGEST_LIMIT as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .ok()?;
    let v: Vec<(String, String, String)> = rows.filter_map(|r| r.ok()).collect();
    if v.is_empty() { None } else { Some(v) }
}
