//! CLI granularity-bandit handlers (`cli_granularity_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::runtime::HookRuntime;

/// CLI handler for `touring granularity status` — returns a JSON snapshot
/// of the granularity bandit's current state (pulls per arm, average reward
/// per arm, total pulls, alpha). Lazily initializes the bandit on first call
/// so the handler always returns a well-formed payload.
///
/// # Payload
///
/// Ignores the payload (no inputs needed).
///
/// # Output shape
///
/// ```json
/// {
///   "total_pulls": 0,
///   "alpha": 1.0,
///   "num_arms": 4,
///   "arms": [
///     {"factor": "Monolithic", "pulls": 0, "avg_reward": 0.0},
///     {"factor": "Split2",     "pulls": 0, "avg_reward": 0.0},
///     {"factor": "Split3",     "pulls": 0, "avg_reward": 0.0},
///     {"factor": "Split4",     "pulls": 0, "avg_reward": 0.0}
///   ]
/// }
/// ```
pub fn cli_granularity_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::bandit::granularity::{GRANULARITY_NUM_ARMS, SplitFactor};
    let bandit = rt.granularity_bandit();
    let pulls = bandit.pulls_per_arm();
    let avg = bandit.avg_reward_per_arm();
    let alpha = bandit.alpha();
    let total = bandit.total_pulls();
    let arms_json: Vec<serde_json::Value> = (0..GRANULARITY_NUM_ARMS)
        .map(|i| {
            let factor = SplitFactor::from_index(i)
                .map(|f| format!("{:?}", f))
                .unwrap_or_else(|| "Unknown".to_string());
            let p = pulls.get(i).copied().unwrap_or(0);
            let r = avg.get(i).copied().unwrap_or(0.0);
            serde_json::json!({ "factor" : factor, "pulls" : p, "avg_reward" : r, })
        })
        .collect();
    let body = serde_json::json!(
        { "total_pulls" : total, "alpha" : alpha, "num_arms" : GRANULARITY_NUM_ARMS,
        "arms" : arms_json, }
    );
    serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
/// CLI handler for `touring granularity reset` — reinitializes the
/// granularity bandit, discarding all pulls and learned weights. Returns a
/// JSON confirmation with the pre-reset total pulls so callers can log what
/// was dropped.
pub fn cli_granularity_reset(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::bandit::granularity::GranularityBandit;
    let prior_pulls = rt
        .learning
        .granularity_bandit
        .as_ref()
        .map(|b| b.total_pulls())
        .unwrap_or(0);
    rt.learning.granularity_bandit = Some(GranularityBandit::new());
    let body = serde_json::json!({ "reset" : true, "prior_pulls" : prior_pulls, });
    serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
/// CLI handler for `touring granularity hint` — returns the GranularityBandit's
/// recommended split factor for a given task.
///
/// Payload: `{"size_loc": N, "language": "rust", "cila_level": N}`
/// Response: `{"split_factor": "Split3", "subtask_count": 3}`
///
/// Wave C2-wiring D2 (2026-04-20): exposes `select_task_split` via daemon hook
/// so `touring-server::reasoning::granularity_adapter` can query it without
/// linking `touring-hooks` into the server crate (avoids cycle).
pub fn cli_granularity_hint(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let size_loc = payload
        .get("size_loc")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as usize;
    let language = payload
        .get("language")
        .and_then(|v| v.as_str())
        .unwrap_or("rust");
    let cila_level = payload
        .get("cila_level")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .min(4) as u8;
    let factor = rt.select_task_split(size_loc, language, cila_level);
    let body = serde_json::json!(
        { "split_factor" : format!("{factor:?}"), "subtask_count" : factor
        .subtask_count(), }
    );
    serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
