//! CLI RL search/validate handlers (`cli_mcts_search`, `cli_shadow_validate`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! MCTS reasoning search + shadow (speculative) validation. Uses
//! `touring_code::ast::{speculate_v2, Lang}` for the shadow validator and
//! fully-qualified `touring_intelligence::reasoning::reasoning_engine::*` for MCTS.

use crate::runtime::HookRuntime;
use touring_code::ast::{Lang, speculate_v2};

/// Runs an MCTS reasoning search from a root state, returning the best action path as JSON.
pub fn cli_mcts_search(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let root_state_str = payload
        .get("root_state")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let complexity = payload
        .get("complexity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);
    let cila_level = if complexity > 0.7 { 3 } else { 2 };
    let root_state_hash = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        root_state_str.hash(&mut hasher);
        hasher.finish()
    };
    let mut query = touring_intelligence::reasoning::reasoning_engine::ReasoningQuery::new(
        root_state_hash,
        root_state_str.to_string(),
    );
    query = query.with_cila_level(cila_level);
    let result = if let Some(ref cognitive) = rt.cognitive {
        if let Some(res) = cognitive.resolve_reasoning(&query) {
            serde_json::json!(
                { "root_state" : root_state_str, "best_action" : res.best_action, "score"
                : res.confidence, "value" : res.value, "visits" : res.metadata
                .get("visits").copied().unwrap_or(0.0), "engine" : res.engine_name,
                "source" : "cognitive_adaptive" }
            )
        } else {
            serde_json::json!(
                { "root_state" : root_state_str, "best_action" : 0u64, "score" :
                complexity, "value" : complexity, "visits" : 0, "engine" : "none",
                "source" : "fallback" }
            )
        }
    } else {
        serde_json::json!(
            { "root_state" : root_state_str, "best_action" : 0u64, "score" : 0.5, "value"
            : 0.5, "visits" : 0, "engine" : "none", "source" : "no_cognitive_runtime" }
        )
    };
    result.to_string()
}
/// Speculatively validates a candidate edit via the shadow validator, returning a confidence score as JSON.
pub fn cli_shadow_validate(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let content = payload
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let lang = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .and_then(|ext| match ext {
            "rs" => Some(Lang::Rust),
            "py" | "pyi" => Some(Lang::Python),
            "ts" | "tsx" => Some(Lang::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Lang::JavaScript),
            "sh" | "bash" | "zsh" => Some(Lang::Bash),
            "go" => Some(Lang::Go),
            "java" => Some(Lang::Java),
            "html" | "htm" => Some(Lang::Html),
            "css" => Some(Lang::Css),
            "md" | "markdown" => Some(Lang::Markdown),
            "json" => Some(Lang::Json),
            "toml" => Some(Lang::Toml),
            "yaml" | "yml" => Some(Lang::Yaml),
            _ => None,
        })
        .unwrap_or(Lang::Rust);
    let result = speculate_v2(content, lang, None, None);
    serde_json::json!(
        { "file_path" : file_path, "valid" : result.all_passed, "score" : result
        .composite_score, "bayesian_score" : result.bayesian_score, "layers" : result
        .layers.iter().map(| l | { serde_json::json!({ "layer" : format!("{:?}", l
        .layer), "passed" : l.passed, "score" : l.score, "diagnostics" : l.diagnostics })
        }).collect::< Vec < _ >> () }
    )
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// RL warm-start handler (Master Plan A.W2.P5 extraction). Re-exported from
// `cli_handlers.rs` so the dispatch closure `crate::cli_handlers::cli_rl_warmstart`
// (hook_registry.rs) resolves.
// ─────────────────────────────────────────────────────────────────────────────

/// Read a **representative random sample** of `(command, success)` bash outcomes
/// from an arbitrary corpus DB, **read-only** — the cross-project warm-start
/// (Cold-start cluster, 2026-05-30) must never mutate another project's
/// knowledge DB.
///
/// `ORDER BY RANDOM()` (not `id DESC`) is deliberate: the recent tail of a corpus
/// is often a streak of successes, which would warm-start the loop to a
/// misleadingly optimistic EMA. A uniform random sample reflects the corpus's
/// *true* success distribution, so the replayed prior is honest.
fn read_corpus_bash_outcomes(db_path: &str, limit: usize) -> Result<Vec<(String, bool)>, String> {
    use rusqlite::OpenFlags;
    let conn = rusqlite::Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("open {db_path}: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT command, success FROM bash_outcomes ORDER BY RANDOM() LIMIT ?1")
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map([limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0))
        })
        .map_err(|e| format!("query: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("collect: {e}"))
}

/// `cli-rl-warmstart` — opt-in cross-project warm-start of the RL reward loop
/// (Cold-start cluster, 2026-05-30).
///
/// The RL substrate is **per-project** by design
/// ([`touring_foundation::TouringConfig::knowledge_db_canonical`] resolves to
/// `project_root/.claude/touring/knowledge.db`), so a fresh project's reward
/// loop is genuinely cold. This handler lets a project **opt in** to seeding
/// its loop from another project's REAL accumulated `bash_outcomes` — *experience
/// replay of real outcomes*, never synthetic. Default (no corpus configured) is
/// a no-op, preserving strict per-project isolation.
///
/// Each replayed outcome is fed through the genuine reward path
/// ([`HookRuntime::process_immediate_reward`]) exactly as a live tool result
/// would be — so `update_count`, `ema_reward`, and the bandit arms warm-start
/// from measured reality (`success → 1.0`, `failure → 0.0`).
///
/// Payload: `{ "corpus_db": "<path>" (or env TOURING_RL_WARMSTART_CORPUS),
///            "limit": u64 (default 200), "max_inject": u64 (default 200) }`
pub fn cli_rl_warmstart(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let corpus_path = payload
        .get("corpus_db")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .or_else(|| std::env::var("TOURING_RL_WARMSTART_CORPUS").ok());
    let Some(corpus_path) = corpus_path else {
        return serde_json::json!({
            "warmstarted": false,
            "reason": "no cross-project corpus configured (set TOURING_RL_WARMSTART_CORPUS or pass --corpus-db); per-project isolation preserved",
        })
        .to_string();
    };
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(200) as usize;
    let max_inject = payload
        .get("max_inject")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(200) as usize;

    let outcomes = match read_corpus_bash_outcomes(&corpus_path, limit) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::json!({
                "warmstarted": false,
                "reason": format!("corpus read failed: {e}"),
                "source_db": corpus_path,
            })
            .to_string();
        }
    };
    if outcomes.is_empty() {
        return serde_json::json!({
            "warmstarted": false,
            "reason": "corpus has no bash_outcomes",
            "source_db": corpus_path,
        })
        .to_string();
    }

    let total = outcomes.len();
    let successes = outcomes.iter().filter(|(_, s)| *s).count();
    let measured = successes as f64 / total as f64;

    // Experience replay: feed each REAL outcome through the genuine reward path.
    let mut qtable = rt.learning.qtable_cache.take().unwrap_or_default();
    let mut replayed = 0usize;
    for (_cmd, success) in outcomes.iter().take(max_inject) {
        let reward = touring_intelligence::rl::ImmediateReward {
            tool_name: "Bash".to_string(),
            accepted: *success,
            latency_ms: 0,
            error_count: u32::from(!*success),
            cila_level: 2,
            file_type: 3,
            quality_score: Some(if *success { 1.0 } else { 0.0 }),
        };
        rt.process_immediate_reward(&reward, &mut qtable);
        replayed += 1;
    }
    rt.learning.qtable_cache = Some(qtable);

    let (update_count_after, ema_after) = rt
        .learning
        .online_rl
        .as_ref()
        .map(|e| (e.update_count(), e.ema_reward()))
        .unwrap_or((0, 0.0));

    serde_json::json!({
        "warmstarted": true,
        "source_db": corpus_path,
        "corpus_outcomes_read": total,
        "replayed": replayed,
        "measured_bash_success": measured,
        "update_count_after": update_count_after,
        "ema_reward_after": ema_after,
        "method": "cross_project_experience_replay (real outcomes, read-only)",
    })
    .to_string()
}
