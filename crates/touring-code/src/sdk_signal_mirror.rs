//! SDK signal mirror — the append-only JSONL sink of hook calls, behind ONE interface.
//!
//! ## Why a mirror
//!
//! The `touring run` journal (`~/.claude/touring/run_journal.jsonl`) records the
//! program execution envelope (lang, exit, duration, bytes). It does NOT record
//! which hooks a program called — those calls live in the daemon's RPC traffic
//! and never reach the journal file. This module is the **bridge**: every
//! observed hook call (a native `touring <hook>` command seen by PostToolUse, or
//! an in-sandbox SDK query) gets one JSON line in `sdk_signal_mirror.jsonl`.
//!
//! ## One interface, the format behind it
//!
//! [`Mirror::record`] writes a call; [`Mirror::read`] returns the aggregate
//! ([`MirrorAggregate`]). Where the file lives, the line format (`ts`,
//! `hook_name`, `duration_ms`, `success`, `origin`), which names count as
//! canonical (`HookName::ALL`), the percentile math and the `ts` window are
//! all decided here — no caller parses a mirror line or builds the path.
//! Same shape as the sibling `touring_foundation::run_outcome::Journal`.
//!
//! ## Why append-only
//!
//! 1. Crash safety — a torn write loses a line, not the whole file; the reader
//!    SKIPS a line it cannot read (not UTF-8, not JSON, no string `hook_name`) and
//!    COUNTS it in `skipped_lines`, so one torn line never hides the rest and never
//!    disappears silently.
//! 2. The mirror is a journal, not a state; the aggregate is re-derived on read.
//!
//! The in-sandbox Python SDK (`touring-server`'s `run.rs` template) appends the
//! same line format itself; `MirrorOrigin::as_str` is the shared constant a test
//! there pins.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::journal::percentiles;
use crate::sdk::{HookName, parse_hook};

/// File name of the mirror under `~/.claude/touring/`.
const MIRROR_FILE: &str = "sdk_signal_mirror.jsonl";

/// The writer of a mirror line. `hooks_complement` divides only the
/// `PostBash` deliveries by the post-bash dispatch count; before this tag the
/// SDK's own queries inflated that ratio to 5.0. A line written before the tag
/// existed has no origin — the aggregate counts it as unknown, never as a
/// post-bash delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorOrigin {
    /// The `post_bash` hook handler classifying a real `touring …` command.
    PostBash,
    /// The Python SDK injected by `touring run --orchestrate`.
    Sdk,
}

impl MirrorOrigin {
    /// Canonical snake_case string — the exact value written to the mirror
    /// (`post_bash` / `sdk`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PostBash => "post_bash",
            Self::Sdk => "sdk",
        }
    }

    /// The inverse of [`Self::as_str`]; any other value (or none) is unknown.
    fn from_wire(s: &str) -> Option<Self> {
        match s {
            "post_bash" => Some(Self::PostBash),
            "sdk" => Some(Self::Sdk),
            _ => None,
        }
    }
}

/// What the mirror says, already aggregated. Plain data: [`Mirror::read`] is
/// where the work happens.
///
/// A name outside `HookName::ALL` (a daemon alias like `cli-index-find`) counts
/// in `total_calls` and `non_canonical_calls` but never in
/// `canonical_hooks_used` — alias drift stays visible and never inflates the
/// signal-use ratio (measured 01/09/2026: ratio 1.0 with 3 canonical hooks).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MirrorAggregate {
    /// Every readable call in the window, canonical or not.
    pub total_calls: u64,
    /// Distinct canonical hooks (`HookName::ALL`) seen at least once.
    pub canonical_hooks_used: usize,
    /// Calls whose name is outside the canonical set.
    pub non_canonical_calls: u64,
    /// Canonical calls written by the `post_bash` feeder.
    pub post_bash_calls: u64,
    /// Canonical calls written by the in-sandbox SDK.
    pub sdk_calls: u64,
    /// Canonical calls with no (or an unrecognised) origin — legacy lines.
    pub unknown_origin_calls: u64,
    /// Median duration over every call that carries one (ms).
    pub duration_ms_p50: u64,
    /// 99th-percentile duration over every call that carries one (ms).
    pub duration_ms_p99: u64,
    /// Non-blank lines that could not be read as a call — not UTF-8, not JSON, or no string
    /// `hook_name` (a torn append, a foreign writer). They are in no other count, whatever
    /// the window, so a damaged mirror shows itself instead of reading as a quiet one.
    pub skipped_lines: u64,
}

impl MirrorAggregate {
    /// Calls whose name is one of `HookName::ALL` — `post_bash_calls +
    /// sdk_calls + unknown_origin_calls`, the deliveries the KPIs credit.
    #[must_use]
    pub fn canonical_calls(&self) -> u64 {
        self.total_calls.saturating_sub(self.non_canonical_calls)
    }
}

/// A handle on the signal mirror file.
#[derive(Debug, Clone)]
pub struct Mirror {
    path: PathBuf,
}

impl Mirror {
    /// A mirror at an explicit path (tests, a copy of the mirror).
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The canonical mirror under `$HOME`; `None` when `HOME` is unset.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        std::env::var_os("HOME").map(|home| Self::at(default_path(Path::new(&home))))
    }

    /// The mirror's path (the sandbox gets it as `TOURING_SDK_SIGNAL_MIRROR`).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one hook-call line. The parent dir is created lazily.
    ///
    /// # Errors
    /// The directory cannot be created or the file cannot be opened/written.
    /// Callers treat it as observability, never as correctness.
    pub fn record(
        &self,
        hook: HookName,
        duration_ms: u32,
        success: bool,
        origin: MirrorOrigin,
    ) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut line = serde_json::to_string(&Line {
            ts: now_unix(),
            hook_name: hook.as_str(),
            duration_ms,
            success,
            origin: origin.as_str(),
        })
        .map_err(io::Error::other)?;
        line.push('\n');
        // ONE write: `writeln!` on a `File` issues the text and the newline as two
        // syscalls, and a concurrent writer (the sandbox's Python SDK appends to the
        // same file) can land between them.
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(line.as_bytes())
    }

    /// Read the mirror and aggregate the calls with `ts >= since_epoch`
    /// (`0` reads everything). `Ok(None)` means there is no mirror file yet —
    /// information, not an empty aggregate. The file is read as BYTES, one line at a
    /// time: a line that is not UTF-8, not JSON, or has no string `hook_name` is skipped
    /// and counted in `skipped_lines`, so a torn line never aborts the read.
    ///
    /// # Errors
    /// The file exists but cannot be read (permissions, a directory in its place).
    pub fn read(&self, since_epoch: u64) -> io::Result<Option<MirrorAggregate>> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(aggregate(&bytes, since_epoch))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// The line `record` writes. Field order is the format.
#[derive(Serialize)]
struct Line<'a> {
    ts: u64,
    hook_name: &'a str,
    duration_ms: u32,
    success: bool,
    origin: &'a str,
}

/// One readable call. Tolerant on purpose: only `hook_name` is required, the
/// rest degrade (a missing `ts` is `0`, a missing duration is no sample).
struct Call {
    ts: u64,
    hook_name: String,
    duration_ms: Option<u32>,
    origin: Option<MirrorOrigin>,
}

impl Call {
    fn parse(line: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(line).ok()?;
        Some(Self {
            hook_name: v.get("hook_name")?.as_str()?.to_owned(),
            ts: v.get("ts").and_then(Value::as_u64).unwrap_or(0),
            duration_ms: v
                .get("duration_ms")
                .and_then(Value::as_u64)
                .and_then(|d| u32::try_from(d).ok()),
            origin: v
                .get("origin")
                .and_then(Value::as_str)
                .and_then(MirrorOrigin::from_wire),
        })
    }
}

fn aggregate(bytes: &[u8], since_epoch: u64) -> MirrorAggregate {
    let mut agg = MirrorAggregate::default();
    let mut seen: HashSet<HookName> = HashSet::new();
    let mut durations: Vec<u32> = Vec::new();
    for raw in bytes.split(|b| *b == b'\n') {
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        if raw.trim_ascii().is_empty() {
            continue;
        }
        let Some(call) = std::str::from_utf8(raw).ok().and_then(Call::parse) else {
            agg.skipped_lines += 1;
            continue;
        };
        if call.ts < since_epoch {
            continue;
        }
        agg.total_calls += 1;
        durations.extend(call.duration_ms);
        match parse_hook(&call.hook_name) {
            Ok(hook) => {
                seen.insert(hook);
                match call.origin {
                    Some(MirrorOrigin::PostBash) => agg.post_bash_calls += 1,
                    Some(MirrorOrigin::Sdk) => agg.sdk_calls += 1,
                    None => agg.unknown_origin_calls += 1,
                }
            }
            Err(_) => agg.non_canonical_calls += 1,
        }
    }
    agg.canonical_hooks_used = seen.len();
    (agg.duration_ms_p50, agg.duration_ms_p99) = percentiles(&mut durations);
    agg
}

/// `~/.claude/touring/sdk_signal_mirror.jsonl` — the run journal's directory.
fn default_path(home: &Path) -> PathBuf {
    home.join(".claude").join("touring").join(MIRROR_FILE)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_mirror(name: &str) -> Mirror {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "sdk-mirror-{}-{}-{name}.jsonl",
            std::process::id(),
            now_unix()
        ));
        Mirror::at(p)
    }

    fn cleanup(m: &Mirror) {
        std::fs::remove_file(m.path()).ok();
    }

    #[test]
    fn empty_mirror_is_present_with_zero_calls() {
        let m = tmp_mirror("empty");
        std::fs::write(m.path(), "").expect("write empty mirror");
        let agg = m.read(0).expect("read empty mirror").expect("file exists");
        assert_eq!(agg, MirrorAggregate::default());
        cleanup(&m);
    }

    #[test]
    fn no_file_is_none_not_an_empty_aggregate() {
        let m = Mirror::at("/nonexistent/mirror.jsonl");
        assert_eq!(m.read(0).expect("a missing mirror is not an error"), None);
    }

    #[test]
    fn record_appends_valid_jsonl() {
        let m = tmp_mirror("append");
        for _ in 0..3 {
            m.record(HookName::AstMeta, 12, true, MirrorOrigin::Sdk)
                .expect("record ast_meta");
        }
        m.record(HookName::Parallel, 5, false, MirrorOrigin::Sdk)
            .expect("record parallel fail");
        let agg = m.read(0).expect("read").expect("file exists");
        assert_eq!(agg.total_calls, 4);
        assert_eq!(agg.canonical_hooks_used, 2, "ast_meta + parallel");
        assert_eq!(agg.sdk_calls, 4);
        assert_eq!(agg.non_canonical_calls, 0);
        assert_eq!(agg.canonical_calls(), 4);
        cleanup(&m);
    }

    /// The line format is the contract with every other writer (the Python SDK in the
    /// sandbox) and every reader of the real mirror: field names AND order.
    #[test]
    fn record_writes_the_documented_line() {
        let m = tmp_mirror("format");
        m.record(HookName::IndexFind, 3, false, MirrorOrigin::PostBash)
            .expect("record");
        let raw = std::fs::read_to_string(m.path()).expect("mirror text");
        assert!(raw.ends_with('\n'), "one line, newline-terminated: {raw:?}");
        let v: Value = serde_json::from_str(raw.trim()).expect("one JSON line");
        let mut keys: Vec<&str> = v
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["duration_ms", "hook_name", "origin", "success", "ts"]
        );
        assert!(raw.starts_with(r#"{"ts":"#), "line: {raw}");
        assert!(
            raw.trim_end().ends_with(
                r#","hook_name":"index_find","duration_ms":3,"success":false,"origin":"post_bash"}"#
            ),
            "line: {raw}"
        );
        assert_eq!(MirrorOrigin::Sdk.as_str(), "sdk");
        assert_eq!(
            MirrorOrigin::from_wire("post_bash"),
            Some(MirrorOrigin::PostBash)
        );
        cleanup(&m);
    }

    /// The real mirror carries lines written before `origin` existed.
    #[test]
    fn legacy_line_without_origin_counts_as_unknown() {
        let m = tmp_mirror("legacy");
        std::fs::write(
            m.path(),
            "{\"ts\":1788300000,\"hook_name\":\"ast_meta\",\"duration_ms\":1,\"success\":true}\n",
        )
        .expect("write legacy line");
        let agg = m.read(0).expect("read").expect("file exists");
        assert_eq!(agg.total_calls, 1);
        assert_eq!(
            agg.unknown_origin_calls, 1,
            "absent origin is unknown, never invented"
        );
        assert_eq!(agg.post_bash_calls, 0);
        cleanup(&m);
    }

    /// Measured 01/09/2026: alias names (`cli-index-find`) lifted the ratio to 1.0 with
    /// only 3 canonical hooks. They stay visible, apart.
    #[test]
    fn alias_names_count_apart_and_never_as_used() {
        let m = tmp_mirror("alias");
        std::fs::write(
            m.path(),
            concat!(
                "{\"ts\":1,\"hook_name\":\"ast_meta\",\"duration_ms\":2,\"success\":true}\n",
                "{\"ts\":2,\"hook_name\":\"cli-index-find\",\"duration_ms\":0,\"success\":true}\n",
                "{\"ts\":3,\"hook_name\":\"cli-gate-metrics\",\"duration_ms\":0,\"success\":true}\n",
                "{\"ts\":4,\"hook_name\":\"index_find\",\"duration_ms\":1,\"success\":true}\n",
                "{\"ts\":5,\"hook_name\":\"index_find\",\"duration_ms\":1,\"success\":true}\n",
            ),
        )
        .expect("write");
        let agg = m.read(0).expect("read").expect("file exists");
        assert_eq!(
            agg.canonical_hooks_used, 2,
            "ast_meta + index_find; cli-* never count"
        );
        assert_eq!(agg.total_calls, 5, "every readable line is a call");
        assert_eq!(agg.non_canonical_calls, 2);
        assert_eq!(agg.canonical_calls(), 3);
        cleanup(&m);
    }

    #[test]
    fn the_window_drops_older_lines_from_every_count() {
        let m = tmp_mirror("window");
        std::fs::write(
            m.path(),
            concat!(
                "{\"ts\":100,\"hook_name\":\"index_find\",\"duration_ms\":9,\"success\":true,\"origin\":\"sdk\"}\n",
                "{\"ts\":200,\"hook_name\":\"ast_meta\",\"duration_ms\":1,\"success\":true,\"origin\":\"post_bash\"}\n",
                "{\"ts\":300,\"hook_name\":\"cli-index-find\",\"duration_ms\":2,\"success\":true}\n",
            ),
        )
        .expect("write");
        let all = m.read(0).expect("read").expect("file exists");
        assert_eq!((all.total_calls, all.sdk_calls), (3, 1));
        let since = m.read(200).expect("read").expect("file exists");
        assert_eq!(since.total_calls, 2);
        assert_eq!(
            since.sdk_calls, 0,
            "the ts=100 delivery is outside the window"
        );
        assert_eq!(since.post_bash_calls, 1);
        assert_eq!(since.non_canonical_calls, 1);
        assert_eq!(since.canonical_hooks_used, 1);
        cleanup(&m);
    }

    /// RED→GREEN (C5): the old `read` aborted on the first malformed line, so ONE torn
    /// line (a crash mid-append) made the KPI drop the percentiles while the counts —
    /// read by a tolerant second parser — kept going. One reader, one semantic: skip.
    #[test]
    fn a_torn_line_does_not_hide_the_durations() {
        let m = tmp_mirror("torn");
        std::fs::write(
            m.path(),
            concat!(
                "{\"ts\":1,\"hook_name\":\"index_find\",\"duration_ms\":8,\"success\":true}\n",
                "{\"ts\":2,\"hook_name\":\"ast_me\n",
                "not json at all\n",
                "{\"ts\":3,\"foo\":1}\n",
                "{\"ts\":4,\"hook_name\":\"memory_recall\",\"duration_ms\":40,\"success\":true}\n",
            ),
        )
        .expect("write");
        let agg = m
            .read(0)
            .expect("a torn line is not an error")
            .expect("file exists");
        assert_eq!(agg.total_calls, 2, "only the two readable lines are calls");
        assert_eq!(agg.canonical_hooks_used, 2);
        assert_eq!(
            (agg.duration_ms_p50, agg.duration_ms_p99),
            (40, 40),
            "p50/p99 over [8, 40]"
        );
        assert_eq!(
            agg.skipped_lines, 3,
            "the torn line, the text and the JSON with no hook_name are counted, not lost"
        );
        let late = m.read(4).expect("read").expect("file exists");
        assert_eq!(
            late.skipped_lines, 3,
            "a line that cannot be read has no date: no window excludes it"
        );
        cleanup(&m);
    }

    /// RED→GREEN (review 12/11/2026, C2): the promise is "a bad line never aborts the read", and
    /// `read_to_string` broke it for a line that is not UTF-8.
    #[test]
    fn a_line_that_is_not_utf8_does_not_abort_the_read() {
        let m = tmp_mirror("utf8");
        let mut body =
            br#"{"ts":1,"hook_name":"index_find","duration_ms":8,"success":true}"#.to_vec();
        body.extend_from_slice(b"\n{\"ts\":2,\"hook_name\":\"ast_\xff\xfe\"}\n");
        body.extend_from_slice(
            br#"{"ts":4,"hook_name":"memory_recall","duration_ms":40,"success":true}"#,
        );
        body.push(b'\n');
        std::fs::write(m.path(), body).expect("write");
        let agg = m
            .read(0)
            .expect("a line that is not UTF-8 is not an error")
            .expect("file exists");
        assert_eq!(agg.total_calls, 2);
        assert_eq!((agg.duration_ms_p50, agg.duration_ms_p99), (40, 40));
        assert_eq!(
            agg.skipped_lines, 1,
            "the line that is not UTF-8 is counted"
        );
        cleanup(&m);
    }

    #[test]
    fn percentiles_follow_the_sorted_nearest_rank() {
        let m = tmp_mirror("pct");
        for ms in [30u32, 10, 20] {
            m.record(HookName::IndexFind, ms, true, MirrorOrigin::Sdk)
                .expect("record");
        }
        let agg = m.read(0).expect("read").expect("file exists");
        assert_eq!((agg.duration_ms_p50, agg.duration_ms_p99), (20, 30));
        cleanup(&m);
    }

    #[test]
    fn default_path_is_canonical() {
        assert_eq!(
            default_path(Path::new("/home/test"))
                .to_str()
                .expect("utf8"),
            "/home/test/.claude/touring/sdk_signal_mirror.jsonl"
        );
    }
}
