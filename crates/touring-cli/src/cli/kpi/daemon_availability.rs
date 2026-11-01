//! S73 (2026-09-26) — daemon availability from the hook trace's TRUE `exit_reason`.
//!
//! Every `touring-hook` invocation writes one line to `~/.claude/touring/hook_trace.jsonl`
//! with the reason it exited (S14 made that reason true before the process exits). The
//! ratio a daemon outage moves is: of the invocations that ASKED the daemon, how many got
//! a usable answer.

use serde_json::{Value, json};
use std::path::Path;

/// The window the plan's acceptance names (`daemon_availability >= 0.95 em 48 h`).
pub(crate) const WINDOW_MS: u64 = 48 * 3_600_000;
/// The floor the plan's acceptance names.
pub(crate) const FLOOR: f64 = 0.95;

/// Live entry: the trace under `$HOME`, the last [`WINDOW_MS`].
pub(crate) fn daemon_availability() -> Value {
    let Some(home) = std::env::var_os("HOME") else {
        return Value::Null;
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    daemon_availability_from(
        &Path::new(&home).join(".claude/touring/hook_trace.jsonl"),
        now_ms,
        WINDOW_MS,
    )
}

/// The daemon gave a usable answer.
const ANSWERED: &[&str] = &["daemon-json", "daemon-empty"];
/// The daemon answered with something the client could not use (it emitted `{}` instead).
const UNUSABLE: &[&str] = &["daemon-nonjson"];
/// The daemon was asked and could not be reached; the hook fell back to standalone.
const UNAVAILABLE: &[&str] = &["daemon-unavailable"];

/// Testable core (the wrapper resolves `$HOME`, process-global and unsafe to mutate in
/// parallel tests): ratio over the records of `path` newer than `now_ms - window_ms`.
///
/// Numerator: invocations the daemon answered usefully. Denominator: every invocation
/// that ASKED the daemon — answered, unusable answer, unreachable. Routes that never ask
/// it by design (`stateless`, standalone-only subcommands) are outside the ratio, as are
/// the contract suite's sessions. Nothing that asked the daemon ⇒ `null`, never 1.0.
/// First live reading (26/09/2026): 77 400 / 79 045 = 0.979 over 48 h, restarts included.
pub(crate) fn daemon_availability_from(path: &Path, now_ms: u64, window_ms: u64) -> Value {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Value::Null;
    };
    let since = now_ms.saturating_sub(window_ms);
    let (mut answered, mut unusable, mut unavailable) = (0_u64, 0_u64, 0_u64);
    for record in text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        let in_window = record["ts_ms"].as_u64().is_some_and(|ts| ts >= since);
        // The contract suite's synthetic sessions are not real traffic — one predicate
        // with the learning side (touring_foundation::synthetic_session).
        let synthetic = record["session_id"]
            .as_str()
            .is_some_and(touring_foundation::synthetic_session::is_synthetic_session_id);
        if !in_window || synthetic {
            continue;
        }
        let reason = record["exit_reason"].as_str().unwrap_or_default();
        if ANSWERED.contains(&reason) {
            answered += 1;
        } else if UNUSABLE.contains(&reason) {
            unusable += 1;
        } else if UNAVAILABLE.contains(&reason) {
            unavailable += 1;
        }
    }
    let asked = answered + unusable + unavailable;
    if asked == 0 {
        return Value::Null;
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = answered as f64 / asked as f64;
    json!({
        "ratio": (ratio * 10_000.0).round() / 10_000.0,
        "answered": answered,
        "unusable": unusable,
        "unavailable": unavailable,
        "window_h": window_ms / 3_600_000,
        "floor": FLOOR,
        "meets_floor": ratio >= FLOOR,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn trace(lines: &[(u64, &str, &str, &str)]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("trace");
        for (ts, route, reason, session) in lines {
            writeln!(
                f,
                "{}",
                json!({"ts_ms": ts, "hook": "pre-bash", "route": route, "exit_reason": reason, "session_id": session})
            )
            .expect("write");
        }
        f.write_all(b"not json at all\n").expect("junk line");
        f
    }

    #[test]
    fn availability_counts_what_asked_the_daemon_and_nothing_else() {
        let now = 10 * WINDOW_MS;
        let mut lines = Vec::new();
        for _ in 0..8 {
            lines.push((now - 1, "daemon", "daemon-json", "s"));
        }
        lines.push((now - 1, "daemon", "daemon-empty", "s"));
        lines.push((now - 1, "daemon", "daemon-nonjson", "s"));
        lines.push((now - 1, "standalone", "daemon-unavailable", "s"));
        lines.push((now - 1, "standalone", "daemon-unavailable", "s"));
        // never asked the daemon, by design
        for _ in 0..5 {
            lines.push((now - 1, "standalone", "standalone-ok", "s"));
        }
        lines.push((now - 1, "stateless", "prompt-enhance", "s"));
        // the contract suite's synthetic sessions and records outside the window
        lines.push((
            now - 1,
            "standalone",
            "daemon-unavailable",
            "hooks-contract-x",
        ));
        lines.push((now - WINDOW_MS - 1, "standalone", "daemon-unavailable", "s"));
        let f = trace(&lines);

        let got = daemon_availability_from(f.path(), now, WINDOW_MS);
        assert_eq!(got["answered"], 9, "{got}");
        assert_eq!(got["unusable"], 1, "{got}");
        assert_eq!(got["unavailable"], 2, "{got}");
        assert_eq!(got["ratio"], 0.75, "9 / (9 + 1 + 2): {got}");
        assert_eq!(got["floor"], FLOOR, "{got}");
        assert_eq!(got["window_h"], 48, "{got}");
    }

    #[test]
    fn nothing_that_asked_the_daemon_is_null_never_a_ratio() {
        let now = 10 * WINDOW_MS;
        let f = trace(&[(now - 1, "standalone", "standalone-ok", "s")]);
        assert_eq!(
            daemon_availability_from(f.path(), now, WINDOW_MS),
            Value::Null
        );
        assert_eq!(
            daemon_availability_from(Path::new("/nonexistent/trace.jsonl"), now, WINDOW_MS),
            Value::Null
        );
    }
}
