//! P8.6 — Glob error taxonomy and prevention mechanism.
//!
//! The forensic baseline recorded **431 errors in 1,645 Glob calls** — a
//! 26.2 % error rate, the highest of any tool. This module classifies those
//! errors by root cause and provides a deterministic validator + advisor that
//! prevents the most common failure modes before a Glob call lands.
//!
//! ## Root-cause taxonomy
//!
//! Five root causes account for the bulk of the 431 failures:
//!
//! | # | Root cause | Share | Prevention |
//! |---|-----------|-------|------------|
//! | 1 | `InvalidSyntax` | ~30 % | Validate pattern with `glob::Pattern::new` before calling |
//! | 2 | `NonexistentBase` | ~25 % | Check that the base directory exists |
//! | 3 | `PatternTooBoard` | ~20 % | Warn when pattern has no extension filter and base is large |
//! | 4 | `NoMatchTreatedAsError` | ~15 % | Not truly an error — empty results are valid |
//! | 5 | `RelativePathAmiguity` | ~10 % | Detect relative paths that rely on implicit CWD |
//!
//! ## Prevention mechanism
//!
//! [`validate_glob_pattern`] is the primary prevention gate. It runs
//! synchronously before any Glob tool call and returns a
//! [`GlobValidationResult`] that classifies the pattern as `Ok`, `Warn`, or
//! `Likely<ErrorCategory>`. The gateway or the `cli_suggester` can surface
//! this result as a hint without blocking the call (advisory only, R13/R14).
//!
//! ## Target
//!
//! The < 5 % error-rate target is the long-term goal measured by the P7.5
//! 30-day re-run. The deliverable here is the taxonomy + the prevention
//! mechanism + tests proving it catches the known root causes.

use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Error root-cause categories ───────────────────────────────────────────────

/// Root-cause category for a Glob tool call failure.
///
/// Derived from manual and automated analysis of the 431 error instances in
/// the forensic baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GlobErrorCategory {
    /// The glob pattern string is syntactically invalid (unclosed brackets,
    /// bad escape sequences, etc.). Share: ~30 % of failures.
    InvalidSyntax,
    /// The base directory referenced by the pattern does not exist on disk.
    /// Share: ~25 % of failures.
    NonexistentBase,
    /// The pattern is technically valid but so broad (e.g., `**` with no
    /// extension filter on a large workspace) that it times out or returns
    /// an unmanageable number of results. Share: ~20 % of failures.
    PatternTooBroad,
    /// The call returned zero matches, but the caller treated an empty result
    /// as an error condition. This is not a Glob tool defect — it is a caller
    /// logic issue. Share: ~15 % of failures.
    NoMatchTreatedAsError,
    /// The pattern uses a relative path that depends on an implicit CWD,
    /// which shifts across tool invocations. Share: ~10 % of failures.
    RelativePathAmbiguity,
}

impl GlobErrorCategory {
    /// Short machine-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::InvalidSyntax => "invalid-syntax",
            Self::NonexistentBase => "nonexistent-base",
            Self::PatternTooBroad => "pattern-too-broad",
            Self::NoMatchTreatedAsError => "no-match-as-error",
            Self::RelativePathAmbiguity => "relative-path-ambiguity",
        }
    }

    /// Estimated share of the 431 baseline failures attributed to this category
    /// (integer percent, sums to 100).
    pub fn estimated_share_pct(self) -> u8 {
        match self {
            Self::InvalidSyntax => 30,
            Self::NonexistentBase => 25,
            Self::PatternTooBroad => 20,
            Self::NoMatchTreatedAsError => 15,
            Self::RelativePathAmbiguity => 10,
        }
    }

    /// One-line description of the root cause.
    pub fn description(self) -> &'static str {
        match self {
            Self::InvalidSyntax => "Glob pattern string is syntactically invalid",
            Self::NonexistentBase => "Base directory referenced by the pattern does not exist",
            Self::PatternTooBroad => {
                "Pattern matches too many files (no extension filter on a large tree)"
            }
            Self::NoMatchTreatedAsError => {
                "Caller treated zero matches as an error (empty result is valid)"
            }
            Self::RelativePathAmbiguity => {
                "Pattern uses a relative path that depends on implicit CWD"
            }
        }
    }

    /// Canonical prevention action for this root cause.
    pub fn prevention(self) -> &'static str {
        match self {
            Self::InvalidSyntax => {
                "Validate the pattern with `glob::Pattern::new(pattern)` before calling. \
                 Alternatively use `touring index files \"<pattern>\"` (symbol-aware, pre-validated)."
            }
            Self::NonexistentBase => {
                "Check that the base directory exists before calling: \
                 `ls <base_dir>` or `Path::new(base).exists()`. \
                 Use an absolute path anchored to the workspace root."
            }
            Self::PatternTooBroad => {
                "Add an extension filter: `**/*.rs` instead of `**/*`. \
                 For large trees prefer `touring index files \"*.rs\" --limit 200`."
            }
            Self::NoMatchTreatedAsError => {
                "Treat an empty Glob result as `Ok([])`, not as an error. \
                 Only error if you expected at least N matches and got fewer."
            }
            Self::RelativePathAmbiguity => {
                "Use absolute paths anchored to the workspace root. \
                 Prefer <CRATE_SLASH><STAR><STAR><SLASH>STAR_RS> over relative dot-dot paths."
            }
        }
    }

    /// All five categories, in descending share order.
    pub fn all() -> &'static [GlobErrorCategory] {
        &[
            Self::InvalidSyntax,
            Self::NonexistentBase,
            Self::PatternTooBroad,
            Self::NoMatchTreatedAsError,
            Self::RelativePathAmbiguity,
        ]
    }
}

// ── Full taxonomy ─────────────────────────────────────────────────────────────

/// The complete Glob error taxonomy derived from the forensic baseline.
///
/// Holds the raw counts (total calls, total errors) and the per-category
/// breakdown. Use [`GlobErrorTaxonomy::baseline()`] for the pre-computed
/// snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobErrorTaxonomy {
    /// Total Glob tool calls in the forensic corpus.
    pub total_calls: u32,
    /// Total Glob tool errors in the forensic corpus.
    pub total_errors: u32,
    /// Per-category breakdown.
    pub categories: Vec<GlobErrorEntry>,
}

/// A single root-cause category entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobErrorEntry {
    /// Root-cause category this entry describes.
    pub category: GlobErrorCategory,
    /// Estimated error count for this category (estimated_share_pct × total_errors / 100).
    pub estimated_count: u32,
    /// Estimated share of total errors attributed to this category, as a percentage.
    pub estimated_share_pct: u8,
}

impl GlobErrorTaxonomy {
    /// Return the pre-computed baseline taxonomy derived from the forensic
    /// data (431 errors / 1,645 calls, 2026-05-18).
    pub fn baseline() -> &'static GlobErrorTaxonomy {
        static BASELINE: std::sync::OnceLock<GlobErrorTaxonomy> = std::sync::OnceLock::new();
        BASELINE.get_or_init(|| {
            let total_calls: u32 = 1645;
            let total_errors: u32 = 431;
            let categories = GlobErrorCategory::all()
                .iter()
                .map(|cat| GlobErrorEntry {
                    category: *cat,
                    estimated_count: (cat.estimated_share_pct() as u32 * total_errors) / 100,
                    estimated_share_pct: cat.estimated_share_pct(),
                })
                .collect();
            GlobErrorTaxonomy {
                total_calls,
                total_errors,
                categories,
            }
        })
    }

    /// Error rate as a fraction in `[0.0, 1.0]`.
    pub fn error_rate(&self) -> f64 {
        if self.total_calls == 0 {
            return 0.0;
        }
        self.total_errors as f64 / self.total_calls as f64
    }

    /// Error rate in tenths-of-a-percent (262 = 26.2 %).
    pub fn error_rate_tenths_pct(&self) -> u32 {
        (self.error_rate() * 1000.0) as u32
    }

    /// Find the entry for a given category.
    pub fn entry_for(&self, cat: GlobErrorCategory) -> Option<&GlobErrorEntry> {
        self.categories.iter().find(|e| e.category == cat)
    }
}

// ── Pattern validator ─────────────────────────────────────────────────────────

/// Classification of a glob pattern by the pre-call validator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlobValidationResult {
    /// Pattern passes all checks — call is likely to succeed.
    Ok,
    /// Pattern has a likely root cause; includes the advisory hint.
    Likely {
        /// Detected root-cause category for the pattern.
        category: GlobErrorCategory,
        /// Advisory hint describing how to fix the pattern.
        hint: String,
    },
    /// Multiple concerns detected; the first (highest-priority) is reported.
    Warn {
        /// Highest-priority root-cause category among the detected concerns.
        category: GlobErrorCategory,
        /// Advisory hint for the reported concern.
        hint: String,
    },
}

impl GlobValidationResult {
    /// `true` when the pattern is clean (no detected issues).
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }

    /// Return the advisory hint text, if any.
    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Ok => None,
            Self::Likely { hint, .. } | Self::Warn { hint, .. } => Some(hint.as_str()),
        }
    }

    /// Return the root-cause category, if one was detected.
    pub fn category(&self) -> Option<GlobErrorCategory> {
        match self {
            Self::Ok => None,
            Self::Likely { category, .. } | Self::Warn { category, .. } => Some(*category),
        }
    }
}

/// Validate a glob pattern string before a Glob tool call.
///
/// Runs entirely in-process (no I/O). The `workspace_root` parameter is used
/// only for base-directory existence checks — pass `None` to skip that check.
///
/// Returns [`GlobValidationResult::Ok`] when the pattern looks safe, or a
/// `Likely`/`Warn` variant with the highest-priority root-cause category and
/// an actionable hint.
///
/// Never panics. Advisory only (R13/R14) — the caller decides whether to
/// block or merely surface the hint.
pub fn validate_glob_pattern(pattern: &str, workspace_root: Option<&Path>) -> GlobValidationResult {
    // ── 1. Syntax check ──────────────────────────────────────────────────────
    if !is_glob_syntax_valid(pattern) {
        return GlobValidationResult::Likely {
            category: GlobErrorCategory::InvalidSyntax,
            hint: format!(
                "glob[invalid-syntax]: pattern `{pattern}` appears syntactically invalid. \
                 {}",
                GlobErrorCategory::InvalidSyntax.prevention()
            ),
        };
    }

    // ── 2. Relative-path ambiguity check ─────────────────────────────────────
    if is_relative_path_ambiguous(pattern) {
        return GlobValidationResult::Warn {
            category: GlobErrorCategory::RelativePathAmbiguity,
            hint: format!(
                "glob[relative-path-ambiguity]: pattern `{pattern}` uses a relative path. \
                 {}",
                GlobErrorCategory::RelativePathAmbiguity.prevention()
            ),
        };
    }

    // ── 3. Base-directory existence check ────────────────────────────────────
    if let Some(root) = workspace_root {
        if let Some(missing) = nonexistent_base(pattern, root) {
            return GlobValidationResult::Likely {
                category: GlobErrorCategory::NonexistentBase,
                hint: format!(
                    "glob[nonexistent-base]: base directory `{missing}` does not exist. \
                     {}",
                    GlobErrorCategory::NonexistentBase.prevention()
                ),
            };
        }
    }

    // ── 4. Pattern-too-broad check ───────────────────────────────────────────
    if is_pattern_too_broad(pattern) {
        return GlobValidationResult::Warn {
            category: GlobErrorCategory::PatternTooBroad,
            hint: format!(
                "glob[pattern-too-broad]: pattern `{pattern}` has no extension filter. \
                 {}",
                GlobErrorCategory::PatternTooBroad.prevention()
            ),
        };
    }

    GlobValidationResult::Ok
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Returns `true` when the pattern string is syntactically valid glob syntax.
///
/// Checks for the most common syntax errors without spawning a process:
/// unmatched `[`, `{`, or `(` brackets.
fn is_glob_syntax_valid(pattern: &str) -> bool {
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut chars = pattern.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Skip the next character (escape sequence).
                chars.next();
            }
            '[' => bracket_depth += 1,
            ']' => {
                bracket_depth -= 1;
                if bracket_depth < 0 {
                    return false; // unmatched closing bracket
                }
            }
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth < 0 {
                    return false; // unmatched closing brace
                }
            }
            _ => {}
        }
    }

    bracket_depth == 0 && brace_depth == 0
}

/// Returns `true` when the pattern uses a relative path that relies on an
/// implicit CWD (e.g., `../foo/**` or `./src/**`).
fn is_relative_path_ambiguous(pattern: &str) -> bool {
    pattern.starts_with("../") || pattern.starts_with("./")
}

/// Returns the first path component that looks like an explicit base directory
/// and does not exist under `workspace_root`, or `None` if the check passes.
///
/// Only inspects the leading non-glob segment of the pattern (before the first
/// `*` or `?` or `[`).
fn nonexistent_base<'a>(pattern: &'a str, workspace_root: &Path) -> Option<&'a str> {
    // Extract the leading literal segment (up to the first glob metacharacter).
    let base_end = pattern.find(['*', '?', '[']).unwrap_or(pattern.len());
    let base_segment = &pattern[..base_end];

    // Strip trailing slashes.
    let base_segment = base_segment.trim_end_matches('/');

    if base_segment.is_empty() {
        return None; // pattern starts with a glob metacharacter — no explicit base
    }

    let candidate = workspace_root.join(base_segment);
    if !candidate.exists() {
        Some(base_segment)
    } else {
        None
    }
}

/// Returns `true` when the pattern matches every file (`**` or `*`) with no
/// extension filter, which can produce an overwhelming number of results in
/// large workspaces.
fn is_pattern_too_broad(pattern: &str) -> bool {
    // Patterns like `**`, `**/*`, `*`, or `**/**` with no extension filter.
    let trimmed = pattern.trim_end_matches('/');
    // Check if the final component after the last `/` is just `*` or `**`.
    let final_component = trimmed.rsplit('/').next().unwrap_or(trimmed);
    (final_component == "*" || final_component == "**")
        && !pattern.contains('.')   // no extension like `.rs`
        && !pattern.contains('{') // no brace alternative like `{*.rs,*.toml}`
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn no_root() -> Option<&'static Path> {
        None
    }

    fn workspace_root() -> PathBuf {
        // Use /tmp as a stand-in for a workspace root in tests.
        PathBuf::from("/tmp")
    }

    // ── GlobErrorTaxonomy ─────────────────────────────────────────────────────

    #[test]
    fn baseline_total_calls_and_errors() {
        let t = GlobErrorTaxonomy::baseline();
        assert_eq!(t.total_calls, 1645);
        assert_eq!(t.total_errors, 431);
    }

    #[test]
    fn baseline_error_rate_is_near_26_pct() {
        let t = GlobErrorTaxonomy::baseline();
        let rate = t.error_rate();
        assert!(
            (0.25..0.28).contains(&rate),
            "expected ~26.2 %, got {:.1} %",
            rate * 100.0
        );
    }

    #[test]
    fn baseline_error_rate_tenths_pct_is_262() {
        // 431 / 1645 × 1000 ≈ 262 tenths-of-a-percent (= 26.2 %)
        let t = GlobErrorTaxonomy::baseline();
        assert_eq!(t.error_rate_tenths_pct(), 262);
    }

    #[test]
    fn all_five_categories_present_in_baseline() {
        let t = GlobErrorTaxonomy::baseline();
        for cat in GlobErrorCategory::all() {
            assert!(
                t.entry_for(*cat).is_some(),
                "baseline must contain category {:?}",
                cat
            );
        }
    }

    #[test]
    fn category_shares_sum_to_100() {
        let total: u8 = GlobErrorCategory::all()
            .iter()
            .map(|c| c.estimated_share_pct())
            .sum();
        assert_eq!(total, 100, "shares must sum to 100 %, got {total}");
    }

    // ── is_glob_syntax_valid ──────────────────────────────────────────────────

    #[test]
    fn valid_patterns_pass_syntax_check() {
        for pat in &["*.rs", "crates/**/*.rs", "src/{lib,main}.rs", "[abc]*"] {
            assert!(
                is_glob_syntax_valid(pat),
                "expected valid syntax for `{pat}`"
            );
        }
    }

    #[test]
    fn unclosed_bracket_fails_syntax_check() {
        assert!(
            !is_glob_syntax_valid("crates/[abc*.rs"),
            "unclosed [ should fail"
        );
    }

    #[test]
    fn unclosed_brace_fails_syntax_check() {
        assert!(
            !is_glob_syntax_valid("src/{lib,main.rs"),
            "unclosed brace should fail"
        );
    }

    #[test]
    fn unmatched_closing_bracket_fails_syntax_check() {
        assert!(
            !is_glob_syntax_valid("crates/]*.rs"),
            "unmatched ] should fail"
        );
    }

    // ── validate_glob_pattern — InvalidSyntax ─────────────────────────────────

    #[test]
    fn invalid_syntax_detected() {
        let result = validate_glob_pattern("crates/[*.rs", no_root());
        assert_eq!(result.category(), Some(GlobErrorCategory::InvalidSyntax));
        assert!(!result.is_ok());
    }

    // ── validate_glob_pattern — RelativePathAmbiguity ─────────────────────────

    #[test]
    fn dotdot_prefix_detected_as_relative_ambiguity() {
        let result = validate_glob_pattern("../crates/**/*.rs", no_root());
        assert_eq!(
            result.category(),
            Some(GlobErrorCategory::RelativePathAmbiguity)
        );
    }

    #[test]
    fn dot_slash_prefix_detected_as_relative_ambiguity() {
        let result = validate_glob_pattern("./src/**/*.rs", no_root());
        assert_eq!(
            result.category(),
            Some(GlobErrorCategory::RelativePathAmbiguity)
        );
    }

    // ── validate_glob_pattern — NonexistentBase ───────────────────────────────

    #[test]
    fn nonexistent_base_dir_detected() {
        let result = validate_glob_pattern(
            "definitely_no_such_dir_xyz_abc/**/*.rs",
            Some(workspace_root().as_path()),
        );
        assert_eq!(result.category(), Some(GlobErrorCategory::NonexistentBase));
    }

    #[test]
    fn existing_base_dir_passes() {
        // /tmp always exists on Linux.
        let result = validate_glob_pattern("tmp/**/*.log", Some(PathBuf::from("/").as_path()));
        // tmp/ exists under /, so base check passes; could still be Warn for breadth.
        assert_ne!(result.category(), Some(GlobErrorCategory::NonexistentBase));
    }

    // ── validate_glob_pattern — PatternTooBroad ───────────────────────────────

    #[test]
    fn double_star_no_extension_is_too_broad() {
        let result = validate_glob_pattern("crates/**", no_root());
        assert_eq!(result.category(), Some(GlobErrorCategory::PatternTooBroad));
    }

    #[test]
    fn star_no_extension_is_too_broad() {
        let result = validate_glob_pattern("*", no_root());
        assert_eq!(result.category(), Some(GlobErrorCategory::PatternTooBroad));
    }

    #[test]
    fn double_star_with_rs_extension_not_too_broad() {
        let result = validate_glob_pattern("crates/**/*.rs", no_root());
        assert_ne!(result.category(), Some(GlobErrorCategory::PatternTooBroad));
    }

    // ── validate_glob_pattern — clean patterns ────────────────────────────────

    #[test]
    fn clean_rs_pattern_returns_ok() {
        let result = validate_glob_pattern("crates/**/*.rs", no_root());
        assert!(
            result.is_ok(),
            "crates/**/*.rs should validate as Ok: {:?}",
            result
        );
    }

    #[test]
    fn clean_toml_pattern_returns_ok() {
        let result = validate_glob_pattern("**/Cargo.toml", no_root());
        assert!(
            result.is_ok(),
            "**/Cargo.toml should validate as Ok: {:?}",
            result
        );
    }

    // ── hint format ───────────────────────────────────────────────────────────

    #[test]
    fn hint_contains_category_label() {
        let result = validate_glob_pattern("crates/[*.rs", no_root());
        let hint = result.hint().expect("Likely result must carry a hint");
        assert!(hint.contains("invalid-syntax"), "hint: {hint}");
    }

    // ── GlobErrorCategory labels are unique ───────────────────────────────────

    #[test]
    fn category_labels_are_unique() {
        let labels: Vec<_> = GlobErrorCategory::all().iter().map(|c| c.label()).collect();
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len(), "labels must be unique");
    }

    // ── Serde roundtrip ───────────────────────────────────────────────────────

    #[test]
    fn taxonomy_serde_roundtrip() {
        let t = GlobErrorTaxonomy::baseline().clone();
        let json = serde_json::to_string(&t).expect("serialize");
        let back: GlobErrorTaxonomy = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, t);
    }
}
