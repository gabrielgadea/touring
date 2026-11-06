//! The standalone route of `touring-hook` is derived from the daemon's dispatch table
//! (02/11/2026) — proven through the real binary, spawned with the daemon skipped.
//!
//! Before, the standalone path was a hand-written `match` of ~47 arms beside the table: 29 names
//! in both, 18 hook events the daemon served that answered `unknown subcommand` with the daemon
//! down, and nothing that could say an arm and its table handler still agreed. Guards:
//!
//! 1. EQUIVALENCE — the 29 names that had an arm print byte-identical stdout/stderr and exit 0
//!    (`fixtures/standalone_parity.json`, captured from the pre-change binary; no username, the
//!    repository is public), except the explicitly justified policy changes below.
//! 2. REACHABILITY — every table hook that is not a CLI query endpoint answers standalone: exit 0,
//!    a standalone trace, never `unknown subcommand`, never a panic. Iterates the LIVE table, so a
//!    hook added tomorrow is exercised with no edit here (a generic payload when the fixture has
//!    none).
//! 3. THE 18 — the hooks that had no arm answer structurally: silence or a JSON object whose
//!    top-level keys Claude Code admits.
//! 4. REFUSALS — a CLI query endpoint and an unknown name fail open, naming the reason.

use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use touring_hook_runtime::hook_response::TOP_LEVEL_FIELDS;
use touring_hooks::hook_registry::{all_daemon_hook_names, is_rpc_endpoint};

const FIXTURE: &str = include_str!("fixtures/standalone_parity.json");

/// Intentional changes from the pre-change binary. C2.3 makes destructive git
/// push a denial instead of the warning-free output captured by this fixture.
const JUSTIFIED_DIFFERENCES: &[&str] = &["pre-bash#git-force"];

struct Ran {
    exit: i32,
    stdout: String,
    stderr: String,
    trace: Value,
}

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("fixtures/standalone_parity.json is JSON")
}

/// A temp project: marked as a root (`.touring/`), one source file, nothing else — so the
/// output depends on the binary and the payload and nothing around them.
fn project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp project");
    std::fs::create_dir_all(tmp.path().join(".touring")).expect("project marker");
    std::fs::create_dir_all(tmp.path().join("src")).expect("src dir");
    std::fs::write(tmp.path().join("src/lib.rs"), "pub fn demo() {}\n").expect("source file");
    tmp
}

/// Run `touring-hook <subcommand>` standalone: cleared environment, daemon skipped, `HOME` and the
/// project root both the temp project, the hook trace on. `<PROJECT>` in the payload and in what
/// comes back stands for the project's path.
fn run_hook(subcommand: &str, payload: &Value) -> Ran {
    let proj = project();
    let root = proj.path().to_string_lossy().into_owned();
    let trace_file = proj.path().join("hook_trace.jsonl");
    let body = payload.to_string().replace("<PROJECT>", &root);
    let mut child = Command::new(env!("CARGO_BIN_EXE_touring-hook"))
        .arg(subcommand)
        .current_dir(proj.path())
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", proj.path())
        .env("CLAUDE_PROJECT_DIR", proj.path())
        .env("TOURING_NO_DAEMON", "1")
        .env("TOURING_HOOK_TRACE_FILE", &trace_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn touring-hook");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(body.as_bytes())
        .expect("write payload");
    let out = child.wait_with_output().expect("wait touring-hook");
    Ran {
        exit: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).replace(&root, "<PROJECT>"),
        stderr: String::from_utf8_lossy(&out.stderr).replace(&root, "<PROJECT>"),
        trace: last_trace_line(&trace_file),
    }
}

fn last_trace_line(path: &Path) -> Value {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("trace line is JSON"))
        .unwrap_or(Value::Null)
}

/// A payload for a table hook that has no entry in the fixture: the fields every hook input has.
fn generic_payload(name: &str) -> Value {
    serde_json::json!({
        "session_id": "parity-smoke",
        "hook_event_name": name,
        "cwd": "<PROJECT>",
    })
}

/// The one exception to "a hook never blocks": an event that keeps the agent working ONLY
/// through exit 2 (TeammateIdle, TaskCompleted — code.claude.com/docs/en/hooks, "Decision
/// control") when the handler answered a `decision: "block"`. Then the contract demands exit 2,
/// nothing on stdout, and the reason — and only the reason — on stderr (#4, 04/11/2026).
fn is_exit_code_block(payload: &Value, ran: &Ran) -> bool {
    let event = payload["hook_event_name"].as_str().unwrap_or_default();
    if ran.exit != 2
        || !touring_hook_runtime::hook_response::EXIT_CODE_ONLY_BLOCK_EVENTS.contains(&event)
    {
        return false;
    }
    assert!(
        ran.stdout.trim().is_empty(),
        "{event}: exit 2 with stdout: {}",
        ran.stdout
    );
    assert!(
        !ran.stderr.trim().is_empty(),
        "{event}: exit 2 without a reason"
    );
    assert!(
        !ran.stderr.contains("hookSpecificOutput"),
        "{event}: the reason must reach the agent alone, not after funnel diagnostics: {}",
        ran.stderr
    );
    assert_eq!(ran.trace["exit_reason"], "exit-code-block", "{}", ran.trace);
    true
}

/// The 13 overrides converged on 04/11/2026 (aprofundamento #4): their standalone answer is the
/// daemon's handler now, so the pre-change bytes are no longer the oracle — the output contract
/// is. The rest of the fixture still pins byte-identical output.
const CONVERGED_2026_11_04: &[&str] = &[
    "cli-suggest",
    "post-tool-rl",
    "session-stop",
    "task-created",
    "task-validation",
    "subagent-stop",
    "post-tool-failure",
    "subagent-start",
    "post-compact",
    "file-changed",
    "cwd-changed",
    "instructions-loaded",
    "worktree-create",
];

/// A converged hook answers like the daemon does: exit 0, a standalone trace, and stdout that is
/// silence or a JSON object whose top-level keys Claude Code admits.
fn assert_converged_answer(key: &str, ran: &Ran) {
    assert_eq!(ran.exit, 0, "{key}: exit (stderr: {})", ran.stderr);
    assert_eq!(ran.trace["route"], "standalone", "{key}: {}", ran.trace);
    let out = ran.stdout.trim();
    if out.is_empty() || out == "{}" {
        return;
    }
    let value: Value = serde_json::from_str(out)
        .unwrap_or_else(|e| panic!("{key}: stdout is not JSON ({e}): {out}"));
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{key}: stdout is not a JSON object: {out}"));
    for field in object.keys() {
        assert!(
            TOP_LEVEL_FIELDS.contains(&field.as_str()),
            "{key}: top-level key `{field}` is outside Claude Code's output contract: {out}"
        );
    }
}

/// Guard 1. The table-derived route prints what the hand-written arms printed, byte for byte, for
/// every name that had an arm — through the payloads captured from the old binary. Several
/// entries are second payloads (`name#variant`) chosen because they make the hook SAY something:
/// an allow proves little about the funnel the answer leaves through.
#[test]
fn standalone_output_is_what_the_hand_written_arms_printed() {
    let fx = fixture();
    let entries = fx["equivalence"].as_object().expect("equivalence object");
    assert!(
        entries.len() >= 29,
        "the fixture must cover all 29 names that had an arm, has {}",
        entries.len()
    );
    let mut with_output = 0;
    let mut justified_seen = 0;
    for (key, entry) in entries {
        let name = key.split('#').next().expect("name");
        let ran = run_hook(name, &entry["payload"]);
        if CONVERGED_2026_11_04.contains(&name) {
            assert_converged_answer(key, &ran);
            if !ran.stdout.trim().is_empty() {
                with_output += 1;
            }
            continue;
        }
        let want_out = entry["stdout"].as_str().expect("stdout");
        let want_err = entry["stderr"].as_str().expect("stderr");
        assert_eq!(
            ran.exit,
            entry["exit"].as_i64().expect("exit") as i32,
            "{key}: exit"
        );
        if JUSTIFIED_DIFFERENCES.contains(&key.as_str()) {
            justified_seen += 1;
            assert_ne!(
                ran.stdout, want_out,
                "{key}: the justified change did not fire"
            );
            assert!(
                ran.stdout.contains("git push is destructive"),
                "{key}: unexpected new stdout: {}",
                ran.stdout
            );
        } else {
            assert_eq!(
                ran.stdout, want_out,
                "{key}: stdout differs from the pre-change binary"
            );
        }
        assert_eq!(
            ran.stderr, want_err,
            "{key}: stderr differs from the pre-change binary"
        );
        assert_eq!(ran.trace["route"], "standalone", "{key}: {}", ran.trace);
        if !want_out.is_empty() {
            with_output += 1;
        }
    }
    assert_eq!(
        justified_seen,
        JUSTIFIED_DIFFERENCES.len(),
        "every justified difference must be exercised"
    );
    assert!(
        with_output >= 8,
        "only {with_output} entries print something — the equivalence would prove little about the funnel"
    );
}

/// Guard 2. EVERY table hook that is not a CLI query endpoint answers standalone without a crash
/// or an `unknown subcommand`. Iterates the live table: a hook added to `dispatch_table()` is
/// reachable in both modes and this exercises it with no edit here.
#[test]
fn every_table_hook_is_reachable_standalone_and_none_panics() {
    let fx = fixture();
    let mut payloads: BTreeMap<String, Value> = BTreeMap::new();
    for (key, entry) in fx["equivalence"].as_object().expect("equivalence") {
        payloads
            .entry(key.split('#').next().expect("name").to_string())
            .or_insert_with(|| entry["payload"].clone());
    }
    for (name, entry) in fx["newly_reachable"].as_object().expect("newly_reachable") {
        payloads.insert(name.clone(), entry["payload"].clone());
    }
    let hooks: Vec<&str> = all_daemon_hook_names()
        .into_iter()
        .filter(|n| !is_rpc_endpoint(n))
        .collect();
    assert!(hooks.len() >= 47, "the table lost hooks: {}", hooks.len());
    for name in hooks {
        let payload = payloads
            .get(name)
            .cloned()
            .unwrap_or_else(|| generic_payload(name));
        let ran = run_hook(name, &payload);
        if is_exit_code_block(&payload, &ran) {
            continue;
        }
        assert_eq!(
            ran.exit, 0,
            "{name}: a hook never blocks Claude Code ({})",
            ran.stderr
        );
        assert!(
            !ran.stderr.contains("unknown subcommand") && !ran.stderr.contains("panicked"),
            "{name} is a table hook and must answer standalone: {}",
            ran.stderr
        );
        assert_eq!(ran.trace["route"], "standalone", "{name}: {}", ran.trace);
        let reason = ran.trace["exit_reason"].as_str().unwrap_or_default();
        assert!(
            !matches!(
                reason,
                "standalone-unknown" | "standalone-panic" | "standalone-daemon-only"
            ),
            "{name}: exit_reason {reason}"
        );
    }
}

/// Guard 3. The 18 table hooks that had no arm now answer — and what they print is silence or a
/// JSON object Claude Code's output contract admits (prose is turned into `{}`, as on the daemon
/// route, with the cause named on stderr).
#[test]
fn the_hooks_that_had_no_arm_now_answer_within_the_output_contract() {
    let fx = fixture();
    let newly = fx["newly_reachable"].as_object().expect("newly_reachable");
    assert_eq!(newly.len(), 18, "the 18 hooks that had no standalone arm");
    for (name, entry) in newly {
        let ran = run_hook(name, &entry["payload"]);
        if is_exit_code_block(&entry["payload"], &ran) {
            continue;
        }
        assert_eq!(ran.exit, 0, "{name}: {}", ran.stderr);
        assert!(
            !ran.stderr.contains("unknown subcommand"),
            "{name}: {}",
            ran.stderr
        );
        let out = ran.stdout.trim();
        if out.is_empty() {
            continue;
        }
        let answer: Value = serde_json::from_str(out)
            .unwrap_or_else(|e| panic!("{name}: stdout is not JSON ({e}): {out}"));
        let object = answer
            .as_object()
            .unwrap_or_else(|| panic!("{name}: stdout is not a JSON object: {out}"));
        for key in object.keys() {
            assert!(
                TOP_LEVEL_FIELDS.contains(&key.as_str()),
                "{name}: top-level key `{key}` is outside Claude Code's output contract: {out}"
            );
        }
    }
}

/// Guard 4a. A table key that answers the `touring` CLI — not a hook event — is refused with the
/// reason named, and fails open (exit 0, nothing on stdout).
#[test]
fn a_cli_query_endpoint_is_refused_with_the_reason_named() {
    let ran = run_hook("cli-doctor", &generic_payload("cli-doctor"));
    assert_eq!(ran.exit, 0, "{}", ran.stderr);
    assert!(ran.stdout.is_empty(), "{}", ran.stdout);
    assert!(
        ran.stderr.contains("daemon query endpoint") && ran.stderr.contains("no standalone mode"),
        "the refusal must name why: {}",
        ran.stderr
    );
    assert_eq!(
        ran.trace["exit_reason"], "standalone-daemon-only",
        "{}",
        ran.trace
    );
}

/// Guard 4b. A name nobody declared is still answered as before: exit 0, `unknown subcommand` on
/// stderr, the dedicated trace reason.
#[test]
fn an_unknown_subcommand_is_still_answered_with_exit_zero() {
    let ran = run_hook("no-such-hook", &generic_payload("no-such-hook"));
    assert_eq!(ran.exit, 0);
    assert!(ran.stdout.is_empty());
    assert!(
        ran.stderr.contains("unknown subcommand: no-such-hook"),
        "{}",
        ran.stderr
    );
    assert_eq!(
        ran.trace["exit_reason"], "standalone-unknown",
        "{}",
        ran.trace
    );
}
