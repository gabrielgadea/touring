//! CLI session command handlers.
//!
//! Handlers for: cli-session-list, cli-session-start, cli-session-checkpoint, cli-session-assess

use crate::runtime::HookRuntime;
use rusqlite::params;
use touring_analysis::e2e::schema_guard;
use touring_foundation::checkpoint::CheckpointSettingsFingerprint;

/// Builds a CheckpointSettingsFingerprint with embedding provider info from env vars.
/// This ensures that changing TOURING_EMBEDDING_PROVIDER or TOURING_EMBEDDING_MODEL
/// invalidates existing session checkpoints (D18 requirement).
fn build_fingerprint_with_embedding(
    base: CheckpointSettingsFingerprint,
) -> CheckpointSettingsFingerprint {
    let provider = std::env::var("TOURING_EMBEDDING_PROVIDER").ok();
    let model = std::env::var("TOURING_EMBEDDING_MODEL").ok();
    match (provider, model) {
        (Some(p), m) => base.with_embedding_info(&p, m.as_deref()),
        (None, _) => base, // No embedding configured — fingerprint unchanged
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Lists recorded work sessions as JSON.
pub fn cli_session_list(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;

    // Ensure table exists (idempotent)
    let _ = db.conn_ref().execute_batch(
        "CREATE TABLE IF NOT EXISTS cli_sessions (
            session_id TEXT PRIMARY KEY,
            task_type TEXT NOT NULL DEFAULT 'general',
            objective TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    );

    let sessions: Vec<serde_json::Value> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT session_id, task_type, objective, created_at, updated_at FROM cli_sessions ORDER BY updated_at DESC LIMIT 20",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("session list prepare failed: {}", e);
                return serde_json::json!({"sessions": [], "count": 0, "error": format!("{}", e)}).to_string();
            }
        };
        stmt.query_map([], |r| {
            Ok(serde_json::json!({
                "session_id": r.get::<_, String>(0)?,
                "task_type": r.get::<_, String>(1)?,
                "objective": r.get::<_, String>(2)?,
                "created_at": r.get::<_, String>(3)?,
                "updated_at": r.get::<_, String>(4)?
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    serde_json::json!({
        "sessions": sessions,
        "count": sessions.len()
    })
    .to_string()
}

/// Starts a new work session with the given id, type, and objective, returning its record as JSON.
pub fn cli_session_start(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let task_type = payload
        .get("task_type")
        .and_then(|v| v.as_str())
        .unwrap_or("general");
    let objective = payload
        .get("objective")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // ─────────────────────────────────────────────────────────────────────────────
    // D18: Read stored fingerprint BEFORE any rt mutable borrows begin.
    // We extract knowledge_conn as a separate reference so we can drop it explicitly
    // before the rt phase starts.
    // ─────────────────────────────────────────────────────────────────────────────
    let knowledge_conn = &rt.ctx.knowledge;
    let now = chrono::Utc::now().to_rfc3339();

    // DB operations — this reference is dropped at the end of this block
    let result = {
        // Ensure table exists with config_fingerprint column (idempotent)
        let _ = knowledge_conn.conn_ref().execute_batch(
            "CREATE TABLE IF NOT EXISTS cli_sessions (
                session_id TEXT PRIMARY KEY,
                task_type TEXT NOT NULL DEFAULT 'general',
                objective TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                config_fingerprint TEXT
            )",
        );

        // Upsert session
        knowledge_conn.conn_ref().execute(
            "INSERT INTO cli_sessions (session_id, task_type, objective, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(session_id) DO UPDATE SET task_type=?2, objective=?3, updated_at=?4",
            params![session_id, task_type, objective, now],
        )
    };
    if let Err(e) = &result {
        tracing::debug!("session start INSERT failed: {}", e);
    }

    // D18: Compute fingerprint and compare — uses separate query, not the stored db reference.
    // This scope ensures the knowledge_conn borrow is fully released before rt mutable phase.
    let config_compatibility = {
        let stored: Option<String> = knowledge_conn
            .conn_ref()
            .query_row(
                "SELECT config_fingerprint FROM cli_sessions WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .ok();

        let current_fp = build_fingerprint_with_embedding(
            CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter")),
        );

        if let Some(stored_json) = stored {
            if let Ok(stored_fp) =
                serde_json::from_str::<CheckpointSettingsFingerprint>(&stored_json)
            {
                let (compatible, impact) = current_fp.is_compatible_with(&stored_fp);
                let description = current_fp.describe_change(&stored_fp);
                // Persist updated fingerprint
                let fp_json = serde_json::to_string(&current_fp).unwrap_or_default();
                let _ = knowledge_conn.conn_ref().execute(
                    "UPDATE cli_sessions SET config_fingerprint = ?1 WHERE session_id = ?2",
                    params![fp_json, session_id],
                );
                serde_json::json!({
                    "compatible": compatible,
                    "impact": format!("{:?}", impact),
                    "description": description,
                    "is_first_session": false
                })
            } else {
                let current_fp =
                    build_fingerprint_with_embedding(CheckpointSettingsFingerprint::symmetric(
                        "SemanticChunker",
                        Some("tree-sitter"),
                    ));
                let fp_json = serde_json::to_string(&current_fp).unwrap_or_default();
                let _ = knowledge_conn.conn_ref().execute(
                    "UPDATE cli_sessions SET config_fingerprint = ?1 WHERE session_id = ?2",
                    params![fp_json, session_id],
                );
                serde_json::json!({
                    "compatible": false,
                    "impact": "BreakingMajor",
                    "description": "corrupted stored fingerprint — treating as first session",
                    "is_first_session": true
                })
            }
        } else {
            // First session — persist initial fingerprint
            let current_fp = build_fingerprint_with_embedding(
                CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter")),
            );
            let fp_json = serde_json::to_string(&current_fp).unwrap_or_default();
            let _ = knowledge_conn.conn_ref().execute(
                "UPDATE cli_sessions SET config_fingerprint = ?1 WHERE session_id = ?2",
                params![fp_json, session_id],
            );
            serde_json::json!({
                "compatible": true,
                "impact": "None",
                "description": "first session — no prior fingerprint",
                "is_first_session": true
            })
        }
    };
    // knowledge_conn borrow ends here — db reference is fully released

    // Record in knowledge
    let _ = rt.ctx.knowledge.record_access("session", session_id);

    // P4.4: Reset AutoSaveHook exchange counter on session-start (fresh session).
    rt.auto_save.reset();

    // S1: Initialize cognitive runtime before first post_tool hook fires.
    // This ensures cognitive_ref() is Some before update_cognitive_engine is called.
    rt.init_cognitive();

    // S2: Spawn cognitive background tasks (safe to call multiple times — internal guard).
    rt.spawn_cognitive_background_tasks();

    // F1: Subscribe cognitive engine to CortexDispatcher broadcast for drift detection.
    rt.subscribe_to_cortex_dispatcher();

    // R6: Bootstrap EMA reward from 0.06 to 0.2 before any real RL rewards.
    // Reset warmup flag first so inject_warmup_reward fires on every fresh session.
    let _ = rt.learning.online_rl.as_mut().map(|rl| {
        rl.reset_warmup_session();
        rl.inject_warmup_reward()
    });

    serde_json::json!({
        "session_id": session_id,
        "task_type": task_type,
        "objective": objective,
        "created_at": now,
        "persisted": result.is_ok(),
        "config_compatibility": config_compatibility
    })
    .to_string()
}

/// Records a checkpoint snapshot of a session's progress, returning the checkpoint result as JSON.
pub fn cli_session_checkpoint(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let checkpoint_id = payload
        .get("checkpoint_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let data = payload.get("data").and_then(|v| v.as_str()).unwrap_or("");

    let db = &rt.ctx.knowledge;
    let now = chrono::Utc::now().to_rfc3339();

    // Ensure table exists (idempotent)
    let _ = db.conn_ref().execute_batch(
        "CREATE TABLE IF NOT EXISTS cli_checkpoints (
            checkpoint_id TEXT NOT NULL,
            session_id TEXT NOT NULL DEFAULT 'default',
            data TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            PRIMARY KEY (session_id, checkpoint_id)
        )",
    );

    let result = db.conn_ref().execute(
        "INSERT OR REPLACE INTO cli_checkpoints (checkpoint_id, session_id, data, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![checkpoint_id, session_id, data, now],
    );
    if let Err(e) = &result {
        tracing::debug!("session checkpoint INSERT failed: {}", e);
    }

    serde_json::json!({
        "checkpoint_id": checkpoint_id,
        "session_id": session_id,
        "data": data,
        "created_at": now,
        "persisted": result.is_ok()
    })
    .to_string()
}

/// Assesses a session's progress and quality, returning the assessment as JSON.
pub fn cli_session_assess(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("current");

    let db = &rt.ctx.knowledge;

    // Get session stats
    let edit_count: i64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE edited_at > datetime('now', '-1 hours')",
                schema_guard::TABLE_EDIT_HISTORY
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let bash_count: i64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', '-1 hours')",
                schema_guard::TABLE_BASH_OUTCOMES
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let gotcha_hits: i64 = db
        .conn_ref()
        .query_row("SELECT COALESCE(SUM(hit_count), 0) FROM gotchas", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    // EC49: First caller of CodeHealthReport::to_summary_line() — runs a lightweight
    // health analysis (hook_path config) and includes the compact summary in the
    // assess output. Format: "HEALTHY 0.87 [wiring:0.95 quality:0.83] 124ms"
    let health = touring_analysis::AnalysisPipeline::new(
        db.conn_ref(),
        touring_analysis::engine::AnalysisConfig::hook_path(),
    )
    .run(rt.project_root.to_str().unwrap_or(""));
    let health_summary = health.to_summary_line();

    serde_json::json!({
        "session_id": session_id,
        "assessed_at": chrono::Utc::now().to_rfc3339(),
        "metrics": {
            "edits_last_hour": edit_count,
            "bash_commands_last_hour": bash_count,
            "gotcha_hits_total": gotcha_hits
        },
        "quality_score": if edit_count > 0 { 1.0 } else { 0.0 },
        "health_summary": health_summary,
    })
    .to_string()
}
