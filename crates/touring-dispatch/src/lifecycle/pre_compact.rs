//! `pre-compact` hook handler.
//!
//! Flushes durable state (LinUCB bandit weights, WAL frames, GoT snapshots)
//! before Claude Code compacts the context window — a transition that
//! discards in-memory session state but must not lose learned weights or
//! reasoning traces. Extracted from `lifecycle.rs` as part of FIX-3
//! modularization.
//!
//! ## ES2 P3 — Compaction-exempt re-attendance
//!
//! Re-attests the [`HarnessContract`](crate::gateway::harness_contract::HarnessContract)
//! at the compaction boundary so the constitutional digest is re-injected into
//! the LLM-visible context. The harness can pin/attest/re-inject; honest carve-out:
//! cannot force model-layer attention non-eviction (KV-cache / serving layer).

use serde_json::Value;

use crate::gateway::harness_contract::HarnessContract;
use crate::runtime::HookRuntime;

/// pre-compact: flush rkyv snapshots before context compaction.
///
/// Context compaction may discard session state — flush persisted models first
/// so that LinUCB weights, WAL frames, and GoT state are safely on disk.
/// The GoT snapshot acts as a "cognitive fossil record" — when the LLM loses
/// reasoning detail after compaction, the HNSW index can recover exact thought
/// nodes from this persisted snapshot.
pub(crate) fn handle_pre_compact(rt: &mut HookRuntime, _input: &Value) -> String {
    // Track which durable operations succeeded (for activity event payload)
    let mut linucb_saved = false;
    let mut got_snapshot_saved = false;

    // Flush LinUCB bandit state
    if let Err(e) = rt.save_linucb() {
        tracing::warn!(error = %e, "pre-compact: linucb flush failed");
    } else {
        linucb_saved = true;
        tracing::debug!("pre-compact: linucb flushed");
    }

    // Wave C1.7-persistence: flush granularity bandit state so split-factor
    // learning survives context compaction.
    if let Err(e) = rt.save_granularity_bandit() {
        tracing::warn!(error = %e, "pre-compact: granularity bandit flush failed");
    } else {
        tracing::debug!("pre-compact: granularity bandit flushed");
    }

    // Wave S-3-persistence: flush pre-edit health cache so health_delta
    // state survives daemon restarts (closes daemon/e2e composite gap).
    if let Err(e) = crate::health_delta::save_health_delta_cache(&rt.project_root) {
        tracing::warn!(error = %e, "pre-compact: health_delta cache flush failed");
    } else {
        tracing::debug!("pre-compact: health_delta cache flushed");
    }

    // WAL checkpoint
    let _ = rt.ctx.knowledge.wal_checkpoint();

    // Persist GoT snapshot — protects reasoning state from compaction loss
    if let Some(ref snapshot) = rt.got_snapshot {
        let db_path = touring_foundation::TouringConfig::graph_db_canonical(&rt.project_root);
        match crate::got_snapshot_store::GoTSnapshotStore::new(&db_path) {
            Ok(store) => {
                let session_id = &snapshot.session_id;
                match store.save(session_id, snapshot) {
                    Ok(()) => {
                        got_snapshot_saved = true;
                        tracing::info!(
                            session_id = %session_id,
                            nodes = snapshot.nodes.len(),
                            "pre-compact: GoT snapshot persisted"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "pre-compact: GoT snapshot save failed");
                    }
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "pre-compact: GoT snapshot store unavailable");
            }
        }
    }

    tracing::info!("pre-compact: snapshots flushed");

    // D1.6: Emit activity event before returning.
    crate::activity_hook::emit_pre_compact(&rt.project_root, linucb_saved, got_snapshot_saved);

    // ES2 P3 — re-attend the constitutional contract so the digest survives
    // compaction. The harness can pin/attest/re-inject; honest carve-out:
    // cannot force model-layer attention non-eviction (KV-cache / serving layer).
    let constitution_root = rt.project_root.join(".claude");
    let contract = HarnessContract::attest(&constitution_root);

    render_contract_status(&contract)
}

/// Render the post-attest status line that the pre-compact hook returns to
/// the LLM. When the contract is fully attested, emits a positive digest
/// line; when claims fail, emits a drift warning listing the short names
/// of the failed claim kinds.
fn render_contract_status(contract: &HarnessContract) -> String {
    if contract.attested {
        let passed = contract.claims.iter().filter(|c| c.passed).count();
        let total = contract.claims.len();
        tracing::info!(
            digest = %contract.digest,
            claims_passed = passed,
            claims_total = total,
            "pre-compact: HarnessContract re-attended (digest survives compaction)"
        );
        return format!(
            "\n\n[ES2 P3] HarnessContract re-attest: digest={}, claims={}/{} passed",
            contract.digest, passed, total,
        );
    }
    let failed: Vec<&'static str> = contract
        .claims
        .iter()
        .filter(|c| !c.passed)
        .map(|c| claim_short_name(&c.claim))
        .collect();
    tracing::warn!(
        failed_claims = ?failed,
        "pre-compact: HarnessContract re-attest FAILED (constitutional drift detected)"
    );
    format!(
        "\n\n[ES2 P3] HarnessContract DRIFT: {} claim(s) failed: {:?}",
        failed.len(),
        failed,
    )
}

/// Map a [`ContractClaim`] variant to a stable short label for logging and
/// drift warnings. Kept private to this module so P3 stays a single-file
/// additive change (P1+P2's `harness_contract.rs` is untouched).
fn claim_short_name(claim: &crate::gateway::harness_contract::ContractClaim) -> &'static str {
    use crate::gateway::harness_contract::ContractClaim as C;
    match claim {
        C::ConstitutionPresent => "ConstitutionPresent",
        C::LineCountUnderStructuralCeiling { .. } => "LineCount",
        C::RegraMarkerPresent { .. } => "RegraMarker",
        C::RuleFilePresent { .. } => "RuleFile",
    }
}
