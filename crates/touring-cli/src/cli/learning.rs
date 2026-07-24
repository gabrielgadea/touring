//! CLI learning handlers (`cli_learning_*`) — extracted from cli_handlers.rs (A-W2.P3).
//!
//! RL status snapshot + reward submission. `inject_synthetic_tool_rewards`
//! (shared bootstrap helper) stays in cli_handlers.rs.

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
    let mean_td_error = online.map(|e| e.last_td_error()).unwrap_or(0.0);
    let bandit_type = bandit
        .map(|b| b.export_snapshot().bandit_type.clone())
        .or_else(|| linucb.map(|l| l.export_snapshot().bandit_type.clone()))
        .unwrap_or_else(|| "none".to_string());
    let arm_count = linucb.map(|l| l.arm_stats().len()).unwrap_or(0);
    let agentic_rl_state = rt.learning.agentic_rl.as_ref().map(|a| a.export_state());
    let status = LearningStatus {
        update_count,
        ema_reward,
        mean_td_error,
        linucb_loaded: linucb.is_some(),
        bandit_type,
        arm_count,
        agentic_rl_state,
    };
    serde_json::to_string(&status)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
// Carve R (2026-06-10): runtime-service handler moved to touring-hook-runtime::ceg_impls
// (it is a pure HookRuntime capability); re-exported at the historical path.
pub use touring_hook_runtime::ceg_impls::cli_learning_reward;
