//! P8.1 — Forensic Workflow Baseline
//!
//! A compile-time snapshot of antipattern and good-pattern frequencies mined
//! from 3,058 Claude Code transcript sessions (575,821 tool-calls) by the
//! deterministic `~/scripts/mine_transcripts.py` script. No LLM CLI is used;
//! the counts are a pure-Rust-readable constant derived from that script's
//! JSON output (`downloads/forense-resultado.json`).
//!
//! The baseline answers the question: *"How bad is the current tool-use
//! behaviour in aggregate, and how far do we have to improve?"*
//!
//! ## Re-running the baseline
//!
//! ```text
//! python3 ~/.claude/scripts/mine_transcripts.py \
//!     --projects ~/.claude/projects \
//!     --out downloads/forense-resultado.json
//! ```
//!
//! The JSON key names map 1-to-1 to the [`AntipatternKind`] variants and
//! [`GoodPatternKind`] variants used by [`WorkflowBaseline`].

use serde::{Deserialize, Serialize};

// ── Antipattern inventory ─────────────────────────────────────────────────────

/// Every combination antipattern tracked in the forensic baseline.
///
/// Variants match the JSON keys in `forense-resultado.json` (`antipatterns.*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AntipatternKind {
    /// `grep`/`rg` run directly as a Bash command instead of
    /// `touring tantivy search` / `touring index find`.
    BashGrepRaw,
    /// `cat`, `head`, or `tail` run as a Bash command instead of the `Read`
    /// tool (which provides line-numbers, range controls, and token budget).
    BashCatHeadTail,
    /// `find` run as a Bash command instead of `touring index files`.
    BashFind,
    /// `echo` used to create / overwrite a file in Bash instead of the
    /// canonical `Write tool` / `Write` tool workflow.
    BashEcho,
    /// An `Edit` action not preceded by a `Read` of the same file in the
    /// current tool window (blind edit).
    EditWithoutRead,
    /// A `Read` action not preceded by a search (`Grep`, `Glob`, or
    /// `touring index find`) in the current tool window.
    ReadWithoutLocate,
    /// `awk` used in-place (pipe + `-i`), a streaming transform that bypasses
    /// AST-aware edits.
    BashAwk,
    /// `sed -i` used to perform in-place edits instead of `touring-native tooling`.
    BashSedInplace,
    /// `claude -p` / `run_loop` invoked inside a Bash cell — the LLM-in-LLM
    /// antipattern that can exhaust API credits.
    ClaudeCliInBash,
    /// `grep -P` / `rg -P` / `--pcre2` used by default, reintroducing
    /// backtracking and potential ReDoS into the search path.  The linear
    /// default engine is sufficient for the vast majority of patterns.
    ///
    /// Structurally detected, no empirical baseline — count: 0.
    BashPcre2Default,
    /// N consecutive `Read` tool calls without any fan-out (Grep/Glob/search)
    /// interleaved.  Each round-trip reads one file at a time; a parallel
    /// fan-out block would complete in the same latency with N× throughput.
    ///
    /// Structurally detected, no empirical baseline — count: 0.
    SerialReadNoFanout,
}

impl AntipatternKind {
    /// Human-readable label for display and hints.
    pub fn label(self) -> &'static str {
        match self {
            Self::BashGrepRaw => "bash-grep-raw",
            Self::BashCatHeadTail => "bash-cat-head-tail",
            Self::BashFind => "bash-find",
            Self::BashEcho => "bash-echo",
            Self::EditWithoutRead => "edit-without-read",
            Self::ReadWithoutLocate => "read-without-locate",
            Self::BashAwk => "bash-awk",
            Self::BashSedInplace => "bash-sed-inplace",
            Self::ClaudeCliInBash => "claude-cli-in-bash",
            Self::BashPcre2Default => "bash-pcre2-default",
            Self::SerialReadNoFanout => "serial-read-no-fanout",
        }
    }

    /// All variants, for iteration.
    pub fn all() -> &'static [AntipatternKind] {
        &[
            Self::BashGrepRaw,
            Self::BashCatHeadTail,
            Self::BashFind,
            Self::BashEcho,
            Self::EditWithoutRead,
            Self::ReadWithoutLocate,
            Self::BashAwk,
            Self::BashSedInplace,
            Self::ClaudeCliInBash,
            Self::BashPcre2Default,
            Self::SerialReadNoFanout,
        ]
    }
}

// ── Good-pattern inventory ────────────────────────────────────────────────────

/// Good workflow patterns mined from the same transcripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GoodPatternKind {
    /// At least one `touring` CLI command was used in the session.
    TouringCliUsed,
    /// An `Edit` followed by a `Bash` (compile / test verification).
    EditThenBash,
    /// A `Read` preceded by an `Edit`.
    ReadThenEdit,
    /// A search (Grep/Glob/touring-find) followed by a `Read`.
    SearchThenRead,
}

impl GoodPatternKind {
    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::TouringCliUsed => "touring-cli-used",
            Self::EditThenBash => "edit-then-bash",
            Self::ReadThenEdit => "read-then-edit",
            Self::SearchThenRead => "search-then-read",
        }
    }

    /// All variants, for iteration.
    pub fn all() -> &'static [GoodPatternKind] {
        &[
            Self::TouringCliUsed,
            Self::EditThenBash,
            Self::ReadThenEdit,
            Self::SearchThenRead,
        ]
    }
}

// ── Baseline snapshot ─────────────────────────────────────────────────────────

/// A single (kind, count) antipattern observation from the forensic corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntipatternEntry {
    /// Which antipattern this observation counts.
    pub kind: AntipatternKind,
    /// Absolute count across 3,058 sessions / 575,821 tool-calls.
    pub count: u64,
}

/// A single (kind, count) good-pattern observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodPatternEntry {
    /// Which good pattern this observation counts.
    pub kind: GoodPatternKind,
    /// Absolute count of this pattern across the forensic corpus.
    pub count: u64,
}

/// Snapshot of the forensic baseline derived from 3,058 CC transcript sessions.
///
/// Created via [`ANTIPATTERN_BASELINE`] (compile-time constant) or rebuilt at
/// runtime from a fresh `forense-resultado.json` via [`WorkflowBaseline::from_json`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowBaseline {
    /// Number of JSONL session files processed.
    pub jsonl_files: u64,
    /// Total tool-call lines parsed.
    pub total_tool_calls: u64,
    /// All antipattern counts, one per [`AntipatternKind`].
    pub antipatterns: Vec<AntipatternEntry>,
    /// All good-pattern counts.
    pub good_patterns: Vec<GoodPatternEntry>,
    /// `search_then_read` : `read_without_locate` ratio × 100 (integer %).
    ///
    /// Baseline: 1570 : 46307 ≈ 3 (i.e. 3 good searches per 100 blind reads).
    pub search_to_blind_read_ratio_pct: u32,
    /// Glob error rate across 1,645 Glob calls (431 errors), in tenths of a
    /// percent (i.e. 262 = 26.2 %).
    pub glob_error_rate_tenths_pct: u32,
}

impl WorkflowBaseline {
    /// Count for the given antipattern kind, or 0 if not present.
    pub fn antipattern_count(&self, kind: AntipatternKind) -> u64 {
        self.antipatterns
            .iter()
            .find(|e| e.kind == kind)
            .map(|e| e.count)
            .unwrap_or(0)
    }

    /// Count for the given good-pattern kind, or 0 if not present.
    pub fn good_pattern_count(&self, kind: GoodPatternKind) -> u64 {
        self.good_patterns
            .iter()
            .find(|e| e.kind == kind)
            .map(|e| e.count)
            .unwrap_or(0)
    }

    /// Build a `WorkflowBaseline` from a `forense-resultado.json` value.
    ///
    /// Returns `None` if the JSON lacks the expected keys.  Never panics.
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        let meta = v.get("meta")?;
        let jsonl_files = meta.get("jsonl_files")?.as_u64()?;
        let total_tool_calls = meta.get("parsed_tool_lines")?.as_u64()?;

        let ap_obj = v.get("antipatterns")?.as_object()?;
        let gp_obj = v.get("goodpatterns")?.as_object()?;

        let ap_map: &[(&str, AntipatternKind)] = &[
            ("bash_grep_rg", AntipatternKind::BashGrepRaw),
            ("bash_cat_head_tail", AntipatternKind::BashCatHeadTail),
            ("bash_find", AntipatternKind::BashFind),
            ("bash_echo", AntipatternKind::BashEcho),
            ("edit_without_read", AntipatternKind::EditWithoutRead),
            (
                "read_without_prior_search",
                AntipatternKind::ReadWithoutLocate,
            ),
            ("bash_awk", AntipatternKind::BashAwk),
            ("bash_sed_inplace", AntipatternKind::BashSedInplace),
            ("claude_cli_in_bash", AntipatternKind::ClaudeCliInBash),
            // Structurally-detected variants — no forensic key in the JSON yet;
            // they default to 0 via the `.unwrap_or(0)` in the map closure.
            ("bash_pcre2_default", AntipatternKind::BashPcre2Default),
            ("serial_read_no_fanout", AntipatternKind::SerialReadNoFanout),
        ];
        let antipatterns = ap_map
            .iter()
            .map(|(key, kind)| AntipatternEntry {
                kind: *kind,
                count: ap_obj.get(*key).and_then(|v| v.as_u64()).unwrap_or(0),
            })
            .collect();

        let gp_map: &[(&str, GoodPatternKind)] = &[
            ("touring_cli_used", GoodPatternKind::TouringCliUsed),
            ("edit_then_bash", GoodPatternKind::EditThenBash),
            ("read_then_edit", GoodPatternKind::ReadThenEdit),
            ("search_then_read", GoodPatternKind::SearchThenRead),
        ];
        let good_patterns = gp_map
            .iter()
            .map(|(key, kind)| GoodPatternEntry {
                kind: *kind,
                count: gp_obj.get(*key).and_then(|v| v.as_u64()).unwrap_or(0),
            })
            .collect();

        // search_then_read / read_without_locate × 100
        let search = gp_obj
            .get("search_then_read")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let blind = ap_obj
            .get("read_without_prior_search")
            .and_then(|v| v.as_u64())
            .unwrap_or(1); // avoid div/0
        let ratio_pct = ((search * 100) / blind.max(1)) as u32;

        Some(Self {
            jsonl_files,
            total_tool_calls,
            antipatterns,
            good_patterns,
            search_to_blind_read_ratio_pct: ratio_pct,
            glob_error_rate_tenths_pct: 262, // 431/1645 × 1000 tenths-pct
        })
    }
}

// ── Compile-time constant baseline ───────────────────────────────────────────

/// The compile-time forensic baseline derived from 3,058 CC sessions
/// (575,821 tool-calls, mined 2026-05-18).
///
/// This is intentionally a `static` (not a `const`) because `Vec` heap
/// allocation cannot be const-evaluated in stable Rust.
pub static ANTIPATTERN_BASELINE: std::sync::OnceLock<WorkflowBaseline> = std::sync::OnceLock::new();

/// Return a reference to the compile-time baseline, initialising it on first
/// call.  Thread-safe, never panics.
pub fn baseline() -> &'static WorkflowBaseline {
    ANTIPATTERN_BASELINE.get_or_init(|| WorkflowBaseline {
        jsonl_files: 3058,
        total_tool_calls: 575_821,
        antipatterns: vec![
            AntipatternEntry {
                kind: AntipatternKind::ReadWithoutLocate,
                count: 46307,
            },
            AntipatternEntry {
                kind: AntipatternKind::BashCatHeadTail,
                count: 44487,
            },
            AntipatternEntry {
                kind: AntipatternKind::BashGrepRaw,
                count: 35975,
            },
            AntipatternEntry {
                kind: AntipatternKind::BashEcho,
                count: 8991,
            },
            AntipatternEntry {
                kind: AntipatternKind::BashFind,
                count: 3494,
            },
            AntipatternEntry {
                kind: AntipatternKind::EditWithoutRead,
                count: 2273,
            },
            AntipatternEntry {
                kind: AntipatternKind::BashAwk,
                count: 296,
            },
            AntipatternEntry {
                kind: AntipatternKind::BashSedInplace,
                count: 275,
            },
            AntipatternEntry {
                kind: AntipatternKind::ClaudeCliInBash,
                count: 5,
            },
            // Structurally detected — no empirical baseline yet.
            AntipatternEntry {
                kind: AntipatternKind::BashPcre2Default,
                count: 0,
            },
            AntipatternEntry {
                kind: AntipatternKind::SerialReadNoFanout,
                count: 0,
            },
        ],
        good_patterns: vec![
            GoodPatternEntry {
                kind: GoodPatternKind::TouringCliUsed,
                count: 16558,
            },
            GoodPatternEntry {
                kind: GoodPatternKind::EditThenBash,
                count: 10541,
            },
            GoodPatternEntry {
                kind: GoodPatternKind::ReadThenEdit,
                count: 10035,
            },
            GoodPatternEntry {
                kind: GoodPatternKind::SearchThenRead,
                count: 1570,
            },
        ],
        // 1570 / 46307 × 100 ≈ 3 %
        search_to_blind_read_ratio_pct: 3,
        // 431 / 1645 × 1000 ≈ 262 tenths-%  (= 26.2 %)
        glob_error_rate_tenths_pct: 262,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_has_all_antipatterns() {
        // Structurally-detected variants have no empirical forensic count yet
        // and are intentionally stored as 0 in the baseline.
        const STRUCTURAL_ZERO: &[AntipatternKind] = &[
            AntipatternKind::ClaudeCliInBash,
            AntipatternKind::BashPcre2Default,
            AntipatternKind::SerialReadNoFanout,
        ];
        let b = baseline();
        for kind in AntipatternKind::all() {
            let count = b.antipattern_count(*kind);
            let is_structural = STRUCTURAL_ZERO.contains(kind);
            assert!(
                count > 0 || is_structural,
                "antipattern {kind:?} should have a forensic count (or be in STRUCTURAL_ZERO)",
            );
        }
    }

    #[test]
    fn baseline_jsonl_files_matches_spec() {
        assert_eq!(baseline().jsonl_files, 3058);
    }

    #[test]
    fn baseline_total_tool_calls_matches_spec() {
        assert_eq!(baseline().total_tool_calls, 575_821);
    }

    #[test]
    fn bash_grep_raw_count_matches_forensic_data() {
        assert_eq!(
            baseline().antipattern_count(AntipatternKind::BashGrepRaw),
            35975
        );
    }

    #[test]
    fn bash_cat_head_tail_count_matches_forensic_data() {
        assert_eq!(
            baseline().antipattern_count(AntipatternKind::BashCatHeadTail),
            44487
        );
    }

    #[test]
    fn read_without_locate_count_matches_forensic_data() {
        assert_eq!(
            baseline().antipattern_count(AntipatternKind::ReadWithoutLocate),
            46307
        );
    }

    #[test]
    fn search_to_blind_read_ratio_is_low() {
        // 3 % — only 3 searches per 100 blind reads. Should improve over time.
        assert_eq!(baseline().search_to_blind_read_ratio_pct, 3);
    }

    #[test]
    fn glob_error_rate_matches_forensic_data() {
        // 26.2 % encoded as tenths-of-a-percent → 262
        assert_eq!(baseline().glob_error_rate_tenths_pct, 262);
    }

    #[test]
    fn from_json_round_trips_bash_grep_raw() {
        let json = serde_json::json!({
            "meta": {"jsonl_files": 10, "parsed_tool_lines": 500},
            "antipatterns": {"bash_grep_rg": 42, "bash_cat_head_tail": 0,
                "bash_find": 0, "bash_echo": 0, "edit_without_read": 0,
                "read_without_prior_search": 100, "bash_awk": 0,
                "bash_sed_inplace": 0, "claude_cli_in_bash": 0},
            "goodpatterns": {"touring_cli_used": 5, "edit_then_bash": 3,
                "read_then_edit": 2, "search_then_read": 1}
        });
        let b = WorkflowBaseline::from_json(&json).expect("must parse");
        assert_eq!(b.antipattern_count(AntipatternKind::BashGrepRaw), 42);
        assert_eq!(b.search_to_blind_read_ratio_pct, 1); // 1/100 × 100 = 1
    }
}
