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
use touring_foundation::run_outcome::ExitStatus;

/// The observed result of an X5 sandbox dry-run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "SandboxOutcomeWire", from = "SandboxOutcomeWire")]
pub struct SandboxOutcome {
    /// How the subprocess ended; sentinel integers exist only at the wire edge.
    pub exit_code: ExitStatus,
    /// Bytes of stdout captured.
    pub output_bytes: u64,
    /// `true` when output was clipped at the configured cap.
    pub was_truncated: bool,
    /// Wire discriminator ONLY: which of the two historical JSON forms a timeout takes —
    /// `-1/true` (the dry-run killed here) or the executor's salvaged `-2/false`. Both are
    /// `exit_code == ExitStatus::TimedOut`; decide on [`ExitStatus::is_timed_out`], never on
    /// this flag (it is `false` for the salvaged form). Kept so the serialization stays
    /// byte-identical to the journal.
    pub timed_out: bool,
    /// BLAKE3 hex digest of captured stdout (`""` when nothing ran).
    pub content_hash: String,
    /// Name of the [`CapabilityProfile`] the dry-run was gated by.
    pub capability_profile: String,
    /// C5 — inline, metadata-first digest of the captured output (exit code,
    /// error signatures, `file:line` refs). Surfaces failures without re-reading.
    pub summary: OutputSummary,
}

/// The historical JSON shape. Timeout and no-exit BOTH used `-1`; the separate
/// boolean distinguishes them on disk. Keeping this adapter outside the typed
/// value makes the serialization byte-compatible instead of changing `-1` to
/// the journal's timeout sentinel `-2`.
#[derive(Serialize, Deserialize)]
struct SandboxOutcomeWire {
    exit_code: i32,
    output_bytes: u64,
    was_truncated: bool,
    timed_out: bool,
    content_hash: String,
    capability_profile: String,
    summary: OutputSummary,
}

impl From<SandboxOutcome> for SandboxOutcomeWire {
    fn from(outcome: SandboxOutcome) -> Self {
        Self {
            exit_code: if outcome.timed_out {
                -1
            } else {
                outcome.exit_code.to_wire()
            },
            output_bytes: outcome.output_bytes,
            was_truncated: outcome.was_truncated,
            timed_out: outcome.timed_out,
            content_hash: outcome.content_hash,
            capability_profile: outcome.capability_profile,
            summary: outcome.summary,
        }
    }
}

impl From<SandboxOutcomeWire> for SandboxOutcome {
    fn from(wire: SandboxOutcomeWire) -> Self {
        Self {
            exit_code: if wire.timed_out {
                ExitStatus::TimedOut
            } else {
                ExitStatus::from_wire(wire.exit_code)
            },
            output_bytes: wire.output_bytes,
            was_truncated: wire.was_truncated,
            timed_out: wire.timed_out,
            content_hash: wire.content_hash,
            capability_profile: wire.capability_profile,
            summary: wire.summary,
        }
    }
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
            exit_code: ExitStatus::TimedOut,
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
            exit_code: ExitStatus::NoExitCode,
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
        self.exit_code.is_zero()
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
            exit_code: ExitStatus::Code(0),
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
            exit_code: ExitStatus::Code(1),
            ..mock_outcome()
        };
        assert!(!o.succeeded());
    }

    #[test]
    fn outcome_failed_on_timeout() {
        let o = SandboxOutcome::timed_out("p");
        assert!(!o.succeeded());
        assert!(o.exit_code.is_timed_out());
        assert_eq!(o.exit_code, ExitStatus::TimedOut);
    }

    #[test]
    fn outcome_spawn_failed_is_not_a_timeout() {
        let o = SandboxOutcome::spawn_failed("p");
        assert!(!o.succeeded());
        assert!(!o.exit_code.is_timed_out());
        assert_eq!(o.exit_code, ExitStatus::NoExitCode);
    }

    #[test]
    fn outcome_from_result_maps_every_field() {
        let result = SandboxResult {
            exit_code: ExitStatus::Code(0),
            output_bytes: 99,
            was_truncated: true,
            content_hash: "deadbeef".to_owned(),
            stored_path: None,
            summary: OutputSummary::empty(0),
            stderr: String::new(),
            stderr_truncated: false,
            tmp_bytes: 0,
            signal: None,
            capped_files: Vec::new(),
        };
        let o = SandboxOutcome::from_result(&result, "readonly");
        assert_eq!(o.exit_code, ExitStatus::Code(0));
        assert_eq!(o.output_bytes, 99);
        assert!(o.was_truncated);
        assert!(!o.exit_code.is_timed_out());
        assert_eq!(o.content_hash, "deadbeef");
        assert_eq!(o.capability_profile, "readonly");
    }

    #[test]
    fn outcome_from_timeout_result_keeps_the_minus_two_fallback_wire() {
        let result = SandboxResult {
            exit_code: ExitStatus::TimedOut,
            output_bytes: 0,
            was_truncated: true,
            content_hash: String::new(),
            stored_path: None,
            summary: OutputSummary::empty(-2),
            stderr: String::new(),
            stderr_truncated: false,
            tmp_bytes: 0,
            signal: None,
            capped_files: Vec::new(),
        };
        let outcome = SandboxOutcome::from_result(&result, "fallback");
        let wire = serde_json::to_value(outcome).expect("serialize");
        assert_eq!(wire["exit_code"], -2);
        assert_eq!(wire["timed_out"], false);
    }

    /// Replays the real run journal through the legacy and typed wire shapes.
    /// Opt-in because the journal is local runtime data, never a repo fixture.
    #[test]
    #[ignore = "requires TOURING_RUN_OUTCOME_REPLAY and TOURING_RUN_OUTCOME_REPLAY_LINES"]
    fn real_run_outcome_wire_replay_is_byte_identical() {
        use std::io::{BufRead, BufReader};

        #[derive(Serialize)]
        struct LegacySandboxOutcome {
            exit_code: i32,
            output_bytes: u64,
            was_truncated: bool,
            timed_out: bool,
            content_hash: String,
            capability_profile: String,
            summary: OutputSummary,
        }

        let path = std::env::var("TOURING_RUN_OUTCOME_REPLAY").expect("journal path");
        let limit = std::env::var("TOURING_RUN_OUTCOME_REPLAY_LINES")
            .expect("line limit")
            .parse::<usize>()
            .expect("numeric line limit");
        let reader = BufReader::new(std::fs::File::open(path).expect("open journal"));
        let (mut lines, mut compared, mut without_exit, mut malformed) = (0, 0, 0, 0);
        for line_text in reader.lines().take(limit) {
            let line_text = line_text.expect("read journal line");
            lines += 1;
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line_text) else {
                malformed += 1;
                continue;
            };
            let Some(exit_code) = value
                .get("exit_code")
                .and_then(serde_json::Value::as_i64)
                .and_then(|code| i32::try_from(code).ok())
            else {
                without_exit += 1;
                continue;
            };
            let legacy = LegacySandboxOutcome {
                exit_code,
                output_bytes: 0,
                was_truncated: false,
                timed_out: value
                    .get("timed_out")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                content_hash: String::new(),
                capability_profile: "replay".to_owned(),
                summary: OutputSummary::empty(exit_code),
            };
            let expected = serde_json::to_string(&legacy).expect("legacy JSON");
            let typed: SandboxOutcome =
                serde_json::from_str(&expected).expect("typed outcome deserializes");
            let actual = serde_json::to_string(&typed).expect("typed JSON");
            assert_eq!(actual, expected, "wire differs at journal line {lines}");
            compared += 1;
        }
        assert_eq!(lines, limit, "the replay must cover the requested corpus");
        assert!(compared > 0, "an empty comparison proves nothing");
        eprintln!(
            "run_outcome replay: lines={lines} compared={compared} without_exit={without_exit} malformed={malformed}"
        );
    }

    #[test]
    fn outcome_serde_roundtrip() {
        let o = mock_outcome();
        let json = serde_json::to_string(&o).expect("serialize");
        let back: SandboxOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(o, back);
    }

    #[test]
    fn outcome_serde_keeps_the_legacy_minus_one_timeout_wire() {
        let outcome = SandboxOutcome::timed_out("legacy");
        let json = serde_json::to_string(&outcome).expect("serialize");
        assert_eq!(
            json,
            r#"{"exit_code":-1,"output_bytes":0,"was_truncated":false,"timed_out":true,"content_hash":"","capability_profile":"legacy","summary":{"exit_code":-1,"total_bytes":0,"error_lines":[],"file_refs":[],"counts":{},"head_tail":[],"elided_lines":0,"truncated":false}}"#
        );
        let back: SandboxOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, outcome);
        let spawn = SandboxOutcome::spawn_failed("legacy");
        let wire = serde_json::to_value(spawn).expect("serialize");
        assert_eq!(wire["exit_code"], -1);
        assert_eq!(wire["timed_out"], false);
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
            exit_code: ExitStatus::Code(7),
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
