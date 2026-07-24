use super::*;

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

/// A realistic process argv: binary + `exec` subcommand + the given words.
fn argv(words: &[&str]) -> Vec<String> {
    let mut v = vec!["touring".to_owned(), "exec".to_owned()];
    v.extend(words.iter().map(|w| (*w).to_owned()));
    v
}

// ── sub_command_args ──────────────────────────────────────────────────

#[test]
fn sub_command_args_skips_binary_and_subcommand() {
    let full = argv(&["--profile", "trusted", "echo", "hi"]);
    assert_eq!(sub_command_args(&full), &full[2..]);
    assert_eq!(sub_command_args(&full)[0], "--profile");
}

#[test]
fn sub_command_args_is_empty_for_a_bare_invocation() {
    // `touring exec` with no further args, and a defensively short argv.
    assert!(sub_command_args(&args(&["touring", "exec"])).is_empty());
    assert!(sub_command_args(&[]).is_empty());
    assert!(sub_command_args(&args(&["touring"])).is_empty());
}

// ── parse_exec_args ───────────────────────────────────────────────────

#[test]
fn parse_exec_args_joins_command_words() {
    let parsed = parse_exec_args(&args(&["echo", "hello", "world"])).expect("parsed");
    assert_eq!(parsed.command, "echo hello world");
    assert_eq!(parsed.profile, "trusted"); // default
    assert!(!parsed.use_real_sandbox);
}

#[test]
fn parse_exec_args_reads_the_profile_flag() {
    let parsed = parse_exec_args(&args(&["--profile", "sandboxed", "ls"])).expect("parsed");
    assert_eq!(parsed.profile, "sandboxed");
    assert_eq!(parsed.command, "ls");
}

#[test]
fn parse_exec_args_reads_the_sandbox_and_intent_flags() {
    let parsed = parse_exec_args(&args(&["--sandbox", "--intent", "build", "cargo", "build"]))
        .expect("parsed");
    assert!(parsed.use_real_sandbox);
    assert_eq!(parsed.intent.as_deref(), Some("build"));
    assert_eq!(parsed.command, "cargo build");
}

#[test]
fn parse_exec_args_rejects_an_empty_command() {
    assert!(parse_exec_args(&[]).is_err());
    assert!(parse_exec_args(&args(&["--profile", "trusted"])).is_err());
}

#[test]
fn parse_exec_args_rejects_an_unknown_profile() {
    let err = parse_exec_args(&args(&["--profile", "godmode", "ls"])).unwrap_err();
    assert!(err.to_string().contains("godmode"));
}

// ── resolve_profile ───────────────────────────────────────────────────

#[test]
fn resolve_profile_maps_each_known_name() {
    assert_eq!(
        resolve_profile("trusted").expect("trusted").name(),
        "Trusted"
    );
    assert_eq!(
        resolve_profile("sandboxed").expect("sandboxed").name(),
        "Sandboxed"
    );
    assert_eq!(
        resolve_profile("readonly").expect("readonly").name(),
        "ReadOnly"
    );
}

#[test]
fn resolve_profile_rejects_an_unknown_name() {
    assert!(resolve_profile("nonexistent").is_err());
}

// ── gate_command ──────────────────────────────────────────────────────

#[test]
fn gate_command_allows_a_clean_command_under_trusted() {
    let outcome = gate_command("echo hello", "trusted", false, None).expect("gated");
    assert_eq!(outcome.decision.verdict, Verdict::Allow);
    assert!(outcome.id.as_str().starts_with("exec-"));
}

#[test]
fn gate_command_denies_a_destructive_command() {
    // Deferred X5 — never spawns — so this is safe to run in a test.
    let outcome = gate_command("rm -rf /", "trusted", false, None).expect("gated");
    assert_eq!(outcome.decision.verdict, Verdict::Deny);
    assert!(outcome.decision.canonical_fix.is_some());
}

#[test]
fn gate_command_denies_a_subprocess_under_sandboxed() {
    // The Sandboxed profile grants no Run capability — a shell command,
    // which always spawns, is denied. This is the stricter question.
    let outcome = gate_command("ls -la", "sandboxed", false, None).expect("gated");
    assert_eq!(outcome.decision.verdict, Verdict::Deny);
}

#[test]
fn gate_command_gates_only_the_command_not_the_argv() {
    // Regression guard (cross-audit 2026-05-18): the gated command is the
    // command itself — never the binary / subcommand. The first denied
    // subprocess capability under Sandboxed must name `ls`, not `touring`.
    let outcome = gate_command("ls -la", "sandboxed", false, None).expect("gated");
    let gate_report = outcome
        .evidence
        .gate_report
        .as_ref()
        .expect("X6 attaches a gate report");
    assert!(
        gate_report.gated.iter().any(|g| g.operation == "ls"),
        "the gated command must be `ls`, got: {:?}",
        gate_report.gated
    );
    assert!(
        !gate_report.gated.iter().any(|g| g.operation == "touring"),
        "the binary name must never leak into the gated command"
    );
}

// ── format_decision_human ─────────────────────────────────────────────

#[test]
fn format_decision_human_reports_verdict_and_score() {
    let outcome = gate_command("rm -rf /", "trusted", false, None).expect("gated");
    let rendered = format_decision_human(&outcome);
    assert!(rendered.contains("Deny"));
    assert!(rendered.contains("composite:"));
    assert!(rendered.contains("CEG gateway"));
}

// ── E2E: run (driven with a realistic full argv) ──────────────────────

#[test]
fn e2e_run_succeeds_for_a_clean_command() {
    // `run` receives the full process argv — binary + subcommand + the
    // command — exactly as `main.rs` dispatches it.
    assert!(run(&argv(&["echo", "hello"])).is_ok());
}

#[test]
fn e2e_run_fails_for_a_destructive_command() {
    // `rm -rf /` → Deny → Err, so the shell sees a non-zero exit. The
    // deferred X5 runner means nothing is actually spawned.
    let result = run(&argv(&["rm", "-rf", "/"]));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Deny"));
}

#[test]
fn e2e_run_with_profile_flag_after_the_subcommand() {
    // The `--profile` flag sits between the subcommand and the command —
    // `sub_command_args` + `parse_exec_args` must both handle it.
    assert!(run(&argv(&["--profile", "trusted", "echo", "ok"])).is_ok());
}

// ── S-12 — exec-speculative ───────────────────────────────────────────

#[test]
fn parse_speculative_collects_candidates_and_flags() {
    let parsed = parse_speculative_args(&args(&["--profile", "sandboxed", "echo a", "echo b"]))
        .expect("parsed");
    assert_eq!(parsed.profile, "sandboxed");
    assert_eq!(
        parsed.candidates,
        vec!["echo a".to_owned(), "echo b".to_owned()]
    );
    assert!(!parsed.use_real_sandbox);
    // No candidates is an error; an unknown profile is rejected.
    assert!(parse_speculative_args(&args(&["--profile", "trusted"])).is_err());
    assert!(parse_speculative_args(&args(&["--profile", "bogus", "echo x"])).is_err());
}

#[test]
fn e2e_speculative_truncates_at_a_deny_and_succeeds() {
    // A destructive command mid-batch truncates the accepted prefix. The
    // command reports the prefix (it does not run anything) and returns Ok —
    // a Deny is data here, never a CLI error.
    let r = run_speculative(&argv(&["echo one", "echo two", "rm -rf /", "echo four"]));
    assert!(
        r.is_ok(),
        "exec-speculative reports the prefix, it never errors on a Deny"
    );
}

// ── S-13 — plan-gated ─────────────────────────────────────────────────

#[test]
fn plan_gated_never_selects_a_zero_credibility_candidate() {
    // [credible, Deny(0.0), credible] → the gated planner must never pick
    // index 1 over a credible alternative.
    let result = plan_gated_select(&[0.9, 0.0, 0.9]).expect("a credible candidate exists");
    assert_ne!(
        result.best_action, 1,
        "a Deny (credibility 0) must never be the plan"
    );
    assert!(
        result.best_action == 0 || result.best_action == 2,
        "the planner must select a credible candidate, got {}",
        result.best_action
    );
}

#[test]
fn e2e_plan_gated_runs_and_reports() {
    // Mixed batch incl. a destructive Deny — the command reports the plan and
    // returns Ok (a Deny is data here, never a CLI error).
    let r = run_plan_gated(&argv(&["echo a", "rm -rf /", "echo c"]));
    assert!(r.is_ok());
}

// ── S-10 — conflict-check ─────────────────────────────────────────────

#[test]
fn partition_waves_serializes_write_write_hazards() {
    let decls = vec![
        AccessDeclaration::new().writing(AccessPath::Path("a".into())),
        AccessDeclaration::new().writing(AccessPath::Path("a".into())), // conflicts with [0]
        AccessDeclaration::new().writing(AccessPath::Path("b".into())), // disjoint
    ];
    let waves = partition_parallel_waves(&decls);
    // a + b parallelize in wave 0; the second a-writer is serialized to wave 1.
    assert_eq!(waves, vec![vec![0, 2], vec![1]]);
}

#[test]
fn partition_waves_all_disjoint_is_one_wave() {
    let decls = vec![
        AccessDeclaration::new().writing(AccessPath::Path("a".into())),
        AccessDeclaration::new().writing(AccessPath::Path("b".into())),
        AccessDeclaration::new().writing(AccessPath::Path("c".into())),
    ];
    let waves = partition_parallel_waves(&decls);
    assert_eq!(
        waves,
        vec![vec![0, 1, 2]],
        "disjoint write-sets all parallelize"
    );
}

#[test]
fn e2e_conflict_check_runs_and_rejects_empty() {
    let r = run_conflict_check(&argv(&[
        "src/a.rs:fix-a",
        "src/a.rs:rename-a",
        "src/b.rs:fix-b",
    ]));
    assert!(r.is_ok());
    assert!(
        parse_conflict_args(&args(&[])).is_err(),
        "needs at least one entry"
    );
}

// ── OP4 — txn-acquire (runtime locking) ───────────────────────────────

#[test]
fn txn_runtime_grants_disjoint_defers_write_write_hazard() {
    // [w a, w a, w b]: a + b granted on arrival; the second a-writer conflicts
    // with the holder of `a` (id 0) and is deferred, then serializes after drain.
    let decls = vec![
        AccessDeclaration::new().writing(AccessPath::Path("a".into())),
        AccessDeclaration::new().writing(AccessPath::Path("a".into())),
        AccessDeclaration::new().writing(AccessPath::Path("b".into())),
    ];
    let r = simulate_txn_runtime(&decls);
    assert_eq!(r.granted, vec![0, 2], "disjoint a + b acquire concurrently");
    assert_eq!(
        r.conflicts,
        vec![(1, 0)],
        "second a-writer blocked by holder id 0"
    );
    assert_eq!(
        r.serialized_after_drain,
        vec![1],
        "deferred writer drains after release"
    );
}

#[test]
fn txn_runtime_read_read_coexists() {
    // Two pure readers of the same path never conflict — both granted.
    let decls = vec![
        AccessDeclaration::new().reading(AccessPath::Path("shared".into())),
        AccessDeclaration::new().reading(AccessPath::Path("shared".into())),
    ];
    let r = simulate_txn_runtime(&decls);
    assert_eq!(r.granted, vec![0, 1], "read-read coexists");
    assert!(r.conflicts.is_empty());
}

#[test]
fn txn_runtime_write_read_hazard_is_a_conflict() {
    // A writer of `x` then a reader of `x` → read-write hazard, deferred.
    let decls = vec![
        AccessDeclaration::new().writing(AccessPath::Path("x".into())),
        AccessDeclaration::new().reading(AccessPath::Path("x".into())),
    ];
    let r = simulate_txn_runtime(&decls);
    assert_eq!(r.granted, vec![0]);
    assert_eq!(
        r.conflicts,
        vec![(1, 0)],
        "reader of x blocked by writer of x"
    );
    assert_eq!(r.serialized_after_drain, vec![1]);
}

#[test]
fn parse_txn_modes_read_write_and_default() {
    let parsed = parse_txn_args(&args(&["r:foo.rs:rd", "w:bar.rs:wr", "baz.rs"]))
        .expect("valid txn declarations parse");
    assert_eq!(parsed.len(), 3);
    assert!(
        parsed[0].1.is_read_only(),
        "r: prefix → read-only declaration"
    );
    assert!(!parsed[1].1.is_read_only(), "w: prefix → write declaration");
    assert!(!parsed[2].1.is_read_only(), "bare path defaults to write");
}

#[test]
fn e2e_txn_acquire_runs_and_rejects_empty() {
    let r = run_txn_acquire(&argv(&[
        "w:src/a.rs:fix",
        "r:src/a.rs:read",
        "w:src/b.rs:other",
    ]));
    assert!(r.is_ok());
    assert!(
        parse_txn_args(&args(&[])).is_err(),
        "needs at least one entry"
    );
}

// ── OP7 Inspectable — evidence ────────────────────────────────────────

#[test]
fn e2e_evidence_inspects_a_clean_command() {
    // A benign command — the gateway renders its EvidenceBundle and returns Ok.
    let r = run_evidence(&argv(&["echo", "hello"]));
    assert!(r.is_ok());
}

#[test]
fn e2e_evidence_inspects_a_destructive_command() {
    // A Deny verdict is inspectable DATA, never a CLI error — evidence still
    // returns Ok and surfaces the per-axis scores behind the refusal.
    let r = run_evidence(&argv(&["rm -rf /"]));
    assert!(r.is_ok());
}

#[test]
fn evidence_rejects_an_empty_command() {
    assert!(
        run_evidence(&argv(&[])).is_err(),
        "needs a command to inspect"
    );
}

// ── B-3 — plan-verified-depth (MCTS↔CEG verified-action-depth) ────────

#[test]
fn verified_depth_truncates_at_the_first_deny() {
    // chain credibility [hi, hi, Deny, hi] → drafts depth 2, the Deny caps it.
    assert_eq!(verified_draft_depth(&[0.9, 0.9, 0.0, 0.9]), 2);
}

#[test]
fn verified_depth_full_chain_when_all_credible() {
    assert_eq!(
        verified_draft_depth(&[0.9, 0.8, 0.7]),
        3,
        "no Deny → full chain drafts"
    );
}

#[test]
fn verified_depth_zero_when_the_first_action_is_denied() {
    assert_eq!(
        verified_draft_depth(&[0.0, 0.9, 0.9]),
        0,
        "a leading Deny drafts nothing"
    );
}

#[test]
fn verified_depth_empty_chain_is_zero() {
    assert_eq!(verified_draft_depth(&[]), 0);
}

// ── B-3 branching draft-trees ───────────────────────────────────────────

#[test]
fn depth_of_breadth_first_fanary_tree() {
    // fanout=2: level0=[0], level1=[1,2], level2=[3,4,5,6].
    assert_eq!(depth_of(0, 2), 0);
    assert_eq!(depth_of(1, 2), 1);
    assert_eq!(depth_of(2, 2), 1);
    assert_eq!(depth_of(3, 2), 2);
    assert_eq!(depth_of(6, 2), 2);
    // fanout<=1 degenerates to the linear chain (depth == state).
    assert_eq!(depth_of(5, 1), 5);
}

#[test]
fn branches_per_depth_reports_fanout_then_zero_at_deny() {
    // [hi, hi, Deny, hi] @ fanout 2 → 2 branches at depths 0,1 then 0 at the Deny.
    assert_eq!(branches_per_depth(&[0.9, 0.9, 0.0, 0.9], 2), vec![2, 2, 0]);
    // full chain credible → fanout at every depth, no trailing 0.
    assert_eq!(branches_per_depth(&[0.9, 0.8], 3), vec![3, 3]);
}

#[test]
fn gated_branch_plan_explores_a_real_tree_not_a_chain() {
    // With fanout>=2 over credible depths the planner returns a plan AND the
    // branch count per credible depth exceeds 1 (a tree, not a single chain).
    let creds = [0.9, 0.9, 0.0, 0.9];
    let plan = gated_branch_plan(&creds, 2);
    assert!(
        plan.is_some(),
        "credible first action → a branching plan exists"
    );
    let bpd = branches_per_depth(&creds, 2);
    assert!(
        bpd.iter().any(|&b| b > 1),
        "branching: >1 continuation per credible depth"
    );
    // The CEG Deny still truncates the draft at depth 2 (same invariant as linear).
    assert_eq!(verified_draft_depth(&creds), 2);
}

#[test]
fn gated_branch_plan_fanout_one_matches_linear_existence() {
    // fanout==1 reduces to the linear chain: both agree on plan existence.
    let creds = [0.9, 0.8, 0.7];
    assert_eq!(
        gated_branch_plan(&creds, 1).is_some(),
        gated_draft_plan(&creds).is_some()
    );
}

#[test]
fn gated_branch_plan_refuses_when_first_action_denied() {
    assert!(
        gated_branch_plan(&[0.0, 0.9], 3).is_none(),
        "leading Deny → no branching plan"
    );
}

#[test]
fn verified_depth_matches_the_longest_credible_prefix() {
    // The MCTS draft depth must equal the pure leading-credible-prefix length.
    for cr in [
        vec![0.9, 0.9, 0.9, 0.9],
        vec![0.5, 0.0, 0.9],
        vec![0.0],
        vec![0.7, 0.7, 0.0, 0.0, 0.9],
    ] {
        let expected = cr.iter().take_while(|&&c| c > 0.0).count();
        assert_eq!(
            verified_draft_depth(&cr),
            expected,
            "depth must equal verified prefix for {cr:?}"
        );
    }
}

#[test]
fn e2e_verified_depth_runs_and_rejects_empty() {
    let r = run_verified_depth(&argv(&["echo a", "echo b", "rm -rf /"]));
    assert!(r.is_ok());
    assert!(
        run_verified_depth(&argv(&[])).is_err(),
        "needs an ordered action chain"
    );
}

// ── B-5 — predict-action (arg parsing; the live path needs a daemon) ──

#[test]
fn predict_action_rejects_a_missing_command() {
    // No --command and no positional → error before any daemon round-trip.
    assert!(
        run_predict_action(&argv(&["--limit", "100"])).is_err(),
        "predict-action requires a command"
    );
}

// ── ES3 P3 (2026-06-02) — `--real-exec` flag wires `run_supervised_with_locks` ──

/// `--real-exec` parses correctly and lives next to `--sandbox` and
/// `--intent`. Parse-side proof the flag exists and is OFF by default.
#[test]
fn parse_exec_args_reads_the_real_exec_flag() {
    let parsed = parse_exec_args(&args(&["--real-exec", "echo", "hi"])).expect("parsed");
    assert!(parsed.use_real_exec, "--real-exec → use_real_exec=true");
    assert!(
        !parsed.use_real_sandbox,
        "--real-exec is independent of --sandbox"
    );
    assert_eq!(parsed.command, "echo hi");

    let parsed_off = parse_exec_args(&args(&["echo", "hi"])).expect("parsed");
    assert!(
        !parsed_off.use_real_exec,
        "default (no flag) → use_real_exec=false, analysis-only"
    );
}

/// E2E (S-3-3 #1): `touring exec --real-exec 'echo ...'` reaches the
/// supervised X8 path, the command spawns, exits 0, and `run()` returns
/// `Ok(())`. Allow verdict + zero-exit ⇒ no `std::process::exit`.
#[test]
fn e2e_run_real_exec_executes_command_when_verdict_allow() {
    let r = run(&argv(&["--real-exec", "echo", "ceg-p3-hello"]));
    assert!(
        r.is_ok(),
        "Allow verdict + zero-exit command must return Ok: {r:?}"
    );
}

/// E2E (S-3-3 #2): the helper preserves the command's actual exit code
/// without calling `std::process::exit` (the CLI wrapper does that). We
/// drive the helper directly so the test process is not terminated.
#[test]
fn e2e_real_exec_with_locks_preserves_command_exit_code() {
    let parsed = ExecArgs {
        command: "false".to_owned(),
        profile: "trusted".to_owned(),
        use_real_sandbox: false,
        use_real_exec: true,
        intent: None,
    };
    let exit_code = real_exec_with_locks("false", &parsed)
        .expect("helper returns Ok with the command's exit code");
    assert_eq!(
        exit_code, 1,
        "the `false` command exits 1; the helper must preserve it"
    );
}

/// E2E (S-3-3 #3, Linux-gated): a concurrent transactional lock on the
/// command's inferred write path triggers `SandboxError::Conflict`, which
/// the helper surfaces as an `anyhow::Error` whose message contains
/// `"concurrent write conflict"`. The CLI wrapper then maps this to the
/// EX_TEMPFAIL exit-code-75 path.
#[cfg(target_os = "linux")]
#[test]
fn e2e_real_exec_with_locks_denies_on_concurrent_conflict() {
    use touring_hooks::gateway::ExecPool;

    // Per-process unique path so concurrent test runs do not stomp.
    let write_path = format!(
        "/tmp/ceg-p3-conflict-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );

    // Hold a transactional lock on the path via the global ExecPool —
    // simulates a concurrent execution that has already acquired it.
    let _held = ExecPool::global()
        .acquire_txn(AccessDeclaration::new().writing(AccessPath::Path(write_path.clone())))
        .expect("the first acquire on a fresh path is granted");

    // The helper builds its own AccessDeclaration from the command via
    // `from_tool_payload_full` — `touch <path>` declares `<path>` as a
    // write, conflicting with the held permit above.
    let parsed = ExecArgs {
        command: format!("touch {write_path}"),
        profile: "trusted".to_owned(),
        use_real_sandbox: false,
        use_real_exec: true,
        intent: None,
    };
    let result = real_exec_with_locks(&parsed.command, &parsed);
    assert!(
        result.is_err(),
        "a second writer of the same path must be denied"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("concurrent write conflict"),
        "the conflict error must surface verbatim: got '{msg}'"
    );
}

/// E2E (S-3-3 #4): WITHOUT `--real-exec` the command is never spawned.
/// Backward compatibility proof — every pre-ES3 P3 invocation keeps the
/// analysis-only behaviour. A side-effect-free `echo` is sufficient
/// because the negative invariant is: `run()` returns Ok and the helper
/// is NOT called (the only spawn path).
#[test]
fn e2e_run_without_real_exec_flag_still_analysis_only() {
    let parsed = parse_exec_args(&args(&["echo", "ceg-p3-analysis-only"])).expect("parsed");
    assert!(
        !parsed.use_real_exec,
        "default invocation must leave use_real_exec=false"
    );
    let r = run(&argv(&["echo", "ceg-p3-analysis-only"]));
    assert!(
        r.is_ok(),
        "Allow verdict without --real-exec must still return Ok (no spawn): {r:?}"
    );
}
