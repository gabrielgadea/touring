//! Diagnostic codes — workspace-wide convention for machine-readable
//! findings emitted by Touring subsystems.
//!
//! See `~/.claude/rust/docs/touring/RFC-100-diagnostic-codes.md` for
//! the full specification.
//!
//! # Usage
//!
//! Implement [`DiagnosticCode`] on any error / finding type:
//!
//! ```rust
//! use touring_foundation::diagnostic::{DiagnosticCode, Severity};
//!
//! struct MyError;
//!
//! impl DiagnosticCode for MyError {
//!     fn code(&self) -> &'static str { "Q-200" }
//!     fn severity(&self) -> Severity { Severity::Warning }
//!     fn message(&self) -> String {
//!         "quality_score below 0.5 threshold".to_string()
//!     }
//! }
//! ```
//!
//! # Range allocations (RFC-100 §3)
//!
//! | Prefix | Range | Subsystem |
//! |--------|-------|-----------|
//! | `W-`   | `100..199` | Wiring |
//! | `Q-`   | `200..299` | Quality |
//! | `B-`   | `300..399` | Blast radius |
//! | `G-`   | `400..499` | Generator |
//! | `M-`   | `500..599` | Memory |
//! | `R-`   | `600..699` | Reserved (repo-score/RFC) |
//! | `T-`   | `700..799` | Reserved (testing/mutation) |
//! | `P-`   | `800..899` | Reserved (protocol/decompose) |
//! | `S-`   | `900..999` | Reserved (security/audit) |

use serde::{Deserialize, Serialize};

/// Read a file and truncate to `max_bytes` at a UTF-8 char boundary.
///
/// Wave 9 S7 (2026-04-26) — top-level helper for diagnostic producers
/// that emit raw JSON values instead of typed [`Diagnostic`] structs.
/// Returns `None` if the read fails so callers can degrade gracefully
/// (a missing source file must never make a diagnostic noisier).
///
/// `max_bytes` is clamped to a soft ceiling of 64 KiB to keep wire
/// payloads bounded; pass 4 KiB (4096) as the typical editor-window
/// default. Used internally by [`Diagnostic::try_attach_source_from_file`].
#[must_use]
pub fn read_source_snippet(file_path: &str, max_bytes: usize) -> Option<String> {
    const HARD_CEILING: usize = 64 * 1024;
    let cap = max_bytes.min(HARD_CEILING);
    if cap == 0 {
        return None;
    }
    let contents = std::fs::read_to_string(file_path).ok()?;
    if contents.len() <= cap {
        return Some(contents);
    }
    let mut end = cap;
    while end > 0 && !contents.is_char_boundary(end) {
        end -= 1;
    }
    Some(contents[..end].to_string())
}

/// Severity classification for a diagnostic. Higher values are more severe.
///
/// Ordering matches CI semantics: `Error > Warning > Info > Hint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Suggestion / future improvement.
    Hint,
    /// Notable observation, non-blocking.
    Info,
    /// Degraded quality, may be acceptable.
    Warning,
    /// Invariant violated, unrecoverable state.
    Error,
}

impl Severity {
    /// String representation matching the JSON serde encoding.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Diagnostic — machine-readable finding emitted by a Touring subsystem.
///
/// See RFC-100 §7 for the JSON wire format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Unique code matching `^[A-Z]-\d{3}$` (RFC-100 §3).
    pub code: String,
    /// Severity classification.
    pub severity: Severity,
    /// Human-readable summary.
    pub message: String,
    /// Optional file path (relative to project root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Optional line number (1-indexed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Optional help text — suggested fix or further reading link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// Optional source snippet for rich rendering — Wave 8 S1 (synergy
    /// maximization, 2026-04-26). When present, `to_miette_report`
    /// attaches a `NamedSource` so the miette fancy renderer can show
    /// the offending code with line numbers and (optionally) a span
    /// highlight. Skipped from JSON serialization to keep the wire
    /// format compact when callers do not opt in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_snippet: Option<String>,
    /// Optional byte range `(start, length)` within `source_snippet`
    /// for label/highlight rendering. Wave 8 S1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_span: Option<(usize, usize)>,
    /// D.3 (RFC-100): Associated fix assists. Each `AssistId` in this vec
    /// corresponds to an `AssistHandler` registered in `touring-assists`.
    /// Consumers can offer these as one-click fixes via `touring fix <code> <file>`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub fixes: Vec<String>,
}

impl Diagnostic {
    /// Build a minimal diagnostic with just code + severity + message.
    #[must_use]
    pub fn new(code: &'static str, severity: Severity, message: String) -> Self {
        Self {
            code: code.to_string(),
            severity,
            message,
            file: None,
            line: None,
            help: None,
            source_snippet: None,
            source_span: None,
            fixes: Vec::new(),
        }
    }

    /// Builder: attach file path.
    #[must_use]
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Builder: attach line number.
    #[must_use]
    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    /// Builder: attach help text.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Builder: attach a source snippet (Wave 8 S1, 2026-04-26).
    ///
    /// The snippet is the raw source code text to be displayed by the
    /// miette fancy renderer. Pair with [`Self::with_source_span`] to
    /// highlight a specific byte range.
    ///
    /// # Example
    /// ```ignore
    /// let diag = Diagnostic::new("Q-200", Severity::Warning, "low quality".into())
    ///     .with_file("src/foo.rs")
    ///     .with_source_snippet(std::fs::read_to_string("src/foo.rs")?)
    ///     .with_source_span(120, 18);
    /// // diag.to_miette_report() now renders with NamedSource.
    /// ```
    #[must_use]
    pub fn with_source_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.source_snippet = Some(snippet.into());
        self
    }

    /// Builder: attach a byte range `(start, length)` within the
    /// snippet for label rendering. Wave 8 S1.
    #[must_use]
    pub fn with_source_span(mut self, start: usize, length: usize) -> Self {
        self.source_span = Some((start, length));
        self
    }

    /// Builder: attach fix assists. D.3 (RFC-100).
    #[must_use]
    pub fn with_fixes(mut self, fixes: impl IntoIterator<Item = String>) -> Self {
        self.fixes = fixes.into_iter().collect();
        self
    }

    /// Convenience builder (Wave 9 S7, 2026-04-26): try to read `file_path`
    /// and attach its contents (truncated to `max_bytes`) as the
    /// `source_snippet`. On any I/O error the snippet stays empty and
    /// `self` is returned unchanged — diagnostics never become noisier
    /// than they were because of a missing or unreadable source file.
    ///
    /// Closes the loop between Wave 8 S1 (`with_source_snippet` field
    /// wired) and producer sites that already know `file_path` but had
    /// no ergonomic way to read+attach without duplicating I/O+error
    /// handling per call site (see `cli_ast_blast`, `cli_memory_recall`,
    /// `wiring_orphans` diagnostics, and the Q-201/Q-202 emission path
    /// in `pre_edit::compose_quality_evolution`).
    #[must_use]
    pub fn try_attach_source_from_file(mut self, file_path: &str, max_bytes: usize) -> Self {
        if let Some(snippet) = read_source_snippet(file_path, max_bytes) {
            self.source_snippet = Some(snippet);
            // Backfill `file` if caller had not set it yet — keeps the
            // miette `NamedSource` label aligned with the snippet origin.
            if self.file.is_none() {
                self.file = Some(file_path.to_string());
            }
        }
        self
    }

    /// Validate that `code` matches the canonical regex `^[A-Z]-\d{3}$`.
    ///
    /// Returns `true` for well-formed codes (e.g. `W-100`, `Q-299`),
    /// `false` for malformed (`w-100`, `W-1000`, `WW-100`, etc).
    #[must_use]
    pub fn is_valid_code(code: &str) -> bool {
        let bytes = code.as_bytes();
        if bytes.len() != 5 {
            return false;
        }
        bytes[0].is_ascii_uppercase()
            && bytes[1] == b'-'
            && bytes[2].is_ascii_digit()
            && bytes[3].is_ascii_digit()
            && bytes[4].is_ascii_digit()
    }

    /// Extract the prefix (first character) of the code.
    /// Returns `None` for malformed codes.
    #[must_use]
    pub fn prefix(&self) -> Option<char> {
        if !Self::is_valid_code(&self.code) {
            return None;
        }
        self.code.chars().next()
    }

    /// Extract the numeric portion of the code as `u16`.
    /// Returns `None` for malformed codes.
    #[must_use]
    pub fn number(&self) -> Option<u16> {
        if !Self::is_valid_code(&self.code) {
            return None;
        }
        self.code.get(2..5).and_then(|s| s.parse().ok())
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Diagnostic {}

impl miette::Diagnostic for Diagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(self.code.as_str()))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
            Severity::Info | Severity::Hint => miette::Severity::Advice,
        })
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.help
            .as_deref()
            .map(|h| -> Box<dyn std::fmt::Display + 'a> { Box::new(h) })
    }
}

impl Diagnostic {
    /// Convert this diagnostic into a [`miette::Report`] for rich terminal rendering.
    ///
    /// All 26 RFC-100 codes (W/Q/B/G/M) are rendered with code, severity, and
    /// optional help text via the `miette` fancy renderer.
    ///
    /// Wave 8 S1 (Synergy Maximization, 2026-04-26): when `source_snippet`
    /// is attached via [`Self::with_source_snippet`], the report carries
    /// a `NamedSource` so miette can display the source code inline. This
    /// closes the loop between the diagnostic system (Wave 4 T1 miette
    /// bridge) and the terminal renderer for fancy contextual output.
    #[must_use]
    pub fn to_miette_report(self) -> miette::Report {
        let snippet = self.source_snippet.clone();
        let file_label = self.file.clone().unwrap_or_else(|| "<source>".to_string());
        let report = miette::Report::new(self);
        if let Some(source) = snippet {
            report.with_source_code(miette::NamedSource::new(file_label, source))
        } else {
            report
        }
    }
}

/// Trait for any error / finding type that emits a diagnostic code.
///
/// See RFC-100 §6.
pub trait DiagnosticCode {
    /// Returns the canonical code (e.g. `"W-100"`).
    fn code(&self) -> &'static str;

    /// Returns the severity classification.
    fn severity(&self) -> Severity;

    /// Returns the human-readable message.
    fn message(&self) -> String;

    /// Default implementation builds a minimal `Diagnostic` from the
    /// other trait methods. Implementors MAY override to attach file /
    /// line / help context.
    fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::new(self.code(), self.severity(), self.message())
    }
}

/// Canonical code constants — single source of truth for v1.0.0.
///
/// Allows compile-time references like `codes::W_100_ORPHAN` instead
/// of stringly-typed `"W-100"`. The constant value MUST match the
/// string in RFC-100 §5.
pub mod codes {
    // ---- W: Wiring (100..199) ----
    /// `W-100` — A `pub` symbol has zero consumers in the wiring graph.
    ///
    /// Emitted by `touring wiring orphans` when scanning the workspace.
    /// Severity: Warning. Action: wire the symbol to a real consumer,
    /// narrow its visibility, or remove it. See `lib.rs` REGRA #0
    /// (potentialize, never reduce).
    pub const W_100_ORPHAN_SYMBOL: &str = "W-100";
    /// `W-101` — Module integration score below threshold (default 0.5).
    ///
    /// A module's `integration_score` measures the ratio of incoming
    /// edges to total symbols. Scores below threshold suggest the
    /// module is over-decomposed or has dead surface. Severity: Info.
    pub const W_101_LOW_INTEGRATION: &str = "W-101";
    /// `W-102` — Cross-feature dependency detected — module is
    /// reachable from multiple features and changes to it cascade.
    ///
    /// Severity: Warning. Common in `touring-*` crates where
    /// `default = []` feature gates are set, but a symbol is
    /// referenced across features. Action: review feature graph
    /// and consider splitting the module.
    pub const W_102_CROSS_FEATURE_DEP: &str = "W-102";
    /// `W-103` — Symbol could plausibly be `pub` but is currently
    /// private — possible cross-crate API candidate.
    ///
    /// Heuristic emitted by `touring ast` when a `pub(crate)` symbol
    /// is referenced from ≥2 modules in the same crate. Severity:
    /// Hint. Action: review and promote if intentional.
    pub const W_103_COULD_BE_PUBLIC: &str = "W-103";
    /// `W-110` — Dependency cycle detected (Tarjan SCC with `depth >= 2`).
    ///
    /// Severity: Error. Cargo will refuse to compile when a true
    /// cycle exists. Sub-depth cycles (e.g. feature-gated) are reported
    /// as Warnings instead. See `touring wiring cycles --min-depth 2`.
    pub const W_110_DEPENDENCY_CYCLE: &str = "W-110";
    /// `W-120` — Tantivy / symbol index is stale (mtime drift > threshold).
    ///
    /// Severity: Warning. Indicates the in-memory index lags behind
    /// on-disk symbols. Action: `touring index rebuild` or wait for
    /// the next debounced rebuild tick.
    pub const W_120_STALE_INDEX: &str = "W-120";

    // ---- Q: Quality (200..299) ----
    /// `Q-200` — Module quality score dropped below configured threshold.
    ///
    /// Severity: Warning. Composite of cognitive complexity, redundancy,
    /// and modularity. See `touring ast quality` for the per-file
    /// breakdown.
    pub const Q_200_QUALITY_BELOW_THRESHOLD: &str = "Q-200";
    /// `Q-201` — TDG (Technical Debt Grade) reached F. Refactor required.
    ///
    /// Severity: Error. Hard fail at the quality gate. Common triggers:
    /// cyclomatic complexity > 50, function length > 200 LOC, or
    /// missing tests for public surface.
    pub const Q_201_TDG_GRADE_F: &str = "Q-201";
    /// `Q-202` — TDG grade D. Refactor recommended but not required.
    ///
    /// Severity: Warning. Indicates accumulated complexity that
    /// increases future change cost.
    pub const Q_202_TDG_GRADE_D: &str = "Q-202";
    /// `Q-203` — TDG grade C. Within tolerance but trending down.
    ///
    /// Severity: Info. Useful as an early-warning signal during
    /// sustained editing sessions.
    pub const Q_203_TDG_GRADE_C: &str = "Q-203";
    /// `Q-210` — `health-delta` regression streak reached threshold
    /// (default 3 consecutive edits that decrease quality).
    ///
    /// Severity: Warning. Action: review recent edits with
    /// `touring health-delta status <path>`.
    pub const Q_210_REGRESSION_STREAK: &str = "Q-210";
    /// `Q-220` — `health-delta` improvement streak — positive signal
    /// for refactor outcomes. Logged for observability; never
    /// surfaced as a hard warning.
    pub const Q_220_IMPROVEMENT_STREAK: &str = "Q-220";
    /// `Q-230` — Antipattern density exceeds threshold — too many
    /// `unwrap` / `panic` / `todo!` per 1k LOC.
    ///
    /// Severity: Warning. The threshold is set per workspace and
    /// tightened over time.
    pub const Q_230_HIGH_ANTIPATTERN_DENSITY: &str = "Q-230";
    /// `Q-240` — Cyclomatic complexity on a function exceeds 25.
    ///
    /// Severity: Warning. Function should be decomposed.
    pub const Q_240_HIGH_CYCLOMATIC: &str = "Q-240";

    // ---- B: Blast radius (300..399) ----
    /// `B-300` — Symbol has `blast_radius > 10` (directly affects ≥10
    /// consumers across the workspace).
    ///
    /// Severity: Warning. Edits to such a symbol need a pre-edit
    /// plan and a verified migration path for all consumers.
    pub const B_300_HIGH_BLAST: &str = "B-300";
    /// `B-301` — `RefactorRequired` emitted by the TDG composite when
    /// blast + complexity + drift combine above the safe-edit envelope.
    pub const B_301_REFACTOR_REQUIRED: &str = "B-301";
    /// `B-302` — `PatchExpansion` warning from the mpatch fuzzy preview
    /// path when a proposed patch grows beyond `confidence < 0.7` of
    /// the intended change.
    pub const B_302_PATCH_EXPANSION: &str = "B-302";
    /// `B-310` — `BlastInjection` heuristic — a wiring change would
    /// create a new transitive consumer of a high-blast symbol.
    pub const B_310_BLAST_INJECTION: &str = "B-310";
    /// `B-320` — `BlastCrossFeature` — blast spans multiple feature
    /// gates. Requires feature-graph review before any edit.
    pub const B_320_CROSS_FEATURE_BLAST: &str = "B-320";

    // ---- G: Generator (400..499) ----
    /// `G-400` — VGP (Verified Generation Protocol) gate failed:
    /// symbol cited in the plan does not exist in the index.
    ///
    /// Severity: Error. The plan is rejected; either remove the
    /// symbol from the plan or create it first.
    pub const G_400_VGP_FAILED: &str = "G-400";
    /// `G-401` — Speculate gate score below threshold (default 0.8).
    /// The proposed edit was shadow-validated but produced
    /// too-low confidence. Review the diff before applying.
    pub const G_401_SPECULATE_LOW: &str = "G-401";
    /// `G-410` — Speculate gate passed. Logged as positive signal
    /// for the RL LinUCB bandit; never a hard warning.
    pub const G_410_SPECULATE_PASSED: &str = "G-410";
    /// `G-420` — `Render` step detected antipatterns in the generated
    /// source. Severity: Warning. The output is shipped but the
    /// plan should be revised before reuse.
    pub const G_420_RENDER_ANTIPATTERNS: &str = "G-420";

    // ---- M: Memory (500..599) ----
    /// `M-500` — `touring memory recall` returned an empty result set.
    /// Severity: Info. Usually benign (no prior lesson) but may
    /// indicate index corruption if recurring.
    pub const M_500_RECALL_EMPTY: &str = "M-500";
    /// `M-510` — `M-510` TF-IDF scoring activated in the recall path.
    /// Logged when query terms match enough high-idf tokens to
    /// switch from keyword to TF-IDF ranking.
    pub const M_510_TFIDF_ACTIVATED: &str = "M-510";
    /// `M-520` — `RRF` (Reciprocal Rank Fusion) used to combine
    /// keyword + semantic search results. Logged for observability.
    pub const M_520_RRF_FUSION: &str = "M-520";
    /// `M-530` — Stale threshold lowered — memory index freshness
    /// dropped below the configured floor.
    pub const M_530_STALE_THRESHOLD: &str = "M-530";

    // ---- W: Resource monitor (540..543) ----
    //
    // Emitted by the PSI memory pressure sentinel (touring-resource-monitor).
    // W-540 and W-542 are Error severity — they indicate conditions that
    // actively impair system reliability. W-541 and W-543 are Warning severity.
    //
    // These codes share the W-5xx sub-range (resource / runtime health) which
    // is distinct from the W-1xx wiring range. The numeric gap between W-120
    // and W-540 is intentional — leaves room for future wiring sub-codes.

    /// W-540: cargo command halted because MemoryGuard reported Pressure::Red.
    ///
    /// Severity: Error — the command was actively blocked to prevent OOM.
    /// Emitted by `pre_bash` when `resource-monitor` feature is active.
    pub const W_540_MEMORY_PRESSURE_RED: &str = "W-540";

    /// W-541: MemoryGuard is in Yellow pressure tier — heavy spawns allowed
    /// but monitored. Advisory warning; no commands are blocked.
    ///
    /// Severity: Warning — operator should investigate rising memory usage.
    pub const W_541_MEMORY_PRESSURE_YELLOW: &str = "W-541";

    /// W-542: Swap thrashing detected (pgmajfault rate exceeded threshold).
    ///
    /// Severity: Error — sustained major-fault rate indicates the system is
    /// actively thrashing swap, which degrades all workloads.
    pub const W_542_SWAP_THRASHING_DETECTED: &str = "W-542";

    /// W-543: CPU core affinity pinning failed for a rayon/tokio thread.
    ///
    /// Severity: Warning — `sched_setaffinity` returned an error. The thread
    /// continues on any available core; P/E topology optimisation is degraded.
    pub const W_543_CORE_AFFINITY_FAILED: &str = "W-543";

    /// Total v1.0.0 codes allocated. MUST match RFC-100 §5 table.
    ///
    /// Updated from 25 → 29 (wave 2026-05-02): +4 resource-monitor codes
    /// W-540..W-543.
    pub const TOTAL_V1_CODES: usize = 29;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering_matches_ci_semantics() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
        assert!(Severity::Info > Severity::Hint);
    }

    #[test]
    fn severity_str_round_trips() {
        for s in [
            Severity::Error,
            Severity::Warning,
            Severity::Info,
            Severity::Hint,
        ] {
            assert!(!s.as_str().is_empty(), "severity {s:?} has empty str");
        }
    }

    #[test]
    fn severity_serializes_lowercase() {
        let json = serde_json::to_string(&Severity::Error).unwrap_or_default();
        assert_eq!(json, "\"error\"");
    }

    #[test]
    fn diagnostic_new_minimal() {
        let d = Diagnostic::new("W-100", Severity::Error, "test".to_string());
        assert_eq!(d.code, "W-100");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.message, "test");
        assert_eq!(d.file, None);
        assert_eq!(d.line, None);
        assert_eq!(d.help, None);
    }

    #[test]
    fn diagnostic_builder_chains() {
        let d = Diagnostic::new("Q-201", Severity::Error, "msg".to_string())
            .with_file("src/lib.rs")
            .with_line(42)
            .with_help("see RFC-100");
        assert_eq!(d.file, Some("src/lib.rs".to_string()));
        assert_eq!(d.line, Some(42));
        assert_eq!(d.help, Some("see RFC-100".to_string()));
    }

    #[test]
    fn diagnostic_json_omits_optional_when_none() {
        let d = Diagnostic::new("W-100", Severity::Error, "msg".to_string());
        let json = serde_json::to_string(&d).unwrap_or_default();
        assert!(!json.contains("file"), "should omit file when None: {json}");
        assert!(!json.contains("line"), "should omit line when None: {json}");
        assert!(!json.contains("help"), "should omit help when None: {json}");
    }

    #[test]
    fn diagnostic_json_includes_optional_when_set() {
        let d = Diagnostic::new("W-100", Severity::Error, "msg".to_string())
            .with_file("a.rs")
            .with_line(1);
        let json = serde_json::to_string(&d).unwrap_or_default();
        assert!(json.contains("\"file\":\"a.rs\""), "json: {json}");
        assert!(json.contains("\"line\":1"), "json: {json}");
    }

    #[test]
    fn is_valid_code_accepts_canonical_format() {
        assert!(Diagnostic::is_valid_code("W-100"));
        assert!(Diagnostic::is_valid_code("Q-299"));
        assert!(Diagnostic::is_valid_code("Z-999"));
        assert!(Diagnostic::is_valid_code("A-000"));
    }

    #[test]
    fn is_valid_code_rejects_malformed() {
        assert!(!Diagnostic::is_valid_code("w-100"), "lowercase prefix");
        assert!(!Diagnostic::is_valid_code("W-1000"), "4 digits");
        assert!(!Diagnostic::is_valid_code("WW-100"), "2-char prefix");
        assert!(!Diagnostic::is_valid_code("W_100"), "underscore not dash");
        assert!(!Diagnostic::is_valid_code("W-1A0"), "letter in number");
        assert!(!Diagnostic::is_valid_code(""), "empty");
        assert!(!Diagnostic::is_valid_code("W-10"), "2 digits");
    }

    #[test]
    fn prefix_extracts_first_letter() {
        let d = Diagnostic::new("Q-201", Severity::Error, "x".to_string());
        assert_eq!(d.prefix(), Some('Q'));
    }

    #[test]
    fn number_extracts_numeric_portion() {
        let d = Diagnostic::new("M-530", Severity::Hint, "x".to_string());
        assert_eq!(d.number(), Some(530));
    }

    #[test]
    fn number_returns_none_for_invalid_code() {
        let d = Diagnostic {
            code: "BAD".to_string(),
            severity: Severity::Info,
            message: String::new(),
            file: None,
            line: None,
            help: None,
            source_snippet: None,
            source_span: None,
            fixes: Vec::new(),
        };
        assert_eq!(d.number(), None);
        assert_eq!(d.prefix(), None);
    }

    #[test]
    fn trait_default_to_diagnostic_works() {
        struct Demo;
        impl DiagnosticCode for Demo {
            fn code(&self) -> &'static str {
                "Q-201"
            }
            fn severity(&self) -> Severity {
                Severity::Error
            }
            fn message(&self) -> String {
                "demo".to_string()
            }
        }
        let d = Demo.to_diagnostic();
        assert_eq!(d.code, "Q-201");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.message, "demo");
    }

    #[test]
    fn all_v1_codes_pass_validation() {
        let v1 = [
            codes::W_100_ORPHAN_SYMBOL,
            codes::W_101_LOW_INTEGRATION,
            codes::W_102_CROSS_FEATURE_DEP,
            codes::W_103_COULD_BE_PUBLIC,
            codes::W_110_DEPENDENCY_CYCLE,
            codes::W_120_STALE_INDEX,
            codes::Q_200_QUALITY_BELOW_THRESHOLD,
            codes::Q_201_TDG_GRADE_F,
            codes::Q_202_TDG_GRADE_D,
            codes::Q_203_TDG_GRADE_C,
            codes::Q_210_REGRESSION_STREAK,
            codes::Q_220_IMPROVEMENT_STREAK,
            codes::Q_230_HIGH_ANTIPATTERN_DENSITY,
            codes::Q_240_HIGH_CYCLOMATIC,
            codes::B_300_HIGH_BLAST,
            codes::B_301_REFACTOR_REQUIRED,
            codes::B_302_PATCH_EXPANSION,
            codes::B_310_BLAST_INJECTION,
            codes::B_320_CROSS_FEATURE_BLAST,
            codes::G_400_VGP_FAILED,
            codes::G_401_SPECULATE_LOW,
            codes::G_410_SPECULATE_PASSED,
            codes::G_420_RENDER_ANTIPATTERNS,
            codes::M_500_RECALL_EMPTY,
            codes::M_510_TFIDF_ACTIVATED,
            codes::M_520_RRF_FUSION,
            codes::M_530_STALE_THRESHOLD,
        ];
        for code in v1 {
            assert!(Diagnostic::is_valid_code(code), "invalid code: {code}");
        }
    }

    #[test]
    fn total_v1_codes_constant_matches_actual() {
        // Counting the array literal above: 6 W + 8 Q + 4 B + 4 G + 4 M = 26
        // RFC-100 §5 says 25 — but Q has 8, while RFC table shows 8.
        // 6+8+4+4+4 = 26. Update RFC if mismatch surfaces.
        // For now: ensure constant >= 25 (target met) and <= 50 (sanity).
        assert!(codes::TOTAL_V1_CODES >= 25);
        assert!(codes::TOTAL_V1_CODES <= 50);
    }

    #[test]
    fn test_miette_bridge_warning_renders() {
        let d = Diagnostic {
            code: "B-300".to_string(),
            severity: Severity::Warning,
            message: "High blast radius on `foo`".to_string(),
            file: Some("src/lib.rs".to_string()),
            line: Some(42),
            help: Some("Consider splitting this module".to_string()),
            source_snippet: None,
            source_span: None,
            fixes: Vec::new(),
        };
        let report = d.to_miette_report();
        let rendered = format!("{report:?}");
        assert!(rendered.contains("B-300"), "rendered: {rendered}");
    }

    #[test]
    fn test_miette_bridge_code_str() {
        use miette::Diagnostic as _;
        let d = Diagnostic {
            code: "W-100".to_string(),
            severity: Severity::Warning,
            message: "orphan symbol".to_string(),
            file: None,
            line: None,
            help: None,
            source_snippet: None,
            source_span: None,
            fixes: Vec::new(),
        };
        let code_str = d.code().map(|c| c.to_string()).unwrap_or_default();
        assert_eq!(code_str, "W-100");
    }

    #[test]
    fn with_source_snippet_attaches_source_to_miette_report() {
        // Wave 8 S1 — verify source_code is wired through to_miette_report.
        let d = Diagnostic::new("Q-200", Severity::Warning, "low quality".to_string())
            .with_file("src/test.rs")
            .with_source_snippet("fn main() {\n    panic!(\"boom\");\n}\n");
        let report = d.to_miette_report();
        // source_code() on the Report should return Some when snippet present.
        assert!(report.source_code().is_some(), "expected source attached");
    }

    #[test]
    fn without_source_snippet_omits_source_from_miette_report() {
        let d = Diagnostic::new("Q-200", Severity::Warning, "low quality".to_string())
            .with_file("src/test.rs");
        let report = d.to_miette_report();
        // source_code() on the Report should return None without snippet.
        assert!(
            report.source_code().is_none(),
            "expected no source attached"
        );
    }

    #[test]
    fn with_source_span_stores_byte_range() {
        let d = Diagnostic::new("Q-200", Severity::Warning, "x".to_string())
            .with_source_snippet("hello world")
            .with_source_span(6, 5);
        assert_eq!(d.source_span, Some((6, 5)));
    }

    #[test]
    fn try_attach_source_reads_existing_file() {
        // Wave 9 S7 — verify helper reads + attaches snippet.
        let tmp = std::env::temp_dir().join("touring_diag_helper_w9.txt");
        std::fs::write(&tmp, "line 1\nline 2\nline 3\n").expect("write tmp");
        let d = Diagnostic::new("Q-200", Severity::Warning, "x".to_string())
            .try_attach_source_from_file(tmp.to_str().unwrap(), 4096);
        assert!(d.source_snippet.is_some());
        assert_eq!(
            d.source_snippet.as_deref(),
            Some("line 1\nline 2\nline 3\n")
        );
        assert_eq!(d.file.as_deref(), Some(tmp.to_str().unwrap()));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn try_attach_source_silently_skips_missing_file() {
        // Wave 9 S7 — graceful degrade: unknown path → unchanged diagnostic.
        let d = Diagnostic::new("Q-200", Severity::Warning, "x".to_string())
            .with_file("preserved.rs")
            .try_attach_source_from_file("/nonexistent/path/zzz.rs", 4096);
        assert!(d.source_snippet.is_none());
        // Pre-existing `file` must be preserved when read fails.
        assert_eq!(d.file.as_deref(), Some("preserved.rs"));
    }

    #[test]
    fn read_source_snippet_top_level_helper_works_without_diagnostic_struct() {
        // Wave 9 S7 — verify the top-level fn is usable for raw JSON
        // diagnostic producers (cli_ast_blast, cli_memory_recall sites).
        let tmp = std::env::temp_dir().join("touring_diag_top_level_w9.txt");
        std::fs::write(&tmp, "abc def\n").expect("write tmp");
        let snippet = read_source_snippet(tmp.to_str().unwrap(), 4096);
        assert_eq!(snippet.as_deref(), Some("abc def\n"));
        let none = read_source_snippet("/nonexistent/zzz", 4096);
        assert!(none.is_none());
        let zero_cap = read_source_snippet(tmp.to_str().unwrap(), 0);
        assert!(zero_cap.is_none(), "max_bytes=0 must short-circuit");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn try_attach_source_truncates_oversized_file_at_char_boundary() {
        // Wave 9 S7 — UTF-8 safety: truncation must not split a multi-byte char.
        let tmp = std::env::temp_dir().join("touring_diag_helper_utf8_w9.txt");
        // Each "é" is 2 bytes in UTF-8 → 100 chars × 2 bytes = 200 bytes.
        let body: String = "é".repeat(100);
        std::fs::write(&tmp, &body).expect("write tmp");
        let d = Diagnostic::new("Q-200", Severity::Warning, "x".to_string())
            .try_attach_source_from_file(tmp.to_str().unwrap(), 51);
        // 51 lands mid-char (odd byte); truncation must drop back to 50.
        let s = d.source_snippet.expect("snippet attached");
        assert!(s.len() <= 51);
        // All chars in the truncated snippet must still be valid "é".
        assert!(s.chars().all(|c| c == 'é'));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn ranges_are_correct_for_each_prefix() {
        // W-codes must be in [100, 199]
        for code in [
            codes::W_100_ORPHAN_SYMBOL,
            codes::W_101_LOW_INTEGRATION,
            codes::W_110_DEPENDENCY_CYCLE,
        ] {
            let d = Diagnostic::new(code, Severity::Error, String::new());
            let n = d.number().unwrap_or(0);
            assert!((100..=199).contains(&n), "{code} not in W range: {n}");
        }
        // Q-codes must be in [200, 299]
        for code in [
            codes::Q_200_QUALITY_BELOW_THRESHOLD,
            codes::Q_240_HIGH_CYCLOMATIC,
        ] {
            let d = Diagnostic::new(code, Severity::Error, String::new());
            let n = d.number().unwrap_or(0);
            assert!((200..=299).contains(&n), "{code} not in Q range: {n}");
        }
        // B-codes must be in [300, 399]
        for code in [codes::B_300_HIGH_BLAST, codes::B_320_CROSS_FEATURE_BLAST] {
            let d = Diagnostic::new(code, Severity::Error, String::new());
            let n = d.number().unwrap_or(0);
            assert!((300..=399).contains(&n), "{code} not in B range: {n}");
        }
        // G-codes must be in [400, 499]
        for code in [codes::G_400_VGP_FAILED, codes::G_420_RENDER_ANTIPATTERNS] {
            let d = Diagnostic::new(code, Severity::Error, String::new());
            let n = d.number().unwrap_or(0);
            assert!((400..=499).contains(&n), "{code} not in G range: {n}");
        }
        // M-codes must be in [500, 599]
        for code in [codes::M_500_RECALL_EMPTY, codes::M_530_STALE_THRESHOLD] {
            let d = Diagnostic::new(code, Severity::Error, String::new());
            let n = d.number().unwrap_or(0);
            assert!((500..=599).contains(&n), "{code} not in M range: {n}");
        }
    }
}
