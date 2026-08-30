//! CLI learning handlers (`cli_learning_*`) — extracted from cli_handlers.rs (A-W2.P3).
//!
//! RL status snapshot + reward submission. `inject_synthetic_tool_rewards`
//! (shared bootstrap helper) stays in cli_handlers.rs.

use crate::cli::params::{str_or, str_or_empty};
use crate::cli_handlers::{LearningStatus, inject_synthetic_tool_rewards};
use crate::runtime::HookRuntime;
use touring_intelligence::rl::bandit::ContextualBandit;

/// Reports the RL learning engine's status snapshot (bandit arms, EMA reward, convergence) as JSON.
pub fn cli_learning_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    if let Some(ref mut engine) = rt.learning.online_rl {
        let _ = engine.inject_warmup_reward();
    }
    let update_count = rt
        .learning
        .online_rl
        .as_ref()
        .map(|e| e.update_count())
        .unwrap_or(0);
    if update_count > 0 && update_count < 5 {
        inject_synthetic_tool_rewards(rt);
        tracing::info!(
            update_count = update_count,
            "S-9: synthetic rewards injected for tool patterns"
        );
    }
    let online = rt.learning.online_rl.as_ref();
    let linucb = rt.learning.linucb.as_ref();
    let bandit = rt.learning.bandit.as_ref();
    let ema_reward = online.map(|e| e.ema_reward()).unwrap_or(0.0);
    let last_td_error = online.map(|e| e.last_td_error()).unwrap_or(0.0);
    // The rolling mean is the convergence signal; fall back to the single most
    // recent sample only while the window is still empty (no update recorded).
    let mean_td_error = online
        .and_then(|e| e.mean_td_error())
        .unwrap_or(last_td_error);
    let bandit_type = bandit
        .map(|b| b.export_snapshot().bandit_type.clone())
        .or_else(|| linucb.map(|l| l.export_snapshot().bandit_type.clone()))
        .unwrap_or_else(|| "none".to_string());
    let arm_count = linucb.map(|l| l.arm_stats().len()).unwrap_or(0);
    let agentic_rl_state = rt
        .learning
        .agentic_rl
        .as_ref()
        .and_then(|a| serde_json::to_value(a.export_state()).ok())
        .map(summarize_numeric_arrays);
    // P2 (29/08): o gap offline fica VISÍVEL no status — quantos outcomes
    // recompensados aguardam replay. `None` = memory.db ilegível (E4),
    // nunca zero por ausência.
    let (replay_rowid, _, _) = replay_cursor_read(&rt.project_root);
    let corpus_pending = replay_corpus_pending(&rt.project_root, replay_rowid);
    let status = LearningStatus {
        update_count,
        ema_reward,
        mean_td_error,
        last_td_error,
        linucb_loaded: linucb.is_some(),
        bandit_type,
        arm_count,
        corpus_pending,
        agentic_rl_state,
    };
    serde_json::to_string(&status)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
/// R6 (29/08, ordem de Gabriel): arrays numéricos longos viram
/// `{len, l2_norm}` — o leitor de um status decide com FORMA e MAGNITUDE,
/// nunca com 8k floats crus (~40KB por chamada, medido ao vivo). Arrays
/// curtos (≤16) e escalares passam intactos; a recursão cobre qualquer campo
/// vetorial futuro sem nova lista.
pub(crate) fn summarize_numeric_arrays(v: serde_json::Value) -> serde_json::Value {
    const KEEP: usize = 16;
    match v {
        serde_json::Value::Array(items) => {
            let nums: Vec<f64> = items
                .iter()
                .filter_map(serde_json::Value::as_f64)
                .collect();
            if nums.len() == items.len() && items.len() > KEEP {
                let l2 = nums.iter().map(|x| x * x).sum::<f64>().sqrt();
                serde_json::json!({ "len": items.len(), "l2_norm": l2 })
            } else {
                serde_json::Value::Array(
                    items.into_iter().map(summarize_numeric_arrays).collect(),
                )
            }
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(k, val)| (k, summarize_numeric_arrays(val)))
                .collect(),
        ),
        other => other,
    }
}

/// P2 replay (29/08, pendência "motor girando a seco"): cursor durável do
/// consumo offline. Contrato de leitura igual ao `code_mode_arm.json`:
/// arquivo ilegível ⇒ zeros — e o replay recomeça DECLARANDO
/// `resumed_from_zero` (updates TD re-aplicados convergem para o mesmo
/// ponto, então recomeçar é seguro; esconder que recomeçou não seria, E9).
fn replay_cursor_path(project_root: &std::path::Path) -> std::path::PathBuf {
    project_root.join(".claude/touring/learning_replay_cursor.json")
}

pub(crate) fn replay_cursor_read(project_root: &std::path::Path) -> (i64, u64, bool) {
    let path = replay_cursor_path(project_root);
    if !path.exists() {
        return (0, 0, false);
    }
    match std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
    {
        Some(v) => (
            v.pointer("/last_rowid").and_then(serde_json::Value::as_i64).unwrap_or(0),
            v.pointer("/replayed_total").and_then(serde_json::Value::as_u64).unwrap_or(0),
            false,
        ),
        // The file EXISTS but cannot be read — restarting from zero is the
        // fail-safe (TD re-application converges), but it must be visible.
        None => (0, 0, true),
    }
}

/// Outcomes recompensados ainda não consumidos pelo replay. `None` quando o
/// memory.db não abre — ausência exibida, nunca zero (E4).
pub(crate) fn replay_corpus_pending(
    project_root: &std::path::Path,
    last_rowid: i64,
) -> Option<i64> {
    let db = touring_foundation::TouringConfig::memory_db_canonical(project_root);
    let conn = rusqlite::Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    conn.query_row(
        "SELECT COUNT(*) FROM memory_entries WHERE outcome_reward IS NOT NULL AND rowid > ?1",
        rusqlite::params![last_rowid],
        |r| r.get(0),
    )
    .ok()
}

/// P2 (29/08): o canal offline→engine que não existia. O corpus de outcomes
/// recompensados (820 na estreia) nunca alcançava o `OnlineRLEngine` — o
/// trickle online (post-tool-rl, +1/tool call) funciona, mas o motor
/// recomeçava quase do zero a cada restart e o acumulado ficava no memory.db
/// sem consumidor. Este handler é o `fit(dataset)` do padrão offline-RL
/// (d3rlpy): pretreina no logado e o fine-tuning online continua no MESMO
/// engine, pelo MESMO `process_immediate_reward` do caminho vivo.
///
/// Payload: `{"limit"?: N (default 2000), "dry_run"?: bool}`.
pub fn cli_learning_replay(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_hook_runtime::runtime::traits::OnlineRLOps;
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(2000)
        .clamp(1, 50_000) as i64;
    let dry_run = payload
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let (last_rowid, replayed_total, resumed_from_zero) = replay_cursor_read(&rt.project_root);
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
            })
            .to_string();
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
            return serde_json::json!({ "error": format!("query failed: {e}") }).to_string();
        }
    };
    if rt.learning.online_rl.is_none() {
        // Sem engine não há para onde replayar — erro que ensina (A5),
        // nunca injeção silenciosa em lugar nenhum.
        return serde_json::json!({
            "error": "online_rl engine unavailable in this runtime",
            "hint": "rode via daemon (`touring learning replay`), não em cache-only",
        })
        .to_string();
    }
    let max_rowid = rows.iter().map(|(r, _, _)| *r).max().unwrap_or(last_rowid);
    // Decorrelate: replay in deterministic pseudo-random order, not insertion
    // order (DQN lineage — sequential logs are temporally correlated and bias
    // the TD updates; an FNV-1a sort key is a seedless, replayable shuffle).
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
            // segment still carries a real reward — it is replayed under a
            // visible fallback, never dropped (lição F-1: entrada estranha
            // nunca é coagida nem descartada em silêncio).
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
        // Persist NOW — the point of the backfill is surviving the next
        // restart; waiting for the every-10 batch would leave up to 9 updates
        // and the whole cursor advance volatile.
        let cursor = serde_json::json!({
            "last_rowid": max_rowid,
            "replayed_total": replayed_total + replayed,
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });
        let cpath = replay_cursor_path(&rt.project_root);
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
            let graph_db =
                touring_foundation::TouringConfig::graph_db_canonical(&rt.project_root);
            let persistence = touring_intelligence::rl::LearningPersistence::new(&graph_db);
            let _ = persistence.save_qtable(qt);
        }
    }
    let engine = rt.learning.online_rl.as_ref();
    let pending = replay_corpus_pending(
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
    .to_string()
}

/// R4 (29/08, ordem de Gabriel): o `ExperimentLog` existia completo em
/// `touring-intelligence::rl` com ZERO chamadores de produção — infra
/// desligada não é infra pronta. Este par record/list é a superfície que o
/// A/B real (o `variant_archive` do loop-engineering) usa para registrar cada
/// variante julgada, no DB do projeto (`.claude/touring/experiment_log.db`).
fn experiment_log_open(
    project_root: &std::path::Path,
) -> Result<touring_intelligence::rl::ExperimentLog, rusqlite::Error> {
    let dir = project_root.join(".claude/touring");
    let _ = std::fs::create_dir_all(&dir);
    let conn = rusqlite::Connection::open(dir.join("experiment_log.db"))?;
    touring_intelligence::rl::ExperimentLog::new(conn)
}

/// Payload: `{"variant": "<o que foi tentado>", "target": "<o que ela ataca>",
/// "reward": f64, "decision": "keep"|"discard", "session_id"?}`. `state`/
/// `action` ficam em 0 — este canal identifica o experimento por texto, não
/// por índice de política.
pub fn cli_experiment_record(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::experiment_log::{ExperimentDecision, ExperimentEntry};
    let variant = str_or_empty(payload, "variant");
    let target = str_or_empty(payload, "target");
    if variant.is_empty() || target.is_empty() {
        return serde_json::json!({
            "error": "variant and target required",
            "hint": "touring learning experiment record --variant \"<tentativa>\" \
                     --target \"<alvo>\" --reward 0.83 --decision keep",
        })
        .to_string();
    }
    let reward = payload
        .get("reward")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    // Achado do cross-audit 29/08 (F-1): `_ => Keep` coagia um rótulo
    // inválido em silêncio para o veredito MAIS FORTE (keep atualiza
    // best_reward). Entrada desconhecida agora ensina, nunca coage (A5).
    let decision = match str_or(payload, "decision", "keep") {
        "keep" => ExperimentDecision::Keep,
        "discard" => ExperimentDecision::Discard,
        other => {
            return serde_json::json!({
                "error": format!("unknown decision '{other}'"),
                "hint": "decision must be `keep` or `discard`",
            })
            .to_string();
        }
    };
    let entry = ExperimentEntry {
        state: 0,
        action: 0,
        reward,
        decision,
        composite_score: payload.get("composite_score").and_then(serde_json::Value::as_f64),
        diagnostic: Some(variant.to_string()),
        tool_name: Some(target.to_string()),
        latency_ms: None,
        session_id: payload
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    };
    let mut log = match experiment_log_open(&rt.project_root) {
        Ok(l) => l,
        Err(e) => return serde_json::json!({ "error": format!("open failed: {e}") }).to_string(),
    };
    match log.record(&entry) {
        Ok(id) => serde_json::json!({
            "recorded": true, "id": id, "target": target, "reward": reward,
        })
        .to_string(),
        Err(e) => serde_json::json!({ "error": format!("record failed: {e}") }).to_string(),
    }
}

/// Lê os experimentos de volta — a metade sem a qual o registro é write-only.
pub fn cli_experiment_list(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(20) as usize;
    let log = match experiment_log_open(&rt.project_root) {
        Ok(l) => l,
        Err(e) => return serde_json::json!({ "error": format!("open failed: {e}") }).to_string(),
    };
    let rows = log.all_events(limit).unwrap_or_default();
    let total = log.total_count().unwrap_or(0);
    let entries: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id, "timestamp": r.timestamp, "reward": r.reward,
                "decision": r.decision, "target": r.tool_name,
                "variant": r.diagnostic,
            })
        })
        .collect();
    serde_json::json!({
        "entries": entries, "shown": entries.len(), "total": total,
        "best_reward": log.best_reward(),
    })
    .to_string()
}

// Carve R (2026-06-10): runtime-service handler moved to touring-hook-runtime::ceg_impls
// (it is a pure HookRuntime capability); re-exported at the historical path.
pub use touring_hook_runtime::ceg_impls::cli_learning_reward;
