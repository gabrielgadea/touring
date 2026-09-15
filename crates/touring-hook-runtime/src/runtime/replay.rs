//! Offline replay core — the outcome corpus feeds the OnlineRL engine.
//!
//! P1 (wave elos-exponenciais, 29/08/2026): extracted from the CLI handler so
//! BOTH surfaces share one implementation — `touring learning replay` (manual
//! backfill/inspection) and the session-start incremental drain (automatic by
//! construction: the corpus can never silently accumulate again). The pattern
//! is d3rlpy's `fit(dataset)` → `fit_online(env)`: offline pretraining and the
//! live per-tool trickle land on the SAME engine via the SAME
//! `process_immediate_reward` path.

use crate::hook_runtime::HookRuntime;
use crate::runtime::traits::OnlineRLOps;
use std::path::{Path, PathBuf};

/// Durable cursor path — consume-once semantics for the outcome corpus.
fn cursor_path(project_root: &Path) -> PathBuf {
    project_root.join(".claude/touring/learning_replay_cursor.json")
}

/// Read the durable cursor: `(last_rowid, replayed_total, resumed_from_zero)`.
///
/// Same contract as `code_mode_arm.json`: an unreadable file yields zeros —
/// and the replay restarts DECLARING it (re-applied TD updates converge to the
/// same point, so restarting is safe; hiding the restart would not be, E9).
pub fn cursor_read(project_root: &Path) -> (i64, u64, bool) {
    let path = cursor_path(project_root);
    if !path.exists() {
        return (0, 0, false);
    }
    match std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
    {
        Some(v) => (
            v.pointer("/last_rowid")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            v.pointer("/replayed_total")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            false,
        ),
        None => (0, 0, true),
    }
}

/// Rewarded outcomes not yet consumed by the replay. `None` when memory.db
/// cannot be opened — absence displayed, never coerced to zero (E4).
pub fn corpus_pending(project_root: &Path, last_rowid: i64) -> Option<i64> {
    let db = touring_foundation::TouringConfig::memory_db_canonical(project_root);
    let conn =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?;
    conn.query_row(
        "SELECT COUNT(*) FROM memory_entries WHERE outcome_reward IS NOT NULL AND rowid > ?1",
        rusqlite::params![last_rowid],
        |r| r.get(0),
    )
    .ok()
}

/// Replay the rewarded-outcome corpus into the runtime's OnlineRL engine.
///
/// Returns the same JSON contract the CLI handler exposes. `dry_run` counts
/// without touching the engine or the cursor. Errors come back as
/// `{"error": …, "hint": …}` — the message teaches the correction (A5).
pub fn replay_outcomes_into(
    rt: &mut HookRuntime,
    limit: usize,
    dry_run: bool,
) -> serde_json::Value {
    let limit = limit.clamp(1, 50_000) as i64;
    let (last_rowid, replayed_total, resumed_from_zero) = cursor_read(&rt.project_root);
    let db = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    let conn = match rusqlite::Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({
                "error": format!("memory.db open failed: {e}"),
                "hint": "o replay lê outcomes de <projeto>/.claude/touring/memory.db",
            });
        }
    };
    let mut rows: Vec<(i64, String, f64)> = match conn.prepare(
        "SELECT rowid, key, outcome_reward FROM memory_entries \
         WHERE outcome_reward IS NOT NULL AND rowid > ?1 ORDER BY rowid LIMIT ?2",
    ) {
        Ok(mut stmt) => stmt
            .query_map(rusqlite::params![last_rowid, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map(|it| it.filter_map(|r| r.ok()).collect())
            .unwrap_or_default(),
        Err(e) => {
            return serde_json::json!({ "error": format!("query failed: {e}") });
        }
    };
    if rt.learning.online_rl.is_none() {
        return serde_json::json!({
            "error": "online_rl engine unavailable in this runtime",
            "hint": "rode via daemon (`touring learning replay`), não em cache-only",
        });
    }
    let max_rowid = rows.iter().map(|(r, _, _)| *r).max().unwrap_or(last_rowid);
    // Decorrelate: deterministic pseudo-random order (FNV-1a sort key), not
    // insertion order — sequential logs are temporally correlated (DQN lineage).
    rows.sort_by_key(|(_, key, _)| {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in key.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    });
    let mut replayed: u64 = 0;
    let mut unmapped_keys: u64 = 0;
    if !dry_run {
        for (_rowid, key, reward) in &rows {
            // "outcome:bash:touring:plain" → tool "bash"; a key without the
            // segment still carries a real reward — replayed under a visible
            // fallback, never dropped (lição F-1).
            let tool_name = match key.split(':').nth(1).filter(|s| !s.is_empty()) {
                Some(t) => t.to_string(),
                None => {
                    unmapped_keys += 1;
                    "replay-unknown".to_string()
                }
            };
            let r = reward.clamp(0.0, 1.0);
            let q = touring_intelligence::rl::ImmediateReward {
                tool_name,
                accepted: r >= 0.5,
                latency_ms: 0,
                error_count: u32::from(r < 0.5),
                cila_level: 0,
                file_type: 0,
                quality_score: Some(r),
            };
            OnlineRLOps::process_immediate_reward(rt, &q);
            replayed += 1;
        }
        // Persist NOW — the point is surviving the next restart; the every-10
        // batch would leave up to 9 updates and the cursor advance volatile.
        let cursor = serde_json::json!({
            "last_rowid": max_rowid,
            "replayed_total": replayed_total + replayed,
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });
        let cpath = cursor_path(&rt.project_root);
        if let Some(dir) = cpath.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&cpath, cursor.to_string());
        if let Some(ref engine) = rt.learning.online_rl
            && let Ok(json) = serde_json::to_string(&engine.export_stats())
        {
            let stats_path = rt.project_root.join(".claude/data/online_rl_state.json");
            if let Some(dir) = stats_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&stats_path, json);
        }
        if let Some(ref qt) = rt.learning.qtable_cache {
            let qtable_path = rt.project_root.join(".claude/data/qtable.rkyv");
            let _ = qt.save_rkyv(&qtable_path, 0);
            let graph_db = touring_foundation::TouringConfig::graph_db_canonical(&rt.project_root);
            let persistence = touring_intelligence::rl::LearningPersistence::new(&graph_db);
            let _ = persistence.save_qtable(qt);
        }
    }
    let engine = rt.learning.online_rl.as_ref();
    let pending = corpus_pending(
        &rt.project_root,
        if dry_run { last_rowid } else { max_rowid },
    );
    serde_json::json!({
        "replayed": if dry_run { 0 } else { replayed },
        "available": rows.len(),
        "dry_run": dry_run,
        "unmapped_keys": unmapped_keys,
        "resumed_from_zero": resumed_from_zero,
        "cursor": {
            "last_rowid": if dry_run { last_rowid } else { max_rowid },
            "replayed_total": replayed_total + if dry_run { 0 } else { replayed },
        },
        "update_count": engine.map(touring_intelligence::rl::OnlineRLEngine::update_count),
        "ema_reward": engine.map(touring_intelligence::rl::OnlineRLEngine::ema_reward),
        "corpus_pending": pending,
    })
}
