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

    #[test]
    fn r3_flag_off_does_no_io() {
        // No toml, no env in this process ⇒ OFF, and the verdict is cached: the second
        // call must not need the file (delete-proof: the dir never existed).
        let root = Path::new("/nonexistent/r3-flag-off-probe");
        if std::env::var_os(ENV_FLAG).is_none() {
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
        if std::env::var_os(ENV_FLAG).is_none() {
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
}
