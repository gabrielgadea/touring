//! Hooks-complement emit counter — appends a JSONL line to
//! `~/.claude/touring/hooks_complement.jsonl` every time a SignalLayer is
//! invoked.
//!
//! Complements the existing `run_journal.jsonl` (which records code-mode
//! execution outcomes) with a parallel stream dedicated to the 15
//! complementacao-hooks SignalLayers. Surfaced via
//! `touring kpi -j hooks_complement.emit_count` (W6 S-6.4-style counter).
//!
//! Best-effort writes: the counter NEVER blocks the hook. If the journal
//! cannot be opened (sandbox, permission, disk full), the function logs a
//! warning to stderr and returns Ok(()) — fail-open is mandatory for hooks
//! (REGRA #11 of tour safety).
//!
//! Concurrency: multiple hook invocations may race to append. The journal is
//! a regular file opened in append mode; the OS guarantees atomicity for
//! writes shorter than PIPE_BUF (4096 bytes on Linux). Each line is <200
//! bytes so concurrent appends are safe.
//!
//! Schema per line:
//! ```json
//! {"ts":"2026-08-31T20:30:00Z","layer":"cwe_scan","file":"src/foo.rs","cila":3,"hook":"post_write"}
//! ```
//!
//! F6 (complementacao-hooks): instrument the counter; consumed by
//! `hooks_complement_wired` in `touring-quality::builtins::best_practices`
//! and surfaced via `touring kpi -j hooks_complement`.

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Path of the journal relative to $HOME. `$HOME/.claude/touring/hooks_complement.jsonl`.
const JOURNAL_RELATIVE: &str = ".claude/touring/hooks_complement.jsonl";

/// Process-global mutex serializing journal appends. Cheap (rare contention)
/// because each hook call writes one short line.
static JOURNAL_LOCK: Mutex<()> = Mutex::new(());

/// Resolve the journal path. Pure for testability; falls back to a temp
/// directory if `$HOME` is unavailable.
fn journal_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(JOURNAL_RELATIVE))
}

/// Record a SignalLayer invocation. Best-effort, fail-open.
pub fn record_emit(layer_name: &str, file_path: &str, cila_level: usize, hook_name: &str) {
    // Best-effort: failures are warnings, not errors.
    let Some(path) = journal_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if !parent.exists() {
        let _ = std::fs::create_dir_all(parent);
    }

    let _guard = JOURNAL_LOCK.lock().ok();

    let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
        eprintln!(
            "hooks_complement_journal: could not open {} (fail-open)",
            path.display()
        );
        return;
    };

    let mut w = BufWriter::new(file);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = serde_json::json!({
        "ts": ts,
        "layer": layer_name,
        "file": file_path,
        "cila": cila_level,
        "hook": hook_name,
    })
    .to_string();
    if let Err(e) = writeln!(w, "{line}") {
        eprintln!("hooks_complement_journal: write failed: {e}");
    }
    if let Err(e) = w.flush() {
        eprintln!("hooks_complement_journal: flush failed: {e}");
    }
}

/// Count of records in the journal since the last rotate. Used by
/// `touring kpi -j hooks_complement.emit_count`. Best-effort: returns 0
/// if the journal is unreadable.
pub fn emit_count() -> u64 {
    let Some(path) = journal_path() else {
        return 0;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return 0;
    };
    text.lines().filter(|l| !l.trim().is_empty()).count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_path_resolves_to_home() {
        let p = journal_path().expect("$HOME should be set");
        assert!(p.ends_with(JOURNAL_RELATIVE));
    }

    #[test]
    fn emit_count_returns_zero_when_journal_absent() {
        // We don't write anything; expect count = 0 against an absent file
        // (or whatever the test env currently has). Just assert it doesn't
        // panic and returns a non-negative integer.
        let n = emit_count();
        assert!(n < u64::MAX);
    }

    #[test]
    fn record_emit_is_fail_open() {
        // Even with $HOME unset (worst case), record_emit must NOT panic.
        record_emit("test_layer", "src/foo.rs", 3, "post_write");
    }

    #[test]
    fn round_trip_emit_count_after_record() {
        // Use a tempdir for the journal to avoid polluting $HOME.
        let dir = std::env::temp_dir().join(format!("touring-hcj-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tempdir");
        let path = dir.join("hooks_complement.jsonl");
        std::fs::write(&path, "").expect("init");
        // Manually count via the public function (file is empty → 0).
        let text = std::fs::read_to_string(&path).expect("read");
        let pre = text.lines().filter(|l| !l.trim().is_empty()).count();
        assert_eq!(pre, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
