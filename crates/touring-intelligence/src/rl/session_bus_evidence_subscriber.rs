//! SessionBus Evidence Subscriber — bridges SessionBus.evidence_tx to LatencyAdaptationPipeline.
//!
//! This module provides an `EvidenceSubscriber` that wraps `LatencyAdaptationPipeline`
//! and implements `HookStatsConsumer`. It subscribes to `SessionBus.evidence_tx`
//! and converts `Evidence` events into `PipelineContext` for drift-aware decisions.
//!
//! # Architecture
//!
//! ```text
//! SessionBus.evidence_tx (broadcast)
//!     │
//!     ▼ (subscribe_evidence())
//! EvidenceSubscriber
//!     │ converts Evidence → PipelineContext
//!     ▼
//! LatencyAdaptationPipeline.run(ctx)
//!     │
//!     ▼
//! MetacognitiveDecision (Stable | IncreaseParallelism | DecreaseParallelism | ResetState)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! // In touring-hooks ContextRuntime where SessionBus lives:
//! let mut subscriber = EvidenceSubscriber::new(pipeline, bus.subscribe_evidence());
//! // Drive the subscriber in a tokio task (requires async-memory feature):
//! tokio::spawn(async move { subscriber.run().await });
//! ```

#[cfg(feature = "async-memory")]
use tokio::sync::broadcast;

use crate::rl::metacognitive_pipeline::{
    LatencyAdaptationPipeline, MetacognitiveDecision, PipelineContext,
};
use crate::rl::streaming_hook_integration::HookStatsConsumer;
use touring_simd::cortex::Evidence;

// ── Evidence Subscriber ────────────────────────────────────────────────────────

/// Subscribes to `SessionBus.evidence_tx` and feeds `Evidence` events into
/// `LatencyAdaptationPipeline` for drift-aware decision making.
///
/// Wraps the pipeline and the broadcast receiver, implementing `HookStatsConsumer`
/// so it can be used as a drop-in consumer from touring-hooks.
///
/// **Available when `async-memory` feature is enabled** (touring-hooks enables it).
#[cfg(feature = "async-memory")]
#[derive(Debug)]
pub struct EvidenceSubscriber {
    /// The latency adaptation pipeline processing each evidence event.
    pipeline: LatencyAdaptationPipeline,
    /// Broadcast receiver for SessionBus.evidence_tx.
    receiver: broadcast::Receiver<Evidence>,
    /// Count of evidence events processed.
    evidence_processed: u64,
    /// Count of decisions that triggered adaptation.
    adaptation_decisions: u64,
}

#[cfg(feature = "async-memory")]
impl EvidenceSubscriber {
    /// Create a new subscriber wrapping the given pipeline and receiver.
    pub fn new(
        pipeline: LatencyAdaptationPipeline,
        receiver: broadcast::Receiver<Evidence>,
    ) -> Self {
        Self {
            pipeline,
            receiver,
            evidence_processed: 0,
            adaptation_decisions: 0,
        }
    }

    /// Run the subscriber, processing evidence events until the receiver drops.
    /// Returns `Ok(())` if the loop exits normally, or `Err` on tokio recv error.
    pub async fn run(&mut self) -> Result<(), broadcast::error::RecvError> {
        loop {
            let evidence = self.receiver.recv().await?;
            self.process_evidence(evidence);
        }
    }

    /// Process a single evidence event — convert to PipelineContext and run pipeline.
    pub fn process_evidence(&mut self, evidence: Evidence) {
        self.evidence_processed += 1;

        let ctx = PipelineContext {
            tool_name: format!("evidence_source_{}", evidence.source_id),
            latency_ms: (evidence.value * 1000.0) as u64,
            success: evidence.total > 0 && evidence.successes == evidence.total,
            file_path: None,
            error_rate: if evidence.total > 0 {
                1.0 - (evidence.successes as f64 / evidence.total as f64)
            } else {
                0.0
            },
            memory_bound: false,
            current_threshold: 64,
            vector_dim: None,
        };

        let decision = self.pipeline.run(ctx);
        if !matches!(
            decision,
            MetacognitiveDecision::Stable | MetacognitiveDecision::LogAndContinue
        ) {
            self.adaptation_decisions += 1;
        }
    }

    /// Get the number of evidence events processed.
    pub fn evidence_processed(&self) -> u64 {
        self.evidence_processed
    }

    /// Get the number of adaptation decisions made.
    pub fn adaptation_decisions(&self) -> u64 {
        self.adaptation_decisions
    }
}

#[cfg(feature = "async-memory")]
impl HookStatsConsumer for EvidenceSubscriber {
    fn consume_hook_quality(
        &mut self,
        _summary: crate::rl::streaming_hook_integration::HookQualitySummary,
    ) {
        // HookQualitySummary is from touring-hooks; we primarily consume Evidence here.
        // EvidenceSubscriber receives Evidence directly from SessionBus.evidence_tx broadcast.
    }

    fn assessments_consumed(&self) -> u64 {
        self.evidence_processed
    }
}

// ── Sync-only Evidence Processor (always available) ─────────────────────────────

/// Synchronous evidence processor that converts `Evidence` to `PipelineContext`
/// and runs it through `LatencyAdaptationPipeline` without requiring async/tokio.
///
/// Use this when you need to process evidence in a sync context or batch mode.
#[derive(Debug)]
pub struct SyncEvidenceProcessor {
    pipeline: LatencyAdaptationPipeline,
    evidence_processed: u64,
    adaptation_decisions: u64,
}

impl SyncEvidenceProcessor {
    /// Create a new sync processor wrapping the given pipeline.
    pub fn new(pipeline: LatencyAdaptationPipeline) -> Self {
        Self {
            pipeline,
            evidence_processed: 0,
            adaptation_decisions: 0,
        }
    }

    /// Process a single evidence event — convert to PipelineContext and run pipeline.
    pub fn process_evidence(&mut self, evidence: Evidence) {
        self.evidence_processed += 1;

        let ctx = PipelineContext {
            tool_name: format!("evidence_source_{}", evidence.source_id),
            latency_ms: (evidence.value * 1000.0) as u64,
            success: evidence.total > 0 && evidence.successes == evidence.total,
            file_path: None,
            error_rate: if evidence.total > 0 {
                1.0 - (evidence.successes as f64 / evidence.total as f64)
            } else {
                0.0
            },
            memory_bound: false,
            current_threshold: 64,
            vector_dim: None,
        };

        let decision = self.pipeline.run(ctx);
        if !matches!(
            decision,
            MetacognitiveDecision::Stable | MetacognitiveDecision::LogAndContinue
        ) {
            self.adaptation_decisions += 1;
        }
    }

    /// Get the number of evidence events processed.
    pub fn evidence_processed(&self) -> u64 {
        self.evidence_processed
    }

    /// Get the number of adaptation decisions made.
    pub fn adaptation_decisions(&self) -> u64 {
        self.adaptation_decisions
    }

    /// Get a reference to the underlying pipeline.
    pub fn pipeline(&self) -> &LatencyAdaptationPipeline {
        &self.pipeline
    }

    /// Get a mutable reference to the underlying pipeline.
    pub fn pipeline_mut(&mut self) -> &mut LatencyAdaptationPipeline {
        &mut self.pipeline
    }
}

impl HookStatsConsumer for SyncEvidenceProcessor {
    fn consume_hook_quality(
        &mut self,
        summary: crate::rl::streaming_hook_integration::HookQualitySummary,
    ) {
        // Synthesize an Evidence from HookQualitySummary for pipeline processing
        let evidence = Evidence {
            source_id: summary.total_hooks_fired as usize,
            value: summary.avg_latency_ms,
            confidence: summary.composite_score,
            successes: (summary.success_rate * summary.total_hooks_fired as f64) as u32,
            total: summary.total_hooks_fired,
        };
        self.process_evidence(evidence);
    }

    fn assessments_consumed(&self) -> u64 {
        self.evidence_processed
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_evidence() -> Evidence {
        Evidence {
            source_id: 1,
            value: 45.0,
            confidence: 0.92,
            successes: 10,
            total: 10,
        }
    }

    #[test]
    fn test_sync_processor_evidence_processed() {
        let pipeline = LatencyAdaptationPipeline::new();
        let mut processor = SyncEvidenceProcessor::new(pipeline);
        processor.process_evidence(make_test_evidence());
        assert_eq!(processor.evidence_processed(), 1);
    }

    #[test]
    fn test_sync_processor_adaptation_decisions() {
        let pipeline = LatencyAdaptationPipeline::new();
        let mut processor = SyncEvidenceProcessor::new(pipeline);

        // High-latency evidence to trigger IncreaseParallelism
        let high_latency = Evidence {
            source_id: 2,
            value: 500.0,
            confidence: 0.95,
            successes: 5,
            total: 10,
        };
        processor.process_evidence(high_latency);
        assert_eq!(processor.evidence_processed(), 1);
    }

    #[test]
    fn test_hook_stats_consumer_trait() {
        let pipeline = LatencyAdaptationPipeline::new();
        let mut processor = SyncEvidenceProcessor::new(pipeline);

        let summary = crate::rl::streaming_hook_integration::HookQualitySummary::from_dims(
            "test-session".to_string(),
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
        processor.consume_hook_quality(summary);
        assert_eq!(processor.assessments_consumed(), 1);
    }

    #[cfg(feature = "async-memory")]
    #[test]
    fn test_async_subscriber_creation() {
        use tokio::sync::broadcast;
        let pipeline = LatencyAdaptationPipeline::new();
        let (_, rx) = broadcast::channel(16);
        let subscriber = EvidenceSubscriber::new(pipeline, rx);
        assert_eq!(subscriber.evidence_processed(), 0);
    }
}
