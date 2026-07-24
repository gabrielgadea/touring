//! Erickson NLP Module
//!
//! Argument mining using the Toulmin model (Stephen Edelston Toulmin).
//! Extracts Claims, Evidence, Warrant, Rebuttal, and Backing from text.
//!
//! # Toulmin Model
//!
//! - **Claim**: The conclusion being argued
//! - **Evidence**: Facts supporting the claim
//! - **Warrant**: The reasoning connecting evidence to claim
//! - **Rebuttal**: Counter-arguments or objections
//! - **Backing**: Additional support for the warrant
//!
//! # Example
//!
//! ```rust
//! use touring_offensive::erickson::{extract, NLPPattern, EricksonElement};
//!
//! let text = "We should upgrade serde because it has a critical CVE.";
//! let elements = extract(text);
//! assert!(elements.iter().any(|e| matches!(e.pattern, NLPPattern::Claim)));
//! ```

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use touring_foundation::feedback::{
    FeedbackPattern, FeedbackRelation, PatternFeedback, PatternFeedbackContext,
};

#[cfg(test)]
mod ptbr_markers;

pub mod confidence;
pub mod relation_population;
pub mod sentence_boundaries;

// ---------------------------------------------------------------------------
// Combined EN + ZH + PT-BR Marker LazyLocks
// ---------------------------------------------------------------------------

/// Combined claim markers for EN + ZH + PT-BR.
/// Merges English, Chinese, and Portuguese-Brazilian assertion patterns.
pub static COMBINED_CLAIM_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    // CJK alternatives intentionally outside \b groups — CJK has no whitespace word boundaries
    Regex::new(
        r"(?i)(?:\b(?:should|must|need to|recommend|argue|conclude|assert|claim|propose|suggest|deve|precisa|recomendamos|argumentamos|concluimos|afirmamos|propomos|sugerimos|devemos|e necessario|importante)\b|应该|必须|需要|建议|认为|证明)",
    )
    .expect("invalid combined claim regex")
});

/// Combined evidence markers for EN + ZH + PT-BR.
/// Merges English, Chinese, and Portuguese-Brazilian evidence patterns.
pub static COMBINED_EVIDENCE_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:\b(?:because|since|evidence|according to|studies|research|data|statistics|reports|evidencia|pesquisa|dados|numeros|relatorios|porque|ja que|visto que|segundo|conforme|estudos|pesquisas|evidencias|comprovadamente|segundo pesquisas)\b|因为|由于|根据|研究|数据|报告|显示|表明)",
    )
    .expect("invalid combined evidence regex")
});

/// Combined warrant markers for EN + ZH + PT-BR.
/// Merges English, Chinese, and Portuguese-Brazilian reasoning patterns.
pub static COMBINED_WARRANT_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:\b(?:therefore|thus|consequently|which means|this shows|implies|indicates|follows that|given that|assuming|portanto|assim|consequentemente|o que significa|isso demonstra|implica|indica|segue que|visto que|dado que|por conseguinte)\b|因此|所以|从而|表明|意味着|根据|鉴于)",
    )
    .expect("invalid combined warrant regex")
});

/// Combined rebuttal markers for EN + ZH + PT-BR.
/// Merges English, Chinese, and Portuguese-Brazilian counter-argument patterns.
pub static COMBINED_REBUTTAL_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:\b(?:however|but|although|despite|nevertheless|objection|entretanto|contudo|porem|apesar de|nao obstante|objecao|contra-argumento|embora|enquanto)\b|但是|然而|尽管|不过|可是)",
    )
    .expect("invalid combined rebuttal regex")
});

/// Combined backing markers for EN + ZH + PT-BR.
/// Merges English, Chinese, and Portuguese-Brazilian additional support patterns.
pub static COMBINED_BACKING_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:\b(?:furthermore|additionally|moreover|in addition|also|besides|another|ademais|outrossim|alem disso|alem do mais|vale notar|inclusive|igualmente|tambem|junto com)\b|此外|另外|而且|再者|并且|与此同时)",
    )
    .expect("invalid combined backing regex")
});

// Re-export combined markers for backward compatibility
pub use COMBINED_BACKING_MARKERS as BACKING_MARKERS;
pub use COMBINED_CLAIM_MARKERS as CLAIM_MARKERS;
pub use COMBINED_EVIDENCE_MARKERS as EVIDENCE_MARKERS;
pub use COMBINED_REBUTTAL_MARKERS as REBUTTAL_MARKERS;
pub use COMBINED_WARRANT_MARKERS as WARRANT_MARKERS;

/// Support indicators for discourse relations.
static SUPPORT_INDICATORS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(yes|agree|true|correct|indeed|exactly|right|also)\b")
        .expect("invalid support regex")
});

/// Attack indicators for discourse relations.
static ATTACK_INDICATORS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(no|disagree|wrong|incorrect|false|mistaken|not true)\b")
        .expect("invalid attack regex")
});

// Note: SUPPORT_INDICATORS and ATTACK_INDICATORS are accessible via relation_population module

// ---------------------------------------------------------------------------
// NLP Pattern Types
// ---------------------------------------------------------------------------

/// NLP pattern types following the Toulmin model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NLPPattern {
    /// Claim - the main assertion being made
    Claim,
    /// Evidence - facts and data supporting the claim
    Evidence,
    /// Warrant - reasoning that connects evidence to claim
    Warrant,
    /// Rebuttal - counter-argument or objection
    Rebuttal,
    /// Backing - additional support for the warrant
    Backing,
}

/// Relation types for discourse parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationType {
    /// Supports the preceding argument
    Support,
    /// Attacks the preceding argument
    Attack,
    /// Elaborates on the preceding argument
    Elaborate,
    /// Constrasts with the preceding argument
    Contrast,
    /// Concludes the argument
    Conclude,
}

impl std::fmt::Display for NLPPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NLPPattern::Claim => write!(f, "Claim"),
            NLPPattern::Evidence => write!(f, "Evidence"),
            NLPPattern::Warrant => write!(f, "Warrant"),
            NLPPattern::Rebuttal => write!(f, "Rebuttal"),
            NLPPattern::Backing => write!(f, "Backing"),
        }
    }
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationType::Support => write!(f, "Support"),
            RelationType::Attack => write!(f, "Attack"),
            RelationType::Elaborate => write!(f, "Elaborate"),
            RelationType::Contrast => write!(f, "Contrast"),
            RelationType::Conclude => write!(f, "Conclude"),
        }
    }
}

/// An element extracted from text using Erickson NLP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EricksonElement {
    /// The pattern type identified
    pub pattern: NLPPattern,
    /// The text span containing this element
    pub text: String,
    /// Character offset in the original text
    pub start: usize,
    /// End character offset
    pub end: usize,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
    /// The discourse relation if applicable
    pub relation: Option<RelationType>,
}

impl EricksonElement {
    /// Creates a new element with given parameters.
    pub fn new(
        pattern: NLPPattern,
        text: impl Into<String>,
        start: usize,
        end: usize,
        confidence: f32,
    ) -> Self {
        EricksonElement {
            pattern,
            text: text.into(),
            start,
            end,
            confidence,
            relation: None,
        }
    }

    /// Creates a claim element.
    pub fn claim(text: impl Into<String>, start: usize, end: usize) -> Self {
        Self::new(NLPPattern::Claim, text, start, end, 0.9)
    }

    /// Creates an evidence element.
    pub fn evidence(text: impl Into<String>, start: usize, end: usize) -> Self {
        Self::new(NLPPattern::Evidence, text, start, end, 0.85)
    }

    /// Creates a warrant element.
    pub fn warrant(text: impl Into<String>, start: usize, end: usize) -> Self {
        Self::new(NLPPattern::Warrant, text, start, end, 0.8)
    }

    /// Creates a rebuttal element.
    pub fn rebuttal(text: impl Into<String>, start: usize, end: usize) -> Self {
        Self::new(NLPPattern::Rebuttal, text, start, end, 0.75)
    }
}

// ---------------------------------------------------------------------------
// Erickson Extractor
// ---------------------------------------------------------------------------

/// Erickson extractor for argument mining.
#[derive(Debug)]
pub struct EricksonExtractor {
    /// Registered NLP patterns
    patterns: Vec<NLPPattern>,
    /// Discourse relations
    discourse_relations: Vec<RelationType>,
    /// Minimum confidence threshold
    min_confidence: f32,
    /// Optional RL feedback handler (from touring-core)
    feedback: Option<Box<dyn PatternFeedback>>,
}

impl Default for EricksonExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl EricksonExtractor {
    /// Creates a new Erickson extractor with default patterns.
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::erickson::EricksonExtractor;
    ///
    /// let extractor = EricksonExtractor::new();
    /// let elements = extractor.extract("We should upgrade serde because of CVE.");
    /// assert!(!elements.is_empty());
    /// ```
    pub fn new() -> Self {
        EricksonExtractor {
            patterns: vec![
                NLPPattern::Claim,
                NLPPattern::Evidence,
                NLPPattern::Warrant,
                NLPPattern::Rebuttal,
                NLPPattern::Backing,
            ],
            discourse_relations: vec![
                RelationType::Support,
                RelationType::Attack,
                RelationType::Elaborate,
                RelationType::Contrast,
                RelationType::Conclude,
            ],
            min_confidence: 0.5,
            feedback: None,
        }
    }

    /// Sets the minimum confidence threshold.
    pub fn with_min_confidence(mut self, threshold: f32) -> Self {
        self.min_confidence = threshold;
        self
    }

    /// Sets an RL feedback handler for pattern matching callbacks.
    pub fn with_rl_feedback(mut self, feedback: impl PatternFeedback + 'static) -> Self {
        self.feedback = Some(Box::new(feedback));
        self
    }

    /// Extracts argument elements from the given text.
    ///
    /// # Pipeline
    ///
    /// 1. Get sentence boundaries via `get_sentence_boundaries()`
    /// 2. Extract all pattern matches (Claim, Evidence, Warrant, Rebuttal, Backing)
    /// 3. Compute confidence using `compute_confidence_v2()` with sentence context
    /// 4. Deduplicate overlapping elements
    /// 5. Infer discourse relations via `infer_relations()`
    pub fn extract(&self, text: &str) -> Vec<EricksonElement> {
        // Step 1: Get sentence boundaries
        let boundaries = sentence_boundaries::get_sentence_boundaries(text);

        let mut elements = Vec::new();

        // Step 2: Extract claims
        for m in COMBINED_CLAIM_MARKERS.find_iter(text) {
            let confidence =
                self.compute_confidence_v2(text, m.as_str(), m.start(), NLPPattern::Claim);
            if confidence >= self.min_confidence {
                elements.push(EricksonElement::new(
                    NLPPattern::Claim,
                    m.as_str().to_string(),
                    m.start(),
                    m.end(),
                    confidence,
                ));
            }
        }

        // Extract evidence
        for m in COMBINED_EVIDENCE_MARKERS.find_iter(text) {
            let confidence =
                self.compute_confidence_v2(text, m.as_str(), m.start(), NLPPattern::Evidence);
            if confidence >= self.min_confidence {
                elements.push(EricksonElement::new(
                    NLPPattern::Evidence,
                    m.as_str().to_string(),
                    m.start(),
                    m.end(),
                    confidence,
                ));
            }
        }

        // Extract warrants
        for m in COMBINED_WARRANT_MARKERS.find_iter(text) {
            let confidence =
                self.compute_confidence_v2(text, m.as_str(), m.start(), NLPPattern::Warrant);
            if confidence >= self.min_confidence {
                elements.push(EricksonElement::new(
                    NLPPattern::Warrant,
                    m.as_str().to_string(),
                    m.start(),
                    m.end(),
                    confidence,
                ));
            }
        }

        // Extract rebuttals
        for m in COMBINED_REBUTTAL_MARKERS.find_iter(text) {
            let confidence =
                self.compute_confidence_v2(text, m.as_str(), m.start(), NLPPattern::Rebuttal);
            if confidence >= self.min_confidence {
                elements.push(EricksonElement::new(
                    NLPPattern::Rebuttal,
                    m.as_str().to_string(),
                    m.start(),
                    m.end(),
                    confidence,
                ));
            }
        }

        // Extract backing
        for m in COMBINED_BACKING_MARKERS.find_iter(text) {
            let confidence =
                self.compute_confidence_v2(text, m.as_str(), m.start(), NLPPattern::Backing);
            if confidence >= self.min_confidence {
                elements.push(EricksonElement::new(
                    NLPPattern::Backing,
                    m.as_str().to_string(),
                    m.start(),
                    m.end(),
                    confidence,
                ));
            }
        }

        // Sort by position
        elements.sort_by_key(|e| e.start);

        // Step 3: Deduplicate overlapping elements
        elements = self.deduplicate(elements);

        // Step 4: Infer discourse relations (pass pre-computed boundaries to avoid recomputation)
        if !elements.is_empty() {
            let mut elements_for_relation = elements.clone();
            let _relations_count = relation_population::infer_relations_with_boundaries(
                &mut elements_for_relation,
                text,
                &boundaries,
            );
            elements = elements_for_relation;
        }

        // Emit RL feedback for each extracted element
        if let Some(ref fb) = self.feedback {
            for elem in &elements {
                let ctx = PatternFeedbackContext {
                    pattern: FeedbackPattern::from(elem.pattern as u8),
                    text: elem.text.clone(),
                    start: elem.start,
                    end: elem.end,
                    confidence: elem.confidence,
                    relation: relation_to_feedback(elem.relation),
                    context: text.to_string(),
                };
                let _ = fb.on_pattern_matched(&ctx);
            }
        }

        elements
    }

    /// Computes confidence score using v2 algorithm with enhanced adjustments.
    fn compute_confidence_v2(
        &self,
        text: &str,
        matched: &str,
        offset: usize,
        pattern: NLPPattern,
    ) -> f32 {
        use self::confidence::compute_confidence_v2 as conf_v2;

        let base = match pattern {
            NLPPattern::Claim => 0.85,
            NLPPattern::Evidence => 0.80,
            NLPPattern::Warrant => 0.75,
            NLPPattern::Rebuttal => 0.70,
            NLPPattern::Backing => 0.70,
        };

        conf_v2(base, text, matched, offset)
    }

    /// Removes overlapping elements, keeping the higher confidence one.
    fn deduplicate(&self, mut elements: Vec<EricksonElement>) -> Vec<EricksonElement> {
        if elements.is_empty() {
            return elements;
        }

        elements.sort_by(|a, b| {
            a.start.cmp(&b.start).then_with(|| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .expect("confidence comparison should not panic")
            })
        });

        let Some(first) = elements.first() else {
            return elements;
        };
        let mut result = vec![first.clone()];
        for current in elements.iter().skip(1) {
            if let Some(last) = result.last_mut() {
                if current.start >= last.end {
                    result.push(current.clone());
                } else if current.confidence > last.confidence {
                    *last = current.clone();
                }
            }
        }

        result
    }

    /// Returns the number of registered patterns.
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// Returns the number of discourse relations.
    pub fn relation_count(&self) -> usize {
        self.discourse_relations.len()
    }
}

fn relation_to_feedback(rel: Option<RelationType>) -> FeedbackRelation {
    match rel {
        Some(RelationType::Support) => FeedbackRelation::Support,
        Some(RelationType::Attack) => FeedbackRelation::Attack,
        Some(RelationType::Elaborate) => FeedbackRelation::Elaborate,
        Some(RelationType::Contrast) => FeedbackRelation::Contrast,
        Some(RelationType::Conclude) => FeedbackRelation::Conclude,
        None => FeedbackRelation::None,
    }
}

/// Extracts argument elements from text using the default extractor.
///
/// # Example
///
/// ```rust
/// use touring_offensive::erickson::{extract, NLPPattern};
///
/// let args = extract("We should upgrade serde because it has a CVE.");
/// let claims: Vec<_> = args.iter().filter(|e| matches!(e.pattern, NLPPattern::Claim)).collect();
/// assert!(!claims.is_empty());
/// ```
pub fn extract(text: &str) -> Vec<EricksonElement> {
    let extractor = EricksonExtractor::new();
    extractor.extract(text)
}

// ---------------------------------------------------------------------------
// Public re-exports from submodules
// ---------------------------------------------------------------------------

/// Re-export QualifierPattern from confidence module for public API.
pub use confidence::QualifierPattern;

/// Re-export compute_qualifier from confidence module for public API.
pub use confidence::compute_qualifier;

/// Re-exports from relation_population for public API consumers.
pub use relation_population::{PopulationResult, RelationContext, get_relation_context};

/// Extracts argument elements and returns a [`PopulationResult`] with relation count.
///
/// Like [`extract`] but also returns how many discourse relations were inferred,
/// enabling callers to inspect relation density without re-running the pipeline.
///
/// # Example
///
/// ```rust
/// use touring_offensive::erickson::{extract_with_relations, NLPPattern};
///
/// let result = extract_with_relations("We should upgrade. Because the CVE is critical.");
/// assert!(!result.elements.is_empty());
/// println!("relations inferred: {}", result.relations_added);
/// ```
pub fn extract_with_relations(text: &str) -> PopulationResult {
    let elements = extract(text);
    let relations_added = elements.iter().filter(|e| e.relation.is_some()).count();
    PopulationResult {
        elements,
        relations_added,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_claim() {
        let text = "We should upgrade serde because it has a CVE.";
        let elements = extract(text);
        assert!(!elements.is_empty());
        assert!(
            elements
                .iter()
                .any(|e| matches!(e.pattern, NLPPattern::Claim)),
            "expected Claim pattern in: {text}"
        );
    }

    #[test]
    fn test_extract_evidence() {
        let text = "According to research, this is a critical vulnerability.";
        let elements = extract(text);
        assert!(
            elements
                .iter()
                .any(|e| matches!(e.pattern, NLPPattern::Evidence)),
            "expected Evidence pattern in: {text}"
        );
    }

    #[test]
    fn test_extract_warrant() {
        let text = "The system is vulnerable, thus we need to upgrade.";
        let elements = extract(text);
        assert!(
            elements
                .iter()
                .any(|e| matches!(e.pattern, NLPPattern::Warrant)),
            "expected to find warrant in: {text}"
        );
    }

    #[test]
    fn test_extract_rebuttal() {
        let text = "However, this may break backward compatibility.";
        let elements = extract(text);
        assert!(
            elements
                .iter()
                .any(|e| matches!(e.pattern, NLPPattern::Rebuttal)),
            "expected Rebuttal pattern in: {text}"
        );
    }

    #[test]
    fn test_extract_backing() {
        let text = "Furthermore, the new version has performance improvements.";
        let elements = extract(text);
        assert!(
            elements
                .iter()
                .any(|e| matches!(e.pattern, NLPPattern::Backing)),
            "expected Backing pattern in: {text}"
        );
    }

    #[test]
    fn test_extract_empty() {
        let elements = extract("");
        assert!(elements.is_empty());
    }

    #[test]
    fn test_extract_no_patterns() {
        let text = "Hello world this is some random text without patterns.";
        let elements = extract(text);
        assert!(elements.len() < 10);
    }

    #[test]
    fn test_erickson_extractor_new() {
        let extractor = EricksonExtractor::new();
        assert_eq!(extractor.pattern_count(), 5);
        assert_eq!(extractor.relation_count(), 5);
    }

    #[test]
    fn test_erickson_extractor_with_confidence() {
        let extractor = EricksonExtractor::new().with_min_confidence(0.9);
        let elements = extractor.extract("We should upgrade serde because it has a CVE.");
        for e in &elements {
            assert!(e.confidence >= 0.9);
        }
    }

    #[test]
    fn test_element_creation() {
        let element = EricksonElement::claim("test claim", 0, 10);
        assert!(matches!(element.pattern, NLPPattern::Claim));
        assert_eq!(element.text, "test claim");
        assert_eq!(element.start, 0);
        assert_eq!(element.end, 10);
        assert_eq!(element.confidence, 0.9);
    }

    #[test]
    fn test_deduplication() {
        let extractor = EricksonExtractor::new();
        let elements = vec![
            EricksonElement::new(NLPPattern::Claim, "test", 0, 4, 0.8),
            EricksonElement::new(NLPPattern::Claim, "test", 0, 4, 0.9),
            EricksonElement::new(NLPPattern::Evidence, "evidence", 5, 13, 0.7),
        ];
        let deduped = extractor.deduplicate(elements);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].confidence, 0.9);
    }

    #[test]
    fn test_nlp_pattern_display() {
        assert_eq!(NLPPattern::Claim.to_string(), "Claim");
        assert_eq!(NLPPattern::Evidence.to_string(), "Evidence");
    }

    #[test]
    fn test_extract_sorted_by_position() {
        let text = "First claim. Evidence here. Second claim.";
        let elements = extract(text);
        if elements.len() >= 2 {
            assert!(elements[0].start < elements[1].start);
        }
    }

    #[test]
    fn test_combined_markers_ptbr() {
        let text = "Devemos atualizar porque tem vulnerabilidade.";
        let elements = extract(text);
        assert!(!elements.is_empty());
    }

    #[test]
    fn test_combined_markers_zh() {
        let text = "我们应该升级因为有严重漏洞。";
        let elements = extract(text);
        assert!(!elements.is_empty());
    }

    #[test]
    fn test_qualifier_pattern_export() {
        let q = compute_qualifier("definitely");
        assert_eq!(q, Some(QualifierPattern::Definitely));
    }

    #[test]
    fn test_mixed_language_text() {
        let text = "We should upgrade. 因此我们需要更新。 Devemos atualizar.";
        let elements = extract(text);
        assert!(!elements.is_empty());
    }

    #[test]
    fn test_compute_qualifier_hedged() {
        assert_eq!(
            compute_qualifier("probably"),
            Some(QualifierPattern::Hedged)
        );
        assert_eq!(compute_qualifier("talvez"), Some(QualifierPattern::Hedged));
    }

    #[test]
    fn test_compute_qualifier_certain() {
        assert_eq!(compute_qualifier("must"), Some(QualifierPattern::Certain));
        assert_eq!(compute_qualifier("shall"), Some(QualifierPattern::Certain));
    }
}
