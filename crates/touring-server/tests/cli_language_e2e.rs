//! E2E tests for touring language CLI + language support matrix (Wave S-LG).
//! Tests verify the `touring language list|rust|python` CLI commands work
//! end-to-end, including Tier 1 Rust support and aliased subcommands (ts, py, etc).

use std::process::Command;

// Run from the touring binary directory so we use the compiled binary
fn touring_binary() -> String {
    // If TOURING_BINARY is set, use it. Otherwise look relative to THIS crate's manifest dir.
    std::env::var("TOURING_BINARY").unwrap_or_else(|_| {
        // Resolve relative to the touring-server crate directory
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let debug = workspace_root.join("target/debug/touring");
        let release = workspace_root.join("target/release/touring");
        if debug.exists() {
            debug.to_string_lossy().to_string()
        } else if release.exists() {
            release.to_string_lossy().to_string()
        } else {
            // Fallback: just "touring" in PATH
            "touring".to_string()
        }
    })
}

fn run_touring_lang(args: &[&str]) -> Result<String, String> {
    let binary = touring_binary();
    let mut cmd = Command::new(&binary);
    cmd.arg("language");
    for arg in args {
        cmd.arg(*arg);
    }
    cmd.env_remove("TOURING_DAEMON_SOCK"); // don't require daemon for this test
    let output = cmd.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[test]
fn language_list_default_subcommand() {
    // `touring language` (no subcommand) defaults to `list`
    let out = run_touring_lang(&[]).expect("touring language should succeed");
    // Must contain Tier info (Rust should be Tier 1)
    assert!(out.contains("Tier"), "list output must contain Tier info");
    assert!(out.contains("rust"), "list output must mention rust");
}

#[test]
fn language_list_explicit() {
    let out = run_touring_lang(&["list"]).expect("touring language list should succeed");
    assert!(out.contains("Tier"), "list output must contain Tier");
    assert!(out.contains("rust"), "rust must appear in list");
    assert!(out.contains("typescript"), "typescript must appear in list");
    assert!(out.contains("python"), "python must appear in list");
}

#[test]
fn language_rust_detail() {
    let out = run_touring_lang(&["rust"]).expect("touring language rust should succeed");
    // Tier 1 Rust must show capabilities
    assert!(out.contains("Tier"), "rust detail must show tier");
    assert!(out.contains("Tier 1"), "rust must be Tier 1");
}

#[test]
fn language_ts_alias_resolves() {
    let out = run_touring_lang(&["ts"]).expect("touring language ts should succeed");
    // ts alias must resolve to typescript
    assert!(
        out.contains("typescript"),
        "ts alias must resolve to typescript detail"
    );
}

#[test]
fn language_py_alias_resolves() {
    let out = run_touring_lang(&["py"]).expect("touring language py should succeed");
    // py alias must resolve to python
    assert!(
        out.contains("python"),
        "py alias must resolve to python detail"
    );
}

#[test]
fn language_unknown_error() {
    let result = run_touring_lang(&["cobol"]);
    assert!(result.is_err(), "unknown language should error");
    let err = result.unwrap_err();
    assert!(
        err.contains("cobol"),
        "error message must mention unknown language"
    );
}

#[test]
fn language_json_output() {
    let out =
        run_touring_lang(&["list", "--json"]).expect("touring language list --json should succeed");
    // JSON output must be valid
    let parsed: serde_json::Value =
        serde_json::from_str(&out).expect("JSON output must be valid JSON");
    // Array must contain rust
    let has_rust = parsed
        .as_array()
        .expect("list --json must return array")
        .iter()
        .filter_map(|v| v.get("language").and_then(|l| l.as_str()))
        .any(|lang| lang == "rust");
    assert!(has_rust, "rust must be in language list JSON");
}

#[test]
fn language_detail_json() {
    let out =
        run_touring_lang(&["rust", "--json"]).expect("touring language rust --json should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&out).expect("detail --json must be valid JSON");
    assert_eq!(
        parsed.get("language").and_then(|v| v.as_str()),
        Some("rust")
    );
    assert_eq!(parsed.get("tier").and_then(|v| v.as_u64()), Some(1));
}

#[test]
fn language_help_flag() {
    // --help should work without daemon
    let binary = touring_binary();
    let output = Command::new(&binary)
        .arg("language")
        .arg("--help")
        .env_remove("TOURING_DAEMON_SOCK")
        .output()
        .expect("touring language --help should work");
    assert!(output.status.success(), "help flag should succeed");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("touring language"), "help must show usage");
}
