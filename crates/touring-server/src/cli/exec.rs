//! `touring exec` — the Code Execution Gateway CLI. Phase **P3.7** of CEG Pln2
//! (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! `touring exec '<command>'` drives a shell command through the full
//! `X0..X9` gateway pipeline and reports the verdict — `Allow` / `Warn` /
//! `Deny` — with the composite score, the per-signal reasons and a canonical
//! fix. It is a *decision* tool: it answers "would this command pass the
//! gateway?" without running it (the X5 sandbox is deferred — see below).
//!
//! ```text
//! touring exec 'cargo test'                 # default: Trusted profile
//! touring exec --profile sandboxed 'ls'     # ask a stricter question
//! touring exec --sandbox 'echo hi'          # opt into the real X5 dry-run
//! touring exec --real-exec 'echo hi'        # ACTUALLY spawn under landlock + lock guard
//! touring exec -j 'rm -rf /'                # machine-readable evidence
//! ```
//!
//! Exit code: `Allow` / `Warn` → `0`; `Deny` → non-zero — so a caller can do
//! `touring exec 'cmd' && run-it`. With `--real-exec`, the spawned command's
//! own exit code is preserved; a `SandboxError::Conflict` becomes exit **75
//! (EX_TEMPFAIL)** — a transient signal that another execution is holding the
//! write paths and the caller should retry.
//!
//! By default the X5 sandbox dry-run is **deferred** (it does not execute):
//! the gateway decides on the non-executing analyses. `--sandbox` opts into the
//! guarded real runner, which still refuses the destructive catalogue outright.
//! See `touring_hooks::gateway::pre_exec` for the rationale.
//!
//! # `--real-exec` (ES3 P3, 2026-06-02)
//!
//! When the gateway verdict is `Allow` or `Warn`, `--real-exec` invokes
//! `real_exec_with_locks` which acquires a transactional `TxnPermit` via
//! `ExecPool::global`, then spawns the command under
//! [`SupervisionPolicy::confined`] (landlock LSM + rlimit on Linux). The
//! permit spans the actual I/O and is released on drop. The lock state is
//! global state via the `ExecPool` singleton — `GatewayDeps` is **not
//! extended** for this wave (P3 leftover pattern preserved).
//!
//! `--real-exec` is OPT-IN; the default behavior (analysis-only, no spawn)
//! is unchanged. Only `run` (`touring exec`) and `run_speculative` (`touring
//! exec-speculative`) honor the flag — `run_plan_gated`, `run_verified_depth`,
//! and `run_evidence` remain analysis-only by design.

use anyhow::anyhow;
use touring_hooks::action_signature::ActionSignature;
use touring_hooks::capability::{CapabilityProfile, builtins};
use touring_hooks::gateway::supervised::{SupervisionPolicy, run_supervised_with_locks};
use touring_hooks::gateway::txn::{AccessDeclaration, AccessPath, AcquireResult, TxnLockManager};
use touring_hooks::gateway::{
    CandidateAction, ExecutionOutcomePredictor, GatewayDeps, GatewayError, GatewayOutcome,
    RawInvocation, SandboxCapabilities, SandboxOutcome, Verdict, deferred_dry_run, guarded_dry_run,
    neutral_outcome_history, rank_by_predicted, record_ceg_sandboxed, record_verdict_counters,
    run_gateway, run_gateway_speculative, soft_pass_symbol,
};
use touring_hooks::sandbox_executor::{SandboxConfig, SandboxError};
use touring_intelligence::reasoning::{MCTSConfig, MCTSEngine, MCTSResult};

use super::{common::parse_global_flags, daemon_query};

/// The parsed `touring exec` argument set.
#[derive(Debug)]
struct ExecArgs {
    /// The shell command to gate.
    command: String,
    /// The capability profile name — `sandboxed` / `readonly` / `trusted`.
    profile: String,
    /// `--sandbox`: use the guarded real X5 runner instead of the deferred one.
    use_real_sandbox: bool,
    /// `--real-exec` (ES3 P3, 2026-06-02): when the gateway verdict is
    /// `Allow` / `Warn`, actually spawn the command via
    /// [`real_exec_with_locks`] (landlock LSM + lost-update guard). Opt-in,
    /// independent of `use_real_sandbox` (which toggles X5 dry-run mode).
    use_real_exec: bool,
    /// `--intent <text>`: the human-stated intent for the command.
    intent: Option<String>,
}

/// The capability profiles `touring exec` accepts via `--profile`.
const KNOWN_PROFILES: &[&str] = &["sandboxed", "readonly", "trusted"];

/// Extract the sub-command arguments from the full process argv.
///
/// `main.rs` dispatches every table command with the **entire**
/// `std::env::args()` vector: `argv[0]` is the binary, `argv[1]` is the
/// subcommand name (`exec`). Both are skipped so the remainder is exactly the
/// command to gate — without this, `touring` and `exec` would themselves leak
/// into the gated command. Defensive against a short argv (never panics).
fn sub_command_args(argv: &[String]) -> &[String] {
    argv.get(2..).unwrap_or(&[])
}

/// Parse the `touring exec` arguments (binary + subcommand + global flags
/// already stripped).
///
/// Every non-flag token is a word of the command, joined with spaces — so
/// `touring exec rm -rf /tmp/x` and `touring exec 'rm -rf /tmp/x'` are
/// equivalent. The default profile is `trusted` (the first-party dev
/// environment question).
fn parse_exec_args(rest: &[String]) -> anyhow::Result<ExecArgs> {
    let mut profile = "trusted".to_owned();
    let mut use_real_sandbox = false;
    let mut use_real_exec = false;
    let mut intent: Option<String> = None;
    let mut words: Vec<String> = Vec::new();

    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--sandbox" => use_real_sandbox = true,
            "--real-exec" => use_real_exec = true,
            "--profile" => {
                profile = iter
                    .next()
                    .ok_or_else(|| anyhow!("--profile requires a value"))?
                    .clone();
            }
            "--intent" => {
                intent = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("--intent requires a value"))?
                        .clone(),
                );
            }
            word => words.push(word.to_owned()),
        }
    }

    if !KNOWN_PROFILES.contains(&profile.as_str()) {
        return Err(anyhow!(
            "unknown profile '{profile}' — expected one of: {}",
            KNOWN_PROFILES.join(", ")
        ));
    }
    let command = words.join(" ");
    if command.trim().is_empty() {
        return Err(GatewayError::MissingCommand.into());
    }
    Ok(ExecArgs {
        command,
        profile,
        use_real_sandbox,
        use_real_exec,
        intent,
    })
}

/// Resolve a profile name to a concrete [`CapabilityProfile`].
///
/// `sandboxed` and `readonly` are rooted at the current working directory;
/// `trusted` needs no path.
fn resolve_profile(name: &str) -> anyhow::Result<CapabilityProfile> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    match name {
        "sandboxed" => Ok(builtins::sandboxed(&cwd)),
        "readonly" => Ok(builtins::read_only(&cwd)),
        "trusted" => Ok(builtins::trusted()),
        other => Err(anyhow!("unknown profile '{other}'")),
    }
}

/// Drive a command through the gateway — the pure, I/O-free core of
/// `touring exec`. Builds the production [`GatewayDeps`] and runs the pipeline.
fn gate_command(
    command: &str,
    profile_name: &str,
    use_real_sandbox: bool,
    intent: Option<String>,
) -> anyhow::Result<GatewayOutcome> {
    let profile = resolve_profile(profile_name)?;
    let predictor = ExecutionOutcomePredictor::new();
    let caps = SandboxCapabilities::from_profile(&profile);
    let guarded = |raw: &RawInvocation| guarded_dry_run(raw, &caps);
    // The deferred runner is the safe default; `--sandbox` opts into the
    // guarded real runner. Both coerce to the `&dyn Fn` the deps expect.
    let sandbox_runner: &dyn Fn(&RawInvocation) -> SandboxOutcome = if use_real_sandbox {
        &guarded
    } else {
        &deferred_dry_run
    };
    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        outcome_history: &neutral_outcome_history,
        sandbox_runner,
        predictor: &predictor,
        profile: &profile,
        // P3 (2026-06-01): X3.5 PROVE — opt-in via the structured
        // `prove_claim` pipeline. Default: no claim, Stub backend.
        claim: None,
        claim_context: touring_hooks::offensive_integration::ClaimContext::default(),
        solver_backend: touring_hooks::offensive_integration::SolverBackendKind::Stub,
    };
    run_gateway("Bash", command, intent, &deps).map_err(anyhow::Error::from)
}

/// Actually spawn `command` under the supervised X8 path with the transactional
/// lost-update guard. **ES3 P3 (2026-06-02)** — wired by [`run`] and
/// [`run_speculative`] when `--real-exec` is set on the parsed args.
///
/// # Returns
///
/// `Ok(exit_code)` — the command spawned and ran to completion. `exit_code`
/// is the child's actual exit code (0 on success, non-zero on command-level
/// failure). The caller maps a non-zero exit code to [`std::process::exit`]
/// so it propagates as the `touring exec` process exit code.
///
/// # Errors
///
/// * [`SandboxError::Conflict`] → `anyhow::Error` (caller maps to exit
///   **75 — EX_TEMPFAIL**, the Unix convention for transient errors that
///   should be retried; the conflicting execution id is included in the
///   message so the caller can correlate with concurrent invocations).
/// * Other [`SandboxError`] variants → `anyhow::Error` (caller exits 1).
/// * `tokio` runtime build failure → `anyhow::Error`.
/// * `std::env::current_dir` failure → `anyhow::Error`.
///
/// # Lock manager access
///
/// The transactional lock state lives in [`ExecPool::global`] — a singleton
/// reached via `static`. `GatewayDeps` is **not** extended for this wave
/// (the P3 leftover pattern is preserved); the CLI gate path
/// ([`gate_command`]) and the real-exec path are kept disjoint with respect
/// to `GatewayDeps`.
///
/// [`ExecPool::global`]: touring_hooks::gateway::ExecPool::global
fn real_exec_with_locks(command: &str, _parsed: &ExecArgs) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let policy = SupervisionPolicy::confined(&cwd, vec![cwd.clone()]);
    let config = SandboxConfig::default();
    let access_decl = AccessDeclaration::from_tool_payload_full("Bash", command);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow!("tokio runtime build: {e}"))?;
    let outcome = rt
        .block_on(run_supervised_with_locks(
            command,
            &policy,
            &config,
            access_decl,
        ))
        .map_err(|e| match e {
            SandboxError::Conflict {
                conflicting_execution_id,
                resource,
            } => {
                let where_text = if resource.is_empty() {
                    String::new()
                } else {
                    format!(" on {resource}")
                };
                anyhow!(
                    "concurrent write conflict{where_text} (held by execution {conflicting_execution_id})"
                )
            }
            other => anyhow!("supervised exec: {other}"),
        })?;

    Ok(outcome.result.exit_code)
}

/// Render a gateway outcome as a human-readable report.
fn format_decision_human(outcome: &GatewayOutcome) -> String {
    let decision = &outcome.decision;
    let profile = outcome
        .evidence
        .gate_report
        .as_ref()
        .map_or("?", |report| report.profile_name.as_str());
    let mut out = format!(
        "CEG gateway — {}\n  verdict:   {:?}\n  composite: {:.2}\n  profile:   {profile}",
        outcome.id, decision.verdict, decision.composite_score
    );
    if decision.reasons.is_empty() {
        out.push_str("\n  reasons:   (none — every signal is clean)");
    } else {
        out.push_str("\n  reasons:");
        for reason in &decision.reasons {
            out.push_str("\n    · ");
            out.push_str(reason);
        }
    }
    if let Some(fix) = &decision.canonical_fix {
        out.push_str("\n  fix: ");
        out.push_str(fix);
    }
    out
}

/// Map a [`Verdict`] to the matching `cli-gate-event` token string.
///
/// Wave 6 (2026-05-23) — CEG observability boundary fix. The names must
/// match the `match` arms in `touring_hooks::cli_handlers::cli_gate_event`.
fn verdict_event_token(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Allow => "fast_path",
        Verdict::Warn => "workflow_advice",
        Verdict::Deny => "blocked",
    }
}

/// Mirror an [`Verdict`] into the daemon's CEG counters via the
/// `cli-gate-event` IPC handler.
///
/// **Fail-open**: a daemon-down case must not break `touring exec`. Any
/// IPC error is swallowed silently — the verdict still travels back to the
/// caller via the human/JSON output and the exit code.
///
/// Wave 6 (2026-05-23) — closes the CEG observability boundary so that
/// `touring exec` invocations are visible in `touring gate-metrics -j`.
/// Without this bridge the daemon's counters would stay at zero forever
/// because the CLI process and the daemon process have independent
/// `static GLOBAL_METRICS: OnceLock<Metrics>` atomics.
///
/// Wave 7.B (2026-05-23) — when `sandboxed=true` (the caller passed
/// `--sandbox`, opting into the real X5 guarded runner), an extra
/// `"sandboxed"` event is appended so `ceg_sandboxed_count` reflects the
/// number of real X5 invocations versus the deferred default.
fn emit_gate_event_to_daemon(verdict: Verdict, sandboxed: bool) {
    let mut events = vec!["captured", verdict_event_token(verdict)];
    if sandboxed {
        events.push("sandboxed");
    }
    let payload = serde_json::json!({ "events": events });
    let _ = daemon_query("cli-gate-event", payload);
}

/// `touring exec` entry point — registered in the CLI command table.
///
/// `args` is the full process argv; `sub_command_args` strips the binary and
/// the `exec` subcommand name before the command is parsed. Prints the gateway
/// decision (human-readable, or JSON with `-j`) and maps the verdict to the
/// process exit code: `Allow` / `Warn` succeed, `Deny` fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let parsed = parse_exec_args(&rest)?;
    let outcome = gate_command(
        &parsed.command,
        &parsed.profile,
        parsed.use_real_sandbox,
        parsed.intent.clone(),
    )?;

    // Wave 6 (2026-05-23) — CEG observability boundary fix.
    // (1) Mirror the verdict into THIS process's static counters — `run_gateway`
    //     only fires `record_ceg_captured` at X0; the verdict counters are wired
    //     in `pre_exec::gate_hook_input` (daemon hook path) but NOT here in the
    //     direct CLI path.
    // (2) Push both `captured` and the verdict event to the daemon so the
    //     production snapshot in `touring gate-metrics -j` reflects every
    //     `touring exec` invocation (without this, the daemon's counters would
    //     stay at zero — counters die when the CLI process exits).
    record_verdict_counters(outcome.decision.verdict);
    // Wave 7.B (2026-05-23) — `use_real_sandbox` toggles the `sandboxed` event
    // so `ceg_sandboxed_count` reflects actual X5 guarded invocations vs the
    // deferred default (the safe no-spawn path).
    if parsed.use_real_sandbox {
        record_ceg_sandboxed();
    }
    emit_gate_event_to_daemon(outcome.decision.verdict, parsed.use_real_sandbox);

    if flags.json {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        println!("{}", format_decision_human(&outcome));
    }

    match outcome.decision.verdict {
        Verdict::Allow | Verdict::Warn => {
            // ES3 P3 (2026-06-02) — only when the caller explicitly opts in
            // via `--real-exec`, hand the command off to the supervised X8
            // path with the lost-update guard. Conflict → exit 75
            // (EX_TEMPFAIL); the command's own non-zero exit code is
            // preserved via `std::process::exit`. Verdict=Deny is unchanged
            // (defence-in-depth: the gateway refusal is final, no spawn).
            if parsed.use_real_exec {
                let exit_code = real_exec_with_locks(&parsed.command, &parsed).map_err(|e| {
                    // Distinguish the transient lock conflict (exit 75)
                    // from other supervised-exec failures (exit 1 via
                    // the default `anyhow::Error` propagation in
                    // `main.rs`). We surface the marker as part of the
                    // error message so a caller can grep for it.
                    if e.to_string().contains("concurrent write conflict") {
                        anyhow!("{e} [exit 75 EX_TEMPFAIL]")
                    } else {
                        e
                    }
                })?;
                if exit_code != 0 {
                    std::process::exit(exit_code);
                }
            }
            Ok(())
        }
        Verdict::Deny => Err(anyhow!("gateway verdict: Deny")),
    }
}

// ── S-12 — `touring exec-speculative` (speculative batch accept-prefix) ──────────

/// The parsed `touring exec-speculative` argument set.
#[derive(Debug)]
struct SpeculativeArgs {
    /// One candidate shell command per entry (shell-quote multi-word commands).
    candidates: Vec<String>,
    /// The capability profile name — `sandboxed` / `readonly` / `trusted`.
    profile: String,
    /// `--sandbox`: use the guarded real X5 runner instead of the deferred one.
    use_real_sandbox: bool,
    /// `--real-exec` (ES3 P3, 2026-06-02): when the speculative driver
    /// produces an accepted prefix, actually spawn each accepted candidate
    /// sequentially via [`real_exec_with_locks`]. **Lossless contract** — a
    /// [`SandboxError::Conflict`] on candidate N truncates the prefix at N
    /// (the remaining accepted candidates that *might* run on disjoint write
    /// paths are NOT attempted; conservative — re-run with a smaller batch
    /// is the recovery).
    use_real_exec: bool,
}

/// Parse `exec-speculative` args: `--profile <p>` / `--sandbox` flags, then one
/// positional candidate command per remaining arg.
fn parse_speculative_args(rest: &[String]) -> anyhow::Result<SpeculativeArgs> {
    let mut profile = "trusted".to_owned();
    let mut use_real_sandbox = false;
    let mut use_real_exec = false;
    let mut candidates = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--profile" => {
                let name = it
                    .next()
                    .ok_or_else(|| anyhow!("--profile needs a value"))?;
                if !KNOWN_PROFILES.contains(&name.as_str()) {
                    return Err(anyhow!(
                        "unknown profile '{name}'; known: {}",
                        KNOWN_PROFILES.join(", ")
                    ));
                }
                profile = name.clone();
            }
            "--sandbox" => use_real_sandbox = true,
            "--real-exec" => use_real_exec = true,
            other => candidates.push(other.to_owned()),
        }
    }
    if candidates.is_empty() {
        return Err(anyhow!(
            "exec-speculative needs at least one candidate command"
        ));
    }
    Ok(SpeculativeArgs {
        candidates,
        profile,
        use_real_sandbox,
        use_real_exec,
    })
}

/// `touring exec-speculative` entry point — S-12's live consumer.
///
/// Gates a *batch* of candidate commands and reports the longest valid leading
/// prefix (the action-level analogue of EAGLE speculative decoding): candidates
/// are ranked most-likely-to-succeed first (the S-11 online model), then run
/// through the full gateway in order, accepting each non-`Deny` and stopping at
/// the first `Deny` — lossless truncation, nothing past the failure is drafted.
///
/// ```text
/// touring exec-speculative 'echo a' 'echo b' 'rm -rf /' 'echo d'
/// #  ✓ echo a / ✓ echo b / ✗ truncated at rm -rf / (echo d never drafted)
/// ```
pub fn run_speculative(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let parsed = parse_speculative_args(&rest)?;

    let profile = resolve_profile(&parsed.profile)?;
    let predictor = ExecutionOutcomePredictor::new();
    let caps = SandboxCapabilities::from_profile(&profile);
    let guarded = |raw: &RawInvocation| guarded_dry_run(raw, &caps);
    let sandbox_runner: &dyn Fn(&RawInvocation) -> SandboxOutcome = if parsed.use_real_sandbox {
        &guarded
    } else {
        &deferred_dry_run
    };
    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        outcome_history: &neutral_outcome_history,
        sandbox_runner,
        predictor: &predictor,
        profile: &profile,
        // P3 (2026-06-01): X3.5 PROVE — opt-in via the structured
        // `prove_claim` pipeline. Default: no claim, Stub backend.
        claim: None,
        claim_context: touring_hooks::offensive_integration::ClaimContext::default(),
        solver_backend: touring_hooks::offensive_integration::SolverBackendKind::Stub,
    };

    let candidates: Vec<CandidateAction> = parsed
        .candidates
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let input =
                serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": cmd } });
            let sig = ActionSignature::from_pre_tool("Bash", &input, None, 0, None, None);
            CandidateAction::new(format!("cand{i}"), cmd.as_str(), sig)
        })
        .collect();
    // Rank with the same source `run_gateway_speculative` uses internally, so the
    // displayed order matches the executed order (the driver's re-rank is then
    // idempotent on an already-ranked list).
    let ranked = rank_by_predicted(candidates, &predictor);
    let prefix = run_gateway_speculative(&ranked, &deps);

    if flags.json {
        let accepted: Vec<&str> = prefix
            .valid_indices
            .iter()
            .filter_map(|&i| ranked.get(i).map(|c| c.payload.as_str()))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ranked_commands": ranked.iter().map(|c| c.payload.clone()).collect::<Vec<_>>(),
                "accepted_indices": prefix.valid_indices,
                "accepted_count": prefix.len(),
                "accepted_commands": accepted,
                "truncated": prefix.len() < ranked.len(),
            }))?
        );
    } else {
        println!(
            "CEG speculative — {} candidate(s), accepted prefix of {} (ranked most-likely-first):",
            ranked.len(),
            prefix.len()
        );
        for (rank, &i) in prefix.valid_indices.iter().enumerate() {
            if let Some(c) = ranked.get(i) {
                println!("  ✓ [{rank}] {}", c.payload);
            }
        }
        if prefix.len() < ranked.len()
            && let Some(c) = ranked.get(prefix.len())
        {
            println!(
                "  ✗ truncated at: {} (gateway Deny — nothing past it is drafted)",
                c.payload
            );
        }
    }

    // ES3 P3 (2026-06-02) — `--real-exec`: actually spawn each accepted
    // candidate sequentially via the supervised X8 path. Lossless contract —
    // a `SandboxError::Conflict` on candidate N truncates execution at N (the
    // already-spawned candidates 0..N-1 ran to completion; N+1..end are not
    // attempted in this batch; the caller's recovery is to re-batch with a
    // smaller set). A non-zero exit code on candidate N also truncates the
    // execution prefix at N + propagates as the process exit code.
    if parsed.use_real_exec {
        let mut exec_args = ExecArgs {
            command: String::new(),
            profile: parsed.profile.clone(),
            use_real_sandbox: parsed.use_real_sandbox,
            use_real_exec: true,
            intent: None,
        };
        for &i in &prefix.valid_indices {
            let cmd = match ranked.get(i) {
                Some(c) => c.payload.as_str(),
                None => continue,
            };
            exec_args.command = cmd.to_owned();
            match real_exec_with_locks(cmd, &exec_args) {
                Ok(0) => {}
                Ok(exit_code) => {
                    eprintln!(
                        "  ✗ candidate [{i}] {cmd} exited with code {exit_code} — execution prefix truncated"
                    );
                    std::process::exit(exit_code);
                }
                Err(e) => {
                    eprintln!("  ✗ candidate [{i}] {cmd} — execution prefix truncated: {e}");
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

// ── S-13 — `touring plan-gated` (credibility-gated MCTS planning) ───────────────

/// Compute each candidate's CEG credibility (`GateDecision::credibility`:
/// Allow → composite, Warn → half, Deny → `0.0`). Bounded — one gateway run per
/// candidate, computed once and reused across every MCTS rollout.
fn compute_credibilities(candidates: &[String], deps: &GatewayDeps<'_>) -> Vec<f64> {
    candidates
        .iter()
        .map(|cmd| {
            run_gateway("Bash", cmd, None, deps)
                .map(|o| o.decision.credibility())
                .unwrap_or(0.0)
        })
        .collect()
}

/// Run the credibility-gated MCTS planner over a flat candidate list (S-13).
///
/// Each candidate index is a root action; reward is uniform so credibility is the
/// sole differentiator — a candidate gated to `0.0` (a Deny) is never selected
/// over a credible one. Pure (no gateway / I-O), so it is unit-testable with a
/// hand-supplied credibility vector.
fn plan_gated_select(credibilities: &[f64]) -> Option<MCTSResult> {
    let n = credibilities.len() as u64;
    let config = MCTSConfig {
        num_rollouts: 64,
        max_depth: 1, // a flat one-level plan over the candidates (pure bandit)
        ..Default::default()
    };
    MCTSEngine::new(config).search_gated(
        0,
        |state| {
            if state == 0 {
                (0..n).collect()
            } else {
                Vec::new()
            }
        },
        |_state, _action| 1.0, // uniform reward — credibility is the differentiator
        |_state, action| credibilities.get(action as usize).copied().unwrap_or(0.0),
    )
}

/// `touring plan-gated` entry point — S-13's live consumer.
///
/// Plans over a batch of candidate commands with a credibility-gated MCTS: each
/// candidate's CEG verdict becomes its credibility, and `plan_gated_select`
/// runs `MCTSEngine::search_gated` so a `Deny` candidate (credibility `0.0`) is
/// never selected over a credible one — the planner-side capability gate.
///
/// ```text
/// touring plan-gated 'echo a' 'rm -rf /' 'echo c'
/// #  credibility[1] = 0.00  rm -rf / (Deny — never selected)
/// #  → planner selected: [0] echo a (or [2]), never [1]
/// ```
pub fn run_plan_gated(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let parsed = parse_speculative_args(&rest)?;

    let profile = resolve_profile(&parsed.profile)?;
    let predictor = ExecutionOutcomePredictor::new();
    let caps = SandboxCapabilities::from_profile(&profile);
    let guarded = |raw: &RawInvocation| guarded_dry_run(raw, &caps);
    let sandbox_runner: &dyn Fn(&RawInvocation) -> SandboxOutcome = if parsed.use_real_sandbox {
        &guarded
    } else {
        &deferred_dry_run
    };
    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        outcome_history: &neutral_outcome_history,
        sandbox_runner,
        predictor: &predictor,
        profile: &profile,
        // P3 (2026-06-01): X3.5 PROVE — opt-in via the structured
        // `prove_claim` pipeline. Default: no claim, Stub backend.
        claim: None,
        claim_context: touring_hooks::offensive_integration::ClaimContext::default(),
        solver_backend: touring_hooks::offensive_integration::SolverBackendKind::Stub,
    };

    let credibilities = compute_credibilities(&parsed.candidates, &deps);
    let result = plan_gated_select(&credibilities);

    if flags.json {
        let best_idx = result.as_ref().map(|r| r.best_action);
        let best_cmd = best_idx.and_then(|i| parsed.candidates.get(i as usize));
        let best_cred = best_idx.and_then(|i| credibilities.get(i as usize).copied());
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "candidates": parsed.candidates,
                "credibilities": credibilities,
                "best_action": best_idx,
                "best_command": best_cmd,
                "best_credibility": best_cred,
                "confidence": result.as_ref().map_or(0.0, |r| r.confidence),
            }))?
        );
    } else {
        println!("CEG gated plan — {} candidate(s):", parsed.candidates.len());
        for (i, (cmd, cred)) in parsed.candidates.iter().zip(&credibilities).enumerate() {
            let tag = if *cred == 0.0 {
                "  (Deny — never selected)"
            } else {
                ""
            };
            println!("  credibility[{i}] = {cred:.2}  {cmd}{tag}");
        }
        match &result {
            Some(r) => {
                let idx = r.best_action as usize;
                let cmd = parsed.candidates.get(idx).map_or("?", String::as_str);
                let cred = credibilities.get(idx).copied().unwrap_or(0.0);
                if cred > 0.0 {
                    println!(
                        "  → planner selected: [{}] {cmd} (credibility {cred:.2}, confidence {:.2})",
                        r.best_action, r.confidence
                    );
                } else {
                    println!(
                        "  → no credible candidate (all gated to 0) — fell back to [{}] {cmd}",
                        r.best_action
                    );
                }
            }
            None => println!("  → no candidates to plan over"),
        }
    }
    Ok(())
}

// ── B-3 — `touring plan-verified-depth` (MCTS↔CEG verified-action-depth) ────────

/// The verified draft depth of an action *chain* via a CEG-gated MCTS — the
/// EAGLE "draft until verification rejects" isomorphism (§3.1.3 / B-3). The MCTS
/// expands the chain one link deeper only while the CEG credibility of the
/// current action is positive (Allow/Warn); a `Deny` (credibility `0.0`)
/// leaves the node a leaf, capping the draft. The returned depth is the length
/// of the longest CEG-verified prefix — the action-level verified depth.
///
/// Pure over `credibilities` — unit-testable. The CEG-gated draft-tree contains
/// a node at depth `d+1` only when action `d` is credible (the gated `expand_fn`
/// in [`gated_draft_plan`] returns no children at the first `Deny`), so the
/// tree's maximum structural depth *is* the leading-credible prefix — the depth
/// an agent may draft before verification refuses to go deeper.
fn verified_draft_depth(credibilities: &[f64]) -> usize {
    credibilities.iter().take_while(|&&c| c > 0.0).count()
}

/// Plan *within* the CEG-verified draft-tree with a gated MCTS (B-3) — the live
/// MCTS↔CEG wiring. The `expand_fn` drafts the edge `d -> d+1` only while action
/// `d` is CEG-credible, and the `credibility_fn` scales each child's UCB by the
/// CEG verdict, so the planner never explores past a `Deny`. Returns the search
/// result (best first link + confidence) over the verified prefix, or `None`
/// when the very first action is denied (the gate refuses the whole draft).
fn gated_draft_plan(credibilities: &[f64]) -> Option<MCTSResult> {
    if credibilities.is_empty() {
        return None;
    }
    let config = MCTSConfig {
        num_rollouts: 64,
        max_depth: credibilities.len(),
        ..Default::default()
    };
    MCTSEngine::new(config).search_gated(
        0,
        |state| {
            let d = state as usize;
            if d < credibilities.len() && credibilities[d] > 0.0 {
                vec![state + 1]
            } else {
                Vec::new()
            }
        },
        |_state, action| action as f64, // reward grows with drafted depth
        |state, _action| credibilities.get(state as usize).copied().unwrap_or(0.0),
    )
}

/// Depth (0-indexed) of `state` in a breadth-first `fanout`-ary tree whose root
/// is node 0 and whose node `s` has children `s*fanout + 1 ..= s*fanout + fanout`.
/// `fanout <= 1` degenerates to the linear chain (depth == state). Used by the
/// branching draft-tree to recover a node's chain-depth (hence its CEG verdict).
fn depth_of(state: u64, fanout: u64) -> usize {
    if fanout <= 1 {
        return state as usize;
    }
    let mut depth = 0usize;
    let mut level_start = 0u64;
    let mut level_size = 1u64;
    while state >= level_start.saturating_add(level_size) {
        level_start = level_start.saturating_add(level_size);
        level_size = level_size.saturating_mul(fanout);
        depth += 1;
        if depth > 64 {
            break;
        }
    }
    depth
}

/// Plan within a CEG-verified **branching** draft-tree — the B-3 generalization of
/// the linear [`gated_draft_plan`] the prior reason named as the missing piece.
/// At each CEG-credible depth the `expand_fn` yields `fanout` candidate
/// continuations (a real tree, multiple drafts per depth), and the
/// `credibility_fn` gates every node by its depth's CEG verdict so no branch is
/// explored past a `Deny`. `fanout == 1` reduces exactly to the linear chain, so
/// this strictly subsumes `gated_draft_plan`.
fn gated_branch_plan(credibilities: &[f64], fanout: u64) -> Option<MCTSResult> {
    if credibilities.is_empty() {
        return None;
    }
    let fanout = fanout.max(1);
    let depth_limit = credibilities.len();
    let config = MCTSConfig {
        num_rollouts: 64,
        max_depth: depth_limit,
        ..Default::default()
    };
    MCTSEngine::new(config).search_gated(
        0,
        move |state| {
            let d = depth_of(state, fanout);
            if d < depth_limit && credibilities[d] > 0.0 {
                (0..fanout).map(|b| state * fanout + 1 + b).collect()
            } else {
                Vec::new()
            }
        },
        move |state, _action| depth_of(state, fanout) as f64 + 1.0,
        move |state, _action| {
            credibilities
                .get(depth_of(state, fanout))
                .copied()
                .unwrap_or(0.0)
        },
    )
}

/// For display: how many candidate continuations the gated branching planner
/// drafts at each depth — `fanout` on every CEG-credible depth, then a single `0`
/// at the first depth the gate refuses. Pure; length is `verified_depth` (+1 when
/// a refusal truncates the chain).
fn branches_per_depth(credibilities: &[f64], fanout: u64) -> Vec<u64> {
    let verified = verified_draft_depth(credibilities);
    let mut v: Vec<u64> = std::iter::repeat_n(fanout.max(1), verified).collect();
    if verified < credibilities.len() {
        v.push(0);
    }
    v
}

/// `touring plan-verified-depth` entry point — B-3's live consumer.
///
/// Treats the positional args as an ordered action *chain* (arg order = depth),
/// computes each action's CEG credibility (`run_gateway` verdict), and runs a
/// credibility-gated MCTS that drafts the chain only as deep as the CEG verifies
/// (`verified_draft_depth`). Reports the verified depth: the longest prefix the
/// harness would let an agent draft before a `Deny` truncates it — wiring MCTS
/// planning to action-level CEG verification (the draft-tree the prior reason
/// said was missing).
///
/// ```text
/// touring plan-verified-depth 'echo a' 'echo b' 'rm -rf /' 'echo d'
/// #  credibility [hi, hi, 0.00 Deny, hi] → verified_depth = 2 (truncated at rm -rf /)
/// ```
pub fn run_verified_depth(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    // B-3 branching: extract `--fanout N` (default 1 = linear chain) and strip it
    // before candidate parsing so it is never mistaken for an action.
    let mut fanout: u64 = 1;
    let mut filtered: Vec<String> = Vec::with_capacity(rest.len());
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == "--fanout" {
            fanout = it.next().and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
        } else {
            filtered.push(a.clone());
        }
    }
    let parsed = parse_speculative_args(&filtered)?;
    if parsed.candidates.is_empty() {
        return Err(anyhow!(
            "plan-verified-depth needs an ordered action chain, e.g. 'echo a' 'rm -rf /'"
        ));
    }

    let profile = resolve_profile(&parsed.profile)?;
    let predictor = ExecutionOutcomePredictor::new();
    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        outcome_history: &neutral_outcome_history,
        sandbox_runner: &deferred_dry_run,
        predictor: &predictor,
        profile: &profile,
        // P3 (2026-06-01): X3.5 PROVE — opt-in via the structured
        // `prove_claim` pipeline. Default: no claim, Stub backend.
        claim: None,
        claim_context: touring_hooks::offensive_integration::ClaimContext::default(),
        solver_backend: touring_hooks::offensive_integration::SolverBackendKind::Stub,
    };

    let credibilities = compute_credibilities(&parsed.candidates, &deps);
    let depth = verified_draft_depth(&credibilities);
    let truncated_at = (depth < parsed.candidates.len()).then_some(depth);
    // The CEG-gated MCTS plan over the verified draft-tree (the live wiring): a
    // plan exists iff the first action is credible. confidence reflects the
    // gated search's certainty in its best first link.
    // fanout == 1 → the linear chain (gated_draft_plan); fanout >= 2 → a real
    // branching draft-tree (gated_branch_plan, B-3 generalization).
    let branching = fanout >= 2;
    let plan = if branching {
        gated_branch_plan(&credibilities, fanout)
    } else {
        gated_draft_plan(&credibilities)
    };
    let plan_confidence = plan.as_ref().map_or(0.0, |r| r.confidence);
    let bpd = branches_per_depth(&credibilities, fanout);
    let max_branches = bpd.iter().copied().max().unwrap_or(0);

    if flags.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "chain": parsed.candidates,
                "credibilities": credibilities,
                "verified_depth": depth,
                "chain_len": parsed.candidates.len(),
                "fully_verified": depth == parsed.candidates.len(),
                "truncated_at_index": truncated_at,
                "gated_plan_exists": plan.is_some(),
                "gated_plan_confidence": plan_confidence,
                "branching": branching,
                "fanout": fanout,
                "branches_per_depth": bpd,
                "max_branches_per_depth": max_branches,
            }))?
        );
    } else {
        println!(
            "CEG plan-verified-depth — chain of {}, verified draft depth = {depth}:",
            parsed.candidates.len()
        );
        for (i, (cmd, cred)) in parsed.candidates.iter().zip(&credibilities).enumerate() {
            let mark = if i < depth {
                "✓ drafted"
            } else {
                "✗ beyond verified depth"
            };
            let deny = if *cred == 0.0 { " (Deny)" } else { "" };
            println!("  [{i}] cred={cred:.2}{deny}  {mark}  {cmd}");
        }
        match truncated_at {
            Some(t) => println!("  draft truncated at depth {t} — the CEG would not verify deeper"),
            None => println!("  full chain verified — the CEG drafts it end-to-end"),
        }
        match &plan {
            Some(_) => println!(
                "  gated MCTS drafted a plan over the verified prefix (confidence {plan_confidence:.2})"
            ),
            None => println!("  gated MCTS refused the draft — the first action is a Deny"),
        }
    }
    Ok(())
}

// ── S-10 — `touring conflict-check` (read/write-set parallelization safety) ──────

/// Parse `conflict-check` args: each positional is `<write-path>:<label>` — the
/// action declares a *write* to `<write-path>`. A bare arg (no `:`) writes to
/// itself with an empty label.
fn parse_conflict_args(rest: &[String]) -> anyhow::Result<Vec<(String, AccessDeclaration)>> {
    if rest.is_empty() {
        return Err(anyhow!(
            "conflict-check needs at least one '<write-path>:<label>' entry"
        ));
    }
    let mut out = Vec::new();
    for entry in rest {
        let (path, label) = match entry.split_once(':') {
            Some((p, l)) => (p.to_owned(), l.to_owned()),
            None => (entry.clone(), String::new()),
        };
        if path.is_empty() {
            return Err(anyhow!("empty write-path in entry '{entry}'"));
        }
        let decl = AccessDeclaration::new().writing(AccessPath::Path(path.clone()));
        let display = if label.is_empty() {
            path
        } else {
            format!("{label} (writes {path})")
        };
        out.push((display, decl));
    }
    Ok(out)
}

/// Partition declarations into parallel "waves": each joins the first wave in
/// which it conflicts with no member (greedy colouring by
/// [`AccessDeclaration::conflicts_with`]). Same-wave actions are safe to run
/// concurrently; different waves must be serialized. Pure — unit-testable.
fn partition_parallel_waves(decls: &[AccessDeclaration]) -> Vec<Vec<usize>> {
    let mut waves: Vec<Vec<usize>> = Vec::new();
    'next: for (i, decl) in decls.iter().enumerate() {
        for wave in &mut waves {
            if wave.iter().all(|&j| !decls[j].conflicts_with(decl)) {
                wave.push(i);
                continue 'next;
            }
        }
        waves.push(vec![i]);
    }
    waves
}

/// `touring conflict-check` entry point — S-10's live consumer.
///
/// Declares a write-set per action and reports the parallel waves: actions with
/// disjoint write-sets run concurrently; a write-write (or write-read) hazard
/// forces serialization. This is the declaration + conflict-detection half of
/// the transactional model ([`AccessDeclaration::conflicts_with`]); the runtime
/// acquire/release serialization (`ExecPool::acquire_txn`) is the opt-in
/// `txn_lock_enforcement` enforcement layered on top.
///
/// ```text
/// touring conflict-check 'src/a.rs:fix-a' 'src/a.rs:rename-a' 'src/b.rs:fix-b'
/// #  wave 0 (parallel): fix-a, fix-b  |  wave 1: rename-a (conflicts on src/a.rs)
/// ```
pub fn run_conflict_check(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let entries = parse_conflict_args(&rest)?;
    let decls: Vec<AccessDeclaration> = entries.iter().map(|(_, d)| d.clone()).collect();
    let waves = partition_parallel_waves(&decls);

    if flags.json {
        let wave_labels: Vec<Vec<&str>> = waves
            .iter()
            .map(|w| w.iter().map(|&i| entries[i].0.as_str()).collect())
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action_count": entries.len(),
                "wave_count": waves.len(),
                "waves": wave_labels,
                "max_parallel": waves.iter().map(Vec::len).max().unwrap_or(0),
            }))?
        );
    } else {
        println!(
            "CEG conflict-check — {} action(s) → {} parallel wave(s):",
            entries.len(),
            waves.len()
        );
        for (w, wave) in waves.iter().enumerate() {
            let labels: Vec<&str> = wave.iter().map(|&i| entries[i].0.as_str()).collect();
            println!("  wave {w} (run concurrently): {}", labels.join(" | "));
        }
        if waves.len() > 1 {
            println!("  (different waves must be serialized — they share a write resource)");
        }
    }
    Ok(())
}

// ── OP4 — runtime transactional locking (state-convergence live consumer) ────

/// Parse `txn-acquire` args. Each entry is `<mode>:<path>[:<label>]` where
/// `mode` is `r` (read) or `w` (write). A bare `<path>` (or any non-`r`/`w`
/// prefix) defaults to a write — the conservative, conflict-maximizing reading.
/// Returns `(display_label, AccessDeclaration)` pairs.
fn parse_txn_args(rest: &[String]) -> anyhow::Result<Vec<(String, AccessDeclaration)>> {
    if rest.is_empty() {
        return Err(anyhow!(
            "txn-acquire needs at least one '<r|w>:<path>[:<label>]' entry"
        ));
    }
    let mut out = Vec::new();
    for entry in rest {
        let parts: Vec<&str> = entry.splitn(3, ':').collect();
        let (mode, path, label) = match parts.as_slice() {
            [m @ ("r" | "w"), p, l] => (*m, (*p).to_owned(), (*l).to_owned()),
            [m @ ("r" | "w"), p] => (*m, (*p).to_owned(), String::new()),
            // No explicit r/w mode — treat the whole "path:label" (or bare path)
            // as a write, splitting an optional trailing label.
            _ => match entry.split_once(':') {
                Some((p, l)) => ("w", p.to_owned(), l.to_owned()),
                None => ("w", entry.clone(), String::new()),
            },
        };
        if path.is_empty() {
            return Err(anyhow!("empty path in entry '{entry}'"));
        }
        let ap = AccessPath::Path(path.clone());
        let decl = if mode == "r" {
            AccessDeclaration::new().reading(ap)
        } else {
            AccessDeclaration::new().writing(ap)
        };
        let verb = if mode == "r" { "reads" } else { "writes" };
        let display = if label.is_empty() {
            format!("{verb} {path}")
        } else {
            format!("{label} ({verb} {path})")
        };
        out.push((display, decl));
    }
    Ok(out)
}

/// Outcome of driving `decls` through a real [`TxnLockManager`] acquire/release
/// cycle. Pure over the declarations — unit-testable without any CLI plumbing.
#[derive(Debug, Default, PartialEq, Eq)]
struct TxnRuntimeReport {
    /// Indices granted immediately on arrival (run concurrently — wave 0).
    granted: Vec<usize>,
    /// `(index, blocking_holder_id)` for each arrival that conflicted.
    conflicts: Vec<(usize, u64)>,
    /// Indices that acquired after the wave-0 holders released (serialized).
    serialized_after_drain: Vec<usize>,
}

/// Drive `decls` through a live [`TxnLockManager`]: each declaration arrives in
/// order and calls `try_acquire`. Disjoint declarations are `Granted` and run
/// together; a write/read hazard returns `Conflict(holder)` and is deferred.
/// Then wave-0 holders `release` and the deferred ones re-acquire — proving the
/// serialization actually drains (no lost update, no deadlock). This exercises
/// the real runtime manager (`try_acquire`/`release`/`active_count`), not just
/// static conflict detection.
fn simulate_txn_runtime(decls: &[AccessDeclaration]) -> TxnRuntimeReport {
    let mut mgr = TxnLockManager::new();
    let mut report = TxnRuntimeReport::default();
    // Phase 1 — arrival: greedily acquire in declaration order.
    for (i, decl) in decls.iter().enumerate() {
        match mgr.try_acquire(i as u64, decl.clone()) {
            AcquireResult::Granted => report.granted.push(i),
            AcquireResult::Conflict(holder) => report.conflicts.push((i, holder)),
        }
    }
    // Phase 2 — drain wave-0 holders, then serialize the deferred acquirers.
    for &g in &report.granted {
        mgr.release(g as u64);
    }
    for &(i, _) in &report.conflicts {
        if mgr.try_acquire(i as u64, decls[i].clone()).is_granted() {
            report.serialized_after_drain.push(i);
            mgr.release(i as u64);
        }
    }
    report
}

/// `touring txn-acquire` entry point — OP4's runtime-locking live consumer.
///
/// Where `conflict-check` does static wave partitioning, this drives the
/// declarations through a real [`TxnLockManager`]: it acquires, detects
/// write/read hazards as `Conflict(holder)`, releases, and re-acquires the
/// deferred ones — the dependency-aware locking discipline (§5.2.4 / OP4) the
/// CRDT eventual-convergence layer cannot provide alone. Read-read never
/// conflicts, so pure readers acquire together.
///
/// ```text
/// touring txn-acquire 'w:src/a.rs:fix' 'r:src/a.rs:read' 'w:src/b.rs:other'
/// #  granted on arrival: fix, other  |  deferred (conflict): read → serialized after fix
/// ```
pub fn run_txn_acquire(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let entries = parse_txn_args(&rest)?;
    let decls: Vec<AccessDeclaration> = entries.iter().map(|(_, d)| d.clone()).collect();
    let report = simulate_txn_runtime(&decls);

    if flags.json {
        let granted: Vec<&str> = report
            .granted
            .iter()
            .map(|&i| entries[i].0.as_str())
            .collect();
        let conflicts: Vec<serde_json::Value> = report
            .conflicts
            .iter()
            .map(|&(i, h)| {
                serde_json::json!({
                    "action": entries[i].0.as_str(),
                    "blocked_by": entries[h as usize].0.as_str(),
                    "holder_id": h,
                })
            })
            .collect();
        let serialized: Vec<&str> = report
            .serialized_after_drain
            .iter()
            .map(|&i| entries[i].0.as_str())
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action_count": entries.len(),
                "granted_on_arrival": granted,
                "max_concurrent": report.granted.len(),
                "conflicts": conflicts,
                "serialized_after_drain": serialized,
                "all_drained": report.serialized_after_drain.len() == report.conflicts.len(),
            }))?
        );
    } else {
        println!(
            "CEG txn-acquire — {} action(s) through TxnLockManager:",
            entries.len()
        );
        let granted: Vec<&str> = report
            .granted
            .iter()
            .map(|&i| entries[i].0.as_str())
            .collect();
        println!(
            "  granted on arrival ({} concurrent): {}",
            report.granted.len(),
            if granted.is_empty() {
                "—".to_string()
            } else {
                granted.join(" | ")
            }
        );
        for &(i, h) in &report.conflicts {
            println!(
                "  CONFLICT: {} blocked by {} (holder id={h}) → deferred",
                entries[i].0, entries[h as usize].0
            );
        }
        if !report.conflicts.is_empty() {
            let serialized: Vec<&str> = report
                .serialized_after_drain
                .iter()
                .map(|&i| entries[i].0.as_str())
                .collect();
            println!(
                "  after holders release → serialized acquire: {}",
                serialized.join(" | ")
            );
            println!("  (dependency-aware locking: no lost update, no deadlock)");
        }
    }
    Ok(())
}

// ── OP7 Inspectable — surface the EvidenceBundle (§5.2.2 non-terminal signal) ──

/// `touring evidence "<command>"` entry point — OP7's *Inspectable* live consumer.
///
/// Runs the command through the CEG decision pipeline (`run_gateway`, **no real
/// execution** — a deferred dry-run) and renders the full, non-terminal
/// [`EvidenceBundle`](touring_hooks::gateway::EvidenceBundle): the five per-axis
/// sub-scores (X2 static, X3 vgp, X4 predict, X5 sandbox, X6 gate) behind the
/// composite, plus the verdict, the per-signal reasons, and the single canonical
/// fix. §5.2.2 of *Code as Agent Harness* warns against collapsing the
/// verification signal into one opaque scalar; this command surfaces *why*, per
/// axis — making the harness's decision evidence inspectable end-to-end.
///
/// ```text
/// touring evidence 'rm -rf /'
/// #  verdict Deny (composite 0.31) — X2 static 0.10  X3 vgp 1.00  ...
/// ```
pub fn run_evidence(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let command = rest.join(" ");
    if command.trim().is_empty() {
        return Err(anyhow!(
            "evidence needs a command to inspect, e.g. touring evidence 'rm -rf /'"
        ));
    }

    // Sandboxed profile + deferred dry-run: inspection never executes the command,
    // it only surfaces the verification evidence the gateway would decide on.
    let profile = resolve_profile("sandboxed")?;
    let predictor = ExecutionOutcomePredictor::new();
    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        outcome_history: &neutral_outcome_history,
        sandbox_runner: &deferred_dry_run,
        predictor: &predictor,
        profile: &profile,
        // P3 (2026-06-01): X3.5 PROVE — opt-in via the structured
        // `prove_claim` pipeline. Default: no claim, Stub backend.
        claim: None,
        claim_context: touring_hooks::offensive_integration::ClaimContext::default(),
        solver_backend: touring_hooks::offensive_integration::SolverBackendKind::Stub,
    };

    let outcome =
        run_gateway("Bash", &command, None, &deps).map_err(|e| anyhow!("gateway error: {e}"))?;
    let d = &outcome.decision;
    let ev = &d.evidence;

    if flags.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "command": command,
                "verdict": format!("{:?}", d.verdict),
                "composite_score": d.composite_score,
                "evidence": {
                    "static_score": ev.static_score,
                    "vgp_score": ev.vgp_score,
                    "predict_score": ev.predict_score,
                    "sandbox_score": ev.sandbox_score,
                    "gate_score": ev.gate_score,
                    "composite": ev.composite(),
                },
                "reasons": d.reasons,
                "canonical_fix": d.canonical_fix,
            }))?
        );
    } else {
        println!(
            "CEG evidence — verdict {:?} (composite {:.3}) for: {command}",
            d.verdict, d.composite_score
        );
        println!("  per-axis EvidenceBundle (§5.2.2 non-terminal verification signal):");
        println!("    X2 static  : {:.3}", ev.static_score);
        println!("    X3 vgp     : {:.3}", ev.vgp_score);
        println!("    X4 predict : {:.3}", ev.predict_score);
        println!("    X5 sandbox : {:.3}", ev.sandbox_score);
        println!("    X6 gate    : {:.3}", ev.gate_score);
        println!("    = composite: {:.3}", ev.composite());
        if !d.reasons.is_empty() {
            println!("  reasons:");
            for r in &d.reasons {
                println!("    - {r}");
            }
        }
        if let Some(fix) = &d.canonical_fix {
            println!("  canonical fix: {fix}");
        }
    }
    Ok(())
}

// ── B-5 — `touring predict-action` (historical action-outcome predictor) ────────

/// `touring predict-action --command "<cmd>" [--limit N]` — B-5's live consumer.
///
/// Asks the daemon to distil the recent `bash_outcomes` history into a
/// `LearnedOutcomeModel` (`cli-predict-action` handler, which owns the knowledge
/// DB) and predict the queried command's success — the experiential substrate
/// turned into a queryable action predictor.
pub fn run_predict_action(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let mut command: Option<String> = None;
    let mut limit: Option<u64> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--command" => {
                command = rest.get(i + 1).cloned();
                i += 2;
            }
            "--limit" => {
                limit = rest.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            other if command.is_none() => {
                command = Some(other.to_owned());
                i += 1;
            }
            _ => i += 1,
        }
    }
    let command = command.ok_or_else(|| anyhow!("predict-action needs --command \"<cmd>\""))?;

    let mut payload = serde_json::json!({ "command": command });
    if let Some(l) = limit {
        payload["limit"] = serde_json::json!(l);
    }
    let output = daemon_query("cli-predict-action", payload)?;

    if flags.json {
        println!("{output}");
    } else {
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(v) => {
                let prob = v
                    .get("success_probability")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let matched = v
                    .get("matched_observations")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let distilled = v
                    .get("distilled_from")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let total = v
                    .get("total_examples")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let conf = v
                    .get("confidence")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("None");
                println!(
                    "CEG predict-action — distilled {distilled} historical outcome(s) ({total} training example(s)):"
                );
                println!("  command            : {command}");
                println!("  success_probability: {prob:.3}");
                println!("  confidence         : {conf} ({matched} matched observation(s))");
            }
            Err(_) => println!("{output}"),
        }
    }
    Ok(())
}

/// `touring calibrate-confidence` — A-A1 conformal calibration of skill selection.
///
/// Distils the live `bash_outcomes` substrate into a split-conformal calibrator
/// and reports the data-derived firing threshold (τ = 1 − q̂, coverage ≥ 1 − α)
/// plus the calibrated decision for a queried confidence — replacing the old
/// hardcoded `0.7` cut with a statistically-grounded one (KnowNo). `--command`
/// keys an optional durable-approval (HITL) override lookup against the
/// `ApprovalStore`.
///
/// Flags: `--confidence <f64>` (default 0.7) · `--alpha <f64>` (default 0.1) ·
/// `--command "<cmd>"` (optional) · `--limit <u64>` (default 2000).
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable.
pub fn run_calibrate_confidence(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let mut confidence: Option<f64> = None;
    let mut alpha: Option<f64> = None;
    let mut command: Option<String> = None;
    let mut limit: Option<u64> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--confidence" => {
                confidence = rest.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            "--alpha" => {
                alpha = rest.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            "--command" => {
                command = rest.get(i + 1).cloned();
                i += 2;
            }
            "--limit" => {
                limit = rest.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            other if confidence.is_none() => {
                confidence = other.parse().ok();
                i += 1;
            }
            _ => i += 1,
        }
    }

    let mut payload = serde_json::json!({ "confidence": confidence.unwrap_or(0.7) });
    if let Some(a) = alpha {
        payload["alpha"] = serde_json::json!(a);
    }
    if let Some(c) = command {
        payload["command"] = serde_json::json!(c);
    }
    if let Some(l) = limit {
        payload["limit"] = serde_json::json!(l);
    }
    let output = daemon_query("cli-calibrate-confidence", payload)?;

    if flags.json {
        println!("{output}");
    } else {
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(v) => {
                let get_f =
                    |k: &str, d: f64| v.get(k).and_then(serde_json::Value::as_f64).unwrap_or(d);
                let get_b = |k: &str| {
                    v.get(k)
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                };
                let tau = get_f("conformal_threshold", 0.7);
                let cov = get_f("coverage_target", 0.9);
                let n = v
                    .get("n_calibration")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let distilled = v
                    .get("distilled_from")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let calibrated = get_b("calibrated");
                let qconf = get_f("queried_confidence", 0.0);
                let in_set = get_b("in_prediction_set");
                let defer = get_b("defer_hitl");
                let eff = v
                    .get("effective_defer")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(defer);
                let ovr = v.get("hitl_override").and_then(serde_json::Value::as_str);
                println!("conformal calibrate-confidence (split conformal prediction / KnowNo):");
                println!(
                    "  distilled          : {distilled} outcome(s) → {n} calibration example(s) [{}]",
                    if calibrated {
                        "conformal"
                    } else {
                        "legacy fallback"
                    }
                );
                println!(
                    "  conformal_threshold: {tau:.3}  (coverage target {:.0}%)",
                    cov * 100.0
                );
                println!(
                    "  queried_confidence : {qconf:.3} → {}",
                    if in_set {
                        "IN prediction set"
                    } else {
                        "OUT (defer)"
                    }
                );
                println!(
                    "  defer_hitl         : {defer}  effective_defer: {eff}{}",
                    ovr.map(|o| format!("  (approval override: {o})"))
                        .unwrap_or_default()
                );
            }
            Err(_) => println!("{output}"),
        }
    }
    Ok(())
}

/// `touring world-model-status [--persist]` — ES4 P1 liveness probe for the
/// durable action world model (the X4 PREDICT online data source).
///
/// The online `LearnedOutcomeModel` accumulates outcome history for the daemon's
/// whole life but was previously RAM-only — a restart reset it to a flat `0.5`
/// cold-start. ES4 P1 persists it to `<project>/.claude/touring/action_world_model.json`
/// and warm-loads it at daemon startup / session-start. This command reports that
/// durable state; `warm_loaded_entries > 0` proves the model survived a restart.
///
/// Flags: `--persist` forces an immediate atomic snapshot (write → restart → read
/// makes the durability cycle provable on demand).
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable.
pub fn run_world_model_status(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let persist = rest.iter().any(|a| a == "--persist" || a == "persist");
    let payload = serde_json::json!({
        "action": if persist { "persist" } else { "status" },
    });
    let output = daemon_query("cli-world-model-status", payload)?;

    if flags.json {
        println!("{output}");
    } else {
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(v) => {
                let get_u = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0);
                let get_b = |k: &str| {
                    v.get(k)
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                };
                let path = v
                    .get("snapshot_path")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("<unconfigured>");
                println!("durable action world model (ES4 P1):");
                println!("  total_examples     : {}", get_u("total_examples"));
                println!("  distinct_features  : {}", get_u("distinct_features"));
                println!(
                    "  warm_loaded_entries: {}  {}",
                    get_u("warm_loaded_entries"),
                    if get_u("warm_loaded_entries") > 0 {
                        "(survived restart)"
                    } else {
                        "(cold this process)"
                    }
                );
                println!("  snapshot_path      : {path}");
                println!("  snapshot_exists    : {}", get_b("snapshot_exists"));
                if persist {
                    println!("  persisted          : {}", get_b("persisted"));
                }
            }
            Err(_) => println!("{output}"),
        }
    }
    Ok(())
}

/// `touring attest-contract [-j]` — ES2 P2. Pins + attests the constitutional
/// sink-token contract (EAGLE B-6): a blake3 digest over `~/.claude/CLAUDE.md`
/// + `rules/*.md` plus per-claim structural verdicts.
///
/// Computed IN-PROCESS via `HarnessContract::attest` (the attestation is a
/// pure filesystem hash needing no daemon state) so the proof runs fail-open
/// even when the daemon socket is down — this is the live executable command
/// that flips oracle row `eagle.b6-sink-token-contract` from `result.cmd: null`.
///
/// # Errors
///
/// Returns an error only if stdout serialization fails.
pub fn run_attest_contract(args: &[String]) -> anyhow::Result<()> {
    let (flags, _rest) = parse_global_flags(sub_command_args(args));
    let root = touring_hooks::cli_handlers::touring_claude_dir();
    let contract = touring_hooks::gateway::harness_contract::HarnessContract::attest(&root);

    if flags.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "attested": contract.attested,
                "digest": contract.digest,
                "file_count": contract.file_count,
                "claims": contract.claims,
            }))?
        );
    } else {
        let short = contract.digest.get(..16).unwrap_or(&contract.digest);
        println!(
            "harness contract — attested={} digest={short} ({} files)",
            contract.attested, contract.file_count
        );
        for c in &contract.claims {
            println!(
                "  [{}] {}",
                if c.passed { "PASS" } else { "FAIL" },
                c.evidence
            );
        }
    }
    Ok(())
}

/// `touring rl-warmstart` — opt-in cross-project warm-start of the RL reward
/// loop (Cold-start cluster, 2026-05-30).
///
/// The RL substrate is per-project; a fresh project's loop is genuinely cold.
/// This seeds it from another project's REAL accumulated `bash_outcomes` via
/// experience replay (never synthetic). Default (no corpus) is a no-op,
/// preserving per-project isolation.
///
/// Flags: `--corpus-db <path>` (or env `TOURING_RL_WARMSTART_CORPUS`) ·
/// `--limit <u64>` (default 200) · `--max-inject <u64>` (default 200).
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable.
pub fn run_rl_warmstart(args: &[String]) -> anyhow::Result<()> {
    let (flags, rest) = parse_global_flags(sub_command_args(args));
    let mut corpus_db: Option<String> = None;
    let mut limit: Option<u64> = None;
    let mut max_inject: Option<u64> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--corpus-db" => {
                corpus_db = rest.get(i + 1).cloned();
                i += 2;
            }
            "--limit" => {
                limit = rest.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            "--max-inject" => {
                max_inject = rest.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            _ => i += 1,
        }
    }

    let mut payload = serde_json::json!({});
    if let Some(c) = corpus_db {
        payload["corpus_db"] = serde_json::json!(c);
    }
    if let Some(l) = limit {
        payload["limit"] = serde_json::json!(l);
    }
    if let Some(m) = max_inject {
        payload["max_inject"] = serde_json::json!(m);
    }
    let output = daemon_query("cli-rl-warmstart", payload)?;

    if flags.json {
        println!("{output}");
    } else {
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(v) => {
                let warm = v
                    .get("warmstarted")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if warm {
                    let replayed = v
                        .get("replayed")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    let read = v
                        .get("corpus_outcomes_read")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    let succ = v
                        .get("measured_bash_success")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    let uc = v
                        .get("update_count_after")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    let ema = v
                        .get("ema_reward_after")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    let src = v
                        .get("source_db")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("?");
                    println!("RL warm-start — cross-project experience replay (REAL outcomes):");
                    println!("  source_db          : {src}");
                    println!("  replayed           : {replayed} of {read} real outcome(s)");
                    println!("  measured_bash_succ : {succ:.3}");
                    println!("  update_count_after : {uc}   ema_reward_after: {ema:.3}");
                } else {
                    let reason = v
                        .get("reason")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("?");
                    println!("RL warm-start: no-op — {reason}");
                }
            }
            Err(_) => println!("{output}"),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;
