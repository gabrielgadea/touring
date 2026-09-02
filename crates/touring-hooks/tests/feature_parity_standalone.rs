//! F0.3c guard (2026-09-01) — the standalone fallback of `touring-hook` must be
//! ALIVE for every hook the daemon fast-path declares.
//!
//! Origin: live PostToolUse(Bash) events lost 42/47 mirror deliveries in one
//! session. Both the deployed toolchain AND the dev binary answered
//! `unknown subcommand: post-bash` when the daemon path was skipped
//! (`TOURING_NO_DAEMON=1`): the standalone `match` arms in `main.rs` are gated
//! on THIS crate's `pre-hooks`/`post-hooks` features, while the façade's
//! `all-hooks` only forwarded to `touring-dispatch/all-hooks` — so every arm
//! was compiled out and the daemon became a silent single point of failure.
//!
//! Two layers, D8-cruzado style: the DECLARATION (cfg) and the EXECUTOR (the
//! real binary, spawned) must agree.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// The default build of this crate must compile the standalone hook arms.
#[test]
fn default_build_enables_standalone_hook_arms() {
    assert!(
        cfg!(feature = "pre-hooks"),
        "feature `pre-hooks` is OFF in the default build — every standalone pre-hook arm \
         in main.rs is compiled out (all-hooks must enable the crate's own features)"
    );
    assert!(
        cfg!(feature = "post-hooks"),
        "feature `post-hooks` is OFF in the default build — `touring-hook post-bash` \
         without a daemon answers `unknown subcommand` (F0.3c)"
    );
}

fn run_hook(
    subcommand: &str,
    stdin_payload: Option<&str>,
    home: &Path,
    trace_file: &Path,
) -> (String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_touring-hook"));
    cmd.arg(subcommand)
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("CLAUDE_PROJECT_DIR", home)
        .env("TOURING_NO_DAEMON", "1")
        .env("TOURING_HOOK_TRACE_FILE", trace_file)
        .stdin(if stdin_payload.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn touring-hook");
    if let Some(payload) = stdin_payload {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(payload.as_bytes()).expect("write payload");
        drop(stdin);
    }
    let out = child.wait_with_output().expect("wait touring-hook");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn read_trace_lines(trace_file: &Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(trace_file).unwrap_or_default();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("trace line is JSON"))
        .collect()
}

/// EXECUTOR guard: the real binary, daemon skipped, must run `post-bash`
/// standalone — and the F0.3a trace must say so in one line.
#[test]
fn standalone_post_bash_is_alive_and_traced() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let trace = tmp.path().join("hook_trace.jsonl");
    let payload = r#"{"session_id":"f03c-guard","tool_name":"Bash","tool_input":{"command":"touring index find F03cGuard -j"},"tool_response":{"stdout":"","stderr":""}}"#;

    let (_stdout, stderr) = run_hook("post-bash", Some(payload), tmp.path(), &trace);

    assert!(
        !stderr.contains("unknown subcommand"),
        "standalone post-bash is DEAD (stderr: {stderr:?}) — the daemon is a silent single point of failure"
    );

    let lines = read_trace_lines(&trace);
    assert_eq!(lines.len(), 1, "exactly one trace line per invocation, got {lines:?}");
    let t = &lines[0];
    assert_eq!(t["hook"], "post-bash");
    assert_eq!(t["route"], "standalone", "daemon skipped ⇒ route must be standalone: {t}");
    assert_eq!(t["stdin_state"], "ok", "payload was delivered whole: {t}");
    assert!(t["stdin_bytes"].as_u64().unwrap_or(0) as usize == payload.len(), "{t}");
    assert_ne!(t["exit_reason"], "standalone-unknown", "{t}");
    assert_eq!(t["session_id"], "f03c-guard", "session id travels for attribution: {t}");
    assert!(t["elapsed_ms"].is_u64() && t["pid"].is_u64() && t["ppid"].is_u64(), "{t}");
}

/// A stateless subcommand (no stdin, exits before any runtime) still leaves
/// its single trace line — the atexit path covers EVERY exit.
#[test]
fn stateless_help_is_traced_with_empty_stdin() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let trace = tmp.path().join("hook_trace.jsonl");

    let (stdout, _stderr) = run_hook("help", None, tmp.path(), &trace);
    assert!(!stdout.is_empty(), "help prints usage");

    let lines = read_trace_lines(&trace);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["hook"], "help");
    assert_eq!(lines[0]["route"], "stateless");
}

/// Without the env var the binary must not create any trace file (opt-in).
#[test]
fn no_trace_file_without_env() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let never = tmp.path().join("never.jsonl");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_touring-hook"));
    let out = cmd
        .arg("help")
        .current_dir(tmp.path())
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", tmp.path())
        .stdin(Stdio::null())
        .output()
        .expect("spawn");
    assert!(out.status.success());
    assert!(!never.exists());
    let created: Vec<_> = std::fs::read_dir(tmp.path())
        .expect("read tmp")
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .collect();
    assert!(created.is_empty(), "trace is opt-in: {created:?}");
}
