//! S4 SDK surface — the 8 hardcoded functions available to a `touring run`
//! program under `--orchestrate`. Mirrors the Python Protocol prelude typed
//! statically at the head of the in-sandbox prompt.
//!
//! ## Hybrid model (strategy v1.1, decision D3)
//!
//! - **Names hardcoded here** (8 functions, frozen at compile time): `ast_meta`,
//!   `ast_blast`, `index_find`, `wiring_orphans`, `memory_recall`, `pre_edit`,
//!   `tantivy_search`, `parallel`. The strategy-doc names them; the rationale
//!   is "calling code must not break when the journal-derived surface shifts".
//! - **Types derived from the journal** at CI time via `scripts/gen_sdk.py`:
//!   per-language + per-failure-kind aggregates land in
//!   `sdk_signal_report.json` (next to this module) and inform
//!   `SignalReport::load()` at runtime.
//!
//! ## Why hybrid
//!
//! A fully hardcoded surface forgets what the harness actually saw (the
//! "anti-pattern of hardcoded surface" cited in strategy v1.1 §D3). A fully
//! journal-derived surface breaks call-sites the moment a hook falls out of
//! use. The hybrid keeps the names honest and the call-sites stable.
//!
//! ## Fail-soft
//!
//! Every public function returns `Result<SdkValue, SdkError>`. A failing hook
//! does NOT panic — it surfaces the error to the calling program, which can
//! `try`/`except` in Python or `?` in Rust and continue. This matches the
//! E/A/M directives (`rules/code-elaboration-directives.md` A5: errors teach
//! the correction).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::journal::{self};

/// Canonical hook identifier. The set is frozen by the strategy-doc — adding
/// a variant requires a strategy-doc amendment (Decision D3 in v1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookName {
    /// `touring ast meta <file>` — file-metadata-first.
    AstMeta,
    /// `touring ast blast <file>` — full dependency tree.
    AstBlast,
    /// `touring index find <symbol>` — exact VGP lookup.
    IndexFind,
    /// `touring wiring orphans` — pub symbols without consumers (REGRA #0).
    WiringOrphans,
    /// `touring memory recall "<topic>"` — semantic + faceted recall.
    MemoryRecall,
    /// `touring pre-edit` — composite score gate (≥ 0.8 before Edit).
    PreEdit,
    /// `touring tantivy search "<query>"` — BM25 ranked hit list.
    TantivySearch,
    /// `touring parallel <calls>` — read-only fan-out (capped at 10).
    Parallel,
}

impl HookName {
    /// All variants in the canonical order. Used by the parser + report
    /// emitter so two runs produce byte-identical reports.
    pub const ALL: &'static [HookName] = &[
        Self::AstMeta,
        Self::AstBlast,
        Self::IndexFind,
        Self::WiringOrphans,
        Self::MemoryRecall,
        Self::PreEdit,
        Self::TantivySearch,
        Self::Parallel,
    ];

    /// Canonical snake_case name as it appears in the in-sandbox prelude
    /// and in the `SignalReport` JSON keys.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AstMeta => "ast_meta",
            Self::AstBlast => "ast_blast",
            Self::IndexFind => "index_find",
            Self::WiringOrphans => "wiring_orphans",
            Self::MemoryRecall => "memory_recall",
            Self::PreEdit => "pre_edit",
            Self::TantivySearch => "tantivy_search",
            Self::Parallel => "parallel",
        }
    }
}

/// Returned by every SDK hook. The shape is `serde_json::Value` so the
/// in-sandbox Python/Rust program parses it uniformly — no per-hook type
/// dance. The strategy-doc calls this "JSON canônico TIPADO".
pub type SdkValue = serde_json::Value;

/// Per-hook execution stats derived from the journal. Emitted by
/// `scripts/gen_sdk.py` and consumed by `SignalReport::load`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookSignal {
    /// How many times this hook was invoked across the journal window.
    pub call_count: u64,
    /// How many of those invocations ended with a non-null `failure_kind`.
    pub failure_count: u64,
    /// Median hook-call duration (ms).
    pub duration_ms_p50: u64,
    /// 99th-percentile hook-call duration (ms).
    pub duration_ms_p99: u64,
}

/// Aggregate signal report — one per `touring run` execution envelope. The
/// schema matches `scripts/gen_sdk.py::emit_report` so the JSON is round-
/// trippable across processes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SignalReport {
    /// Unix timestamp (seconds) the report was emitted.
    pub generated_at_unix: u64,
    /// Absolute path of the journal this report was built from.
    pub source_journal_path: String,
    /// Number of valid journal entries that contributed to this report.
    pub total_runs: u64,
    /// Per-hook stats keyed by canonical name (`ast_meta`, etc).
    pub hooks: BTreeMap<String, HookSignal>,
    /// Per-language stats (count + failure_count).
    pub by_language: BTreeMap<String, JournalLanguageStats>,
    /// Failure-kind histogram across all runs.
    pub failure_taxonomy: BTreeMap<String, u64>,
}

/// Per-language breakdown carried by `SignalReport::by_language`. Stripped
/// version of `journal::LanguageStats` — drops duration percentiles that
/// the report consumer does not need.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JournalLanguageStats {
    /// Total runs in this language.
    pub count: u64,
    /// Runs in this language that had a non-null `failure_kind`.
    pub failure_count: u64,
}

/// Errors that can surface from any public function in `sdk.rs`.
#[derive(Debug, Error)]
pub enum SdkError {
    /// `path` does not exist on disk.
    #[error("journal not found: {0}")]
    JournalMissing(String),
    /// Journal exists but every line failed to parse; `line` is the first offender.
    #[error("journal malformed at line {line}: {source}")]
    JournalMalformed {
        /// Zero-indexed line number of the first malformed JSON line.
        line: usize,
        /// The serde JSON parse error itself (chained via `#[source]`).
        #[source]
        source: serde_json::Error,
    },
    /// Underlying I/O error reading the file.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Caller asked for a SignalReport that doesn't exist on disk yet.
    #[error("signal report missing: {0}")]
    SignalReportMissing(String),
    /// Hook name string did not match any canonical `HookName` variant.
    #[error("unknown hook name: {0}")]
    UnknownHook(String),
}

/// Build a fresh `SignalReport` from a journal path. This is what
/// `gen_sdk.py` would emit, but it's also the runtime bootstrap — a CLI
/// that wants the current signal state without going through Python just
/// calls `signal_report_from_journal(path)`.
///
/// Returns the aggregate alongside the total run count so the caller can
/// decide whether to write the report to disk or just use it.
pub fn signal_report_from_journal(path: &Path) -> Result<SignalReport, SdkError> {
    let agg = journal::read_journal(path).map_err(|e| match e {
        journal::JournalError::NotFound(p) => SdkError::JournalMissing(p),
        journal::JournalError::Malformed { line, source } => {
            SdkError::JournalMalformed { line, source }
        }
        journal::JournalError::Io(e) => SdkError::Io(e),
    })?;

    let mut report = SignalReport {
        generated_at_unix: now_unix(),
        source_journal_path: path.display().to_string(),
        total_runs: agg.total_entries,
        hooks: BTreeMap::new(),
        by_language: BTreeMap::new(),
        failure_taxonomy: BTreeMap::new(),
    };

    for (lang, stats) in &agg.by_language {
        report.by_language.insert(
            lang.clone(),
            JournalLanguageStats {
                count: stats.count,
                failure_count: stats.failure_count,
            },
        );
    }
    for (kind, count) in &agg.failure_counts {
        report.failure_taxonomy.insert(kind.clone(), *count);
    }

    // Per-hook stats require the journal to carry a hook dimension. The
    // current journal schema does not — hooks are inferred from the
    // program body, not recorded at execution time. Until F3 ships the
    // PostToolUse-sync, every hook has zero observed calls. We still
    // materialize all 8 so consumers see the full surface.
    for hook in HookName::ALL {
        report.hooks.insert(
            hook.as_str().to_string(),
            HookSignal {
                call_count: 0,
                failure_count: 0,
                duration_ms_p50: 0,
                duration_ms_p99: 0,
            },
        );
    }

    Ok(report)
}

/// Load a previously-emitted `SignalReport` from disk. Used by SDK consumers
/// that want a stable view across runs (vs. the live journal view).
pub fn load_signal_report(path: &Path) -> Result<SignalReport, SdkError> {
    if !path.exists() {
        return Err(SdkError::SignalReportMissing(path.display().to_string()));
    }
    let bytes = std::fs::read(path)?;
    let report: SignalReport = serde_json::from_slice(&bytes)
        .map_err(|e| SdkError::SignalReportMissing(format!("{}: {e}", path.display())))?;
    Ok(report)
}

/// Resolve a hook name string (as it appears in the program preamble or the
/// report) into the canonical enum.
pub fn parse_hook(s: &str) -> Result<HookName, SdkError> {
    for h in HookName::ALL {
        if h.as_str() == s {
            return Ok(*h);
        }
    }
    Err(SdkError::UnknownHook(s.to_string()))
}

/// Record one hook invocation: appends to the mirror JSONL sink and is
/// available for downstream `signal_report_from_journal_and_mirror` reads.
///
/// This is the PostToolUse-side hook the strategy-doc D4 names. The Rust
/// caller (e.g. the daemon's RPC handler) invokes this with the canonical
/// hook + measured duration + success flag, and the mirror carries the
/// signal forward into the next report aggregation.
///
/// Returns the path the line was appended to. The parent dir is created
/// lazily if absent (`~/.claude/touring/` exists by convention; we mkdir
/// the rest of the way).
///
/// LatencyMarker note: we DO NOT touch `/tmp/touring-hooks/latency/` here —
/// that path lives in a different crate (`touring-hooks-shared`) and the
/// marker file format is incompatible with JSONL. The mirror is the
/// canonical PostToolUse-side sink for `SignalReport::load`.
pub fn record_hook_call(
    mirror_path: &Path,
    hook: HookName,
    duration_ms: u32,
    success: bool,
    origin: crate::sdk_signal_mirror::MirrorOrigin,
) -> Result<PathBuf, crate::sdk_signal_mirror::MirrorError> {
    crate::sdk_signal_mirror::record(mirror_path, hook, duration_ms, success, origin)
}

/// F4 P3 (2026-09-01) — classify a PostToolUse bash command line into the
/// canonical hook it invokes, if any.
///
/// Deliberately precision-first: `touring` must be the command's FIRST
/// program token (leading `VAR=value` assignments are skipped, a path prefix
/// like `.touring/bin/touring` is accepted) and only the seven CLI-reachable
/// hooks match (`parallel` is SDK-only). Anything else — other subcommands,
/// `touring` as an argument, compound prefixes like `cd x && touring …` —
/// returns `None`, because a false positive would inflate `signal_use`.
#[must_use]
pub fn classify_bash_command(command: &str) -> Option<HookName> {
    let mut toks = command.split_whitespace();
    let mut program = toks.next()?;
    // Skip leading VAR=VALUE env assignments (the per-command relax idiom).
    while program.contains('=') && !program.starts_with('/') && !program.starts_with('.') {
        program = toks.next()?;
    }
    let base = program.rsplit('/').next().unwrap_or(program);
    if base != "touring" {
        return None;
    }
    let first = toks.next()?;
    let second = toks.next();
    match (first, second) {
        ("ast", Some("meta")) => Some(HookName::AstMeta),
        ("ast", Some("blast")) => Some(HookName::AstBlast),
        ("index", Some("find")) => Some(HookName::IndexFind),
        ("wiring", Some("orphans")) => Some(HookName::WiringOrphans),
        ("memory", Some("recall")) => Some(HookName::MemoryRecall),
        ("pre-edit", _) => Some(HookName::PreEdit),
        ("tantivy", Some("search")) => Some(HookName::TantivySearch),
        _ => None,
    }
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

    #[test]
    fn all_hooks_have_strings() {
        for h in HookName::ALL {
            assert!(!h.as_str().is_empty());
        }
    }

    #[test]
    fn parse_hook_roundtrip() {
        for h in HookName::ALL {
            assert_eq!(parse_hook(h.as_str()).expect("known hook"), *h);
        }
    }

    #[test]
    fn parse_hook_unknown_is_error() {
        let err = parse_hook("nonexistent").expect_err("must fail");
        assert!(matches!(err, SdkError::UnknownHook(_)));
    }

    // ── F4 P3 (2026-09-01) — PostToolUse feeder: bash command → hook ─────

    #[test]
    fn classify_maps_the_seven_cli_shapes() {
        let cases = [
            ("touring ast meta src/lib.rs --depth summary -j", HookName::AstMeta),
            ("touring ast blast crates/x/src/lib.rs", HookName::AstBlast),
            ("touring index find MySymbol -j", HookName::IndexFind),
            ("touring wiring orphans -j", HookName::WiringOrphans),
            ("touring memory recall \"topic\"", HookName::MemoryRecall),
            ("touring pre-edit", HookName::PreEdit),
            ("touring tantivy search \"query\"", HookName::TantivySearch),
        ];
        for (cmd, want) in cases {
            assert_eq!(classify_bash_command(cmd), Some(want), "cmd: {cmd}");
        }
    }

    #[test]
    fn classify_sees_through_env_prefix_and_binary_path() {
        assert_eq!(
            classify_bash_command("TOURING_CODE_MODE=native touring ast meta f.rs"),
            Some(HookName::AstMeta)
        );
        assert_eq!(
            classify_bash_command("/home/u/.touring/bin/touring index find Sym"),
            Some(HookName::IndexFind)
        );
    }

    #[test]
    fn classify_rejects_non_touring_and_other_subcommands() {
        for cmd in [
            "grep -rn touring",
            "touring doctor -j",
            "touring status -j",
            "touring run --lang python --code 'x'",
            "echo touring ast meta",
            "",
        ] {
            assert_eq!(classify_bash_command(cmd), None, "cmd: {cmd}");
        }
    }

    #[test]
    fn signal_report_emits_all_hooks_even_with_zero_calls() {
        // Empty journal: signal report still has all 8 hooks materialized.
        let tmp = tempfile_journal_in(&[]);
        let report = signal_report_from_journal(&tmp).expect("must build report");
        assert_eq!(report.hooks.len(), HookName::ALL.len());
        std::fs::remove_file(&tmp).ok();
    }

    fn tempfile_journal_in(entries: &[(&str, &str)]) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "sdk-test-{}-{}.jsonl",
            std::process::id(),
            now_unix()
        ));
        let mut body = String::new();
        for (lang, dur) in entries {
            body.push_str(&format!(
                r#"{{"ts":1,"language":"{lang}","exit_code":0,"failure_kind":null,"duration_ms":{dur},"code_hash_stdout_bytes":0,"bytes_elided":0}}"#
            ));
            body.push('\n');
        }
        std::fs::write(&p, body).expect("tempfile write");
        p
    }
}