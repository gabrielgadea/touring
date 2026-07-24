//! Prompt Cache Strategy — stratifies context for maximum Anthropic cache stability.
//!
//! Anthropic prompt caching preserves exact prefixes: tools -> system -> messages.
//! Cache hits cost 10% of input tokens. This module ensures:
//!
//! 1. **Stable context** (project summary, index digest) is emitted FIRST (cached).
//! 2. **Volatile context** (CILA level, focus path, errors, relations) is emitted LAST.
//! 3. Context is deterministically serialized (sorted keys, no HashMap randomization).
//!
//! # MCP Tool Ordering
//!
//! The rmcp `ToolRouter::list_all()` already sorts tools alphabetically by name
//! (see `rmcp/src/handler/server/router/tool.rs:415-418`). Even though tools
//! are stored in a `HashMap`, the serialized tool list is deterministic across
//! server restarts. No additional fix is needed for tool ordering.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::borrow::Cow;
use zstd::stream::{decode_all, encode_all};

// P3-1: Zstd Compression for volatile context
// The volatile context suffix can be large. When the volatile string exceeds
// COMPRESSION_THRESHOLD_CHARS bytes, it is Zstd-compressed to reduce token overhead.
// Compression is transparent to callers — the context is decompressed on retrieval.
// Uses base64 encoding to safely store compressed binary data as ASCII strings.

/// Cache strategy schema version (semver).
///
/// Bump this when the shape of `StableSessionContext` or `VolatilePromptContext`
/// changes. Consumers can use this to detect schema drift across binary updates
/// without parsing the full context string.
///
/// - MAJOR: breaking change in output format (consumers must update parsers).
/// - MINOR: additive fields (backward-compatible enrichment).
/// - PATCH: determinism or sorting fix (output may change byte-for-byte).
///
/// ## Changelog
/// - 2.0.0: Initial stratified context (stable + volatile)
/// - 2.1.0: S0.4 — moved error_patterns and relations to volatile context
pub const CACHE_STRATEGY_VERSION: &str = "2.1.0";

/// Stable session-level context — cached for the entire session.
///
/// Emitted once at SessionStart and re-emitted only if the underlying
/// data changes (file count, relation count, crate list). Positioned
/// early in the context injection to maximize prefix cache hits.
///
/// ## S0.4: Purity Guarantee
///
/// This struct contains ONLY fields that are stable within a session:
/// - Project summary, symbol/file counts (change only on reindex)
/// - Crate names, enforcement rules (change only on project config change)
/// - Top edited files, active gotchas (change infrequently)
///
/// Per-turn fields (error patterns, focus relations) are in `VolatilePromptContext`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableSessionContext {
    /// Human-readable project summary line.
    pub project_summary: String,
    /// Number of symbols indexed.
    pub symbol_count: usize,
    /// Number of files known to the knowledge DB.
    pub file_count: usize,
    /// Sorted list of workspace crate names.
    pub crate_names: Vec<String>,
    /// Sorted list of active enforcement rules.
    pub enforcement_rules: Vec<String>,
    /// Top edited files in current session, sorted by edit count descending.
    pub top_edited_files: Vec<(String, u32)>,
    /// Active gotchas (pattern, description), sorted by pattern.
    pub active_gotchas: Vec<(String, String)>,
}

impl StableSessionContext {
    /// Generate a deterministic string representation for caching.
    ///
    /// All collections are sorted before serialization to ensure identical
    /// output across sessions with the same data, eliminating any iteration
    /// order randomness.
    pub(crate) fn to_cache_string(&self) -> String {
        let mut crates = self.crate_names.clone();
        crates.sort();
        let mut rules = self.enforcement_rules.clone();
        rules.sort();

        let mut parts = vec![format!(
            "Touring v{} Knowledge: {} symbols, {} files | Crates: {} | Rules: {}",
            CACHE_STRATEGY_VERSION,
            self.symbol_count,
            self.file_count,
            crates.join(", "),
            rules.join(", "),
        )];

        // Top edited files — sorted by count desc, then name asc for determinism
        if !self.top_edited_files.is_empty() {
            let mut files = self.top_edited_files.clone();
            files.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let files_str: Vec<String> = files
                .iter()
                .take(5)
                .map(|(f, c)| format!("{}({})", f, c))
                .collect();
            parts.push(format!("Focus: {}", files_str.join(", ")));
        }

        // Active gotchas — sorted by pattern for determinism
        if !self.active_gotchas.is_empty() {
            let mut gotchas = self.active_gotchas.clone();
            gotchas.sort();
            let gotchas_str: Vec<String> = gotchas
                .iter()
                .take(5)
                .map(|(p, g)| format!("[{}] {}", p, g))
                .collect();
            parts.push(format!("Gotchas: {}", gotchas_str.join(" | ")));
        }

        // S0.4: session_error_patterns and focus_file_relations
        // are NO LONGER emitted here — they moved to VolatilePromptContext.

        parts.join(" | ")
    }

    /// Check if the context has materially changed since the last emission.
    ///
    /// Uses the deterministic cache string for comparison — if the string
    /// is identical, no re-emission is needed.
    pub fn has_changed(&self, previous_cache_string: &str) -> bool {
        self.to_cache_string() != previous_cache_string
    }
}

/// Volatile per-turn context — changes each prompt, positioned last.
///
/// This context is appended after the stable prefix. Because it changes
/// every turn (different CILA level, different focus file), it will
/// invalidate any cache that includes it. By placing it last, we ensure
/// the stable prefix remains cacheable.
///
/// ## S0.4: Enriched with per-turn fields
///
/// `session_error_patterns` and `focus_file_relations` were moved here from
/// `StableSessionContext` because they change per-turn and were undermining
/// the stability of the cached prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolatilePromptContext {
    /// CILA complexity level (0-6).
    pub cila_level: u8,
    /// Human-readable CILA level name (e.g., "tool_augmented").
    pub cila_name: String,
    /// Current focus file path, if any.
    pub focus_path: Option<String>,
    /// Sorted list of predicted tool names for this turn.
    pub predicted_tools: Vec<String>,
    /// Whether CODE-FIRST protocol is required.
    pub requires_code_first: bool,
    /// Recent error patterns from this session (moved from Stable in S0.4).
    pub session_error_patterns: Vec<String>,
    /// Relations of the current focus file (moved from Stable in S0.4).
    pub focus_file_relations: Vec<String>,
}

impl VolatilePromptContext {
    /// Serialize to a compact context line.
    ///
    /// Predicted tools are sorted for deterministic output within the same turn
    /// (though the volatile section itself changes between turns).
    pub(crate) fn to_context_string(&self) -> String {
        let focus = self.focus_path.as_deref().unwrap_or("none");
        let mut tools = self.predicted_tools.clone();
        tools.sort();
        let tools_str = if tools.is_empty() {
            String::new()
        } else {
            format!(" | tools={}", tools.join(","))
        };

        // S0.4: Include error patterns and relations (moved from stable)
        let errors_str = if self.session_error_patterns.is_empty() {
            String::new()
        } else {
            let mut errs = self.session_error_patterns.clone();
            errs.sort();
            errs.dedup();
            format!(" | errors={}", errs.join(","))
        };

        let rels_str = if self.focus_file_relations.is_empty() {
            String::new()
        } else {
            let mut rels = self.focus_file_relations.clone();
            rels.sort();
            format!(" | rels={}", rels.join(","))
        };

        format!(
            "\u{21b3} [L{} {}] focus={} | code_first={}{}{}{}",
            self.cila_level,
            self.cila_name,
            focus,
            self.requires_code_first,
            tools_str,
            errors_str,
            rels_str,
        )
    }
}

/// Compose the full context with stable prefix + volatile suffix.
///
/// The stable section is emitted first to maximize prompt cache hits.
/// A newline separates the two sections for readability.
pub fn compose_stratified_context(
    stable: &StableSessionContext,
    volatile: &VolatilePromptContext,
) -> String {
    format!(
        "{}\n{}",
        stable.to_cache_string(),
        volatile.to_context_string()
    )
}

// ── P3-1: Zstd Compression ─────────────────────────────────────────────────

/// Minimum string length (bytes) before triggering Zstd compression.
/// Below this threshold, compression overhead exceeds the token savings.
const COMPRESSION_THRESHOLD_CHARS: usize = 500;

/// E2-S6: Zstd compression level for volatile context.
///
/// Level 1 gives ~15% better compression ratio than level 0 (default)
/// with negligible latency impact (<0.1ms for strings <5KB).
/// Level 0 maps to zstd's internal "default" which is actually level 3,
/// but the API treats 0 as "fast mode". Level 1 is an explicit fast level
/// with better ratio than the implicit level-0 fast mode.
const ZSTD_COMPRESSION_LEVEL: i32 = 1;

/// Compress a volatile context string if it exceeds the threshold.
///
/// Returns the original string if below threshold, or a Zstd-compressed
/// string prefixed with `!:b64:` if compression was applied.
///
/// The `!` prefix acts as a magic marker for decompression detection.
pub fn compress_volatile_context(volatile: &str) -> Cow<'_, str> {
    if volatile.len() < COMPRESSION_THRESHOLD_CHARS {
        return Cow::Borrowed(volatile);
    }

    match encode_all(volatile.as_bytes(), ZSTD_COMPRESSION_LEVEL) {
        Ok(compressed) => {
            // Base64-encode the compressed binary data for safe UTF-8 storage
            let encoded = base64::engine::general_purpose::STANDARD.encode(&compressed);
            Cow::Owned(format!("!:b64:{encoded}"))
        }
        Err(_) => Cow::Borrowed(volatile), // Compression failed — return raw
    }
}

/// Decompress a volatile context string if it was previously compressed.
///
/// Detects the `!:b64:` magic prefix and decompresses accordingly.
/// Returns the original string unchanged if not compressed.
pub fn decompress_volatile_context(volatile: &str) -> Cow<'_, str> {
    if !volatile.starts_with("!:b64:") {
        return Cow::Borrowed(volatile);
    }

    let encoded = &volatile[6..]; // strip the "!:b64:" prefix
    match STANDARD.decode(encoded) {
        Ok(bytes) => match decode_all(bytes.as_slice()) {
            Ok(decoded) => {
                Cow::Owned(String::from_utf8(decoded).expect("zstd decompress produces valid utf8"))
            }
            Err(_) => Cow::Borrowed(volatile), // Decompression failed — return as-is
        },
        Err(_) => Cow::Borrowed(volatile), // Base64 decode failed — return as-is
    }
}

/// Split a previously composed context string back into stable/volatile parts.
///
/// Returns `(stable_part, volatile_part)`. Useful for cache comparison:
/// if the stable part has not changed, only the volatile suffix invalidates.
pub fn split_stratified_context(composed: &str) -> (&str, &str) {
    match composed.split_once('\n') {
        Some((stable, volatile)) => (stable, volatile),
        None => (composed, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_stable() -> StableSessionContext {
        StableSessionContext {
            project_summary: "analise — regulatory analysis workspace".to_string(),
            symbol_count: 300390,
            file_count: 466,
            crate_names: vec![
                "touring-server".to_string(),
                "touring-core".to_string(),
                "touring-ast".to_string(),
                "touring-cognitive".to_string(),
            ],
            enforcement_rules: vec![
                "code-first".to_string(),
                "zero-hallucination".to_string(),
                "taco-identity".to_string(),
            ],
            top_edited_files: vec![],
            active_gotchas: vec![],
        }
    }

    fn sample_volatile() -> VolatilePromptContext {
        VolatilePromptContext {
            cila_level: 2,
            cila_name: "tool_augmented".to_string(),
            focus_path: Some("src/main.rs".to_string()),
            predicted_tools: vec!["Read".to_string(), "Grep".to_string(), "Bash".to_string()],
            requires_code_first: true,
            session_error_patterns: vec![],
            focus_file_relations: vec![],
        }
    }

    // ── Determinism Tests ──────────────────────────────────────────────

    #[test]
    fn stable_context_is_deterministic_across_calls() {
        let ctx = sample_stable();
        let s1 = ctx.to_cache_string();
        let s2 = ctx.to_cache_string();
        assert_eq!(s1, s2, "Same input must produce identical output");
    }

    #[test]
    fn stable_context_sorts_crate_names() {
        let ctx = sample_stable();
        let s = ctx.to_cache_string();
        assert!(
            s.contains("touring-ast, touring-cognitive, touring-core, touring-server"),
            "Crate names must be sorted. Got: {s}"
        );
    }

    #[test]
    fn stable_context_sorts_enforcement_rules() {
        let ctx = sample_stable();
        let s = ctx.to_cache_string();
        assert!(
            s.contains("code-first, taco-identity, zero-hallucination"),
            "Rules must be sorted. Got: {s}"
        );
    }

    #[test]
    fn stable_context_identical_data_produces_identical_string() {
        let ctx1 = sample_stable();
        let ctx2 = StableSessionContext {
            project_summary: "analise — regulatory analysis workspace".to_string(),
            symbol_count: 300390,
            file_count: 466,
            crate_names: vec![
                "touring-cognitive".to_string(),
                "touring-ast".to_string(),
                "touring-server".to_string(),
                "touring-core".to_string(),
            ],
            enforcement_rules: vec![
                "zero-hallucination".to_string(),
                "code-first".to_string(),
                "taco-identity".to_string(),
            ],
            top_edited_files: vec![],
            active_gotchas: vec![],
        };
        assert_eq!(
            ctx1.to_cache_string(),
            ctx2.to_cache_string(),
            "Different insertion order must produce identical cache strings"
        );
    }

    #[test]
    fn stable_context_has_changed_detects_difference() {
        let ctx = sample_stable();
        let cached = ctx.to_cache_string();

        assert!(!ctx.has_changed(&cached));

        let mut modified = ctx.clone();
        modified.file_count = 500;
        assert!(modified.has_changed(&cached));
    }

    #[test]
    fn stable_context_has_changed_identical_returns_false() {
        let ctx = sample_stable();
        let cached = ctx.to_cache_string();
        assert!(
            !ctx.has_changed(&cached),
            "Identical context should report no change"
        );
    }

    // ── S0.4: Stable prefix no longer contains errors/relations ──────

    #[test]
    fn s0_4_stable_prefix_excludes_errors_and_relations() {
        let ctx = StableSessionContext {
            top_edited_files: vec![("a.rs".into(), 1)],
            active_gotchas: vec![("pat".into(), "desc".into())],
            ..sample_stable()
        };
        let s = ctx.to_cache_string();
        assert!(
            !s.contains("Errors:"),
            "S0.4: Stable prefix must NOT contain Errors. Got: {s}"
        );
        assert!(
            !s.contains("Relations:"),
            "S0.4: Stable prefix must NOT contain Relations. Got: {s}"
        );
    }

    #[test]
    fn s0_4_stable_prefix_unchanged_between_turns() {
        // The stable prefix should be identical regardless of what
        // errors/relations exist (they're now volatile).
        let ctx = sample_stable();
        let s1 = ctx.to_cache_string();
        let s2 = ctx.to_cache_string();
        assert_eq!(s1, s2, "Stable prefix must be identical between turns");
    }

    // ── Volatile Context Tests ─────────────────────────────────────────

    #[test]
    fn volatile_context_includes_cila_and_focus() {
        let vol = sample_volatile();
        let s = vol.to_context_string();
        assert!(s.contains("[L2 tool_augmented]"), "Missing CILA. Got: {s}");
        assert!(s.contains("focus=src/main.rs"), "Missing focus. Got: {s}");
        assert!(
            s.contains("code_first=true"),
            "Missing code_first. Got: {s}"
        );
    }

    #[test]
    fn volatile_context_no_focus_shows_none() {
        let vol = VolatilePromptContext {
            cila_level: 0,
            cila_name: "direct".to_string(),
            focus_path: None,
            predicted_tools: vec![],
            requires_code_first: false,
            session_error_patterns: vec![],
            focus_file_relations: vec![],
        };
        let s = vol.to_context_string();
        assert!(s.contains("focus=none"), "Missing none focus. Got: {s}");
    }

    #[test]
    fn volatile_context_sorts_predicted_tools() {
        let vol = sample_volatile();
        let s = vol.to_context_string();
        assert!(
            s.contains("tools=Bash,Grep,Read"),
            "Predicted tools must be sorted. Got: {s}"
        );
    }

    #[test]
    fn volatile_context_empty_tools_omits_field() {
        let vol = VolatilePromptContext {
            cila_level: 1,
            cila_name: "pal".to_string(),
            focus_path: None,
            predicted_tools: vec![],
            requires_code_first: false,
            session_error_patterns: vec![],
            focus_file_relations: vec![],
        };
        let s = vol.to_context_string();
        assert!(
            !s.contains("tools="),
            "Empty tools should be omitted. Got: {s}"
        );
    }

    // ── S0.4: Volatile context now includes errors/relations ─────────

    #[test]
    fn s0_4_volatile_includes_error_patterns() {
        let vol = VolatilePromptContext {
            session_error_patterns: vec!["string_not_found".into(), "syntax_error".into()],
            ..sample_volatile()
        };
        let s = vol.to_context_string();
        assert!(
            s.contains("errors=string_not_found,syntax_error"),
            "S0.4: Volatile must include errors. Got: {s}"
        );
    }

    #[test]
    fn s0_4_volatile_includes_focus_relations() {
        let vol = VolatilePromptContext {
            focus_file_relations: vec!["mod_b".into(), "mod_a".into()],
            ..sample_volatile()
        };
        let s = vol.to_context_string();
        assert!(
            s.contains("rels=mod_a,mod_b"),
            "S0.4: Volatile must include sorted relations. Got: {s}"
        );
    }

    #[test]
    fn s0_4_volatile_empty_errors_and_rels_omitted() {
        let vol = sample_volatile(); // both empty
        let s = vol.to_context_string();
        assert!(!s.contains("errors="), "Empty errors should be omitted");
        assert!(!s.contains("rels="), "Empty rels should be omitted");
    }

    #[test]
    fn s0_4_volatile_deduplicates_errors() {
        let vol = VolatilePromptContext {
            session_error_patterns: vec!["err_a".into(), "err_b".into(), "err_a".into()],
            ..sample_volatile()
        };
        let s = vol.to_context_string();
        // Should be deduplicated and sorted
        assert!(
            s.contains("errors=err_a,err_b"),
            "Errors should be deduplicated. Got: {s}"
        );
        // Count occurrences of err_a — should be exactly 1
        assert_eq!(
            s.matches("err_a").count(),
            1,
            "err_a should appear only once after dedup"
        );
    }

    // ── Composition Tests ──────────────────────────────────────────────

    #[test]
    fn compose_puts_stable_before_volatile() {
        let stable = sample_stable();
        let volatile = sample_volatile();
        let composed = compose_stratified_context(&stable, &volatile);

        let lines: Vec<&str> = composed.lines().collect();
        assert_eq!(lines.len(), 2, "Should have exactly 2 lines");
        assert!(
            lines[0].starts_with("Touring v"),
            "First line must be stable context"
        );
        assert!(
            lines[1].starts_with('\u{21b3}'),
            "Second line must be volatile context (arrow prefix)"
        );
    }

    #[test]
    fn split_roundtrips_correctly() {
        let stable = sample_stable();
        let volatile = sample_volatile();
        let composed = compose_stratified_context(&stable, &volatile);

        let (stable_part, volatile_part) = split_stratified_context(&composed);
        assert_eq!(stable_part, stable.to_cache_string());
        assert_eq!(volatile_part, volatile.to_context_string());
    }

    #[test]
    fn split_handles_no_newline() {
        let (stable, volatile) = split_stratified_context("just stable");
        assert_eq!(stable, "just stable");
        assert_eq!(volatile, "");
    }

    // ── Edge Cases ─────────────────────────────────────────────────────

    #[test]
    fn stable_context_empty_collections() {
        let ctx = StableSessionContext {
            project_summary: String::new(),
            symbol_count: 0,
            file_count: 0,
            crate_names: vec![],
            enforcement_rules: vec![],
            top_edited_files: vec![],
            active_gotchas: vec![],
        };
        let s = ctx.to_cache_string();
        assert!(
            s.contains("0 symbols, 0 files"),
            "Should handle zero counts. Got: {s}"
        );
        assert!(
            s.starts_with(&format!("Touring v{}", CACHE_STRATEGY_VERSION)),
            "Should include version prefix. Got: {s}"
        );
        assert!(
            s.contains("Crates:  |"),
            "Empty crates should produce empty string. Got: {s}"
        );
    }

    #[test]
    fn volatile_context_high_cila_level() {
        let vol = VolatilePromptContext {
            cila_level: 6,
            cila_name: "multi_agent".to_string(),
            focus_path: Some("/very/long/path/to/deeply/nested/file.rs".to_string()),
            predicted_tools: vec![
                "TaskCreate".to_string(),
                "TeamCreate".to_string(),
                "Bash".to_string(),
            ],
            requires_code_first: true,
            session_error_patterns: vec![],
            focus_file_relations: vec![],
        };
        let s = vol.to_context_string();
        assert!(s.contains("[L6 multi_agent]"));
        assert!(s.contains("tools=Bash,TaskCreate,TeamCreate"));
    }

    // ── Enriched StableSessionContext Tests (E6.1) ────────────────────

    #[test]
    fn stable_context_includes_top_edited_files() {
        let mut ctx = sample_stable();
        ctx.top_edited_files = vec![("server.rs".into(), 5), ("main.rs".into(), 3)];
        let s = ctx.to_cache_string();
        assert!(s.contains("Focus:"), "Missing Focus section. Got: {s}");
        assert!(s.contains("server.rs(5)"), "Missing file count. Got: {s}");
    }

    #[test]
    fn stable_context_includes_active_gotchas() {
        let mut ctx = sample_stable();
        ctx.active_gotchas = vec![("rust_bridge".into(), "use matched_text".into())];
        let s = ctx.to_cache_string();
        assert!(s.contains("Gotchas:"), "Missing Gotchas. Got: {s}");
        assert!(s.contains("[rust_bridge]"), "Missing pattern. Got: {s}");
    }

    #[test]
    fn stable_context_empty_new_fields_produce_no_extra_output() {
        let ctx = sample_stable(); // all enrichment fields empty
        let s = ctx.to_cache_string();
        assert!(!s.contains("Focus:"), "Should omit empty Focus");
        assert!(!s.contains("Gotchas:"), "Should omit empty Gotchas");
        // S0.4: Errors and Relations are now volatile, not stable
        assert!(!s.contains("Errors:"), "Should not have Errors");
        assert!(!s.contains("Relations:"), "Should not have Relations");
    }

    #[test]
    fn stable_context_sorts_new_fields_deterministically() {
        let ctx1 = StableSessionContext {
            top_edited_files: vec![("b.rs".into(), 2), ("a.rs".into(), 3)],
            active_gotchas: vec![
                ("z_pat".into(), "z_gotcha".into()),
                ("a_pat".into(), "a_gotcha".into()),
            ],
            ..sample_stable()
        };
        let ctx2 = StableSessionContext {
            top_edited_files: vec![("a.rs".into(), 3), ("b.rs".into(), 2)],
            active_gotchas: vec![
                ("a_pat".into(), "a_gotcha".into()),
                ("z_pat".into(), "z_gotcha".into()),
            ],
            ..sample_stable()
        };
        assert_eq!(
            ctx1.to_cache_string(),
            ctx2.to_cache_string(),
            "Different insertion order must produce identical output"
        );
    }

    // ── Cache Strategy Version Tests (E4.3) ─────────────────────────────

    #[test]
    fn cache_strategy_version_is_included_in_stable_output() {
        let ctx = sample_stable();
        let s = ctx.to_cache_string();
        let version_tag = format!("Touring v{} Knowledge:", CACHE_STRATEGY_VERSION,);
        assert!(
            s.contains(&version_tag),
            "Stable context must include version tag. Got: {s}"
        );
    }

    #[test]
    fn cache_strategy_version_is_stable_across_calls() {
        let v1 = CACHE_STRATEGY_VERSION;
        let v2 = CACHE_STRATEGY_VERSION;
        assert_eq!(v1, v2, "Version constant must be stable");
        assert!(
            v1.starts_with("2."),
            "Version must be 2.x.x (current schema). Got: {v1}"
        );
    }

    #[test]
    fn cache_strategy_version_is_valid_semver() {
        let parts: Vec<&str> = CACHE_STRATEGY_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "Version must be semver (MAJOR.MINOR.PATCH)");
        for (i, part) in parts.iter().enumerate() {
            assert!(
                part.parse::<u32>().is_ok(),
                "Version part {i} must be numeric. Got: {part}"
            );
        }
    }

    #[test]
    fn s0_6_version_is_2_1_0() {
        assert_eq!(
            CACHE_STRATEGY_VERSION, "2.1.0",
            "S0.6: Version must be 2.1.0 after S0.4 changes"
        );
    }

    // ── Output Stability Contract (E2.3) ────────────────────────────────

    #[test]
    fn output_is_byte_identical_across_multiple_calls() {
        let stable = sample_stable();
        let volatile = sample_volatile();

        let reference = compose_stratified_context(&stable, &volatile);
        for i in 1..=10 {
            let output = compose_stratified_context(&stable, &volatile);
            assert_eq!(
                reference.as_bytes(),
                output.as_bytes(),
                "Output must be byte-identical on call #{i}"
            );
        }
    }

    #[test]
    fn output_stability_with_all_enrichment_fields() {
        let stable = StableSessionContext {
            project_summary: "test project".to_string(),
            symbol_count: 1000,
            file_count: 50,
            crate_names: vec!["c".into(), "a".into(), "b".into()],
            enforcement_rules: vec!["z-rule".into(), "a-rule".into()],
            top_edited_files: vec![("z.rs".into(), 1), ("a.rs".into(), 5)],
            active_gotchas: vec![
                ("z_pattern".into(), "z desc".into()),
                ("a_pattern".into(), "a desc".into()),
            ],
        };
        let volatile = VolatilePromptContext {
            cila_level: 4,
            cila_name: "agent_loop".to_string(),
            focus_path: Some("lib.rs".to_string()),
            predicted_tools: vec!["Write".into(), "Read".into(), "Bash".into()],
            requires_code_first: true,
            session_error_patterns: vec!["err_z".into(), "err_a".into(), "err_z".into()],
            focus_file_relations: vec!["rel_z".into(), "rel_a".into()],
        };

        let reference = compose_stratified_context(&stable, &volatile);
        for i in 1..=10 {
            let output = compose_stratified_context(&stable, &volatile);
            assert_eq!(
                reference, output,
                "Enriched output must be byte-identical on call #{i}"
            );
        }

        // Verify sorting is applied (determinism guarantee)
        assert!(reference.contains("Crates: a, b, c"), "Crates not sorted");
        assert!(reference.contains("a-rule, z-rule"), "Rules not sorted");
        assert!(
            reference.contains("tools=Bash,Read,Write"),
            "Tools not sorted"
        );
        // S0.4: errors and rels now in volatile line
        assert!(
            reference.contains("errors=err_a,err_z"),
            "Errors should be sorted+deduped in volatile"
        );
        assert!(
            reference.contains("rels=rel_a,rel_z"),
            "Relations should be sorted in volatile"
        );
    }

    // ── P3-1: Zstd Compression tests ────────────────────────────────────────

    #[test]
    fn test_compress_small_string_passes_through() {
        let short = "short string";
        let result = compress_volatile_context(short);
        assert_eq!(result, short);
        assert!(!result.contains("!:b64:"));
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let large: String = "x".repeat(1000);
        let compressed = compress_volatile_context(&large);
        assert!(compressed.starts_with("!:b64:"));

        let decompressed = decompress_volatile_context(&compressed);
        assert_eq!(decompressed, large);
    }

    #[test]
    fn test_decompress_non_compressed_passes_through() {
        let raw = "not compressed at all";
        let result = decompress_volatile_context(raw);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_compress_threshold_boundary() {
        // Below threshold should not compress
        let below_threshold: String = "x".repeat(COMPRESSION_THRESHOLD_CHARS - 1);
        let result = compress_volatile_context(&below_threshold);
        assert!(!result.starts_with("!:b64:"));

        // At threshold should compress
        let at_threshold: String = "x".repeat(COMPRESSION_THRESHOLD_CHARS);
        let result2 = compress_volatile_context(&at_threshold);
        assert!(result2.starts_with("!:b64:"));
    }

    #[test]
    fn test_compression_reduces_size() {
        let large = "content to compress ".repeat(100);
        let compressed = compress_volatile_context(&large);
        // Must have started with compression
        assert!(compressed.starts_with("!:b64:"));
        let decompressed = decompress_volatile_context(&compressed).into_owned();
        assert_eq!(decompressed, large);
    }

    #[test]
    fn test_compress_tolerates_corrupt_data() {
        let corrupt = "!:b64:not_valid_compressed_data";
        // Decompression of invalid data should return original
        let result = decompress_volatile_context(corrupt);
        assert_eq!(result, corrupt);
    }
}
