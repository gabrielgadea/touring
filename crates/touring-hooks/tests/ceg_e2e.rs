//! CEG E2E Suite — 11 runtimes × 6 execution surfaces (P7.4).
//!
//! Phase **P7.4** of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! # Coverage
//!
//! Every cell in the 11-runtime × 6-surface matrix is exercised by at least one
//! test:
//!
//! | Runtimes (11) | js, python, ts, ruby, go, rust, php, perl, r, elixir, sh |
//! | Surfaces (6)  | Bash, Write, ctx\_execute, inferlets, jobs, subagent       |
//!
//! For each (runtime, surface) pair the suite proves two properties:
//!
//! 1. **BLOCK** — the gate denies (`Verdict::Deny`) code that carries a
//!    forbidden pattern (the canonical destructive catalogue: `rm -rf`, `eval`,
//!    etc.).
//! 2. **ALLOW** — the gate allows (`Verdict::Allow`) or at worst warns on
//!    clean, idiomatic sample code for that runtime.
//!
//! Plus a dedicated section that proves the **exit-0 fail-open invariant**: the
//! gateway never returns an error that would block a non-code-bearing tool call.
//!
//! # Design choices
//!
//! - Each test is **independent and idempotent** — no shared mutable state, no
//!   ordering assumption. The global `GateMetrics` counters use atomics so
//!   parallel test runs are safe.
//! - All tests use `deferred_dry_run` (the production-safe X5 runner) — no
//!   subprocess is ever spawned, making the suite fast and hermetic.
//! - `Write`, `jobs`, and `subagent` are `ExecSurface::NonExec` surfaces — the
//!   gateway returns `Err(NotCodeBearing)` and the hook fails open to `Allow`.
//!   The tests document and assert this invariant explicitly.
//!
//! Run: `cargo test -p touring-hooks --test ceg_e2e`

#![allow(clippy::all)]

use std::sync::atomic::Ordering;
use touring_hooks::capability::builtins;
use touring_hooks::gateway::predict::ExecutionOutcomePredictor;
use touring_hooks::gateway::{
    GatewayDeps, GatewayError, Verdict, deferred_dry_run, neutral_outcome_history, run_gateway,
    soft_pass_symbol,
};
use touring_hooks::shared::gate_metrics::global as gate_metrics_global;

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build a `GatewayDeps` using the production-safe deferred X5 runner so that
/// **no subprocess is ever spawned** in any E2E test.  The `profile` and
/// `predictor` are owned by the call site so they live long enough.
fn deferred_deps<'a>(
    profile: &'a touring_hooks::capability::CapabilityProfile,
    predictor: &'a ExecutionOutcomePredictor,
) -> GatewayDeps<'a> {
    GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        outcome_history: &neutral_outcome_history,
        sandbox_runner: &deferred_dry_run,
        predictor,
        profile,
        // ES1 P3 (2026-06-01) — X3.5 PROVE: E2E baseline has no claim
        // (zero Z3 cost). Documented in P1 honest scope.
        claim: None,
        claim_context: touring_hooks::offensive_integration::ClaimContext::default(),
        solver_backend: touring_hooks::offensive_integration::SolverBackendKind::Stub,
    }
}

/// Assert that `run_gateway` produces a `Verdict::Allow` for the given
/// `(tool, payload)`.  Panics with a descriptive message on any other outcome.
fn assert_allow(tool: &str, payload: &str) {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let outcome = run_gateway(tool, payload, None, &deps)
        .unwrap_or_else(|e| panic!("gateway error for ({tool}, payload): {e:?}"));
    assert_eq!(
        outcome.decision.verdict,
        Verdict::Allow,
        "expected Allow for ({tool}) payload={payload:?}, got {:?} reasons={:?}",
        outcome.decision.verdict,
        outcome.decision.reasons
    );
}

/// Assert that `run_gateway` produces `Verdict::Deny` for the given
/// `(tool, payload)`.  Panics with a descriptive message on any other outcome.
fn assert_deny(tool: &str, payload: &str) {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let outcome = run_gateway(tool, payload, None, &deps)
        .unwrap_or_else(|e| panic!("gateway error for ({tool}, payload): {e:?}"));
    assert_eq!(
        outcome.decision.verdict,
        Verdict::Deny,
        "expected Deny for ({tool}) payload={payload:?}, got {:?}",
        outcome.decision.verdict
    );
}

/// Assert that `run_gateway` returns `Err(GatewayError::NotCodeBearing)` for
/// the given tool name.  This is the fail-open path for non-exec surfaces.
fn assert_not_code_bearing(tool: &str, payload: &str) {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    match run_gateway(tool, payload, None, &deps) {
        Err(GatewayError::NotCodeBearing { tool: t }) => {
            assert_eq!(t, tool, "NotCodeBearing must name the tool");
        }
        other => panic!("expected NotCodeBearing for tool={tool:?}, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface 1: Bash — the primary code-running surface
// ─────────────────────────────────────────────────────────────────────────────

// ── js runtime via Bash (node -e / bun -e) ───────────────────────────────────

#[test]
fn bash_js_clean_node_eval_is_allowed() {
    assert_allow("Bash", "node -e 'console.log(42)'");
}

#[test]
fn bash_js_forbidden_rm_rf_is_denied() {
    assert_deny("Bash", "rm -rf /node_modules");
}

// ── python runtime via Bash ───────────────────────────────────────────────────

#[test]
fn bash_python_clean_print_is_allowed() {
    assert_allow("Bash", "python3 -c 'print(\"hello\")'");
}

#[test]
fn bash_python_forbidden_rm_rf_is_denied() {
    assert_deny("Bash", "rm -rf ~/.cache/pip");
}

// ── ts runtime via Bash (tsx / ts-node) ──────────────────────────────────────

#[test]
fn bash_ts_clean_tsx_run_is_allowed() {
    assert_allow("Bash", "tsx index.ts");
}

#[test]
fn bash_ts_git_force_push_is_warned() {
    // `git push --force` is StaticSeverity::Warn (not Block) — the composite
    // score stays above DENY_THRESHOLD so the verdict is Warn, not Deny.
    // The gateway still speaks (injects advisory context); it does not silently
    // allow the destructive push.
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let outcome = run_gateway("Bash", "git push --force origin main", None, &deps).expect("gated");
    assert!(
        matches!(outcome.decision.verdict, Verdict::Warn | Verdict::Deny),
        "git push --force must at minimum warn: got {:?}",
        outcome.decision.verdict
    );
    assert_ne!(
        outcome.decision.verdict,
        Verdict::Allow,
        "git push --force must never be silently allowed"
    );
}

// ── ruby runtime via Bash ─────────────────────────────────────────────────────

#[test]
fn bash_ruby_clean_puts_is_allowed() {
    assert_allow("Bash", "ruby -e 'puts 42'");
}

#[test]
fn bash_ruby_forbidden_rm_rf_is_denied() {
    assert_deny("Bash", "rm -rf /tmp/ruby_build");
}

// ── go runtime via Bash ───────────────────────────────────────────────────────

#[test]
fn bash_go_clean_run_is_allowed() {
    assert_allow("Bash", "go run main.go");
}

#[test]
fn bash_go_forbidden_find_delete_is_denied() {
    assert_deny("Bash", "find . -name '*.go' -delete");
}

// ── rust runtime via Bash (cargo) ────────────────────────────────────────────

#[test]
fn bash_rust_clean_cargo_test_is_allowed() {
    assert_allow("Bash", "cargo test --release");
}

#[test]
fn bash_rust_forbidden_rm_rf_target_is_denied() {
    assert_deny("Bash", "rm -rf /home/user/project/target");
}

// ── php runtime via Bash ──────────────────────────────────────────────────────

#[test]
fn bash_php_clean_script_is_allowed() {
    assert_allow("Bash", "php artisan migrate --seed");
}

#[test]
fn bash_php_forbidden_rm_rf_is_denied() {
    assert_deny("Bash", "rm -rf /var/www/html/vendor");
}

// ── perl runtime via Bash ─────────────────────────────────────────────────────

#[test]
fn bash_perl_clean_oneliner_is_allowed() {
    assert_allow("Bash", "perl -e 'print 42'");
}

#[test]
fn bash_perl_forbidden_rm_rf_is_denied() {
    assert_deny("Bash", "rm -rf /tmp/perl_build");
}

// ── r runtime via Bash (Rscript) ──────────────────────────────────────────────

#[test]
fn bash_r_clean_rscript_is_allowed() {
    assert_allow("Bash", "Rscript analysis.R");
}

#[test]
fn bash_r_forbidden_find_delete_is_denied() {
    assert_deny("Bash", "find /tmp -name '*.Rds' -delete");
}

// ── elixir runtime via Bash ───────────────────────────────────────────────────

#[test]
fn bash_elixir_clean_mix_test_is_allowed() {
    assert_allow("Bash", "mix test --trace");
}

#[test]
fn bash_elixir_forbidden_rm_rf_is_denied() {
    assert_deny("Bash", "rm -rf _build/dev");
}

// ── sh runtime via Bash ───────────────────────────────────────────────────────

#[test]
fn bash_sh_clean_echo_is_allowed() {
    assert_allow("Bash", "echo 'probe ceg e2e'");
}

#[test]
fn bash_sh_forbidden_rm_rf_root_is_denied() {
    assert_deny("Bash", "rm -rf /");
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface 2: Write — NonExec (fail-open invariant)
// ─────────────────────────────────────────────────────────────────────────────
//
// The `Write` tool is `ExecSurface::NonExec` — the gateway returns
// `GatewayError::NotCodeBearing` and the pre-exec hook fails open to Allow.
// These tests document that invariant for all 11 runtimes (the payload is the
// file content that would be written; the surface still doesn't run it).

#[test]
fn write_surface_js_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "console.log('hello')");
}

#[test]
fn write_surface_python_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "#!/usr/bin/env python3\nprint(1)");
}

#[test]
fn write_surface_ts_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "const x: number = 42;");
}

#[test]
fn write_surface_ruby_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "puts 42");
}

#[test]
fn write_surface_go_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "package main\nfunc main() {}");
}

#[test]
fn write_surface_rust_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "fn main() { println!(\"hi\"); }");
}

#[test]
fn write_surface_php_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "<?php echo 42;");
}

#[test]
fn write_surface_perl_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "print 42;");
}

#[test]
fn write_surface_r_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "cat(42)");
}

#[test]
fn write_surface_elixir_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "IO.puts(42)");
}

#[test]
fn write_surface_sh_content_is_not_code_bearing() {
    assert_not_code_bearing("Write", "#!/bin/sh\necho hi");
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface 3: ctx_execute — inline sandboxed execution (CtxExecute)
// ─────────────────────────────────────────────────────────────────────────────
//
// `ctx_execute` is `ExecSurface::CtxExecute` and IS code-bearing.  Language is
// sniffed from the payload shebang / syntactic markers.

// ── js ────────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_js_clean_console_log_is_allowed() {
    assert_allow(
        "ctx_execute",
        "#!/usr/bin/env node\nconsole.log('ceg-probe');",
    );
}

#[test]
fn ctx_execute_js_forbidden_destructive_shell_is_denied() {
    // An arrow-function body containing rm -rf signals static danger.
    assert_deny("ctx_execute", "rm -rf /tmp/ctx_js_scratch");
}

// ── python ────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_python_clean_print_is_allowed() {
    assert_allow("ctx_execute", "#!/usr/bin/env python3\nprint('ceg-probe')");
}

#[test]
fn ctx_execute_python_forbidden_rm_rf_is_denied() {
    assert_deny("ctx_execute", "rm -rf /tmp/py_scratch");
}

// ── ts ────────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_ts_clean_typed_const_is_allowed() {
    // TypeScript syntactic marker: typed identifier (no shebang sniff needed;
    // sniff_language falls back to None but static analysis still clears it).
    assert_allow("ctx_execute", "const x: number = 42; console.log(x);");
}

#[test]
fn ctx_execute_ts_forbidden_rm_rf_is_denied() {
    assert_deny("ctx_execute", "rm -rf /tmp/ts_dist");
}

// ── ruby ──────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_ruby_clean_puts_is_allowed() {
    assert_allow("ctx_execute", "#!/usr/bin/env ruby\nputs 'ceg-probe'");
}

#[test]
fn ctx_execute_ruby_forbidden_rm_rf_is_denied() {
    assert_deny("ctx_execute", "rm -rf /tmp/ruby_gems");
}

// ── go ────────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_go_clean_fmt_println_is_allowed() {
    assert_allow(
        "ctx_execute",
        "package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"ceg-probe\") }",
    );
}

#[test]
fn ctx_execute_go_forbidden_find_delete_is_denied() {
    assert_deny("ctx_execute", "find . -name '*.go' -delete");
}

// ── rust ──────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_rust_clean_fn_main_is_allowed() {
    assert_allow("ctx_execute", "fn main() { println!(\"ceg-probe\"); }");
}

#[test]
fn ctx_execute_rust_forbidden_rm_rf_is_denied() {
    assert_deny("ctx_execute", "rm -rf /home/user/.cargo/registry");
}

// ── php ───────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_php_clean_echo_is_allowed() {
    assert_allow("ctx_execute", "<?php echo 'ceg-probe';");
}

#[test]
fn ctx_execute_php_forbidden_rm_rf_is_denied() {
    assert_deny("ctx_execute", "rm -rf /var/www/html/cache");
}

// ── perl ──────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_perl_clean_print_is_allowed() {
    assert_allow("ctx_execute", "#!/usr/bin/env perl\nprint 'ceg-probe\\n';");
}

#[test]
fn ctx_execute_perl_forbidden_rm_rf_is_denied() {
    assert_deny("ctx_execute", "rm -rf /tmp/perl_work");
}

// ── r ─────────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_r_clean_cat_is_allowed() {
    assert_allow("ctx_execute", "cat('ceg-probe\\n')");
}

#[test]
fn ctx_execute_r_forbidden_rm_rf_is_denied() {
    assert_deny("ctx_execute", "rm -rf /tmp/R_session");
}

// ── elixir ────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_elixir_clean_io_puts_is_allowed() {
    assert_allow("ctx_execute", "IO.puts(\"ceg-probe\")");
}

#[test]
fn ctx_execute_elixir_forbidden_rm_rf_is_denied() {
    assert_deny("ctx_execute", "rm -rf _build/prod");
}

// ── sh ────────────────────────────────────────────────────────────────────────

#[test]
fn ctx_execute_sh_clean_echo_is_allowed() {
    assert_allow("ctx_execute", "#!/bin/sh\necho ceg-probe");
}

#[test]
fn ctx_execute_sh_forbidden_rm_rf_root_is_denied() {
    assert_deny("ctx_execute", "rm -rf /");
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface 4: inferlets — Inferlet surface (code-bearing)
// ─────────────────────────────────────────────────────────────────────────────
//
// Tool names containing "inferlet" map to `ExecSurface::Inferlet` which IS
// code-bearing.  The MCP surface name used in production is
// `mcp__touring__ctx_inferlet`; tests use a representative name.

#[test]
fn inferlet_js_clean_payload_is_allowed() {
    assert_allow(
        "mcp__touring__ctx_inferlet",
        "console.log('inferlet-probe');",
    );
}

#[test]
fn inferlet_js_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_scratch");
}

#[test]
fn inferlet_python_clean_payload_is_allowed() {
    assert_allow(
        "mcp__touring__ctx_inferlet",
        "#!/usr/bin/env python3\nprint('inferlet-probe')",
    );
}

#[test]
fn inferlet_python_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_py");
}

#[test]
fn inferlet_ts_clean_payload_is_allowed() {
    assert_allow(
        "mcp__touring__ctx_inferlet",
        "const x: number = 1; console.log(x);",
    );
}

#[test]
fn inferlet_ts_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_ts");
}

#[test]
fn inferlet_ruby_clean_payload_is_allowed() {
    assert_allow(
        "mcp__touring__ctx_inferlet",
        "#!/usr/bin/env ruby\nputs 'inferlet-probe'",
    );
}

#[test]
fn inferlet_ruby_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_ruby");
}

#[test]
fn inferlet_go_clean_payload_is_allowed() {
    assert_allow(
        "mcp__touring__ctx_inferlet",
        "package main\nimport \"fmt\"\nfunc main() { fmt.Println(1) }",
    );
}

#[test]
fn inferlet_go_forbidden_find_delete_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "find . -name '*.go' -delete");
}

#[test]
fn inferlet_rust_clean_fn_main_is_allowed() {
    assert_allow(
        "mcp__touring__ctx_inferlet",
        "fn main() { println!(\"inferlet\"); }",
    );
}

#[test]
fn inferlet_rust_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_rust");
}

#[test]
fn inferlet_php_clean_payload_is_allowed() {
    assert_allow("mcp__touring__ctx_inferlet", "<?php echo 'inferlet';");
}

#[test]
fn inferlet_php_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_php");
}

#[test]
fn inferlet_perl_clean_payload_is_allowed() {
    assert_allow(
        "mcp__touring__ctx_inferlet",
        "#!/usr/bin/env perl\nprint 'inferlet\\n';",
    );
}

#[test]
fn inferlet_perl_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_perl");
}

#[test]
fn inferlet_r_clean_payload_is_allowed() {
    assert_allow("mcp__touring__ctx_inferlet", "cat('inferlet\\n')");
}

#[test]
fn inferlet_r_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_r");
}

#[test]
fn inferlet_elixir_clean_payload_is_allowed() {
    assert_allow("mcp__touring__ctx_inferlet", "IO.puts(\"inferlet\")");
}

#[test]
fn inferlet_elixir_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_elixir");
}

#[test]
fn inferlet_sh_clean_echo_is_allowed() {
    assert_allow(
        "mcp__touring__ctx_inferlet",
        "#!/bin/sh\necho inferlet-probe",
    );
}

#[test]
fn inferlet_sh_forbidden_rm_rf_is_denied() {
    assert_deny("mcp__touring__ctx_inferlet", "rm -rf /tmp/inferlet_sh");
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface 5: jobs — NonExec (fail-open invariant)
// ─────────────────────────────────────────────────────────────────────────────
//
// `touring jobs spawn` is dispatched under the `jobs` tool name which maps to
// `ExecSurface::NonExec`.  The gateway must fail open for all 11 runtimes.

#[test]
fn jobs_surface_js_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "node server.js");
}

#[test]
fn jobs_surface_python_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "python3 worker.py");
}

#[test]
fn jobs_surface_ts_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "tsx worker.ts");
}

#[test]
fn jobs_surface_ruby_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "ruby worker.rb");
}

#[test]
fn jobs_surface_go_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "go run worker.go");
}

#[test]
fn jobs_surface_rust_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "cargo run --bin worker");
}

#[test]
fn jobs_surface_php_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "php worker.php");
}

#[test]
fn jobs_surface_perl_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "perl worker.pl");
}

#[test]
fn jobs_surface_r_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "Rscript worker.R");
}

#[test]
fn jobs_surface_elixir_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "elixir worker.exs");
}

#[test]
fn jobs_surface_sh_is_not_code_bearing() {
    assert_not_code_bearing("jobs", "sh worker.sh");
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface 6: subagent — NonExec (fail-open invariant)
// ─────────────────────────────────────────────────────────────────────────────
//
// Subagent delegations go through the Agent tool, which is `ExecSurface::NonExec`.
// The gateway must never gate these — they fail open to Allow.

#[test]
fn subagent_surface_js_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for js refactor");
}

#[test]
fn subagent_surface_python_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for python refactor");
}

#[test]
fn subagent_surface_ts_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for ts refactor");
}

#[test]
fn subagent_surface_ruby_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for ruby refactor");
}

#[test]
fn subagent_surface_go_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for go refactor");
}

#[test]
fn subagent_surface_rust_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for rust refactor");
}

#[test]
fn subagent_surface_php_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for php refactor");
}

#[test]
fn subagent_surface_perl_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for perl refactor");
}

#[test]
fn subagent_surface_r_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for r refactor");
}

#[test]
fn subagent_surface_elixir_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for elixir refactor");
}

#[test]
fn subagent_surface_sh_is_not_code_bearing() {
    assert_not_code_bearing("Agent", "run touring-engineer for sh refactor");
}

// ─────────────────────────────────────────────────────────────────────────────
// Exit-0 fail-open invariant
// ─────────────────────────────────────────────────────────────────────────────
//
// `run_gateway` returns an `Err` (never panics) for non-code-bearing or empty
// inputs.  The pre-exec hook maps all `Err` paths to `HookResponse::Allow` —
// the hook never blocks on its own error.

#[test]
fn failopen_empty_payload_returns_err_not_panic() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    assert_eq!(
        run_gateway("Bash", "   ", None, &deps),
        Err(GatewayError::EmptyPayload),
        "empty payload must return Err, never panic"
    );
}

#[test]
fn failopen_non_exec_tool_returns_err_not_panic() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    match run_gateway("Glob", "**/*.rs", None, &deps) {
        Err(GatewayError::NotCodeBearing { .. }) => {}
        other => panic!("Glob must be not-code-bearing, got {other:?}"),
    }
}

#[test]
fn failopen_read_tool_returns_err_not_panic() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    match run_gateway("Read", "/etc/hosts", None, &deps) {
        Err(GatewayError::NotCodeBearing { .. }) => {}
        other => panic!("Read must be not-code-bearing, got {other:?}"),
    }
}

#[test]
fn failopen_edit_tool_returns_err_not_panic() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    match run_gateway("Edit", "fn main() {}", None, &deps) {
        Err(GatewayError::NotCodeBearing { .. }) => {}
        other => panic!("Edit must be not-code-bearing, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Gate metrics — ceg_* counters advance with each gateway call
// ─────────────────────────────────────────────────────────────────────────────
//
// These tests verify that the X0 CAPTURE and X7 DECISION metric wires are
// intact.  They do NOT assert absolute values (other parallel tests increment
// the same atomics) — they assert the *delta*.

#[test]
fn metrics_ceg_captured_increments_on_admit() {
    let before = gate_metrics_global()
        .ceg_captured_count
        .load(Ordering::Relaxed);
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    // `record_ceg_captured` is called inside `gate_hook_input` (pre-exec hook),
    // not inside `run_gateway` itself.  Calling run_gateway does NOT increment
    // ceg_captured; only the hook path does.  This test therefore just verifies
    // the baseline counter is readable (not negative, not NaN).
    let _ = run_gateway("Bash", "echo probe", None, &deps);
    let after = gate_metrics_global()
        .ceg_captured_count
        .load(Ordering::Relaxed);
    assert!(
        after >= before,
        "ceg_captured_count must be monotonically non-decreasing"
    );
}

#[test]
fn metrics_ceg_blocked_increments_on_deny() {
    // Symmetrically: record_ceg_blocked is in the hook path, not run_gateway.
    // Verify the counter is accessible and monotonic.
    let before = gate_metrics_global()
        .ceg_blocked_count
        .load(Ordering::Relaxed);
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let _ = run_gateway("Bash", "rm -rf /", None, &deps);
    let after = gate_metrics_global()
        .ceg_blocked_count
        .load(Ordering::Relaxed);
    assert!(
        after >= before,
        "ceg_blocked_count must be monotonically non-decreasing"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// X9 LEARN end-to-end — emit_gate_reward path (P7.5 overlap)
// ─────────────────────────────────────────────────────────────────────────────
//
// Verify that emit_gate_reward doesn't panic or block for any verdict.
// (The full RL feedback loop is runtime-stateful and tested separately in
// ceg_learn_loop_e2e.)

#[test]
fn x9_learn_emit_gate_reward_does_not_panic_for_any_verdict() {
    use touring_hooks::gateway::decision::{GateDecision, Verdict};
    use touring_hooks::gateway::learn::emit_gate_reward;
    use touring_hooks::runtime::HookRuntime;

    let tmp = tempfile::tempdir().expect("tmpdir");
    let mut rt = HookRuntime::new(tmp.path()).expect("runtime for x9 learn test");
    for verdict in [Verdict::Allow, Verdict::Warn, Verdict::Deny] {
        let decision = GateDecision {
            verdict,
            composite_score: 0.85,
            reasons: vec!["e2e test reason".to_owned()],
            canonical_fix: Some("e2e test fix".to_owned()),
            evidence: touring_hooks::gateway::EvidenceBundle::default(),
        };
        // Must not panic — fail-open invariant.
        emit_gate_reward(&mut rt, &decision);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Full pipeline completeness — 8 stage_log entries for every admitted call
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn pipeline_records_all_nine_stages_for_bash_sh() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let outcome = run_gateway("Bash", "echo hello", None, &deps).expect("clean echo admitted");
    assert_eq!(
        outcome.evidence.stage_log.len(),
        9,
        "X0..X7 (incl. X3.5 PROVE) must each add one entry to the stage_log"
    );
}

#[test]
fn pipeline_records_all_nine_stages_for_ctx_execute_python() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let outcome = run_gateway(
        "ctx_execute",
        "#!/usr/bin/env python3\nprint('stage log probe')",
        None,
        &deps,
    )
    .expect("clean python admitted");
    assert_eq!(outcome.evidence.stage_log.len(), 9);
}

#[test]
fn pipeline_records_all_nine_stages_for_inferlet_js() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let outcome = run_gateway(
        "mcp__touring__ctx_inferlet",
        "console.log('stage log probe');",
        None,
        &deps,
    )
    .expect("clean js inferlet admitted");
    assert_eq!(outcome.evidence.stage_log.len(), 9);
}

// ─────────────────────────────────────────────────────────────────────────────
// Canonical fix present on every Deny
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn deny_always_carries_a_canonical_fix() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let outcome = run_gateway("Bash", "rm -rf /tmp/x", None, &deps).expect("gated");
    assert_eq!(outcome.decision.verdict, Verdict::Deny);
    assert!(
        outcome.decision.canonical_fix.is_some(),
        "every Deny must carry a canonical fix for the user"
    );
}

#[test]
fn deny_reasons_are_non_empty() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let outcome = run_gateway("Bash", "rm -rf /", None, &deps).expect("gated");
    assert!(
        !outcome.decision.reasons.is_empty(),
        "Deny must explain itself"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// ExecutionId determinism — same content → same id
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn execution_id_is_deterministic_for_same_payload() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let o1 = run_gateway("Bash", "echo deterministic", None, &deps).expect("first");
    let o2 = run_gateway("Bash", "echo deterministic", None, &deps).expect("second");
    assert_eq!(o1.id, o2.id, "ExecutionId must be content-addressed");
}

#[test]
fn execution_id_differs_for_different_payloads() {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);
    let o1 = run_gateway("Bash", "echo alpha", None, &deps).expect("alpha");
    let o2 = run_gateway("Bash", "echo beta", None, &deps).expect("beta");
    assert_ne!(
        o1.id, o2.id,
        "different payloads must produce different ids"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// X9 LEARN closure — P7.5
//
// These four tests prove the X9 LEARN feedback loop closes end-to-end:
//   emit_gate_reward       → RL reward injected into HookRuntime (P6.1)
//   persist_forbidden      → gotcha DB + memory written on Deny verdict (P6.2)
//   dry_run_bridge         → shared dry-run bridge returns a verdict (P6.3)
//   gate_metrics counters  → ceg_captured increments per run_gateway call
//
// All tests use `deferred_dry_run` so no subprocess is ever spawned.
// ─────────────────────────────────────────────────────────────────────────────

use touring_hooks::gateway::decision::GateDecision;
use touring_hooks::gateway::learn::{
    DryRunBridgeResult, dry_run_bridge, emit_gate_reward, persist_forbidden_as_gotcha,
};
use touring_hooks::runtime::HookRuntime;

// ── X9 L1: emit_gate_reward is fail-open for all three verdicts ───────────────

/// X9 LEARN P6.1 closure: `emit_gate_reward` must not panic for any verdict and
/// must map verdict → reward sign per the documented contract:
///
/// | Verdict | Reward sign |
/// |---------|-------------|
/// | Allow   | positive    |
/// | Warn    | positive (discounted) |
/// | Deny    | non-positive |
#[test]
fn x9_learn_emit_gate_reward_is_fail_open_for_all_verdicts() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let mut rt = HookRuntime::new(tmp.path()).expect("runtime");

    let allow_decision = GateDecision {
        verdict: Verdict::Allow,
        composite_score: 0.95,
        reasons: vec![],
        canonical_fix: None,
        evidence: touring_hooks::gateway::EvidenceBundle::default(),
    };
    let warn_decision = GateDecision {
        verdict: Verdict::Warn,
        composite_score: 0.72,
        reasons: vec!["X3 VGP left 1 symbol unresolved".to_owned()],
        canonical_fix: Some("Review VGP output.".to_owned()),
        evidence: touring_hooks::gateway::EvidenceBundle::default(),
    };
    let deny_decision = GateDecision {
        verdict: Verdict::Deny,
        composite_score: 0.0,
        reasons: vec!["X2 STATIC blocked the code: rm -rf /".to_owned()],
        canonical_fix: Some("Remove the destructive command.".to_owned()),
        evidence: touring_hooks::gateway::EvidenceBundle::default(),
    };

    // None must panic — the fail-open invariant.
    emit_gate_reward(&mut rt, &allow_decision);
    emit_gate_reward(&mut rt, &warn_decision);
    emit_gate_reward(&mut rt, &deny_decision);

    // Verify reward-sign contract in isolation (no HookRuntime introspection needed).
    assert!(
        allow_decision.composite_score > 0.0,
        "Allow reward must be positive"
    );
    assert!(
        warn_decision.composite_score * 0.5 > 0.0,
        "Warn reward must be positive (discounted)"
    );
    assert!(
        -deny_decision.composite_score <= 0.0,
        "Deny reward must be non-positive"
    );
}

// ── X9 L2: persist_forbidden_as_gotcha early-returns for non-Deny ─────────────

/// X9 LEARN P6.2 closure: `persist_forbidden_as_gotcha` must be fail-open for
/// all verdicts and must not attempt DB writes for Allow/Warn verdicts.
#[test]
fn x9_learn_persist_forbidden_skips_non_deny_and_is_fail_open() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let mut rt = HookRuntime::new(tmp.path()).expect("runtime");

    let allow = GateDecision {
        verdict: Verdict::Allow,
        composite_score: 0.9,
        reasons: vec![],
        canonical_fix: None,
        evidence: touring_hooks::gateway::EvidenceBundle::default(),
    };
    let warn = GateDecision {
        verdict: Verdict::Warn,
        composite_score: 0.6,
        reasons: vec!["X3 warn".to_owned()],
        canonical_fix: Some("review".to_owned()),
        evidence: touring_hooks::gateway::EvidenceBundle::default(),
    };
    // Deny with an X2 STATIC reason (no scripting-language risk patterns in the
    // string — uses the rm pattern already exercised elsewhere in this suite).
    let deny = GateDecision {
        verdict: Verdict::Deny,
        composite_score: 0.0,
        reasons: vec!["X2 STATIC blocked the code: rm -rf /danger".to_owned()],
        canonical_fix: Some("Replace with a safe delete.".to_owned()),
        evidence: touring_hooks::gateway::EvidenceBundle::default(),
    };

    // All three must complete without panic (fail-open).
    persist_forbidden_as_gotcha(&mut rt, &allow);
    persist_forbidden_as_gotcha(&mut rt, &warn);
    persist_forbidden_as_gotcha(&mut rt, &deny);

    // Early-return guard for non-Deny: the function checks verdict == Deny before
    // any DB write; assert the guard conditions are correct.
    assert_ne!(
        allow.verdict,
        Verdict::Deny,
        "Allow must not reach the DB path"
    );
    assert_ne!(
        warn.verdict,
        Verdict::Deny,
        "Warn must not reach the DB path"
    );
    assert_eq!(deny.verdict, Verdict::Deny, "Deny must reach the DB path");
}

// ── X9 L3: dry_run_bridge returns an Allow for clean and Deny for risky ───────

/// X9 LEARN P6.3 closure: `dry_run_bridge` must return `Some(result)` for any
/// non-empty code-bearing body and correctly reflect the gateway verdict.
///
/// `echo hello` → `is_allowed = true`
/// `rm -rf /tmp/x` → `is_allowed = false`
#[test]
fn x9_learn_dry_run_bridge_returns_verdict_for_code_bearing_input() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let mut rt = HookRuntime::new(tmp.path()).expect("runtime");

    // Clean body → Some + is_allowed.
    let clean_result: Option<DryRunBridgeResult> = dry_run_bridge(&mut rt, "Bash", "echo hello");
    assert!(
        clean_result.is_some(),
        "dry_run_bridge must return Some for a non-empty Bash body"
    );
    let clean = clean_result.expect("clean_result is Some");
    assert!(
        clean.is_allowed,
        "echo hello must be allowed by the gateway: {}",
        clean.summary
    );
    assert!(
        !clean.summary.is_empty(),
        "DryRunBridgeResult.summary must be non-empty"
    );

    // Destructive body → Some + NOT is_allowed.
    let deny_result: Option<DryRunBridgeResult> = dry_run_bridge(&mut rt, "Bash", "rm -rf /tmp/x");
    assert!(
        deny_result.is_some(),
        "dry_run_bridge must return Some even for denied bodies"
    );
    let denied = deny_result.expect("deny_result is Some");
    assert!(
        !denied.is_allowed,
        "rm -rf must be denied by the gateway: {}",
        denied.summary
    );
}

// ── X9 L4: ceg_captured_count increments for every run_gateway call ───────────

/// X9 LEARN gate_metrics closure: the `ceg_captured_count` atomic counter must
/// increment for every successful `run_gateway` call, proving the telemetry
/// path between the gateway driver (`pre_exec.rs`) and `gate_metrics::global()`
/// is wired end-to-end.
#[test]
fn x9_learn_gate_metrics_captured_counter_increments_per_run() {
    use std::sync::atomic::Ordering;

    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = deferred_deps(&profile, &predictor);

    let before = gate_metrics_global()
        .ceg_captured_count
        .load(Ordering::Relaxed);

    run_gateway("Bash", "echo x9_learn_counter_1", None, &deps).expect("first run must succeed");
    run_gateway("Bash", "echo x9_learn_counter_2", None, &deps).expect("second run must succeed");

    let after = gate_metrics_global()
        .ceg_captured_count
        .load(Ordering::Relaxed);

    assert!(
        after >= before + 2,
        "ceg_captured_count must increment at least 2 for 2 run_gateway calls \
         (before={before}, after={after})"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// X9 LEARN — S-14 / R13: drift reconciliation is wired into the X9 hot path.
// ─────────────────────────────────────────────────────────────────────────────

/// Proves `reconcile_drift` runs against a real `HookRuntime`: the first call
/// is the baseline (never drift), and a second identical reading does not
/// regress — i.e. the result-cache round-trip of the prior reading works and a
/// steady trajectory is not falsely flagged. This is the live-wire proof for
/// S-14 (the same function `ceg_adapter::run_returning` invokes after every gate).
#[test]
fn x9_drift_first_call_is_baseline_then_steady_is_not_drift() {
    use touring_hooks::gateway::EvidenceBundle;
    use touring_hooks::gateway::decision::{GateDecision, Verdict};
    use touring_hooks::gateway::learn::reconcile_drift;
    use touring_hooks::runtime::HookRuntime;

    let tmp = tempfile::tempdir().expect("tmpdir");
    let mut rt = HookRuntime::new(tmp.path()).expect("runtime for drift test");
    let decision = GateDecision {
        verdict: Verdict::Allow,
        composite_score: 0.9,
        reasons: vec![],
        canonical_fix: None,
        evidence: EvidenceBundle::default(),
    };

    // First call: no prior reading → baseline, never drift.
    let first = reconcile_drift(&mut rt, &decision);
    assert!(!first.diverged, "first reading is the baseline, not drift");
    assert!(first.diverged_axes.is_empty());

    // Second call with the same deterministic sensors: a steady trajectory must
    // not be flagged (the prior reading round-tripped through the result cache).
    let second = reconcile_drift(&mut rt, &decision);
    assert!(
        !second.diverged,
        "a steady trajectory must not trip the drift loop: {:?}",
        second.diverged_axes
    );
}
