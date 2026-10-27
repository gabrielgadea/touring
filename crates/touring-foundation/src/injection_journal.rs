//! Journal of what the injection policy CUT (S-3, 27/10/2026).
//!
//! WHY A FILE
//! ----------
//! The policy ([`crate::injection_policy`]) runs inside EPHEMERAL hook
//! processes: one `touring-hook` per invocation, gone in a few milliseconds.
//! An in-memory counter dies with the process, and the daemon's
//! `gate_metrics` atomics never see a cut made in the client. The established
//! cross-process channel of this repository is an append-only JSONL under
//! `~/.claude/touring/` that `touring kpi` reads (`run_subcalls.jsonl`,
//! `sdk_signal_mirror.jsonl`, `r3_shadow.jsonl`); this module is that channel
//! for the policy.
//!
//! WHAT IS RECORDED
//! ----------------
//! One line per application of the policy that actually cut something —
//! **never** one per invocation: at the default level (`full`) nothing is
//! written and nothing is computed, and at `no-advisory` a hook that carried
//! only facts writes nothing either. A line is:
//!
//! ```text
//! {"ts":<unix ms>,"level":"no-advisory","site":"daemon-forward",
//!  "session":"<id, when known>","cuts":[{"family":"touring-suggest","bytes":412}]}
//! ```
//!
//! Absence of lines therefore means "nothing was cut", and it is NOT evidence
//! of the level: the level is on every line that exists.
//!
//! GUARANTEES
//! ----------
//! - **Fail-open**: every I/O failure is an `Err` the caller discards; a hook
//!   never loses its answer because the journal could not be written.
//! - **Atomic append**: a line is built whole and written with ONE `write_all`
//!   on an `O_APPEND` descriptor, so concurrent hook processes never
//!   interleave bytes inside a line (a line stays far below `PIPE_BUF`).
//! - **Bounded**: past [`ROTATE_BYTES`] the file is renamed to `<name>.1`
//!   (overwriting the previous rotation) and a fresh one starts; the reader
//!   looks at the last [`READ_TAIL_BYTES`] only.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// File name of the journal under `<home>/.claude/touring/`.
pub const JOURNAL_FILE: &str = "injection_suppressed.jsonl";

/// Size past which the writer rotates the journal to `<name>.1`.
pub const ROTATE_BYTES: u64 = 8 * 1024 * 1024;

/// How much of the journal's tail the summary reads.
pub const READ_TAIL_BYTES: u64 = 4 * 1024 * 1024;

/// One piece of context the policy removed: the PRIMARY family of the removed
/// text and its size in bytes. The sum of `bytes` over a journal is exactly
/// the number of bytes kept out of the model's window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    /// Primary family of the removed text (`injection_family::injection_family`).
    pub family: &'static str,
    /// Size of the removed text, in bytes.
    pub bytes: usize,
}

/// `<home>/.claude/touring/injection_suppressed.jsonl` — the same root the
/// other kpi journals use.
#[must_use]
pub fn journal_path_in(home: &Path) -> PathBuf {
    home.join(".claude").join("touring").join(JOURNAL_FILE)
}

/// The journal under `$HOME`, when `HOME` is set.
#[must_use]
pub fn journal_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| journal_path_in(Path::new(&home)))
}

/// The Claude Code session this process answers, recorded once by the
/// `touring-hook` client from the payload. A process that never calls
/// [`set_session`] (the daemon, a test) journals its lines unattributed.
static SESSION: OnceLock<String> = OnceLock::new();

/// Record the session id of this hook process. First call wins; an empty id
/// records nothing (no session is not a session named "").
pub fn set_session(id: &str) {
    if !id.is_empty() {
        let _ = SESSION.set(id.to_string());
    }
}

fn session() -> Option<&'static str> {
    SESSION.get().map(String::as_str)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The journal line (with its trailing newline) for one application of the
/// policy. Pure: the caller supplies the clock and the session.
#[must_use]
pub fn line_for(
    level: &str,
    site: &str,
    session: Option<&str>,
    cuts: &[Cut],
    ts_ms: u64,
) -> String {
    let mut line = serde_json::json!({
        "ts": ts_ms,
        "level": level,
        "site": site,
        "cuts": cuts
            .iter()
            .map(|c| serde_json::json!({"family": c.family, "bytes": c.bytes}))
            .collect::<Vec<_>>(),
    });
    if let (Some(id), Some(map)) = (session, line.as_object_mut()) {
        map.insert(
            "session".to_string(),
            serde_json::Value::String(id.to_string()),
        );
    }
    let mut text = line.to_string();
    text.push('\n');
    text
}

/// Rotate the journal when it outgrew [`ROTATE_BYTES`].
fn rotate_if_needed(path: &Path) {
    let outgrew = std::fs::metadata(path).is_ok_and(|m| m.len() > ROTATE_BYTES);
    if outgrew {
        let mut rotated = path.as_os_str().to_owned();
        rotated.push(".1");
        let _ = std::fs::rename(path, PathBuf::from(rotated));
    }
}

/// Append one line for `cuts` to the journal at `path`, stamped with the
/// current time and this process's session. `Err` is for the caller to
/// discard (fail-open) — and for tests to assert on.
///
/// # Errors
/// Any I/O error from creating the directory or appending the line.
pub fn record(path: &Path, level: &str, site: &str, cuts: &[Cut]) -> std::io::Result<()> {
    let line = line_for(level, site, session(), cuts, now_ms());
    append_line(path, &line)
}

/// The append itself — one `write_all` on an `O_APPEND` descriptor.
fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    rotate_if_needed(path);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())
}

/// Totals for one family.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FamilyTotal {
    /// Number of cuts of this family.
    pub count: u64,
    /// Bytes kept out of the window by those cuts.
    pub bytes: u64,
}

/// What a journal says, scoped to a set of sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// Lines attributed to a session of the scope (the ones counted below).
    pub lines: u64,
    /// Cuts in those lines.
    pub count: u64,
    /// Bytes cut in those lines.
    pub bytes: u64,
    /// Totals per primary family.
    pub by_family: BTreeMap<String, FamilyTotal>,
    /// Bytes cut per application site.
    pub by_site: BTreeMap<String, u64>,
    /// Lines per level that was in force when the cut was made.
    pub by_level: BTreeMap<String, u64>,
    /// Bytes cut per session.
    pub by_session: BTreeMap<String, u64>,
    /// Lines with no session (cuts made by the daemon, or by a hook whose
    /// payload carried no id). Reported apart, never folded into the totals.
    pub unattributed_lines: u64,
    /// Bytes of those lines.
    pub unattributed_bytes: u64,
    /// Lines that belong to a session outside the scope (other projects).
    pub out_of_scope_lines: u64,
    /// Lines that did not parse — a number that says the read was not clean.
    pub malformed_lines: u64,
}

fn cut_bytes(cut: &serde_json::Value) -> u64 {
    cut.get("bytes")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

/// Fold the journal `text` into a [`Summary`].
///
/// `scope = Some(sessions)` keeps only lines whose session is in the set (a
/// project's transcripts); `None` keeps every attributed line. A line with no
/// session is never in scope: it is counted in `unattributed_*` instead.
#[must_use]
pub fn summarize(text: &str, scope: Option<&BTreeSet<String>>) -> Summary {
    let mut out = Summary::default();
    for raw in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(line) = serde_json::from_str::<serde_json::Value>(raw) else {
            out.malformed_lines += 1;
            continue;
        };
        let cuts = line
            .get("cuts")
            .and_then(serde_json::Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let bytes: u64 = cuts.iter().map(cut_bytes).sum();
        let Some(session) = line.get("session").and_then(serde_json::Value::as_str) else {
            out.unattributed_lines += 1;
            out.unattributed_bytes += bytes;
            continue;
        };
        if scope.is_some_and(|set| !set.contains(session)) {
            out.out_of_scope_lines += 1;
            continue;
        }
        out.lines += 1;
        out.bytes += bytes;
        out.count += cuts.len() as u64;
        *out.by_session.entry(session.to_string()).or_default() += bytes;
        if let Some(site) = line.get("site").and_then(serde_json::Value::as_str) {
            *out.by_site.entry(site.to_string()).or_default() += bytes;
        }
        if let Some(level) = line.get("level").and_then(serde_json::Value::as_str) {
            *out.by_level.entry(level.to_string()).or_default() += 1;
        }
        for cut in cuts {
            let family = cut
                .get("family")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("other");
            let total = out.by_family.entry(family.to_string()).or_default();
            total.count += 1;
            total.bytes += cut_bytes(cut);
        }
    }
    out
}

/// The last [`READ_TAIL_BYTES`] of the journal as text, starting on a line
/// boundary. `None` when the file does not exist or cannot be read — the
/// reader must say "no journal", not "zero cuts".
#[must_use]
pub fn read_tail(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(READ_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(READ_TAIL_BYTES).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if start == 0 {
        return Some(text);
    }
    // The tail began mid-line: drop the partial first line.
    Some(
        text.split_once('\n')
            .map_or_else(String::new, |(_, rest)| rest.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut(family: &'static str, bytes: usize) -> Cut {
        Cut { family, bytes }
    }

    #[test]
    fn the_line_names_level_site_session_and_every_cut() {
        let line = line_for(
            "no-advisory",
            "daemon-forward",
            Some("sess-1"),
            &[cut("touring-suggest", 412), cut("prompt-enhance", 88)],
            1_000,
        );
        assert!(line.ends_with('\n'), "one line, newline-terminated");
        let v: serde_json::Value = serde_json::from_str(line.trim_end()).expect("valid JSON");
        assert_eq!(v["ts"], 1_000);
        assert_eq!(v["level"], "no-advisory");
        assert_eq!(v["site"], "daemon-forward");
        assert_eq!(v["session"], "sess-1");
        assert_eq!(v["cuts"][0]["family"], "touring-suggest");
        assert_eq!(v["cuts"][1]["bytes"], 88);
    }

    #[test]
    fn a_line_without_a_session_has_no_session_key() {
        let line = line_for("off", "verdict", None, &[cut("pre-bash", 5)], 1);
        let v: serde_json::Value = serde_json::from_str(line.trim_end()).expect("valid JSON");
        assert!(v.get("session").is_none(), "{line}");
    }

    #[test]
    fn record_appends_whole_lines_and_creates_the_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = journal_path_in(dir.path());
        record(&path, "off", "emit-runtime", &[cut("touring-suggest", 10)]).expect("first");
        record(&path, "off", "emit-core", &[cut("prompt-enhance", 20)]).expect("second");
        let text = std::fs::read_to_string(&path).expect("journal exists");
        assert_eq!(text.lines().count(), 2, "{text}");
        for l in text.lines() {
            serde_json::from_str::<serde_json::Value>(l).expect("each line parses alone");
        }
    }

    #[test]
    fn record_to_an_unwritable_path_is_an_error_not_a_panic() {
        // A path whose parent is a regular file cannot be created: the error is
        // returned for the caller to discard — the hook keeps its answer.
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("plain");
        std::fs::write(&file, "x").expect("write");
        assert!(record(&file.join("nested").join("j.jsonl"), "off", "s", &[]).is_err());
    }

    #[test]
    fn an_oversized_journal_rotates_and_the_fresh_file_has_only_the_new_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = journal_path_in(dir.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let big = std::fs::File::create(&path).expect("create");
        big.set_len(ROTATE_BYTES + 1).expect("sparse grow");
        drop(big);
        record(&path, "off", "emit-core", &[cut("pre-bash", 7)]).expect("record");
        let rotated = PathBuf::from(format!("{}.1", path.display()));
        assert!(rotated.is_file(), "the old journal is kept as .1");
        assert_eq!(
            std::fs::read_to_string(&path)
                .expect("fresh")
                .lines()
                .count(),
            1
        );
    }

    fn journal() -> String {
        [
            line_for(
                "no-advisory",
                "daemon-forward",
                Some("a"),
                &[cut("touring-suggest", 100), cut("prompt-enhance", 50)],
                1,
            ),
            line_for(
                "no-advisory",
                "emit-runtime",
                Some("a"),
                &[cut("touring-suggest", 30)],
                2,
            ),
            line_for("off", "verdict", Some("b"), &[cut("pre-bash", 7)], 3),
            line_for("off", "verdict", None, &[cut("pre-bash", 900)], 4),
            line_for(
                "off",
                "emit-core",
                Some("elsewhere"),
                &[cut("touring-suggest", 5000)],
                5,
            ),
            "{ not json\n".to_string(),
        ]
        .concat()
    }

    #[test]
    fn the_summary_totals_by_family_site_level_and_session() {
        let s = summarize(&journal(), None);
        assert_eq!(s.lines, 4, "every attributed line when unscoped");
        assert_eq!(s.count, 5);
        assert_eq!(s.bytes, 100 + 50 + 30 + 7 + 5000);
        assert_eq!(
            s.by_family["touring-suggest"],
            FamilyTotal {
                count: 3,
                bytes: 5130
            }
        );
        assert_eq!(
            s.by_family["prompt-enhance"],
            FamilyTotal {
                count: 1,
                bytes: 50
            }
        );
        assert_eq!(s.by_site["verdict"], 7);
        assert_eq!(s.by_level["no-advisory"], 2);
        assert_eq!(s.by_session["a"], 180);
        assert_eq!(
            s.malformed_lines, 1,
            "a broken line is counted, not dropped silently"
        );
    }

    #[test]
    fn a_scoped_summary_keeps_only_the_projects_sessions_and_reports_the_rest_apart() {
        let scope: BTreeSet<String> = ["a", "b"].iter().map(|s| (*s).to_string()).collect();
        let s = summarize(&journal(), Some(&scope));
        assert_eq!(
            s.bytes,
            100 + 50 + 30 + 7,
            "the other project's 5000 bytes are not ours"
        );
        assert_eq!(s.out_of_scope_lines, 1);
        assert_eq!(s.unattributed_lines, 1);
        assert_eq!(
            s.unattributed_bytes, 900,
            "a daemon-side cut is shown, never folded in"
        );
        assert_eq!(
            s.by_family["touring-suggest"],
            FamilyTotal {
                count: 2,
                bytes: 130
            }
        );
    }

    #[test]
    fn the_tail_reader_drops_the_partial_first_line_and_says_none_without_a_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("j.jsonl");
        assert_eq!(read_tail(&path), None, "no file is not zero cuts");
        let big = "x".repeat(usize::try_from(READ_TAIL_BYTES).expect("fits") + 10);
        std::fs::write(
            &path,
            format!(
                "{big}\n{}",
                line_for("off", "s", Some("a"), &[cut("pre-bash", 1)], 1)
            ),
        )
        .expect("write");
        let tail = read_tail(&path).expect("read");
        assert_eq!(
            tail.lines().count(),
            1,
            "the half line is gone: {}",
            &tail[..40.min(tail.len())]
        );
        assert_eq!(summarize(&tail, None).bytes, 1);
    }
}
