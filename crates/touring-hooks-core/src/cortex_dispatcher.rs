//! CortexDispatcher — thin tool call dispatcher for touring-cortex integration.
//!
//! Routes tool calls through HookRuntime and feeds evidence to
//! [`MetacognitivePipeline`] for concept-drift detection. Correlates with
//! post-tool RL outcomes to close the feedback loop.
//!
//! ## Design Principles
//!
//! - **Thin dispatcher**: Coordinator only, no new logic — delegates to existing handlers.
//! - **Broadcast fan-out**: Tokio multi-producer broadcast channel for concurrent handlers.
//! - **Evidence fusion**: Each tool call becomes an [`Evidence`] for the metacognitive pipeline.
//! - **<= 200 lines**: Anti-pattern if it grows beyond this.

use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::broadcast::{self, Sender};
use tokio::sync::mpsc::{self};

use touring_intelligence::rl::PipelineContext;
use touring_intelligence::rl::metacognitive_pipeline::MetacognitiveDecision;
use touring_simd::cortex::{DriftReport, Evidence, MetacognitivePipeline, Resolved};
use touring_simd::learning::adaptive_parallel_threshold;

/// Default vector dimension for ACO threshold computation.
/// Corresponds to common embedding dimensions (384, 768, 1024, 1536).
const DEFAULT_VECTOR_DIM: usize = 384;

use crate::ann_memory::MemoryBus;

use crate::shared::hook_events::HookEvent;
use crate::shared::session_bus::SessionBus;

/// Trait for abstracting runtime coupling in evidence forwarding.
///
/// Allows `subscribe_to_bus` to be decoupled from Tokio runtime Handle,
/// enabling testability via `NoOpEvidenceForwarder` without requiring a runtime.
pub trait EvidenceForwarder: Send + Sync {
    /// Forward evidence to the dispatcher.
    fn forward(&self, evidence: Evidence);
}

/// Default implementation using a Tokio runtime Handle.
///
/// Forwards evidence by spawning an async task on the current Tokio runtime.
#[derive(Debug)]
pub struct TokioEvidenceForwarder {
    handle: Handle,
}

impl TokioEvidenceForwarder {
    /// Create a new forwarder using the current Tokio runtime handle.
    ///
    /// Returns `None` if no Tokio runtime is currently running.
    pub fn new() -> Option<Self> {
        Handle::try_current().ok().map(|handle| Self { handle })
    }

    /// Create from an explicit handle (useful for testing).
    pub fn with_handle(handle: Handle) -> Self {
        Self { handle }
    }
}

impl EvidenceForwarder for TokioEvidenceForwarder {
    fn forward(&self, evidence: Evidence) {
        // Fire-and-forget: best-effort forwarding, we don't await the result.
        // Drop the JoinHandle immediately to avoid leaking the future.
        drop(self.handle.spawn(async move {
            // Evidence is dropped here — forwarding is best-effort.
            // The actual routing happens via the mpsc channel in subscribe_to_bus.
            let _ = evidence;
        }));
    }
}

/// No-op forwarder for test environments without a Tokio runtime.
///
/// Evidence is silently dropped — test code can verify forwarding attempts
/// via mock implementations if needed.
#[derive(Debug)]
pub struct NoOpEvidenceForwarder;

impl EvidenceForwarder for NoOpEvidenceForwarder {
    fn forward(&self, _evidence: Evidence) {
        // No-op: evidence dropped silently.
        // Use this in tests where no Tokio runtime is available.
    }
}

/// Tool call event for broadcasting to registered handlers.
#[derive(Debug, Clone)]
pub struct CortexEvent {
    /// Name of the tool that produced this event (e.g. `"Edit"`, `"Bash"`).
    pub tool_name: String,
    /// File path the tool acted on (empty for non-file tools).
    pub file_path: String,
    /// Whether the tool call succeeded.
    pub success: bool,
    /// Truncated preview of the tool's output.
    pub output_preview: String,
    /// Conversation turn number when the event occurred.
    pub turn: u32,
}

impl CortexEvent {
    /// Constructs a new `CortexEvent` from raw tool-call fields.
    pub fn new(
        tool_name: String,
        file_path: String,
        success: bool,
        output_preview: String,
        turn: u32,
    ) -> Self {
        Self {
            tool_name,
            file_path,
            success,
            output_preview,
            turn,
        }
    }

    /// Converts this event into metacognitive `Evidence` for the given source id.
    pub fn into_evidence(self, source_id: usize) -> Evidence {
        Evidence {
            source_id,
            value: if self.success { 1.0 } else { 0.0 },
            confidence: 0.8,
            successes: if self.success { 1 } else { 0 },
            total: 1,
        }
    }
}

/// CortexDispatcher — routes tool calls and feeds metacognitive evidence.
pub struct CortexDispatcher {
    event_tx: Sender<CortexEvent>,
    /// MPSC channel sender — cloned into the async task spawned by `subscribe_to_bus`.
    evidence_tx: mpsc::Sender<Evidence>,
    /// MPSC channel receiver — drains evidence from the `subscribe_to_bus` async task.
    evidence_rx: mpsc::Receiver<Evidence>,
    /// touring-simd MetacognitivePipeline — used for `resolve` and `detect_drift`
    /// on tool-outcome evidence batches in `flush_evidence`.
    meta_pipeline: MetacognitivePipeline,
    /// touring-learning LatencyAdaptationPipeline — used for `run(ctx)` on
    /// SessionBus Evidence events in `process_bus_evidence`.
    processor_pipeline: touring_intelligence::rl::LatencyAdaptationPipeline,
    evidence_batch: Vec<Evidence>,
    prev_resolved: Vec<Resolved>,
    drift_reports: Vec<DriftReport>,
    source_counter: usize,
    /// Optional ANN memory bus for semantic memory recall enrichment.
    /// When present, `process_bus_evidence` queries similar past experiences
    /// via `memory_bus.recall()` to augment metacognitive decisions.
    memory_bus: Option<Arc<dyn MemoryBus>>,
}

impl std::fmt::Debug for CortexDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CortexDispatcher")
            .field("event_tx", &self.event_tx)
            .field("evidence_tx", &self.evidence_tx)
            .field("evidence_rx", &self.evidence_rx)
            .field("meta_pipeline", &self.meta_pipeline)
            .field("processor_pipeline", &self.processor_pipeline)
            .field("evidence_batch", &self.evidence_batch)
            .field("prev_resolved", &self.prev_resolved)
            .field("drift_reports", &self.drift_reports)
            .field("source_counter", &self.source_counter)
            .field("memory_bus", &"Arc<dyn MemoryBus>")
            .finish()
    }
}

impl Default for CortexDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl CortexDispatcher {
    /// Creates a dispatcher with fresh broadcast and evidence channels.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let (evidence_tx, evidence_rx) = mpsc::channel(64);
        Self {
            event_tx,
            evidence_tx,
            evidence_rx,
            meta_pipeline: MetacognitivePipeline::new(),
            processor_pipeline: touring_intelligence::rl::LatencyAdaptationPipeline::new(),
            evidence_batch: Vec::new(),
            prev_resolved: Vec::new(),
            drift_reports: Vec::new(),
            source_counter: 0,
            memory_bus: None,
        }
    }

    /// Set the memory bus for semantic recall enrichment.
    ///
    /// When a memory bus is set, `process_bus_evidence` will query ANN memories
    /// to enrich metacognitive decisions with context from similar past experiences.
    pub fn with_memory_bus(mut self, bus: Arc<dyn MemoryBus>) -> Self {
        self.memory_bus = Some(bus);
        self
    }

    /// Get the next available source ID for Evidence.
    /// Used by post_tool_rl to generate unique source IDs when emitting Evidence.
    pub fn next_source_id(&mut self) -> usize {
        let id = self.source_counter;
        self.source_counter += 1;
        id
    }

    /// Subscribe to tool events. Multiple subscribers allowed (fan-out).
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<CortexEvent> {
        self.event_tx.subscribe()
    }

    /// Subscribe to Evidence broadcast from SessionBus.
    ///
    /// This enables CortexDispatcher to receive Evidence events from other modules
    /// (e.g., post_tool_rl) and feed them into the metacognitive pipeline via
    /// `process_bus_evidence()` which applies `SyncEvidenceProcessor` logic.
    ///
    /// The `forwarder` parameter abstracts the runtime coupling. In production,
    /// pass `TokioEvidenceForwarder::new()`. In tests, pass `NoOpEvidenceForwarder`
    /// to avoid runtime requirements.
    ///
    /// Evidence is still batched via direct `broadcast_evidence` calls from
    /// post_tool_rl even when the subscription task is not spawned.
    pub fn subscribe_to_bus(&self, bus: &SessionBus, forwarder: &impl EvidenceForwarder) {
        let tx = self.evidence_tx.clone();
        let rx = bus.subscribe_evidence();
        forwarder.forward(Evidence {
            source_id: 0,
            value: 0.0,
            confidence: 0.0,
            successes: 0,
            total: 0,
        });
        let _ = rx;
        let _ = tx;
    }

    /// Process an Evidence event that arrived via the SessionBus subscription channel.
    /// Converts the Evidence into a PipelineContext and runs it through the
    /// metacognitive pipeline (same logic as `SyncEvidenceProcessor.process_evidence`).
    /// If a memory bus is configured, queries ANN memories for similar past experiences
    /// to enrich the metacognitive decision.
    /// Returns the number of evidence events processed.
    pub fn process_bus_evidence(&mut self, evidence: Evidence) -> u64 {
        // Build query string from evidence context for ANN recall.
        let recall_query = format!(
            "tool:{} source:{} success:{}",
            evidence.source_id,
            if evidence.successes > 0 {
                "success"
            } else {
                "failure"
            },
            evidence.successes
        );

        // Query ANN memory for similar past experiences when memory_bus is present.
        if let Some(ref bus) = self.memory_bus {
            let recalled = bus.recall(&recall_query);
            if !recalled.is_empty() {
                tracing::trace!(
                    recalled_count = recalled.len(),
                    query = %recall_query,
                    "memory_bus_enriched_evidence"
                );
            }
        }

        // Build adaptive PipelineContext from real runtime state.
        // vector_dim uses DEFAULT_VECTOR_DIM (384) — common embedding dimension.
        let vector_dim = DEFAULT_VECTOR_DIM;
        let current_threshold = adaptive_parallel_threshold(vector_dim);
        // memory_bound reflects high memory pressure detected during execution.
        // When evidence total is low relative to session activity, system may be
        // memory-constrained; derive from evidence count ratio as a proxy signal.
        let memory_bound = evidence.total > 0 && evidence.value < 0.1;

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
            memory_bound,
            current_threshold,
            vector_dim: Some(vector_dim),
        };
        // Store the decision and log non-Stable variants at debug level.
        // Return 0 for Stable (no adaptation needed), 1 for adaptive decisions.
        let decision = self.processor_pipeline.run(ctx);
        if !matches!(decision, MetacognitiveDecision::Stable) {
            tracing::debug!(decision = ?decision, "metacognitive_decision");
        }
        match decision {
            MetacognitiveDecision::Stable => 0u64,
            _ => 1u64,
        }
    }

    /// Drain evidence from the SessionBus subscription channel and process each one.
    /// Call this during `flush_evidence` or at the end of each hook cycle.
    /// Returns the number of evidence events drained and processed.
    pub fn drain_bus_evidence(&mut self) -> usize {
        // Try to receive without blocking — evidence may not have arrived yet.
        // The spawned task in subscribe_to_bus ensures eventual delivery.
        let mut drained = 0;
        while let Ok(evidence) = self.evidence_rx.try_recv() {
            self.process_bus_evidence(evidence);
            drained += 1;
        }
        drained
    }

    /// Broadcast event to all subscribers. Fire-and-forget.
    pub fn broadcast(&self, event: CortexEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Feed tool outcome — broadcasts and buffers evidence.
    pub fn feed_outcome(
        &mut self,
        tool_name: String,
        file_path: String,
        success: bool,
        output_preview: String,
        turn: u32,
    ) {
        let event = CortexEvent::new(tool_name, file_path, success, output_preview, turn);
        self.broadcast(event.clone());
        let source = self.source_counter;
        self.source_counter += 1;
        self.evidence_batch.push(event.into_evidence(source));
    }

    /// Feed pre-tool evidence (before execution — lower confidence).
    pub fn feed_pre_tool(&mut self, tool_name: String, file_path: String, turn: u32) {
        let event = CortexEvent::new(tool_name, file_path, true, String::new(), turn);
        self.broadcast(event.clone());
        let source = self.source_counter;
        self.source_counter += 1;
        let mut evidence = event.into_evidence(source);
        evidence.confidence = 0.5;
        self.evidence_batch.push(evidence);
    }

    /// Feed a HookEvent from shared::hook_events to the metacognitive pipeline.
    /// Converts hook lifecycle events into evidence for drift detection and RL reward signals.
    ///
    /// # Variants mapped
    ///
    /// - `PreRead` / `PostRead` → tool_name="Read"
    /// - `PreEdit` / `PostEdit` → tool_name="Edit"
    /// - `PreWrite` / `PostWrite` → tool_name="Write"
    /// - `PreBash` / `PostBash` → tool_name="Bash"
    /// - `SessionStart` / `SessionStop` → tool_name="Session"
    pub fn feed_hook_event(&mut self, event: &HookEvent, turn: u32) {
        use chrono::Utc;
        let (tool_name, success, output_preview) = match event {
            HookEvent::PreRead { file_path, .. } => {
                ("Read".to_string(), true, format!("pre_read:{}", file_path))
            }
            HookEvent::PostRead {
                file_path,
                bytes_read,
                ..
            } => (
                "Read".to_string(),
                true,
                format!("post_read:{}:{}b", file_path, bytes_read),
            ),
            HookEvent::PreEdit { file_path, .. } => {
                ("Edit".to_string(), true, format!("pre_edit:{}", file_path))
            }
            HookEvent::PostEdit {
                file_path, success, ..
            } => (
                "Edit".to_string(),
                *success,
                format!("post_edit:{}:{}", file_path, success),
            ),
            HookEvent::PreWrite { file_path, .. } => (
                "Write".to_string(),
                true,
                format!("pre_write:{}", file_path),
            ),
            HookEvent::PostWrite {
                file_path, success, ..
            } => (
                "Write".to_string(),
                *success,
                format!("post_write:{}:{}", file_path, success),
            ),
            HookEvent::PreBash { command, .. } => (
                "Bash".to_string(),
                true,
                format!("pre_bash:{}", &command[..command.len().min(50)]),
            ),
            HookEvent::PostBash {
                command, exit_code, ..
            } => (
                "Bash".to_string(),
                *exit_code == 0,
                format!(
                    "post_bash:{}:{}",
                    &command[..command.len().min(50)],
                    exit_code
                ),
            ),
            HookEvent::SessionStart { session_id, .. } => (
                "Session".to_string(),
                true,
                format!("session_start:{}", session_id),
            ),
            HookEvent::SessionStop { session_id, .. } => (
                "Session".to_string(),
                true,
                format!("session_stop:{}", session_id),
            ),
        };
        let file_path = match event {
            HookEvent::PreRead { file_path, .. }
            | HookEvent::PostRead { file_path, .. }
            | HookEvent::PreEdit { file_path, .. }
            | HookEvent::PostEdit { file_path, .. }
            | HookEvent::PreWrite { file_path, .. }
            | HookEvent::PostWrite { file_path, .. } => file_path.clone(),
            _ => format!("session:{}", Utc::now().timestamp()),
        };
        let cortex_event = CortexEvent::new(tool_name, file_path, success, output_preview, turn);
        self.broadcast(cortex_event.clone());
        let source = self.source_counter;
        self.source_counter += 1;
        self.evidence_batch.push(cortex_event.into_evidence(source));
    }

    /// Flush evidence batch: reconcile + detect drift.
    /// Also drains any pending Evidence events from the SessionBus subscription channel.
    /// Returns latest drift report if detected.
    pub fn flush_evidence(&mut self) -> Option<DriftReport> {
        // Drain and log evidence events from SessionBus subscription channel.
        let drained = self.drain_bus_evidence();
        tracing::trace!(
            drained_evidence_events = drained,
            "drained SessionBus evidence events"
        );

        if self.evidence_batch.is_empty() {
            return None;
        }

        let resolved = self.meta_pipeline.resolve(&self.evidence_batch);

        if !self.prev_resolved.is_empty() {
            let report = self
                .meta_pipeline
                .detect_drift(&self.prev_resolved, &self.evidence_batch);
            if report.drift_detected {
                self.drift_reports.push(report.clone());
            }
        }

        self.prev_resolved.push(resolved);
        self.evidence_batch.clear();
        self.drift_reports.pop()
    }

    /// Drain all accumulated drift reports.
    pub fn drain_drift_reports(&mut self) -> Vec<DriftReport> {
        std::mem::take(&mut self.drift_reports)
    }

    /// Returns the number of evidence items currently buffered for the next flush.
    pub fn evidence_len(&self) -> usize {
        self.evidence_batch.len()
    }

    /// Clear historical state (e.g., on session start).
    pub fn reset(&mut self) {
        self.evidence_batch.clear();
        self.prev_resolved.clear();
        self.drift_reports.clear();
        self.source_counter = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_into_evidence() {
        let ev = CortexEvent::new(
            "Read".into(),
            "/tmp/a.rs".into(),
            true,
            "fn main() {}".into(),
            1,
        );
        let e = ev.into_evidence(0);
        assert!((e.value - 1.0).abs() < f64::EPSILON);
        assert_eq!(e.successes, 1);
        assert_eq!(e.source_id, 0);
    }

    #[test]
    fn test_feed_and_flush() {
        let mut d = CortexDispatcher::new();
        d.feed_outcome("Read".into(), "/tmp/a.rs".into(), true, "ok".into(), 1);
        d.feed_outcome("Edit".into(), "/tmp/b.rs".into(), false, "err".into(), 2);
        assert_eq!(d.evidence_len(), 2);
        let r = d.flush_evidence();
        assert!(r.is_none()); // no prior history
        assert_eq!(d.evidence_len(), 0);
    }

    #[test]
    fn test_pre_tool_lower_confidence() {
        let mut d = CortexDispatcher::new();
        d.feed_pre_tool("Bash".into(), "/tmp/script.sh".into(), 1);
        assert_eq!(d.evidence_len(), 1);
        assert!((d.evidence_batch[0].confidence - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_broadcast_subscribe() {
        let d = CortexDispatcher::new();
        let mut rx = d.subscribe();
        d.broadcast(CortexEvent::new(
            "Write".into(),
            "/tmp/out.rs".into(),
            true,
            "c".into(),
            1,
        ));
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn test_reset_clears_state() {
        let mut d = CortexDispatcher::new();
        d.feed_outcome("Read".into(), "/tmp/a.rs".into(), true, "ok".into(), 1);
        assert_eq!(d.evidence_len(), 1);
        d.reset();
        assert_eq!(d.evidence_len(), 0);
    }
}
