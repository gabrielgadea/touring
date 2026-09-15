//! SecurityAnalyzer — bridges VulnerabilityPattern (touring-offensive) with antipattern detection.
//!
//! Provides unified `SecurityReport` combining:
//! - **Antipattern hits** (SIMD memchr scanning from `touring-analysis`)
//! - **Vulnerability matches** (VulnerabilityPattern trait from `touring-offensive`)
//!
//! # Architecture
//!
//! - `touring-analysis/src/quality/security.rs` imports from `touring-offensive::vuln::`
//! - `touring-offensive` stays "offensive-only" — it defines the trait and concrete patterns
//! - `touring-analysis` implements the analyzer that composes both systems
//! - No circular dependency: touring-analysis → touring-offensive (directional, clean)
//!
//! # Example
//!
//! ```
//! use touring_analysis::quality::SecurityAnalyzer;
//!
//! let source = "fn example() { let x = something.unwrap(); }";
//! let report = SecurityAnalyzer::new().analyze(source, "rust");
//! assert!(!report.antipattern_hits.is_empty() || !report.vuln_matches.is_empty());
//! ```

use std::sync::Arc;
use touring_offensive::vuln::{PatternRegistry, VulnMatch, VulnerabilityPattern};

/// Unified security report combining antipattern and vulnerability analysis.
#[derive(Debug, Clone)]
pub struct SecurityReport {
    /// Antipattern hits: (warning_message, first_line_number).
    pub antipattern_hits: Vec<(String, usize)>,
    /// Vulnerability matches from touring-offensive patterns.
    pub vuln_matches: Vec<VulnMatch>,
    /// Combined quality score in [0.0, 1.0]: 0.4 * antipattern_score + 0.6 * vuln_score.
    pub combined_score: f32,
    /// Language identifier used for analysis.
    pub lang: String,
}

/// Analyzes source code for antipatterns AND vulnerability patterns.
pub struct SecurityAnalyzer {
    /// Pre-loaded registry of CWE/OWASP patterns (from `PatternRegistry::all()`).
    registry: PatternRegistry,
    /// Additional patterns supplied via [`with_patterns`].
    custom_patterns: Vec<Arc<dyn VulnerabilityPattern>>,
}

impl SecurityAnalyzer {
    /// Creates a new SecurityAnalyzer with all registered vulnerability patterns.
    pub fn new() -> Self {
        Self {
            registry: PatternRegistry::all(),
            custom_patterns: vec![],
        }
    }

    /// Creates a SecurityAnalyzer with custom vulnerability patterns only.
    ///
    /// The built-in `PatternRegistry::all()` patterns are NOT included; callers
    /// supply the full pattern set.
    pub fn with_patterns(patterns: Vec<Arc<dyn VulnerabilityPattern>>) -> Self {
        Self {
            registry: PatternRegistry::new(),
            custom_patterns: patterns,
        }
    }

    /// Analyzes `source` code in `lang` for antipatterns and vulnerability patterns.
    ///
    /// Returns a `SecurityReport` with combined scoring:
    /// - `antipattern_score = 1.0` if no hits, `0.0` if hits found
    /// - `vuln_score = 1.0 - min(sum(severities) / 10.0, 1.0)`
    /// - `combined_score = 0.4 * antipattern_score + 0.6 * vuln_score`
    pub fn analyze(&self, source: &str, lang: &str) -> SecurityReport {
        // 1. Antipattern detection (SIMD memchr, touring-analysis native)
        let antipattern_hits = super::antipatterns::detect_antipatterns(source, lang);

        // 2. Vulnerability pattern detection: built-in registry + custom patterns,
        // EVERY match of each (cross-audit R2, 14/09/2026). With one match per
        // pattern, the precision pass below dropped a payload quoted in a comment
        // at the top of a file and took the real sink further down with it.
        let mut vuln_matches: Vec<VulnMatch> = self.registry.detect_every(source);
        vuln_matches.extend(
            self.custom_patterns
                .iter()
                .flat_map(|p| p.detect_every(source)),
        );

        // 2b. AST-aware precision pass (2026-06-21): a vulnerability literal
        // living in a comment or a `#[cfg(test)]` corpus is documentation /
        // fixture, not an exploitable sink — drop it. Mirrors Semgrep's
        // comment-disregard + test-path exclusion, applied at in-file region
        // granularity (more precise than path exclusion: a real sink in a
        // production helper sharing a file with a test module is still scanned).
        // Guarded on non-empty so the lexer runs only for the ~1% of files that
        // actually produced a match. See `super::code_regions`.
        if !vuln_matches.is_empty() {
            let mut regions = super::code_regions::non_executable_regions(source, lang);
            if lang == "python" {
                // A docstring documents; it never reaches a sink (cross-audit R2).
                regions.extend(super::code_regions::python_docstring_regions(source));
            }
            if !regions.is_empty() {
                vuln_matches
                    .retain(|v| !super::code_regions::offset_suppressed(v.span.0, &regions));
            }
        }
        if is_shell(lang) {
            vuln_matches.retain(|v| !is_shell_syntax_payload(source, v));
        }
        // One finding per pattern, the first that survived: the score keeps the
        // meaning it had when each pattern reported a single match.
        let mut seen: Vec<String> = Vec::new();
        vuln_matches.retain(|v| {
            let first = !seen.contains(&v.pattern_name);
            if first {
                seen.push(v.pattern_name.clone());
            }
            first
        });

        // 3. Combined scoring
        let antipattern_score = if antipattern_hits.is_empty() {
            1.0
        } else {
            0.0
        };
        let vuln_score = if vuln_matches.is_empty() {
            1.0
        } else {
            let sum_severity: f32 = vuln_matches.iter().map(|v| v.severity).sum();
            (1.0 - (sum_severity / 10.0)).clamp(0.0, 1.0)
        };
        let combined_score = 0.4 * antipattern_score + 0.6 * vuln_score;

        SecurityReport {
            antipattern_hits,
            vuln_matches,
            combined_score,
            lang: lang.to_string(),
        }
    }
}

/// Whether `lang` names a POSIX-shell dialect.
fn is_shell(lang: &str) -> bool {
    matches!(lang, "shell" | "bash" | "sh" | "zsh")
}

/// A CMDi match that is ordinary shell syntax in a shell script.
///
/// The metacharacter arms of the CWE-78 pattern (`; rm`, `| ncat`, `&& curl`)
/// describe a payload smuggled INTO a command string from another language. In
/// a shell script they are the script itself: `install.sh` downloading its
/// signature with `&& curl -fSL …` failed F2.1 (cross-audit R2, 14/09/2026).
/// The dynamic-exec arms (`os.system(f"…")`, `shell=True`, `exec(`${…}`)`)
/// never start with a metacharacter and still count.
fn is_shell_syntax_payload(source: &str, m: &VulnMatch) -> bool {
    m.pattern_name == "CMDi"
        && source
            .get(m.span.0..m.span.1)
            .is_some_and(|text| text.starts_with([';', '|', '&']))
}

impl Default for SecurityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
