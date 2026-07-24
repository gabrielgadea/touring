//! CLI prediction/world-model handlers (`cli_predict_action`, `cli_world_model_status`, `cli_agentic_rl_status`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Action-outcome prediction (CEG X4), world-model snapshot, and agentic-RL
//! status. All dependencies are fully-qualified (`crate::action_signature::*`,
//! `crate::agentic_rl::*`, `crate::gateway::*`, `crate::shared::*`,
//! `touring_foundation::*`).

use crate::runtime::HookRuntime;

/// B-5 / R10 — distill the historical bash-outcome substrate into an action
/// predictor on demand. Reads the recent `bash_outcomes` (the experiential
/// memory, up to `limit`), trains a `LearnedOutcomeModel` via
/// `train_from_examples`, then predicts the success of the queried command —
/// closing the "rich experiential substrate not distilled into a predictor"
/// gap. Distinct from the S-11 online global model (which only folds in *new*
/// post-tool outcomes): this distils the persisted *history* on demand.
///
/// Payload: `{"command": "<cmd>", "limit": <usize?>}`. Returns prediction JSON
/// `{command, success_probability, confidence, matched_observations,
/// distilled_from, total_examples, distinct_features}`.
pub fn cli_predict_action(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use crate::action_signature::ActionSignature;
    use crate::gateway::ExecutionOutcomePredictor;
    use crate::gateway::outcome_learner::{ActionFeatures, LearnedOutcomeModel, OutcomeExample};

    let Some(command) = payload.get("command").and_then(|v| v.as_str()) else {
        return "{\"error\":\"payload must carry a 'command' string\"}".to_owned();
    };
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(2000) as usize;

    // Map any bash command to the same ActionFeatures space the online S-11
    // model uses, so the historical predictor and the online one are compatible.
    let features_of = |cmd: &str| -> ActionFeatures {
        let input = serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": cmd } });
        ActionFeatures::from_signature(&ActionSignature::from_pre_tool(
            "Bash", &input, None, 0, None, None,
        ))
    };

    // Distill the persisted experiential substrate.
    let outcomes = rt
        .ctx
        .knowledge
        .recent_bash_outcomes(limit)
        .unwrap_or_default();
    let model = LearnedOutcomeModel::train_from_examples(
        outcomes
            .iter()
            .map(|o| OutcomeExample::new(features_of(&o.command), o.success)),
    );

    // ES4 P5 — predict via the calibrated substrate (was the missing
    // production caller of the P3 substrate; X4-observable via the 3
    // new counters). Cold-start = total < 10; full bucket scheme (None /
    // Low / Medium / High) comes from PredictionConfidence::from_total.
    let predictor = ExecutionOutcomePredictor::new();
    let query_features = features_of(command);
    // `stats_for` returns None for unseen (tool, intent, ctx); default
    // OutcomeStats yields the bare prior (prob=0.5, confidence=None=cold).
    let stats = model.stats_for(&query_features).unwrap_or_default();
    let calibrated = predictor.prediction_calibrated(&stats);

    serde_json::json!({
        "command": command,
        "success_probability": calibrated.prob,
        "confidence": format!("{:?}", calibrated.confidence),
        "success_count": calibrated.success_count,
        "failure_count": calibrated.failure_count,
        "brier_contribution": calibrated.brier_contribution,
        "matched_observations": stats.total(),
        "distilled_from": outcomes.len(),
        "total_examples": model.total_examples(),
        "distinct_features": model.distinct_features(),
    })
    .to_string()
}
/// `cli-world-model-status` — ES4 P1 liveness probe for the durable action world
/// model (the X4 PREDICT online data source) + ES4 P5 Brier trending.
///
/// The online `LearnedOutcomeModel` accumulates `(tool, intent, ctx) → outcome`
/// counts for the daemon's whole life, but was previously RAM-only — a restart
/// reset it to a flat `0.5` cold-start. ES4 P1 persists it to
/// `<project>/.claude/touring/action_world_model.json` and warm-loads it at
/// daemon startup / session-start. This handler exposes that durable state:
///
/// - `action: "status"` (default) — report `total_examples`, `distinct_features`,
///   `warm_loaded_entries` (entries this process loaded from disk; `> 0` proves
///   the model survived a restart), the snapshot path / existence, and the ES4
///   P5 Brier-trending fields (`mean_brier`, `confiance_distribution`,
///   `total_distillations`) computed live from the 5 X4-observable counters
///   (predict / brier_running_sum / cold_start / distill).
/// - `action: "persist"` — force an immediate atomic snapshot to the canonical
///   path, so the durability cycle is provable on demand (write → restart → read).
///
/// Payload: `{ "action": "status" | "persist" }`.
pub fn cli_world_model_status(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use crate::gateway::outcome_learner::{persist_global_model_to, world_model_status};
    use std::sync::atomic::Ordering;

    let action = payload
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("status");
    let canonical = touring_foundation::TouringConfig::world_model_canonical(&rt.project_root);
    let status = world_model_status();

    // ES4 P5 — read the 3 P3.2 + 1 P2.3 counter live (atomic load) and
    // surface Brier trending + cold-start ratio to the operator. The
    // brier_running_sum is bit-cast f64 (per `record_outcome_learner_brier`).
    let gm = crate::shared::gate_metrics::global();
    let predict_count = gm.outcome_learner_predict_count.load(Ordering::Relaxed);
    let brier_bits = gm.outcome_learner_brier_running_sum.load(Ordering::Relaxed);
    let cold_start_count = gm.outcome_learner_cold_start_count.load(Ordering::Relaxed);
    let distill_count = gm.outcome_learner_distill_count.load(Ordering::Relaxed);
    let brier_sum = f64::from_bits(brier_bits);
    let mean_brier = if predict_count > 0 {
        brier_sum / (predict_count as f64)
    } else {
        0.0
    };
    let cold_start_ratio = if predict_count > 0 {
        cold_start_count as f64 / predict_count as f64
    } else {
        0.0
    };

    if action == "persist" {
        let persisted = persist_global_model_to(&canonical);
        return serde_json::json!({
            "action": "persist",
            "persisted": persisted,
            "snapshot_path": canonical.display().to_string(),
            "snapshot_exists": canonical.exists(),
            "total_examples": status.total_examples,
            "distinct_features": status.distinct_features,
            "warm_loaded_entries": status.warm_loaded_entries,
            "predict_count": predict_count,
            "mean_brier": mean_brier,
            "cold_start_ratio": cold_start_ratio,
            "total_distillations": distill_count,
        })
        .to_string();
    }

    serde_json::json!({
        "action": "status",
        "durable": true,
        "total_examples": status.total_examples,
        "distinct_features": status.distinct_features,
        "warm_loaded_entries": status.warm_loaded_entries,
        "snapshot_path": status
            .snapshot_path
            .unwrap_or_else(|| canonical.display().to_string()),
        "snapshot_exists": canonical.exists(),
        // ES4 P5 — Brier trending (running sum / predict_count) + cold-start
        // ratio (cold_count / predict_count) + total distillations.
        "predict_count": predict_count,
        "mean_brier": mean_brier,
        "cold_start_ratio": cold_start_ratio,
        "total_distillations": distill_count,
    })
    .to_string()
}
/// `cli-agentic-rl-status` — operator visibility for the meta-loop (CAH
/// `mech.evolution-agent` row observability). The AgenticRL engine is wired
/// in `post_tool_rl.rs:373-398` and only fires when
/// `learning_phase_score > ACTIVATION_THRESHOLD (0.5)`. This handler surfaces
/// the current state so the operator can SEE whether the meta-loop is armed
/// (active=true) and how many PPO updates have been performed, instead of
/// inferring from absence.
///
/// Returns JSON: `{initialized, active, learning_phase_score, update_count,
/// activation_threshold, note}` where `initialized=false` means the meta-loop
/// has never been used (lazy init, no allocation triggered by this read).
pub fn cli_agentic_rl_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    use crate::agentic_rl::ACTIVATION_THRESHOLD;
    // The runtime holds `agentic_rl: Option<AgenticRL>` (lazy init). Read via
    // `agentic_rl_mut()` which returns `&mut AgenticRL`; we only read fields
    // through `state_view()` (the dedicated accessor) so the mut semantics are
    // not violated. If the field is `None` (meta-loop never used), report the
    // uninitialized state honestly.
    if let Some(rl) = rt.learning.agentic_rl.as_ref() {
        let view = rl.state_view();
        serde_json::json!({
            "initialized": true,
            "active": view.active,
            "learning_phase_score": view.learning_phase_score,
            "update_count": view.update_count,
            "activation_threshold": view.activation_threshold,
            "note": "active=true means learning_phase_score > activation_threshold; PPO updates fire in post_tool_rl.rs:373-398 when active",
        })
        .to_string()
    } else {
        serde_json::json!({
            "initialized": false,
            "active": false,
            "learning_phase_score": 0.0,
            "update_count": 0,
            "activation_threshold": ACTIVATION_THRESHOLD,
            "note": "meta-loop has never been used (lazy init); first call to post_tool_rl agentic_rl_mut() will allocate",
        })
        .to_string()
    }
}
