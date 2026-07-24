//! Feedback types for the Erickson NLP RL integration.
//!
//! This module defines the `PatternFeedback` trait and supporting types
//! that allow the Erickson extractor to emit reward signals to a reinforcement
//! learning system without creating a cyclic dependency between
//! `touring-offensive` and `touring-learning`.

use serde::{Deserialize, Serialize};

/// Discourse relation types from the Erickson argument mining taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackRelation {
    /// The current element supports a previously-emitted claim.
    Support,
    /// The current element attacks (contradicts) a previously-emitted claim.
    Attack,
    /// The current element elaborates on a previously-emitted claim.
    Elaborate,
    /// The current element contrasts with a previously-emitted claim.
    Contrast,
    /// The current element concludes a chain of reasoning.
    Conclude,
    /// No discourse relation to a previous element.
    None,
}

/// NLP pattern types that can be extracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackPattern {
    /// A claim — an assertion that the author is advancing.
    Claim,
    /// Evidence — material offered in support of a claim.
    Evidence,
    /// Warrant — the bridge that licenses the inference from evidence to claim.
    Warrant,
    /// Rebuttal — material attacking a claim or warrant.
    Rebuttal,
    /// Backing — additional support for a warrant.
    Backing,
}

impl From<u8> for FeedbackPattern {
    fn from(v: u8) -> Self {
        match v {
            0 => FeedbackPattern::Claim,
            1 => FeedbackPattern::Evidence,
            2 => FeedbackPattern::Warrant,
            3 => FeedbackPattern::Rebuttal,
            4 => FeedbackPattern::Backing,
            _ => FeedbackPattern::Claim,
        }
    }
}

/// A lightweight reference to an extracted Erickson element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternFeedbackContext {
    /// Which [`FeedbackPattern`] was matched.
    pub pattern: FeedbackPattern,
    /// Snippet text that triggered the match.
    pub text: String,
    /// Byte offset of the start of the match in the source text.
    pub start: usize,
    /// Byte offset of the end of the match in the source text.
    pub end: usize,
    /// Confidence score in `[0.0, 1.0]` from the extractor.
    pub confidence: f32,
    /// Discourse relation to the surrounding context.
    pub relation: FeedbackRelation,
    /// Surrounding context (typically one sentence on each side).
    pub context: String,
}

/// Feedback signal types for RL integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackSignal {
    /// Pattern was correct on all dimensions.
    Correct,
    /// Pattern was correct in intent but had a minor issue
    /// (e.g. wrong offset, off-by-one boundary).
    MinorError,
    /// Pattern was wrong — false positive, wrong label, or
    /// out-of-context.
    MajorError,
    /// No feedback was provided for this pattern.
    NoFeedback,
}

/// Result of RL feedback processing.
#[derive(Debug, Clone)]
pub struct FeedbackResult {
    /// Whether the operation succeeded (true for positive
    /// feedback, false for failure).
    pub success: bool,
    /// Signal type, mirroring [`FeedbackSignal`].
    pub signal: FeedbackSignal,
    /// Optional numeric reward in the LinUCB arm update.
    /// `None` signals "no update".
    pub reward: Option<f32>,
}

impl Default for FeedbackResult {
    fn default() -> Self {
        Self {
            success: false,
            signal: FeedbackSignal::NoFeedback,
            reward: None,
        }
    }
}

impl FeedbackResult {
    /// Build a successful [`FeedbackResult`] with the given
    /// signal and reward value.
    pub fn success(signal: FeedbackSignal, reward: f32) -> Self {
        Self {
            success: true,
            signal,
            reward: Some(reward),
        }
    }
    /// Build a failed [`FeedbackResult`] (the default — no reward
    /// update is emitted to the LinUCB bandit).
    pub fn failure() -> Self {
        Self::default()
    }
}

/// Trait for RL-driven extraction quality feedback.
pub trait PatternFeedback: Send + Sync + std::fmt::Debug {
    /// Invoked by the Erickson extractor every time a pattern
    /// matches. Implementations convert the context into a
    /// [`FeedbackResult`] and forward it to the LinUCB bandit.
    fn on_pattern_matched(&self, element: &PatternFeedbackContext) -> FeedbackResult;
}
