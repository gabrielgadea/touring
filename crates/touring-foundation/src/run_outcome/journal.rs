//! The run journal — `~/.claude/touring/run_journal.jsonl`: where it lives, how
//! a run is appended to it, and how a line of it is read back.
//!
//! One JSON object per line. The WRITER is `touring run`
//! (`ctx_execute_tools::journal_run`); everything else reads. Before this module
//! the path literal was spelled in nine files and the line-parsing in five, each
//! with its own tolerance for a bad line.
//!
//! # Reading: two views, on purpose
//!
//! The journal is append-only, written by processes that die mid-write, and it
//! holds two kinds of line: a run record and the repeat-gate's `repeat_blocked`
//! refusal event (valid JSON, no `exit_code`, no `language`). The readers always
//! disagreed about what counts as a line, and each disagreement is a MEASURED
//! quantity (the KPI denominator), so it is kept, named, not unified:
//!
//! | view | for | a line counts when | blank line |
//! |---|---|---|---|
//! | [`classify_line`] → [`RunRecord`] (lenient) | the rulers: KPI `code_mode_adherence`, the M1 gate | it is any valid JSON | [`Line::Blank`] — the caller decides |
//! | [`scan_entries`] → [`JournalEntry`] (strict) | the aggregate (signal report, reuse KPI) | `ts`, `language`, `exit_code`, `duration_ms` are present and typed | skipped |
//!
//! So a refusal event is a "run" (a failed one: no `exit_code` reads as no exit)
//! to the rulers and a malformed line to the aggregate. That is how it was; it is
//! now ONE visible decision instead of five accidents.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use super::outcome::{ExitStatus, FailureKind, RunOutcome};

/// The journal under a user's home, relative to it.
const JOURNAL_RELATIVE_PATH: &str = ".claude/touring/run_journal.jsonl";

/// How much of the END of the journal the recurrence searches look at (512 KiB):
/// older runs fade out by design — the question is "is this being re-run
/// LATELY?", not "did it ever run".
const TAIL_BYTES: u64 = 512 * 1024;

/// `PIPE_BUF` — the size up to which POSIX guarantees an `O_APPEND` write to a
/// regular file is atomic between writers (Linux: 4096).
const PIPE_BUF: usize = 4096;

/// The canonical journal path under `home`. Pure — no env reading — so tests
/// can pass a tempdir.
#[must_use]
pub fn journal_path(home: &Path) -> PathBuf {
    home.join(JOURNAL_RELATIVE_PATH)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Writing a run
// ─────────────────────────────────────────────────────────────────────────────

/// Everything one run contributes to a journal record, as plain borrowed data.
///
/// The record carries, beyond the outcome: the origin of the program body
/// (`source`/`file`), `harvest`, the presentation flags, the bytes the run left
/// in its private tmp, the ladder hint, and the body's identity (`code_hash`,
/// exact; two SimHash-64 versions, by similarity) — the fields the reuse and
/// repeat-gate rulers read.
#[derive(Debug, Clone)]
pub struct RunLog<'a> {
    /// This execution's identity (`run-<epoch_ms>-<pid>`).
    pub run_id: &'a str,
    /// The language as the caller wrote it.
    pub language: &'a str,
    /// Full stdout size in bytes (journaled as `code_hash_stdout_bytes`: the
    /// legacy name carries a LENGTH, never a hash).
    pub stdout_bytes: usize,
    /// How the process ended and the failure class, if any.
    pub outcome: RunOutcome,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Bytes the elision kept out of the context.
    pub bytes_elided: u64,
    /// sha256[:16] of the program body — exact identity.
    pub code_hash: &'a str,
    /// SimHash-64 (v1) of the normalized body.
    pub code_simhash: u64,
    /// SimHash-64 (v2) of the normalized body.
    pub code_simhash_v2: u64,
    /// Origin of the body: `file` | `inline` | `stdin`, when the caller declared it.
    pub source_kind: Option<&'a str>,
    /// The script path when the body came from a file.
    pub source_file: Option<&'a str>,
    /// The `--harvest <slug>` of the run, if any.
    pub harvest: Option<&'a str>,
    /// `--brief` was requested.
    pub brief: bool,
    /// The `--orchestrate` SDK was injected.
    pub orchestrate: bool,
    /// The ladder's top similar block `(entry_key, similarity)` at run time.
    pub hint: Option<(&'a str, f64)>,
    /// Bytes the run left in its private tmp dir.
    pub tmp_bytes: u64,
}

impl RunLog<'_> {
    /// The journal record. `ts` (unix seconds) and `prior_same_hash` (how many
    /// times this exact `code_hash` ran before, from the journal tail) are
    /// injected so the record is a pure function of its inputs.
    ///
    /// Keys serialize in `serde_json`'s map order (sorted), the same bytes the
    /// pre-consolidation writer emitted — pinned against real journal lines in
    /// the tests, because existing journals on disk must keep parsing the same.
    #[must_use]
    pub fn to_json(&self, ts: u64, prior_same_hash: u32) -> Value {
        json!({
            "ts": ts,
            "run_id": self.run_id,
            "language": self.language,
            "code_hash_stdout_bytes": self.stdout_bytes,
            "code_hash": self.code_hash,
            "code_simhash": format!("{:016x}", self.code_simhash),
            "code_simhash_v2": format!("{:016x}", self.code_simhash_v2),
            "prior_same_hash": prior_same_hash,
            "exit_code": self.outcome.exit.to_wire(),
            "duration_ms": self.duration_ms,
            "failure_kind": self.outcome.failure.map(FailureKind::as_str),
            "bytes_elided": self.bytes_elided,
            "source": self.source_kind,
            "file": self.source_file,
            "harvest": self.harvest,
            "brief": self.brief,
            "orchestrate": self.orchestrate,
            "hint": self.hint.map(|(key, sim)| json!({ "key": key, "sim": sim })),
            "tmp_bytes": self.tmp_bytes,
        })
    }
}

/// Which SimHash generation a similarity search compares. Only v2 against v2
/// (and v1 against v1): a line written before a version existed lacks its field
/// and must match nothing, instead of being compared across two different votes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimhashVersion {
    /// `code_simhash` — the original vote.
    V1,
    /// `code_simhash_v2` — one vote per distinct shingle.
    V2,
}

impl SimhashVersion {
    /// The journal field that carries this version's hash.
    const fn field(self) -> &'static str {
        match self {
            Self::V1 => "code_simhash",
            Self::V2 => "code_simhash_v2",
        }
    }
}

/// One similar prior: who, how far, and whether it was harvested.
#[derive(Debug, Clone)]
pub struct PriorSimilar {
    /// `run_id` of the earlier execution (the address to investigate).
    pub run_id: String,
    /// Hamming distance to the current body (≤ τ by construction).
    pub dist: u32,
    /// The `--harvest` slug of that run, when there was one (the ready route to reuse).
    pub harvest: Option<String>,
}

/// The result of a similarity search: the priors AND the tail lines that did
/// not parse. A skipped corrupt line hides that the journal is corrupt, and a
/// partial read (a prior that did not come back) looks like "no precedent".
#[derive(Debug, Default)]
pub struct PriorSearch {
    /// Priors within τ, nearest first.
    pub priors: Vec<PriorSimilar>,
    /// Tail lines (among those carrying the field) that did not parse.
    pub malformed_lines: u64,
}

/// A handle on a journal file.
#[derive(Debug, Clone)]
pub struct Journal {
    path: PathBuf,
}

impl Journal {
    /// A journal at an explicit path (tests, a copy of the journal).
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The canonical journal under `home` ([`journal_path`]).
    #[must_use]
    pub fn under_home(home: &Path) -> Self {
        Self::at(journal_path(home))
    }

    /// The canonical journal under `$HOME`; `None` when `HOME` is unset.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        std::env::var_os("HOME").map(|home| Self::under_home(Path::new(&home)))
    }

    /// The journal's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The whole journal as text (the rulers read it whole).
    ///
    /// # Errors
    /// The file is missing, unreadable, or not valid UTF-8.
    pub fn read_to_string(&self) -> std::io::Result<String> {
        std::fs::read_to_string(&self.path)
    }

    /// Journal one run and return how many times its exact `code_hash` appeared
    /// in the journal tail BEFORE this record (the recurrence the harvest nudge
    /// reads). Fail-open: any I/O problem is swallowed and reads as `0` —
    /// observability never blocks a run.
    pub fn record_run(&self, run: &RunLog<'_>) -> u32 {
        if let Some(dir) = self.path.parent()
            && std::fs::create_dir_all(dir).is_err()
        {
            return 0;
        }
        // Counted BEFORE the append so the record does not see itself.
        let prior = self.count_prior_same_hash(run.code_hash);
        self.append_json(&run.to_json(unix_now(), prior));
        prior
    }

    /// Append ONE JSON record as one line — one write, the whole record.
    ///
    /// Two calls (the body, then the `\n`) let two writers splice between them:
    /// measured on the real journal, 400 invalid lines in 47.102, 99 of them
    /// with 2+ `run_id` (ids 1 ms apart). With `O_APPEND` and the whole record in
    /// ONE call the kernel positions at the end and writes the block at once.
    /// Above `PIPE_BUF` that guarantee does not cover the size, so an exclusive
    /// `flock` serializes the writers (same pattern as the daemon's lock).
    ///
    /// Returns whether the whole record was written. Fail-open: callers may
    /// ignore it.
    pub fn append_json(&self, record: &Value) -> bool {
        let Ok(mut line) = serde_json::to_string(record) else {
            return false;
        };
        line.push('\n');
        let bytes = line.as_bytes();
        let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return false;
        };
        if bytes.len() > PIPE_BUF {
            let _guard = lock::JournalLock::acquire(&f);
            return f.write_all(bytes).is_ok();
        }
        // Up to PIPE_BUF: ONE write call (the record and its `\n` in one buffer)
        // — `write`, NOT `write_all`: `write_all` may emit several syscalls on a
        // partial write, and each syscall is a splice point.
        match f.write(bytes) {
            Ok(n) if n == bytes.len() => true,
            Ok(n) => {
                // Partial write: the rest goes under the lock — otherwise the
                // record is cut in the middle and the line becomes garbage.
                let _guard = lock::JournalLock::acquire(&f);
                f.write_all(&bytes[n..]).is_ok()
            }
            Err(_) => false,
        }
    }

    /// The last [`TAIL_BYTES`] of the journal. Fail-open: `None`.
    fn tail(&self) -> Option<Vec<u8>> {
        let mut f = std::fs::File::open(&self.path).ok()?;
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES)))
            .ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    /// How many records in the journal tail carry this exact `code_hash`.
    /// Byte-level match on the fixed JSON key — no per-line parse. Fail-open:
    /// any I/O problem reads as zero (the nudge simply does not fire, never a
    /// wrong count).
    #[must_use]
    pub fn count_prior_same_hash(&self, hash: &str) -> u32 {
        let Some(buf) = self.tail() else {
            return 0;
        };
        let needle = format!("\"code_hash\":\"{hash}\"").into_bytes();
        if needle.len() > buf.len() {
            return 0;
        }
        buf.windows(needle.len())
            .filter(|w| *w == needle.as_slice())
            .count() as u32
    }

    /// The priors within Hamming distance `tau` of `simhash` in the journal
    /// tail. Filters by the field's substring before parsing a line (the tail
    /// holds hundreds; the parse runs only on lines that carry the field).
    /// Fail-open: an empty search.
    #[must_use]
    pub fn find_prior_similar(
        &self,
        version: SimhashVersion,
        simhash: u64,
        tau: u32,
    ) -> PriorSearch {
        let field = version.field();
        let marker = format!("\"{field}\"");
        let Some(buf) = self.tail() else {
            return PriorSearch::default();
        };
        let Ok(text) = String::from_utf8(buf) else {
            return PriorSearch::default(); // the tail cut a char in half — the next run recounts
        };
        let mut priors = Vec::new();
        let mut malformed_lines = 0u64;
        for line in text.lines().filter(|l| l.contains(&marker)) {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                malformed_lines += 1;
                continue;
            };
            let Some(h) = v
                .get(field)
                .and_then(Value::as_str)
                .and_then(|x| u64::from_str_radix(x, 16).ok())
            else {
                continue;
            };
            let dist = (h ^ simhash).count_ones();
            if dist <= tau {
                priors.push(PriorSimilar {
                    run_id: v
                        .get("run_id")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string(),
                    dist,
                    harvest: v.get("harvest").and_then(Value::as_str).map(str::to_string),
                });
            }
        }
        priors.sort_by_key(|p| p.dist);
        PriorSearch {
            priors,
            malformed_lines,
        }
    }
}

/// The journal's exclusive `flock` (writers above `PIPE_BUF`).
///
/// `flock(2)` is declared locally — the same pattern as the daemon's lock — so
/// the module pulls no `libc` for one syscall. Fail-open: if the lock fails the
/// write goes ahead; the journal never blocks a run.
mod lock {
    #[cfg(unix)]
    use std::os::raw::c_int;
    #[cfg(unix)]
    use std::os::unix::io::{AsRawFd, RawFd};

    #[cfg(unix)]
    const LOCK_EX: c_int = 2;
    #[cfg(unix)]
    const LOCK_UN: c_int = 8;

    #[cfg(unix)]
    unsafe extern "C" {
        fn flock(fd: RawFd, operation: c_int) -> c_int;
    }

    /// Holds the lock until dropped. Holds the FD, not the `&File`: the guard has
    /// to live beside a mutable write on the same file.
    pub(super) struct JournalLock {
        #[cfg(unix)]
        fd: RawFd,
    }

    #[cfg(unix)]
    impl JournalLock {
        pub(super) fn acquire(file: &std::fs::File) -> Self {
            let fd = file.as_raw_fd();
            // SAFETY: `flock(2)` on a valid fd; the return is only observed.
            let rc = unsafe { flock(fd, LOCK_EX) };
            if rc != 0 {
                tracing::debug!("journal: flock failed; the write proceeds without the lock");
            }
            Self { fd }
        }
    }

    #[cfg(unix)]
    impl Drop for JournalLock {
        fn drop(&mut self) {
            // SAFETY: same fd — release (the OS also releases on close).
            unsafe {
                flock(self.fd, LOCK_UN);
            }
        }
    }

    #[cfg(not(unix))]
    impl JournalLock {
        pub(super) fn acquire(_file: &std::fs::File) -> Self {
            Self {}
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Reading: the lenient view (the rulers)
// ─────────────────────────────────────────────────────────────────────────────

/// One journal line, classified.
#[derive(Debug, Clone)]
pub enum Line {
    /// Empty or whitespace only.
    Blank,
    /// Not valid JSON — corruption (a concurrent write spliced, a truncated tail).
    Malformed,
    /// Valid JSON: a run record, or any other event the file carries.
    Record(RunRecord),
}

/// Classify one line (a `str::lines()` item or a `BufRead::lines()` item).
#[must_use]
pub fn classify_line(line: &str) -> Line {
    if line.trim().is_empty() {
        return Line::Blank;
    }
    match serde_json::from_str::<Value>(line) {
        Ok(value) => Line::Record(RunRecord { value }),
        Err(_) => Line::Malformed,
    }
}

/// The lenient view of a valid-JSON journal line: every field optional, every
/// accessor total. A line that is not an object (`123`, `"x"`) is a record whose
/// every accessor answers "absent".
#[derive(Debug, Clone)]
pub struct RunRecord {
    value: Value,
}

impl RunRecord {
    /// The program language, if the line carries one.
    #[must_use]
    pub fn language(&self) -> Option<&str> {
        self.value.get("language").and_then(Value::as_str)
    }

    /// The run's unix timestamp (seconds), if present as an unsigned integer.
    #[must_use]
    pub fn ts(&self) -> Option<u64> {
        self.value.get("ts").and_then(Value::as_u64)
    }

    /// How the process ended. A line with no usable `exit_code` (absent, a
    /// float, beyond `i32`) reads as [`ExitStatus::NoExitCode`] — no exit is
    /// not a zero exit, which is how the rulers have always read it.
    #[must_use]
    pub fn exit(&self) -> ExitStatus {
        self.value
            .get("exit_code")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .map_or(ExitStatus::NoExitCode, ExitStatus::from_wire)
    }

    /// The failure class from the canonical taxonomy. An unknown label is
    /// [`FailureKind::Other`]; an absent or non-string label is `None`.
    #[must_use]
    pub fn failure_kind(&self) -> Option<FailureKind> {
        self.value
            .get("failure_kind")
            .and_then(Value::as_str)
            .map(FailureKind::from_label)
    }

    /// The outcome the line records.
    #[must_use]
    pub fn outcome(&self) -> RunOutcome {
        RunOutcome::new(self.exit(), self.failure_kind())
    }
}

/// `(runs_total, runs_ok)` over journal text — the M1 ruler's fraction. Every
/// valid-JSON line is a run; a blank or unparseable line is skipped (the
/// journal is append-only from processes that die mid-write). `ok` is
/// [`RunOutcome::exited_zero`].
#[must_use]
pub fn count_runs(text: &str) -> (u64, u64) {
    let (mut total, mut ok) = (0u64, 0u64);
    for line in text.lines() {
        if let Line::Record(run) = classify_line(line) {
            total += 1;
            if run.outcome().exited_zero() {
                ok += 1;
            }
        }
    }
    (total, ok)
}

// ─────────────────────────────────────────────────────────────────────────────
// Reading: the strict view (the aggregate)
// ─────────────────────────────────────────────────────────────────────────────

/// N6 (2026-09-23): the ladder's top similar block for the run's program,
/// journaled at execution time.
#[derive(Debug, Clone, Deserialize)]
pub struct JournalHint {
    /// The ladder key of the top similar snippet.
    pub key: String,
    /// Jaccard similarity of the match, in `[0.0, 1.0]`.
    pub sim: f64,
}

/// One strictly-typed journal entry. Field order matches the on-disk JSON.
/// `ts`, `language`, `exit_code` and `duration_ms` are required; everything
/// later in the schema's history defaults, so v1 lines still parse.
#[derive(Debug, Clone, Deserialize)]
pub struct JournalEntry {
    /// Unix timestamp (seconds) of the run.
    pub ts: u64,
    /// Program language (`bash` | `python` | `js` | `shell` | `rust`).
    pub language: String,
    /// The wire exit code (see [`ExitStatus::from_wire`]).
    pub exit_code: i32,
    /// `None` on success; otherwise the [`FailureKind`] label as written.
    #[serde(default)]
    pub failure_kind: Option<String>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u32,
    /// Output payload size in bytes (post-elision).
    #[serde(default)]
    pub code_hash_stdout_bytes: u64,
    /// Bytes omitted from the journal by the elision step (token-budget).
    #[serde(default)]
    pub bytes_elided: u64,
    /// B2 (2026-09-02): where the body came from — `file` | `inline` | `stdin`.
    /// `None` on v1 lines (written before the field existed).
    #[serde(default)]
    pub source: Option<String>,
    /// B2: the script path when `source == "file"`.
    #[serde(default)]
    pub file: Option<String>,
    /// B2: the `--harvest <slug>` this run enrolled, if any.
    #[serde(default)]
    pub harvest: Option<String>,
    /// B2: `--brief` was requested.
    #[serde(default)]
    pub brief: bool,
    /// B2: `--orchestrate` SDK was injected.
    #[serde(default)]
    pub orchestrate: bool,
    /// B2: bytes the run left in its private temp dir (B1), measured before removal.
    #[serde(default)]
    pub tmp_bytes: u64,
    /// N6: the ladder's top similar block at run time, when one existed.
    #[serde(default)]
    pub hint: Option<JournalHint>,
}

impl JournalEntry {
    /// The outcome the entry records (a label this build does not know is
    /// [`FailureKind::Other`]).
    #[must_use]
    pub fn outcome(&self) -> RunOutcome {
        RunOutcome::new(
            ExitStatus::from_wire(self.exit_code),
            self.failure_kind.as_deref().map(FailureKind::from_label),
        )
    }
}

/// The result of reading a journal strictly.
#[derive(Debug, Default)]
pub struct EntryScan {
    /// The lines that parsed as a [`JournalEntry`], in file order.
    pub entries: Vec<JournalEntry>,
    /// Non-blank lines that did NOT parse — COUNTED and EXPOSED (T-1,
    /// 31/10/2026): skipped silently they hide the corruption of the
    /// denominator, and a record that was lost looks like one that never existed.
    pub malformed_lines: u64,
    /// The position (0-based) of the FIRST malformed line — where to look.
    pub malformed_first_line: Option<u64>,
    /// The FIRST malformed line's parse error — what a caller reports when
    /// nothing at all was readable.
    pub first_error: Option<(usize, serde_json::Error)>,
}

/// Read journal lines strictly. `lines` yields `(line_number, line_result)` so a
/// malformed line reports its position. Blank lines are skipped; a line that
/// fails to parse is counted and skipped — the reader is fail-soft (one bad line
/// does not abort a 50k-line journal).
///
/// # Errors
/// An I/O error reading a line aborts the scan.
pub fn scan_entries<I>(lines: I) -> std::io::Result<EntryScan>
where
    I: IntoIterator<Item = (usize, std::io::Result<String>)>,
{
    let mut scan = EntryScan::default();
    for (line_no, line) in lines {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<JournalEntry>(trimmed) {
            Ok(entry) => scan.entries.push(entry),
            Err(err) => {
                scan.malformed_lines += 1;
                if scan.malformed_first_line.is_none() {
                    scan.malformed_first_line = Some(line_no as u64);
                }
                if scan.first_error.is_none() {
                    scan.first_error = Some((line_no, err));
                }
            }
        }
    }
    Ok(scan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_log<'a>(
        run_id: &'a str,
        language: &'a str,
        stdout_bytes: usize,
        outcome: RunOutcome,
    ) -> RunLog<'a> {
        RunLog {
            run_id,
            language,
            stdout_bytes,
            outcome,
            duration_ms: 0,
            bytes_elided: 0,
            code_hash: "0000000000000000",
            code_simhash: 0,
            code_simhash_v2: 0,
            source_kind: None,
            source_file: None,
            harvest: None,
            brief: false,
            orchestrate: false,
            hint: None,
            tmp_bytes: 0,
        }
    }

    fn outcome(wire: i32, failure: Option<FailureKind>) -> RunOutcome {
        RunOutcome::new(ExitStatus::from_wire(wire), failure)
    }

    // ── the writer is byte-for-byte the one it replaces ─────────────────────

    /// Five REAL journal lines (the latest schema, written by the writer this
    /// module replaced), sanitized only in free-text values (`file` paths).
    /// `RunLog::to_json` must reproduce every byte: existing journals on disk
    /// must keep parsing the same, and new records must serialize identically to
    /// what the old writer produced for the same outcome.
    #[test]
    fn to_json_reproduces_real_journal_lines_byte_for_byte() {
        // (golden line, the RunLog that produced it, ts, prior_same_hash)
        let mut cases: Vec<(&str, RunLog<'_>, u64, u32)> = Vec::new();

        let mut ok_hinted = run_log("run-1793294236685-459457", "python", 351, outcome(0, None));
        ok_hinted.duration_ms = 21;
        ok_hinted.code_hash = "c3c308319b7afaa7";
        ok_hinted.code_simhash = 0x0d9e_925d_7b64_d83f;
        ok_hinted.code_simhash_v2 = 0x0d9e_925d_7b64_d83f;
        ok_hinted.source_kind = Some("inline");
        ok_hinted.hint = Some(("snippet:auto:1abf59ca6914", 0.379));
        cases.push((
            r#"{"brief":false,"bytes_elided":0,"code_hash":"c3c308319b7afaa7","code_hash_stdout_bytes":351,"code_simhash":"0d9e925d7b64d83f","code_simhash_v2":"0d9e925d7b64d83f","duration_ms":21,"exit_code":0,"failure_kind":null,"file":null,"harvest":null,"hint":{"key":"snippet:auto:1abf59ca6914","sim":0.379},"language":"python","orchestrate":false,"prior_same_hash":0,"run_id":"run-1793294236685-459457","source":"inline","tmp_bytes":0,"ts":1793294236}"#,
            ok_hinted,
            1_793_294_236,
            0,
        ));

        let mut timed_out = run_log(
            "run-1793221341926-392070",
            "bash",
            0,
            outcome(-2, Some(FailureKind::Timeout)),
        );
        timed_out.duration_ms = 60252;
        timed_out.code_hash = "fdd6f52f459db556";
        timed_out.code_simhash = 0xa9c6_a21b_9254_81d1;
        timed_out.code_simhash_v2 = 0xa9c6_a21b_9254_83c1;
        timed_out.source_kind = Some("inline");
        cases.push((
            r#"{"brief":false,"bytes_elided":0,"code_hash":"fdd6f52f459db556","code_hash_stdout_bytes":0,"code_simhash":"a9c6a21b925481d1","code_simhash_v2":"a9c6a21b925483c1","duration_ms":60252,"exit_code":-2,"failure_kind":"timeout","file":null,"harvest":null,"hint":null,"language":"bash","orchestrate":false,"prior_same_hash":0,"run_id":"run-1793221341926-392070","source":"inline","tmp_bytes":0,"ts":1793221402}"#,
            timed_out,
            1_793_221_402,
            0,
        ));

        let mut exception = run_log(
            "run-1793200215179-2909345",
            "python",
            10,
            outcome(3, Some(FailureKind::Exception)),
        );
        exception.duration_ms = 35;
        exception.code_hash = "0a8b054edec7bbfb";
        exception.code_simhash = 0xa900_068e_274b_4d20;
        exception.code_simhash_v2 = 0xa900_068e_274b_4d20;
        cases.push((
            r#"{"brief":false,"bytes_elided":0,"code_hash":"0a8b054edec7bbfb","code_hash_stdout_bytes":10,"code_simhash":"a900068e274b4d20","code_simhash_v2":"a900068e274b4d20","duration_ms":35,"exit_code":3,"failure_kind":"exception","file":null,"harvest":null,"hint":null,"language":"python","orchestrate":false,"prior_same_hash":0,"run_id":"run-1793200215179-2909345","source":null,"tmp_bytes":0,"ts":1793200215}"#,
            exception,
            1_793_200_215,
            0,
        ));

        let mut cut = run_log(
            "run-1793545710305-1273806",
            "python",
            1_006_488,
            outcome(1, Some(FailureKind::OutputLimit)),
        );
        cut.duration_ms = 11353;
        cut.bytes_elided = 998_265;
        cut.code_hash = "5d3a06ba80330eb3";
        cut.code_simhash = 0x910e_9b15_7a5e_9d2b;
        cut.code_simhash_v2 = 0x910c_9315_6b5c_9d29;
        cut.source_kind = Some("file");
        cut.source_file = Some("scripts/a21_feat.py");
        cases.push((
            r#"{"brief":false,"bytes_elided":998265,"code_hash":"5d3a06ba80330eb3","code_hash_stdout_bytes":1006488,"code_simhash":"910e9b157a5e9d2b","code_simhash_v2":"910c93156b5c9d29","duration_ms":11353,"exit_code":1,"failure_kind":"output-limit","file":"scripts/a21_feat.py","harvest":null,"hint":null,"language":"python","orchestrate":false,"prior_same_hash":0,"run_id":"run-1793545710305-1273806","source":"file","tmp_bytes":0,"ts":1793545721}"#,
            cut,
            1_793_545_721,
            0,
        ));

        let mut recurring = run_log("run-1793198511710-1032087", "bash", 41, outcome(0, None));
        recurring.duration_ms = 4;
        recurring.code_hash = "863158b0212508cf";
        recurring.code_simhash = 0xbb93_121c_fed3_c4c0;
        recurring.code_simhash_v2 = 0xbb93_121c_fed3_c4c0;
        cases.push((
            r#"{"brief":false,"bytes_elided":0,"code_hash":"863158b0212508cf","code_hash_stdout_bytes":41,"code_simhash":"bb93121cfed3c4c0","code_simhash_v2":"bb93121cfed3c4c0","duration_ms":4,"exit_code":0,"failure_kind":null,"file":null,"harvest":null,"hint":null,"language":"bash","orchestrate":false,"prior_same_hash":8,"run_id":"run-1793198511710-1032087","source":null,"tmp_bytes":0,"ts":1793198511}"#,
            recurring,
            1_793_198_511,
            8,
        ));

        for (golden, log, ts, prior) in cases {
            let json = log.to_json(ts, prior);
            assert_eq!(
                serde_json::to_string(&json).expect("serializes"),
                golden,
                "the record for {} drifted from what the old writer produced",
                log.run_id
            );
        }
    }

    #[test]
    fn to_json_carries_the_flags_harvest_and_file_the_gates_read() {
        let mut log = run_log("run-9", "bash", 0, outcome(0, None));
        log.source_kind = Some("file");
        log.source_file = Some("scripts/x.sh");
        log.harvest = Some("slug");
        log.brief = true;
        log.orchestrate = true;
        log.tmp_bytes = 4096;
        let v = log.to_json(1, 0);
        assert_eq!(v["source"], "file");
        assert_eq!(v["file"], "scripts/x.sh");
        assert_eq!(v["harvest"], "slug");
        assert_eq!(v["brief"], true);
        assert_eq!(v["orchestrate"], true);
        assert_eq!(v["tmp_bytes"], 4096);
        assert_eq!(v["hint"], Value::Null);
    }

    // ── paths ────────────────────────────────────────────────────────────────

    #[test]
    fn the_journal_path_is_the_canonical_one() {
        assert_eq!(
            journal_path(Path::new("/home/test"))
                .to_str()
                .expect("utf-8 literal"),
            "/home/test/.claude/touring/run_journal.jsonl"
        );
        assert_eq!(
            Journal::under_home(Path::new("/h")).path(),
            Path::new("/h/.claude/touring/run_journal.jsonl")
        );
        assert_eq!(Journal::at("/x/y.jsonl").path(), Path::new("/x/y.jsonl"));
    }

    // ── the lenient view ─────────────────────────────────────────────────────

    #[test]
    fn classify_line_separates_blank_malformed_and_records() {
        assert!(matches!(classify_line(""), Line::Blank));
        assert!(matches!(classify_line("  \t "), Line::Blank));
        assert!(matches!(classify_line("NOT JSON"), Line::Malformed));
        assert!(matches!(classify_line("{\"ts\":17"), Line::Malformed));
        assert!(matches!(
            classify_line("{\"a\":1} trailing"),
            Line::Malformed
        ));
        assert!(matches!(classify_line("123"), Line::Record(_)));
        assert!(matches!(classify_line("  {\"ts\":1}  "), Line::Record(_)));
    }

    fn record(line: &str) -> RunRecord {
        match classify_line(line) {
            Line::Record(r) => r,
            other => panic!("not a record: {other:?}"),
        }
    }

    #[test]
    fn a_record_reads_its_fields_totally() {
        let r = record(
            r#"{"ts":1793,"language":"bash","exit_code":-2,"failure_kind":"timeout","duration_ms":5}"#,
        );
        assert_eq!(r.language(), Some("bash"));
        assert_eq!(r.ts(), Some(1793));
        assert_eq!(r.exit(), ExitStatus::TimedOut);
        assert_eq!(r.failure_kind(), Some(FailureKind::Timeout));
        assert_eq!(
            r.outcome(),
            RunOutcome::new(ExitStatus::TimedOut, Some(FailureKind::Timeout))
        );
    }

    /// A line that is valid JSON but not a run record: every accessor answers
    /// "absent", and an absent exit is NOT a zero exit.
    #[test]
    fn a_foreign_json_line_reads_as_a_failed_run_with_nothing_known() {
        for line in [
            "123",
            "\"x\"",
            "[1,2]",
            "null",
            r#"{"success":false,"repeat_blocked":{"ts":9}}"#,
        ] {
            let r = record(line);
            assert_eq!(r.language(), None, "{line}");
            assert_eq!(r.ts(), None, "{line}");
            assert_eq!(r.exit(), ExitStatus::NoExitCode, "{line}");
            assert!(!r.outcome().exited_zero(), "{line}");
            assert_eq!(r.failure_kind(), None, "{line}");
        }
    }

    /// Exit codes the old reader's `as_i64` could not use: a float, a string,
    /// beyond `i32`, null. All read as no exit (non-zero, never ok).
    #[test]
    fn an_unusable_exit_code_reads_as_no_exit_not_as_zero() {
        for line in [
            r#"{"exit_code":0.0}"#,
            r#"{"exit_code":"0"}"#,
            r#"{"exit_code":null}"#,
            r#"{"exit_code":3000000000}"#,
            r#"{"exit_code":-3000000000}"#,
        ] {
            assert_eq!(record(line).exit(), ExitStatus::NoExitCode, "{line}");
        }
        assert_eq!(record(r#"{"exit_code":0}"#).exit(), ExitStatus::Code(0));
        assert_eq!(record(r#"{"exit_code":141}"#).exit(), ExitStatus::Code(141));
    }

    #[test]
    fn failure_kind_goes_through_the_canonical_taxonomy() {
        assert_eq!(
            record(r#"{"failure_kind":"kind-que-nao-existe"}"#).failure_kind(),
            Some(FailureKind::Other)
        );
        assert_eq!(record(r#"{"failure_kind":null}"#).failure_kind(), None);
        assert_eq!(record(r#"{"failure_kind":7}"#).failure_kind(), None);
        assert_eq!(record("{}").failure_kind(), None);
        assert_eq!(
            record(r#"{"failure_kind":"proc-exit"}"#).failure_kind(),
            Some(FailureKind::ProcExit)
        );
    }

    #[test]
    fn count_runs_counts_json_lines_and_exit_zero_and_skips_garbage() {
        let journal = concat!(
            "{\"exit_code\":0,\"language\":\"python\"}\n",
            "lixo-nao-json\n",
            "\n",
            "{\"exit_code\":1,\"language\":\"bash\"}\n",
            "{\"exit_code\":0}\n",
            "{\"success\":false}\n",
            "7\n",
            "{\"exit_code\":0,\"failure_kind\":\"output-limit\"}\n",
        );
        // 6 JSON lines; exit 0 on 3 (the output-limit one counts: PROCESS-level).
        assert_eq!(count_runs(journal), (6, 3));
        assert_eq!(count_runs(""), (0, 0));
    }

    // ── the strict view ──────────────────────────────────────────────────────

    fn lines_iter(s: &str) -> impl Iterator<Item = (usize, std::io::Result<String>)> + '_ {
        s.lines().enumerate().map(|(i, l)| (i, Ok(l.to_string())))
    }

    #[test]
    fn scan_entries_is_strict_counts_malformed_and_skips_blank() {
        let s = concat!(
            r#"{"ts":1,"language":"bash","exit_code":0,"duration_ms":5}"#,
            "\n",
            "\n",
            "NOT JSON\n",
            // valid JSON, but not a full entry (no duration_ms): STRICT view rejects it
            r#"{"ts":2,"language":"bash","exit_code":0}"#,
            "\n",
            r#"{"success":false,"repeat_blocked":{"ts":3}}"#,
            "\n",
            r#"{"ts":4,"language":"python","exit_code":1,"failure_kind":"exception","duration_ms":9}"#,
        );
        let scan = scan_entries(lines_iter(s)).expect("no io error");
        assert_eq!(scan.entries.len(), 2);
        assert_eq!(scan.malformed_lines, 3);
        assert_eq!(scan.malformed_first_line, Some(2));
        let (line, _) = scan.first_error.expect("first error kept");
        assert_eq!(line, 2);
    }

    #[test]
    fn scan_entries_propagates_an_io_error() {
        let bad = [(0usize, Err(std::io::Error::other("boom")))];
        assert!(scan_entries(bad).is_err());
    }

    #[test]
    fn an_entry_outcome_decodes_wire_and_label() {
        let e: JournalEntry = serde_json::from_str(
            r#"{"ts":1,"language":"bash","exit_code":-2,"failure_kind":"timeout","duration_ms":3}"#,
        )
        .expect("parses");
        assert_eq!(
            e.outcome(),
            RunOutcome::new(ExitStatus::TimedOut, Some(FailureKind::Timeout))
        );
        let e: JournalEntry = serde_json::from_str(
            r#"{"ts":1,"language":"bash","exit_code":0,"failure_kind":"zzz","duration_ms":3}"#,
        )
        .expect("parses");
        assert_eq!(e.outcome().failure, Some(FailureKind::Other));
        assert!(e.outcome().exited_zero() && !e.outcome().is_success());
    }

    // ── the tail searches ────────────────────────────────────────────────────

    fn tmp_journal() -> (tempfile::TempDir, Journal) {
        let dir = tempfile::tempdir().expect("tempdir");
        let j = Journal::at(dir.path().join("run_journal.jsonl"));
        (dir, j)
    }

    #[test]
    fn count_prior_same_hash_reads_the_tail_and_fails_open() {
        let (_dir, j) = tmp_journal();
        assert_eq!(
            j.count_prior_same_hash("aaaaaaaaaaaaaaaa"),
            0,
            "no file: fail-open"
        );
        std::fs::write(
            j.path(),
            concat!(
                "{\"code_hash\":\"aaaaaaaaaaaaaaaa\",\"exit_code\":0}\n",
                "{\"code_hash\":\"bbbbbbbbbbbbbbbb\",\"exit_code\":0}\n",
                "{\"code_hash\":\"aaaaaaaaaaaaaaaa\",\"exit_code\":1}\n",
            ),
        )
        .expect("write");
        assert_eq!(j.count_prior_same_hash("aaaaaaaaaaaaaaaa"), 2);
        assert_eq!(j.count_prior_same_hash("bbbbbbbbbbbbbbbb"), 1);
        assert_eq!(j.count_prior_same_hash("cccccccccccccccc"), 0);
    }

    #[test]
    fn count_prior_same_hash_sees_only_the_last_512_kib() {
        let (_dir, j) = tmp_journal();
        let old = "{\"code_hash\":\"aaaaaaaaaaaaaaaa\"}\n";
        let filler = "{\"x\":\"".to_string() + &"y".repeat(1000) + "\"}\n";
        let mut body = String::from(old);
        while body.len() < (TAIL_BYTES as usize) + 4096 {
            body.push_str(&filler);
        }
        body.push_str("{\"code_hash\":\"aaaaaaaaaaaaaaaa\"}\n");
        std::fs::write(j.path(), body).expect("write");
        assert_eq!(
            j.count_prior_same_hash("aaaaaaaaaaaaaaaa"),
            1,
            "the run older than the tail window faded out by design"
        );
    }

    #[test]
    fn find_prior_similar_respects_tau_orders_by_distance_and_reads_metadata() {
        let (_dir, j) = tmp_journal();
        let base: u64 = 0xF0F0_F0F0_F0F0_F0F0;
        let near = base ^ 0b11; // 2 bits
        let nearer = base ^ 0b1; // 1 bit
        let far = base ^ 0xFFFF; // 16 bits
        let line = |id: &str, h: u64, harvest: &str| {
            format!(
                "{{\"run_id\":\"{id}\",\"code_simhash_v2\":\"{h:016x}\",\"harvest\":{harvest}}}\n"
            )
        };
        let body = [
            line("far", far, "null"),
            line("near", near, "\"slug-a\""),
            line("nearer", nearer, "null"),
            "{\"run_id\":\"old\",\"code_simhash\":\"f0f0f0f0f0f0f0f0\"}\n".to_string(),
        ]
        .concat();
        std::fs::write(j.path(), body).expect("write");
        let s = j.find_prior_similar(SimhashVersion::V2, base, 9);
        let ids: Vec<_> = s
            .priors
            .iter()
            .map(|p| (p.run_id.as_str(), p.dist))
            .collect();
        assert_eq!(ids, [("nearer", 1), ("near", 2)]);
        assert_eq!(s.priors[1].harvest.as_deref(), Some("slug-a"));
        assert_eq!(s.priors[0].harvest, None);
        assert_eq!(s.malformed_lines, 0);
        // v1 and v2 never compare across versions: the v1 search sees only the v1 line.
        let v1 = j.find_prior_similar(SimhashVersion::V1, base, 9);
        assert_eq!(v1.priors.len(), 1);
        assert_eq!(v1.priors[0].run_id, "old");
        // τ is inclusive
        assert_eq!(
            j.find_prior_similar(SimhashVersion::V2, base, 1)
                .priors
                .len(),
            1
        );
        assert_eq!(
            j.find_prior_similar(SimhashVersion::V2, base, 0)
                .priors
                .len(),
            0
        );
    }

    #[test]
    fn find_prior_similar_counts_corrupt_tail_lines_and_fails_open_without_a_file() {
        let (_dir, j) = tmp_journal();
        assert!(
            j.find_prior_similar(SimhashVersion::V2, 0, 9)
                .priors
                .is_empty()
        );
        std::fs::write(
            j.path(),
            concat!(
                "{\"run_id\":\"a\",\"code_simhash_v2\":\"0000000000000000\"}\n",
                "{\"run_id\":\"b\",\"code_simhash_v2\":\"00000000\n",
                "{\"run_id\":\"c\",\"code_simhash_v2\":\"zzzz\"}\n",
            ),
        )
        .expect("write");
        let s = j.find_prior_similar(SimhashVersion::V2, 0, 9);
        assert_eq!(s.priors.len(), 1, "only `a` parses to a hash");
        assert_eq!(
            s.malformed_lines, 1,
            "the truncated line is COUNTED, not skipped silently"
        );
    }

    // ── the writer ───────────────────────────────────────────────────────────

    #[test]
    fn record_run_appends_one_line_and_returns_the_prior_count() {
        let (dir, _) = tmp_journal();
        let j = Journal::under_home(dir.path()); // dir `.claude/touring` does not exist yet
        let mut log = run_log("run-1", "bash", 3, outcome(0, None));
        log.code_hash = "1111111111111111";
        assert_eq!(j.record_run(&log), 0, "first run: nothing before it");
        log.run_id = "run-2";
        assert_eq!(
            j.record_run(&log),
            1,
            "second run sees the first, not itself"
        );
        let text = j.read_to_string().expect("journal exists");
        let ids: Vec<_> = text
            .lines()
            .map(|l| {
                let r: Value = serde_json::from_str(l).expect("each line is one record");
                (
                    r["run_id"].as_str().map(str::to_string),
                    r["prior_same_hash"].as_u64(),
                )
            })
            .collect();
        assert_eq!(
            ids,
            [
                (Some("run-1".into()), Some(0)),
                (Some("run-2".into()), Some(1))
            ]
        );
    }

    #[test]
    fn append_json_fails_open_when_the_directory_is_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let j = Journal::at(dir.path().join("no-such-dir").join("j.jsonl"));
        assert!(!j.append_json(&json!({"a": 1})));
    }

    /// Concurrent writers, small AND above `PIPE_BUF`: every line must still be
    /// one whole record — the splice the single-write discipline exists to prevent.
    #[test]
    fn concurrent_appends_never_splice_a_record() {
        let (_dir, j) = tmp_journal();
        let big = "z".repeat(PIPE_BUF * 3);
        std::thread::scope(|s| {
            for t in 0..8u32 {
                let j = &j;
                let big = &big;
                s.spawn(move || {
                    for i in 0..40u32 {
                        let rec = if i % 4 == 0 {
                            json!({"t": t, "i": i, "pad": big})
                        } else {
                            json!({"t": t, "i": i})
                        };
                        assert!(j.append_json(&rec));
                    }
                });
            }
        });
        let text = j.read_to_string().expect("journal");
        let mut seen = std::collections::HashSet::new();
        for line in text.lines() {
            let v: Value = serde_json::from_str(line).expect("no spliced/garbled line");
            seen.insert((v["t"].as_u64().expect("t"), v["i"].as_u64().expect("i")));
        }
        assert_eq!(seen.len(), 8 * 40, "every record landed exactly once");
    }

    /// T-1 (31/10/2026) — o journal NÃO pode ser corrompido por concorrência.
    ///
    /// O defeito: o registro era escrito em DUAS chamadas (o corpo e o `\n`),
    /// e dois processos emendavam entre elas — 400 linhas inválidas em
    /// 47.102 no journal real, 99 delas com 2+ `run_id` (ids 1 ms apartados).
    ///
    /// O teste re-executa o próprio binário de teste em N PROCESSOS (o modo
    /// filho é o MESMO teste, com a variável de ambiente que o faz só
    /// escrever — é o que impede a recursão). Cada processo anexa M
    /// registros; o journal final tem de ter ZERO linhas inválidas e N×M
    /// registros.
    #[test]
    fn journal_append_is_atomic_across_processes() {
        const FILHOS: usize = 12;
        const POR_FILHO: usize = 250;

        // Modo filho: só escreve e sai.
        if let Ok(dir) = std::env::var("TOURING_JOURNAL_RACE_DIR") {
            let journal = Journal::at(PathBuf::from(dir).join("run_journal.jsonl"));
            for i in 0..POR_FILHO {
                journal.append_json(&json!({
                    "run_id": format!("run-corrida-{i}"),
                    "language": "bash",
                    "ts": i,
                    "exit_code": 0,
                    "duration_ms": 1,
                    "failure_kind": Value::Null,
                    "bytes_elided": 0,
                    "code_hash_stdout_bytes": 0,
                    "code_hash": "abc",
                    "code_simhash": "0000000000000000",
                    "code_simhash_v2": "0000000000000000",
                    "prior": 0,
                }));
            }
            return;
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let exe = std::env::current_exe().expect("current_exe");
        let mut filhos = Vec::new();
        for _ in 0..FILHOS {
            let mut c = std::process::Command::new(&exe);
            c.env("TOURING_JOURNAL_RACE_DIR", dir.path());
            c.arg("journal_append_is_atomic_across_processes");
            c.arg("--nocapture");
            c.arg("--test-threads=1");
            filhos.push(c.spawn().expect("spawn do filho"));
        }
        for mut f in filhos {
            assert!(f.wait().expect("wait do filho").success(), "o filho falhou");
        }

        let conteudo = Journal::at(dir.path().join("run_journal.jsonl"))
            .read_to_string()
            .expect("lê o journal");
        let (mut invalidas, mut total) = (0u64, 0u64);
        for line in conteudo.lines() {
            match classify_line(line) {
                Line::Blank => {}
                Line::Malformed => {
                    total += 1;
                    invalidas += 1;
                }
                Line::Record(_) => total += 1,
            }
        }
        assert_eq!(invalidas, 0, "linhas inválidas no journal: {invalidas}");
        assert_eq!(
            total,
            (FILHOS * POR_FILHO) as u64,
            "registros: {total} (esperado {})",
            FILHOS * POR_FILHO
        );
    }
}
