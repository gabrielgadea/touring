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
    let status = LearningStatus {
        update_count,
        ema_reward,
        mean_td_error,
        last_td_error,
        linucb_loaded: linucb.is_some(),
        bandit_type,
        arm_count,
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
    let decision = match str_or(payload, "decision", "keep") {
        "discard" => ExperimentDecision::Discard,
        _ => ExperimentDecision::Keep,
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
