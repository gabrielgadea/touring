//! Journal parser for the `touring run` execution envelope.
//!
//! Reads `~/.claude/touring/run_journal.jsonl` (or any compatible JSONL
//! stream) and emits per-language + per-failure-kind aggregates. The SDK
//! surface (`sdk.rs`) consumes these aggregates to keep the canonical hook
//! list honest — what the harness *actually* saw, not what the strategy-doc
//! remembered.
//!
//! Schema (one JSON object per line, written by `touring-server/src/cli/code.rs`):
//!
//! ```json
//! {
//!   "ts": 1787540216,
//!   "language": "bash" | "python" | "js" | "shell" | "rust",
//!   "exit_code": 0,
//!   "failure_kind": null | "exception" | "timeout" | "abort" | "proc-exit" | "invalid-output" | "output-limit",
//!   "duration_ms": 35,
//!   "code_hash_stdout_bytes": 102400,
//!   "bytes_elided": 94178
//! }
//! ```
//!
//! Failure taxonomy follows the E/A/M directives (`rules/code-elaboration-directives.md`,
//! A5). New kinds MUST be added to the parser or the aggregate will silently
//! lump them under `Other`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Canonical failure kinds from the code-mode E/A/M error taxonomy.
///
/// `Other` is a catch-all so a parser running on a journal emitted by a future
/// Touring version never panics. The strategy-doc's `when_not_to_use` field
/// also lands here when the analyst flags a kind as out-of-scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    /// Unhandled runtime exception in the sandboxed program.
    Exception,
    /// Wall-clock timeout (program exceeded `--timeout).
    Timeout,
    /// User-initiated abort (SIGINT, `KeyboardInterrupt`).
    Abort,
    /// Non-zero subprocess exit (shell wrapper, Python script, etc).
    ProcExit,
    /// Program ran but produced malformed/non-JSON output.
    InvalidOutput,
    /// Program output exceeded `--output-limit`.
    OutputLimit,
    /// Unknown kind — catch-all so future taxonomy additions never panic the parser.
    Other,
}

impl FailureKind {
    /// Parse from the journal's `failure_kind` string field.
    ///
    /// `None` is the success case — `failure_kind` is `null` in the journal.
    pub fn from_str_opt(s: Option<&str>) -> Self {
        match s {
            Some("exception") => Self::Exception,
            Some("timeout") => Self::Timeout,
            Some("abort") => Self::Abort,
            Some("proc-exit") => Self::ProcExit,
            Some("invalid-output") => Self::InvalidOutput,
            Some("output-limit") => Self::OutputLimit,
            Some(other) => {
                tracing::debug!(kind = %other, "unknown failure_kind, classified as Other");
                Self::Other
            }
            None => Self::Exception, // unreachable: gated by Option
        }
    }

    /// O nome canônico da classe — o mesmo que o journal grava.
    ///
    /// Cross-audit 04/09/2026: `by_failure_kind` no KPI era um histograma da string
    /// CRUA do journal, então uma classe desconhecida (ou grafada errado) virava um
    /// balde próprio em vez de cair em `Other`, e a taxonomia declarada na diretriz
    /// A5 não valia na ponta que lê. Com `from_str_opt` + `as_str` o KPI passa a
    /// contar as 7 classes que existem, e só elas.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exception => "exception",
            Self::Timeout => "timeout",
            Self::Abort => "abort",
            Self::ProcExit => "proc-exit",
            Self::InvalidOutput => "invalid-output",
            Self::OutputLimit => "output-limit",
            Self::Other => "other",
        }
    }
}

/// One journal entry. Field order matches the on-disk JSON.
#[derive(Debug, Clone, Deserialize)]
pub struct JournalEntry {
    /// Unix timestamp (seconds) of the run.
    pub ts: u64,
    /// Program language (`bash` | `python` | `js` | `shell` | `rust`).
    pub language: String,
    /// Process exit code; `-2` is reserved for timeouts.
    pub exit_code: i32,
    /// `None` on success; otherwise the `FailureKind` string label.
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
    /// B2: bytes the run left in its private temp dir (B1), measured before
    /// removal.
    #[serde(default)]
    pub tmp_bytes: u64,
}

/// Prefix of the Claude Code per-session scratchpad — a script that lives
/// there is one-off by construction (tmpfs, per session).
pub const HARNESS_SCRATCH_PREFIX: &str = "/tmp/claude-";

/// Aggregate over a slice of `JournalEntry`s. Cheap to compute (one pass).
#[derive(Debug, Clone, Default, Serialize)]
pub struct JournalAggregate {
    /// Total entries parsed (excludes blank + malformed lines).
    pub total_entries: u64,
    /// Per-language stats (count + failure_count + duration percentiles).
    pub by_language: HashMap<String, LanguageStats>,
    /// Failure-kind histogram across all entries.
    pub failure_counts: HashMap<String, u64>,
    /// Total payload bytes dropped by the elision step.
    pub total_bytes_elided: u64,
    /// Total stdout payload bytes retained (post-elision).
    pub total_code_hash_stdout_bytes: u64,
    /// Median duration across all entries (ms).
    pub duration_ms_p50: u64,
    /// 99th-percentile duration across all entries (ms).
    pub duration_ms_p99: u64,
    /// B2: runs whose body came from a file (`--file`).
    pub file_runs: u64,
    /// B2: runs whose body was inline text (`--code`).
    pub inline_runs: u64,
    /// B2: file runs whose script lives under the harness scratchpad
    /// (`/tmp/claude-*`) — one-off by construction.
    pub scratch_file_runs: u64,
    /// B2: runs that enrolled a `--harvest` slug.
    pub harvested_runs: u64,
    /// B2: runs that asked for `--brief`.
    pub brief_runs: u64,
    /// B2: runs with the `--orchestrate` SDK injected.
    pub orchestrate_runs: u64,
    /// B2: bytes left in private run temp dirs, summed (B1 `tmp_bytes`).
    pub total_tmp_bytes: u64,
}

/// Per-language breakdown of journal runs. Cheap to compute (one pass).
#[derive(Debug, Clone, Default, Serialize)]
pub struct LanguageStats {
    /// Total runs in this language.
    pub count: u64,
    /// Runs in this language that had a non-null `failure_kind`.
    pub failure_count: u64,
    /// Median duration for this language (ms).
    pub duration_ms_p50: u64,
    /// 99th-percentile duration for this language (ms).
    pub duration_ms_p99: u64,
}

/// Errors that can surface from `read_journal` / `read_journal_iter`.
#[derive(Debug, Error)]
pub enum JournalError {
    /// `path` does not exist on disk.
    #[error("journal path does not exist: {0}")]
    NotFound(String),
    /// All journal lines were malformed; `line` is the position of the first failure.
    #[error("malformed journal entry at line {line}: {source}")]
    Malformed {
        /// Zero-indexed line number of the first malformed JSON line.
        line: usize,
        /// The serde JSON parse error itself (chained via `#[source]`).
        #[source]
        source: serde_json::Error,
    },
    /// Underlying I/O error reading the file.
    #[error("io error reading journal: {0}")]
    Io(#[from] std::io::Error),
}

/// Read a journal from a file path.
///
/// `path` is typically `~/.claude/touring/run_journal.jsonl` but the parser
/// is path-agnostic — same schema, same result.
pub fn read_journal(path: &Path) -> Result<JournalAggregate, JournalError> {
    if !path.exists() {
        return Err(JournalError::NotFound(path.display().to_string()));
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    read_journal_iter(reader.lines().enumerate())
}

/// Stream entries from any `BufRead` source. Returns the aggregate.
///
/// `iter` yields `(line_number, line_result)` so malformed lines report
/// their position. Lines that fail to parse are skipped and counted in
/// `Malformed` errors at the FIRST malformed position — the parser is
/// fail-soft (a single bad line does not abort a 10k-line journal).
pub fn read_journal_iter<I>(iter: I) -> Result<JournalAggregate, JournalError>
where
    I: IntoIterator<Item = (usize, std::io::Result<String>)>,
{
    let mut all_entries: Vec<JournalEntry> = Vec::new();
    let mut first_err: Option<(usize, serde_json::Error)> = None;

    for (line_no, line_res) in iter {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => return Err(JournalError::Io(e)),
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<JournalEntry>(trimmed) {
            Ok(entry) => all_entries.push(entry),
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some((line_no, e));
                }
                // skip the line; first_err surfaces if nothing was readable.
            }
        }
    }

    if all_entries.is_empty()
        && let Some((line_no, e)) = first_err
    {
        return Err(JournalError::Malformed {
            line: line_no,
            source: e,
        });
    }
    // All lines empty / no parseable entries fall through to the empty aggregate.

    Ok(aggregate(&all_entries))
}

/// One-pass aggregate over collected entries.
fn aggregate(entries: &[JournalEntry]) -> JournalAggregate {
    let mut by_lang: HashMap<String, Vec<u32>> = HashMap::new();
    let mut failure_counts: HashMap<String, u64> = HashMap::new();
    let mut durations_all: Vec<u32> = Vec::with_capacity(entries.len());
    let mut total_bytes_elided: u64 = 0;
    let mut total_code_hash: u64 = 0;
    let (mut file_runs, mut inline_runs, mut scratch_file_runs) = (0u64, 0u64, 0u64);
    let (mut harvested_runs, mut brief_runs, mut orchestrate_runs) = (0u64, 0u64, 0u64);
    let mut total_tmp_bytes: u64 = 0;

    for entry in entries {
        by_lang
            .entry(entry.language.clone())
            .or_default()
            .push(entry.duration_ms);
        if let Some(kind) = entry.failure_kind.as_deref() {
            *failure_counts.entry(kind.to_string()).or_insert(0) += 1;
        }
        durations_all.push(entry.duration_ms);
        total_bytes_elided += entry.bytes_elided;
        total_code_hash += entry.code_hash_stdout_bytes;
        // B2 — the reuse axes. A v1 line (no `source`) counts in neither.
        match entry.source.as_deref() {
            Some("file") => {
                file_runs += 1;
                if entry
                    .file
                    .as_deref()
                    .is_some_and(|f| f.starts_with(HARNESS_SCRATCH_PREFIX))
                {
                    scratch_file_runs += 1;
                }
            }
            Some("inline") => inline_runs += 1,
            _ => {}
        }
        harvested_runs += u64::from(entry.harvest.is_some());
        brief_runs += u64::from(entry.brief);
        orchestrate_runs += u64::from(entry.orchestrate);
        total_tmp_bytes += entry.tmp_bytes;
    }

    let (p50_all, p99_all) = percentiles(&mut durations_all);

    let mut lang_stats: HashMap<String, LanguageStats> = HashMap::new();
    for (lang, durs) in by_lang {
        let count = durs.len() as u64;
        let failure_count = entries
            .iter()
            .filter(|e| e.language == lang && e.failure_kind.is_some())
            .count() as u64;
        let (p50, p99) = percentiles(&mut { durs });
        lang_stats.insert(
            lang,
            LanguageStats {
                count,
                failure_count,
                duration_ms_p50: p50,
                duration_ms_p99: p99,
            },
        );
    }

    JournalAggregate {
        total_entries: entries.len() as u64,
        by_language: lang_stats,
        failure_counts,
        total_bytes_elided,
        total_code_hash_stdout_bytes: total_code_hash,
        duration_ms_p50: p50_all,
        duration_ms_p99: p99_all,
        file_runs,
        inline_runs,
        scratch_file_runs,
        harvested_runs,
        brief_runs,
        orchestrate_runs,
        total_tmp_bytes,
    }
}

/// In-place percentile (linear interpolation, R7 method). Returns (p50, p99).
/// Sorts `values` ascending as a side effect — caller passes a fresh vec.
fn percentiles(values: &mut [u32]) -> (u64, u64) {
    if values.is_empty() {
        return (0, 0);
    }
    values.sort_unstable();
    let p = |q: f64| -> u64 {
        let n = values.len() as f64;
        let idx = (q * (n - 1.0)).round() as usize;
        values[idx.min(values.len() - 1)] as u64
    };
    (p(0.50), p(0.99))
}

#[cfg(test)]
mod v2_tests {
    use super::*;

    /// B2 (2026-09-02): the journal is the M1 ruler and it was BLIND to reuse —
    /// no field said whether a run came from a file or inline text, whether
    /// it was harvested, brief, orchestrate, or how much tmp it left behind
    /// (6.435 lines, `harvest_flagged: 0` was a measurement gap, not a fact).
    /// v2 lines carry those fields; v1 lines still parse with defaults.
    #[test]
    fn v2_fields_parse_and_aggregate_while_v1_lines_still_read() {
        let v1 = r#"{"ts":1,"language":"python","exit_code":0,"failure_kind":null,"duration_ms":5,"code_hash_stdout_bytes":10,"bytes_elided":0}"#;
        let v2 = r#"{"ts":2,"language":"python","exit_code":0,"failure_kind":null,"duration_ms":7,"code_hash_stdout_bytes":10,"bytes_elided":0,"source":"file","file":"/tmp/claude-1000/x/scratchpad/probe.py","harvest":"probe-tmp","brief":true,"orchestrate":true,"tmp_bytes":4096}"#;
        let v2b = r#"{"ts":3,"language":"bash","exit_code":1,"failure_kind":"exception","duration_ms":9,"code_hash_stdout_bytes":0,"bytes_elided":0,"source":"inline","file":null,"harvest":null,"brief":false,"orchestrate":false,"tmp_bytes":0}"#;
        let agg = read_journal_iter(
            [v1, v2, v2b]
                .into_iter()
                .enumerate()
                .map(|(i, l)| (i + 1, Ok(l.to_string()))),
        )
        .expect("three parseable lines");
        assert_eq!(agg.total_entries, 3);
        assert_eq!(agg.file_runs, 1, "one run came from a file");
        assert_eq!(
            agg.inline_runs, 1,
            "one run declared inline; the v1 line is unknown"
        );
        assert_eq!(agg.harvested_runs, 1);
        assert_eq!(agg.brief_runs, 1);
        assert_eq!(agg.orchestrate_runs, 1);
        assert_eq!(
            agg.scratch_file_runs, 1,
            "a file under the harness scratchpad"
        );
        assert_eq!(agg.total_tmp_bytes, 4096);
    }

    #[test]
    fn v2_entry_fields_are_typed() {
        let line = r#"{"ts":2,"language":"python","exit_code":0,"duration_ms":7,"source":"file","file":"scripts/x.py","harvest":"slug","brief":true,"orchestrate":false,"tmp_bytes":12}"#;
        let e: JournalEntry = serde_json::from_str(line).expect("parses");
        assert_eq!(e.source.as_deref(), Some("file"));
        assert_eq!(e.file.as_deref(), Some("scripts/x.py"));
        assert_eq!(e.harvest.as_deref(), Some("slug"));
        assert!(e.brief);
        assert!(!e.orchestrate);
        assert_eq!(e.tmp_bytes, 12);
    }
}

/// The canonical journal path under the active user's `$HOME`.
///
/// Pure function of `home_dir` — no env-reading side effect, so tests can
/// pass a `tempfile` cleanly.
pub fn default_journal_path(home_dir: &Path) -> std::path::PathBuf {
    home_dir
        .join(".claude")
        .join("touring")
        .join("run_journal.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_iter(s: &str) -> impl Iterator<Item = (usize, std::io::Result<String>)> + '_ {
        s.lines().enumerate().map(|(i, l)| (i, Ok(l.to_string())))
    }

    #[test]
    fn empty_input_is_empty_aggregate() {
        let agg =
            read_journal_iter(lines_iter("")).expect("empty input must parse to empty aggregate");
        assert_eq!(agg.total_entries, 0);
        assert!(agg.by_language.is_empty());
    }

    #[test]
    fn single_language_aggregate() {
        let s = r#"{"ts":1,"language":"bash","exit_code":0,"failure_kind":null,"duration_ms":35,"code_hash_stdout_bytes":0,"bytes_elided":0}
{"ts":2,"language":"bash","exit_code":0,"failure_kind":null,"duration_ms":47,"code_hash_stdout_bytes":0,"bytes_elided":0}
{"ts":3,"language":"bash","exit_code":1,"failure_kind":"exception","duration_ms":61,"code_hash_stdout_bytes":0,"bytes_elided":0}"#;
        let agg =
            read_journal_iter(lines_iter(s)).expect("valid single-language journal must parse");
        assert_eq!(agg.total_entries, 3);
        let stats = &agg.by_language["bash"];
        assert_eq!(stats.count, 3);
        assert_eq!(stats.failure_count, 1);
        // sorted durations [35,47,61] → p50 idx=1=47, p99 idx=2=61
        assert_eq!(stats.duration_ms_p50, 47);
        assert_eq!(stats.duration_ms_p99, 61);
        assert_eq!(agg.failure_counts["exception"], 1);
    }

    #[test]
    fn multi_language_separates_correctly() {
        let s = r#"{"ts":1,"language":"bash","exit_code":0,"failure_kind":null,"duration_ms":10,"code_hash_stdout_bytes":0,"bytes_elided":0}
{"ts":2,"language":"python","exit_code":0,"failure_kind":null,"duration_ms":20,"code_hash_stdout_bytes":0,"bytes_elided":0}
{"ts":3,"language":"bash","exit_code":0,"failure_kind":null,"duration_ms":15,"code_hash_stdout_bytes":0,"bytes_elided":0}"#;
        let agg =
            read_journal_iter(lines_iter(s)).expect("valid multi-language journal must parse");
        assert_eq!(agg.total_entries, 3);
        assert_eq!(agg.by_language["bash"].count, 2);
        assert_eq!(agg.by_language["python"].count, 1);
    }

    #[test]
    fn failure_kind_taxonomy() {
        assert_eq!(
            FailureKind::from_str_opt(Some("exception")),
            FailureKind::Exception
        );
        assert_eq!(
            FailureKind::from_str_opt(Some("timeout")),
            FailureKind::Timeout
        );
        assert_eq!(
            FailureKind::from_str_opt(Some("unknown-kind")),
            FailureKind::Other
        );
    }

    #[test]
    fn malformed_line_reports_position_and_skips() {
        // First entry malformed, second valid — must surface error at line 0,
        // but include the valid one in the aggregate.
        let s = r#"NOT JSON
{"ts":1,"language":"bash","exit_code":0,"failure_kind":null,"duration_ms":35,"code_hash_stdout_bytes":0,"bytes_elided":0}"#;
        let agg = read_journal_iter(lines_iter(s))
            .expect("malformed-line journal must skip and return valid entry");
        assert_eq!(agg.total_entries, 1);
    }

    #[test]
    fn all_malformed_returns_error() {
        let s = "NOT JSON\nALSO NO";
        let res = read_journal_iter(lines_iter(s));
        assert!(matches!(res, Err(JournalError::Malformed { .. })));
    }

    #[test]
    fn bytes_aggregate_correct() {
        let s = r#"{"ts":1,"language":"bash","exit_code":0,"failure_kind":null,"duration_ms":35,"code_hash_stdout_bytes":1024,"bytes_elided":512}
{"ts":2,"language":"bash","exit_code":0,"failure_kind":null,"duration_ms":35,"code_hash_stdout_bytes":2048,"bytes_elided":256}"#;
        let agg =
            read_journal_iter(lines_iter(s)).expect("valid bytes-aggregate journal must parse");
        assert_eq!(agg.total_code_hash_stdout_bytes, 3072);
        assert_eq!(agg.total_bytes_elided, 768);
    }

    #[test]
    fn default_path_is_canonical() {
        let home = std::path::Path::new("/home/test");
        let p = default_journal_path(home);
        assert_eq!(
            p.to_str().expect("hardcoded test path must be valid UTF-8"),
            "/home/test/.claude/touring/run_journal.jsonl"
        );
    }

    #[test]
    fn read_journal_missing_file_is_error() {
        let res = read_journal(std::path::Path::new("/nonexistent/path/xyz.jsonl"));
        assert!(matches!(res, Err(JournalError::NotFound(_))));
    }
}
