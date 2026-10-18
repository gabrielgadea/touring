//! C4 (2026-10-17) — a GUARD that cannot read its stdin fails CLOSED.
//!
//! Until C4 `read_stdin` answered `{}` for an empty, timed-out or broken stdin, and a hook that
//! can deny ran on it: `pre-bash`, `pre-edit-prevention` and `pre-tool-use` saw an empty
//! command and ALLOWED (measured 17/10/2026: rc=0, empty stdout). These tests drive the REAL
//! `touring-hook` binary, daemon skipped, with a payload the hook cannot read, and require:
//!
//! - every guard denies, naming the cause and the way out;
//! - an advisory fed the SAME bytes keeps failing open (the control);
//! - a payload that was whole but never got its EOF is USED, not denied (F0.3b);
//! - zero bytes is allowed, counted;
//! - every state lands in the hook trace, where the fraction is measured.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use touring_hook_runtime::hook_runtime_ext::MAX_STDIN_BYTES;
use touring_hooks::hook_registry::GUARD_HOOK_NAMES;

/// What the hook did: raw stdout, exit code and the single trace line it left behind.
struct Ran {
    stdout: String,
    code: Option<i32>,
    trace: serde_json::Value,
}

/// How the test ends the child's stdin.
#[derive(Clone, Copy)]
enum Stdin {
    /// Write the bytes, then close (clean EOF).
    Closed,
    /// Write the bytes and keep the pipe open until the child has exited (no EOF ever).
    Open,
}

fn run_hook_env(
    subcommand: &str,
    bytes: Vec<u8>,
    stdin: Stdin,
    home: &Path,
    extra_env: &[(&str, &str)],
) -> Ran {
    let trace_path = home.join("trace.jsonl");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_touring-hook"));
    cmd.arg(subcommand)
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("CLAUDE_PROJECT_DIR", home)
        .env("TOURING_NO_DAEMON", "1")
        .env("TOURING_HOOK_TRACE_FILE", &trace_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("spawn touring-hook");
    let mut pipe = child.stdin.take().expect("stdin");
    // A writer thread: an over-cap payload outlives the reader (it stops at the cap), so the
    // write may end in EPIPE — that is the point, not an error.
    let writer = std::thread::spawn(move || {
        let _ = pipe.write_all(&bytes);
        let _ = pipe.flush();
        match stdin {
            Stdin::Closed => drop(pipe),
            Stdin::Open => return Some(pipe),
        }
        None
    });
    let out = child.wait_with_output().expect("wait touring-hook");
    // Only now may an `Open` pipe close.
    drop(writer.join().expect("writer thread"));
    let trace = std::fs::read_to_string(&trace_path)
        .ok()
        .and_then(|b| b.lines().last().map(str::to_owned))
        .and_then(|l| serde_json::from_str(&l).ok())
        .unwrap_or(serde_json::Value::Null);
    Ran {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        code: out.status.code(),
        trace,
    }
}

fn run_hook(subcommand: &str, bytes: &[u8], stdin: Stdin, home: &Path) -> Ran {
    run_hook_env(subcommand, bytes.to_vec(), stdin, home, &[])
}

/// The permission decision the hook printed, if it spoke at all.
fn decision(ran: &Ran) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(ran.stdout.trim()).ok()?;
    let hso = v.get("hookSpecificOutput")?;
    Some((
        hso.get("permissionDecision")?.as_str()?.to_string(),
        hso.get("permissionDecisionReason")
            .and_then(|r| r.as_str())
            .unwrap_or_default()
            .to_string(),
    ))
}

fn assert_c4_deny(ran: &Ran, hook: &str, state: &str) {
    assert_eq!(ran.code, Some(0), "a deny is exit 0: {}", ran.stdout);
    let (verdict, reason) = decision(ran)
        .unwrap_or_else(|| panic!("{hook}: the guard stayed silent: {:?}", ran.stdout));
    assert_eq!(verdict, "deny", "{hook}: {reason}");
    assert!(
        reason.contains(&format!("[entrada-ilegível] {hook}")),
        "{hook}: the reason must name the cause and the hook: {reason}"
    );
    assert!(reason.contains("nada foi inspecionado"), "{reason}");
    assert!(reason.contains("repita"), "the way out: {reason}");
    let v: serde_json::Value = serde_json::from_str(ran.stdout.trim()).expect("json");
    assert_eq!(
        v["hookSpecificOutput"]["hookEventName"], "PreToolUse",
        "the deny speaks as the invoking event"
    );
    assert_eq!(ran.trace["stdin_state"], state, "trace: {}", ran.trace);
    assert_eq!(ran.trace["exit_reason"], "stdin-unreadable-deny");
}

fn is_c4_deny(ran: &Ran) -> bool {
    decision(ran).is_some_and(|(_, reason)| reason.contains("[entrada-ilegível]"))
}

const WHOLE: &str = r#"{"session_id":"c4","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"echo c4"}}"#;

/// The truncated payload of the defect: a destructive command cut mid-string, no EOF.
const TRUNCATED: &str = r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"rm -rf /home/gabri"#;

#[test]
fn guard_denies_a_truncated_payload_that_never_gets_its_eof() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ran = run_hook("pre-bash", TRUNCATED.as_bytes(), Stdin::Open, tmp.path());
    assert_c4_deny(&ran, "pre-bash", "timeout");
    assert_eq!(ran.trace["stdin_bytes"], TRUNCATED.len() as u64);
}

#[test]
fn every_guard_denies_stdin_that_is_not_json() {
    assert!(GUARD_HOOK_NAMES.len() >= 6, "the guard set is not empty");
    for hook in GUARD_HOOK_NAMES {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ran = run_hook(hook, b"this is not json", Stdin::Closed, tmp.path());
        assert_c4_deny(&ran, hook, "parse-error");
    }
}

#[test]
fn every_guard_denies_a_truncated_document_that_does_end() {
    for hook in GUARD_HOOK_NAMES {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ran = run_hook(hook, TRUNCATED.as_bytes(), Stdin::Closed, tmp.path());
        assert_c4_deny(&ran, hook, "parse-error");
    }
}

#[test]
fn guard_denies_a_payload_over_the_byte_cap() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut huge = vec![b'a'; MAX_STDIN_BYTES + 4096];
    huge[0] = b'{';
    let ran = run_hook_env("pre-write", huge, Stdin::Closed, tmp.path(), &[]);
    assert_c4_deny(&ran, "pre-write", "too-large");
    assert_eq!(ran.trace["stdin_bytes"], MAX_STDIN_BYTES as u64);
    let (_, reason) = decision(&ran).expect("deny");
    assert!(reason.contains("teto de 16 MiB"), "{reason}");
}

/// The F0.3b shape, now for a guard: the payload is whole but the EOF never comes. It is
/// USED (state `ok-recovered`), not denied — the guard still inspects the real command.
#[test]
fn guard_uses_a_whole_payload_even_without_eof() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ran = run_hook("pre-bash", WHOLE.as_bytes(), Stdin::Open, tmp.path());
    assert!(
        !is_c4_deny(&ran),
        "a whole payload must be inspected: {}",
        ran.stdout
    );
    assert_eq!(ran.code, Some(0));
    assert_eq!(
        ran.trace["stdin_state"], "ok-recovered",
        "trace: {}",
        ran.trace
    );
    assert_ne!(ran.trace["exit_reason"], "stdin-unreadable-deny");
}

/// (c) Zero bytes with a clean EOF is allowed — and counted as its own state.
#[test]
fn guard_allows_empty_stdin_but_counts_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ran = run_hook("pre-bash", b"", Stdin::Closed, tmp.path());
    assert!(!is_c4_deny(&ran), "{}", ran.stdout);
    assert_eq!(ran.code, Some(0));
    assert_eq!(ran.trace["stdin_state"], "empty", "trace: {}", ran.trace);
}

/// A healthy read is untouched: state `ok`, no C4 deny.
#[test]
fn guard_with_a_normal_payload_is_unaffected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ran = run_hook("pre-bash", WHOLE.as_bytes(), Stdin::Closed, tmp.path());
    assert!(!is_c4_deny(&ran), "{}", ran.stdout);
    assert_eq!(ran.trace["stdin_state"], "ok");
}

/// CONTROL: an advisory fed the very bytes that make a guard deny keeps failing open.
#[test]
fn advisory_with_the_same_unreadable_stdin_still_fails_open() {
    for hook in ["pre-grep", "pre-glob", "cli-suggest", "post-bash"] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ran = run_hook(hook, TRUNCATED.as_bytes(), Stdin::Closed, tmp.path());
        assert!(!is_c4_deny(&ran), "{hook} is an advisory: {}", ran.stdout);
        assert_eq!(ran.code, Some(0), "{hook} fails open with exit 0");
        assert_eq!(
            ran.trace["stdin_state"], "parse-error",
            "{hook}: {}",
            ran.trace
        );
        assert_ne!(ran.trace["exit_reason"], "stdin-unreadable-deny");
    }
    // and the timeout flavour of the same bytes
    let tmp = tempfile::tempdir().expect("tempdir");
    let ran = run_hook("pre-grep", TRUNCATED.as_bytes(), Stdin::Open, tmp.path());
    assert!(!is_c4_deny(&ran), "{}", ran.stdout);
    assert_eq!(ran.trace["stdin_state"], "timeout");
}

/// The human switch restores the old fail-open for guards (a persistently broken pipe must not
/// brick the session) — and only for exactly `1`.
#[test]
fn the_human_switch_restores_fail_open_for_guards() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let on = run_hook_env(
        "pre-bash",
        b"this is not json".to_vec(),
        Stdin::Closed,
        tmp.path(),
        &[("TOURING_HOOK_STDIN_GUARD_DISABLED", "1")],
    );
    assert!(!is_c4_deny(&on), "{}", on.stdout);
    assert_eq!(on.trace["stdin_state"], "parse-error", "still counted");
    let tmp = tempfile::tempdir().expect("tempdir");
    let junk = run_hook_env(
        "pre-bash",
        b"this is not json".to_vec(),
        Stdin::Closed,
        tmp.path(),
        &[("TOURING_HOOK_STDIN_GUARD_DISABLED", "yes")],
    );
    assert_c4_deny(&junk, "pre-bash", "parse-error");
}
