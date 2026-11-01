//! r3 shadow — the observe-only event emitter (plan `2026-10-02-jev-harness-r3`, S-8).
//!
//! The shadow only OBSERVES and RECORDS: it never changes what is injected nor what
//! is classified. Three obligations, each guarded by a test:
//!
//! 1. **Flag OFF ⇒ zero I/O and an identical return.** [`r3_shadow_enabled`] resolves
//!    `TOURING_R3_SHADOW` (env, `1`/`true` ⇒ on, `0`/`false` ⇒ off) over
//!    `[shadow] r3 = true` in `<root>/.touring/touring.toml`; env beats the file and the
//!    default is OFF. The env var is read per call (no I/O); the file verdict is read
//!    ONCE per project per process and cached, so a flag-OFF hook path costs a map lookup.
//!    Because the daemon reads the file once, flipping the toml takes effect on the next
//!    daemon restart (the same as the env var, which is fixed at daemon spawn).
//! 2. **Emission failure never reaches the caller.** [`emit_r3_event`] returns a `bool`
//!    nobody has to read; an I/O error is swallowed and counted in [`dropped_event_count`].
//! 3. **No free text.** The schema is closed (`scripts/r3_event_schema.py`, the executable
//!    source) and no string in a line may exceed [`MAX_TEXT_CHARS`]; [`emit_r3_event_at`]
//!    refuses (and counts) a line that breaks the cap instead of writing it, so a future
//!    field cannot smuggle a prompt out by accident. The event contract is
//!    `docs/plans/2026-10-02-jev-harness-r3/data/EVENT-CONTRACT.md`.
//!
//! Lines go to `~/.claude/touring/logs/r3_events.jsonl` through
//! [`crate::hook_trace::append_json_line`] (one `write_all` over `O_APPEND`, atomic per
//! line). The evaluator is a separate process; the only coupling is the file.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Env var that turns the shadow on (`1`/`true`) or off (`0`/`false`).
pub const ENV_FLAG: &str = "TOURING_R3_SHADOW";
/// Explicit override of the events file (precedent: `TOURING_HOOK_TRACE_FILE`).
pub const ENV_EVENTS_FILE: &str = "TOURING_R3_EVENTS_FILE";
/// No string anywhere in an event line may be longer than this (S6, privacy).
pub const MAX_TEXT_CHARS: usize = 80;
/// An A2 offer closes after this many Bash calls without a verdict (E1.4, K = 3).
pub const K_WINDOW: usize = 3;
/// Line schema version (`"v"`).
pub const SCHEMA_VERSION: u64 = 1;

/// Lines the emitter refused or failed to write — never silent (rule: swallow + count).
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// How many event lines this process failed or refused to write.
pub fn dropped_event_count() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

/// `1`/`true` ⇒ `Some(true)`, `0`/`false` ⇒ `Some(false)`, anything else ⇒ `None`
/// (a value we do not understand must not flip the flag in either direction).
fn parse_bool_flag(raw: &str) -> Option<bool> {
    let v = raw.trim().trim_matches('"');
    if v == "1" || v.eq_ignore_ascii_case("true") {
        Some(true)
    } else if v == "0" || v.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

/// `[shadow] r3 = <bool>` from a `touring.toml` text. A line scan with section state
/// (the same stance as `code_mode::project_presentation`: neither side of this crate
/// pulls a TOML parser for one key). Any other shape ⇒ `None` ⇒ falls to the default.
fn toml_declares_r3(text: &str) -> Option<bool> {
    let mut in_shadow = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_shadow = l == "[shadow]";
            continue;
        }
        if !in_shadow {
            continue;
        }
        let Some((key, value)) = l.split_once('=') else {
            continue;
        };
        if key.trim() != "r3" {
            continue;
        }
        let value = value.split('#').next().unwrap_or("");
        return parse_bool_flag(value);
    }
    None
}

/// The precedence rule, pure: env (when it says something) > toml > OFF.
fn resolve_flag(env: Option<&str>, toml_text: Option<&str>) -> bool {
    env.and_then(parse_bool_flag)
        .or_else(|| toml_text.and_then(toml_declares_r3))
        .unwrap_or(false)
}

/// Is the r3 shadow on for this project? Default OFF.
///
/// OFF costs a `var_os` lookup plus a cached-map read — no file I/O after the first
/// call per project (the toml verdict, including "absent", is remembered).
pub fn r3_shadow_enabled(project_root: &Path) -> bool {
    if let Some(raw) = std::env::var_os(ENV_FLAG)
        && let Some(verdict) = raw.to_str().and_then(parse_bool_flag)
    {
        return verdict;
    }
    static TOML_VERDICT: OnceLock<Mutex<HashMap<PathBuf, bool>>> = OnceLock::new();
    let cache = TOML_VERDICT.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&verdict) = cache.get(project_root) {
        return verdict;
    }
    let text = std::fs::read_to_string(project_root.join(".touring/touring.toml")).ok();
    let verdict = resolve_flag(None, text.as_deref());
    cache.insert(project_root.to_path_buf(), verdict);
    verdict
}

/// Where event lines go: `TOURING_R3_EVENTS_FILE`, else
/// `~/.claude/touring/logs/r3_events.jsonl`.
pub fn events_path() -> PathBuf {
    if let Some(p) = std::env::var_os(ENV_EVENTS_FILE).filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(format!("{home}/.claude/touring/logs/r3_events.jsonl"))
}

thread_local! {
    /// Test-only redirect: when set, `maybe_emit_a1_block` writes here instead of
    /// the default file. Use [`with_events_path`] to set/clear in an RAII scope.
    static THREAD_EVENTS_PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Redirect A1 emissions on the **current thread** to `path` for the duration of
/// `f`, then restore the previous setting.
///
/// Used only in tests that call `run_returning` and want to capture emitted events
/// without setting `TOURING_R3_EVENTS_FILE` (which would race parallel tests).
#[doc(hidden)]
pub fn with_events_path<R>(path: &Path, f: impl FnOnce() -> R) -> R {
    /// Puts the PREVIOUS redirect back on every exit, a panic in `f` included. The doc used to
    /// promise this ("restore the previous setting", "RAII scope") while the body cleared to
    /// `None`, so nesting erased the outer redirect (cross-audit 03/10/2026, finding E).
    struct Restore(Option<PathBuf>);
    impl Drop for Restore {
        fn drop(&mut self) {
            THREAD_EVENTS_PATH.with(|tl| *tl.borrow_mut() = self.0.take());
        }
    }
    let previous = THREAD_EVENTS_PATH.with(|tl| tl.borrow_mut().replace(path.to_path_buf()));
    let _restore = Restore(previous);
    f()
}

/// First [`MAX_TEXT_CHARS`] characters (never splits a code point).
pub fn truncate_text(s: &str) -> String {
    s.chars().take(MAX_TEXT_CHARS).collect()
}

/// `sha256(raw session id)` as 12 lowercase hex chars — the only way a session is ever
/// named in an event; the raw id is never written.
pub fn session_hash(raw_session_id: &str) -> String {
    let digest = Sha256::digest(raw_session_id.as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// Does any string in `value` exceed the text cap? (the privacy net the emitter owns).
fn exceeds_text_cap(value: &Value) -> bool {
    match value {
        Value::String(s) => s.chars().count() > MAX_TEXT_CHARS,
        Value::Array(items) => items.iter().any(exceeds_text_cap),
        Value::Object(map) => map.values().any(exceeds_text_cap),
        _ => false,
    }
}

/// Append `event` to `path` as ONE line. Returns whether the line was written.
///
/// Never panics and never propagates: a refused line (text cap) or an I/O error bumps
/// [`dropped_event_count`] and returns `false`. The caller is free to ignore the result.
pub fn emit_r3_event_at(path: &Path, event: &Value) -> bool {
    if exceeds_text_cap(event) {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        tracing::debug!("r3_shadow: line refused — a string exceeds the text cap");
        return false;
    }
    match crate::hook_trace::append_json_line(path, event) {
        Ok(()) => true,
        Err(err) => {
            DROPPED.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(?err, "r3_shadow: event write failed (swallowed)");
            false
        }
    }
}

/// [`emit_r3_event_at`] on the default events file.
pub fn emit_r3_event(event: &Value) -> bool {
    emit_r3_event_at(&events_path(), event)
}

// ── A1 emitter ────────────────────────────────────────────────────────────────

/// Emit an `a1_block` event to the default events file when a PreToolUse hook
/// injects context into the model. The public entry point; tests use
/// [`maybe_emit_a1_block_at`] to supply an explicit file path.
///
/// Observe-only: `ctx` is received by shared reference and never mutated.
/// Flag OFF ⇒ immediate return without any I/O or allocation (beyond the
/// `var_os` check inside `r3_shadow_enabled`). Any emission failure is
/// swallowed and counted by [`emit_r3_event_at`] — it never reaches the caller.
pub fn maybe_emit_a1_block(project_root: &Path, session_id: &str, hook: &str, ctx: &str) {
    let path = THREAD_EVENTS_PATH
        .with(|tl| tl.borrow().clone())
        .unwrap_or_else(events_path);
    maybe_emit_a1_block_at(&path, project_root, session_id, hook, ctx);
}

/// [`maybe_emit_a1_block`] with an explicit destination path.
///
/// Used in tests and micro-benchmarks so they never touch the production events
/// file and never race on the `TOURING_R3_EVENTS_FILE` environment variable.
pub fn maybe_emit_a1_block_at(
    path: &Path,
    project_root: &Path,
    session_id: &str,
    hook: &str,
    ctx: &str,
) {
    if !r3_shadow_enabled(project_root) {
        return;
    }
    // All work beyond this point runs ONLY when the shadow is ON.
    let bytes = ctx.len() as u64;
    let first_line = ctx.lines().next().unwrap_or("");
    let first_line_head = truncate_text(first_line);
    // sha256 of the first line → first 8 bytes → 16 lowercase hex chars.
    // Satisfies SHA16_RE = r"^[0-9a-f]{16}$" in the schema (S-1).
    let first_line_sha: String = {
        let d = Sha256::digest(first_line.as_bytes());
        d.iter().take(8).map(|b| format!("{b:02x}")).collect()
    };
    // Schema: 0 < len ≤ 40 chars. Cap via chars so a multibyte name never panics.
    let hook_safe: String = hook.chars().take(40).collect();
    let event = json!({
        "v": SCHEMA_VERSION,
        "kind": "a1_block",
        "use_case": "a1_relevance",
        "ts_ms": now_ms(),
        "session_hash": session_hash(session_id),
        "hook": hook_safe,
        "bytes": bytes,
        "first_line_sha": first_line_sha,
        "first_line_head": first_line_head,
    });
    let _ = emit_r3_event_at(path, &event);
}

/// How an A2 offer closed (`closed_by` in the contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedBy {
    /// The classifier returned `Some` and the offer was consumed.
    Consumed,
    /// [`K_WINDOW`] Bash calls went by without a verdict.
    Window,
    /// A new offer in the same session replaced it.
    Replaced,
}

impl ClosedBy {
    /// Contract spelling.
    pub fn label(self) -> &'static str {
        match self {
            Self::Consumed => "consumed",
            Self::Window => "window",
            Self::Replaced => "replaced",
        }
    }
}

/// Everything an `a2_offer_closed` line carries (inputs of [`a2_offer_closed_event`]).
pub struct A2Closed<'a> {
    /// Raw session id (only its [`session_hash`] is written).
    pub session: &'a str,
    /// `code` | `both` | `native` — the presentation in force when the route went out.
    pub mode: &'a str,
    /// When the offer was made (secs since the epoch).
    pub offered_secs: u64,
    /// Why it closed.
    pub closed_by: ClosedBy,
    /// The Bash commands that followed (the first [`K_WINDOW`] are kept, each cut to
    /// [`MAX_TEXT_CHARS`]).
    pub cmd_heads: &'a [String],
    /// The `Some(v)` of `classify_route_outcome` that closed the offer; `None` if none.
    pub baseline_value: Option<f64>,
}

/// Milliseconds since the epoch (0 if the clock is before it).
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Build the `a2_offer_closed` line exactly as the contract spells it. Pure over its
/// input apart from the clock; the cap on text is enforced HERE (truncate) and again by
/// [`emit_r3_event_at`] (refuse).
pub fn a2_offer_closed_event(c: &A2Closed<'_>) -> Value {
    let heads: Vec<String> = c
        .cmd_heads
        .iter()
        .take(K_WINDOW)
        .map(|h| truncate_text(h))
        .collect();
    json!({
        "v": SCHEMA_VERSION,
        "kind": "a2_offer_closed",
        "use_case": "a2_outcome",
        "ts_ms": now_ms(),
        "session_hash": session_hash(c.session),
        "mode": c.mode,
        "offered_secs": c.offered_secs,
        "closed_by": c.closed_by.label(),
        "cmd_heads": heads,
        "baseline_value": c.baseline_value,
        "baseline_followed": c.baseline_value.is_some_and(|v| v > 0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_precedence_env_over_toml_default_off() {
        let on = "[shadow]\nr3 = true\n";
        let off = "[shadow]\nr3 = false\n";
        assert!(!resolve_flag(None, None), "default OFF");
        assert!(resolve_flag(None, Some(on)), "toml on");
        assert!(!resolve_flag(None, Some(off)));
        assert!(resolve_flag(Some("1"), Some(off)), "env on beats toml off");
        assert!(resolve_flag(Some("TRUE"), None));
        assert!(!resolve_flag(Some("0"), Some(on)), "env off beats toml on");
        assert!(!resolve_flag(Some("false"), Some(on)));
        // A value we do not understand is ignored, never read as ON.
        assert!(!resolve_flag(Some("sim"), None));
        assert!(
            resolve_flag(Some("sim"), Some(on)),
            "garbage env falls through to toml"
        );
    }

    #[test]
    fn toml_scan_reads_only_the_shadow_section() {
        assert_eq!(toml_declares_r3("[other]\nr3 = true\n"), None);
        assert_eq!(
            toml_declares_r3("[shadow]\n\n# c\nr3 = true # on\n"),
            Some(true)
        );
        assert_eq!(toml_declares_r3("[shadow]\nr3 = \"1\"\n"), Some(true));
        assert_eq!(toml_declares_r3("[shadow]\nr3 = maybe\n"), None);
        assert_eq!(
            toml_declares_r3("[shadow]\nother = 1\n[x]\nr3 = true\n"),
            None
        );
    }

    fn current_redirect() -> Option<PathBuf> {
        THREAD_EVENTS_PATH.with(|tl| tl.borrow().clone())
    }

    /// The doc promises "restore the previous setting" and an RAII scope (cross-audit 03/10/2026,
    /// finding E). Nesting must put the OUTER redirect back, not clear it.
    #[test]
    fn with_events_path_restores_the_previous_redirect_when_nested() {
        let outer = Path::new("/r3/outer.jsonl");
        let inner = Path::new("/r3/inner.jsonl");
        assert_eq!(current_redirect(), None);
        with_events_path(outer, || {
            with_events_path(inner, || {
                assert_eq!(current_redirect().as_deref(), Some(inner));
            });
            assert_eq!(
                current_redirect().as_deref(),
                Some(outer),
                "leaving the inner scope must put the outer redirect back, not clear it"
            );
        });
        assert_eq!(
            current_redirect(),
            None,
            "leaving the outermost scope clears the redirect"
        );
    }

    /// A panic inside the closure must not leave the redirect set on the thread.
    #[test]
    fn with_events_path_restores_the_redirect_when_the_closure_panics() {
        let path = Path::new("/r3/panics.jsonl");
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_events_path(path, || panic!("r3 probe: the closure panics on purpose"));
        }));
        assert!(caught.is_err(), "the closure's panic must still propagate");
        assert_eq!(
            current_redirect(),
            None,
            "a panic must not leave the redirect set"
        );
    }

    /// These tests assume the shadow flag is NOT forced by the environment. Fail LOUDLY otherwise: a guard that
    /// skips the body (`if env.is_none() { … }`) turns the observe-only proof into a green no-op when the shell
    /// carries `TOURING_R3_SHADOW` (cross-audit 02/10/2026, finding 2).
    fn require_flag_unset() {
        assert!(
            std::env::var_os(ENV_FLAG).is_none(),
            "run these tests with TOURING_R3_SHADOW unset"
        );
    }

    #[test]
    fn r3_flag_off_does_no_io() {
        // No toml, no env in this process ⇒ OFF, and the verdict is cached: the second
        // call must not need the file (delete-proof: the dir never existed).
        let root = Path::new("/nonexistent/r3-flag-off-probe");
        require_flag_unset();
        {
            assert!(!r3_shadow_enabled(root));
            assert!(!r3_shadow_enabled(root));
        }
        assert!(!root.exists(), "the probe path must not have been created");
    }

    #[test]
    fn toml_flag_is_read_once_per_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let toml = dir.path().join(".touring/touring.toml");
        std::fs::create_dir_all(toml.parent().expect("parent")).expect("mkdir");
        std::fs::write(&toml, "[shadow]\nr3 = true\n").expect("write");
        require_flag_unset();
        {
            assert!(r3_shadow_enabled(dir.path()));
            // Rewriting the file does not flip a cached verdict (documented: restart).
            std::fs::write(&toml, "[shadow]\nr3 = false\n").expect("rewrite");
            assert!(r3_shadow_enabled(dir.path()));
        }
    }

    #[test]
    fn session_hash_is_sha256_hex12() {
        // sha256("test") = 9f86d081884c7d65… — the contract's own example value.
        assert_eq!(session_hash("test"), "9f86d081884c");
        assert_eq!(session_hash("test").len(), 12);
        assert_ne!(session_hash("a"), session_hash("b"));
    }

    #[test]
    fn truncate_never_splits_a_code_point() {
        let long = "ç".repeat(200);
        let cut = truncate_text(&long);
        assert_eq!(cut.chars().count(), MAX_TEXT_CHARS);
        assert_eq!(truncate_text("abc"), "abc");
    }

    #[test]
    fn a2_event_matches_the_contract_shape() {
        let heads = vec![
            "touring run --lang bash --code 'ls'".to_string(),
            "x".repeat(200),
            "b".to_string(),
            "dropped-4th".to_string(),
        ];
        let e = a2_offer_closed_event(&A2Closed {
            session: "test",
            mode: "code",
            offered_secs: 7,
            closed_by: ClosedBy::Consumed,
            cmd_heads: &heads,
            baseline_value: Some(1.0),
        });
        assert_eq!(e["v"], 1);
        assert_eq!(e["kind"], "a2_offer_closed");
        assert_eq!(e["use_case"], "a2_outcome");
        assert_eq!(e["session_hash"], "9f86d081884c");
        assert_eq!(e["mode"], "code");
        assert_eq!(e["closed_by"], "consumed");
        assert_eq!(e["baseline_value"], 1.0);
        assert_eq!(e["baseline_followed"], true);
        let heads = e["cmd_heads"].as_array().expect("array");
        assert_eq!(heads.len(), K_WINDOW, "at most K heads");
        assert!(
            heads
                .iter()
                .all(|h| h.as_str().is_some_and(|s| s.chars().count() <= 80))
        );
        // closed schema: exactly these 11 keys, no raw session, no free text key.
        let mut keys: Vec<_> = e.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "baseline_followed",
                "baseline_value",
                "closed_by",
                "cmd_heads",
                "kind",
                "mode",
                "offered_secs",
                "session_hash",
                "ts_ms",
                "use_case",
                "v"
            ]
        );
        assert!(!e.to_string().contains("\"session_id\""));
    }

    #[test]
    fn a2_null_baseline_is_not_followed() {
        let e = a2_offer_closed_event(&A2Closed {
            session: "s",
            mode: "both",
            offered_secs: 0,
            closed_by: ClosedBy::Window,
            cmd_heads: &[],
            baseline_value: None,
        });
        assert!(e["baseline_value"].is_null());
        assert_eq!(e["baseline_followed"], false);
        // 0.0 is a verdict (bypass), distinct from null, and still not "followed".
        let z = a2_offer_closed_event(&A2Closed {
            session: "s",
            mode: "both",
            offered_secs: 0,
            closed_by: ClosedBy::Consumed,
            cmd_heads: &[],
            baseline_value: Some(0.0),
        });
        assert_eq!(z["baseline_value"], 0.0);
        assert_eq!(z["baseline_followed"], false);
    }

    #[test]
    fn emit_appends_one_line_per_event() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("logs/r3_events.jsonl");
        assert!(emit_r3_event_at(
            &path,
            &json!({"v": 1, "kind": "a2_offer_closed"})
        ));
        assert!(emit_r3_event_at(
            &path,
            &json!({"v": 1, "kind": "a1_block"})
        ));
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(text.lines().count(), 2);
        assert!(
            text.lines()
                .all(|l| serde_json::from_str::<Value>(l).is_ok())
        );
    }

    #[test]
    fn a_string_over_the_cap_is_refused_and_counted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("r3_events.jsonl");
        let before = dropped_event_count();
        let leaky = json!({"v": 1, "cmd_heads": ["y".repeat(MAX_TEXT_CHARS + 1)]});
        assert!(!emit_r3_event_at(&path, &leaky));
        assert!(!path.exists(), "a refused line must not reach the disk");
        assert!(dropped_event_count() > before);
        // exactly at the cap passes.
        let edge = json!({"v": 1, "cmd_heads": ["y".repeat(MAX_TEXT_CHARS)]});
        assert!(emit_r3_event_at(&path, &edge));
    }

    #[test]
    fn r3_emission_failure_never_changes_the_return() {
        // `/proc` cannot grow directories: the write MUST fail, and the failure must come
        // back as `false` + a counted drop — not a panic, not an `Err`.
        let before = dropped_event_count();
        let wrote = emit_r3_event_at(
            Path::new("/proc/r3-shadow-probe/r3_events.jsonl"),
            &json!({"v": 1}),
        );
        assert!(!wrote);
        assert!(
            dropped_event_count() > before,
            "the failure is counted, not silent"
        );
    }

    // ── A1 emitter tests ──────────────────────────────────────────────────────

    /// Helper: a temp dir with `[shadow] r3 = true` in its touring.toml.
    fn flag_on_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let toml = dir.path().join(".touring/touring.toml");
        std::fs::create_dir_all(toml.parent().expect("parent")).expect("mkdir");
        std::fs::write(&toml, "[shadow]\nr3 = true\n").expect("write toml");
        dir
    }

    /// Guard: flag OFF ⇒ zero I/O. Mutant: remove the `r3_shadow_enabled` early return
    /// → file would be created and the assertion would fail.
    #[test]
    fn a1_block_flag_off_no_io() {
        // Fail loudly if the env var is set — this test relies on the default-OFF path.
        assert!(
            std::env::var_os(ENV_FLAG).is_none(),
            "run without TOURING_R3_SHADOW"
        );
        // Use a fresh directory with no toml ⇒ default OFF.
        let dir = tempfile::tempdir().expect("tempdir");
        let events = dir.path().join("events.jsonl");
        maybe_emit_a1_block_at(
            &events,
            dir.path(),
            "test-session",
            "pre-bash",
            "some context",
        );
        assert!(!events.exists(), "flag OFF must not create the events file");
    }

    /// Guard: observe-only — ctx is unchanged after the call. Mutant: modify ctx inside
    /// the function → the assertion on the copy would fail.
    #[test]
    fn a1_block_flag_on_ctx_byte_identical() {
        let dir = flag_on_dir();
        let events = dir.path().join("events.jsonl");
        let ctx = "[TOURING SUGGEST · code-mode-loop · conf=0.95]\nsecond line of context";
        require_flag_unset();
        {
            let original = ctx.to_string();
            maybe_emit_a1_block_at(&events, dir.path(), "sess-abc", "pre-bash", ctx);
            assert_eq!(ctx, original.as_str(), "ctx must not be mutated");
            assert!(events.exists(), "flag ON must create the events file");
        }
    }

    /// Guard: write failure never panics and is counted. Mutant: propagate the error
    /// → the `?` or `unwrap` would panic and the test would see it.
    #[test]
    fn a1_block_write_failure_does_not_propagate() {
        let dir = flag_on_dir();
        let before = dropped_event_count();
        require_flag_unset();
        {
            // `/proc` cannot grow directories — write MUST fail.
            maybe_emit_a1_block_at(
                Path::new("/proc/r3-a1-probe/events.jsonl"),
                dir.path(),
                "sess",
                "pre-bash",
                "ctx",
            );
            assert!(
                dropped_event_count() > before,
                "write failure must be counted, not silent"
            );
        }
    }

    /// Guard: exactly ONE line per invocation. Mutant: emit 0 or 2 lines per call
    /// → the count assertion would fail.
    #[test]
    fn a1_block_exactly_one_line_per_invocation() {
        let dir = flag_on_dir();
        let events = dir.path().join("events.jsonl");
        require_flag_unset();
        {
            for _ in 0..3 {
                maybe_emit_a1_block_at(
                    &events,
                    dir.path(),
                    "sess",
                    "pre-bash",
                    "context text here",
                );
            }
            let text = std::fs::read_to_string(&events).expect("read");
            assert_eq!(text.lines().count(), 3, "exactly 1 line per invocation");
        }
    }

    /// Guard: event carries required fields with correct types. Mutant: remove a field
    /// or use the wrong type → one of the field assertions would fail.
    #[test]
    fn a1_block_event_has_required_fields() {
        let dir = flag_on_dir();
        let events = dir.path().join("events.jsonl");
        let ctx = "[TOURING SUGGEST · code-mode-loop · conf=0.95]\nsecond line";
        require_flag_unset();
        {
            maybe_emit_a1_block_at(&events, dir.path(), "test-session-xyz", "pre-bash", ctx);
            let text = std::fs::read_to_string(&events).expect("read");
            let line = text.lines().next().expect("at least one line");
            let ev: Value = serde_json::from_str(line).expect("valid JSON");

            assert_eq!(ev["v"], 1);
            assert_eq!(ev["kind"], "a1_block");
            assert_eq!(ev["use_case"], "a1_relevance");
            assert_eq!(ev["hook"], "pre-bash");
            assert!(ev["ts_ms"].is_u64(), "ts_ms must be a uint");
            assert!(ev["bytes"].is_u64(), "bytes must be a uint");

            let sha = ev["first_line_sha"]
                .as_str()
                .expect("first_line_sha is str");
            assert_eq!(sha.len(), 16, "SHA16_RE: exactly 16 hex chars");
            assert!(
                sha.chars().all(|c| c.is_ascii_hexdigit()),
                "SHA16_RE: all hex digits"
            );

            let session_h = ev["session_hash"].as_str().expect("session_hash str");
            assert_eq!(session_h.len(), 12, "session_hash: 12 hex chars");
            assert!(
                session_h.chars().all(|c| c.is_ascii_hexdigit()),
                "session_hash: all hex digits"
            );

            let head = ev["first_line_head"].as_str().expect("first_line_head str");
            assert!(
                head.chars().count() <= MAX_TEXT_CHARS,
                "first_line_head must be ≤ {} chars",
                MAX_TEXT_CHARS
            );

            // Closed schema: exactly 9 keys — an extra key would be a privacy violation.
            let mut keys: Vec<_> = ev
                .as_object()
                .expect("event is an object")
                .keys()
                .cloned()
                .collect();
            keys.sort();
            assert_eq!(
                keys,
                [
                    "bytes",
                    "first_line_head",
                    "first_line_sha",
                    "hook",
                    "kind",
                    "session_hash",
                    "ts_ms",
                    "use_case",
                    "v"
                ],
                "closed schema: exactly 9 known keys"
            );
        }
    }

    /// Guard: `first_line_sha` is sha256 of the FIRST LINE (not the whole context),
    /// `bytes` equals `ctx.len()` (not `first_line.len()`), and `first_line_head` is a
    /// prefix of the first line capped at [`MAX_TEXT_CHARS`] chars.
    ///
    /// Three mutants killed by the VALUE assertions here:
    ///   - `sha_over_whole_ctx`: `Sha256::digest(ctx.as_bytes())` → sha differs from
    ///     what is computed here independently from the first line;
    ///   - `bytes_of_first_line`: `bytes = first_line.len() as u64` → bytes differ
    ///     from ctx_a.len() and ctx_b.len();
    ///   - `head_over_whole_ctx`: `truncate_text(ctx)` → head is not a prefix of first
    ///     line (it would be a prefix of the whole context string).
    #[test]
    fn a1_block_values_are_correct() {
        assert!(
            std::env::var_os(ENV_FLAG).is_none(),
            "run without TOURING_R3_SHADOW"
        );
        let dir = flag_on_dir();
        let events = dir.path().join("events_val.jsonl");

        // Two contexts with the SAME first line but different bodies.
        let first_line = "[TOURING SUGGEST · code-mode-loop · conf=0.95]";
        let ctx_a = format!("{first_line}\nbody-alpha here — 36 extra chars of body");
        let ctx_b =
            format!("{first_line}\nbody-beta is longer and differs from alpha by quite a lot");

        maybe_emit_a1_block_at(&events, dir.path(), "sess-val", "pre-bash", &ctx_a);
        maybe_emit_a1_block_at(&events, dir.path(), "sess-val", "pre-bash", &ctx_b);

        let text = std::fs::read_to_string(&events).expect("read events");
        let evs: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).expect("valid JSON"))
            .collect();
        assert_eq!(evs.len(), 2, "two events");

        // Compute the expected sha INDEPENDENTLY — NOT reusing the function under test.
        let expected_sha: String = {
            let d = Sha256::digest(first_line.as_bytes());
            d.iter().take(8).map(|b| format!("{b:02x}")).collect()
        };

        // Both events must carry the sha of the FIRST LINE (same first line → same sha).
        assert_eq!(
            evs[0]["first_line_sha"], expected_sha,
            "first_line_sha must be sha256(first_line)[..8 bytes] for event a"
        );
        assert_eq!(
            evs[1]["first_line_sha"], expected_sha,
            "first_line_sha must be sha256(first_line)[..8 bytes] for event b"
        );

        // `bytes` must equal ctx.len() — the WHOLE context, not just the first line.
        assert_eq!(
            evs[0]["bytes"].as_u64().unwrap(),
            ctx_a.len() as u64,
            "bytes must equal ctx_a.len()"
        );
        assert_eq!(
            evs[1]["bytes"].as_u64().unwrap(),
            ctx_b.len() as u64,
            "bytes must equal ctx_b.len()"
        );

        // Same first line → same sha; different bodies → different bytes.
        assert_ne!(
            evs[0]["bytes"], evs[1]["bytes"],
            "same first line but different bodies ⇒ bytes must differ"
        );

        // first_line_head is a prefix of the first line, capped at MAX_TEXT_CHARS.
        // (first_line is 46 chars < 80 so head == first_line here.)
        for ev in &evs {
            let head = ev["first_line_head"].as_str().expect("first_line_head str");
            assert!(
                first_line.starts_with(head),
                "first_line_head must be a prefix of the first line"
            );
            assert!(
                head.chars().count() <= MAX_TEXT_CHARS,
                "first_line_head must be ≤ {MAX_TEXT_CHARS} chars"
            );
        }
    }

    /// Guard: no emission when the hook name is empty (schema: 0 < len). Mutant: emit
    /// for an empty hook → the schema check in validation would catch it, but here we
    /// verify the emitter caps at 40 chars (chars().take(40) on an empty string produces
    /// an empty string — the text-cap gate in emit_r3_event_at does not refuse it, but
    /// `validate_event` in Python would; this test documents the Rust-side behaviour).
    ///
    /// Guards the "no double-count between A1 and A2" property by construction: A1 is
    /// emitted by `maybe_emit_a1_block_at` and A2 by `a2_offer_closed_event`; they write
    /// to the same file but with different `kind` fields, so a reader that filters by
    /// `kind` sees each at most once per invocation.
    #[test]
    fn a1_and_a2_kinds_are_distinct() {
        let dir = flag_on_dir();
        let events = dir.path().join("events.jsonl");
        require_flag_unset();
        {
            // Emit one A1 and one A2.
            maybe_emit_a1_block_at(&events, dir.path(), "sess", "pre-bash", "ctx");
            let a2_ev = a2_offer_closed_event(&A2Closed {
                session: "sess",
                mode: "code",
                offered_secs: 0,
                closed_by: ClosedBy::Window,
                cmd_heads: &[],
                baseline_value: None,
            });
            assert!(emit_r3_event_at(&events, &a2_ev));

            let text = std::fs::read_to_string(&events).expect("read");
            let kinds: Vec<_> = text
                .lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .map(|e| e["kind"].as_str().unwrap_or("").to_string())
                .collect();
            assert_eq!(kinds.len(), 2, "two events total");
            assert!(kinds.contains(&"a1_block".to_string()), "a1_block present");
            assert!(
                kinds.contains(&"a2_offer_closed".to_string()),
                "a2_offer_closed present"
            );
        }
    }

    /// S1a (Errata 5, E5.2): emission cost must be bounded.
    /// Run with: `cargo test -p touring-hook-runtime -- --ignored --nocapture r3_emission_cost_is_bounded`
    #[test]
    #[ignore]
    fn r3_emission_cost_is_bounded() {
        use std::time::Instant;

        let n_emit: usize = 10_000;
        let n_off: usize = 100_000;

        // ── OFF-path cost: var_os + OnceLock + Mutex + HashMap lookup ──────────
        // Use a nonexistent dir so the toml read fails immediately and is cached.
        let off_root = std::path::Path::new("/nonexistent/r3-cost-off-probe-s1a");
        // Warm-up: populate the cache for this path (file I/O happens once).
        require_flag_unset();
        {
            let _ = r3_shadow_enabled(off_root);
        }
        let mut off_ns: Vec<u64> = Vec::with_capacity(n_off);
        for _ in 0..n_off {
            let t = Instant::now();
            // Even with ENV_FLAG unset, this is the hot path (env lookup + map hit).
            let _enabled = if std::env::var_os(ENV_FLAG).is_none() {
                r3_shadow_enabled(off_root)
            } else {
                false
            };
            off_ns.push(t.elapsed().as_nanos() as u64);
        }
        off_ns.sort_unstable();
        let p50_off_ns = off_ns[n_off * 50 / 100];
        let p99_off_ns = off_ns[n_off * 99 / 100];
        println!("OFF  p50={p50_off_ns} ns   p99={p99_off_ns} ns");

        // ── Emission-path cost ────────────────────────────────────────────────
        let dir = flag_on_dir();
        let events = dir.path().join("r3_cost_events.jsonl");
        let ctx =
            "[TOURING SUGGEST · code-mode-loop · conf=0.95]\nsome context text for s1a cost test";
        let mut emit_ns: Vec<u64> = Vec::with_capacity(n_emit);

        require_flag_unset();
        {
            // Warm-up pass (populates toml cache + creates the file).
            maybe_emit_a1_block_at(&events, dir.path(), "sess-s1a", "pre-bash", ctx);
            emit_ns.clear();

            for _ in 0..n_emit {
                let t = Instant::now();
                maybe_emit_a1_block_at(&events, dir.path(), "sess-s1a", "pre-bash", ctx);
                emit_ns.push(t.elapsed().as_nanos() as u64);
            }
            emit_ns.sort_unstable();
            let p50_emit_us = emit_ns[n_emit * 50 / 100] / 1_000;
            let p99_emit_us = emit_ns[n_emit * 99 / 100] / 1_000;
            println!("EMIT p50={p50_emit_us} µs  p99={p99_emit_us} µs");

            assert!(
                p99_off_ns <= 50_000,
                "OFF p99 {p99_off_ns} ns exceeds 50 µs budget"
            );
            assert!(
                p99_emit_us <= 1_000,
                "EMIT p99 {p99_emit_us} µs exceeds 1 ms budget"
            );
        }
    }
}
