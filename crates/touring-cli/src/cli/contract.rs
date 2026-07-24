//! CLI change/harness-contract handlers (`cli_change_contract`, `cli_attest_contract`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Change-contract recording + harness-contract attestation over the gateway
//! contract types. Dependencies are fully-qualified (`crate::approval_store::*`,
//! `crate::cli_suggester::*`, `crate::gateway::*`); the shared `touring_claude_dir`
//! helper stays in cli_handlers.rs and is imported.

use crate::cli_handlers::touring_claude_dir;
use crate::runtime::HookRuntime;

/// S-09/R8 — evaluate a formal `ChangeContract`: given `pre` and `post`
/// `HarnessQuality` snapshots (and optional `invariants`), report whether a
/// self-mutation may commit — the no-regression proof gating the meta-loop.
/// Exposed via CLI `touring change-contract`; the `Edit tool` gate
/// calls it with the before/after `harness-metric` it captures around an edit.
///
/// # Payload
///
/// `{"pre": <HarnessQuality>, "post": <HarnessQuality>, "invariants": [<Invariant>]?}`
/// — `invariants` defaults to no-composite + no-dimension regression when absent.
///
/// # Returns
///
/// `ContractVerdict` JSON `{"committed": bool, "violations": [String]}`.
pub fn cli_change_contract(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use crate::gateway::change_contract::ChangeContract;
    use crate::gateway::harness_metric::HarnessQuality;
    let parse = |k: &str| -> Option<HarnessQuality> {
        payload
            .get(k)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    };
    let (Some(pre), Some(post)) = (parse("pre"), parse("post")) else {
        return "{\"error\":\"payload must carry 'pre' and 'post' HarnessQuality objects\"}"
            .to_owned();
    };
    let contract = match payload
        .get("invariants")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
    {
        Some(invariants) => ChangeContract::with_invariants(pre, post, invariants),
        None => ChangeContract::new(pre, post),
    };
    serde_json::to_string(&contract.evaluate())
        .unwrap_or_else(|e| format!("{{\"error\":\"serialize failed: {e}\"}}"))
}
/// `_rt` is unused — the constitution root is HOME-based (`touring_claude_dir`),
/// not the per-project runtime.
pub fn cli_attest_contract(_rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let root = touring_claude_dir();
    let contract = crate::gateway::harness_contract::HarnessContract::attest(&root);
    serde_json::to_string(&serde_json::json!({
        "attested": contract.attested,
        "digest": contract.digest,
        "file_count": contract.file_count,
        "claims": contract.claims,
    }))
    .unwrap_or_else(|_| "{}".to_owned())
}
