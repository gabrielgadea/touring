//! The Code Execution Gateway **hook driver** — the only place where the CEG
//! `X0..X7` pipeline meets the Claude Code hook protocol ([`HookResponse`])
//! and the parent runtime ([`HookRuntime`]).
//!
//! Session A · F2 of the `touring-ceg` extraction (2026-06-10): after this
//! split, `crate::gateway` + `crate::capability` are parent-type-free — this
//! adapter is the seam that stays behind in `touring-hooks` when the gateway
//! core moves to the leaf crate. The gateway-core API it consumes
//! ([`run_gateway`], [`GatewayDeps`], [`observe`], `learn::emit_gate_reward`,
//! `metrics::record_verdict_counters`) is already `pub`, so the only change
//! the physical move requires here is none at all: `crate::gateway::*` keeps
//! resolving through the parent's `pub use touring_ceg::gateway` re-export.
//!
//! # What lives here
//!
//! - [`run`] / [`run_returning`] — the `pre-exec` enforcement hook entry
//!   points. X9 LEARN runs through the parent's [`HookRuntime`], which
//!   implements the `CegRuntime` / `LearnRuntime` IoC traits
//!   (`touring-contracts`), so `gateway::learn` stays runtime-agnostic.
//! - [`run_observe_only`] — the S-01 universal observe hook (`ceg-observe`
//!   one-shot + the in-daemon dispatch in `hook_registry`).
//! - The verdict → [`HookResponse`] mapping and the hook-input parsing —
//!   hook-protocol policy, deliberately **not** gateway-core.

use crate::capability::Capability;
use crate::capability::builtins;
use crate::gateway::decision::Verdict;
use crate::gateway::error::GatewayError;
use crate::gateway::learn::{detect_silent_failure, emit_gate_reward, reconcile_drift};
use crate::gateway::metrics::record_verdict_counters;
use crate::gateway::offensive_integration::{ClaimContext, SolverBackendKind};
use crate::gateway::outcome_learner::global_model_outcome_history;
use crate::gateway::pre_exec::{
    GatewayDeps, GatewayOutcome, deferred_dry_run, observe, run_gateway, soft_pass_symbol,
};
use crate::gateway::predict::ExecutionOutcomePredictor;
use crate::gateway::selective_checkpoint::decide_checkpoint;
use crate::runtime::{HookResponse, HookRuntime};

// ── Hook-input parsing ────────────────────────────────────────────────────────

/// Extract the `(tool_name, code_body)` pair from a PreToolUse hook input.
///
/// The body is the `command` field (a `Bash` call) or the `code` field (a
/// `ctx_execute` call) of `tool_input`.
fn extract_call(input: &serde_json::Value) -> Result<(String, String), GatewayError> {
    let tool = input
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| GatewayError::MalformedHookInput {
            detail: "missing 'tool_name'".to_owned(),
        })?;
    let tool_input = input
        .get("tool_input")
        .ok_or_else(|| GatewayError::MalformedHookInput {
            detail: "missing 'tool_input'".to_owned(),
        })?;
    let body = tool_input
        .get("command")
        .or_else(|| tool_input.get("code"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| GatewayError::MalformedHookInput {
            detail: "no 'command' or 'code' in tool_input".to_owned(),
        })?;
    Ok((tool.to_owned(), body.to_owned()))
}

// ── Hook policy ───────────────────────────────────────────────────────────────

/// `true` when `CEG_ENFORCE=1` — the opt-in that lets the pre-exec hook hard-
/// deny. Default (unset, or any other value) is advisory **observe mode**: the
/// hook reports but never blocks. Observe-before-enforce is the safe rollout
/// while the gateway is still being hardened (P4).
fn enforcement_enabled() -> bool {
    std::env::var("CEG_ENFORCE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// A compact, human-readable summary of a gateway decision.
fn verdict_summary(outcome: &GatewayOutcome) -> String {
    let decision = &outcome.decision;
    let mut summary = format!(
        "CEG gateway [{}]: {:?} — composite {:.2}",
        outcome.id, decision.verdict, decision.composite_score
    );
    for reason in &decision.reasons {
        summary.push_str("\n  · ");
        summary.push_str(reason);
    }
    if let Some(fix) = &decision.canonical_fix {
        summary.push_str("\n  fix: ");
        summary.push_str(fix);
    }
    summary
}

/// Map a [`GatewayOutcome`] to a [`HookResponse`].
///
/// `Allow` → silent ([`HookResponse::Allow`]); the hook speaks only when it has
/// something to say. `Warn` → advisory context. `Deny` → advisory context
/// unless `enforce` is set, in which case a hard [`HookResponse::Deny`]
/// (`permissionDecision: "deny"`).
fn verdict_to_response(outcome: &GatewayOutcome, enforce: bool) -> HookResponse {
    match outcome.decision.verdict {
        Verdict::Allow => HookResponse::Allow,
        Verdict::Warn => HookResponse::context_with_event(verdict_summary(outcome), "PreToolUse"),
        Verdict::Deny => {
            if enforce {
                HookResponse::Deny {
                    reason: outcome
                        .decision
                        .canonical_fix
                        .clone()
                        .unwrap_or_else(|| "the CEG gateway denied this execution".to_owned()),
                    context: Some(verdict_summary(outcome)),
                    event_name: Some("PreToolUse".to_owned()),
                }
            } else {
                HookResponse::context_with_event(verdict_summary(outcome), "PreToolUse")
            }
        }
    }
}

/// The runtime-free core of the pre-exec hook: parse the input, drive the
/// gateway, map the verdict to a [`HookResponse`]. Fail-open at every step — a
/// hook never blocks on its own malformed input or an internal hiccup.
///
/// Returns both the `HookResponse` and the `GatewayOutcome` (when available)
/// so the caller can feed the outcome to X9 LEARN (`emit_gate_reward`).
fn gate_hook_input(input: &serde_json::Value) -> (HookResponse, Option<GatewayOutcome>) {
    let (tool, body) = match extract_call(input) {
        Ok(pair) => pair,
        Err(_) => return (HookResponse::Allow, None), // fail-open
    };
    // The pre-exec hook gates a first-party developer environment, so it
    // analyses against the `Trusted` profile — `rm` / `sudo` / outbound network
    // stay denied, everything else is allowed. A routine `cargo test` is
    // silent; `rm -rf /` is not.
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        // S-11 — X4 PREDICT reads the global online model instead of the flat
        // neutral prior, so a seen action class predicts its learned success rate.
        outcome_history: &global_model_outcome_history,
        sandbox_runner: &deferred_dry_run,
        predictor: &predictor,
        profile: &profile,
        // ES1 P3 (2026-06-01) — X3.5 PROVE: no claim by default in the
        // pre-exec hook (zero Z3 cost for the hot path). Advanced
        // callers can build their own `GatewayDeps` and inject a claim.
        claim: None,
        claim_context: ClaimContext::default(),
        solver_backend: SolverBackendKind::Stub,
    };
    match run_gateway(&tool, &body, None, &deps) {
        Ok(outcome) => {
            // X7 DECISION — wire counters per verdict before mapping response.
            // Wave 6 (2026-05-23) — delegated to shared helper so the daemon
            // hook path and `cli/exec.rs` stay in lock-step forever (REGRA #0).
            record_verdict_counters(outcome.decision.verdict);
            let response = verdict_to_response(&outcome, enforcement_enabled());
            (response, Some(outcome))
        }
        // Benign (non-code-bearing / empty) or any other error → fail-open.
        Err(_) => (HookResponse::Allow, None),
    }
}

// ── Hook entry points ─────────────────────────────────────────────────────────

/// S-01 (elite-harness 2026-05-29) — the **universal CEG observability hook**.
///
/// Routes *every* code-bearing tool call (a `Bash` `command`, a `ctx_execute`
/// `code`) through the X0..X7 gateway purely to feed the `ceg_*` gate-metrics
/// counters, so `touring gate-metrics -j` reflects the whole action stream —
/// not just `touring exec`.
///
/// This is the runtime realisation of Reflex #9 (Sandbox-First) and the
/// **Inspectable** north-star property (CAH §5.2.7): the harness *sees* every
/// action it could gate. It differs from [`run`] / [`run_returning`] (the
/// `pre-exec` enforcement hook) and from `pre-bash` (scoped by an `if` matcher
/// to cargo/rust/touring commands and carrying the heavier lesson-recall
/// pipeline): this hook is matcher-wide and does nothing but [`observe`] —
/// cheap, fail-open, and it **never** alters the response. The canonical deny
/// gates (`pre-bash`, `block_git.sh`, `touring-native tooling-guard.sh`, and `pre-exec`
/// under `CEG_ENFORCE=1`) stay authoritative.
///
/// Dispatched both as the one-shot `touring-hook ceg-observe` (daemon-first;
/// see `main.rs`) and in-daemon (see `build_dispatch_table`) so the counter it
/// increments is always the daemon's — the value `touring gate-metrics -j`
/// reads.
#[must_use]
pub fn run_observe_only(input: &serde_json::Value) -> HookResponse {
    // Fail-open at the entry layer: a malformed or non-code-bearing input is
    // simply not observed — it never blocks.
    if let Ok((tool, body)) = extract_call(input) {
        // ES3 P2 (2026-06-02) — surface the full write-inference in
        // observability. We use `from_tool_payload_full` (not
        // `from_tool_payload`) here so the inferred write-set feeds the
        // `ceg_write_paths_observed_count` counter. The counter is the
        // operational signal that production traffic is exercising the
        // new write-inference path; a non-zero value confirms that
        // shell redirects and write-tool commands are now reaching the
        // lock manager's conflict rule.
        #[cfg(feature = "txn_lock_enforcement")]
        {
            let decl = crate::gateway::txn::AccessDeclaration::from_tool_payload_full(&tool, &body);
            if !decl.is_read_only() {
                crate::shared::gate_metrics::record_ceg_write_paths_observed(decl.writes.len());
            }
        }
        let _ = observe(&tool, &body);
    }
    HookResponse::Allow
}

/// Run the pre-exec hook, returning a [`HookResponse`] instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`. The
/// `HookRuntime` is consulted for X9 LEARN: `emit_gate_reward` closes the RL
/// feedback loop after every gateway decision.
pub fn run_returning(runtime: &mut HookRuntime, input: &serde_json::Value) -> HookResponse {
    let (response, outcome) = gate_hook_input(input);
    // X9 LEARN — P6.1: inject RL reward for the completed decision.
    // Fail-open: emit_gate_reward never panics, errors are logged inside.
    if let Some(ref outcome) = outcome {
        emit_gate_reward(runtime, &outcome.decision);
        // X9 LEARN — S-14 / R13: re-ground the trajectory against the prior
        // deterministic-sensor reading; a regression past threshold flags drift.
        // Fail-open: reconcile_drift never panics (poisoned cache → baseline).
        let _ = reconcile_drift(runtime, &outcome.decision);
        // X9 LEARN — C9: Class-D silent-failure detector. Cross-check the cleared
        // verdict against the REAL sandbox outcome (the C5 OutputSummary on the X5
        // ledger entry); a cleared-yet-failed action writes a gotcha + negative
        // reward so the gate learns from the missed failure. Fail-open.
        if let Some(sandbox) = outcome.evidence.sandbox_outcome.as_ref() {
            let _ = detect_silent_failure(runtime, &outcome.decision, &sandbox.summary);
        }
        // X8 — C13: selective checkpointing. Decide from the X6 gate report whether
        // this action carries side effects (FsWrite / Net / Run) worth a compensating
        // saga step; the ~majority (reads / classification) skip it entirely. Pure
        // decision, observable via tracing — the X8 supervised-exec consumer reads this
        // to register a `DistributedSagaCoordinator::compensate` step only when needed.
        // Fail-open: a missing gate report simply yields no compensation need.
        if let Some(gate_report) = outcome.evidence.gate_report.as_ref() {
            let caps: Vec<Capability> = gate_report
                .gated
                .iter()
                .map(|g| g.capability.clone())
                .collect();
            let checkpoint = decide_checkpoint(&caps);
            if checkpoint.needs_compensation {
                tracing::debug!(
                    side_effects = checkpoint.compensation_steps(),
                    "ceg:x8:selective-checkpoint: action needs a compensating saga step"
                );
            }
        }
    }
    response
}

/// Run the pre-exec hook (diverging version — the daemon dispatch entry point).
///
/// Always exits the process with code `0` — the touring hook contract. The
/// allow / warn / deny decision travels in the [`HookResponse`] JSON payload,
/// never in the exit code.
pub fn run(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), crate::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::decision::{EvidenceBundle, GateDecision};
    use crate::gateway::typestate::{Evidence, ExecutionId, RawInvocation};
    use serde_json::json;

    fn outcome_with(verdict: Verdict) -> GatewayOutcome {
        GatewayOutcome {
            id: ExecutionId::derive(&RawInvocation::new("Bash", "echo probe")),
            decision: GateDecision {
                verdict,
                composite_score: 0.9,
                reasons: vec!["a sample reason".to_owned()],
                canonical_fix: Some("a sample fix".to_owned()),
                evidence: EvidenceBundle::default(),
            },
            evidence: Evidence::default(),
        }
    }

    // ── extract_call ──────────────────────────────────────────────────────

    #[test]
    fn extract_call_parses_a_bash_command() {
        let input = json!({"tool_name": "Bash", "tool_input": {"command": "ls -la"}});
        assert_eq!(
            extract_call(&input).expect("parsed"),
            ("Bash".to_owned(), "ls -la".to_owned())
        );
    }

    #[test]
    fn extract_call_parses_ctx_execute_code() {
        let input = json!({"tool_name": "ctx_execute", "tool_input": {"code": "print(1)"}});
        assert_eq!(
            extract_call(&input).expect("parsed"),
            ("ctx_execute".to_owned(), "print(1)".to_owned())
        );
    }

    #[test]
    fn extract_call_rejects_missing_tool_name() {
        let input = json!({"tool_input": {"command": "ls"}});
        assert!(matches!(
            extract_call(&input),
            Err(GatewayError::MalformedHookInput { .. })
        ));
    }

    #[test]
    fn extract_call_rejects_missing_command_body() {
        let input = json!({"tool_name": "Bash", "tool_input": {"description": "no body"}});
        assert!(matches!(
            extract_call(&input),
            Err(GatewayError::MalformedHookInput { .. })
        ));
    }

    // ── verdict mapping ───────────────────────────────────────────────────

    #[test]
    fn verdict_to_response_allow_is_silent() {
        let resp = verdict_to_response(&outcome_with(Verdict::Allow), false);
        assert_eq!(resp, HookResponse::Allow);
    }

    #[test]
    fn verdict_to_response_deny_advisory_vs_enforced() {
        let denied = outcome_with(Verdict::Deny);
        // Advisory (default) — a Context injection, never a hard deny.
        match verdict_to_response(&denied, false) {
            HookResponse::Context { .. } => {}
            other => panic!("advisory Deny must be a Context, got {other:?}"),
        }
        // Enforced — a hard permissionDecision: deny.
        match verdict_to_response(&denied, true) {
            HookResponse::Deny { .. } => {}
            other => panic!("enforced Deny must be a Deny, got {other:?}"),
        }
    }

    #[test]
    fn verdict_to_response_warn_injects_context() {
        match verdict_to_response(&outcome_with(Verdict::Warn), false) {
            HookResponse::Context { .. } => {}
            other => panic!("Warn must inject context, got {other:?}"),
        }
    }

    #[test]
    fn verdict_summary_names_the_verdict_and_reasons() {
        let summary = verdict_summary(&outcome_with(Verdict::Deny));
        assert!(summary.contains("Deny"));
        assert!(summary.contains("a sample reason"));
        assert!(summary.contains("a sample fix"));
    }

    // ── gate_hook_input — fail-open ───────────────────────────────────────

    #[test]
    fn gate_hook_input_allows_malformed_input() {
        assert_eq!(gate_hook_input(&json!({})).0, HookResponse::Allow);
        assert_eq!(
            gate_hook_input(&json!({"tool_name": 42})).0,
            HookResponse::Allow
        );
    }

    #[test]
    fn gate_hook_input_allows_non_code_bearing_tool() {
        let input = json!({"tool_name": "Read", "tool_input": {"command": "/etc/hosts"}});
        assert_eq!(gate_hook_input(&input).0, HookResponse::Allow);
    }

    // ── E2E ───────────────────────────────────────────────────────────────

    #[test]
    fn e2e_gate_hook_input_clean_command_is_silent() {
        let input = json!({"tool_name": "Bash", "tool_input": {"command": "echo hello"}});
        // `echo hello` under Trusted is clean — the hook stays silent.
        assert_eq!(gate_hook_input(&input).0, HookResponse::Allow);
    }

    #[test]
    fn e2e_gate_hook_input_destructive_command_speaks() {
        let input = json!({"tool_name": "Bash", "tool_input": {"command": "rm -rf /"}});
        // Default (no CEG_ENFORCE) — advisory: the hook injects context but
        // does not hard-deny. The deferred X5 runner never spawns `rm -rf /`.
        match gate_hook_input(&input).0 {
            HookResponse::Context { context, .. } => {
                assert!(
                    context.contains("Deny"),
                    "summary must report the Deny verdict"
                );
            }
            other => panic!("a destructive command must produce advisory context, got {other:?}"),
        }
    }

    // ── S-01 observe hook ─────────────────────────────────────────────────

    /// S-01 (elite-harness 2026-05-29) — `run_observe_only` must capture a
    /// *transparent* Bash event (a plain non-rust command the `pre-bash` `if`
    /// matcher would skip) into the CEG `ceg_captured_count` counter, and must
    /// never alter the response (always Allow / fail-open).
    #[test]
    fn ceg_captures_transparent_bash() {
        use crate::shared::gate_metrics::global;
        use std::sync::atomic::Ordering;
        let input = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "ls -la /tmp" }
        });
        let before = global().ceg_captured_count.load(Ordering::Relaxed);
        let response = run_observe_only(&input);
        let after = global().ceg_captured_count.load(Ordering::Relaxed);
        // `>=` not `==`: the global counter is shared across the parallel test
        // binary, so concurrent observers may also increment between reads.
        assert!(
            after >= before + 1,
            "transparent Bash must increment ceg_captured_count (before={before}, after={after})"
        );
        assert!(
            matches!(response, HookResponse::Allow),
            "observe-only hook must never alter the response — fail-open Allow"
        );
    }

    /// S-01 — fail-open: a malformed hook input (no `tool_input`) must not
    /// panic, must return Allow, and must not touch the counter.
    #[test]
    fn observe_only_fails_open_on_malformed_input() {
        use crate::shared::gate_metrics::global;
        use std::sync::atomic::Ordering;
        let before = global().ceg_captured_count.load(Ordering::Relaxed);
        let response = run_observe_only(&json!({ "tool_name": "Bash" }));
        let after = global().ceg_captured_count.load(Ordering::Relaxed);
        assert_eq!(
            after, before,
            "malformed input must not increment the counter"
        );
        assert!(matches!(response, HookResponse::Allow));
    }

    // ── ES3 P2 (2026-06-02) — S-01 observe hook + write-paths counter ───

    /// ES3 P2 (2026-06-02) — a Bash redirect into an absolute path must
    /// bump the new `ceg_write_paths_observed_count` counter (the
    /// observability surface for the production wire-inference path).
    #[cfg(feature = "txn_lock_enforcement")]
    #[test]
    fn run_observe_only_increments_write_paths_counter_when_writes_detected() {
        use crate::shared::gate_metrics::global;
        use std::sync::atomic::Ordering;
        let before = global()
            .ceg_write_paths_observed_count
            .load(Ordering::Relaxed);
        let input = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "echo es3p2-obs > /tmp/es3p2-observe-target.log" }
        });
        let response = run_observe_only(&input);
        let after = global()
            .ceg_write_paths_observed_count
            .load(Ordering::Relaxed);
        assert!(
            after > before,
            "Bash with redirect must increment write-paths counter (before={before}, after={after})"
        );
        assert!(matches!(response, HookResponse::Allow));
    }

    /// ES3 P2 (2026-06-02) — a pure-read Bash command (e.g. `cat`)
    /// must NOT touch the `ceg_write_paths_observed_count` counter. The
    /// counter call is gated on `!decl.is_read_only()`, so the deterministic
    /// assertion is that guard predicate itself — the global counter is
    /// shared across the parallel test binary and an equality assertion on
    /// it is racy (hardened 2026-06-10 after a scheduling flake).
    #[cfg(feature = "txn_lock_enforcement")]
    #[test]
    fn run_observe_only_no_increment_for_pure_reads() {
        let decl = crate::gateway::txn::AccessDeclaration::from_tool_payload_full(
            "Bash",
            "cat /etc/hostname",
        );
        assert!(
            decl.is_read_only(),
            "pure-read command must infer an empty write-set (the counter guard); got writes={:?}",
            decl.writes
        );
        let input = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "cat /etc/hostname" }
        });
        let response = run_observe_only(&input);
        assert!(matches!(response, HookResponse::Allow));
    }
}
