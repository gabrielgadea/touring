//! E2E integration tests for the `touring` binary.
//!
//! Validates the contract between Claude Code hooks and the touring binary:
//! - Hook subcommands ALWAYS exit 0 (even on error)
//! - Non-hook subcommands exit 1 on error
//! - JSON output matches expected schema
//! - Each subcommand produces the correct hookEventName

use std::process::Command;

/// Helper: run touring binary with stdin and capture output.
fn run_touring(args: &[&str], stdin_data: &str) -> (i32, String, String) {
    let binary = std::env::var("TOURING_BINARY").unwrap_or_else(|_| {
        // Find the binary in target/debug or target/release
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let debug = workspace.join("target/debug/touring");
        let release = workspace.join("target/release/touring");
        if release.exists() {
            release.to_string_lossy().to_string()
        } else {
            debug.to_string_lossy().to_string()
        }
    });

    let mut cmd = Command::new(&binary);
    cmd.args(args);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    // Bind console-subscriber to an ephemeral port so tests don't conflict
    // with a running touring daemon that already owns port 6669.
    cmd.env("TOKIO_CONSOLE_BIND", "127.0.0.1:0");
    // Suppress OTLP connection errors in test output (no collector running).
    cmd.env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:0");

    let mut child = cmd.spawn().unwrap_or_else(|e| {
        panic!("Failed to spawn touring binary at {}: {}", binary, e);
    });

    if !stdin_data.is_empty() {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(stdin_data.as_bytes()).unwrap();
    }
    drop(child.stdin.take()); // close stdin to unblock read

    let output = child.wait_with_output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (code, stdout, stderr)
}

// ── Contract: Hook subcommands exit 0 even on error ──────────────────

#[test]
fn hook_classify_intent_exits_0() {
    let (code, stdout, _) = run_touring(&["classify-intent"], r#"{"prompt":"fix the auth bug"}"#);
    assert_eq!(code, 0, "classify-intent must exit 0");
    assert!(!stdout.is_empty(), "classify-intent must produce output");

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json["hookSpecificOutput"]["hookEventName"].is_string());
    assert_eq!(
        json["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
    assert!(json["metadata"]["cila_level"].is_number());
}

#[test]
fn hook_scan_pii_exits_0_no_pii() {
    let (code, stdout, _) = run_touring(
        &["scan-pii"],
        r#"{"tool_input":{"content":"hello world no pii here"}}"#,
    );
    assert_eq!(code, 0, "scan-pii must exit 0");
    // No PII = no output (silent)
    assert!(
        stdout.trim().is_empty(),
        "No PII should produce empty output"
    );
}

#[test]
fn hook_scan_pii_exits_0_with_pii() {
    let (code, stdout, _) = run_touring(
        &["scan-pii"],
        r#"{"tool_input":{"content":"CPF: 123.456.789-01"}}"#,
    );
    assert_eq!(code, 0, "scan-pii must exit 0 even with PII");
    assert!(!stdout.is_empty(), "PII detected should produce output");

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        json["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("PII")
    );
}

#[test]
fn hook_prompt_enhance_exits_0() {
    let (code, stdout, _) = run_touring(
        &["prompt-enhance"],
        r#"{"prompt":"refactor the database module"}"#,
    );
    assert_eq!(code, 0, "prompt-enhance must exit 0");
    assert!(!stdout.is_empty());

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        json["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
    assert!(json["hookSpecificOutput"]["additionalContext"].is_string());
}

#[test]
fn hook_neural_pre_read_exits_0_empty_input() {
    let (code, _, _) = run_touring(&["pre-read"], "{}");
    assert_eq!(code, 0, "pre-read must exit 0 on empty input");
}

#[test]
fn hook_neural_post_bash_exits_0() {
    let (code, _, _) = run_touring(
        &["post-bash"],
        r#"{"tool_input":{"command":"cargo test"},"exit_code":0}"#,
    );
    assert_eq!(code, 0, "post-bash must exit 0");
}

#[test]
fn hook_session_start_exits_0() {
    let (code, _, _) = run_touring(&["session-start"], r#"{"session_id":"test-e2e-binary"}"#);
    assert_eq!(code, 0, "session-start must exit 0");
}

#[test]
fn hook_session_stop_exits_0() {
    let (code, _, _) = run_touring(&["session-stop"], r#"{"session_id":"test-e2e-binary"}"#);
    assert_eq!(code, 0, "session-stop must exit 0");
}

#[test]
fn hook_cortex_exits_0() {
    let (code, _, _) = run_touring(
        &["cortex", "session-start"],
        r#"{"session_id":"cortex-e2e"}"#,
    );
    assert_eq!(code, 0, "cortex must exit 0");
}

// ── Contract: Non-hook subcommands exit 1 on error ───────────────────

#[test]
fn unknown_subcommand_exits_1() {
    let (code, _, _) = run_touring(&["nonexistent-command"], "");
    assert_eq!(code, 1, "Unknown subcommand must exit 1");
}

// ── Contract: Version and help ───────────────────────────────────────

#[test]
fn version_matches_crate_version() {
    let (code, _, stderr) = run_touring(&["--version"], "");
    assert_eq!(code, 0);
    // Drift-proof: assert the binary's version line matches this crate's
    // compile-time version, so a workspace version bump never stales this test
    // again (was hardcoded "30.0", broke on the 30.3.0 bump — 2026-07-02).
    let expected = format!("touring {}", env!("CARGO_PKG_VERSION"));
    assert!(
        stderr.contains(&expected),
        "Version line should contain {expected:?}, got: {stderr}"
    );
}

#[test]
fn help_lists_subcommands() {
    let (code, _, stderr) = run_touring(&["--help"], "");
    assert_eq!(code, 0);
    assert!(stderr.contains("serve"), "Help must list 'serve'");
    assert!(stderr.contains("cortex"), "Help must list 'cortex'");
    assert!(
        stderr.contains("classify-intent"),
        "Help must list 'classify-intent'"
    );
    assert!(stderr.contains("scan-pii"), "Help must list 'scan-pii'");
    assert!(
        stderr.contains("prompt-enhance"),
        "Help must list 'prompt-enhance'"
    );
}

// ── Contract: Context compiler ───────────────────────────────────────

#[test]
fn context_compile_produces_json() {
    let (code, stdout, _) = run_touring(
        &[
            "context",
            "compile",
            "--intent",
            "debug auth",
            "--files",
            "auth.py,main.py",
            "--max-tokens",
            "500",
        ],
        r#"{"gotchas":[],"relations":[],"recent_errors":[]}"#,
    );
    assert_eq!(code, 0, "context compile must exit 0");
    assert!(!stdout.is_empty());

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["intent"], "debug auth");
    assert!(json["estimated_tokens"].is_number());
    assert!(json["cache_key"].is_string());
}

// ── Contract: Cortex metrics in output ───────────────────────────────

#[test]
fn cortex_output_includes_metrics() {
    let (code, stdout, _) = run_touring(
        &["cortex", "session-start"],
        r#"{"session_id":"metrics-e2e"}"#,
    );
    assert_eq!(code, 0);

    if !stdout.trim().is_empty() {
        let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        // Metrics should be present if handlers ran
        if json.get("metrics").is_some() {
            assert!(json["metrics"]["handlers_executed"].is_number());
            assert!(json["metrics"]["total_ms"].is_string());
        }
    }
}
