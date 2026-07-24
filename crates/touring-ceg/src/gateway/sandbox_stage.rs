//! Stage **X5 SANDBOX** of the Code Execution Gateway. Phase **P3.5** of CEG
//! Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X5 runs the code body **once, in the sandbox** — a real subprocess under a
//! wall-clock timeout and an output cap — before any unsandboxed execution is
//! ever permitted. The observed exit code, output size and truncation are
//! captured into a [`SandboxOutcome`] and attached to the evidence ledger, so
//! the X6/X7 gate can decide on *evidence of behaviour*, not just static
//! inspection.
//!
//! # Capability scope
//!
//! The dry-run is parameterised by a [`SandboxCapabilities`] derived from the
//! execution's [`CapabilityProfile`]: a
//! restrictive (`Deny`-default) profile gets a shorter leash. The deeper
//! kernel-level enforcement — feeding the profile's resource caps into the
//! subprocess via `apply_rlimit` / landlock — is **P4 (Sandbox Completion &
//! Hardening)**, consistent with the P2.4 split (P2.4-A `apply_rlimit` shipped;
//! P2.4-B landlock deferred). P3.5 delivers the X5 *stage*: the dry-run runs
//! under a capability-derived `SandboxConfig` and records which profile gated
//! it.
//!
//! # Plan note (VGP — FIX-S4)
//!
//! The plan names `execute_in_sandbox`; the verified in-repo symbol is
//! [`execute_in_sandbox_blocking`] — the synchronous wrapper, the correct
//! choice for the gateway's blocking transition.
//!
//! The runner is injected as a closure, so the
//! [`sandbox_dry_run`](Execution::sandbox_dry_run) transition is unit-testable
//! without spawning a subprocess; [`dry_run_in_sandbox`] is the production
//! runner the wiring (P3.7) passes.

use super::typestate::{Execution, Predicted, RawInvocation, SandboxTested};
use crate::capability::{CapabilityProfile, Decision};
use crate::gateway::sandbox_executor::{
    SandboxConfig, SandboxError, SandboxResult, execute_in_sandbox_blocking,
};
use crate::gateway::summarize::OutputSummary;
use serde::{Deserialize, Serialize};

/// The observed result of an X5 sandbox dry-run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxOutcome {
    /// Subprocess exit code; `-1` marks a timeout or a spawn failure.
    pub exit_code: i32,
    /// Bytes of stdout captured.
    pub output_bytes: u64,
    /// `true` when output was clipped at the configured cap.
    pub was_truncated: bool,
    /// `true` when the subprocess was killed at the wall-clock timeout.
    pub timed_out: bool,
    /// BLAKE3 hex digest of captured stdout (`""` when nothing ran).
    pub content_hash: String,
    /// Name of the [`CapabilityProfile`] the dry-run was gated by.
    pub capability_profile: String,
    /// C5 — inline, metadata-first digest of the captured output (exit code,
    /// error signatures, `file:line` refs). Surfaces failures without re-reading.
    pub summary: OutputSummary,
}

impl SandboxOutcome {
    /// Map a completed [`SandboxResult`] into an outcome.
    fn from_result(result: &SandboxResult, profile: &str) -> Self {
        Self {
            exit_code: result.exit_code,
            output_bytes: result.output_bytes,
            was_truncated: result.was_truncated,
            timed_out: false,
            content_hash: result.content_hash.clone(),
            capability_profile: profile.to_owned(),
            summary: result.summary.clone(),
        }
    }

    /// The outcome of a dry-run killed at the timeout.
    fn timed_out(profile: &str) -> Self {
        Self {
            exit_code: -1,
            output_bytes: 0,
            was_truncated: false,
            timed_out: true,
            content_hash: String::new(),
            capability_profile: profile.to_owned(),
            summary: OutputSummary::empty(-1),
        }
    }

    /// The outcome of a dry-run that never started (spawn / I/O failure).
    fn spawn_failed(profile: &str) -> Self {
        Self {
            exit_code: -1,
            output_bytes: 0,
            was_truncated: false,
            timed_out: false,
            content_hash: String::new(),
            capability_profile: profile.to_owned(),
            summary: OutputSummary::empty(-1),
        }
    }

    /// `true` when the dry-run ran to completion with a zero exit code.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.exit_code == 0 && !self.timed_out
    }
}

/// The sandbox parameters derived from an execution's [`CapabilityProfile`].
#[derive(Debug, Clone)]
pub struct SandboxCapabilities {
    /// Name of the capability profile gating the dry-run.
    pub profile_name: String,
    /// The subprocess configuration the dry-run runs under.
    pub config: SandboxConfig,
}

impl SandboxCapabilities {
    /// Derive the dry-run parameters from a capability profile.
    ///
    /// The subprocess timeout follows the profile's default decision: a
    /// `Deny`-default (restrictive) profile gets a short leash, an
    /// `Allow`-default (trusted) profile the full default.
    #[must_use]
    pub fn from_profile(profile: &CapabilityProfile) -> Self {
        let timeout_ms = match profile.default_decision() {
            Decision::Deny => 5_000,
            Decision::Prompt => 15_000,
            Decision::Allow => 30_000,
        };
        Self {
            profile_name: profile.name().to_owned(),
            config: SandboxConfig {
                timeout_ms,
                ..SandboxConfig::default()
            },
        }
    }
}

/// **The X5 production runner.** Runs the code body once in the sandbox under
/// the given [`SandboxCapabilities`] and captures the [`SandboxOutcome`].
///
/// Reuses [`execute_in_sandbox_blocking`]; a timeout or spawn failure is
/// folded into the outcome rather than propagated, so X5 always yields an
/// outcome for the gate to weigh.
#[must_use]
pub fn dry_run_in_sandbox(raw: &RawInvocation, caps: &SandboxCapabilities) -> SandboxOutcome {
    let args = serde_json::json!({ "command": raw.payload });
    match execute_in_sandbox_blocking(&raw.tool, args, caps.config.clone()) {
        Ok(result) => SandboxOutcome::from_result(&result, &caps.profile_name),
        Err(SandboxError::Timeout(_)) => SandboxOutcome::timed_out(&caps.profile_name),
        Err(_) => SandboxOutcome::spawn_failed(&caps.profile_name),
    }
}

impl Execution<Predicted> {
    /// **X5 SANDBOX** — run the code body once in the sandbox, attach the
    /// [`SandboxOutcome`] to the evidence ledger, and advance to
    /// [`SandboxTested`].
    ///
    /// `runner` performs the actual dry-run; the production wiring (P3.7)
    /// passes a closure backed by [`dry_run_in_sandbox`], while tests pass a
    /// mock so no subprocess is spawned.
    pub fn sandbox_dry_run<F>(mut self, runner: F) -> Execution<SandboxTested>
    where
        F: Fn(&RawInvocation) -> SandboxOutcome,
    {
        let outcome = runner(self.raw());
        self.evidence_mut().sandbox_outcome = Some(outcome);
        self.advance()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::{ExecutionOutcomePredictor, OutcomeStats, capture_tool_call};

    fn predicted() -> Execution<Predicted> {
        capture_tool_call("Bash", "echo dry-run", None)
            .expect("Bash is code-bearing")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true)
            .prove_claim(
                None,
                crate::gateway::offensive_integration::SolverBackendKind::Stub,
                &crate::gateway::offensive_integration::ClaimContext::default(),
            )
            .predict(&ExecutionOutcomePredictor::default(), |_| {
                OutcomeStats::default()
            })
    }

    fn mock_outcome() -> SandboxOutcome {
        SandboxOutcome {
            exit_code: 0,
            output_bytes: 12,
            was_truncated: false,
            timed_out: false,
            content_hash: "abc123".to_owned(),
            capability_profile: "mock".to_owned(),
            summary: OutputSummary::empty(0),
        }
    }

    #[test]
    fn outcome_succeeded_on_zero_exit() {
        assert!(mock_outcome().succeeded());
    }

    #[test]
    fn outcome_failed_on_nonzero_exit() {
        let o = SandboxOutcome {
            exit_code: 1,
            ..mock_outcome()
        };
        assert!(!o.succeeded());
    }

    #[test]
    fn outcome_failed_on_timeout() {
        let o = SandboxOutcome::timed_out("p");
        assert!(!o.succeeded());
        assert!(o.timed_out);
        assert_eq!(o.exit_code, -1);
    }

    #[test]
    fn outcome_spawn_failed_is_not_a_timeout() {
        let o = SandboxOutcome::spawn_failed("p");
        assert!(!o.succeeded());
        assert!(!o.timed_out);
        assert_eq!(o.exit_code, -1);
    }

    #[test]
    fn outcome_from_result_maps_every_field() {
        let result = SandboxResult {
            exit_code: 0,
            output_bytes: 99,
            was_truncated: true,
            content_hash: "deadbeef".to_owned(),
            stored_path: None,
            summary: OutputSummary::empty(0),
        };
        let o = SandboxOutcome::from_result(&result, "readonly");
        assert_eq!(o.exit_code, 0);
        assert_eq!(o.output_bytes, 99);
        assert!(o.was_truncated);
        assert!(!o.timed_out);
        assert_eq!(o.content_hash, "deadbeef");
        assert_eq!(o.capability_profile, "readonly");
    }

    #[test]
    fn outcome_serde_roundtrip() {
        let o = mock_outcome();
        let json = serde_json::to_string(&o).expect("serialize");
        let back: SandboxOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(o, back);
    }

    #[test]
    fn capabilities_from_deny_profile_get_a_short_leash() {
        let profile = CapabilityProfile::new("locked", Decision::Deny);
        let caps = SandboxCapabilities::from_profile(&profile);
        assert_eq!(caps.config.timeout_ms, 5_000);
        assert_eq!(caps.profile_name, "locked");
    }

    #[test]
    fn capabilities_from_allow_profile_get_the_full_timeout() {
        let profile = CapabilityProfile::new("open", Decision::Allow);
        assert_eq!(
            SandboxCapabilities::from_profile(&profile)
                .config
                .timeout_ms,
            30_000
        );
    }

    #[test]
    fn capabilities_from_prompt_profile_get_a_medium_leash() {
        let profile = CapabilityProfile::new("ask", Decision::Prompt);
        assert_eq!(
            SandboxCapabilities::from_profile(&profile)
                .config
                .timeout_ms,
            15_000
        );
    }

    #[test]
    fn sandbox_dry_run_transition_attaches_and_advances() {
        let tested = predicted().sandbox_dry_run(|_| mock_outcome());
        assert_eq!(tested.ordinal(), 6);
        assert_eq!(tested.stage(), "X5-SANDBOX");
        let outcome = tested
            .evidence()
            .sandbox_outcome
            .as_ref()
            .expect("sandbox_dry_run must attach a SandboxOutcome");
        assert!(outcome.succeeded());
        assert_eq!(outcome.capability_profile, "mock");
    }

    #[test]
    fn sandbox_dry_run_records_runner_outcome_verbatim() {
        let failing = SandboxOutcome {
            exit_code: 7,
            ..mock_outcome()
        };
        let tested = predicted().sandbox_dry_run(|_| failing.clone());
        assert_eq!(
            tested.evidence().sandbox_outcome.as_ref().expect("outcome"),
            &failing
        );
    }

    #[test]
    fn sandbox_dry_run_passes_the_raw_invocation_to_the_runner() {
        let tested = predicted().sandbox_dry_run(|raw| {
            assert_eq!(raw.payload, "echo dry-run");
            mock_outcome()
        });
        assert_eq!(tested.ordinal(), 6);
    }

    // ── E2E: the real sandbox subprocess ──────────────────────────────────

    #[test]
    fn e2e_real_sandbox_runs_echo_to_zero_exit() {
        let raw = RawInvocation::new("Bash", "echo hello-ceg");
        let caps =
            SandboxCapabilities::from_profile(&CapabilityProfile::new("e2e", Decision::Allow));
        let outcome = dry_run_in_sandbox(&raw, &caps);
        assert!(outcome.succeeded(), "echo should exit 0: {outcome:?}");
        assert_eq!(outcome.capability_profile, "e2e");
    }

    #[test]
    fn e2e_real_sandbox_nonzero_command_is_not_a_success() {
        let raw = RawInvocation::new("Bash", "exit 3");
        let caps =
            SandboxCapabilities::from_profile(&CapabilityProfile::new("e2e", Decision::Allow));
        let outcome = dry_run_in_sandbox(&raw, &caps);
        assert!(
            !outcome.succeeded(),
            "a non-zero command must not be a success: {outcome:?}"
        );
    }

    #[test]
    fn e2e_full_chain_to_sandbox_tested_with_mock() {
        let tested = capture_tool_call("Bash", "echo chain", None)
            .expect("admitted at X0")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true)
            .prove_claim(
                None,
                crate::gateway::offensive_integration::SolverBackendKind::Stub,
                &crate::gateway::offensive_integration::ClaimContext::default(),
            )
            .predict(&ExecutionOutcomePredictor::new(), |_| {
                OutcomeStats::default()
            })
            .sandbox_dry_run(|_| mock_outcome());
        assert_eq!(tested.ordinal(), 6);
        // ES1 P3 (2026-06-01): 6 advances (X0→X1→X2→X3→X3.5→X4→X5),
        // stage_log has 7 records (X0 initial + 6 transitions).
        assert_eq!(tested.evidence().stage_log.len(), 7);
    }

    #[test]
    fn e2e_full_chain_with_the_real_dry_run() {
        // X0 → … → X5 with the real sandbox running a trivial command.
        let caps =
            SandboxCapabilities::from_profile(&CapabilityProfile::new("e2e-real", Decision::Allow));
        let tested = capture_tool_call("Bash", "echo end-to-end", None)
            .expect("admitted at X0")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true)
            .prove_claim(
                None,
                crate::gateway::offensive_integration::SolverBackendKind::Stub,
                &crate::gateway::offensive_integration::ClaimContext::default(),
            )
            .predict(&ExecutionOutcomePredictor::new(), |_| {
                OutcomeStats::default()
            })
            .sandbox_dry_run(|raw| dry_run_in_sandbox(raw, &caps));
        assert_eq!(tested.ordinal(), 6);
        let outcome = tested.evidence().sandbox_outcome.as_ref().expect("outcome");
        assert!(
            outcome.succeeded(),
            "the real echo dry-run should succeed: {outcome:?}"
        );
    }
}
