//! Live `cli-pre-task-scout` handler (Master Plan A.W2.P5 extraction).
//!
//! Mechanical extraction of the LIVE `cli_pre_task_scout` (the one wired in
//! `hook_registry.rs`) from `cli_handlers.rs`, together with its private cache
//! helpers, the `run_scouter_*` quick/task-mode functions, `quick_quality_context`,
//! and the `PreToolUsePayload` parser.
//!
//! NOTE: a previously-diverged, unwired `cli/scout.rs` fork (a stale, less-capable
//! duplicate — no task-mode routing, no Pensieve, no process-group guard) was verified
//! to have zero external consumers and removed (REGRA #0, 2026-06-29). This module is the
//! single wired implementation.

use crate::runtime::HookRuntime;
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Cache database path for pre-task-scout results.
fn pre_task_scout_cache_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/gabrielgadea".to_string());
    Path::new(&home).join(".claude/data/pre_task_scout_cache.db")
}

/// Initialize the pre_task_scout cache schema.
///
/// Schema: cache(key TEXT PRIMARY KEY, response_json TEXT, created_at INTEGER, file_mtime REAL)
fn ensure_pre_task_scout_cache(db: &rusqlite::Connection) -> rusqlite::Result<()> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS pre_task_scout_cache (
            key TEXT PRIMARY KEY,
            response_json TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            file_mtime REAL NOT NULL
        )",
        [],
    )?;
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_pre_task_scout_created_at ON pre_task_scout_cache(created_at)",
        [],
    )?;
    db.execute("PRAGMA journal_mode=WAL", [])?;
    Ok(())
}

/// Compute cache key from file_path + tool_name + mtime.
fn pre_task_scout_cache_key(
    file_path: &str,
    tool_name: &str,
    mtime: f64,
    session_id: &str,
) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{file_path}|{tool_name}|{mtime}|{session_id}").hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Get mtime of a file, returning 0.0 if file does not exist.
fn file_mtime(file_path: &str) -> f64 {
    std::fs::metadata(file_path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64()
        })
        .unwrap_or(0.0)
}

/// LRU eviction: keep only the newest 500 entries.
fn evict_lru_if_needed(db: &rusqlite::Connection) -> rusqlite::Result<()> {
    let count: i64 = db.query_row("SELECT COUNT(*) FROM pre_task_scout_cache", [], |r| {
        r.get(0)
    })?;
    if count > 500 {
        db.execute(
            "DELETE FROM pre_task_scout_cache WHERE key IN (
                SELECT key FROM pre_task_scout_cache ORDER BY created_at ASC LIMIT ?1
            )",
            params![count - 450],
        )?;
    }
    Ok(())
}

/// Check if cache entry is still valid (TTL = 1 hour = 3600 seconds).
fn cache_entry_valid(db: &rusqlite::Connection, key: &str, file_mtime: f64) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let one_hour_ago = now.saturating_sub(3600) as i64;
    db.query_row(
        "SELECT response_json FROM pre_task_scout_cache
         WHERE key = ?1 AND created_at > ?2 AND ABS(file_mtime - ?3) < 0.001",
        params![key, one_hour_ago, file_mtime],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

/// True when the real-`touring`-binary subprocess in `cli_pre_task_scout` must be
/// suppressed: under `cargo test` (`cfg!(test)` — covers this crate's own unit tests)
/// or when `TOURING_PRETASK_NO_SUBPROCESS` is set to a **non-zero** value (integration
/// tests / binary-spawn harnesses, where `cfg(test)` is inactive for this lib). `=0` or
/// unset ⇒ off — opt-in via a non-zero value, mirroring the house env convention (so a
/// `…=0` meant as "disable" never silently enables suppression). Avoids the pipe-leak
/// hang where a `timeout`-killed `touring` leaves a detached daemon grandchild holding the
/// stdout pipe and `Command::output()` blocks on EOF (gotcha
/// `pretask-pensieve-flaky-subprocess`, 2026-06-27). Production spawns normally.
#[inline]
fn subprocess_suppressed() -> bool {
    cfg!(test) || std::env::var("TOURING_PRETASK_NO_SUBPROCESS").is_ok_and(|v| v != "0")
}

/// Run one `touring …` quick-mode subprocess under the process-group timeout guard,
/// appending its trimmed stdout (+ newline) to `out` on clean success. Bounding EVERY
/// quick-mode spawn — not just the cortex one — closes the whole file-branch against the
/// pipe-leak hang: a `touring index find`/`ast blast`/`wiring score` that races a
/// starting daemon can no longer block on an inherited pipe (gotcha 2026-06-27).
fn run_quick(touring_bin: &str, args: &[&str], out: &mut String, timeout: Duration) {
    let mut cmd = std::process::Command::new(touring_bin);
    cmd.args(args);
    if let Some(found) = run_in_process_group_timeout(cmd, timeout)
        && !found.is_empty()
    {
        out.push_str(&found);
        out.push('\n');
    }
}

/// Run touring-scouter quick-mode for a file and return the findings. Each of the three
/// probes is process-group-bounded (see `run_quick`) so the fallback never out-blocks the
/// cortex path it backstops.
fn run_scouter_quick_mode(file_path: &str) -> String {
    if subprocess_suppressed() {
        // No real-binary spawn under test — return empty findings in-process.
        return String::new();
    }
    let touring_bin = std::env::var("TOURING_BIN")
        .unwrap_or_else(|_| "/home/gabrielgadea/.local/bin/touring".to_string());
    let quick = Duration::from_secs(4);
    let mut output = String::new();
    if let Some(basename) = Path::new(file_path).file_name().and_then(|n| n.to_str()) {
        run_quick(
            &touring_bin,
            &["index", "find", basename, "-j"],
            &mut output,
            quick,
        );
    }
    run_quick(
        &touring_bin,
        &["ast", "blast", file_path, "-j"],
        &mut output,
        quick,
    );
    run_quick(
        &touring_bin,
        &["wiring", "score", file_path, "-j"],
        &mut output,
        quick,
    );
    output.trim().to_string()
}

/// Wall-clock cap for the real-binary scout subprocess (was the coreutils `timeout 8`
/// wrapper; now enforced in-process so the whole process group can be SIGKILLed on
/// expiry — coreutils `timeout` signals only the direct child, not detached descendants).
const SCOUT_SUBPROCESS_TIMEOUT_SECS: u64 = 8;

/// Spawn the real `touring cortex pre-task-scout` for `file_path`, falling back to
/// in-process quick-mode on timeout / failure. Suppressed under test
/// (`subprocess_suppressed`). The spawn is bounded by `run_in_process_group_timeout`,
/// which SIGKILLs the whole process group on expiry so a detached daemon descendant
/// that inherited the stdout pipe can never block us on an EOF that never arrives
/// (pipe-leak hang, gotcha `pretask-pensieve-flaky-subprocess`, 2026-06-27).
fn spawn_scouter_for_file(file_path: &str, tool_name: &str) -> String {
    if subprocess_suppressed() {
        return run_scouter_quick_mode(file_path);
    }
    let mut cmd = std::process::Command::new("/home/gabrielgadea/.claude/hooks/touring");
    cmd.args(["cortex", "pre-task-scout"])
        .env("TOURING_SCOUT_FILE", file_path)
        .env("TOURING_SCOUT_TOOL", tool_name);
    run_in_process_group_timeout(cmd, Duration::from_secs(SCOUT_SUBPROCESS_TIMEOUT_SECS))
        .unwrap_or_else(|| run_scouter_quick_mode(file_path))
}

/// SIGKILL an entire process group (best-effort). A thin safe wrapper that confines the
/// single `unsafe` `killpg`: it is a pure syscall with no memory-safety precondition, and
/// any error (e.g. `ESRCH` for an already-dead group) is ignorable — so the call site stays
/// `unsafe`-free and the invariant is documented in exactly one place.
fn sigkill_process_group(pgid: libc::pid_t) {
    // SAFETY: `killpg` carries no memory-safety contract; it signals a process group and
    // returns an errno we deliberately ignore. SIGKILL is always deliverable.
    unsafe {
        libc::killpg(pgid, libc::SIGKILL);
    }
}

/// Run `cmd` in its OWN process group, capture stdout, and bound the wait by `timeout`.
/// On expiry the ENTIRE group is SIGKILLed (`killpg`), so a detached descendant that
/// inherited the stdout pipe cannot keep us blocked on an EOF that never comes — the
/// root of the pre-task-scout pipe-leak hang (2026-06-27). Returns the trimmed stdout
/// on clean success, or `None` on timeout / spawn failure / non-zero exit so the caller
/// can fall back to in-process quick-mode.
fn run_in_process_group_timeout(
    mut cmd: std::process::Command,
    timeout: Duration,
) -> Option<String> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // New process group (pgid == child pid) so `killpg` reaches every descendant.
        .process_group(0);
    let mut child = cmd.spawn().ok()?;
    let pgid: libc::pid_t = child.id().try_into().ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    // Reader thread drains stdout. It EOFs when the child AND every descendant holding
    // the pipe have exited — i.e. promptly on a clean finish, or after the killpg below.
    std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        let _ = tx.send(buf);
    });
    match rx.recv_timeout(timeout) {
        Ok(buf) => child
            .wait()
            .is_ok_and(|s| s.success())
            .then(|| buf.trim().to_string()),
        Err(_) => {
            // Kill the child + any in-group descendant holding the pipe (the group we
            // created via `process_group(0)`). The single `unsafe` lives in the wrapper.
            sigkill_process_group(pgid);
            let _ = child.wait();
            // The reader thread is intentionally NOT joined: were a misbehaving
            // setsid'd descendant to escape the killpg and keep the pipe open, joining
            // would re-introduce the very hang this guards against. Detaching it is
            // benign — the daemon handler returns now, which is the whole point.
            None
        }
    }
}

/// Wave 25 (2026-04-18): task-mode counterpart to `run_scouter_quick_mode`.
/// Wired by `cli_pre_task_scout` for Task* / EnterPlanMode tools that carry
/// `subject` / `description` instead of `file_path`. Composes a compact
/// hook-context line from three signals — past memory hits, ready
/// subtasks, and the current orphan headline.
///
/// **Critical**: this runs INSIDE the per-project actor that owns
/// `HookRuntime`. Spawning `touring` subprocesses here would re-enter the
/// same actor through the daemon socket and deadlock (the daemon serves
/// commands serially per project). We therefore call the in-process
/// handlers directly and parse their JSON output — same data path, zero
/// subprocess + zero socket round-trip.
fn run_scouter_task_mode(rt: &mut HookRuntime, subject: &str, tool_name: &str) -> String {
    use crate::cli_handlers::{cli_decompose_ready, cli_memory_recall, cli_wiring_orphans};
    let mut parts: Vec<String> = Vec::new();
    let recall_payload = serde_json::json!({ "query" : subject, "limit" : 2 });
    let recall_raw = cli_memory_recall(rt, &recall_payload);
    if let Ok(recall) = serde_json::from_str::<serde_json::Value>(&recall_raw) {
        let keys: Vec<String> = recall
            .get("entries")
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter()
                    .take(2)
                    .filter_map(|e| e.get("key").and_then(|k| k.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if !keys.is_empty() {
            parts.push(format!("memory: {}", keys.join(", ")));
        }
    }
    let ready_raw = cli_decompose_ready(rt, &serde_json::json!({}));
    if let Ok(ready) = serde_json::from_str::<serde_json::Value>(&ready_raw) {
        let count = ready
            .get("ready_count")
            .and_then(|c| c.as_u64())
            .unwrap_or(0);
        if count > 0 {
            parts.push(format!("decompose ready: {count} subtask(s)"));
        }
    }
    let wants_orphans = tool_name == "TaskCreate" || tool_name == "EnterPlanMode";
    if wants_orphans {
        let orphans_raw = cli_wiring_orphans(rt, &serde_json::Value::Null);
        if let Ok(orphans) = serde_json::from_str::<serde_json::Value>(&orphans_raw) {
            let count = orphans
                .get("orphan_count")
                .and_then(|c| c.as_u64())
                .unwrap_or(0);
            if count > 0 {
                parts.push(format!(
                    "wiring orphans: {count} pub symbol(s) — candidates for wiring tasks"
                ));
            }
        }
    }
    if tool_name == "TaskCreate" && !subject.is_empty() {
        let state_hashes: Vec<u64> = subject
            .split_whitespace()
            .map(|word| {
                let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                for &b in word.as_bytes() {
                    h ^= u64::from(b);
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
                h
            })
            .collect();
        if !state_hashes.is_empty()
            && let Ok(pensieve) = rt.learning.pensieve.try_borrow()
            && let Some(penalty) = pensieve.check_known_failure_seq(&state_hashes)
        {
            parts
                        .push(
                            format!(
                                "⚠ pensieve: similar task failed before (penalty={penalty:.2}) — review gotchas before proceeding"
                            ),
                        );
        }
    }
    parts.join(" | ")
}

/// Wave 26 (2026-04-18): ported from the now-deleted
/// `cli_handlers_scout.rs`. Quick quality label for `rel_path` from the
/// knowledge DB — uses `module_wiring_status::integration_score` as a
/// quality proxy and returns a single-line string for context injection,
/// or an empty string when the file has no registered symbols (avoids
/// `UNKNOWN` noise for files that haven't been indexed yet).
///
/// Wired into the file-based path of `cli_pre_task_scout` so that
/// PreToolUse context for Read / Edit / Write tools now includes the
/// target file's wiring-integration quality headline.
fn quick_quality_context(rt: &HookRuntime, rel_path: &str) -> String {
    let status = match rt.ctx.knowledge.module_wiring_status(rel_path) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    if status.total_pub_symbols == 0 {
        return String::new();
    }
    let quality_label = if status.integration_score >= 0.9 {
        "HIGH"
    } else if status.integration_score >= 0.5 {
        "MEDIUM"
    } else {
        "LOW"
    };
    format!(
        "[QUALITY] {} — integration_quality={} (score={:.0}%, orphans={})",
        rel_path,
        quality_label,
        status.integration_score * 100.0,
        status.orphan_symbols.len(),
    )
}

/// PreToolUse payload expected from Claude Code hook.
#[derive(serde::Deserialize)]
struct PreToolUsePayload {
    tool_name: String,
    tool_input: serde_json::Value,
    /// Included in cache key for session-discriminant scouting (line ~2479).
    session_id: Option<String>,
}
impl PreToolUsePayload {
    fn file_path(&self) -> Option<&str> {
        self.tool_input.get("file_path").and_then(|v| v.as_str())
    }
    /// Wave 25 (2026-04-18): extract the human-authored task intent from
    /// tool_input for Task* / EnterPlanMode-style tools that don't carry a
    /// `file_path`. Prefers the more specific `subject` over the descriptive
    /// `description`, and returns the first non-empty string found. This is
    /// what drives memory-recall + decompose-ready enrichment.
    fn task_subject(&self) -> Option<String> {
        let input = &self.tool_input;
        for key in ["subject", "description", "prompt"] {
            if let Some(s) = input.get(key).and_then(|v| v.as_str()) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }
    /// Wave 25: `TaskCreate`, `TaskUpdate`, `TaskList`, `TaskOutput`,
    /// `TaskGet`, `EnterPlanMode`, `ExitPlanMode` carry no `file_path` —
    /// route them through `run_scouter_task_mode` instead of returning an
    /// empty `additionalContext`.
    fn is_task_tool(&self) -> bool {
        self.tool_name.starts_with("Task")
            || self.tool_name == "EnterPlanMode"
            || self.tool_name == "ExitPlanMode"
    }
}

/// `cli-pre-task-scout` handler.
///
/// Receives PreToolUse JSON via daemon_query payload, computes cache key from
/// file_path + tool_name + mtime, checks SQLite LRU cache,
/// on miss spawns touring-scouter (8s timeout), caches result (TTL 1h, max 500 entries),
/// returns `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "<scouter findings>"}}`.
pub fn cli_pre_task_scout(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let payload: PreToolUsePayload = match serde_json::from_value(payload.clone()) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!("pre_task_scout: failed to parse payload: {}", e);
            return serde_json::json!(
                { "hookSpecificOutput" : { "hookEventName" : "PreToolUse",
                "additionalContext" : "" } }
            )
            .to_string();
        }
    };
    if payload.is_task_tool() {
        let context = payload
            .task_subject()
            .map(|subject| run_scouter_task_mode(rt, &subject, &payload.tool_name))
            .unwrap_or_default();
        return serde_json::json!(
            { "hookSpecificOutput" : { "hookEventName" : "PreToolUse",
            "additionalContext" : context } }
        )
        .to_string();
    }
    let file_path = match payload.file_path() {
        Some(fp) if !fp.is_empty() => fp,
        _ => {
            return serde_json::json!(
                { "hookSpecificOutput" : { "hookEventName" : "PreToolUse",
                "additionalContext" : "" } }
            )
            .to_string();
        }
    };
    let tool_name = &payload.tool_name;
    let mtime = file_mtime(file_path);
    let session_discriminant = payload.session_id.as_deref().unwrap_or("no-session");
    let cache_key = pre_task_scout_cache_key(file_path, tool_name, mtime, session_discriminant);
    let cache_path = pre_task_scout_cache_path();
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = match rusqlite::Connection::open(&cache_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("pre_task_scout: failed to open cache DB: {}", e);
            let findings = run_scouter_quick_mode(file_path);
            return serde_json::json!(
                { "hookSpecificOutput" : { "hookEventName" : "PreToolUse",
                "additionalContext" : findings } }
            )
            .to_string();
        }
    };
    if let Err(e) = ensure_pre_task_scout_cache(&conn) {
        tracing::debug!("pre_task_scout: failed to init cache schema: {}", e);
    }
    if let Some(cached) = cache_entry_valid(&conn, &cache_key, mtime) {
        tracing::debug!("pre_task_scout: cache hit for {}", file_path);
        return serde_json::json!(
            { "hookSpecificOutput" : { "hookEventName" : "PreToolUse",
            "additionalContext" : cached } }
        )
        .to_string();
    }
    tracing::debug!(
        "pre_task_scout: cache miss for {}, running scouter",
        file_path
    );
    let mut findings = spawn_scouter_for_file(file_path, tool_name);
    let rel_path = crate::runtime::make_relative(file_path, &rt.project_root);
    let quality_ctx = quick_quality_context(rt, &rel_path);
    if !quality_ctx.is_empty() {
        if !findings.is_empty() {
            findings.push('\n');
        }
        findings.push_str(&quality_ctx);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let _ = conn.execute(
        "INSERT OR REPLACE INTO pre_task_scout_cache (key, response_json, created_at, file_mtime)
         VALUES (?1, ?2, ?3, ?4)",
        params![cache_key, findings, now, mtime],
    );
    let _ = evict_lru_if_needed(&conn);
    serde_json::json!(
        { "hookSpecificOutput" : { "hookEventName" : "PreToolUse", "additionalContext" :
        findings } }
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        run_in_process_group_timeout, run_scouter_quick_mode, run_scouter_task_mode,
        spawn_scouter_for_file, subprocess_suppressed,
    };
    use crate::runtime::HookRuntime;
    use std::time::Duration;

    fn fnv1a(word: &str) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in word.as_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    }
    fn make_rt() -> HookRuntime {
        let tmp = tempfile::tempdir().expect("tempdir");
        HookRuntime::new(tmp.path()).expect("HookRuntime::new")
    }
    #[test]
    fn pensieve_empty_no_warning_on_task_create() {
        let mut rt = make_rt();
        let out = run_scouter_task_mode(&mut rt, "refactor auth module", "TaskCreate");
        assert!(
            !out.contains("pensieve"),
            "empty Pensieve should not emit warning, got: {out}"
        );
    }
    #[test]
    fn pensieve_known_failure_emits_warning_on_task_create() {
        let mut rt = make_rt();
        let states: Vec<u64> = "refactor auth module"
            .split_whitespace()
            .map(fnv1a)
            .collect();
        {
            let mut p = rt.learning.pensieve.borrow_mut();
            p.record_failure(&states, "dependency cycle in auth refactor", 3);
        }
        let out = run_scouter_task_mode(&mut rt, "refactor auth module", "TaskCreate");
        assert!(
            out.contains("pensieve"),
            "should emit pensieve warning after recording matching failure, got: {out}"
        );
        assert!(
            out.contains("penalty="),
            "warning should include penalty value, got: {out}"
        );
    }
    #[test]
    fn pensieve_check_skipped_for_task_list() {
        let mut rt = make_rt();
        {
            let mut p = rt.learning.pensieve.borrow_mut();
            p.record_failure(&[1, 2, 3], "past failure", 1);
        }
        let out = run_scouter_task_mode(&mut rt, "refactor auth module", "TaskList");
        assert!(
            !out.contains("pensieve"),
            "TaskList should NOT trigger Pensieve check, got: {out}"
        );
    }

    #[test]
    fn subprocess_suppressed_true_under_cfg_test() {
        // Under `cargo test`, cfg!(test) is active for this crate, so the real-binary
        // scout subprocess in cli_pre_task_scout MUST be suppressed — this is the guard
        // that prevents the pipe-leak hang (gotcha pretask-pensieve-flaky-subprocess).
        assert!(
            subprocess_suppressed(),
            "cfg!(test) must suppress the pre-task-scout subprocess under cargo test"
        );
    }

    #[test]
    fn quick_mode_returns_empty_without_spawning_under_test() {
        // run_scouter_quick_mode must NOT spawn the `touring` binary under test: it
        // returns empty findings in-process. Before the guard it spawned 3 subprocesses
        // (index find / ast blast / wiring score) and could pipe-leak hang under load.
        let out = run_scouter_quick_mode("/nonexistent/path/foo.rs");
        assert!(
            out.is_empty(),
            "guarded quick-mode must return empty in-process, got: {out}"
        );
    }

    #[test]
    fn spawn_scouter_for_file_no_subprocess_under_test() {
        // The file-path branch of cli_pre_task_scout routes through spawn_scouter_for_file,
        // which under test must skip the `timeout … touring cortex pre-task-scout` spawn
        // and its quick-mode fallback entirely — returning empty findings in-process.
        let out = spawn_scouter_for_file("/nonexistent/path/foo.rs", "Edit");
        assert!(
            out.is_empty(),
            "guarded file-branch must not spawn; got: {out}"
        );
    }

    #[test]
    fn process_group_timeout_returns_output_on_fast_command() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "printf scout-ok"]);
        let out = run_in_process_group_timeout(cmd, Duration::from_secs(5));
        assert_eq!(out.as_deref(), Some("scout-ok"));
    }

    #[test]
    fn process_group_timeout_kills_orphan_holding_pipe_without_hanging() {
        // Reproduces the pipe-leak: `sh` exits immediately but backgrounds a `sleep`
        // that inherits its stdout pipe. A naive `.output()` would block ~30s on the
        // orphan's write-end; the process-group SIGKILL must terminate the whole group
        // on timeout and return promptly with None (the root fix for the 2026-06-27
        // pre-task-scout hang).
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "sleep 30 & exit 0"]);
        let start = std::time::Instant::now();
        let out = run_in_process_group_timeout(cmd, Duration::from_millis(500));
        let elapsed = start.elapsed();
        assert!(out.is_none(), "timeout path must return None, got {out:?}");
        assert!(
            elapsed < Duration::from_secs(5),
            "must not block on the orphan's pipe; took {elapsed:?}"
        );
    }

    #[test]
    fn process_group_timeout_actually_kills_the_orphan_not_just_returns() {
        // The test above proves we RETURN fast; this proves the killpg is EFFECTIVE — the
        // detached orphan is actually SIGKILLed (no process leak), not merely abandoned by
        // the detached reader. A backgrounded `sleep` would `touch` a sentinel after 1s; if
        // the process-group SIGKILL works it dies first and the sentinel never appears.
        let sentinel = std::env::temp_dir().join(format!("ppg_kill_proof_{}", std::process::id()));
        let _ = std::fs::remove_file(&sentinel);
        let script = format!("sleep 1 && touch {} & exit 0", sentinel.display());
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", &script]);
        let out = run_in_process_group_timeout(cmd, Duration::from_millis(200));
        assert!(out.is_none(), "timeout path returns None, got {out:?}");
        // Wait past the orphan's 1s touch window: if killpg fired, the touch never ran.
        std::thread::sleep(Duration::from_millis(1500));
        let leaked = sentinel.exists();
        let _ = std::fs::remove_file(&sentinel);
        assert!(
            !leaked,
            "killpg must SIGKILL the backgrounded orphan before its `touch` fires"
        );
    }
}
