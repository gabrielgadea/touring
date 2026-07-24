//! Enhanced confidence scoring utilities for argument mining.
//!
//! Provides v2 confidence computation with position-based, qualifier,
//! modal verb, and length adjustments.

use regex::Regex;
use std::sync::LazyLock;

/// Position of a match within the sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionInSentence {
    /// Start of sentence (first 25%) — +0.05 boost
    Start,
    /// Middle of sentence (25-75%) — no boost
    Middle,
    /// End of sentence (last 25%) — +0.03 boost
    End,
}

impl PositionInSentence {
    /// Determines position from match offset and text length.
    pub fn from_position(offset: usize, text_len: usize) -> Self {
        if text_len == 0 {
            return PositionInSentence::Middle;
        }
        let ratio = offset as f32 / text_len as f32;
        if ratio < 0.25 {
            PositionInSentence::Start
        } else if ratio > 0.75 {
            PositionInSentence::End
        } else {
            PositionInSentence::Middle
        }
    }

    /// Returns the confidence adjustment for this position.
    pub fn adjustment(&self) -> f32 {
        match self {
            PositionInSentence::Start => 0.05,
            PositionInSentence::Middle => 0.0,
            PositionInSentence::End => 0.03,
        }
    }
}

/// Qualifier pattern classification for hedging/strengthening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualifierPattern {
    /// Definitely, clearly, obviously, absolutely, certainly, etc.
    Definitely,
    /// Possibly, probably, maybe, perhaps, etc.
    Hedged,
    /// Must, shall — strong modal commitment
    Certain,
}

impl QualifierPattern {
    /// Classifies a matched qualifier string into a pattern.
    ///
    /// Returns `None` if the string does not match any known qualifier.
    /// Classifies a matched qualifier string into a pattern.
    ///
    /// Returns `None` if the string does not match any known qualifier.
    /// (Named `classify` rather than `from_str` to avoid confusion with
    /// the standard `FromStr` trait which requires a `Result` return type.)
    pub fn classify(matched: &str) -> Option<Self> {
        let lower = matched.to_lowercase();
        if Self::DEFINITELY_WORDS.contains(&lower.as_str()) {
            Some(QualifierPattern::Definitely)
        } else if Self::HEDGED_WORDS.contains(&lower.as_str()) {
            Some(QualifierPattern::Hedged)
        } else if Self::CERTAIN_WORDS.contains(&lower.as_str()) {
            Some(QualifierPattern::Certain)
        } else {
            None
        }
    }

    /// Returns the confidence adjustment for this qualifier pattern.
    pub fn adjustment(&self) -> f32 {
        match self {
            QualifierPattern::Definitely => 0.05,
            QualifierPattern::Hedged => -0.05,
            QualifierPattern::Certain => 0.05,
        }
    }

    /// Definitely/strengthening qualifier words (EN + PT-BR)
    const DEFINITELY_WORDS: &'static [&'static str] = &[
        "definitely",
        "clearly",
        "obviously",
        "absolutely",
        "certainly",
        "certamente",
        "definitivamente",
    ];

    /// Hedged/weakening qualifier words (EN + PT-BR)
    const HEDGED_WORDS: &'static [&'static str] = &[
        "possibly",
        "probably",
        "maybe",
        "perhaps",
        "provavelmente",
        "talvez",
        "possivelmente",
    ];

    /// Certain modal verbs
    const CERTAIN_WORDS: &'static [&'static str] = &["must", "shall"];
}

// Pre-compiled regex patterns for qualifier and modal detection
static QUALIFIER_BOOST_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(definitely|clearly|obviously|absolutely|certainly|certamente|definitivamente)\b",
    )
    .expect("invalid qualifier boost regex")
});

static QUALIFIER_PENALTY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(possibly|probably|maybe|perhaps|provavelmente|talvez|possivelmente)\b")
        .expect("invalid qualifier penalty regex")
});

static MODAL_MUST_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bmust\b").expect("invalid modal must regex"));

static MODAL_SHOULD_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bshould\b").expect("invalid modal should regex"));

static MODAL_MAY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bmay\b").expect("invalid modal may regex"));

static MODAL_MIGHT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bmight\b").expect("invalid modal might regex"));

/// Computes the qualifier pattern from a matched string.
///
/// Returns `QualifierPattern::Definitely` for strengthening qualifiers,
/// `QualifierPattern::Hedged` for weakening ones, and
/// `QualifierPattern::Certain` for strong modal verbs like "must".
///
/// # Examples
///
/// ```
/// use touring_offensive::erickson::compute_qualifier;
/// use touring_offensive::erickson::confidence::QualifierPattern;
///
/// assert_eq!(compute_qualifier("definitely"), Some(QualifierPattern::Definitely));
/// assert_eq!(compute_qualifier("probably"), Some(QualifierPattern::Hedged));
/// assert_eq!(compute_qualifier("must"), Some(QualifierPattern::Certain));
/// assert_eq!(compute_qualifier("unknown"), None);
/// ```
pub fn compute_qualifier(matched: &str) -> Option<QualifierPattern> {
    QualifierPattern::classify(matched)
}

/// Computes an enhanced v2 confidence score with multiple adjustment factors.
///
/// # Adjustments
///
/// - **Position**: Start (+0.05), End (+0.03), Middle (0)
/// - **Qualifier boost**: definitely, clearly, obviously, absolutely (+0.05)
/// - **Qualifier penalty**: possibly, probably, maybe, perhaps (-0.05)
/// - **Modal verb**: must (+0.05), should (+0.03), may (0), might (-0.02)
/// - **Length**: 5-15 chars (+0.03), 20+ chars (-0.02)
///
/// # Arguments
///
/// * `base` — Base confidence score (e.g., 0.85 for claims)
/// * `text` — Full text being analyzed
/// * `matched` — The matched marker/pattern text
/// * `offset` — Character offset of the match in the text
///
/// # Example
///
/// ```
/// use touring_offensive::erickson::confidence::compute_confidence_v2;
///
/// // Claim at start with "definitely" qualifier and "must" modal
/// let confidence = compute_confidence_v2(0.85, "Definitely, we must act now", "must", 14);
/// assert!(confidence > 0.85); // adjustments applied
/// ```
pub fn compute_confidence_v2(base: f32, text: &str, matched: &str, offset: usize) -> f32 {
    let mut adjustment = 0.0_f32;

    // 1. Position-based adjustment
    let position = PositionInSentence::from_position(offset, text.len());
    adjustment += position.adjustment();

    // 2. Qualifier detection in surrounding context (±15 bytes, snapped to char boundaries)
    let raw_start = offset.saturating_sub(15);
    let context_start = {
        let mut s = raw_start;
        while s < text.len() && !text.is_char_boundary(s) {
            s += 1;
        }
        s
    };
    let raw_end = (offset + matched.len() + 15).min(text.len());
    let context_end = {
        let mut e = raw_end;
        while e > context_start && !text.is_char_boundary(e) {
            e -= 1;
        }
        e
    };
    let context = &text[context_start..context_end];

    if QUALIFIER_BOOST_REGEX.is_match(context) {
        adjustment += 0.05;
    }
    if QUALIFIER_PENALTY_REGEX.is_match(context) {
        adjustment -= 0.05;
    }

    // 3. Modal verb detection in context (ordered by strength: must > should > may > might)
    if MODAL_MUST_REGEX.is_match(context) {
        adjustment += 0.05;
    } else if MODAL_SHOULD_REGEX.is_match(context) {
        adjustment += 0.03;
    } else if MODAL_MAY_REGEX.is_match(context) {
        // "may" (modal of permission/possibility) — no confidence adjustment
    } else if MODAL_MIGHT_REGEX.is_match(context) {
        adjustment -= 0.02;
    }

    // 4. Length adjustment for matched text
    let matched_len = matched.len();
    if (5..=15).contains(&matched_len) {
        adjustment += 0.03;
    } else if matched_len >= 20 {
        adjustment -= 0.02;
    }

    (base + adjustment).clamp(0.0_f32, 1.0_f32)
}

/// Base confidence scores for each pattern type.
#[derive(Debug, Clone, Copy)]
pub struct BaseConfidence {
    /// Claim base confidence
    pub claim: f32,
    /// Evidence base confidence
    pub evidence: f32,
    /// Warrant base confidence
    pub warrant: f32,
    /// Rebuttal base confidence
    pub rebuttal: f32,
    /// Backing base confidence
    pub backing: f32,
}

impl Default for BaseConfidence {
    fn default() -> Self {
        Self {
            claim: 0.85,
            evidence: 0.80,
            warrant: 0.75,
            rebuttal: 0.70,
            backing: 0.70,
        }
    }
}

/// Context boost factors for confidence adjustment.
#[derive(Debug, Clone, Copy)]
pub struct ContextBoost {
    /// Boost for support indicators nearby
    pub support: f32,
    /// Boost for attack indicators nearby
    pub attack: f32,
    /// Boost for negation
    pub negation: f32,
}

impl Default for ContextBoost {
    fn default() -> Self {
        Self {
            support: 0.10,
            attack: 0.05,
            negation: -0.05,
        }
    }
}

/// Computes final confidence with context boost clamped to [0.0, 1.0].
pub fn compute_with_boost(base: f32, boost: f32) -> f32 {
    (base + boost).clamp(0.0_f32, 1.0_f32)
}

/// Applies length penalty for very short or very long matches.
pub fn length_penalty(confidence: f32, text_len: usize) -> f32 {
    let penalty = if text_len < 3 {
        -0.1 // Too short — penalize
    } else if text_len > 200 {
        -0.05 // Very long — slight penalty
    } else {
        0.0
    };
    (confidence + penalty).clamp(0.0_f32, 1.0_f32)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // PositionInSentence tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_position_start() {
        // offset 0 of 40 chars = 0% = Start
        let pos = PositionInSentence::from_position(0, 40);
        assert_eq!(pos, PositionInSentence::Start);
        assert_eq!(pos.adjustment(), 0.05);
    }

    #[test]
    fn test_position_start_boundary() {
        // offset 9 of 40 chars = 22.5% < 25% = Start
        let pos = PositionInSentence::from_position(9, 40);
        assert_eq!(pos, PositionInSentence::Start);
    }

    #[test]
    fn test_position_middle() {
        // offset 15 of 40 chars = 37.5% = Middle
        let pos = PositionInSentence::from_position(15, 40);
        assert_eq!(pos, PositionInSentence::Middle);
        assert_eq!(pos.adjustment(), 0.0);
    }

    #[test]
    fn test_position_end_boundary() {
        // offset 31 of 40 chars = 77.5% > 75% = End
        let pos = PositionInSentence::from_position(31, 40);
        assert_eq!(pos, PositionInSentence::End);
        assert_eq!(pos.adjustment(), 0.03);
    }

    #[test]
    fn test_position_end() {
        // offset 39 of 40 chars = 97.5% = End
        let pos = PositionInSentence::from_position(39, 40);
        assert_eq!(pos, PositionInSentence::End);
    }

    #[test]
    fn test_position_empty_text() {
        let pos = PositionInSentence::from_position(0, 0);
        assert_eq!(pos, PositionInSentence::Middle);
    }

    // -------------------------------------------------------------------------
    // QualifierPattern tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_qualifier_definitely() {
        assert_eq!(
            compute_qualifier("definitely"),
            Some(QualifierPattern::Definitely)
        );
        assert_eq!(
            compute_qualifier("clearly"),
            Some(QualifierPattern::Definitely)
        );
        assert_eq!(
            compute_qualifier("obviously"),
            Some(QualifierPattern::Definitely)
        );
        assert_eq!(
            compute_qualifier("absolutely"),
            Some(QualifierPattern::Definitely)
        );
        assert_eq!(
            compute_qualifier("certainly"),
            Some(QualifierPattern::Definitely)
        );
        assert_eq!(
            compute_qualifier("certamente"),
            Some(QualifierPattern::Definitely)
        );
        assert_eq!(
            compute_qualifier("definitivamente"),
            Some(QualifierPattern::Definitely)
        );
        assert_eq!(
            compute_qualifier("DEFINITELY"),
            Some(QualifierPattern::Definitely)
        ); // case insensitive
    }

    #[test]
    fn test_qualifier_hedged() {
        assert_eq!(
            compute_qualifier("possibly"),
            Some(QualifierPattern::Hedged)
        );
        assert_eq!(
            compute_qualifier("probably"),
            Some(QualifierPattern::Hedged)
        );
        assert_eq!(compute_qualifier("maybe"), Some(QualifierPattern::Hedged));
        assert_eq!(compute_qualifier("perhaps"), Some(QualifierPattern::Hedged));
        assert_eq!(
            compute_qualifier("provavelmente"),
            Some(QualifierPattern::Hedged)
        );
        assert_eq!(compute_qualifier("talvez"), Some(QualifierPattern::Hedged));
        assert_eq!(
            compute_qualifier("possivelmente"),
            Some(QualifierPattern::Hedged)
        );
    }

    #[test]
    fn test_qualifier_certain() {
        assert_eq!(compute_qualifier("must"), Some(QualifierPattern::Certain));
        assert_eq!(compute_qualifier("shall"), Some(QualifierPattern::Certain));
    }

    #[test]
    fn test_qualifier_none() {
        assert_eq!(compute_qualifier("unknown"), None);
        assert_eq!(compute_qualifier(""), None);
        assert_eq!(compute_qualifier("randomword"), None);
    }

    #[test]
    fn test_qualifier_adjustment() {
        assert_eq!(QualifierPattern::Definitely.adjustment(), 0.05);
        assert_eq!(QualifierPattern::Hedged.adjustment(), -0.05);
        assert_eq!(QualifierPattern::Certain.adjustment(), 0.05);
    }

    // -------------------------------------------------------------------------
    // compute_confidence_v2 tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_confidence_v2_base_only() {
        // offset=3, len=17, ratio=0.176 → Start (+0.05); "should" modal (+0.03); length 6∈[5,15] (+0.03)
        let text = "We should upgrade";
        let conf = compute_confidence_v2(0.85, text, "should", 3);
        // Base 0.85 + Start (+0.05) + should (+0.03) + length (+0.03) = 0.96
        assert!((conf - 0.96).abs() < 0.001, "expected 0.96, got {conf}");
    }

    #[test]
    fn test_confidence_v2_position_start() {
        // offset=0, len=18, ratio=0.0 → Start (+0.05); "Should" modal (+0.03); length 6∈[5,15] (+0.03)
        let text = "Should we upgrade?";
        let conf = compute_confidence_v2(0.85, text, "Should", 0);
        // Base 0.85 + Start (+0.05) + should (+0.03) + length (+0.03) = 0.96
        assert!((conf - 0.96).abs() < 0.001, "expected 0.96, got {conf}");
    }

    #[test]
    fn test_confidence_v2_position_end() {
        // offset=13, len=17, ratio=0.765 → End (+0.03); "should" modal (+0.03); length 6∈[5,15] (+0.03)
        let text = "We upgrade should";
        let conf = compute_confidence_v2(0.85, text, "should", 13);
        // Base 0.85 + End (+0.03) + should (+0.03) + length (+0.03) = 0.94
        assert!((conf - 0.94).abs() < 0.001, "expected 0.94, got {conf}");
    }

    #[test]
    fn test_confidence_v2_qualifier_boost() {
        // definitely qualifier (+0.05)
        let text = "We definitely should upgrade";
        let conf = compute_confidence_v2(0.85, text, "should", 14);
        // Base 0.85 + Middle (0) + definitely (+0.05) + should (+0.03) + length (+0.03) = 0.96
        assert!((conf - 0.96).abs() < 0.001);
    }

    #[test]
    fn test_confidence_v2_qualifier_penalty() {
        // probably qualifier (-0.05)
        let text = "We probably should upgrade";
        let conf = compute_confidence_v2(0.85, text, "should", 16);
        // Base 0.85 + Middle (0) + probably (-0.05) + should (+0.03) + length (+0.03) = 0.86
        assert!((conf - 0.86).abs() < 0.001);
    }

    #[test]
    fn test_confidence_v2_must_modal() {
        // offset=3, len=15, ratio=0.2 → Start (+0.05); "must" modal (+0.05); len=4 < 5 → no length bonus
        let text = "We must upgrade";
        let conf = compute_confidence_v2(0.85, text, "must", 3);
        // Base 0.85 + Start (+0.05) + must (+0.05) = 0.95
        assert!((conf - 0.95).abs() < 0.001, "expected 0.95, got {conf}");
    }

    #[test]
    fn test_confidence_v2_might_modal() {
        // offset=3, len=16, ratio=0.188 → Start (+0.05); "might" modal (-0.02); len=5∈[5,15] (+0.03)
        let text = "We might upgrade";
        let conf = compute_confidence_v2(0.85, text, "might", 3);
        // Base 0.85 + Start (+0.05) + might (-0.02) + length (+0.03) = 0.91
        assert!((conf - 0.91).abs() < 0.001, "expected 0.91, got {conf}");
    }

    #[test]
    fn test_confidence_v2_long_match_penalty() {
        // Match >= 20 chars gets -0.02
        let text = "This is some text with a verylongpattern indeed";
        let conf = compute_confidence_v2(0.85, text, "verylongpattern indeed", 24);
        // Base 0.85 + length 20+ (-0.02) = 0.83
        assert!((conf - 0.83).abs() < 0.001);
    }

    #[test]
    fn test_confidence_v2_clamp_upper() {
        // Sum > 1.0 should clamp to 1.0
        let text = "Definitely must upgrade";
        let conf = compute_confidence_v2(0.95, text, "must", 11);
        // Base 0.95 + Start (+0.05) + definitely (+0.05) + must (+0.05) = 1.10 -> clamped to 1.0
        assert_eq!(conf, 1.0);
    }

    #[test]
    fn test_confidence_v2_clamp_lower() {
        // Sum < 0.0 should clamp to 0.0
        let text = "This is some text that is very long with maybe might pattern";
        let conf = compute_confidence_v2(0.05, text, "might", 50);
        // Base 0.05 + End (+0.03) + maybe (-0.05) + might (-0.02) + length 20+ (-0.02) could go negative
        assert!(conf >= 0.0);
    }

    #[test]
    fn test_confidence_v2_combined_boosts() {
        // offset=11, len=23, ratio=0.478 → Middle (0.0); "Definitely" boost (+0.05); "must" modal (+0.05); len=4 < 5 → no length bonus
        let text = "Definitely must upgrade";
        let conf = compute_confidence_v2(0.80, text, "must", 11);
        // Base 0.80 + Middle (0.0) + definitely (+0.05) + must (+0.05) = 0.90
        assert!((conf - 0.90).abs() < 0.001, "expected 0.90, got {conf}");
    }

    #[test]
    fn test_confidence_v2_combined_penalties() {
        // offset=6, len=31, ratio=0.194 → Start (+0.05); "Maybe" penalty (-0.05); "might" modal (-0.02); len=5∈[5,15] (+0.03)
        let text = "Maybe might verylongpatternname";
        let conf = compute_confidence_v2(0.80, text, "might", 6);
        // Base 0.80 + Start (+0.05) + maybe (-0.05) + might (-0.02) + length (+0.03) = 0.81
        assert!((conf - 0.81).abs() < 0.001, "expected 0.81, got {conf}");
    }

    // -------------------------------------------------------------------------
    // BaseConfidence and ContextBoost tests (v1 compatibility)
    // -------------------------------------------------------------------------

    #[test]
    fn test_base_confidence_default() {
        let bc = BaseConfidence::default();
        assert_eq!(bc.claim, 0.85);
        assert_eq!(bc.evidence, 0.80);
    }

    #[test]
    fn test_compute_with_boost() {
        // f32 arithmetic: 0.8 + 0.1 ≠ exactly 0.9 in IEEE 754 — use approx equality
        let r = compute_with_boost(0.8, 0.1);
        assert!((r - 0.9).abs() < 0.001, "expected ~0.9, got {r}");
        assert_eq!(compute_with_boost(0.95, 0.1), 1.0); // clamped
        assert_eq!(compute_with_boost(0.1, -0.2), 0.0); // clamped
    }

    #[test]
    fn test_length_penalty() {
        let confidence = 0.8;
        assert!(length_penalty(confidence, 2) < confidence);
        assert_eq!(length_penalty(confidence, 50), confidence);
    }

    // -------------------------------------------------------------------------
    // PT-BR qualifier tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_qualifier_ptbr_boost() {
        assert_eq!(
            compute_qualifier("certamente"),
            Some(QualifierPattern::Definitely)
        );
        assert_eq!(
            compute_qualifier("definitivamente"),
            Some(QualifierPattern::Definitely)
        );
    }

    #[test]
    fn test_qualifier_ptbr_penalty() {
        assert_eq!(
            compute_qualifier("provavelmente"),
            Some(QualifierPattern::Hedged)
        );
        assert_eq!(compute_qualifier("talvez"), Some(QualifierPattern::Hedged));
        assert_eq!(
            compute_qualifier("possivelmente"),
            Some(QualifierPattern::Hedged)
        );
    }
}
