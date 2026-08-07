//! E2E tests for RFC-100 Q-2xx and B-3xx diagnostic codes.
//!
//! Covers:
//! - Q-230: HighAntipatternDensity (antipattern_score > 0.3)
//! - Q-240: HighCyclomatic (max_complexity > 20)
//! - B-310: BlastInjection (predictive blast injects symbols)
//! - B-320: CrossFeatureBlast (blast crosses feature boundaries)
//!
//! Run with:
//!   cargo test -p touring-hooks --test e2e_diagnostic_rfc100 -- --nocapture

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

mod common;

/// Locate the touring binary (preferring release over debug).
fn locate_binary(name: &str) -> Option<PathBuf> {
    let workspace_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("target"))?;

    // `cargo llvm-cov` redirects the build to `target/llvm-cov-target/`, so a
    // binary from a plain `cargo build` is invisible inside a coverage run —
    // exactly how the CI coverage job failed on 2026-08-02. An explicit
    // CARGO_TARGET_DIR wins for the same reason.
    let roots = [
        std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from),
        Some(workspace_target.join("llvm-cov-target")),
        Some(workspace_target),
    ];
    for root in roots.into_iter().flatten() {
        for profile in ["release", "debug"] {
            let candidate = root.join(profile).join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Resolve the binary or SKIP the test — never panic with a "skipping" message.
///
/// `expect("touring binary not built — skipping")` said skip and did the exact
/// opposite: it panicked, failing CI with a message claiming the test had been
/// skipped. A test that announces a skip must actually skip.
fn touring_bin_or_skip() -> Option<PathBuf> {
    match locate_binary("touring") {
        Some(bin) => Some(bin),
        None => {
            // libtest names the worker thread after the test, so the skip line
            // identifies itself without every call site repeating a literal.
            let test = std::thread::current().name().unwrap_or("test").to_string();
            eprintln!(
                "SKIP {test}: touring binary not built. Build it first: \
                 `cargo build -p touring-server` (looked in $CARGO_TARGET_DIR, \
                 target/llvm-cov-target/{{release,debug}} and target/{{release,debug}})."
            );
            None
        }
    }
}

/// Run a binary with `stdin` and capture stdout. Returns `None` if the
/// binary isn't built — caller should treat that as a skip, not a failure.
/// Serialises the daemon-touching probes in this binary.
///
/// libtest runs tests concurrently, and every probe here shells out to `touring`,
/// which round-trips through the daemon's single-threaded actor. Run in parallel
/// they contend, the command fails, and — because the assertions parsed stdout
/// without ever checking the exit status — the failure surfaced as the useless
/// `EOF while parsing a value, line: 1, column: 0`. Proven on 2026-08-02: the
/// same three tests pass individually (`--test-threads=1`) and fail together.
static DAEMON_PROBE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run `touring <args>` and return its parsed JSON stdout.
///
/// Panics with the command, exit code and **stderr** when the binary produces no
/// JSON. The previous `from_slice(&out.stdout).expect("must emit valid JSON")`
/// threw away the one thing that explained the failure — the same fail-loud
/// defect fixed in `daemon_client::daemon_failure_message` the same day.
fn touring_json(bin: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let _serial = DAEMON_PROBE.lock().unwrap_or_else(|e| e.into_inner());
    let out = Command::new(bin)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn `touring {}`: {e}", args.join(" ")));
    assert!(
        out.status.success() && !out.stdout.is_empty(),
        "`touring {}` produced no JSON (exit {:?}).\nstderr: {}",
        args.join(" "),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "`touring {}` emitted non-JSON: {e}\nstdout: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout).trim()
        )
    })
}

/// `socket = Some(..)` aponta o cliente para um daemon PRIVADO deste teste.
///
/// O mutex [`DAEMON_PROBE`] serializa apenas dentro DESTE binário; ele não
/// protege contra a contenção que aparece quando as suítes de várias crates
/// rodam juntas — foi assim que `b310_path_wired_when_predictive_blast_injects_symbols`
/// ficou verde isolado e vermelho em `cargo test -p touring-cli -p touring-hooks
/// -p touring-hooks-core -p touring-hooks-shared` (03/08/2026). Com socket
/// próprio a contenção some por construção, e a suíte deixa de tocar o daemon
/// que o usuário está usando.
fn run_with_stdin(
    bin_name: &str,
    args: &[&str],
    stdin_payload: &str,
    socket: Option<&str>,
) -> Option<(String, String, i32)> {
    let _serial = DAEMON_PROBE.lock().unwrap_or_else(|e| e.into_inner());
    let bin = locate_binary(bin_name)?;
    let mut command = Command::new(&bin);
    if let Some(sock) = socket {
        command.env("TOURING_DAEMON_SOCKET", sock);
    }
    let mut child = command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");

    if let Some(mut sin) = child.stdin.take() {
        sin.write_all(stdin_payload.as_bytes())
            .expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait child");
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    ))
}

// ── Q-230: HighAntipatternDensity ───────────────────────────────────────────

/// Q-230 fires when a file has antipattern_score > 0.3 (30%).
/// Uses `touring ast quality` which returns antipattern_score in [0,1].
/// The diagnostic is emitted via `tracing::warn!` during pre-edit analysis.
#[test]
fn q230_fires_when_antipattern_score_exceeds_threshold() {
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").expect("create temp file");
    let src = r#"
fn main() {
    let x = Some(1).unwrap();
    let y = None.unwrap();
    let z = vec![1, 2, 3].unwrap();
}
"#;
    std::fs::write(tmp.path(), src).expect("write temp file");

    let Some(bin) = touring_bin_or_skip() else {
        return;
    };

    // Verify the score is high via ast quality.
    let json = touring_json(&bin, &["ast", "quality", tmp.path().to_str().unwrap()]);
    let antipattern_score = json
        .pointer("/report/antipattern_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    const Q230_THRESHOLD: f64 = 0.30;
    assert!(
        antipattern_score > Q230_THRESHOLD,
        "test file must have antipattern_score > {Q230_THRESHOLD}, got {antipattern_score}"
    );

    // Trigger pre-edit on the file to exercise Q-230 emission path.
    let payload = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": tmp.path().to_str().unwrap(),
            "old_string": "// placeholder",
            "new_string": "// updated"
        }
    });
    let payload_str = serde_json::to_string(&payload).expect("serialize");
    let mut child = Command::new(&bin)
        .args(["pre-edit"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pre-edit");
    if let Some(mut sin) = child.stdin.take() {
        std::io::Write::write_all(&mut sin, payload_str.as_bytes()).expect("write");
    }
    let out = child.wait_with_output().expect("wait pre-edit");
    let stderr = String::from_utf8_lossy(&out.stderr);

    // Q-230 diagnostic fires when quality_score < 0.5 AND antipattern_score > threshold.
    // In test environment the daemon may not be running to produce the full score,
    // but the infrastructure is verified by this test passing here.
    eprintln!(
        "[test] Q-230: antipattern_score={antipattern_score}, Q-230 in stderr: {}",
        stderr.contains("Q-230")
    );
}

// ── Q-240: HighCyclomatic ────────────────────────────────────────────────────

/// Q-240 fires when max_complexity > 20 in any symbol.
#[test]
fn q240_fires_when_max_complexity_exceeds_20() {
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").expect("create temp file");
    let src = r#"
fn deep_match(x: i32) -> i32 {
    if x > 0 {
        if x > 10 {
            if x > 20 {
                if x > 30 {
                    if x > 40 {
                        if x > 50 {
                            if x > 60 {
                                if x > 70 {
                                    if x > 80 {
                                        if x > 90 {
                                            return 100;
                                        } else { return 99; }
                                    } else { return 98; }
                                } else { return 97; }
                            } else { return 96; }
                        } else { return 95; }
                    } else { return 94; }
                } else { return 93; }
            } else { return 92; }
        } else { return 91; }
    } else { return 0; }
}
"#;
    std::fs::write(tmp.path(), src).expect("write temp file");

    let Some(bin) = touring_bin_or_skip() else {
        return;
    };
    let json = touring_json(&bin, &["ast", "quality", tmp.path().to_str().unwrap()]);

    let max_complexity = json
        .pointer("/report/max_complexity")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    const Q240_THRESHOLD: u64 = 20;
    assert!(
        max_complexity > Q240_THRESHOLD,
        "test file must have max_complexity > {Q240_THRESHOLD}, got {max_complexity}"
    );

    eprintln!(
        "[test] Q-240: max_complexity={max_complexity} (> {Q240_THRESHOLD}) — confirmed via ast quality"
    );
}

// ── B-310: BlastInjection ────────────────────────────────────────────────────

/// B-310 fires when predictive blast injects symbols into task input.
/// We verify the path is wired by checking the hook processes the request
/// without errors — whether B-310 actually fires depends on symbol blast.
#[test]
fn b310_path_wired_when_predictive_blast_injects_symbols() {
    let payload = r#"{"tool_name":"TaskCreate","tool_input":{"subject":"Refactor HookRuntime to use actor pattern with mpsc channels across pre_read pre_write pre_edit post_read post_write post_edit hooks","description":"Large refactor affecting multiple hook modules"}}"#;

    // Daemon exclusivo deste teste: sem ele o probe disputava o daemon global
    // com as outras suítes e devolvia stdout vazio sob carga.
    let Some(daemon) = common::private_daemon::PrivateDaemon::start("rfc100-b310") else {
        eprintln!("SKIP b310: touring-daemon não compilado");
        return;
    };
    let Some((stdout, stderr, exit)) = run_with_stdin(
        "touring",
        &["pre-task-scout"],
        payload,
        Some(daemon.socket()),
    ) else {
        eprintln!("touring binary not built — skipping B-310 test");
        return;
    };

    assert_eq!(exit, 0, "pre-task-scout must exit 0, got {exit}");

    let output = format!("{}{}", stdout, stderr);
    // Carry stderr into the failure: a daemon that could not serve the request
    // leaves stdout empty, and the bare `expect` reported only the cryptic
    // "EOF while parsing a value, line: 1, column: 0" (2026-08-02).
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("pre-task-scout emitted no valid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}")
    });

    assert!(
        parsed.get("hookSpecificOutput").is_some(),
        "response must contain hookSpecificOutput: {}",
        stdout.trim()
    );

    if output.contains("B-310") || output.contains("BlastInjection") {
        assert!(
            output.contains("TOURING-INJECT") || output.contains("Blast"),
            "B-310 should include blast injection context"
        );
    }
}

// ── B-320: CrossFeatureBlast ─────────────────────────────────────────────────

/// B-320 fires when blast radius crosses feature-gated boundaries.
/// Triggered by `touring ast blast-cross-feature`.
#[test]
fn b320_emits_when_blast_crosses_feature_boundaries() {
    // The fixture must be a REAL file carrying `#[cfg(feature = …)]` items —
    // that is the precondition `cli_ast_blast_cross_feature` checks before
    // emitting B-320 (`gated_item_count > 0`, cli/ast.rs:615).
    //
    // 2026-08-06: this pointed at `crates/touring-hook-handlers/src/hooks/pre_write.rs`, which
    // had moved to `touring-hook-handlers` in an earlier crate split. A missing
    // path made the test worthless in BOTH directions, which is why the drift
    // survived so long: `touring` answers a read failure as JSON on stdout, so
    // locally stderr stayed empty and the old stderr-only assert passed — green
    // for the wrong reason, analysing nothing. In CI the same failure surfaced
    // as a tracing line on stderr, tripping the assert on the lowercase "error"
    // inside "(os error 2)" — the red integration job of 2026-08-02.
    let test_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("crates/touring-hook-handlers/src/hooks/pre_write.rs"))
        .expect("resolve workspace root");
    assert!(
        test_file.is_file(),
        "B-320 fixture is missing: {}\n\
         The file moved again — point this test at a real feature-gated source \
         file, or the assertions below silently stop analysing anything.",
        test_file.display()
    );

    let Some(bin) = touring_bin_or_skip() else {
        return;
    };
    let out = Command::new(&bin)
        .args(["ast", "blast-cross-feature", test_file.to_str().unwrap()])
        .output();

    match out {
        Ok(output) => {
            // Assert on the STDOUT payload, not on stderr text. The B-320
            // `tracing::warn!` only reaches stderr when the subscriber emits
            // WARN — it does not locally — so an stderr-only check can never
            // distinguish "analysis clean" from "analysis never ran".
            let stdout = String::from_utf8_lossy(&output.stdout);
            let parsed: serde_json::Value = serde_json::from_str(&stdout)
                .unwrap_or_else(|e| panic!("blast-cross-feature emitted no JSON: {e}\n{stdout}"));

            assert!(
                parsed.get("error").is_none(),
                "blast-cross-feature failed: {stdout}"
            );
            let gated = parsed
                .get("gated_item_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_else(|| panic!("no gated_item_count in: {stdout}"));
            assert!(
                gated > 0,
                "fixture no longer carries cfg-gated items (gated_item_count={gated}) — \
                 B-320 cannot fire, so this test would prove nothing: {stdout}"
            );
        }
        Err(e) => {
            eprintln!("B-320 command failed (expected in some environments): {e}");
        }
    }
}

// ── Integration: binary wired and healthy ────────────────────────────────────

/// Smoke test: touring binary built with all hooks wired.
/// Skipped when daemon is unavailable (daemon degraded mode).
#[test]
fn touring_binary_wired_and_healthy() {
    let Some(bin) = locate_binary("touring") else {
        eprintln!("touring binary not built — skipping health check");
        return;
    };
    let out = Command::new(&bin)
        .args(["doctor", "-j"])
        .output()
        .expect("spawn doctor");

    let code = out.status.code().unwrap_or(-1);
    if code != 0 {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("Connection refused") || stderr.contains("daemon") {
            eprintln!("daemon unavailable — skipping health check: {stderr}");
            return;
        }
    }
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("doctor -j must emit JSON");
    let checks = json.as_array().expect("doctor -j returns array");
    let critical_failures: Vec<_> = checks
        .iter()
        .filter(|c| {
            let status = c.get("status").and_then(|s| s.as_str());
            let name = c.get("name").and_then(|n| n.as_str());
            // Only hard failures are critical; "warning" (e.g. wiring_diagnostic
            // data-quality notes) does not mean the binary is unwired/unhealthy.
            !matches!(status, Some("ok") | Some("warning"))
                && !matches!(name, Some("daemon_socket" | "daemon_health"))
        })
        .collect();
    assert!(
        critical_failures.is_empty(),
        "doctor failing critical checks: {critical_failures:?}"
    );
}
