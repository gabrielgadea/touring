//! SDK signal mirror — append-only JSONL sink consumed by `SignalReport::load`.
//!
//! ## Why a mirror
//!
//! The `touring run` journal (`~/.claude/touring/run_journal.jsonl`) records
//! program execution envelope (lang, exit, duration, bytes). It does NOT
//! record which hooks a program called — those calls live in the daemon's
//! RPC traffic and never reach the journal file.
//!
//! This module is the **bridge**: every `touring run` execution that
//! touched an SDK hook gets a JSON line appended to `sdk_signal_mirror.jsonl`.
//! `signal_report_from_journal` then AUGMENTS its base report with these
//! hook-level calls, so the `HookSignal::call_count` field finally reflects
//! reality instead of zero.
//!
//! ## Why append-only
//!
//! Two reasons:
//! 1. Crash safety — a torn write loses a line, not the whole file. The
//!    signal report rebuild is idempotent (re-deriving from the mirror is
//!    cheaper than maintaining a mutable index).
//! 2. The mirror is a journal, not a state. `signal_report_from_journal`
//!    is the reader; nothing else mutates it.
//!
//! ## Path
//!
//! `~/.claude/touring/sdk_signal_mirror.jsonl` — same dir as the run
//! journal, same SEG-2 allowlist path.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::journal;
use crate::sdk::{HookName, HookSignal, SignalReport};

/// Canonical mirror file path under the active user's `$HOME.
pub fn default_mirror_path(home_dir: &Path) -> PathBuf {
    home_dir.join(".claude").join("touring").join("sdk_signal_mirror.jsonl")
}

/// One mirror entry — one hook call observed by PostToolUse.
///
/// Schema mirrors the Rust `SignalReport::hooks` key (canonical `as_str`),
/// not the camelCase variant name. The two emitters (PostToolUse handler
/// + signal report reader) MUST agree on this string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCallEntry {
    /// Unix timestamp (seconds) of the hook call.
    pub ts: u64,
    /// Canonical hook name (`ast_meta`, `parallel`, etc).
    pub hook_name: String,
    /// How long the hook took (ms).
    pub duration_ms: u32,
    /// `true` if the hook returned a result the caller could consume;
    /// `false` if it errored / timed out / returned empty.
    pub success: bool,
    /// Who wrote the line (F9-origem, 2026-09-02): `post_bash` for the
    /// PostToolUse/PostToolUseFailure feeder, `sdk` for in-sandbox
    /// `--orchestrate` queries. `None` on lines written before the field
    /// existed — readers treat it as unknown, never as a post-bash delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

/// The writer of a mirror line. `hooks_complement` divides only the
/// `PostBash` deliveries by the post-bash dispatch count; before this tag the
/// SDK's own queries inflated that ratio to 5.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MirrorOrigin {
    /// The `post_bash` hook handler classifying a real `touring …` command.
    PostBash,
    /// The Python SDK injected by `touring run --orchestrate`.
    Sdk,
}

impl MirrorOrigin {
    /// Canonical snake_case string — the exact value written to the mirror
    /// and matched by the KPI reader (`post_bash` / `sdk`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PostBash => "post_bash",
            Self::Sdk => "sdk",
        }
    }
}

/// Aggregate over the mirror — sums calls + failures + durations by hook.
///
/// Mirrors `JournalAggregate::by_language` shape (HashMap by key) but
/// computed over mirror entries, not journal entries.
#[derive(Debug, Clone, Default)]
pub struct MirrorAggregate {
    /// Total hook calls recorded across the entire mirror.
    pub total_calls: u64,
    /// Total calls that returned a non-success status.
    pub total_failures: u64,
    /// Per-hook breakdown (key = canonical `HookName::as_str`).
    pub by_hook: std::collections::HashMap<String, HookCallCounts>,
}

/// Per-hook aggregate carried by `MirrorAggregate::by_hook`.
#[derive(Debug, Clone, Default)]
pub struct HookCallCounts {
    /// Total invocations of this hook.
    pub call_count: u64,
    /// Invocations that returned non-success.
    pub failure_count: u64,
    /// Per-call durations for percentile math (sorted lazily).
    pub durations: Vec<u32>,
}

impl MirrorAggregate {
    /// Median duration across all hook calls in the mirror (ms).
    pub fn duration_p50(&self) -> u64 {
        let mut all: Vec<u32> = self
            .by_hook
            .values()
            .flat_map(|c| c.durations.iter().copied())
            .collect();
        percentiles(&mut all).0
    }
    /// 99th-percentile duration across all hook calls in the mirror (ms).
    pub fn duration_p99(&self) -> u64 {
        let mut all: Vec<u32> = self
            .by_hook
            .values()
            .flat_map(|c| c.durations.iter().copied())
            .collect();
        percentiles(&mut all).1
    }
}

/// Errors that can surface from `record` / `read` / `augment_with_mirror`.
#[derive(Debug, Error)]
pub enum MirrorError {
    /// Underlying I/O error (open, append, mkdir).
    #[error("io error writing mirror: {0}")]
    Io(#[from] std::io::Error),
    /// A line in the mirror failed JSON parsing; `line` is its 0-indexed position.
    #[error("malformed mirror entry at line {line}: {source}")]
    Malformed {
        /// Zero-indexed line number of the malformed JSON entry.
        line: usize,
        /// The serde JSON parse error itself (chained via `#[source]`).
        #[source]
        source: serde_json::Error,
    },
    /// A hook name string did not match any canonical `HookName` variant.
    #[error("unknown hook name: {0}")]
    UnknownHook(String),
}

/// Append one hook-call entry to the mirror. Called by PostToolUse handler.
///
/// Returns the path that was appended. The parent dir is created lazily
/// (`~/.claude/touring/` exists by convention; we mkdir if absent).
pub fn record(
    path: &Path,
    hook: HookName,
    duration_ms: u32,
    success: bool,
    origin: MirrorOrigin,
) -> Result<PathBuf, MirrorError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let entry = HookCallEntry {
        ts: now_unix(),
        hook_name: hook.as_str().to_string(),
        duration_ms,
        success,
        origin: Some(origin.as_str().to_string()),
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(&entry)
        .map_err(|e| MirrorError::Malformed { line: 0, source: e })?;
    writeln!(file, "{}", line)?;
    Ok(path.to_path_buf())
}

/// Read the entire mirror and return the aggregate.
pub fn read(path: &Path) -> Result<MirrorAggregate, MirrorError> {
    if !path.exists() {
        return Ok(MirrorAggregate::default());
    }
    let content = fs::read_to_string(path)?;
    let mut agg = MirrorAggregate::default();
    for (line_no, raw) in content.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let entry: HookCallEntry = serde_json::from_str(trimmed).map_err(|e| {
            MirrorError::Malformed {
                line: line_no,
                source: e,
            }
        })?;
        let counts = agg.by_hook.entry(entry.hook_name).or_default();
        counts.call_count += 1;
        if !entry.success {
            counts.failure_count += 1;
        }
        counts.durations.push(entry.duration_ms);
        agg.total_calls += 1;
        if !entry.success {
            agg.total_failures += 1;
        }
    }
    Ok(agg)
}

/// Augment a `SignalReport` with mirror data. The base report's hook stats
/// are zeroed (today); this function fills them in from the mirror so the
/// downstream consumer sees real counts.
///
/// Existing entries with `call_count > 0` are left alone — the base report
/// may have hardcoded stats we don't want to clobber.
pub fn augment_with_mirror(report: &mut SignalReport, agg: &MirrorAggregate) {
    for (hook_name, counts) in &agg.by_hook {
        let entry = report.hooks.entry(hook_name.clone()).or_default();
        if entry.call_count == 0 {
            entry.call_count = counts.call_count;
            entry.failure_count = counts.failure_count;
            let (p50, p99) = percentiles(&mut counts.durations.clone());
            entry.duration_ms_p50 = p50;
            entry.duration_ms_p99 = p99;
        }
    }
}

/// Convenience: build a fresh `SignalReport` from BOTH the journal and the
/// mirror. Equivalent to `signal_report_from_journal` + `augment_with_mirror`.
pub fn signal_report_from_journal_and_mirror(
    journal_path: &Path,
    mirror_path: &Path,
) -> Result<SignalReport, Box<dyn std::error::Error>> {
    let mut report = journal::read_journal(journal_path)
        .map_err(|e| anyhow::anyhow!("journal: {e}"))?;
    // Bridge: we only need the aggregate bits — build a SignalReport
    // shape on the fly and call augment.
    let mut partial = SignalReport {
        generated_at_unix: now_unix(),
        source_journal_path: journal_path.display().to_string(),
        total_runs: report.total_entries,
        hooks: std::collections::BTreeMap::new(),
        by_language: std::collections::BTreeMap::new(),
        failure_taxonomy: std::collections::BTreeMap::new(),
    };
    for (lang, stats) in &report.by_language {
        partial.by_language.insert(
            lang.clone(),
            crate::sdk::JournalLanguageStats {
                count: stats.count,
                failure_count: stats.failure_count,
            },
        );
    }
    for (kind, count) in &report.failure_counts {
        partial.failure_taxonomy.insert(kind.clone(), *count);
    }
    for hook in HookName::ALL {
        partial.hooks.insert(
            hook.as_str().to_string(),
            HookSignal::default(),
        );
    }

    let agg = read(mirror_path)?;
    augment_with_mirror(&mut partial, &agg);

    // The journal summary gets discarded — only the augmented SignalReport
    // shape survives. Caller gets back `report` so they can compare
    // journal-only stats separately if needed.
    report.total_entries = partial.total_runs;
    Ok(partial)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn percentiles(values: &mut [u32]) -> (u64, u64) {
    if values.is_empty() {
        return (0, 0);
    }
    values.sort_unstable();
    let n = values.len() as f64;
    let p = |q: f64| -> u64 {
        let idx = (q * (n - 1.0)).round() as usize;
        values[idx.min(values.len() - 1)] as u64
    };
    (p(0.50), p(0.99))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_mirror(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("sdk-mirror-{}-{}-{name}.jsonl", std::process::id(), now_unix()));
        p
    }

    #[test]
    fn empty_mirror_returns_default() {
        let p = tmp_mirror("none");
        std::fs::write(&p, "").unwrap_or(());
        let r = read(&p).expect("read empty mirror");
        assert_eq!(r.total_calls, 0);
        assert_eq!(r.by_hook.len(), 0);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn record_appends_valid_jsonl() {
        let p = tmp_mirror("append");
        for _ in 0..3 {
            record(&p, HookName::AstMeta, 12, true, MirrorOrigin::Sdk).expect("record ast_meta");
        }
        record(&p, HookName::Parallel, 5, false, MirrorOrigin::Sdk).expect("record parallel fail");
        let r = read(&p).expect("read after appends");
        assert_eq!(r.total_calls, 4);
        assert_eq!(r.total_failures, 1);
        assert_eq!(r.by_hook["ast_meta"].call_count, 3);
        assert_eq!(r.by_hook["ast_meta"].failure_count, 0);
        assert_eq!(r.by_hook["parallel"].call_count, 1);
        assert_eq!(r.by_hook["parallel"].failure_count, 1);
        std::fs::remove_file(&p).ok();
    }

    // ── F9-origem (2026-09-02) ───────────────────────────────────────────
    // The `hooks_complement` ratio divided EVERY mirror delivery by the
    // post-bash dispatch count; the SDK's in-sandbox queries inflated it to
    // 5.0. Each line now says who wrote it.

    #[test]
    fn record_writes_the_origin_of_the_delivery() {
        let p = tmp_mirror("origin");
        record(&p, HookName::IndexFind, 3, true, MirrorOrigin::PostBash).expect("record");
        let raw = std::fs::read_to_string(&p).expect("mirror text");
        assert!(raw.contains(r#""origin":"post_bash""#), "line: {raw}");
        let entry: HookCallEntry = serde_json::from_str(raw.trim()).expect("entry parses");
        assert_eq!(entry.origin.as_deref(), Some("post_bash"));
        assert_eq!(MirrorOrigin::Sdk.as_str(), "sdk");
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn legacy_line_without_origin_still_parses_and_counts() {
        let p = tmp_mirror("legacy");
        std::fs::write(
            &p,
            "{\"ts\":1788300000,\"hook_name\":\"ast_meta\",\"duration_ms\":1,\"success\":true}\n",
        )
        .expect("write legacy line");
        let entry: HookCallEntry =
            serde_json::from_str(std::fs::read_to_string(&p).expect("text").trim())
                .expect("legacy entry parses");
        assert_eq!(entry.origin, None, "absent origin is unknown, never invented");
        let r = read(&p).expect("read legacy mirror");
        assert_eq!(r.total_calls, 1);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn augment_zeroed_hooks_get_mirror_counts() {
        let mut report = SignalReport::default();
        for h in HookName::ALL {
            report.hooks.insert(h.as_str().to_string(), HookSignal::default());
        }
        let mut agg = MirrorAggregate::default();
        let counts = agg.by_hook.entry("ast_meta".to_string()).or_default();
        counts.call_count = 7;
        counts.failure_count = 1;
        counts.durations.push(10);
        counts.durations.push(20);
        counts.durations.push(30);

        augment_with_mirror(&mut report, &agg);

        let entry = &report.hooks["ast_meta"];
        assert_eq!(entry.call_count, 7);
        assert_eq!(entry.failure_count, 1);
        assert_eq!(entry.duration_ms_p50, 20); // sorted [10,20,30] → idx 1
        assert_eq!(entry.duration_ms_p99, 30);
    }

    #[test]
    fn augment_does_not_clobber_existing() {
        // If a hook already has call_count > 0 (e.g. hardcoded stat), the
        // mirror MUST NOT overwrite it. The base report is the authority
        // for hardcoded stats; the mirror fills in the gaps.
        let mut report = SignalReport::default();
        let mut entry = HookSignal::default();
        entry.call_count = 999;
        report.hooks.insert("ast_meta".to_string(), entry);

        let mut agg = MirrorAggregate::default();
        agg.by_hook
            .entry("ast_meta".to_string())
            .or_default()
            .call_count = 7;

        augment_with_mirror(&mut report, &agg);

        assert_eq!(report.hooks["ast_meta"].call_count, 999);
    }

    #[test]
    fn default_mirror_path_is_canonical() {
        let home = std::path::Path::new("/home/test");
        let p = default_mirror_path(home);
        assert_eq!(
            p.to_str().expect("utf8"),
            "/home/test/.claude/touring/sdk_signal_mirror.jsonl"
        );
    }

    #[test]
    fn read_missing_returns_default_not_error() {
        let r = read(std::path::Path::new("/nonexistent/mirror.jsonl"))
        .expect("read missing mirror is empty aggregate");
        assert_eq!(r.total_calls, 0);
    }
}