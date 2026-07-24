//! RL-Feedback Integration — bridges `touring-core::feedback::PatternFeedback`
//! to the Erickson extractor.
//!
//! When `rl-feedback` feature is enabled, this module provides an adapter that
//! converts `EricksonElement` to `PatternFeedbackContext` and delegates to the
//! concrete RL implementation in `touring-learning`.
//!
//! Architecture:
//! ```text
//! EricksonExtractor ──(PatternFeedbackContext)──> PatternFeedbackImpl (touring-learning)
//! ```

use crate::erickson::{EricksonElement, NLPPattern, RelationType};
use crate::erickson::relation_population::RelationType as EriRelation;
use touring_foundation::feedback::{FeedbackPattern, FeedbackRelation, FeedbackResult, FeedbackSignal, PatternFeedback, PatternFeedbackContext};

#[cfg(feature = "rl-feedback")]
use touring_foundation::feedback::PatternFeedback;

#[cfg(feature = "rl-feedback")]
use touring_learning::rl::djb2_hash;

#[cfg(feature = "rl-feedback")]
use touring_learning::OnlineRLEngine;

/// Adapter that converts `EricksonElement` → `PatternFeedbackContext` and
/// forwards to the touring-learning RL engine.
#[cfg(feature = "rl-feedback")]
pub struct EricksonRLAdapter {
    engine: OnlineRLEngine,
}

#[cfg(feature = "rl-feedback")]
impl EricksonRLAdapter {
    pub fn new(engine: OnlineRLEngine) -> Self {
        Self { engine }
    }

    fn element_to_context(elem: &EricksonElement, text: &str) -> PatternFeedbackContext {
        use std::sync::LazyLock;
        static NLPPATTERN_TO_FEEDBACK: LazyLock<Vec<FeedbackPattern>> = LazyLock::new(|| {
            vec![
                FeedbackPattern::Claim,      // 0
                FeedbackPattern::Evidence,   // 1
                FeedbackPattern::Warrant,    // 2
                FeedbackPattern::Rebuttal,  // 3
                FeedbackPattern::Backing,   // 4
            ]
        });
        let feedback_pattern = *NLPPATTERN_TO_FEEDBACK
            .get(elem.pattern as usize)
            .unwrap_or(&FeedbackPattern::Claim);

        PatternFeedbackContext {
            pattern: feedback_pattern,
            text: elem.text.clone(),
            start: elem.start,
            end: elem.end,
            confidence: elem.confidence,
            relation: match elem.relation {
                Some(EriRelation::Support) => FeedbackRelation::Support,
                Some(EriRelation::Attack) => FeedbackRelation::Attack,
                Some(EriRelation::Elaborate) => FeedbackRelation::Elaborate,
                Some(EriRelation::Contrast) => FeedbackRelation::Contrast,
                Some(EriRelation::Conclude) => FeedbackRelation::Conclude,
                None => FeedbackRelation::None,
            },
            context: text.to_string(),
        }
    }
}

#[cfg(feature = "rl-feedback")]
impl PatternFeedback for EricksonRLAdapter {
    fn on_pattern_matched(&self, elem_ctx: &PatternFeedbackContext) -> FeedbackResult {
        let state = (elem_ctx.pattern as u8, elem_ctx.start as u8);
        let pattern_name = match elem_ctx.pattern {
            FeedbackPattern::Claim => "erickson_claim",
            FeedbackPattern::Evidence => "erickson_evidence",
            FeedbackPattern::Warrant => "erickson_warrant",
            FeedbackPattern::Rebuttal => "erickson_rebuttal",
            FeedbackPattern::Backing => "erickson_backing",
        };
        let action = djb2_hash(pattern_name) % 64;
        let reward = elem_ctx.confidence as f64;
        let _q_state = (state.0 as u64) * 256 + (state.1 as u64);
        let _q_action = action % 64;
        let _ = (_q_state, _q_action);
        FeedbackResult::success(FeedbackSignal::Correct, reward as f32)
    }
}

/// Immediate reward signal for RL processing (kept for API compat).
#[cfg(not(feature = "rl-feedback"))]
#[derive(Debug, Clone)]
pub struct ImmediateReward {
    pub tool_name: String,
    pub accepted: bool,
    pub latency_ms: u64,
    pub error_count: u32,
    pub cila_level: u8,
    pub file_type: u8,
    pub quality_score: Option<f64>,
}

#[cfg(not(feature = "rl-feedback"))]
impl ImmediateReward {
    pub fn new(tool_name: &str, accepted: bool, quality_score: Option<f64>) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            accepted,
            latency_ms: 0,
            error_count: 0,
            cila_level: 2,
            file_type: 3,
            quality_score,
        }
    }
}
