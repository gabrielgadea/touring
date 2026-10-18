//! S28 gate — acceptance tests for the cross-process baseline (17/10/2026).
//!
//! **The defect these lock.** `triad_hook::arm` wrote the pre-write baseline into a
//! process-local `moka` cache, but pre-write and post-write do not always run in the same
//! process: the client falls back to the standalone binary when the daemon misses its 3 s
//! budget, and under load that fallback is frequent. `post_write` then found no baseline,
//! `new_antipatterns` became `0`, and the gate passed as if the file were clean. Measured on
//! the `analise` project with a clean file and 5 new antipatterns: daemon→daemon 5/5 blocked,
//! every mixed pair 0/5, and under 16 concurrent loops the contract test failed 7/8 (0/10
//! unloaded).
//!
//! **What is proved here**, at the handler level rather than the store level:
//!
//! * [`s28_blocks_new_antipatterns_across_two_processes`] — arm in one OS process, gate in
//!   another, and get a real `Block` carrying `kept on disk`. It re-execs this same test
//!   binary for the "pre-write" half, so the two halves genuinely share nothing but the
//!   on-disk baseline.
//! * [`post_write_declares_the_abstention_when_no_pre_write_ran`] — no baseline ⇒ `Allow`
//!   or `Context`, and the context always carries the abstention text.
//! * [`post_write_declares_the_abstention_when_the_baseline_expires`] — TTL elapsed.
//! * [`post_write_declines_a_baseline_above_the_cap`] — above `TRIAD_DISK_MAX_BYTES`.
//! * [`post_edit_declares_the_abstention_when_the_edit_cannot_be_reversed`] — the sibling
//!   hook, whose baseline is rebuilt from the tool input and fails for a different reason.

use crate::runtime::HookRuntime;
use touring_hook_runtime::runtime::HookResponse;
use touring_hook_runtime::triad_hook;

/// Set in the re-executed child to tell it which half of the two-process test to run.
const CHILD_ROLE: &str = "S28_TEST_CHILD_ROLE";
/// The directory both halves work in — passed to the child because the two halves are
/// separate processes and a pid-derived path would differ between them.
const WORK_DIR: &str = "S28_TEST_WORK_DIR";
/// The session id both halves share — it is the key, and it must survive the process.
const SESSION: &str = "s28-two-process";

/// A clean Python file with none of the antipatterns the gate counts.
const CLEAN: &str = "def f():\n    return 1\n";

/// The same file with five NEW antipatterns: two bare `except`, a global, a star import and a
/// `print`. Five is past the block threshold of four, so a correct gate stops the write.
const DIRTY: &str = concat!(
    "import os\n",
    "from os import *\n",
    "COUNT = 0\n",
    "\n",
    "\n",
    "def f():\n",
    "    global COUNT\n",
    "    try:\n",
    "        pass\n",
    "    except:\n",
    "        pass\n",
    "    try:\n",
    "        pass\n",
    "    except:\n",
    "        pass\n",
    "    print(COUNT)\n",
);

/// Text of the context a `Context`/`Block` verdict carries, if any.
fn context_of(response: &HookResponse) -> Option<&str> {
    match response {
        HookResponse::Context { context, .. } => Some(context),
        HookResponse::Block { context, .. } => context.as_deref(),
        _ => None,
    }
}

/// A private state dir for one test. Installs `TOURING_TRIAD_DIR` so the store never touches
/// the real per-user directory.
///
/// Env vars are process-global and `cargo test` runs a process' tests on parallel threads, so
/// every test here is `#[serial]`.
fn use_state_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("s28-gate-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // 0700: the store refuses a pre-existing directory that is not private, so the test's
    // store directory must be created the way the store creates its own.
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .expect("state dir");
    // SAFETY: `#[serial]` guarantees no other thread in this process reads it concurrently.
    unsafe { std::env::set_var("TOURING_TRIAD_DIR", &dir) };
    dir
}

/// Re-run this test binary as a child process with `role`, the state dir, and the work dir.
///
/// The work dir must be passed explicitly: the two halves are separate processes with
/// separate pids, so deriving it from `process::id()` would have each half work in its own
/// directory and the parent would never see the file the child wrote.
fn run_child_role(
    role: &str,
    state: &std::path::Path,
    work: &std::path::Path,
) -> std::process::Output {
    let exe = std::env::current_exe().expect("test binary path");
    std::process::Command::new(exe)
        .arg("--exact")
        .arg("s28_gate::s28_blocks_new_antipatterns_across_two_processes")
        .arg("--nocapture")
        .env(CHILD_ROLE, role)
        .env("TOURING_TRIAD_DIR", state)
        .env(WORK_DIR, work)
        .output()
        .expect("spawn child")
}

/// Acceptance (a): a write that introduces new antipatterns is BLOCKED even when the pre-write
/// and the post-write ran in DIFFERENT processes.
///
/// The child half re-execs this binary, calls `pre_write` (which arms the baseline), writes the
/// dirty body to disk — as Claude Code would between the two hooks — and exits. The parent half
/// then runs `post_write` and must see a `Block`. It shares no memory with the child; the only
/// thing carried across is the on-disk baseline, which is the whole point.
///
/// **Fails on the pre-fix code**: the parent's `take` finds nothing in its own (empty) cache,
/// abstains, and the write passes.
#[test]
#[serial_test::serial(s28_state)]
fn s28_blocks_new_antipatterns_across_two_processes() {
    // ── Child half FIRST, before anything touches the environment.
    //
    // The child re-runs this same function from the top, so any `use_state_dir` call above
    // this branch would replace the state dir the parent handed down with one derived from
    // the CHILD's pid — and the two halves would silently stop sharing anything. That is the
    // whole point of this test, so the branch comes first and exits.
    if std::env::var(CHILD_ROLE).as_deref() == Ok("pre") {
        let work = std::path::PathBuf::from(std::env::var(WORK_DIR).expect("work dir from parent"));
        let file = work.join("s28_target.py");
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).expect("work dir");
        std::fs::write(&file, CLEAN).expect("seed clean file");

        let proj = tempfile::TempDir::new().expect("project");
        let mut runtime = HookRuntime::new(proj.path()).expect("runtime");
        let input = serde_json::json!({
            "session_id": SESSION,
            "tool_name": "Write",
            "tool_input": { "file_path": file.to_str().expect("utf8"), "content": CLEAN }
        });
        crate::pre_write::run_returning(&mut runtime, &input);

        // The model writes the dirty body; the file is on disk before the post hook runs.
        std::fs::write(&file, DIRTY).expect("apply write");
        std::process::exit(0);
    }

    // ── Parent half: only the post-write runs here; the pre-write just happened in the child.
    let state = use_state_dir("xproc");
    let work = std::env::temp_dir().join(format!("s28-gate-{}-xproc-work", std::process::id()));
    let file = work.join("s28_target.py");

    let child = run_child_role("pre", &state, &work);
    assert!(
        child.status.success(),
        "the pre-write half failed: {}\n--- stdout ---\n{}",
        String::from_utf8_lossy(&child.stderr),
        String::from_utf8_lossy(&child.stdout)
    );
    assert!(
        file.exists(),
        "the child must leave the target on disk for the post-write half; \
         child stdout: {}",
        String::from_utf8_lossy(&child.stdout)
    );

    let proj = tempfile::TempDir::new().expect("project");
    let mut runtime = HookRuntime::new(proj.path()).expect("runtime");
    let input = serde_json::json!({
        "session_id": SESSION,
        "tool_name": "Write",
        "tool_input": { "file_path": file.to_str().expect("utf8"), "content": DIRTY }
    });
    let response = crate::post_write::run_returning(&mut runtime, &input);

    match &response {
        HookResponse::Block { reason, .. } => {
            assert!(
                reason.contains("anti-patterns"),
                "the Block must name the gate that fired: {reason}"
            );
        }
        other => panic!(
            "a write introducing {DIRTY_LINES} antipatterns across a process boundary must Block, got {other:?}",
            DIRTY_LINES = 5
        ),
    }
    let ctx = context_of(&response).expect("the Block carries context");
    assert!(
        ctx.contains("kept on disk"),
        "the Block must tell the model the content is already written: {ctx}"
    );
    assert_eq!(
        std::fs::read_to_string(&file).expect("read").len(),
        DIRTY.len(),
        "D1-A: the hook never reverts the file"
    );
}

/// Acceptance (b): no pre-write at all ⇒ the gate abstains AND SAYS SO.
#[test]
#[serial_test::serial(s28_state)]
fn post_write_declares_the_abstention_when_no_pre_write_ran() {
    let _state = use_state_dir("no-pre");
    let work = tempfile::TempDir::new().expect("work");
    let file = work.path().join("no_pre.py");
    // Written but never armed: a gate with no baseline cannot tell pre-existing antipatterns
    // from new ones, so it must abstain — visibly.
    std::fs::write(&file, DIRTY).expect("write");

    let proj = tempfile::TempDir::new().expect("project");
    let mut runtime = HookRuntime::new(proj.path()).expect("runtime");
    let input = serde_json::json!({
        "session_id": "s28-never-armed",
        "tool_name": "Write",
        "tool_input": { "file_path": file.to_str().expect("utf8"), "content": DIRTY }
    });
    let response = crate::post_write::run_returning(&mut runtime, &input);

    assert!(
        !matches!(response, HookResponse::Block { .. }),
        "with no baseline the gate must never block blind, got {response:?}"
    );
    let ctx = context_of(&response).expect("an abstention is not silence — it must be declared");
    assert!(
        ctx.contains("S28 abstained"),
        "the context must declare the abstention: {ctx}"
    );
    assert!(
        ctx.contains("NEW anti-patterns were not counted"),
        "the declaration must say what did not happen: {ctx}"
    );
}

/// Acceptance (c, TTL): a baseline older than `TRIAD_TTL` is refused, and the refusal is
/// declared instead of degrading into "no antipatterns".
#[test]
#[serial_test::serial(s28_state)]
fn post_write_declares_the_abstention_when_the_baseline_expires() {
    let state = use_state_dir("expired");
    let work = tempfile::TempDir::new().expect("work");
    let file = work.path().join("expired.py");
    std::fs::write(&file, CLEAN).expect("seed clean file");
    let path = triad_hook::canonical_path(file.to_str().expect("utf8"));

    // Age the record past the TTL by rewriting its header timestamp. Sleeping 600 s is not a
    // test; the record's own clock is what the reader trusts, so that is what moves.
    let aged = aged_record(&state, "s28-expired", &path, CLEAN);
    assert!(aged, "the aged record must exist on disk");

    std::fs::write(&file, DIRTY).expect("apply write");
    let proj = tempfile::TempDir::new().expect("project");
    let mut runtime = HookRuntime::new(proj.path()).expect("runtime");
    let input = serde_json::json!({
        "session_id": "s28-expired",
        "tool_name": "Write",
        "tool_input": { "file_path": file.to_str().expect("utf8"), "content": DIRTY }
    });
    let response = crate::post_write::run_returning(&mut runtime, &input);

    assert!(
        !matches!(response, HookResponse::Block { .. }),
        "an expired baseline must not block, got {response:?}"
    );
    let ctx = context_of(&response).expect("an expired baseline must be declared");
    assert!(ctx.contains("S28 abstained"), "{ctx}");
    assert!(
        ctx.contains("expired"),
        "the declaration must name the reason: {ctx}"
    );
}

/// Acceptance (c, cap): a baseline above `TRIAD_DISK_MAX_BYTES` is not persisted, and the
/// resulting abstention is declared.
#[test]
#[serial_test::serial(s28_state)]
fn post_write_declines_a_baseline_above_the_cap() {
    let state = use_state_dir("cap");
    let work = tempfile::TempDir::new().expect("work");
    let file = work.path().join("huge.py");
    // A baseline over the cap is written by hand: the writer refuses to produce one, and
    // generating 8 MiB of source to prove it is refused would be its own kind of waste.
    let big = "x".repeat(triad_hook::TRIAD_DISK_MAX_BYTES + 1);
    std::fs::write(&file, &big).expect("write big file");
    let path = triad_hook::canonical_path(file.to_str().expect("utf8"));
    let header = serde_json::json!({
        "v": 1,
        "at": now_secs(),
        "path": path.display().to_string(),
        "session": "s28-cap",
        "len": big.len(),
    });
    let mut body = serde_json::to_string(&header).expect("json");
    body.push('\n');
    let rec = state.join(format!("{}.tbase", blake3_hex("s28-cap", &path)));
    std::fs::write(&rec, body).expect("write record");

    std::fs::write(&file, DIRTY).expect("apply write");
    let proj = tempfile::TempDir::new().expect("project");
    let mut runtime = HookRuntime::new(proj.path()).expect("runtime");
    let input = serde_json::json!({
        "session_id": "s28-cap",
        "tool_name": "Write",
        "tool_input": { "file_path": file.to_str().expect("utf8"), "content": DIRTY }
    });
    let response = crate::post_write::run_returning(&mut runtime, &input);

    assert!(
        !matches!(response, HookResponse::Block { .. }),
        "a baseline over the cap must not be used, got {response:?}"
    );
    let ctx = context_of(&response).expect("the cap refusal must be declared");
    assert!(ctx.contains("S28 abstained"), "{ctx}");
    assert!(
        ctx.contains("TRIAD_DISK_MAX_BYTES"),
        "the declaration must name the cap: {ctx}"
    );
}

/// The same gate, same declaration, in `post_edit` — whose baseline is rebuilt from the tool
/// input and fails for a different reason (an edit whose `old_string` is not in the post-state).
#[test]
#[serial_test::serial(s28_state)]
fn post_edit_declares_the_abstention_when_the_edit_cannot_be_reversed() {
    let _state = use_state_dir("edit");
    let work = tempfile::TempDir::new().expect("work");
    let file = work.path().join("edit_target.rs");
    // A file with no diagnostics: `post_edit` returns early with `Allow` when the PRE-edit
    // state had errors (post_edit.rs `had_error`), which would bypass the gate entirely and
    // make this test assert nothing.
    const AFTER: &str = "fn g() -> u8 {\n    2\n}\n";
    std::fs::write(&file, AFTER).expect("apply edit");

    let proj = tempfile::TempDir::new().expect("project");
    let mut runtime = HookRuntime::new(proj.path()).expect("runtime");
    // An edit whose `new_string` is NOT in the post-state — a pure deletion, which cannot be
    // located and therefore cannot be reversed. `content_before_edit` returns `None`, the
    // gate has no baseline, and it must say so instead of passing as a clean edit.
    let input = serde_json::json!({
        "session_id": "s28-edit",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": file.to_str().expect("utf8"),
            "old_string": "    1\n",
            "new_string": "",
        }
    });
    let response = crate::post_edit::run_returning(&mut runtime, &input);

    assert!(
        !matches!(response, HookResponse::Block { .. }),
        "with no reconstructable baseline the gate must never block blind, got {response:?}"
    );
    let ctx = context_of(&response)
        .unwrap_or_else(|| panic!("an abstention must be declared, not silent: {response:?}"));
    assert!(ctx.contains("S28 abstained"), "{ctx}");
    assert!(
        ctx.contains("NEW anti-patterns were not counted"),
        "the declaration must say what did not happen: {ctx}"
    );
}

/// A NEW file counts EVERY antipattern as new, and is not mistaken for an abstention.
///
/// The regression this locks (cross-audit, 17/10/2026): `Baseline::as_before()` returns
/// `None` for a file that did not exist, and that `None` was routed through the same
/// `Option<&str>` arm as a genuine abstention. A new file therefore counted ZERO and — worse
/// than the old silence — did not even declare that it had abstained. Every one of the other
/// tests here seeds a pre-existing file, which is exactly why that gap survived a green run.
#[test]
#[serial_test::serial(s28_state)]
fn a_new_file_counts_every_antipattern_as_new() {
    let _state = use_state_dir("new-file");
    let work = tempfile::TempDir::new().expect("work");
    let file = work.path().join("brand_new.py");
    // The file does NOT exist yet — this is the pre-write, and it arms `Baseline::Absent`.
    // The payload carries the content that is ABOUT to be written (that is what a real
    // `Write` looks like): `pre_write` skips an empty body before it ever reaches `arm`.
    let proj = tempfile::TempDir::new().expect("project");
    let mut runtime = HookRuntime::new(proj.path()).expect("runtime");
    crate::pre_write::run_returning(
        &mut runtime,
        &serde_json::json!({
            "session_id": "s28-new-file",
            "tool_name": "Write",
            "tool_input": { "file_path": file.to_str().expect("utf8"), "content": DIRTY }
        }),
    );
    assert!(!file.exists(), "the pre-write must not create the file");

    // Now the write lands: 5 antipatterns into a file that did not exist a moment ago.
    std::fs::write(&file, DIRTY).expect("create with antipatterns");
    let mut runtime = HookRuntime::new(proj.path()).expect("runtime");
    let response = crate::post_write::run_returning(
        &mut runtime,
        &serde_json::json!({
            "session_id": "s28-new-file",
            "tool_name": "Write",
            "tool_input": { "file_path": file.to_str().expect("utf8"), "content": DIRTY }
        }),
    );

    match &response {
        HookResponse::Block { reason, .. } => assert!(
            reason.contains("anti-patterns"),
            "the gate must fire on a new file: {reason}"
        ),
        other => panic!("5 antipatterns in a NEW file must Block, got {other:?}"),
    }
    let ctx = context_of(&response).expect("the Block carries context");
    assert!(
        ctx.contains("kept on disk"),
        "the Block must say the content is written: {ctx}"
    );
    assert!(
        !ctx.contains("S28 abstained"),
        "a new file HAS a baseline (it did not exist) — abstaining here would be a false \
         negative about the gate's own reach: {ctx}"
    );
}

/// A record whose timestamp is one second past the TTL, written the same way the hook writes
/// its own (header line of JSON, then the text). Returns whether the file landed.
fn aged_record(state: &std::path::Path, session: &str, path: &std::path::Path, text: &str) -> bool {
    let ttl = triad_hook::TRIAD_TTL.as_secs();
    let header = serde_json::json!({
        "v": 1,
        "at": now_secs().saturating_sub(ttl + 1),
        "path": path.display().to_string(),
        "session": session,
        "len": text.len(),
    });
    let mut body = serde_json::to_string(&header).expect("json");
    body.push('\n');
    body.push_str(text);
    let rec = state.join(format!("{}.tbase", blake3_hex(session, path)));
    std::fs::write(&rec, body).expect("write record");
    rec.exists()
}

/// The same key derivation the store uses: BLAKE3 over (session, canonical path).
fn blake3_hex(session: &str, path: &std::path::Path) -> String {
    let digest = blake3::hash(format!("{session}\u{0}{}", path.display()).as_bytes());
    digest.to_hex().to_string()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
