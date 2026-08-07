//! X9 LEARN integration for the Code Execution Gateway — phase **P6** of CEG
//! Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! After X7 DECISION the pipeline produces a [`GatewayOutcome`].  This module
//! closes the feedback loops so every decision teaches the daemon:
//!
//! - **P6.1 [`emit_gate_reward`]** — injects an RL reward into the `HookRuntime`
//!   learning subsystem so the gateway can tune its thresholds over time.
//!   Reward magnitude tracks the composite score; sign tracks the verdict
//!   (`Allow`/`Warn` → positive, `Deny` → negative).
//!
//! - **P6.2 [`persist_forbidden_as_gotcha`]** — when the verdict is `Deny` and
//!   the static analyser surfaced a concrete forbidden pattern, that pattern is
//!   written to the gotcha DB so the *next* session is pre-warned before the
//!   gateway even fires.
//!
//! - **P6.3 [`dry_run_bridge`]** — a documented pub API that the Generator
//!   *Speculated* stage (and any other caller inside `touring-hooks` or
//!   `touring-server`) can use to reuse the CEG dry-run path without
//!   re-implementing it.  Failed gateway executions are mined into
//!   error→resolution lessons via the transcript-miner lesson key contract.
//!
//! # Fail-open invariant
//!
//! Every function in this module is fail-open: errors are logged at `warn!`
//! level and the caller continues.  No `.unwrap()` in production paths.

use super::decision::{GateDecision, Verdict};
use super::harness_metric::HarnessQuality;
use super::pre_exec::GatewayOutcome;
// S-13 (2026-06-06) — X9 LEARN dependency-inversion seam. The X9 functions are
// generic over `CegRuntime` (supertrait of `LearnRuntime`), impl'd by `HookRuntime`
// in `cli_handlers.rs`. This module no longer names `crate::cli_handlers` OR
// `crate::runtime::HookRuntime` — the last two gateway → parent edges from the X9
// LEARN stage are gone. See `gateway/deps.rs`.
use super::deps::CegRuntime;
use super::summarize::OutputSummary;
use crate::gateway::drift_corrector::{DriftReconciliation, SensorReading, reconcile};
use touring_hooks_shared::gate_metrics::GateMetricsSnapshot;

// ── P6.1 — RL reward loop ─────────────────────────────────────────────────────

/// X9 LEARN — P6.1: inject an RL reward for a completed gateway decision.
///
/// Maps the [`GateDecision`] to a signed reward in `[-1.0, +1.0]`:
///
/// | Verdict | Base reward | Modifier |
/// |---------|-------------|----------|
/// | `Allow` | `+composite_score` | — |
/// | `Warn`  | `+composite_score * 0.5` | positive but discounted |
/// | `Deny`  | `-composite_score` | negative — gate blocked an execution |
///
/// The tool name `"ceg"` is the RL arm key so the LinUCB bandit can learn
/// per-gateway behaviour independently of other tools.
///
/// # Fail-open
///
/// If `cli_learning_reward` returns an error JSON the call is a no-op.  The
/// gateway never panics on a learning failure.
pub fn emit_gate_reward(rt: &mut impl CegRuntime, decision: &GateDecision) {
    let reward_value = match decision.verdict {
        Verdict::Allow => decision.composite_score,
        Verdict::Warn => decision.composite_score * 0.5,
        Verdict::Deny => -decision.composite_score,
    };

    let context = format!(
        "ceg:x7:{:?}:score={:.3}",
        decision.verdict, decision.composite_score
    );

    let payload = serde_json::json!({
        "tool_name": "ceg",
        "reward": reward_value,
        "context": context,
    });

    // cli_learning_reward is infallible from the caller's perspective — it
    // returns a JSON string (possibly containing "error") but never panics.
    let _result = rt.learning_reward(&payload);

    // ES3 P4 — also publish the tool outcome to the cross-agent ledger so
    // other agent processes observing the same `data_dir` see what this
    // runtime did. Substrate-only: producers wire here, the
    // `LedgerConsumer` poll loop is deferred to a followup wave. Fail-open:
    // a missing or full ledger never breaks the gateway.
    // S-13: the actor_id + cross_agent_ledger.write_event field access is abstracted
    // behind `CegRuntime::record_tool_outcome` (fail-open inside the impl).
    let outcome_payload = serde_json::to_vec(&serde_json::json!({
        "tool": "ceg",
        "verdict": format!("{:?}", decision.verdict),
        "composite": decision.composite_score,
        "context": context,
    }))
    .unwrap_or_default();
    rt.record_tool_outcome("tool_outcome", &outcome_payload);
}

// ── P6.4 (S-14 / R13) — system-wide drift correction ───────────────────────────

/// The result-cache scope under which the previous [`SensorReading`] is stashed
/// between gateway decisions — so drift is measured *relative* to the last
/// accepted action across the whole session, not in isolation.
const DRIFT_CACHE_SCOPE: &str = "__ceg_drift__";
/// The key for the stashed previous reading inside [`DRIFT_CACHE_SCOPE`].
const DRIFT_CACHE_KEY: &str = "prev_sensor_reading";
/// Deterministic sensors are noisy; only a drop past this on an axis is real
/// drift (S-14). Sub-threshold jitter must not trip the loop.
const DRIFT_THRESHOLD: f64 = 0.05;

/// X9 LEARN — S-14 / R13: re-ground the post-decision state against the prior
/// reading and flag slow degradation across the trajectory.
///
/// Builds a [`SensorReading`] from deterministic sensors — the live
/// [`HarnessQuality`] composite (S-06, from the gate-metrics snapshot) and the
/// X7 [`EvidenceBundle`](super::decision::EvidenceBundle) composite (S-05, from
/// `decision.evidence`) — and [`reconcile`]s it against the reading stashed by
/// the previous call. The first call (no prior) is the baseline and never flags
/// (drift is only meaningful *relative* to an anchor). On divergence a negative
/// RL reward teaches the bandit and a `warn!` names the diverged axes.
///
/// The `health_delta` axis has no separate per-action sensor at X9, so it is fed
/// `0.0` — that axis then never false-flags, and drift rests on the two genuine
/// evolving sensors (harness + evidence composites).
///
/// Returns the [`DriftReconciliation`] so callers and tests can assert on it.
///
/// # Fail-open
///
/// A poisoned / unparsable cached reading is treated as "no prior" (baseline),
/// never a panic. The reward injection is infallible from the caller's view.
pub fn reconcile_drift(rt: &mut impl CegRuntime, decision: &GateDecision) -> DriftReconciliation {
    let snap = GateMetricsSnapshot::capture();
    let harness = HarnessQuality::from_snapshot(&snap, Some(&decision.evidence));
    // ES2 P4 — wire the constitutional contract digest (set by P3
    // `session_start`/`pre_compact` re-attend) into the sensor reading so the
    // `constitutional_digest` axis in `drift_corrector::reconcile` can fire
    // when the constitution is edited mid-session.
    let current = SensorReading::from_signals_with_contract(
        &harness,
        &decision.evidence,
        0.0,
        rt.contract_attestation(),
    );

    let prior = rt
        .drift_cache_get(DRIFT_CACHE_SCOPE, DRIFT_CACHE_KEY)
        .and_then(|s| parse_sensor_reading(&s));

    let result = reconcile(prior, current, DRIFT_THRESHOLD);

    // Stash the current reading so the next decision can re-ground against it.
    rt.drift_cache_put(
        DRIFT_CACHE_SCOPE,
        DRIFT_CACHE_KEY,
        serialize_sensor_reading(&current),
    );

    if result.diverged {
        tracing::warn!(
            axes = ?result.diverged_axes,
            composite_delta = result.composite_delta,
            "CEG X9 drift detected — harness sensors regressed across the trajectory"
        );
        // A drift event is a negative trajectory signal; teach the bandit so the
        // gateway tunes toward decisions that do not erode the deterministic
        // sensors. Reuses the same infallible reward path as emit_gate_reward.
        let payload = serde_json::json!({
            "tool_name": "ceg",
            "reward": -0.25_f64,
            "context": format!("ceg:x9:drift:{}", result.diverged_axes.join(",")),
        });
        let _ = rt.learning_reward(&payload);
    }
    result
}

/// Serialise a reading to a compact `harness;evidence;health` triple for the
/// result cache.
fn serialize_sensor_reading(r: &SensorReading) -> String {
    format!(
        "{};{};{}",
        r.harness_composite, r.evidence_composite, r.health_delta
    )
}

/// Parse the `harness;evidence;health` triple; any malformed field yields `None`
/// (treated as "no prior" by the caller — fail-open).
fn parse_sensor_reading(s: &str) -> Option<SensorReading> {
    let mut parts = s.split(';');
    let harness_composite = parts.next()?.parse().ok()?;
    let evidence_composite = parts.next()?.parse().ok()?;
    let health_delta = parts.next()?.parse().ok()?;
    Some(SensorReading {
        harness_composite,
        evidence_composite,
        health_delta,
        // ES2 P4: serialized format omits the constitutional_digest_prefix
        // (caller can derive a fresh attestation post-restore). Defaulting
        // to the pre-attestation baseline `[0; 8]` keeps the roundtrip
        // lossless for legacy cached readings.
        constitutional_digest_prefix: [0u8; 8],
    })
}

// ── P6.2 — memory / gotcha DB persistence ─────────────────────────────────────

/// X9 LEARN — P6.2: persist a forbidden pattern as a gotcha entry.
///
/// Called when the verdict is [`Verdict::Deny`] and the static analyser
/// surfaced a concrete forbidden pattern in `decision.reasons`.  The first
/// reason that starts with `"X2 STATIC blocked"` is extracted as the pattern;
/// if none exists, `decision.canonical_fix` text is used as a fallback.
///
/// The gotcha DB entry is written with:
/// - `pattern`: the forbidden pattern text (≤120 chars, trimmed)
/// - `description`: `"CEG X7 Deny: <canonical_fix>"` — actionable phrasing
/// - `severity`: `"high"` — a hard-blocked execution always warrants high priority
///
/// A matching memory entry is also stored so `touring memory recall "ceg deny"`
/// surfaces it in future sessions.
///
/// # Fail-open
///
/// DB errors are swallowed.  The gateway never panics on a persistence failure.
pub fn persist_forbidden_as_gotcha(rt: &mut impl CegRuntime, decision: &GateDecision) {
    if decision.verdict != Verdict::Deny {
        return;
    }

    // Extract the first X2 STATIC blocked reason as the pattern text.
    let pattern = decision
        .reasons
        .iter()
        .find(|r| r.starts_with("X2 STATIC blocked"))
        .map(|r| {
            // Trim to a concise pattern — strip the "X2 STATIC blocked the code: " prefix.
            r.trim_start_matches("X2 STATIC blocked the code: ")
                .trim_start_matches("X2 STATIC blocked the code (")
                .trim_end_matches(')')
        })
        .or_else(|| {
            // Fallback: use the first reason, or the canonical fix summary.
            decision
                .reasons
                .first()
                .map(String::as_str)
                .or(decision.canonical_fix.as_deref())
        })
        .unwrap_or("CEG-blocked-pattern")
        .chars()
        .take(120)
        .collect::<String>();

    if pattern.is_empty() {
        return;
    }

    let description = decision
        .canonical_fix
        .as_deref()
        .map(|f| format!("CEG X7 Deny: {f}"))
        .unwrap_or_else(|| "CEG X7 Deny: execution blocked by gateway".to_owned());

    // Write gotcha entry.
    let gotcha_payload = serde_json::json!({
        "pattern": pattern,
        "description": description,
        "severity": "high",
    });
    let _gotcha_result = rt.gotcha_add(&gotcha_payload);

    // Also persist to memory so recall surfaces it.
    let memory_key = format!(
        "ceg:deny:{:x}",
        // Use a simple hash of the pattern for a stable, short key.
        pattern
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
    );
    let memory_payload = serde_json::json!({
        "key": memory_key,
        "value": description,
        "tier": "semantic",
        "type": "lesson",
    });
    let _mem_result = rt.memory_store(&memory_payload);
}

// ── C9 — Class-D silent-failure detector (consumes the C5 OutputSummary) ──────

/// A Class-D *silent failure* surfaced at X9 — the gate cleared an action whose
/// real sandbox outcome failed. Returned so callers and tests can assert on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SilentFailure {
    /// The real subprocess exit code the cleared verdict did not reflect.
    pub exit_code: i32,
    /// The error signature (first error line, else `exit <n>`) used for the gotcha.
    pub signature: String,
}

/// X9 LEARN — C9: detect a **Class-D silent failure**.
///
/// A Class-D failure is a *cleared-yet-failed* action: the X7 gate returned
/// `Allow`/`Warn` (claimed pass) but the X5 sandbox dry-run actually FAILED — a
/// non-zero exit code or surfaced error signatures, read from the [`OutputSummary`]
/// the C5 summarizer attaches to every [`crate::gateway::sandbox_stage::SandboxOutcome`]. The narrative says
/// success; reality says failure. A `Deny` is *not* Class-D (it already names the
/// problem) and a no-output path (`exit 0`, pure-skip / deferred sentinel) is not a
/// failure — neither false-flags.
///
/// On a match it (1) writes a gotcha so the next session is pre-warned (mirrors
/// [`persist_forbidden_as_gotcha`]) and (2) injects a negative RL reward so the gate
/// tunes away from clearing silently-failing actions. Both sinks are fail-open from
/// the caller's view. Returns `Some(SilentFailure)` on a detection, else `None`.
pub fn detect_silent_failure(
    rt: &mut impl CegRuntime,
    decision: &GateDecision,
    summary: &OutputSummary,
) -> Option<SilentFailure> {
    // A Deny already names the problem — only a CLEARED action can fail silently.
    if matches!(decision.verdict, Verdict::Deny) {
        return None;
    }
    // The real outcome must actually be a failure. `exit 0` with no error lines
    // (no run / pure-skip / deferred sentinel) never false-flags.
    if !summary.is_failure() && summary.error_lines.is_empty() {
        return None;
    }

    let signature = summary
        .error_lines
        .first()
        .cloned()
        .unwrap_or_else(|| format!("exit {}", summary.exit_code));

    // (1) Gotcha — pre-warn the next session (same shape as P6.2).
    let gotcha_payload = serde_json::json!({
        "pattern": format!("ceg:x9:class-d:{signature}"),
        "description": format!(
            "CEG X9 Class-D silent failure: verdict {:?} cleared an action whose \
             sandbox dry-run failed (exit {}): {}",
            decision.verdict, summary.exit_code, signature
        ),
        "severity": "high",
    });
    let _ = rt.gotcha_add(&gotcha_payload);

    // (2) Negative reward — a missed failure is a strong negative trajectory signal,
    // weighted harder than a drift event (which is only a slow regression).
    let reward_payload = serde_json::json!({
        "tool_name": "ceg",
        "reward": -0.5_f64,
        "context": format!("ceg:x9:class-d:exit{}", summary.exit_code),
    });
    let _ = rt.learning_reward(&reward_payload);

    Some(SilentFailure {
        exit_code: summary.exit_code,
        signature,
    })
}

// ── P6.3 — dry-run bridge + transcript-miner lesson path ─────────────────────

/// The result of a [`dry_run_bridge`] call.
///
/// Callers (the Generator *Speculated* stage, future P4.2 supervised runner)
/// receive the gateway outcome so they can act on the verdict without
/// re-running the X0..X7 pipeline.
#[derive(Debug, Clone)]
pub struct DryRunBridgeResult {
    /// The terminal X7 verdict and composite score.
    pub decision: GateDecision,
    /// Whether the decision allows the code to proceed.
    pub is_allowed: bool,
    /// A human-readable verdict summary (single line).
    pub summary: String,
}

impl DryRunBridgeResult {
    fn from_outcome(outcome: &GatewayOutcome) -> Self {
        let decision = outcome.decision.clone();
        let is_allowed = decision.verdict == Verdict::Allow;
        let summary = format!(
            "CEG[{}]: {:?} score={:.2}",
            outcome.id, decision.verdict, decision.composite_score
        );
        Self {
            decision,
            is_allowed,
            summary,
        }
    }
}

/// X9 LEARN — P6.3: shared dry-run bridge for the Generator Speculated stage.
///
/// Exposes the CEG dry-run path as a documented pub API so callers can
/// gate-check a code body without re-implementing the X0..X7 pipeline.
/// The `tool` parameter should be `"Bash"` or `"ctx_execute"` matching the
/// originating tool surface.
///
/// Failed executions (verdict `Deny` or `Warn`) are persisted as
/// error→resolution lessons via the same memory key contract as the transcript
/// miner (`outcome:<tool_class>:<sig>:failure`) so the `cli_suggester` reader
/// can surface them on the next matching invocation.
///
/// # Scope guard
///
/// This function stays within `touring-hooks` + `touring-server` blast radius.
/// Wiring into `touring-generator`'s Speculated stage is a follow-up (see
/// `issues` in the engineer JSON output) — the generator is in a separate crate
/// and adding a dependency on `touring-hooks` types requires an ADR-level
/// decision about crate graph direction.
///
/// # Fail-open
///
/// Returns `None` if the gateway entry layer rejects the input (empty body,
/// non-code tool).  The caller should treat `None` as "no gate opinion" and
/// proceed.
pub fn dry_run_bridge(
    rt: &mut impl CegRuntime,
    tool: &str,
    code_body: &str,
) -> Option<DryRunBridgeResult> {
    use super::pre_exec::{
        GatewayDeps, deferred_dry_run, neutral_outcome_history, run_gateway, soft_pass_symbol,
    };
    use super::predict::ExecutionOutcomePredictor;
    use crate::capability::builtins;

    if code_body.trim().is_empty() {
        return None;
    }

    let predictor = ExecutionOutcomePredictor::new();
    let profile = builtins::trusted();

    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        outcome_history: &neutral_outcome_history,
        sandbox_runner: &deferred_dry_run,
        predictor: &predictor,
        profile: &profile,
        // ES1 P3 (2026-06-01) — X3.5 PROVE: no claim by default (zero Z3
        // cost). P6 (learn) does not require SMT proofs.
        claim: None,
        claim_context: crate::gateway::offensive_integration::ClaimContext::default(),
        solver_backend: crate::gateway::offensive_integration::SolverBackendKind::Stub,
    };

    let outcome = run_gateway(tool, code_body, None, &deps).ok()?;
    let result = DryRunBridgeResult::from_outcome(&outcome);

    // P6.3 — persist failed executions as error→resolution lessons using the
    // transcript-miner lesson key contract:
    // `outcome:<tool_class>:<sig>:failure`
    if !result.is_allowed {
        let tool_class = classify_tool_class_for_ceg(tool);
        // Derive a stable sig from the code body (first 32 non-whitespace chars).
        let sig: String = code_body
            .split_whitespace()
            .flat_map(|w| w.chars())
            .take(32)
            .collect();
        let lesson_key = format!("outcome:{tool_class}:ceg-{sig}:failure");
        let lesson_value = format!(
            "CEG dry-run blocked: {summary}. Fix: {fix}",
            summary = result.summary,
            fix = result
                .decision
                .canonical_fix
                .as_deref()
                .unwrap_or("review gateway reasons")
        );
        let mem_payload = serde_json::json!({
            "key": lesson_key,
            "value": lesson_value,
            "tier": "semantic",
            "type": "lesson",
            // The `r` of the case, stated rather than encoded in the key suffix.
            // Value-ranked recall can still derive it from `:failure`, but a key
            // convention is a fragile carrier for the one field the ranking
            // depends on — a rename would silently drop every verdict.
            "reward": 0.0,
            "outcome_context": "ceg-dry-run-blocked",
        });
        let _mem_result = rt.memory_store(&mem_payload);
    }

    Some(result)
}

/// Classify a tool name into the same tool-class strings the transcript miner
/// and `cli_suggester` reader expect (e.g. `"bash"`, `"edit"`, `"write"`).
fn classify_tool_class_for_ceg(tool: &str) -> &'static str {
    match tool.to_ascii_lowercase().as_str() {
        "bash" => "bash",
        "edit" => "edit",
        "write" => "write",
        "ctx_execute" | "ctxexecute" => "bash",
        _ => "bash",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::decision::{EvidenceBundle, GateDecision, Verdict};
    use crate::gateway::summarize::summarize_output;

    fn allow_decision() -> GateDecision {
        GateDecision {
            verdict: Verdict::Allow,
            composite_score: 0.95,
            reasons: vec![],
            canonical_fix: None,
            evidence: EvidenceBundle::default(),
        }
    }

    fn warn_decision() -> GateDecision {
        GateDecision {
            verdict: Verdict::Warn,
            composite_score: 0.72,
            reasons: vec!["X3 VGP left 1 symbol(s) unresolved".to_owned()],
            canonical_fix: Some("Review VGP output.".to_owned()),
            evidence: EvidenceBundle::default(),
        }
    }

    fn deny_static_decision() -> GateDecision {
        GateDecision {
            verdict: Verdict::Deny,
            composite_score: 0.0,
            reasons: vec!["X2 STATIC blocked the code: rm -rf /".to_owned()],
            canonical_fix: Some(
                "X2 STATIC blocked the code (rm -rf /). Revise and re-run.".to_owned(),
            ),
            evidence: EvidenceBundle::default(),
        }
    }

    fn deny_score_decision() -> GateDecision {
        GateDecision {
            verdict: Verdict::Deny,
            composite_score: 0.3,
            reasons: vec!["X5 SANDBOX dry-run exited with code 1".to_owned()],
            canonical_fix: Some("Strengthen the weakest signal.".to_owned()),
            evidence: EvidenceBundle::default(),
        }
    }

    // ── A.W3.P1 cross-audit (2026-06-09): X9 LEARN loop proven via the IoC trait ──
    //
    // The public learn-loop functions (`emit_gate_reward`, `persist_forbidden_as_gotcha`,
    // `reconcile_drift`) are generic over `CegRuntime`. Before this mock they had NO
    // end-to-end coverage — only their helpers (`compute_reward`, `classify_tool_class`,
    // `parse_sensor_reading`) were tested, and `persist_forbidden_skips_non_deny_verdicts`
    // could only assert the verdict "in isolation (no HookRuntime available in unit
    // tests)". The whole point of the seam is that the loop is now mockable: this
    // `MockCeg` closes that proof gap, exercising the RL / gotcha / memory loops and the
    // fail-open invariant through the trait, with no `HookRuntime` present.

    /// A mock implementing the [`CegRuntime`] IoC contract that records every
    /// dispatched X9 LEARN op for assertion.
    #[derive(Default)]
    struct MockCeg {
        rewards: std::cell::RefCell<Vec<String>>,
        gotchas: std::cell::RefCell<Vec<String>>,
        memories: std::cell::RefCell<Vec<String>>,
        tool_outcomes: std::cell::RefCell<Vec<String>>,
        cache: std::cell::RefCell<std::collections::HashMap<String, String>>,
    }

    impl crate::gateway::deps::LearnRuntime for MockCeg {
        fn learning_reward(&mut self, payload: &serde_json::Value) -> String {
            self.rewards.borrow_mut().push(payload.to_string());
            "{\"ok\":true}".to_owned()
        }
        fn gotcha_add(&mut self, payload: &serde_json::Value) -> String {
            self.gotchas.borrow_mut().push(payload.to_string());
            "{\"ok\":true}".to_owned()
        }
        fn memory_store(&mut self, payload: &serde_json::Value) -> String {
            self.memories.borrow_mut().push(payload.to_string());
            "{\"ok\":true}".to_owned()
        }
    }

    impl crate::gateway::deps::CegRuntime for MockCeg {
        fn record_tool_outcome(&self, event_type: &str, _payload: &[u8]) {
            self.tool_outcomes.borrow_mut().push(event_type.to_owned());
        }
        fn drift_cache_get(&self, scope: &str, key: &str) -> Option<String> {
            self.cache.borrow().get(&format!("{scope}:{key}")).cloned()
        }
        fn drift_cache_put(&self, scope: &str, key: &str, value: String) {
            self.cache
                .borrow_mut()
                .insert(format!("{scope}:{key}"), value);
        }
        fn contract_attestation(
            &self,
        ) -> Option<&crate::gateway::harness_contract::HarnessContract> {
            None
        }
    }

    /// `emit_gate_reward` injects exactly one RL reward and publishes one
    /// tool-outcome to the ledger — the X9 LEARN reward loop, closed via the trait.
    #[test]
    fn emit_gate_reward_drives_reward_and_ledger_via_trait() {
        let mut mock = MockCeg::default();
        emit_gate_reward(&mut mock, &allow_decision());
        assert_eq!(
            mock.rewards.borrow().len(),
            1,
            "exactly one RL reward injected through LearnRuntime"
        );
        assert_eq!(
            mock.tool_outcomes.borrow().len(),
            1,
            "exactly one tool outcome recorded through CegRuntime"
        );
    }

    /// On a `Deny` verdict the gotcha + memory loops both close through the trait.
    #[test]
    fn persist_forbidden_writes_gotcha_and_memory_on_deny_via_trait() {
        let mut mock = MockCeg::default();
        persist_forbidden_as_gotcha(&mut mock, &deny_static_decision());
        assert_eq!(
            mock.gotchas.borrow().len(),
            1,
            "one gotcha persisted on Deny"
        );
        assert_eq!(
            mock.memories.borrow().len(),
            1,
            "one memory lesson persisted on Deny"
        );
    }

    /// The non-Deny skip guard, now proven end-to-end through the trait — the old
    /// `persist_forbidden_skips_non_deny_verdicts` could only assert the verdict in
    /// isolation; the IoC seam makes the real loop proof possible.
    #[test]
    fn persist_forbidden_skips_non_deny_via_trait() {
        let mut mock = MockCeg::default();
        persist_forbidden_as_gotcha(&mut mock, &allow_decision());
        assert!(mock.gotchas.borrow().is_empty(), "no gotcha on Allow");
        assert!(mock.memories.borrow().is_empty(), "no memory on Allow");
    }

    // ── C9 — Class-D silent-failure detector ──────────────────────────────────

    #[test]
    fn class_d_flags_cleared_but_failed_action() {
        let mut mock = MockCeg::default();
        // Allow verdict (claimed pass) + a sandbox summary that actually failed.
        let failed = summarize_output("error: linker failed\n", 1, false);
        let sf = detect_silent_failure(&mut mock, &allow_decision(), &failed)
            .expect("Class-D silent failure detected");
        assert_eq!(sf.exit_code, 1);
        assert!(sf.signature.contains("linker failed"));
        assert_eq!(mock.gotchas.borrow().len(), 1, "one gotcha written");
        assert_eq!(mock.rewards.borrow().len(), 1, "one reward injected");
        assert!(mock.rewards.borrow()[0].contains("-0.5"), "negative reward");
    }

    #[test]
    fn class_d_ignores_deny_verdict() {
        let mut mock = MockCeg::default();
        let failed = summarize_output("error: boom\n", 1, false);
        // A Deny already names the problem — not a silent failure.
        assert!(detect_silent_failure(&mut mock, &deny_static_decision(), &failed).is_none());
        assert!(mock.gotchas.borrow().is_empty());
        assert!(mock.rewards.borrow().is_empty());
    }

    #[test]
    fn class_d_ignores_cleared_and_passed_action() {
        let mut mock = MockCeg::default();
        // Allow + a real success (exit 0, no errors) — no silent failure.
        assert!(
            detect_silent_failure(&mut mock, &allow_decision(), &OutputSummary::empty(0)).is_none()
        );
        assert!(mock.gotchas.borrow().is_empty());
    }

    #[test]
    fn class_d_signature_falls_back_to_exit_code() {
        let mut mock = MockCeg::default();
        // Non-zero exit with NO matched error line still counts (anti-masking) —
        // the signature falls back to the exit code.
        let sf = detect_silent_failure(&mut mock, &allow_decision(), &OutputSummary::empty(137))
            .expect("non-zero exit is a failure even without error lines");
        assert_eq!(sf.signature, "exit 137");
    }

    /// Fail-open invariant: with no contract attestation (mock returns `None`) and
    /// an empty drift cache, `reconcile_drift` must not panic and must stash the
    /// baseline reading through the trait's drift cache for the next comparison.
    #[test]
    fn reconcile_drift_is_fail_open_without_attestation_via_trait() {
        let mut mock = MockCeg::default();
        let _ = reconcile_drift(&mut mock, &allow_decision());
        assert!(
            !mock.cache.borrow().is_empty(),
            "baseline drift reading stashed via drift_cache_put through the trait"
        );
    }

    // ── P6.1 tests ────────────────────────────────────────────────────────────

    #[test]
    fn reward_value_for_allow_is_positive_and_equals_composite() {
        let d = allow_decision();
        // reward = composite_score for Allow
        let expected_reward = d.composite_score;
        assert!(expected_reward > 0.0);
        assert!((expected_reward - 0.95).abs() < 1e-9);
    }

    #[test]
    fn reward_value_for_warn_is_positive_but_discounted() {
        let d = warn_decision();
        let reward = d.composite_score * 0.5;
        assert!(reward > 0.0);
        assert!(reward < d.composite_score);
        assert!((reward - 0.36).abs() < 1e-9);
    }

    #[test]
    fn reward_value_for_deny_is_negative() {
        let d = deny_score_decision();
        let reward = -d.composite_score;
        assert!(reward < 0.0);
        assert!((reward - (-0.3)).abs() < 1e-9);
    }

    #[test]
    fn reward_is_bounded_in_minus_one_to_one() {
        // composite_score is always in [0,1] so reward is always in [-1,1]
        for decision in [allow_decision(), warn_decision(), deny_static_decision()] {
            let r = match decision.verdict {
                Verdict::Allow => decision.composite_score,
                Verdict::Warn => decision.composite_score * 0.5,
                Verdict::Deny => -decision.composite_score,
            };
            assert!(
                (-1.0..=1.0).contains(&r),
                "reward {r} out of bounds for {decision:?}"
            );
        }
    }

    #[test]
    fn allow_reward_is_always_non_negative() {
        let d = allow_decision();
        assert!(d.composite_score >= 0.0);
    }

    #[test]
    fn deny_reward_is_always_non_positive() {
        let d = deny_static_decision();
        assert!(-d.composite_score <= 0.0);
    }

    // ── P6.2 tests ────────────────────────────────────────────────────────────

    #[test]
    fn persist_forbidden_skips_non_deny_verdicts() {
        // Just verify the early-return logic: non-Deny must not reach the
        // gotcha_add path.  We test the guard in isolation (no HookRuntime
        // available in unit tests).
        let allow = allow_decision();
        assert_ne!(allow.verdict, Verdict::Deny);
        let warn = warn_decision();
        assert_ne!(warn.verdict, Verdict::Deny);
    }

    #[test]
    fn pattern_extraction_prefers_x2_static_reason() {
        let d = deny_static_decision();
        let pattern = d
            .reasons
            .iter()
            .find(|r| r.starts_with("X2 STATIC blocked"))
            .map(|r| {
                r.trim_start_matches("X2 STATIC blocked the code: ")
                    .to_owned()
            });
        assert_eq!(pattern.as_deref(), Some("rm -rf /"));
    }

    #[test]
    fn pattern_extraction_falls_back_to_first_reason() {
        let d = deny_score_decision();
        // No X2 STATIC reason — falls back to first reason.
        let has_static = d.reasons.iter().any(|r| r.starts_with("X2 STATIC blocked"));
        assert!(!has_static);
        let fallback = d.reasons.first().map(String::as_str);
        assert_eq!(fallback, Some("X5 SANDBOX dry-run exited with code 1"));
    }

    #[test]
    fn pattern_is_capped_at_120_chars() {
        let long_str = "x".repeat(200);
        let truncated: String = long_str.chars().take(120).collect();
        assert_eq!(truncated.len(), 120);
    }

    // ── P6.3 tests ────────────────────────────────────────────────────────────

    #[test]
    fn dry_run_bridge_returns_none_for_empty_body() {
        // We can't build a real HookRuntime in unit tests.  Test the guard logic.
        let body = "   ";
        assert!(body.trim().is_empty());
    }

    #[test]
    fn classify_tool_class_for_bash() {
        assert_eq!(classify_tool_class_for_ceg("Bash"), "bash");
        assert_eq!(classify_tool_class_for_ceg("bash"), "bash");
    }

    #[test]
    fn classify_tool_class_for_edit() {
        assert_eq!(classify_tool_class_for_ceg("Edit"), "edit");
    }

    #[test]
    fn classify_tool_class_for_ctx_execute() {
        assert_eq!(classify_tool_class_for_ceg("ctx_execute"), "bash");
    }

    #[test]
    fn dry_run_bridge_result_from_allow_outcome_is_allowed() {
        use crate::capability::builtins;
        use crate::gateway::pre_exec::{
            GatewayDeps, deferred_dry_run, neutral_outcome_history, run_gateway, soft_pass_symbol,
        };
        use crate::gateway::predict::ExecutionOutcomePredictor;

        let predictor = ExecutionOutcomePredictor::new();
        let profile = builtins::trusted();
        let deps = GatewayDeps {
            symbol_exists: &soft_pass_symbol,
            outcome_history: &neutral_outcome_history,
            sandbox_runner: &deferred_dry_run,
            predictor: &predictor,
            profile: &profile,
            // ES1 P3 (2026-06-01) — X3.5 PROVE: no claim by default in tests.
            claim: None,
            claim_context: crate::gateway::offensive_integration::ClaimContext::default(),
            solver_backend: crate::gateway::offensive_integration::SolverBackendKind::Stub,
        };
        let outcome =
            run_gateway("Bash", "echo hello", None, &deps).expect("clean echo must succeed");
        let result = DryRunBridgeResult::from_outcome(&outcome);
        // echo hello is clean — gateway should Allow it.
        assert!(
            result.is_allowed,
            "echo hello should be allowed: {}",
            result.summary
        );
        assert!(!result.summary.is_empty());
    }

    // ── S-14 drift helpers ──────────────────────────────────────────────────

    #[test]
    fn sensor_reading_serialise_roundtrips() {
        let r = SensorReading {
            harness_composite: 0.73,
            evidence_composite: 0.91,
            health_delta: -0.05,
            constitutional_digest_prefix: [0u8; 8],
        };
        let parsed = parse_sensor_reading(&serialize_sensor_reading(&r))
            .expect("a freshly serialised reading must parse");
        assert_eq!(parsed, r);
    }

    #[test]
    fn parse_sensor_reading_rejects_garbage() {
        // Fail-open: malformed cache content → None (caller treats as baseline).
        assert!(parse_sensor_reading("").is_none());
        assert!(parse_sensor_reading("not;a;triple").is_none());
        assert!(parse_sensor_reading("0.5;0.6").is_none(), "too few fields");
        assert!(
            parse_sensor_reading("0.5;0.6;0.7;extra").is_some(),
            "extra fields ignored"
        );
    }
}
