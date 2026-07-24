//! Streaming hook integration - bridges ACO hook quality assessments to RL subsystems.
//!
//! This module provides the bridge that connects `HookQualityAssessment` (from
//! touring-hooks) to the RL subsystems in touring-learning (LinUCB, SelfOptimizer,
//! RingBuffer).
//!
//! # Architecture
//!
//! The bridge uses a **push model** where touring-hooks calls into touring-learning
//! when it has a `HookQualityAssessment` ready. This avoids a dependency cycle since
//! touring-learning does NOT import from touring-hooks.
//!
//! ```text
//! touring-hooks                      touring-learning
//!      |                                    |
//!      |  [HookStatsConsumer trait]         |
//!      |---------------------------------->|
//!      |  consume_hook_quality(summary)    |
//!      |                                    |
//!      |                         [StreamingStatsBridge]
//!      |                                    |
//!      |                         feed_to_linucb()
//!      |                         feed_to_optimizer()
//!      |                         feed_to_ring_buffer()
//! ```
//!
//! # Usage
//!
//! touring-hooks implements `HookStatsConsumer` and calls `consume_hook_quality()`
//! when a `HookQualityAssessment` is ready. The bridge then distributes the data
//! to all registered RL subsystems.

use serde::{Deserialize, Serialize};

/// Summary of hook quality metrics extracted from `HookQualityAssessment`.
///
/// This is a simplified DTO that avoids a direct import of `HookQualityAssessment`
/// from touring-hooks, preventing a dependency cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookQualitySummary {
    /// Session identifier.
    pub session_id: String,
    /// Total hooks fired in this assessment window.
    pub total_hooks_fired: u32,
    /// Success rate (0.0-1.0).
    pub success_rate: f64,
    /// Average latency in milliseconds.
    pub avg_latency_ms: f64,
    /// Maximum latency in milliseconds.
    pub max_latency_ms: f64,
    /// Ratio of fast hooks (within latency target).
    pub fast_hooks_ratio: f64,
    /// Precision dimension score (0.0-1.0).
    pub precision_score: f64,
    /// Coverage dimension score (0.0-1.0).
    pub coverage_score: f64,
    /// Latency dimension score (0.0-1.0).
    pub latency_score: f64,
    /// Knowledge dimension score (0.0-1.0).
    pub knowledge_score: f64,
    /// Context dimension score (0.0-1.0).
    pub context_score: f64,
    /// Reliability dimension score (0.0-1.0).
    pub reliability_score: f64,
    /// Integration dimension score (0.0-1.0).
    pub integration_score: f64,
    /// Security dimension score (0.0-1.0).
    pub security_score: f64,
    /// Evolution dimension score (0.0-1.0).
    pub evolution_score: f64,
    /// Composite quality score (0.0-1.0).
    pub composite_score: f64,
}

impl HookQualitySummary {
    /// Build a summary from dimension results (called from touring-hooks).
    #[allow(clippy::too_many_arguments)]
    pub fn from_dims(
        session_id: String,
        total_hooks_fired: u32,
        success_rate: f64,
        avg_latency_ms: f64,
        max_latency_ms: f64,
        fast_hooks_ratio: f64,
        precision_score: f64,
        coverage_score: f64,
        latency_score: f64,
        knowledge_score: f64,
        context_score: f64,
        reliability_score: f64,
        integration_score: f64,
        security_score: f64,
        evolution_score: f64,
    ) -> Self {
        let composite_score = (precision_score
            + coverage_score
            + latency_score
            + knowledge_score
            + context_score
            + reliability_score
            + integration_score
            + security_score
            + evolution_score)
            / 9.0;
        Self {
            session_id,
            total_hooks_fired,
            success_rate,
            avg_latency_ms,
            max_latency_ms,
            fast_hooks_ratio,
            precision_score,
            coverage_score,
            latency_score,
            knowledge_score,
            context_score,
            reliability_score,
            integration_score,
            security_score,
            evolution_score,
            composite_score,
        }
    }
}

/// Consumer trait for hook quality assessments.
///
/// Implement this trait in touring-hooks to bridge `HookQualityAssessment` to
/// touring-learning's RL subsystems. touring-learning defines this trait (consumer
/// defines interface), and touring-hooks implements it.
pub trait HookStatsConsumer {
    /// Consume a hook quality assessment, distributing it to all RL subsystems.
    fn consume_hook_quality(&mut self, summary: HookQualitySummary);

    /// Returns the number of assessments consumed so far.
    fn assessments_consumed(&self) -> u64;
}

/// Fixed-capacity buffer for hook quality summaries.
///
/// Stores the most recent N hook quality summaries for trend analysis.
pub type HookQualityBuffer = super::RingBuffer<HookQualitySummary>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_composite_calculation() {
        let summary = HookQualitySummary::from_dims(
            "test".to_string(),
            10,
            0.9,
            50.0,
            100.0,
            0.95,
            0.9,
            1.0,
            0.95,
            0.8,
            0.85,
            0.9,
            0.88,
            1.0,
            0.75,
        );
        // Composite = average of 9 dimensions
        let expected = (0.9 + 1.0 + 0.95 + 0.8 + 0.85 + 0.9 + 0.88 + 1.0 + 0.75) / 9.0;
        assert!((summary.composite_score - expected).abs() < 1e-6);
    }

    #[test]
    fn test_ring_buffer_capacity() {
        let mut buffer: HookQualityBuffer = HookQualityBuffer::new(3);
        for i in 0..5 {
            buffer.write(HookQualitySummary::from_dims(
                format!("session-{}", i),
                i as u32,
                0.9,
                50.0,
                100.0,
                0.95,
                0.9,
                1.0,
                0.95,
                0.8,
                0.85,
                0.9,
                0.88,
                1.0,
                0.75,
            ));
        }
        // Buffer holds at most 3, oldest 2 overwritten
        assert_eq!(buffer.len(), 3);
    }
}
