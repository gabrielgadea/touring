//! NLP enrichment for CodeHealthReport via EricksonExtractor.
//!
//! Provides NlpMetrics that enrich CodeHealthReport with NLP-derived
//! argument analysis (claim/evidence/warrant ratios, confidence scores).
//!
//! # Feature Gate
//! This module requires the `erickson-bridge` feature in touring-analysis/Cargo.toml
//! which adds optional dependency on touring-offensive.

/// NLP metrics extracted from text via EricksonExtractor.
#[derive(Debug, Clone)]
pub struct NlpMetrics {
    /// Total number of argument elements (claims + evidence + warrants).
    pub argument_count: usize,
    /// Average confidence across all extracted elements [0.0, 1.0].
    pub avg_confidence: f32,
    /// Ratio of claims to evidence: claim_count / evidence_count.
    /// Values > 1.0 mean more claims than evidence.
    pub claim_evidence_ratio: f32,
    /// Number of warrant elements (logical bridging).
    pub warrant_count: usize,
    /// Number of claim elements.
    pub claim_count: usize,
    /// Number of evidence elements.
    pub evidence_count: usize,
}

impl NlpMetrics {
    /// Compute NLP metrics from EricksonExtractor output.
    pub fn from_elements(elements: &[crate::health::EricksonElement]) -> Self {
        if elements.is_empty() {
            return NlpMetrics {
                argument_count: 0,
                avg_confidence: 0.0,
                claim_evidence_ratio: 0.0,
                warrant_count: 0,
                claim_count: 0,
                evidence_count: 0,
            };
        }

        use touring_offensive::erickson::NLPPattern;
        let claim_count = elements
            .iter()
            .filter(|e| matches!(e.pattern, NLPPattern::Claim))
            .count();
        let evidence_count = elements
            .iter()
            .filter(|e| matches!(e.pattern, NLPPattern::Evidence))
            .count();
        let warrant_count = elements
            .iter()
            .filter(|e| matches!(e.pattern, NLPPattern::Warrant))
            .count();
        let avg_confidence =
            elements.iter().map(|e| e.confidence).sum::<f32>() / elements.len() as f32;
        let claim_evidence_ratio = if evidence_count > 0 {
            claim_count as f32 / evidence_count as f32
        } else {
            claim_count as f32
        };

        NlpMetrics {
            argument_count: elements.len(),
            avg_confidence,
            claim_evidence_ratio,
            warrant_count,
            claim_count,
            evidence_count,
        }
    }
}

/// Compute NLP metrics from raw text using EricksonExtractor.
#[cfg(feature = "erickson-bridge")]
pub fn compute_nlp_metrics(text: &str) -> Option<NlpMetrics> {
    use touring_offensive::erickson::EricksonExtractor;

    let extractor = EricksonExtractor::new();
    let elements = extractor.extract(text);

    if elements.is_empty() {
        return None;
    }

    Some(NlpMetrics::from_elements(&elements))
}

/// Compute NLP metrics from pre-extracted elements (no feature gate needed).
pub fn compute_from_elements(elements: &[crate::health::EricksonElement]) -> Option<NlpMetrics> {
    if elements.is_empty() {
        return None;
    }
    Some(NlpMetrics::from_elements(elements))
}
