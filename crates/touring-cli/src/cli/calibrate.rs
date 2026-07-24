//! `cli-calibrate-confidence` handler (Master Plan A.W2.P5 extraction).
//!
//! Mechanical extraction from `cli_handlers.rs`. Split-conformal
//! calibration (KnowNo) of the suggester firing threshold, with durable
//! HITL approval override via `crate::approval_store::ApprovalStore`.

use crate::runtime::HookRuntime;

/// `cli-calibrate-confidence` — A-A1 conformal calibration of skill selection.
///
/// Distils the recent `bash_outcomes` substrate into split-conformal calibration
/// examples (each historical command re-classified via
/// [`crate::cli_suggester::classify_confidence`] to recover the confidence the
/// suggester *would* assign, paired with whether the action succeeded) and
/// returns the data-derived firing threshold `τ = 1 − q̂` plus the calibrated
/// decision for a queried confidence — replacing the old hardcoded `0.7` cut
/// with a statistically-grounded one (KnowNo / split conformal prediction).
///
/// When the calibrator advises deferring to a human, the durable
/// [`crate::approval_store::ApprovalStore`] (S-15/R14) is consulted: a human who
/// already approved the action class overrides the conformal defer — realizing
/// the "conformal-threshold routing" the `approval_store` docstring anticipates.
///
/// Payload: `{ "confidence": f64 (default 0.7), "alpha": f64 (default 0.1),
///            "command": "<cmd>" (optional — keys the approval lookup),
///            "limit": u64 (default 2000) }`
pub fn cli_calibrate_confidence(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use crate::action_signature::ActionSignature;
    use crate::approval_store::{ApprovalStatus, ApprovalStore};
    use crate::conformal::{ConformalCalibrator, DEFAULT_ALPHA, LEGACY_THRESHOLD};

    let queried = payload
        .get("confidence")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(LEGACY_THRESHOLD);
    let alpha = payload
        .get("alpha")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(DEFAULT_ALPHA);
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(2000) as usize;
    let command = payload.get("command").and_then(|v| v.as_str());

    // Distil the persisted experiential substrate into calibration examples.
    let outcomes = rt
        .ctx
        .knowledge
        .recent_bash_outcomes(limit)
        .unwrap_or_default();
    let cal = ConformalCalibrator::from_examples(
        alpha,
        outcomes.iter().filter_map(|o| {
            let input = serde_json::json!({ "command": o.command });
            crate::cli_suggester::classify_confidence("Bash", &input)
                .map(|c| (f64::from(c), o.success))
        }),
    );
    let decision = cal.calibrate(queried);

    // Potentialize the existing durable HITL store (S-15/R14): if conformal
    // advises deferral but a human already ruled on this action class, the
    // standing approval/denial overrides the conformal defer.
    let mut hitl_override = serde_json::Value::Null;
    let mut effective_defer = decision.defer_hitl;
    let mut action_sig_key = serde_json::Value::Null;
    if let Some(cmd) = command {
        let input = serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": cmd } });
        let sig = ActionSignature::from_pre_tool("Bash", &input, None, 0, None, None);
        let key = sig.to_key();
        action_sig_key = serde_json::Value::String(key.clone());
        if decision.defer_hitl {
            let store = ApprovalStore::new(rt.ctx.knowledge.conn_ref());
            let _ = store.ensure_table();
            if let Ok(Some(record)) = store.get(&key) {
                match record.status {
                    ApprovalStatus::Approved => {
                        hitl_override = serde_json::Value::String("approved".into());
                        effective_defer = false;
                    }
                    ApprovalStatus::Denied => {
                        hitl_override = serde_json::Value::String("denied".into());
                    }
                    ApprovalStatus::Pending => {
                        hitl_override = serde_json::Value::String("pending".into());
                    }
                }
            }
        }
    }

    serde_json::json!({
        "queried_confidence": decision.raw_confidence,
        "conformal_threshold": decision.calibrated_threshold,
        "coverage_target": decision.coverage_target,
        "in_prediction_set": decision.in_prediction_set,
        "defer_hitl": decision.defer_hitl,
        "effective_defer": effective_defer,
        "hitl_override": hitl_override,
        "action_sig_key": action_sig_key,
        "n_calibration": decision.n_calibration,
        "calibrated": decision.calibrated,
        "alpha": alpha,
        "distilled_from": outcomes.len(),
        "method": "split_conformal_prediction (KnowNo)",
    })
    .to_string()
}
