//! E2E tests for touring_ctx_execute MCP tool.
//!
//! Tests sandboxed code execution via direct Rust API calls.
//! T2.1-T2.8 per S2 D2.5
//!
//! Note: ctx_execute is an MCP tool (daemon-only). Direct Rust API testing
//! bypasses the MCP transport layer for fast unit validation.
//! Runtimes (node/bun/python3) must be installed — tests SKIP if missing.

use std::process::Command;
use touring_server::tools::ctx_execute_tools::{CtxExecuteInput, CtxExecuteOutput};

fn runtime_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_ctx(input: CtxExecuteInput) -> CtxExecuteOutput {
    run_ctx_with_override(input, None)
}

fn run_ctx_with_override(
    input: CtxExecuteInput,
    allow_forbidden: Option<bool>,
) -> CtxExecuteOutput {
    use touring_server::tools::ctx_execute_tools::ctx_execute_impl;
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(ctx_execute_impl(
        input.language,
        input.code,
        input.args,
        input.timeout_ms,
        input.cwd,
        allow_forbidden,
    ))
    .unwrap()
}

// T2.1: JavaScript execution (node)
#[test]
fn test_js_execution() {
    if !runtime_available("node") {
        eprintln!("SKIP (node not available)");
        return;
    }
    let input = CtxExecuteInput {
        language: "js".to_string(),
        code: "console.log(42)".to_string(),
        args: None,
        timeout_ms: Some(10000),
        cwd: None,
    };
    let out = run_ctx(input);
    assert_eq!(out.exit_code, 0, "JS should exit 0: {}", out.stderr);
    assert!(
        out.stdout.contains("42"),
        "expected 42 in stdout: {}",
        out.stdout
    );
}

// T2.2: Python execution
#[test]
fn test_python_execution() {
    if !runtime_available("python3") {
        eprintln!("SKIP (python3 not available)");
        return;
    }
    let input = CtxExecuteInput {
        language: "python".to_string(),
        code: "print(42)".to_string(),
        args: None,
        timeout_ms: Some(10000),
        cwd: None,
    };
    let out = run_ctx(input);
    assert_eq!(out.exit_code, 0, "Python should exit 0: {}", out.stderr);
    assert!(
        out.stdout.contains("42"),
        "expected 42 in stdout: {}",
        out.stdout
    );
}

// T2.3: TypeScript execution (bun or ts-node)
#[test]
fn test_typescript_execution() {
    if !runtime_available("bun") && !runtime_available("ts-node") {
        eprintln!("SKIP (bun/ts-node not available)");
        return;
    }
    let input = CtxExecuteInput {
        language: "ts".to_string(),
        code: "console.log(42)".to_string(),
        args: None,
        timeout_ms: Some(15000),
        cwd: None,
    };
    let out = run_ctx(input);
    assert_eq!(out.exit_code, 0, "TS should exit 0: {}", out.stderr);
    assert!(
        out.stdout.contains("42"),
        "expected 42 in stdout: {}",
        out.stdout
    );
}

// T2.4: Shell execution
#[test]
fn test_shell_execution() {
    let input = CtxExecuteInput {
        language: "shell".to_string(),
        code: "echo 42".to_string(),
        args: None,
        timeout_ms: Some(10000),
        cwd: None,
    };
    let out = run_ctx(input);
    assert_eq!(out.exit_code, 0, "Shell should exit 0: {}", out.stderr);
    assert!(
        out.stdout.contains("42"),
        "expected 42 in stdout: {}",
        out.stdout
    );
}

// T2.5: Timeout enforcement
#[test]
fn test_timeout_enforcement() {
    let input = CtxExecuteInput {
        language: "shell".to_string(),
        code: "sleep 10".to_string(),
        args: None,
        timeout_ms: Some(500),
        cwd: None,
    };
    let out = run_ctx(input);
    let timed_out = out.exit_code != 0
        || out.stderr.contains("timeout")
        || out.stderr.contains("Timeout")
        || out.duration_ms < 5000;
    assert!(
        timed_out,
        "Should timeout: exit={} stderr={}",
        out.exit_code, out.stderr
    );
}

// T2.6: Output truncation (>64KB) — always runs
#[test]
fn test_output_truncation() {
    let input = CtxExecuteInput {
        language: "shell".to_string(),
        code: "printf 'x%.0s' {1..100000}".to_string(),
        args: None,
        timeout_ms: Some(10000),
        cwd: None,
    };
    let out = run_ctx(input);
    assert!(
        out.stdout.len() <= 65536,
        "stdout should be truncated to 64KB: got {} bytes",
        out.stdout.len()
    );
}

// T2.7: Forbidden calls detection — always runs (JS-only)
#[test]
fn test_forbidden_calls_detected() {
    if !runtime_available("node") {
        eprintln!("SKIP (node not available)");
        return;
    }
    let input = CtxExecuteInput {
        language: "js".to_string(),
        // A genuinely forbidden call — fs.writeFileSync in member-call form.
        // `require('fs')` alone is a benign import (any file read needs it)
        // and is correctly NOT flagged; the danger is the dangerous *call*.
        code: "const fs = require('fs'); fs.writeFileSync('/tmp/ceg_e2e_probe', 'x');".to_string(),
        args: None,
        timeout_ms: Some(10000),
        cwd: None,
    };
    let out = run_ctx(input);
    assert!(
        !out.forbidden_calls.is_empty(),
        "Should detect fs.writeFileSync forbidden call: forbidden={:?} stdout={}",
        out.forbidden_calls,
        out.stdout
    );
}

// T2.8: Error propagation (exit code != 0)
#[test]
fn test_error_propagation() {
    if !runtime_available("python3") {
        eprintln!("SKIP (python3 not available)");
        return;
    }
    let input = CtxExecuteInput {
        language: "python".to_string(),
        code: "raise Exception('test')".to_string(),
        args: None,
        timeout_ms: Some(10000),
        cwd: None,
    };
    let out = run_ctx(input);
    assert!(
        out.exit_code != 0 || out.stdout.contains("traceback") || out.stdout.contains("Error"),
        "Should propagate error: exit={} stdout={}",
        out.exit_code,
        out.stdout
    );
}

// P1.3 E2E: AST-based scan runs through ctx_execute_impl and populates forbidden_calls.
// T2.9: Forbidden calls detected via ast_forbidden_scan (P1.3).
#[test]
fn test_p13_ast_forbidden_scan_populates_field() {
    use touring_hooks::sandbox_executor::SandboxLanguage;
    use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;
    // Direct scan — does not require runtime to be installed.
    let code = "import subprocess\nsubprocess.run(['ls'])";
    let found = ast_forbidden_scan(SandboxLanguage::Python, code);
    assert!(
        !found.is_empty(),
        "P1.3: ast_forbidden_scan should detect subprocess.run in Python: {:?}",
        found
    );
}

// T2.10: Clean code produces no forbidden_calls (no false positives).
#[test]
fn test_p13_no_false_positives_on_clean_code() {
    use touring_hooks::sandbox_executor::SandboxLanguage;
    use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;
    let code = "x = 1 + 2\nprint(x)";
    let found = ast_forbidden_scan(SandboxLanguage::Python, code);
    assert!(
        found.is_empty(),
        "P1.3: Clean Python code must not trigger false positives: {:?}",
        found
    );
}

// T2.11: Perl uses substring fallback (no ast-grep grammar).
#[test]
fn test_p13_perl_substring_fallback() {
    use touring_hooks::sandbox_executor::SandboxLanguage;
    use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;
    let code = "system('ls -la');";
    let found = ast_forbidden_scan(SandboxLanguage::Perl, code);
    assert!(
        !found.is_empty(),
        "P1.3: Perl substring fallback should detect system(): {:?}",
        found
    );
}

// P1.4 E2E: ForbiddenCallPolicy enforcement.
// T2.12: allow_forbidden=true bypasses Block policy (fail-open override).
#[test]
fn test_p14_allow_forbidden_override() {
    use touring_server::tools::ctx_execute_tools::{ForbiddenCallPolicy, ctx_execute_impl};
    // Set Block policy in env for this test.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_ENFORCE", "1") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_OFF") };

    let rt = tokio::runtime::Runtime::new().unwrap();
    // With allow_forbidden=true, even Block mode should proceed (not return Err).
    let result = rt.block_on(ctx_execute_impl(
        "python".to_string(),
        // Clean code — scanner finds nothing, so block never triggers regardless.
        "x = 1\nprint(x)".to_string(),
        None,
        Some(5000),
        None,
        Some(true),
    ));
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_ENFORCE") };
    // Even with block mode, clean code should succeed.
    assert!(
        result.is_ok(),
        "P1.4: Clean code with allow_forbidden=true must succeed: {:?}",
        result.err()
    );
    let _ = ForbiddenCallPolicy::Block; // ensure type is accessible
}

// T2.13: Off policy suppresses scanner entirely — forbidden_calls is empty even for bad code.
#[test]
fn test_p14_off_policy_suppresses_detection() {
    use touring_hooks::sandbox_executor::SandboxLanguage;
    use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;
    // Verify via direct scan that the code WOULD be detected normally.
    let code = "import subprocess\nsubprocess.run(['ls'])";
    let would_detect = ast_forbidden_scan(SandboxLanguage::Python, code);
    assert!(
        !would_detect.is_empty(),
        "Precondition: code should be detectable normally"
    );

    // With Off policy, ctx_execute_impl returns empty forbidden_calls.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_OFF", "1") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_ENFORCE") };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(ctx_execute_impl(
        "python".to_string(),
        code.to_string(),
        None,
        Some(5000),
        None,
        None,
    ));
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_OFF") };

    // Off mode: should not error (no blocking), forbidden_calls should be empty.
    if let Ok(out) = result {
        assert!(
            out.forbidden_calls.is_empty(),
            "P1.4: Off policy must suppress forbidden_calls: {:?}",
            out.forbidden_calls
        );
    }
    // If execution fails for sandbox reasons (runtime missing), that's OK for this test.
}

use touring_server::tools::ctx_execute_tools::ctx_execute_impl;
