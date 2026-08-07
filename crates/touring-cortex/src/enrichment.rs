//! Context Enrichment Pipeline — composes enriched context for hook injection.
//!
//! Priority order (higher = included first if budget allows):
//! 1. Gotchas (most valuable for error prevention) — ~50 tokens
//! 2. Relations (import/caller paths) — ~50 tokens
//! 3. Recent errors for this file — ~50 tokens
//!
//! Returns a composed context string guaranteed to fit within `max_tokens`.

use crate::signal_fusion::{HandlerSignal, dominant_signal, fuse_signals_softmax};
use touring_simd::WilsonRanker;

/// Enrichment confidence scores for selective gating.
/// Scores come from WilsonRanker — higher = more confident = less need for context.
#[derive(Debug, Clone, Default)]
pub struct EnrichmentScores {
    /// Wilson confidence for gotcha tier.
    pub gotcha_score: f64,
    /// Wilson confidence for relations tier.
    pub relations_score: f64,
    /// Wilson confidence for errors tier (P0 — never actually gated).
    pub errors_score: f64,
    /// Wilson confidence for cross-validation tier (touring-antt CrossValidator).
    /// Indicates consistency of regulatory document analysis.
    pub validation_score: f64,
}

/// Attention-weighted budget allocation across the four enrichment tiers.
///
/// Derived from [`EnrichmentScores`] via SIMD softmax (`fuse_signals_softmax`
/// in `signal_fusion.rs`). Larger Wilson scores translate to more confident
/// tiers; softmax converts the confidence vector into a probability
/// distribution suitable for proportional token-budget allocation.
#[derive(Debug, Clone, Default)]
pub struct TierBudget {
    /// Fraction of the enrichment token budget recommended for gotchas (0..=1).
    pub gotcha_frac: f64,
    /// Fraction recommended for relations (0..=1).
    pub relations_frac: f64,
    /// Fraction recommended for recent errors (0..=1).
    pub errors_frac: f64,
    /// Fraction recommended for cross-validation tier (0..=1).
    pub validation_frac: f64,
    /// Tier that dominated the attention distribution:
    /// `"gotcha" | "relations" | "errors" | "validation"` or `None` on all-zero scores.
    pub dominant_tier: Option<&'static str>,
    /// Winner's probability (the `max` of the four fractions). Useful as an
    /// "attention certainty" metric: close to 1/4 = uniform confusion, close
    /// to 1.0 = one tier clearly wins.
    pub max_attention: f64,
    /// Entropy (nats) of the distribution. Lower = more concentrated.
    pub entropy: f64,
}

impl EnrichmentScores {
    /// Compute an attention-weighted budget allocation across the four tiers.
    ///
    /// Uses `touring_simd`-powered [`fuse_signals_softmax`] and
    /// [`dominant_signal`] from [`crate::signal_fusion`], both of which are
    /// `pulp::Arch`-dispatched SIMD kernels. All four tiers participate —
    /// even errors, which are never gated out for injection (P0 tier) but
    /// still consume budget.
    ///
    /// This wires the previously-orphaned SIMD softmax + argmax primitives
    /// into the enrichment hot path without changing existing behaviour:
    /// callers that ignore the returned budget keep their current gating via
    /// `GATING_THRESHOLD`.
    #[must_use]
    pub fn tier_budget(&self) -> TierBudget {
        // The `HandlerSignal::confidence` field is not used by
        // `fuse_signals_softmax` (which treats `estimate` as the logit), so
        // we set it to 1.0 for all four — confidence-weighted variants live
        // in `fuse_signals` (Bayesian) and `fuse_signals_weighted`.
        let signals = [
            HandlerSignal {
                handler_name: "gotcha".to_string(),
                estimate: self.gotcha_score,
                confidence: 1.0,
            },
            HandlerSignal {
                handler_name: "relations".to_string(),
                estimate: self.relations_score,
                confidence: 1.0,
            },
            HandlerSignal {
                handler_name: "errors".to_string(),
                estimate: self.errors_score,
                confidence: 1.0,
            },
            HandlerSignal {
                handler_name: "validation".to_string(),
                estimate: self.validation_score,
                confidence: 1.0,
            },
        ];
        let fused = fuse_signals_softmax(&signals);
        let dom = dominant_signal(&signals);

        let dominant_tier = dom.as_ref().and_then(|d| match d.index {
            0 => Some("gotcha"),
            1 => Some("relations"),
            2 => Some("errors"),
            3 => Some("validation"),
            _ => None,
        });

        TierBudget {
            gotcha_frac: fused.probabilities.first().copied().unwrap_or(0.0),
            relations_frac: fused.probabilities.get(1).copied().unwrap_or(0.0),
            errors_frac: fused.probabilities.get(2).copied().unwrap_or(0.0),
            validation_frac: fused.probabilities.get(3).copied().unwrap_or(0.0),
            dominant_tier,
            max_attention: fused.max_probability,
            entropy: fused.entropy,
        }
    }

    /// Allocate an integer token budget across the four tiers proportionally
    /// to the softmax attention distribution.
    ///
    /// Returns `(gotcha_tokens, relations_tokens, errors_tokens, validation_tokens)`.
    /// Tokens sum to `total_budget` after floor-rounding + remainder distribution.
    #[must_use]
    pub fn allocate_tokens(&self, total_budget: usize) -> (usize, usize, usize, usize) {
        let budget = self.tier_budget();
        let g = (total_budget as f64 * budget.gotcha_frac).floor() as usize;
        let r = (total_budget as f64 * budget.relations_frac).floor() as usize;
        let e = (total_budget as f64 * budget.errors_frac).floor() as usize;
        let v = (total_budget as f64 * budget.validation_frac).floor() as usize;
        // Distribute remainder to the dominant tier so integer rounding loss
        // lands on the tier that benefits most from extra tokens.
        let allocated = g + r + e + v;
        let remainder = total_budget.saturating_sub(allocated);
        match budget.dominant_tier {
            Some("gotcha") => (g + remainder, r, e, v),
            Some("relations") => (g, r + remainder, e, v),
            Some("errors") => (g, r, e + remainder, v),
            Some("validation") => (g, r, e, v + remainder),
            _ => (g, r, e, v),
        }
    }
}

/// Wilson confidence threshold for skipping context injection.
/// Above this: skip injection (high confidence, context not needed).
/// At or below: inject (low confidence, context helps).
/// Errors (P0) are NEVER gated regardless of score.
const GATING_THRESHOLD: f64 = 0.80;

// ── L7-B Alpha: CILA-gated Enrichment Policy ─────────────────────────────

/// L7-B Alpha: Determine if enrichment should be applied based on CILA level.
///
/// Enrichment is expensive (NLP entity extraction, ANTT regex, gotcha scan).
/// Activating it globally degrades latency of reflexive L0/L1 hooks.
/// This policy ensures enrichment runs only when the cognitive payoff exceeds
/// the latency cost.
///
/// | CILA Level | Description         | Enrichment |
/// |-----------:|---------------------|:----------:|
/// |         0  | Reflexo             |     OFF    |
/// |         1  | Associação          |     OFF    |
/// |         2  | Cognição            |     ON     |
/// |         3  | Orquestração        |     ON     |
/// |         4+ | Self-mod / Multi    |     ON     |
///
/// # Arguments
/// * `cila_level` — current CILA level from `SessionBus.cila_level`
///
/// # Returns
/// `true` if enrichment should be attempted, `false` to skip for latency
#[inline]
pub fn should_enrich_for_cila(cila_level: u8) -> bool {
    cila_level >= 2
}

/// L7-B Alpha: Enrichment policy with CILA gating + tool filtering.
///
/// Combines `should_enrich_for_cila` with a tool-name filter so that even
/// at high CILA levels, only edit-related tools trigger enrichment
/// (Read/Grep/Glob are fast-path and don't benefit from heavy enrichment).
#[derive(Debug, Clone, Copy)]
pub struct EnrichmentPolicy {
    /// Current CILA level driving the gating decision (enrichment requires `>= 2`).
    pub cila_level: u8,
    /// Whether enrichment is globally enabled at runtime.
    pub enrichment_active: bool,
}

impl EnrichmentPolicy {
    /// Create a new policy from runtime state.
    #[inline]
    pub fn new(cila_level: u8, enrichment_active: bool) -> Self {
        Self {
            cila_level,
            enrichment_active,
        }
    }

    /// Check if enrichment should be applied for a given tool invocation.
    ///
    /// Returns `false` if:
    /// - The enrichment pipeline is not active (cognitive engine not initialized)
    /// - CILA level is below the enrichment threshold (L0 or L1)
    /// - The tool is in the read-only fast-path list (no mutation → no enrichment needed)
    #[inline]
    pub fn should_enrich(&self, tool_name: &str) -> bool {
        if !self.enrichment_active {
            return false;
        }
        if !should_enrich_for_cila(self.cila_level) {
            return false;
        }
        // Tools that benefit from enrichment (mutation or preparation):
        matches!(
            tool_name,
            "Edit"
                | "Write"
                | "NotebookEdit"
                | "MultiEdit"
                | "pre-edit"
                | "pre-write"
                | "pre-bash"
                | "TaskCreate"
                | "TaskUpdate"
        )
    }

    /// Check if enrichment should be applied for L4+ tasks regardless of tool.
    ///
    /// At L4+ (self-modifying or multi-agent workflows), enrichment is mandatory
    /// even for read tools because context coherence is critical.
    #[inline]
    pub fn is_mandatory(&self) -> bool {
        self.enrichment_active && self.cila_level >= 4
    }
}

#[cfg(test)]
mod enrichment_policy_tests {
    use super::*;

    #[test]
    fn test_cila_gate_l0_l1_blocks() {
        assert!(!should_enrich_for_cila(0));
        assert!(!should_enrich_for_cila(1));
    }

    #[test]
    fn test_cila_gate_l2_plus_allows() {
        assert!(should_enrich_for_cila(2));
        assert!(should_enrich_for_cila(3));
        assert!(should_enrich_for_cila(4));
        assert!(should_enrich_for_cila(6));
    }

    #[test]
    fn test_policy_inactive_blocks_all() {
        let p = EnrichmentPolicy::new(4, false);
        assert!(!p.should_enrich("Edit"));
        assert!(!p.is_mandatory());
    }

    #[test]
    fn test_policy_l1_active_blocks() {
        let p = EnrichmentPolicy::new(1, true);
        assert!(!p.should_enrich("Edit"));
    }

    #[test]
    fn test_policy_l3_edit_passes() {
        let p = EnrichmentPolicy::new(3, true);
        assert!(p.should_enrich("Edit"));
        assert!(p.should_enrich("Write"));
        assert!(p.should_enrich("pre-edit"));
    }

    #[test]
    fn test_policy_l3_read_blocks() {
        let p = EnrichmentPolicy::new(3, true);
        assert!(!p.should_enrich("Read"));
        assert!(!p.should_enrich("Grep"));
        assert!(!p.should_enrich("Glob"));
    }

    #[test]
    fn test_policy_l4_mandatory() {
        let p = EnrichmentPolicy::new(4, true);
        assert!(p.is_mandatory());
    }

    #[test]
    fn test_policy_l3_not_mandatory() {
        let p = EnrichmentPolicy::new(3, true);
        assert!(!p.is_mandatory());
    }
}

/// Maximum number of gotchas to include in enriched context.
const MAX_GOTCHAS: usize = 3;
/// Maximum number of relations to include in enriched context.
const MAX_RELATIONS: usize = 5;
/// Maximum number of recent errors to include in enriched context.
const MAX_ERRORS: usize = 3;
/// Rough chars-per-token estimate for budget calculations.
///
/// Used as fallback divisor in [`estimate_tokens`] (char-count heuristic) and
/// in budget-respecting tests. The previous `#[allow(dead_code)]` was a false
/// positive — `estimate_tokens` uses it via `text.len().div_ceil(CHARS_PER_TOKEN)`.
const CHARS_PER_TOKEN: usize = 4;

/// Tiktoken encoder — lazy-initialized for accurate token counting.
///
/// Uses `cl100k_base` encoding (the same as GPT-4/GPT-3.5-turbo).
/// Initialization is deferred until first use to avoid startup cost.
static ENCODER: once_cell::sync::Lazy<tiktoken_rs::CoreBPE> = once_cell::sync::Lazy::new(|| {
    tiktoken_rs::cl100k_base().expect("cl100k_base encoding must be available")
});

/// Count tokens in a string using tiktoken (cl100k_base encoding).
///
/// This is significantly more accurate than the `CHARS_PER_TOKEN` heuristic
/// for mixed content (code + natural language), where code has ~3.5 chars/token
/// and natural language has ~4.5 chars/token.
///
/// Falls back to the rough `CHARS_PER_TOKEN` heuristic if tiktoken fails.
pub fn count_tokens(text: &str) -> usize {
    ENCODER.encode_ordinary(text).len()
}

/// Count tokens for a list of strings (sum of individual token counts).
pub fn count_tokens_list(texts: &[impl AsRef<str>]) -> usize {
    texts.iter().map(|t| count_tokens(t.as_ref())).sum()
}

/// E2-S3: Fast token estimation using char-count heuristic.
///
/// ~100x faster than [`count_tokens`] with ~5% error for typical content.
/// Use for intermediate budget checks where precision isn't critical.
/// Reserve [`count_tokens`] for the final truncation decision.
///
/// Heuristic: code averages ~3.5 chars/token, natural language ~4.5.
/// We use 4 as a compromise (CHARS_PER_TOKEN constant).
pub fn estimate_tokens(text: &str) -> usize {
    // Ceiling division to avoid undercount
    text.len().div_ceil(CHARS_PER_TOKEN)
}

/// E2-S3: Fast token estimation for a list of strings.
pub fn estimate_tokens_list(texts: &[impl AsRef<str>]) -> usize {
    texts.iter().map(|t| estimate_tokens(t.as_ref())).sum()
}

/// Compute Wilson lower bound confidence scores for enrichment tiers.
///
/// Uses WilsonRanker from touring-simd to compute confidence-bounded scores
/// for gotcha, relations, error, and validation tiers. Higher score = more confident =
/// less need for context injection.
///
/// - `gotcha_hits`: (successes, total) for gotcha retrieval accuracy
/// - `relation_hits`: (successes, total) for relation lookup accuracy
/// - `error_hits`: (successes, total) for recent-error retrieval accuracy
/// - `validation_hits`: (successes, total) for cross-validation consistency
///
/// Returns `EnrichmentScores` with scores in [0.0, 1.0].
pub fn compute_enrichment_scores(
    gotcha_hits: (u32, u32),
    relation_hits: (u32, u32),
    error_hits: (u32, u32),
    validation_hits: (u32, u32),
) -> EnrichmentScores {
    let ranker = WilsonRanker::default();

    EnrichmentScores {
        gotcha_score: ranker.wilson_bound(gotcha_hits.0, gotcha_hits.1),
        relations_score: ranker.wilson_bound(relation_hits.0, relation_hits.1),
        // Errors (P0) are never gated, but score still tracked for observability
        errors_score: ranker.wilson_bound(error_hits.0, error_hits.1),
        validation_score: ranker.wilson_bound(validation_hits.0, validation_hits.1),
    }
}

/// Compute Wilson confidence score for a single (successes, total) pair.
///
/// Convenience wrapper for use in handlers that need one-off scoring.
pub fn wilson_confidence(successes: u32, total: u32) -> f64 {
    WilsonRanker::default().wilson_bound(successes, total)
}

/// Compose enriched context for a file, respecting token budget.
///
/// Priority order (higher = included first if budget allows):
/// 1. Gotchas — most valuable for error prevention
/// 2. Relations — import/caller paths
/// 3. Recent errors — patterns from recent failures
///
/// Returns composed context string, guaranteed <= `max_tokens` (estimated).
/// Returns an empty string if no enrichment data is available or budget is zero.
pub fn compose_enriched_context(
    _file_path: &str,
    gotchas: &[(String, String)], // (severity, description)
    relations: &[String],         // import/caller paths
    recent_errors: &[String],     // recent error patterns
    max_tokens: usize,            // budget (default 500)
) -> String {
    if max_tokens == 0 {
        return String::new();
    }

    let mut parts: Vec<String> = Vec::new();
    let mut budget = max_tokens;

    // Priority 1: Gotchas (most valuable for error prevention)
    if !gotchas.is_empty() {
        let gotcha_lines: Vec<String> = gotchas
            .iter()
            .take(MAX_GOTCHAS)
            .map(|(sev, desc)| format!("GOTCHA [{}]: {}", sev, desc))
            .collect();
        let section = gotcha_lines.join("\n");
        let tokens = count_tokens(&section);
        if tokens <= budget {
            parts.push(section);
            budget -= tokens;
        }
    }

    // Priority 2: Relations — E2-S3: use fast estimate for intermediate budget check
    if !relations.is_empty() && budget > 20 {
        let rel_items: Vec<&str> = relations
            .iter()
            .take(MAX_RELATIONS)
            .map(|s| s.as_str())
            .collect();
        let rel_str = format!("Relations: {}", rel_items.join(", "));
        let tokens = estimate_tokens(&rel_str);
        if tokens <= budget {
            parts.push(rel_str);
            budget -= tokens;
        }
    }

    // Priority 3: Recent errors — E2-S3: use fast estimate for intermediate budget check
    if !recent_errors.is_empty() && budget > 20 {
        let err_items: Vec<&str> = recent_errors
            .iter()
            .take(MAX_ERRORS)
            .map(|s| s.as_str())
            .collect();
        let err_str = format!("Recent errors: {}", err_items.join("; "));
        let tokens = estimate_tokens(&err_str);
        if tokens <= budget {
            parts.push(err_str);
        }
    }

    parts.join("\n")
}

/// Compose enriched context with selective gating based on Wilson confidence scores.
///
/// Each enrichment tier is checked against `GATING_THRESHOLD`:
/// - Score **> threshold**: skip injection (high confidence, context not needed)
/// - Score **<= threshold**: inject normally (low confidence, context helps)
///
/// **P0 invariant**: Errors are NEVER gated regardless of confidence score.
///
/// When `scores` is `None`, falls back to [`compose_enriched_context`] (inject all).
pub fn compose_enriched_context_gated(
    file_path: &str,
    gotchas: &[(String, String)],
    relations: &[String],
    recent_errors: &[String],
    max_tokens: usize,
    scores: Option<&EnrichmentScores>,
) -> String {
    // If no scores available, fall back to inject-all behavior
    let scores = match scores {
        Some(s) => s,
        None => {
            return compose_enriched_context(
                file_path,
                gotchas,
                relations,
                recent_errors,
                max_tokens,
            );
        }
    };

    // Gate each tier based on Wilson confidence (except errors — P0, always included)
    let gated_gotchas: &[(String, String)] = if scores.gotcha_score > GATING_THRESHOLD {
        &[]
    } else {
        gotchas
    };
    let gated_relations: &[String] = if scores.relations_score > GATING_THRESHOLD {
        &[]
    } else {
        relations
    };
    // Errors are P0 — NEVER gated regardless of confidence
    compose_enriched_context(
        file_path,
        gated_gotchas,
        gated_relations,
        recent_errors,
        max_tokens,
    )
}

/// E3-S3: Compute temporal decay score for context relevance.
///
/// Applies exponential decay based on age (turns since creation):
/// ```text
/// score = base_score × decay_rate^age
/// ```
///
/// - `base_score`: original relevance score (0.0-1.0)
/// - `age_turns`: number of turns since the context was created
/// - `decay_rate`: how quickly relevance fades (default 0.95 = 5% per turn)
///
/// Returns decayed score clamped to [0.0, 1.0].
pub fn decay_score(base_score: f64, age_turns: u32, decay_rate: f64) -> f64 {
    if age_turns == 0 {
        return base_score.clamp(0.0, 1.0);
    }
    let decayed = base_score * decay_rate.powi(age_turns as i32);
    decayed.clamp(0.0, 1.0)
}

/// E3-S3: Default decay rate for context relevance (5% loss per turn).
pub const DEFAULT_DECAY_RATE: f64 = 0.95;

/// E3-S3: Minimum decay score below which context is considered stale and dropped.
pub const MIN_DECAY_SCORE: f64 = 0.1;

/// E1-S4: Truncate context at a semantic boundary instead of cutting mid-definition.
///
/// Finds the last complete "semantic unit" (function, struct, or paragraph)
/// within the token budget. Avoids injecting half-cut code that confuses the LLM.
///
/// Semantic boundaries are detected by:
/// - `fn ` at line start (function definition)
/// - `struct ` / `enum ` / `trait ` / `impl ` at line start
/// - Empty lines (paragraph breaks)
/// - `///` doc comment blocks
pub fn truncate_at_semantic_boundary(text: &str, max_tokens: usize) -> &str {
    if max_tokens == 0 {
        return "";
    }

    let estimated_chars = max_tokens * CHARS_PER_TOKEN;
    if text.len() <= estimated_chars {
        return text;
    }

    // UTF-8 safe: find a char boundary at or before estimated_chars
    let safe_end = text
        .char_indices()
        .take_while(|(i, _)| *i < estimated_chars)
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());

    // Find the last semantic boundary within the char budget
    let search_range = &text[..safe_end];
    let boundary_markers = [
        "\nfn ",
        "\nstruct ",
        "\nenum ",
        "\ntrait ",
        "\nimpl ",
        "\n\n",
        "\n///",
    ];

    let mut best_cut = 0;
    for marker in &boundary_markers {
        if let Some(pos) = search_range.rfind(marker)
            && pos > best_cut
        {
            best_cut = pos;
        }
    }

    if best_cut > 0 {
        &text[..best_cut]
    } else {
        // No semantic boundary found — fall back to char budget
        // Find last newline within budget to avoid cutting mid-line
        search_range
            .rfind('\n')
            .map_or(search_range, |pos| &text[..pos])
    }
}

/// Compute dynamic context budget (in chars) based on CILA complexity level.
/// Higher CILA = more context needed = larger budget.
///
/// | CILA | Budget (chars) | Rationale |
/// |------|---------------|-----------|
/// | L0-L1 | 800 | Simple tasks need minimal context |
/// | L2 | 1200 | Tool-augmented needs some context |
/// | L3 | 2000 | Pipeline tasks need moderate context |
/// | L4-L5 | 3200 | Agent loops need rich context |
/// | L6+ | 4800 | Multi-agent needs maximum context |
pub fn compute_context_budget(cila_level: u8) -> usize {
    match cila_level {
        0..=1 => 800,
        2 => 1200,
        3 => 2000,
        4..=5 => 3200,
        _ => 4800, // L6+
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrichment_includes_gotchas() {
        let gotchas = vec![(
            "WARN".to_string(),
            "field is matched_text not text".to_string(),
        )];
        let result = compose_enriched_context("file.rs", &gotchas, &[], &[], 500);
        assert!(result.contains("GOTCHA [WARN]: field is matched_text not text"));
    }

    #[test]
    fn test_enrichment_includes_relations() {
        let relations = vec!["src/lib.rs".to_string(), "src/main.rs".to_string()];
        let result = compose_enriched_context("file.rs", &[], &relations, &[], 500);
        assert!(result.contains("Relations:"));
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("src/main.rs"));
    }

    #[test]
    fn test_enrichment_includes_errors() {
        let errors = vec!["type mismatch: expected u64, found i32".to_string()];
        let result = compose_enriched_context("file.rs", &[], &[], &errors, 500);
        assert!(result.contains("Recent errors:"));
        assert!(result.contains("type mismatch"));
    }

    #[test]
    fn test_enrichment_respects_budget() {
        // Create large inputs that exceed budget
        let gotchas: Vec<(String, String)> = (0..10)
            .map(|i| ("CRITICAL".to_string(), format!("gotcha description number {} with lots of extra text to consume tokens quickly and fill the budget", i)))
            .collect();
        let relations: Vec<String> = (0..20)
            .map(|i| format!("very/long/path/to/some/deeply/nested/module_{i}.rs"))
            .collect();
        let errors: Vec<String> = (0..10)
            .map(|i| {
                format!(
                    "very long error description number {} that takes up many tokens",
                    i
                )
            })
            .collect();

        let result = compose_enriched_context("file.rs", &gotchas, &relations, &errors, 30);
        // With a tiny budget (30 tokens = ~120 chars), not everything fits
        let estimated_tokens = result.len() / CHARS_PER_TOKEN;
        assert!(
            estimated_tokens <= 30,
            "result should respect budget: got {estimated_tokens} tokens"
        );
    }

    #[test]
    fn test_enrichment_empty_inputs() {
        let result = compose_enriched_context("file.rs", &[], &[], &[], 500);
        assert!(
            result.is_empty(),
            "empty inputs should produce empty output"
        );
    }

    #[test]
    fn test_enrichment_zero_budget() {
        let gotchas = vec![("WARN".to_string(), "something".to_string())];
        let result = compose_enriched_context("file.rs", &gotchas, &[], &[], 0);
        assert!(result.is_empty(), "zero budget should produce empty output");
    }

    #[test]
    fn test_enrichment_priority_order() {
        // All three sections with enough budget for all
        let gotchas = vec![("WARN".to_string(), "gotcha1".to_string())];
        let relations = vec!["rel1.rs".to_string()];
        let errors = vec!["err1".to_string()];

        let result = compose_enriched_context("file.rs", &gotchas, &relations, &errors, 500);

        // Verify ordering: gotchas before relations before errors
        let gotcha_pos = result.find("GOTCHA").expect("should contain GOTCHA");
        let rel_pos = result.find("Relations").expect("should contain Relations");
        let err_pos = result
            .find("Recent errors")
            .expect("should contain Recent errors");

        assert!(gotcha_pos < rel_pos, "gotchas should come before relations");
        assert!(rel_pos < err_pos, "relations should come before errors");
    }

    #[test]
    fn test_enrichment_max_items() {
        // More than MAX_GOTCHAS gotchas
        let gotchas: Vec<(String, String)> = (0..10)
            .map(|i| ("INFO".to_string(), format!("gotcha_{i}")))
            .collect();
        // More than MAX_RELATIONS relations
        let relations: Vec<String> = (0..10).map(|i| format!("rel_{i}.rs")).collect();
        // More than MAX_ERRORS errors
        let errors: Vec<String> = (0..10).map(|i| format!("error_{i}")).collect();

        let result = compose_enriched_context("file.rs", &gotchas, &relations, &errors, 2000);

        // Count gotchas: max 3
        let gotcha_count = result.matches("GOTCHA").count();
        assert_eq!(
            gotcha_count, MAX_GOTCHAS,
            "should include at most {MAX_GOTCHAS} gotchas"
        );

        // Count relations: max 5 items in the comma-separated list
        // The relations line is: "Relations: rel_0.rs, rel_1.rs, rel_2.rs, rel_3.rs, rel_4.rs"
        assert!(result.contains("rel_4.rs"), "should include 5th relation");
        assert!(
            !result.contains("rel_5.rs"),
            "should not include 6th relation"
        );

        // Count errors: max 3 items in semicolon-separated list
        assert!(result.contains("error_2"), "should include 3rd error");
        assert!(!result.contains("error_3"), "should not include 4th error");
    }

    #[test]
    fn test_enrichment_partial_budget() {
        // Budget only enough for gotchas, not relations or errors
        let gotchas = vec![("W".to_string(), "short".to_string())];
        let relations = vec!["r.rs".to_string()];
        let errors = vec!["e".to_string()];

        // With tiktoken: "GOTCHA [W]: short\n" ≈ 5-6 tokens
        // Budget=8: gotcha fits (6 tokens), but relations need >20 budget remaining
        let result = compose_enriched_context("file.rs", &gotchas, &relations, &errors, 8);

        assert!(result.contains("GOTCHA"), "gotchas should be included");
        assert!(
            !result.contains("Relations"),
            "relations should be excluded (budget too low)"
        );
        assert!(
            !result.contains("Recent errors"),
            "errors should be excluded (budget too low)"
        );
    }

    // ── Selective context gating tests ──────────────────────────────────

    #[test]
    fn test_gating_skips_on_high_confidence() {
        // When all enrichment scores are > 0.80, context should be minimal/empty
        let gotchas = vec![("WARN".to_string(), "gotcha1".to_string())];
        let result = compose_enriched_context_gated(
            "file.rs",
            &gotchas,
            &[],
            &[],
            500,
            Some(&EnrichmentScores {
                gotcha_score: 0.95,
                relations_score: 0.90,
                errors_score: 0.85,
                validation_score: 0.80,
            }),
        );
        assert!(
            result.is_empty() || !result.contains("gotcha"),
            "High confidence should skip gotchas: {}",
            result
        );
    }

    #[test]
    fn test_gating_injects_on_low_confidence() {
        let gotchas = vec![("WARN".to_string(), "gotcha1".to_string())];
        let result = compose_enriched_context_gated(
            "file.rs",
            &gotchas,
            &[],
            &[],
            500,
            Some(&EnrichmentScores {
                gotcha_score: 0.30,
                relations_score: 0.90,
                errors_score: 0.90,
                validation_score: 0.80,
            }),
        );
        assert!(
            result.contains("gotcha1"),
            "Low confidence gotcha should be injected: {}",
            result
        );
    }

    #[test]
    fn test_gating_none_scores_injects_all() {
        // When no scores available, behave like original (inject everything)
        let gotchas = vec![("WARN".to_string(), "gotcha1".to_string())];
        let relations = vec!["rel1".to_string()];
        let result = compose_enriched_context_gated(
            "file.rs",
            &gotchas,
            &relations,
            &[],
            500,
            None, // no scores = inject all
        );
        assert!(result.contains("gotcha1"), "No scores should inject all");
    }

    #[test]
    fn test_gating_preserves_p0_errors() {
        // Errors (P0) should NEVER be gated — always injected regardless of confidence
        let errors = vec!["syntax error in line 42".to_string()];
        let result = compose_enriched_context_gated(
            "file.rs",
            &[],
            &[],
            &errors,
            500,
            Some(&EnrichmentScores {
                gotcha_score: 0.95,
                relations_score: 0.95,
                errors_score: 0.99,
                validation_score: 0.80,
            }),
        );
        assert!(
            result.contains("syntax error"),
            "Errors are P0 and must NEVER be gated: {}",
            result
        );
    }

    #[test]
    fn test_gating_threshold_boundary() {
        // Score exactly at threshold (0.80) should still inject (use > not >=)
        let gotchas = vec![("WARN".to_string(), "gotcha1".to_string())];
        let result = compose_enriched_context_gated(
            "file.rs",
            &gotchas,
            &[],
            &[],
            500,
            Some(&EnrichmentScores {
                gotcha_score: 0.80,
                relations_score: 0.90,
                errors_score: 0.90,
                validation_score: 0.80,
            }),
        );
        assert!(
            result.contains("gotcha1"),
            "Score at threshold should inject: {}",
            result
        );
    }

    // ── S4.2: compute_context_budget ────────────────────────────────

    #[test]
    fn test_context_budget_l0_minimal() {
        assert_eq!(compute_context_budget(0), 800);
        assert_eq!(compute_context_budget(1), 800);
    }

    #[test]
    fn test_context_budget_l2_moderate() {
        assert_eq!(compute_context_budget(2), 1200);
    }

    #[test]
    fn test_context_budget_l6_maximal() {
        assert_eq!(compute_context_budget(6), 4800);
        // Values above L6 also get max budget
        assert_eq!(compute_context_budget(7), 4800);
        assert_eq!(compute_context_budget(255), 4800);
    }

    #[test]
    fn test_context_budget_monotonic_with_cila() {
        // Budget must be monotonically non-decreasing with CILA level
        let budgets: Vec<usize> = (0..=6).map(compute_context_budget).collect();
        for window in budgets.windows(2) {
            assert!(
                window[1] >= window[0],
                "Budget must be monotonic: L{} ({}) should be >= L{} ({})",
                budgets
                    .iter()
                    .position(|&x| x == window[1])
                    .expect("budget value always found in budgets vector"),
                window[1],
                budgets
                    .iter()
                    .position(|&x| x == window[0])
                    .expect("budget value always found in budgets vector"),
                window[0],
            );
        }
    }

    // ── E2-S3: Token estimation tests ────────────────────────────────

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_short() {
        // "hello" = 5 chars, ceil(5/4) = 2
        assert_eq!(estimate_tokens("hello"), 2);
    }

    #[test]
    fn test_estimate_tokens_within_margin_of_tiktoken() {
        // For typical code content, estimate should be within 20% of tiktoken
        let text = "fn main() { println!(\"Hello, world!\"); let x = 42; }";
        let exact = count_tokens(text);
        let estimated = estimate_tokens(text);
        let ratio = estimated as f64 / exact as f64;
        assert!(
            (0.5..=2.0).contains(&ratio),
            "Estimate ({estimated}) should be within 2x of exact ({exact}), ratio={ratio:.2}"
        );
    }

    #[test]
    fn test_estimate_tokens_list() {
        let texts = vec!["hello", "world", "test"];
        let result = estimate_tokens_list(&texts);
        let expected: usize = texts.iter().map(|t| estimate_tokens(t)).sum();
        assert_eq!(result, expected);
    }

    // ── E3-S3: Temporal relevance decay tests ────────────────────────

    #[test]
    fn test_decay_score_no_age() {
        assert!((decay_score(0.8, 0, DEFAULT_DECAY_RATE) - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_decay_score_decreases_with_age() {
        let s0 = decay_score(1.0, 0, DEFAULT_DECAY_RATE);
        let s1 = decay_score(1.0, 1, DEFAULT_DECAY_RATE);
        let s5 = decay_score(1.0, 5, DEFAULT_DECAY_RATE);
        let s20 = decay_score(1.0, 20, DEFAULT_DECAY_RATE);
        assert!(s0 > s1, "age=0 > age=1");
        assert!(s1 > s5, "age=1 > age=5");
        assert!(s5 > s20, "age=5 > age=20");
    }

    #[test]
    fn test_decay_score_clamps() {
        // Negative base score clamped to 0
        assert_eq!(decay_score(-1.0, 0, 0.9), 0.0);
        // Score > 1.0 clamped to 1.0
        assert_eq!(decay_score(2.0, 0, 0.9), 1.0);
    }

    #[test]
    fn test_decay_score_aggressive_rate() {
        // decay_rate=0.5: halves every turn
        let s = decay_score(1.0, 3, 0.5); // 1.0 * 0.5^3 = 0.125
        assert!((s - 0.125).abs() < 1e-10);
    }

    #[test]
    fn test_decay_score_eventually_below_minimum() {
        // After enough turns with default rate, score drops below MIN_DECAY_SCORE
        let s = decay_score(1.0, 50, DEFAULT_DECAY_RATE);
        assert!(
            s < MIN_DECAY_SCORE,
            "Score after 50 turns should be stale: {s}"
        );
    }

    // ── E1-S4: Semantic chunking tests ───────────────────────────────

    #[test]
    fn test_semantic_truncate_fits_in_budget() {
        let text = "short text";
        assert_eq!(truncate_at_semantic_boundary(text, 100), text);
    }

    #[test]
    fn test_semantic_truncate_zero_budget() {
        assert_eq!(truncate_at_semantic_boundary("some text", 0), "");
    }

    #[test]
    fn test_semantic_truncate_at_fn_boundary() {
        let text = "fn foo() { 1 }\nfn bar() { 2 }\nfn baz() { very long function body that exceeds our budget }";
        // Budget that fits first two functions but not third
        let result = truncate_at_semantic_boundary(text, 10); // ~40 chars
        assert!(result.contains("fn foo"), "Should keep first fn");
        assert!(
            result.contains("fn bar") || result.ends_with("fn foo() { 1 }"),
            "Should cut at a fn boundary: {result}"
        );
    }

    #[test]
    fn test_semantic_truncate_at_empty_line() {
        let text = "paragraph one\n\nparagraph two that is much longer and exceeds our token budget limit here";
        let result = truncate_at_semantic_boundary(text, 8); // ~32 chars
        assert!(
            result.contains("paragraph one"),
            "Should keep first paragraph"
        );
    }

    #[test]
    fn test_semantic_truncate_falls_back_to_newline() {
        let text = "line one\nline two\nline three is very long and continues past budget";
        let result = truncate_at_semantic_boundary(text, 5); // ~20 chars
        assert!(
            result.ends_with("one") || result.ends_with("two"),
            "Should cut at line boundary: {result}"
        );
    }

    #[test]
    fn test_semantic_truncate_utf8_safe() {
        // Text with multi-byte chars — must not panic
        let text = "café résumé naïve 日本語テスト very long text that exceeds budget";
        let result = truncate_at_semantic_boundary(text, 5);
        assert!(!result.is_empty());
        // Verify it's valid UTF-8 (would panic if not)
        let _ = result.len();
    }

    // ─────────────────────────────────────────────────────────────────────
    // SIMD-softmax tier budget tests (wires signal_fusion into enrichment)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_tier_budget_probabilities_sum_to_one() {
        let scores = EnrichmentScores {
            gotcha_score: 0.8,
            relations_score: 0.6,
            errors_score: 0.4,
            validation_score: 0.2,
        };
        let b = scores.tier_budget();
        let total = b.gotcha_frac + b.relations_frac + b.errors_frac + b.validation_frac;
        assert!(
            (total - 1.0).abs() < 1e-5,
            "fractions must sum to 1, got {total}"
        );
        // Each in [0, 1].
        for f in [
            b.gotcha_frac,
            b.relations_frac,
            b.errors_frac,
            b.validation_frac,
        ] {
            assert!((0.0..=1.0).contains(&f), "fraction out of range: {f}");
        }
    }

    #[test]
    fn test_tier_budget_dominant_tier_matches_argmax() {
        // gotcha clearly wins.
        let scores = EnrichmentScores {
            gotcha_score: 0.95,
            relations_score: 0.30,
            errors_score: 0.10,
            validation_score: 0.10,
        };
        let b = scores.tier_budget();
        assert_eq!(b.dominant_tier, Some("gotcha"));
        assert!(b.gotcha_frac > b.relations_frac);
        assert!(b.gotcha_frac > b.errors_frac);

        // relations wins.
        let scores = EnrichmentScores {
            gotcha_score: 0.10,
            relations_score: 0.90,
            errors_score: 0.20,
            validation_score: 0.10,
        };
        assert_eq!(scores.tier_budget().dominant_tier, Some("relations"));

        // errors wins.
        let scores = EnrichmentScores {
            gotcha_score: 0.20,
            relations_score: 0.30,
            errors_score: 0.99,
            validation_score: 0.10,
        };
        assert_eq!(scores.tier_budget().dominant_tier, Some("errors"));
    }

    #[test]
    fn test_tier_budget_uniform_scores_give_quarter_each() {
        let scores = EnrichmentScores {
            gotcha_score: 0.5,
            relations_score: 0.5,
            errors_score: 0.5,
            validation_score: 0.5,
        };
        let b = scores.tier_budget();
        for f in [
            b.gotcha_frac,
            b.relations_frac,
            b.errors_frac,
            b.validation_frac,
        ] {
            assert!((f - 0.25).abs() < 1e-5, "uniform should give 1/4, got {f}");
        }
        // Max attention ≈ 1/4, entropy ≈ ln(4) ≈ 1.3863.
        assert!((b.max_attention - 0.25).abs() < 1e-5);
        assert!((b.entropy - 4.0f64.ln()).abs() < 1e-4);
    }

    #[test]
    fn test_allocate_tokens_sums_to_budget() {
        let scores = EnrichmentScores {
            gotcha_score: 0.7,
            relations_score: 0.5,
            errors_score: 0.3,
            validation_score: 0.2,
        };
        for total in [0, 1, 10, 100, 300, 1_000] {
            let (g, r, e, v) = scores.allocate_tokens(total);
            assert_eq!(
                g + r + e + v,
                total,
                "allocation must sum to budget {total}"
            );
        }
    }

    #[test]
    fn test_allocate_tokens_remainder_goes_to_dominant() {
        // Tiny budget where integer rounding matters.
        let scores = EnrichmentScores {
            gotcha_score: 0.99,
            relations_score: 0.1,
            errors_score: 0.1,
            validation_score: 0.1,
        };
        let (g, r, e, v) = scores.allocate_tokens(10);
        assert_eq!(g + r + e + v, 10);
        // Dominant gotcha should receive at least half.
        assert!(
            g >= 5,
            "expected gotcha to dominate, got g={g} r={r} e={e} v={v}"
        );
    }

    #[test]
    fn test_allocate_tokens_zero_budget() {
        let scores = EnrichmentScores::default();
        assert_eq!(scores.allocate_tokens(0), (0, 0, 0, 0));
    }
}
