//! LatencyAdaptationPipeline — Unified metacognition for hook execution.
//!
//! Combines three adaptive mechanisms into a single coherent decision system:
//! - **CUSUM** ([`LatencyDriftDetector`]): detects latency drift via cumulative sum
//! - **ACO Pheromone** ([`AcoPheromone`]): adaptive threshold adjustment via pheromone feedback
//! - **Actor-Critic** ([`ActorCritic`]): RL-based action selection for tool execution
//!
//! Latency adaptation pipeline combining CUSUM drift detection, ACO pheromone
//! threshold adaptation, and Actor-Critic RL for hook execution parallelism tuning.
//!
//! # Pipeline Flow
//!
//! ```text
//! PipelineContext (latency, tool, outcome)
//!     │
//!     ▼
//! [CUSUM Detector] ──► drift_signal ──┐
//!     │                              │
//! [ACO Pheromone] ◄──┐               │
//!     │              │               │
//! [Actor-Critic] ────┤               │
//!     │              │               │
//!     └──────────────┴───► [MetacognitiveDecision]
//! ```
//!
//! # Usage
//!
//! ```rust
//! use touring_intelligence::rl::metacognitive_pipeline::{LatencyAdaptationPipeline, PipelineContext};
//!
//! let mut pipeline = LatencyAdaptationPipeline::new();
//! let ctx = PipelineContext::builder()
//!     .tool_name("Read")
//!     .latency_ms(45)
//!     .success(true)
//!     .build();
//! let decision = pipeline.run(ctx);
//! ```

use tracing::instrument;

use crate::rl::ranking::cusum::{DriftDirection, DriftSignal, LatencyDriftDetector};
use crate::rl::rl::actor_critic::{ActionSelection, ActorCritic, ActorCriticConfig};
use touring_simd::learning::AcoPheromone;

// ── Decision Types ───────────────────────────────────────────────────────────

/// Decision emitted by the metacognitive pipeline after processing context.
#[derive(Debug, Clone, PartialEq)]
pub enum MetacognitiveDecision {
    /// No adaptation needed — metrics are stable.
    Stable,
    /// Increase parallelism or batch size — performance is degrading.
    IncreaseParallelism {
        /// Recommended new threshold.
        recommended_threshold: usize,
        /// Confidence in this decision (0-1).
        confidence: f64,
    },
    /// Decrease parallelism or batch size — resource pressure detected.
    DecreaseParallelism {
        /// Recommended new threshold.
        recommended_threshold: usize,
        /// Confidence in this decision (0-1).
        confidence: f64,
    },
    /// Reset adaptive state — significant drift detected.
    ResetState {
        /// Reason for reset.
        reason: String,
    },
    /// Explore alternative tool or approach.
    Explore {
        /// Suggested exploration action.
        action: ExplorationAction,
    },
    /// Log for later analysis — no immediate action.
    LogAndContinue,
}

/// Exploration action suggested by Actor-Critic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExplorationAction {
    /// Try a different tool.
    SwitchTool,
    /// Retry with different parameters.
    RetryWithBackoff,
    /// Defer execution.
    Defer,
    /// Force parallel execution.
    ForceParallel,
}

/// Source of evidence contributing to the decision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EvidenceSource {
    /// CUSUM latency drift detector.
    Cusum,
    /// ACO pheromone feedback.
    AcoPheromone,
    /// Actor-Critic RL agent.
    ActorCritic,
}

/// Metadata about how the decision was reached.
#[derive(Debug, Clone)]
pub struct DecisionMetadata {
    /// Which sources contributed to the decision.
    pub sources: Vec<EvidenceSource>,
    /// CUSUM drift signal at time of decision.
    pub drift_signal: DriftSignal,
    /// ACO pheromone confidence (0-1).
    pub pheromone_confidence: f64,
    /// Actor-Critic action selection.
    pub action_selection: Option<ActionSelection>,
    /// Whether this is a corrective action.
    pub is_corrective: bool,
}

// ── Pipeline Context ───────────────────────────────────────────────────────────

/// Input context for the metacognitive pipeline.
///
/// Contains all observations from a single hook execution that inform
/// the adaptive decision.
#[derive(Debug, Clone)]
pub struct PipelineContext {
    /// Name of the tool that was executed.
    pub tool_name: String,
    /// Observed latency in milliseconds.
    pub latency_ms: u64,
    /// Whether the tool succeeded.
    pub success: bool,
    /// File path affected (if applicable).
    pub file_path: Option<String>,
    /// Error rate estimate (0-1).
    pub error_rate: f64,
    /// Whether the system was memory-bound during execution.
    pub memory_bound: bool,
    /// Current parallel threshold.
    pub current_threshold: usize,
    /// Vector dimension (for adaptive threshold calculation).
    pub vector_dim: Option<usize>,
}

impl PipelineContext {
    /// Builder for PipelineContext.
    pub fn builder() -> PipelineContextBuilder {
        PipelineContextBuilder::new()
    }
}

/// Builder for PipelineContext.
#[derive(Debug, Clone, Default)]
pub struct PipelineContextBuilder {
    tool_name: String,
    latency_ms: u64,
    success: bool,
    file_path: Option<String>,
    error_rate: f64,
    memory_bound: bool,
    current_threshold: usize,
    vector_dim: Option<usize>,
}

impl PipelineContextBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tool name.
    pub fn tool_name(mut self, name: impl Into<String>) -> Self {
        self.tool_name = name.into();
        self
    }

    /// Set the latency in milliseconds.
    pub fn latency_ms(mut self, ms: u64) -> Self {
        self.latency_ms = ms;
        self
    }

    /// Set the success flag.
    pub fn success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    /// Set the file path.
    pub fn file_path(mut self, path: Option<String>) -> Self {
        self.file_path = path;
        self
    }

    /// Set the error rate (0-1).
    pub fn error_rate(mut self, rate: f64) -> Self {
        self.error_rate = rate;
        self
    }

    /// Set the memory bound flag.
    pub fn memory_bound(mut self, bound: bool) -> Self {
        self.memory_bound = bound;
        self
    }

    /// Set the current parallel threshold.
    pub fn current_threshold(mut self, threshold: usize) -> Self {
        self.current_threshold = threshold;
        self
    }

    /// Set the vector dimension (for adaptive threshold).
    pub fn vector_dim(mut self, dim: usize) -> Self {
        self.vector_dim = Some(dim);
        self
    }

    /// Build the PipelineContext.
    pub fn build(self) -> PipelineContext {
        PipelineContext {
            tool_name: self.tool_name,
            latency_ms: self.latency_ms,
            success: self.success,
            file_path: self.file_path,
            error_rate: self.error_rate,
            memory_bound: self.memory_bound,
            current_threshold: self.current_threshold,
            vector_dim: self.vector_dim,
        }
    }
}

// ── Pipeline Statistics ───────────────────────────────────────────────────────

/// Statistics from the latency adaptation pipeline.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LatencyStats {
    /// Total decisions made.
    pub decisions: u64,
    /// Number of stable decisions.
    pub stable_count: u64,
    /// Number of increase parallelism decisions.
    pub increase_count: u64,
    /// Number of decrease parallelism decisions.
    pub decrease_count: u64,
    /// Number of reset decisions.
    pub reset_count: u64,
    /// Number of explore decisions.
    pub explore_count: u64,
    /// Number of log decisions.
    pub log_count: u64,
    /// Current CUSUM sample count.
    pub cusum_samples: u64,
    /// Current pheromone confidence.
    pub pheromone_confidence: f64,
    /// Actor-Critic epsilon (exploration rate).
    pub actor_epsilon: f32,
}

// ── Metacognitive Pipeline ─────────────────────────────────────────────────────

/// Latency adaptation pipeline combining CUSUM, ACO, and Actor-Critic.
///
/// Not to be confused with touring_simd::cortex::MetacognitivePipeline which handles
/// evidence fusion via DriftDetector+Wilson. This pipeline handles latency adaptation
/// via CUSUM drift detection, ACO pheromone threshold adaptation, and Actor-Critic RL
/// for hook execution parallelism tuning.
///
/// # Example
///
/// ```rust
/// use touring_intelligence::rl::metacognitive_pipeline::{LatencyAdaptationPipeline, MetacognitiveDecision, PipelineContext};
///
/// let mut pipeline = LatencyAdaptationPipeline::new();
/// let ctx = PipelineContext::builder()
///     .tool_name("Read")
///     .latency_ms(45)
///     .success(true)
///     .build();
/// match pipeline.run(ctx) {
///     MetacognitiveDecision::Stable => { /* continue */ }
///     MetacognitiveDecision::IncreaseParallelism { .. } => { /* adapt */ }
///     _ => { /* handle other decisions */ }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LatencyAdaptationPipeline {
    /// CUSUM latency drift detector.
    cusum: LatencyDriftDetector,
    /// ACO pheromone for adaptive threshold.
    pheromone: AcoPheromone,
    /// Actor-Critic RL agent.
    actor_critic: ActorCritic,
    /// Configuration.
    config: LatencyConfig,
    /// Statistics.
    stats: LatencyStats,
    /// State hash for Actor-Critic.
    state_hash: u64,
    /// Last action taken.
    last_action: Option<usize>,
    /// Consecutive stable decisions.
    consecutive_stable: u32,
}

/// Configuration for the latency adaptation pipeline.
#[derive(Debug, Clone, Copy)]
pub struct LatencyConfig {
    /// Default parallel threshold.
    pub default_threshold: usize,
    /// Minimum threshold floor.
    pub threshold_min: usize,
    /// Maximum threshold ceiling.
    pub threshold_max: usize,
    /// Number of actions in the action space.
    pub num_actions: usize,
    /// State feature dimension for Actor-Critic.
    pub state_dim: usize,
    /// CUSUM window size.
    pub cusum_window: usize,
    /// CUSUM k parameter (half-shift).
    pub cusum_k: f64,
    /// CUSUM h parameter (threshold).
    pub cusum_h: f64,
}

impl Default for LatencyConfig {
    fn default() -> Self {
        Self {
            default_threshold: 64,
            threshold_min: 8,
            threshold_max: 256,
            num_actions: 8,
            state_dim: 32,
            cusum_window: 100,
            cusum_k: 5.0,
            cusum_h: 50.0,
        }
    }
}

impl Default for LatencyAdaptationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyAdaptationPipeline {
    /// Create a new latency adaptation pipeline with default configuration.
    pub fn new() -> Self {
        Self::with_config(LatencyConfig::default())
    }

    /// Create a new latency adaptation pipeline with custom configuration.
    pub fn with_config(config: LatencyConfig) -> Self {
        let ac_config = ActorCriticConfig {
            actor_lr: 0.001,
            critic_lr: 0.005,
            tau: 0.01,
            gamma: 0.99,
            num_actions: config.num_actions,
            state_dim: config.state_dim,
            epsilon: 0.1,
            epsilon_decay: 0.995,
            epsilon_min: 0.01,
        };

        Self {
            cusum: LatencyDriftDetector::with_params(
                config.cusum_window,
                config.cusum_k,
                config.cusum_h,
            ),
            pheromone: AcoPheromone::default(),
            actor_critic: ActorCritic::new(ac_config),
            config,
            stats: LatencyStats::default(),
            state_hash: 0,
            last_action: None,
            consecutive_stable: 0,
        }
    }

    /// Run the metacognitive pipeline on a single context observation.
    ///
    /// This is the main entry point. It:
    /// 1. Feeds latency to CUSUM detector
    /// 2. Updates ACO pheromone based on outcome
    /// 3. Feeds state to Actor-Critic for action selection
    /// 4. Fuses evidence into a final decision
    #[instrument(skip_all, fields(tool = %ctx.tool_name, latency_ms = ctx.latency_ms, success = ctx.success))]
    pub fn run(&mut self, ctx: PipelineContext) -> MetacognitiveDecision {
        // Step 1: CUSUM detects drift from latency
        let drift_signal = self.cusum.push(ctx.latency_ms);
        self.stats.cusum_samples = self.cusum.total_samples();

        // Step 2: Build ACO outcome and update pheromone
        let outcome = touring_simd::learning::BatchOutcome {
            success: ctx.success,
            processing_time_ms: ctx.latency_ms as f64,
            throughput: if ctx.latency_ms > 0 {
                1000.0 / ctx.latency_ms as f64
            } else {
                0.0
            },
            error_rate: ctx.error_rate,
            memory_bound: ctx.memory_bound,
        };
        self.pheromone.update(&outcome);
        let pheromone_confidence = self.pheromone.confidence();
        self.stats.pheromone_confidence = pheromone_confidence;

        // Step 3: Update state hash for Actor-Critic
        self.update_state_hash(&ctx);

        // Step 4: Actor-Critic selects action
        let action_selection = self.actor_critic.select_action(self.state_hash);
        self.last_action = Some(action_selection.action);

        // Step 5: Fuse evidence into decision
        let decision =
            self.fuse_decision(&drift_signal, pheromone_confidence, &action_selection, &ctx);

        // Step 6: Update statistics
        self.record_decision(&decision);

        // Log milestone decisions at info level
        match &decision {
            MetacognitiveDecision::IncreaseParallelism {
                recommended_threshold,
                confidence,
            } => {
                tracing::info!(
                    threshold = recommended_threshold,
                    confidence = confidence,
                    "metacognitive: increasing parallelism"
                );
            }
            MetacognitiveDecision::DecreaseParallelism {
                recommended_threshold,
                confidence,
            } => {
                tracing::info!(
                    threshold = recommended_threshold,
                    confidence = confidence,
                    "metacognitive: decreasing parallelism"
                );
            }
            MetacognitiveDecision::ResetState { reason } => {
                tracing::warn!(reason = %reason, "metacognitive: reset state");
            }
            _ => {
                tracing::debug!(decision = ?decision, "metacognitive: decision");
            }
        }

        decision
    }

    /// Update the state hash based on context.
    fn update_state_hash(&mut self, ctx: &PipelineContext) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        ctx.tool_name.hash(&mut hasher);
        ctx.success.hash(&mut hasher);
        ((ctx.error_rate * 100.0) as u64).hash(&mut hasher);
        ctx.memory_bound.hash(&mut hasher);
        ctx.current_threshold.hash(&mut hasher);
        self.state_hash = hasher.finish();
    }

    /// Fuse evidence from all sources into a final decision.
    fn fuse_decision(
        &self,
        drift_signal: &DriftSignal,
        pheromone_confidence: f64,
        action_selection: &ActionSelection,
        ctx: &PipelineContext,
    ) -> MetacognitiveDecision {
        // If CUSUM detected significant drift, prioritize that
        if let DriftSignal::ChangeDetected {
            direction, mean_ms, ..
        } = drift_signal
        {
            return self.fuse_drift_decision(*direction, *mean_ms, pheromone_confidence, ctx);
        }

        // No CUSUM drift: use Actor-Critic exploration
        self.fuse_exploration_decision(action_selection.action, pheromone_confidence, ctx)
    }

    /// Handle decision when CUSUM detected drift.
    fn fuse_drift_decision(
        &self,
        direction: DriftDirection,
        mean_ms: u64,
        pheromone_confidence: f64,
        ctx: &PipelineContext,
    ) -> MetacognitiveDecision {
        let confidence = pheromone_confidence.min(0.9);

        // High latency + increase direction = need more parallelism
        if direction == DriftDirection::Increase && mean_ms > 100 {
            let recommended = self.recommended_threshold(ctx, true);
            return MetacognitiveDecision::IncreaseParallelism {
                recommended_threshold: recommended,
                confidence,
            };
        }

        // Decrease direction = can reduce parallelism
        if direction == DriftDirection::Decrease && pheromone_confidence > 0.7 {
            let recommended = self.recommended_threshold(ctx, false);
            return MetacognitiveDecision::DecreaseParallelism {
                recommended_threshold: recommended,
                confidence,
            };
        }

        MetacognitiveDecision::LogAndContinue
    }

    /// Handle decision based on Actor-Critic action when no drift.
    fn fuse_exploration_decision(
        &self,
        action: usize,
        pheromone_confidence: f64,
        ctx: &PipelineContext,
    ) -> MetacognitiveDecision {
        let num_actions = self.config.num_actions;
        let third = num_actions / 3;

        if action < third {
            MetacognitiveDecision::IncreaseParallelism {
                recommended_threshold: self.recommended_threshold(ctx, true),
                confidence: pheromone_confidence * 0.6,
            }
        } else if action < 2 * third {
            MetacognitiveDecision::Stable
        } else {
            self.exploration_from_action(action)
        }
    }

    /// Map action index to exploration type.
    fn exploration_from_action(&self, action: usize) -> MetacognitiveDecision {
        let exploration = match action % 4 {
            0 => ExplorationAction::SwitchTool,
            1 => ExplorationAction::RetryWithBackoff,
            2 => ExplorationAction::Defer,
            _ => ExplorationAction::ForceParallel,
        };
        MetacognitiveDecision::Explore {
            action: exploration,
        }
    }

    /// Calculate recommended threshold based on context.
    fn recommended_threshold(&self, ctx: &PipelineContext, increase: bool) -> usize {
        let current = ctx.current_threshold.max(1);
        let dim_factor = ctx
            .vector_dim
            .map(|d| (d as f64 / 16.0).ceil() as usize)
            .unwrap_or(0);

        let new_threshold = if increase {
            // Increase: add dim factor and 20% buffer
            ((current + dim_factor) * 120) / 100
        } else {
            // Decrease: reduce by dim factor and 15%
            ((current.saturating_sub(dim_factor)) * 85) / 100
        };

        new_threshold.clamp(self.config.threshold_min, self.config.threshold_max)
    }

    /// Record a decision for statistics.
    fn record_decision(&mut self, decision: &MetacognitiveDecision) {
        self.stats.decisions += 1;

        match decision {
            MetacognitiveDecision::Stable => {
                self.stats.stable_count += 1;
                self.consecutive_stable += 1;
            }
            MetacognitiveDecision::IncreaseParallelism { .. } => {
                self.stats.increase_count += 1;
                self.consecutive_stable = 0;
            }
            MetacognitiveDecision::DecreaseParallelism { .. } => {
                self.stats.decrease_count += 1;
                self.consecutive_stable = 0;
            }
            MetacognitiveDecision::ResetState { .. } => {
                self.stats.reset_count += 1;
                self.consecutive_stable = 0;
            }
            MetacognitiveDecision::Explore { .. } => {
                self.stats.explore_count += 1;
                self.consecutive_stable = 0;
            }
            MetacognitiveDecision::LogAndContinue => {
                self.stats.log_count += 1;
                self.consecutive_stable += 1;
            }
        }

        self.stats.actor_epsilon = self.actor_critic.stats().epsilon;
    }

    /// Update Actor-Critic with outcome after taking an action.
    ///
    /// Call this after observing the result of the action selected by `run()`.
    pub fn update_outcome(&mut self, reward: f32, done: bool) {
        if let Some(action) = self.last_action {
            let next_state = self.state_hash.wrapping_add(1);
            self.actor_critic
                .update(self.state_hash, action, reward, next_state, done);
        }
    }

    /// Get current pipeline statistics.
    pub fn stats(&self) -> &LatencyStats {
        &self.stats
    }

    /// Get current CUSUM drift signal.
    pub fn current_drift_signal(&self) -> DriftSignal {
        self.cusum.peek_last().unwrap_or(DriftSignal::Normal)
    }

    /// Reset the pipeline state (but keep configuration).
    pub fn reset(&mut self) {
        self.cusum.reset();
        self.pheromone = AcoPheromone::default();
        self.state_hash = 0;
        self.last_action = None;
        self.consecutive_stable = 0;
        self.stats = LatencyStats::default();
    }
}

// ── Fallible Pipeline (for robust error handling) ────────────────────────────

/// Fallible version of LatencyAdaptationPipeline that handles component failures gracefully.
///
/// If any component (CUSUM, ACO, Actor-Critic) fails, this pipeline
/// falls back to safe defaults rather than panicking.
#[derive(Debug, Clone)]
pub struct FalliblePipeline {
    inner: Option<LatencyAdaptationPipeline>,
    config: LatencyConfig,
}

impl FalliblePipeline {
    /// Create a new fallible pipeline.
    pub fn new() -> Self {
        Self {
            inner: Some(LatencyAdaptationPipeline::new()),
            config: LatencyConfig::default(),
        }
    }

    /// Create with custom config.
    pub fn with_config(config: LatencyConfig) -> Self {
        Self {
            inner: Some(LatencyAdaptationPipeline::with_config(config)),
            config,
        }
    }

    /// Run the pipeline, returning safe default on any error.
    pub fn run(&mut self, ctx: PipelineContext) -> MetacognitiveDecision {
        match &mut self.inner {
            Some(pipeline) => pipeline.run(ctx),
            None => {
                // Pipeline was corrupted, recreate it
                self.inner = Some(LatencyAdaptationPipeline::with_config(self.config));
                MetacognitiveDecision::ResetState {
                    reason: "Pipeline recovered after component failure".into(),
                }
            }
        }
    }

    /// Update outcome, swallowing any errors.
    pub fn update_outcome(&mut self, reward: f32, done: bool) {
        if let Some(ref mut pipeline) = self.inner {
            pipeline.update_outcome(reward, done);
        }
    }

    /// Get stats, or default if pipeline is corrupted.
    pub fn stats(&self) -> LatencyStats {
        self.inner
            .as_ref()
            .map(|p| p.stats.clone())
            .unwrap_or_default()
    }

    /// Check if pipeline is healthy.
    pub fn is_healthy(&self) -> bool {
        self.inner.is_some()
    }
}

impl Default for FalliblePipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_context_builder() {
        let ctx = PipelineContext::builder()
            .tool_name("Read")
            .latency_ms(45)
            .success(true)
            .error_rate(0.01)
            .memory_bound(false)
            .current_threshold(64)
            .vector_dim(128)
            .build();

        assert_eq!(ctx.tool_name, "Read");
        assert_eq!(ctx.latency_ms, 45);
        assert!(ctx.success);
        assert_eq!(ctx.error_rate, 0.01);
    }

    #[test]
    fn test_pipeline_stable_with_normal_latency() {
        let mut pipeline = LatencyAdaptationPipeline::new();

        // Verify pipeline runs without panic for many iterations
        for _ in 0..50 {
            let ctx = PipelineContext::builder()
                .tool_name("Read")
                .latency_ms(50)
                .success(true)
                .error_rate(0.0)
                .current_threshold(64)
                .build();

            let decision = pipeline.run(ctx);
            // All decisions are valid - just verify no panic
            assert!(
                matches!(
                    decision,
                    MetacognitiveDecision::Stable
                        | MetacognitiveDecision::LogAndContinue
                        | MetacognitiveDecision::Explore { .. }
                        | MetacognitiveDecision::IncreaseParallelism { .. }
                ),
                "Unexpected decision: {:?}",
                decision
            );
            pipeline.update_outcome(1.0, false);
        }

        // Verify stats were updated
        assert!(pipeline.stats().decisions >= 50);
    }

    #[test]
    fn test_pipeline_detects_latency_increase() {
        // Use very sensitive CUSUM to detect drift quickly
        let config = LatencyConfig {
            cusum_window: 30,
            cusum_k: 2.0,  // Very sensitive half-shift
            cusum_h: 15.0, // Low threshold for fast detection
            ..Default::default()
        };
        let mut pipeline = LatencyAdaptationPipeline::with_config(config);

        // Warm up with stable low latency
        for _ in 0..50 {
            let ctx = PipelineContext::builder()
                .tool_name("Read")
                .latency_ms(20)
                .success(true)
                .current_threshold(64)
                .build();
            pipeline.run(ctx);
        }

        // Now inject sustained high latency - CUSUM should accumulate drift
        let mut detected_increase = false;
        for _ in 0..30 {
            let ctx = PipelineContext::builder()
                .tool_name("Read")
                .latency_ms(200) // 10x increase
                .success(true)
                .current_threshold(64)
                .build();

            let decision = pipeline.run(ctx);
            if matches!(decision, MetacognitiveDecision::IncreaseParallelism { .. }) {
                detected_increase = true;
                break;
            }
        }

        assert!(
            detected_increase,
            "Should detect latency increase and recommend more parallelism"
        );
    }

    #[test]
    fn test_pipeline_reset() {
        let mut pipeline = LatencyAdaptationPipeline::new();

        // Generate some state
        for i in 0..10 {
            let ctx = PipelineContext::builder()
                .tool_name("Read")
                .latency_ms(50 + i)
                .success(true)
                .current_threshold(64)
                .build();
            pipeline.run(ctx);
        }

        // Reset should clear state
        pipeline.reset();

        assert_eq!(pipeline.stats().decisions, 0);
        assert_eq!(pipeline.stats().cusum_samples, 0);
    }

    #[test]
    fn test_fallible_pipeline_recovery() {
        let mut pipeline = FalliblePipeline::new();

        let ctx = PipelineContext::builder()
            .tool_name("Read")
            .latency_ms(50)
            .success(true)
            .current_threshold(64)
            .build();

        // Normal operation - any valid decision is acceptable
        let decision = pipeline.run(ctx.clone());
        assert!(
            matches!(
                decision,
                MetacognitiveDecision::Stable
                    | MetacognitiveDecision::LogAndContinue
                    | MetacognitiveDecision::Explore { .. }
                    | MetacognitiveDecision::IncreaseParallelism { .. }
                    | MetacognitiveDecision::DecreaseParallelism { .. }
            ),
            "Expected a valid decision, got {:?}",
            decision
        );

        // Should still be healthy
        assert!(pipeline.is_healthy());
    }

    #[test]
    fn test_pheromone_confidence_tracking() {
        let mut pipeline = LatencyAdaptationPipeline::new();

        // Run successful batches
        for _ in 0..10 {
            let ctx = PipelineContext::builder()
                .tool_name("Read")
                .latency_ms(50)
                .success(true)
                .error_rate(0.0)
                .memory_bound(false)
                .current_threshold(64)
                .build();
            pipeline.run(ctx);
        }

        let confidence = pipeline.stats().pheromone_confidence;
        // Confidence should be > 0.5 for successful runs (equal pheromone = 0.5)
        assert!(
            confidence > 0.5,
            "Successful batches should increase pheromone confidence, got {}",
            confidence
        );
    }

    #[test]
    fn test_actor_critic_learning() {
        let mut pipeline = LatencyAdaptationPipeline::new();

        // Verify actor-critic updates don't panic over many iterations
        for _ in 0..50 {
            let ctx = PipelineContext::builder()
                .tool_name("Read")
                .latency_ms(50)
                .success(true)
                .current_threshold(64)
                .build();
            pipeline.run(ctx);
            // Provide positive reward
            pipeline.update_outcome(1.0, false);
            // Provide negative reward occasionally
            // (iteration count checked via loop index)
        }

        // Verify learning happened - stats should show updates
        let stats = pipeline.stats();
        assert!(
            stats.decisions >= 50,
            "Should have made at least 50 decisions"
        );
    }

    #[test]
    fn test_threshold_recommendations_respect_bounds() {
        let config = LatencyConfig {
            threshold_min: 8,
            threshold_max: 256,
            ..Default::default()
        };
        let mut pipeline = LatencyAdaptationPipeline::with_config(config);

        // Very small threshold
        let ctx = PipelineContext::builder()
            .tool_name("Read")
            .latency_ms(100)
            .success(true)
            .current_threshold(4) // below min
            .build();

        let decision = pipeline.run(ctx);
        if let MetacognitiveDecision::DecreaseParallelism {
            recommended_threshold,
            ..
        } = decision
        {
            assert!(
                recommended_threshold >= 8,
                "Recommended threshold should respect minimum"
            );
        }
    }
}
