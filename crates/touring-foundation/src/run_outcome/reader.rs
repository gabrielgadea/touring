//! Reading the journal: the two views of a line.
//!
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

use serde::Deserialize;
use serde_json::Value;

use super::outcome::{ExitStatus, FailureKind, RunOutcome};

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

/// What a valid-JSON line IS, for the rulers that must not count a refusal as
/// a failed run.
///
/// The lenient view ([`classify_line`]) answers only "is this parseable"; a
/// ruler that counts RUNS also has to say which of the parseable lines were
/// runs at all. Two families are not:
///
/// - [`LineFamily::Refusal`] — the repeat-gate's refusal event: the program was
///   never started, so it is not a failed run, it is NO run;
/// - [`LineFamily::NotObject`] — valid JSON that is not an object (`123`, `"x"`):
///   it carries no record at all.
///
/// Everything else is a run record — INCLUDING a record whose every field is
/// absent (the lenient view has always counted it) and a record whose exit
/// code is unusable (`0.0`, `"0"`): the family says what the line IS, the
/// outcome says how it went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineFamily {
    /// A run record: the line describes a process that ran.
    Run,
    /// The repeat-gate's refusal event (`repeat_blocked`) — the program never ran.
    Refusal,
    /// Valid JSON that is not an object (`123`, `"x"`) — no record at all.
    NotObject,
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

    /// What this line IS — a run record, a refusal event, or a line that is
    /// not even an object.
    ///
    /// The classification lives HERE, next to the reader that parses the line:
    /// the same question asked in two places gets two answers. A ruler that
    /// counts RUNS asks the family first and the outcome second.
    #[must_use]
    pub fn family(&self) -> LineFamily {
        if !self.value.is_object() {
            return LineFamily::NotObject;
        }
        if self.value.get("repeat_blocked").is_some() {
            return LineFamily::Refusal;
        }
        LineFamily::Run
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
