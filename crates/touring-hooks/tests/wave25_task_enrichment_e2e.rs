//! Wave 25 (2026-04-18) — E2E tests for `cli_pre_task_scout` task-mode
//! enrichment. Wave 24 wired `PreToolUse:TaskCreate|...` to this handler
//! but the handler returned empty `additionalContext` for any payload
//! lacking `file_path`, which is EVERY Task* and EnterPlanMode invocation.
//!
//! This test pins the new behaviour: Task* / EnterPlanMode payloads carry
//! `subject` / `description` / `prompt`, and the handler now surfaces a
//! compact hook-context line from three signals:
//!
//!   1. memory recall on the subject (top-2 matching keys)
//!   2. decompose `ready_count` (pending subtasks w/ deps completed)
//!   3. wiring orphan headline (only for TaskCreate / EnterPlanMode —
//!      the tools that spawn new work, REGRA #0 potencializar)
//!
//! The tests drive the real compiled `touring` binary so they cover the
//! full wire path: CLI arg parse → daemon_query → hook_registry dispatch
//! → `cli_pre_task_scout` → in-process helper calls → JSON envelope.
//!
//! # Hermeticity (10/10/2026)
//!
//! The two ENRICHMENT tests (1 and 2) used to assert `!context.is_empty()` and so
//! relied on the three signals existing in the AMBIENT state of whatever project
//! the test happened to run in: green in the main tree (thousands of memories, 8k
//! orphans), red in a fresh worktree (nothing to recall). The daemon resolves the
//! project from the CLIENT's cwd — `TOURING_PROJECT_ROOT` on its spawn is only the
//! default — and `scout()` inherited cargo's cwd, so the verdict depended on where
//! the file lived, not on the code.
//!
//! Each of them now builds its own world ([`Hermetic`]): a temporary project root
//! and a temporary `HOME` (recall is federated through `$HOME/.claude`), one private
//! daemon that sees only those, and clients pinned to the same root. The proof has
//! two halves, and the first is what makes the second mean anything:
//!
//!   * CONTROL — the empty root yields NO context at all, so nothing ambient can
//!     leak in and satisfy the assertion by accident;
//!   * SEED — after storing one memory whose text IS the subject, the context
//!     names that key.
//!
//! Tests 3-6 assert emptiness / absence / envelope shape and hold wherever they
//! run; they keep using the shared workspace daemon.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// 21/08/2026: every `touring` this file spawns talks to a daemon PRIVATE to this
// test process (shared helper; see its header for why).
#[path = "common/private_daemon.rs"]
#[allow(dead_code)]
mod private_daemon;
use private_daemon::{PrivateDaemon, ProjectPin, private_daemon_env};

/// Locate the `touring` release / debug binary, skipping if absent.
///
/// Delegates to the shared helper (10/10/2026): this file kept its own copy that
/// looked ONLY under `<workspace>/target`, ignoring `CARGO_TARGET_DIR` and the
/// root that built this test — so in a worktree with its own target dir every
/// test here took the silent SKIP path and "passed" without running anything.
fn touring_binary() -> Option<PathBuf> {
    private_daemon::locate_binary("touring")
}

/// Run `touring pre-task-scout` with the given hook payload, return the
/// parsed `hookSpecificOutput` envelope or `None` if the binary is not
/// built / the daemon is unavailable (these tests are advisory when
/// infrastructure is missing — CI-safe).
fn scout(payload: &serde_json::Value) -> Option<serde_json::Value> {
    let bin = touring_binary()?;
    let mut child = Command::new(&bin)
        .envs(private_daemon_env())
        .arg("pre-task-scout")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn touring");
    if let Some(mut sin) = child.stdin.take() {
        let _ = sin.write_all(payload.to_string().as_bytes());
    }
    let out = child.wait_with_output().expect("wait touring");
    if !out.status.success() {
        // stderr often contains "Connection refused" / "Daemon returned
        // success=false" under cold-start races. Treat as infrastructure
        // gap, not a test failure.
        let err = String::from_utf8_lossy(&out.stderr);
        eprintln!("touring pre-task-scout failed ({}): {}", out.status, err);
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    parsed.get("hookSpecificOutput").cloned()
}

/// Pull the additionalContext string out of a parsed envelope.
fn context_of(envelope: &serde_json::Value) -> String {
    envelope
        .get("additionalContext")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// One test's private world: a temporary project root, a temporary `HOME` and a
/// daemon that sees only those two.
///
/// `daemon` is declared BEFORE `_dir` on purpose: fields drop in declaration
/// order, so the process dies (by the PID this test created — REGRA #19) before
/// the directory it is serving is removed.
struct Hermetic {
    daemon: PrivateDaemon,
    _dir: tempfile::TempDir,
}

impl Hermetic {
    /// `None` when the `touring` / `touring-daemon` binaries are not built (CI
    /// without a workspace build): a SKIP, announced on stderr — never a pass
    /// that looks like a proof. Every hermetic test prints `E2EHERM-RAN` once its
    /// assertions have executed; a run without that line tested nothing.
    fn start(tag: &str) -> Option<Self> {
        let dir = tempfile::Builder::new()
            .prefix("touring-e2eherm-")
            .tempdir()
            .expect("create the hermetic temp dir");
        let root = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&root).expect("create the hermetic project root");
        std::fs::create_dir_all(&home).expect("create the hermetic HOME");
        let Some(daemon) = PrivateDaemon::start_isolated(tag, ProjectPin { root, home }, Some(90))
        else {
            eprintln!(
                "E2EHERM-SKIP {tag}: touring / touring-daemon not built, or the daemon was not \
                 ready — nothing was tested"
            );
            return None;
        };
        Some(Self { daemon, _dir: dir })
    }

    /// Run `touring <args>` pinned to this world, feeding `stdin`. FAILS LOUDLY on a
    /// non-zero exit: inside a world that is entirely ours a failing client is a
    /// defect, not the "infrastructure gap" the shared-daemon tests tolerate.
    fn run(&self, args: &[&str], stdin: &str) -> String {
        let mut cmd = self
            .daemon
            .client_command("touring")
            .expect("`touring` binary vanished after the daemon started");
        // No `--timeout`: the client already waits 120s by default, which covers the
        // first call's cold start (a LEADING `--timeout N` breaks clap-routed
        // commands — `touring --timeout 120 memory store …` is `unrecognized
        // subcommand '120'` — and a trailing one is `unexpected argument`).
        let mut child = cmd
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn touring");
        if let Some(mut sin) = child.stdin.take() {
            sin.write_all(stdin.as_bytes()).expect("write payload");
        }
        let out = child.wait_with_output().expect("wait touring");
        assert!(
            out.status.success(),
            "`touring {}` failed ({}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// `touring pre-task-scout` for `payload`: the `additionalContext` it produced.
    fn scout_context(&self, payload: &serde_json::Value) -> String {
        let stdout = self.run(&["pre-task-scout"], &payload.to_string());
        let parsed: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("pre-task-scout stdout is JSON");
        let envelope = parsed
            .get("hookSpecificOutput")
            .expect("pre-task-scout envelope has hookSpecificOutput");
        assert_eq!(
            envelope.get("hookEventName").and_then(|v| v.as_str()),
            Some("PreToolUse"),
            "envelope must declare hookEventName=PreToolUse"
        );
        context_of(envelope)
    }

    /// Store one memory in THIS world's project (the socket, the cwd and the
    /// project env all point here, so it cannot land anywhere else).
    fn seed_memory(&self, key: &str, text: &str) {
        let stdout = self.run(&["memory", "store", key, text, "--tag", "#kind:lesson"], "");
        let parsed: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("memory store stdout is JSON");
        assert_eq!(
            parsed.get("status").and_then(|v| v.as_str()),
            Some("stored"),
            "seeding the memory must succeed, got: {stdout}"
        );
    }
}

/// The CONTROL + SEED sequence both enrichment tests share.
///
/// Returns `(context_before_seed, context_after_seed)` for the same payload, the
/// same root and the same daemon — the seed is the only thing that changed.
/// `seed_text` is what the recall must match: the subject the handler extracts
/// from `payload` (FTS5 ANDs every query term, so the stored text carries them all).
fn control_then_seed(
    world: &Hermetic,
    payload: &serde_json::Value,
    seed_key: &str,
    seed_text: &str,
) -> (String, String) {
    let before = world.scout_context(payload);
    world.seed_memory(seed_key, seed_text);
    let after = world.scout_context(payload);
    (before, after)
}

/// Test 1 — TaskCreate with a real-seeming subject: the handler must surface the
/// memory the subject matches. The control (empty root → empty context) is what
/// proves the hint came from the SEED and not from whatever state the test runs in.
#[test]
fn task_create_payload_returns_enriched_context() {
    let Some(world) = Hermetic::start("task-create") else {
        return;
    };
    let seed_key = "e2eherm-seed-task-create";
    let payload = serde_json::json!({
        "tool_name": "TaskCreate",
        "tool_input": {
            "subject": "refactor wiring orphans",
            "description": "reduce orphan pub symbols"
        }
    });

    let (before, after) = control_then_seed(&world, &payload, seed_key, "refactor wiring orphans");

    assert_eq!(
        before, "",
        "CONTROL: an empty project must yield no hint at all (anything here is ambient state \
         leaking into the test); got: {before}"
    );
    assert!(
        after.contains(&format!("memory: {seed_key}")),
        "TaskCreate must surface the seeded memory `{seed_key}` in its hint; got: {after}"
    );
    eprintln!("E2EHERM-RAN task_create: control={before:?} seeded={after:?}");
}

/// Test 2 — EnterPlanMode carries its intent in `prompt`, not `subject`.
/// The extractor must cover `prompt` so EnterPlanMode also benefits: with the
/// extractor broken the subject is `None`, the handler short-circuits to an empty
/// context and the seeded memory never surfaces — this test then fails.
#[test]
fn enter_plan_mode_prompt_is_treated_as_subject() {
    let Some(world) = Hermetic::start("plan-mode") else {
        return;
    };
    let seed_key = "e2eherm-seed-plan-mode";
    let payload = serde_json::json!({
        "tool_name": "EnterPlanMode",
        "tool_input": {"prompt": "refactor the wiring subsystem"}
    });

    let (before, after) =
        control_then_seed(&world, &payload, seed_key, "refactor the wiring subsystem");

    assert_eq!(
        before, "",
        "CONTROL: an empty project must yield no hint at all (anything here is ambient state \
         leaking into the test); got: {before}"
    );
    assert!(
        after.contains(&format!("memory: {seed_key}")),
        "EnterPlanMode with `prompt` must surface the seeded memory `{seed_key}`; got: {after}"
    );
    eprintln!("E2EHERM-RAN enter_plan_mode: control={before:?} seeded={after:?}");
}

/// Test 3 — TaskList has no `subject` / `description` / `prompt`, so the
/// `task_subject()` extractor returns None and the handler short-circuits
/// to empty context. Important to pin because this behaviour prevents
/// spamming the read-only list operation with noise.
#[test]
fn task_list_without_subject_returns_empty_context() {
    let payload = serde_json::json!({
        "tool_name": "TaskList",
        "tool_input": {}
    });
    let Some(env) = scout(&payload) else { return };

    assert_eq!(
        context_of(&env),
        "",
        "TaskList with empty tool_input must yield empty context (no subject = no hint)"
    );
}

/// Test 4 — the `wants_orphans` gate should suppress the wiring hint for
/// the read-only Task* tools even when a subject is present. We exploit
/// that by picking a nonsense subject that is very unlikely to match any
/// memory key (no memory hit) and verify that the orphan hint is NOT in
/// the output for TaskList / TaskGet / TaskOutput.
#[test]
fn read_only_task_tools_suppress_wiring_hint() {
    // "xyzzy_neverexisting_9487_query" is unlikely to hit memory / decompose.
    let subject = "xyzzy_neverexisting_9487_query";

    for read_only_tool in ["TaskList", "TaskGet", "TaskOutput"] {
        let payload = serde_json::json!({
            "tool_name": read_only_tool,
            "tool_input": {"subject": subject}
        });
        let Some(env) = scout(&payload) else { continue };
        let context = context_of(&env);
        assert!(
            !context.contains("wiring orphans:"),
            "{read_only_tool} must NOT include wiring orphan hint, got: {context}"
        );
    }
}

/// Test 5 — the file-based path (Read / Edit / Write) is preserved. This
/// tests that the Wave 25 dispatch doesn't accidentally swallow file
/// payloads. The existing SQLite-backed LRU cache produces non-error
/// output for any real file; empty context is also acceptable (cache
/// miss + scouter returned empty). The assertion is on the envelope
/// shape, not the content.
#[test]
fn file_based_tool_preserves_existing_shape() {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("lib.rs");
    let payload = serde_json::json!({
        "tool_name": "Read",
        "tool_input": {"file_path": file_path.to_string_lossy()}
    });
    let Some(env) = scout(&payload) else { return };

    assert_eq!(
        env.get("hookEventName").and_then(|v| v.as_str()),
        Some("PreToolUse"),
        "file-based dispatch must still emit PreToolUse envelope"
    );
    // additionalContext may be empty or populated depending on cache /
    // scouter state; both are correct behaviours for a file payload.
    assert!(
        env.get("additionalContext").is_some(),
        "envelope missing additionalContext key for Read payload"
    );
}

/// Test 6 — tool_name that isn't Task* / EnterPlanMode / file-based
/// should also route through the file_path branch, which short-circuits
/// to empty context when no file_path is present. This is the legacy
/// behaviour for tools outside the Task family (Bash, Grep, etc.).
#[test]
fn non_task_non_file_tool_returns_empty_context() {
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": "echo hi"}
    });
    let Some(env) = scout(&payload) else { return };
    assert_eq!(
        context_of(&env),
        "",
        "Bash without file_path falls through to empty context (existing behaviour)"
    );
}
