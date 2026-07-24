//! C5 — Active Output Summarizer: inline, metadata-first digest of sandbox output.
//!
//! The CEG truncates captured stdout at `max_output_bytes` and otherwise returns
//! only a `content_hash` reference. The Codex pathology ("Is Grep All You Need?")
//! shows an agent that must re-read a file-based ref degrades sharply (93→55%),
//! and — worse — a failed run can reach the model as an opaque hash, **masking the
//! failure**.
//!
//! [`summarize_output`] builds an [`OutputSummary`]: an inline, metadata-first digest
//! (<200 tokens) that preserves the **exit code, error signatures and `file:line`
//! references**, while the full output stays on disk (on-demand by `content_hash`).
//!
//! Design: `docs/2026-06-27-c5-active-summarizer-design.md`. The summarizer is **pure**
//! (no I/O, deterministic) and fully unit-tested here; it is the testable heart of C5.
//!
//! ## Anti-masking invariant (N3 ↔ I4) — non-negotiable
//!
//! `exit_code != 0` ⟹ the summary carries the exit code AND at least one error
//! signature OR the tail of the output. A failure is **never** reduced to a silent hash.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Max error-signature lines retained (verbatim, head of output order).
const MAX_ERROR_LINES: usize = 8;
/// Max deduped `file:line` references retained.
const MAX_FILE_REFS: usize = 12;
/// Head/tail line count kept when no errors dominate (K first + K last).
const HEAD_TAIL_K: usize = 3;
/// Per-line character cap to bound token usage of any single retained line.
const MAX_LINE_LEN: usize = 200;

/// `path.ext:line[:col]` reference — e.g. `src/foo.rs:10:5` or `test_x.py:42`.
static FILE_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([\w./\-]+\.[A-Za-z][A-Za-z0-9]*):(\d+)(?::\d+)?").expect("valid static regex")
});

/// `<n> passed|failed|error[s]|warning[s]` count phrases (test/compiler summaries).
static COUNT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\d+)\s+(passed|failed|errors?|warnings?)").expect("valid static regex")
});

/// Inline, metadata-first digest of a (possibly truncated) sandbox capture.
///
/// Token budget: < 200 tokens when serialized. NEVER masks a failure (see module docs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSummary {
    /// Subprocess exit code, verbatim (`-1` timeout/spawn-fail, `-2` dry-run skip).
    pub exit_code: i32,
    /// Total stdout bytes captured (so the model knows the real size).
    pub total_bytes: u64,
    /// First error-signature lines (`error`, `panic`, `Traceback`, pytest `E`, …), verbatim.
    pub error_lines: Vec<String>,
    /// Deduped `file:line` references extracted from the output.
    pub file_refs: Vec<String>,
    /// Counts by normalized class (`passed` / `failed` / `error` / `warning`).
    pub counts: BTreeMap<String, u32>,
    /// First K + last K lines (head/tail retention) when no errors dominate.
    pub head_tail: Vec<String>,
    /// `true` when the full output was clipped (full available on-demand by `content_hash`).
    pub truncated: bool,
}

impl OutputSummary {
    /// An empty summary for paths that captured no output (timeout, spawn failure,
    /// pure-code skip, dry-run sentinel). Still records the exit code — a non-zero
    /// `exit_code` here is itself the (unmasked) failure signal.
    #[must_use]
    pub fn empty(exit_code: i32) -> Self {
        Self {
            exit_code,
            total_bytes: 0,
            error_lines: Vec::new(),
            file_refs: Vec::new(),
            counts: BTreeMap::new(),
            head_tail: Vec::new(),
            truncated: false,
        }
    }

    /// `true` when this summary represents a failure (non-zero exit).
    #[must_use]
    pub fn is_failure(&self) -> bool {
        self.exit_code != 0
    }
}

/// Clip a line to [`MAX_LINE_LEN`] characters (char-boundary safe) and trim the end.
fn clip(line: &str) -> String {
    let line = line.trim_end();
    if line.chars().count() <= MAX_LINE_LEN {
        return line.to_string();
    }
    let truncated: String = line.chars().take(MAX_LINE_LEN).collect();
    format!("{truncated}…")
}

/// Whether a line carries an error signature. Matches LINE-LEADING forms only
/// (low false-positive: a benign "no errors" line never leads with `error`).
fn is_error_line(line: &str) -> bool {
    let t = line.trim_start();
    if t.is_empty() {
        return false;
    }
    let tl = t.to_ascii_lowercase();
    tl.starts_with("error")              // rustc `error[E0382]`, generic `error:`
        || tl.starts_with("panicked at")  // rust panic body
        || (tl.starts_with("thread '") && tl.contains("panicked")) // rust panic header
        || tl.starts_with("traceback (")  // python
        || tl.starts_with("fatal")        // git/linker/clang `fatal:` / `fatal error`
        || tl.starts_with("failed")       // generic / cargo
        || is_pytest_error(t)             // pytest `E   ...`
        || is_python_exception(t) // `FooError: ...` / `FooException: ...`
}

/// pytest reports assertion details on lines that start with `E` + ≥2 spaces.
fn is_pytest_error(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() >= 3 && b[0] == b'E' && b[1] == b' ' && b[2] == b' '
}

/// A leading `SomethingError:` / `SomethingException:` token (python tracebacks).
fn is_python_exception(t: &str) -> bool {
    let head: String = t
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (head.ends_with("Error") || head.ends_with("Exception"))
        && t.len() > head.len()
        && t[head.len()..].starts_with(':')
}

/// Extract deduped `file:line` references, capped at [`MAX_FILE_REFS`].
fn extract_file_refs(output: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for cap in FILE_REF_RE.captures_iter(output) {
        let r = format!("{}:{}", &cap[1], &cap[2]);
        if !seen.contains(&r) {
            seen.push(r);
            if seen.len() >= MAX_FILE_REFS {
                break;
            }
        }
    }
    seen
}

/// Accumulate `<n> <class>` counts, normalizing plurals (`errors`→`error`).
fn extract_counts(output: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for cap in COUNT_RE.captures_iter(output) {
        let Ok(n) = cap[1].parse::<u32>() else {
            continue;
        };
        let class = cap[2].to_ascii_lowercase();
        let key = class.strip_suffix('s').unwrap_or(&class).to_string();
        *counts.entry(key).or_insert(0) += n;
    }
    counts
}

/// First K + last K non-empty lines (deduped when they overlap on short output).
fn head_tail_lines(lines: &[&str]) -> Vec<String> {
    let non_empty: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty())
        .collect();
    if non_empty.len() <= HEAD_TAIL_K * 2 {
        return non_empty.iter().map(|l| clip(l)).collect();
    }
    let mut out: Vec<String> = non_empty
        .iter()
        .take(HEAD_TAIL_K)
        .map(|l| clip(l))
        .collect();
    out.extend(
        non_empty
            .iter()
            .rev()
            .take(HEAD_TAIL_K)
            .rev()
            .map(|l| clip(l)),
    );
    out
}

/// Build an [`OutputSummary`] from raw stdout + the real exit code.
///
/// PURE: no I/O, deterministic. Enforces the anti-masking invariant — when
/// `exit_code != 0` and no error line matched, the output tail is forced into
/// `head_tail` so the failure is always visible inline.
#[must_use]
pub fn summarize_output(output: &str, exit_code: i32, truncated: bool) -> OutputSummary {
    let total_bytes = output.len() as u64;
    let lines: Vec<&str> = output.lines().collect();

    let error_lines: Vec<String> = lines
        .iter()
        .filter(|l| is_error_line(l))
        .take(MAX_ERROR_LINES)
        .map(|l| clip(l))
        .collect();

    let file_refs = extract_file_refs(output);
    let counts = extract_counts(output);

    // head/tail only when errors do not already dominate the digest.
    let mut head_tail = if error_lines.is_empty() {
        head_tail_lines(&lines)
    } else {
        Vec::new()
    };

    // Anti-masking invariant: a failure must never be a silent hash.
    if exit_code != 0 && error_lines.is_empty() && head_tail.is_empty() {
        head_tail = lines
            .iter()
            .rev()
            .take(HEAD_TAIL_K)
            .rev()
            .map(|l| clip(l))
            .collect();
    }

    OutputSummary {
        exit_code,
        total_bytes,
        error_lines,
        file_refs,
        counts,
        head_tail,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_output_has_no_errors() {
        let s = summarize_output("compiling\nFinished in 2s\n", 0, false);
        assert!(s.error_lines.is_empty());
        assert!(!s.head_tail.is_empty()); // head/tail retained for context
        assert!(!s.is_failure());
    }

    #[test]
    fn rustc_error_extracts_line_and_file_ref() {
        let out = "error[E0382]: borrow of moved value\n --> src/foo.rs:10:5\n";
        let s = summarize_output(out, 1, false);
        assert!(s.error_lines.iter().any(|l| l.contains("E0382")));
        assert!(s.file_refs.contains(&"src/foo.rs:10".to_string()));
        assert!(s.is_failure());
    }

    #[test]
    fn pytest_failure_counts_and_error_line() {
        let out = "E   assert 1 == 2\n=== 3 failed, 5 passed in 0.1s ===\n";
        let s = summarize_output(out, 1, false);
        assert!(s.error_lines.iter().any(|l| l.contains("assert 1 == 2")));
        assert_eq!(s.counts.get("failed"), Some(&3));
        assert_eq!(s.counts.get("passed"), Some(&5));
    }

    #[test]
    fn python_traceback_detected() {
        let out = "Traceback (most recent call last):\n  File \"x.py\", line 5\nValueError: bad\n";
        let s = summarize_output(out, 1, false);
        assert!(s.error_lines.iter().any(|l| l.starts_with("Traceback")));
        assert!(s.error_lines.iter().any(|l| l.starts_with("ValueError:")));
    }

    #[test]
    fn preserves_exit_code() {
        assert_eq!(summarize_output("x", 137, false).exit_code, 137);
        assert_eq!(OutputSummary::empty(-1).exit_code, -1);
    }

    #[test]
    fn failure_never_masked() {
        // Non-zero exit with output that matches NO error pattern → tail forced.
        let out = "doing work\nstep one\nstep two\nstep three done\n";
        let s = summarize_output(out, 2, false);
        assert!(s.is_failure());
        assert!(
            !s.head_tail.is_empty() || !s.error_lines.is_empty(),
            "a failure must always surface error_lines or head_tail"
        );
        assert!(s.head_tail.iter().any(|l| l.contains("step three done")));
    }

    #[test]
    fn clean_word_error_in_prose_is_not_flagged() {
        // "no errors found" must NOT be treated as an error line (line-leading only).
        let s = summarize_output("scan complete: no errors found\n", 0, false);
        assert!(s.error_lines.is_empty());
    }

    #[test]
    fn long_line_is_clipped() {
        let long = "error: ".to_string() + &"x".repeat(500);
        let s = summarize_output(&long, 1, false);
        assert_eq!(s.error_lines.len(), 1);
        assert!(s.error_lines[0].chars().count() <= MAX_LINE_LEN + 1); // +1 for the ellipsis
    }

    #[test]
    fn truncated_flag_propagates() {
        assert!(summarize_output("partial", 0, true).truncated);
        assert!(!summarize_output("complete", 0, false).truncated);
    }

    #[test]
    fn file_refs_deduped_and_capped() {
        let mut out = String::new();
        for i in 0..20 {
            out.push_str(&format!("at src/m{i}.rs:{i}\n"));
        }
        out.push_str("again src/m0.rs:0\n"); // duplicate
        let s = summarize_output(&out, 1, false);
        assert!(s.file_refs.len() <= MAX_FILE_REFS);
        assert_eq!(
            s.file_refs.iter().filter(|r| *r == "src/m0.rs:0").count(),
            1
        );
    }

    #[test]
    fn empty_output_clean_exit_is_minimal() {
        let s = summarize_output("", 0, false);
        assert!(s.error_lines.is_empty() && s.head_tail.is_empty() && s.file_refs.is_empty());
        assert_eq!(s.total_bytes, 0);
    }
}
