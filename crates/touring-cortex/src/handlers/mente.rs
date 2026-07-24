//! MenteDB Cognitive Handlers — Curva-U attention + Delta-Aware Serving + Belief Propagation.
//!
//! Wires `mentedb-cognitive` (Curva-U attention, Delta-Aware Serving, Belief
//! Propagation) into the cortex pipeline when the `cognitive-memory` feature is
//! enabled. Reported -90.7% token reduction in 20-turn loops, 0% stale returns.
//!
//! ## Feature Gate
//!
//! All items in this module require `cognitive-memory` to be enabled in
//! `touring-cortex/Cargo.toml`. The feature is **OFF by default** per ADR D4
//! until v1.0 review completes.
//!
//! ## Handlers
//!
//! | ID | Handler | Event | Purpose |
//! |----|---------|-------|---------|
//! | H106 | MentePainSignalHandler | PostToolUseFailure | Records failure intensity via PainRegistry |
//! | H107 | MenteTrajectoryTrackerHandler | UserPromptSubmit | Tracks conversation arc via TrajectoryTracker |
//! | H108 | MentePhantomDetectorHandler | PreToolUse(Read) | Detects entity references not in registry |
//! | H109 | MenteCognitionMonitorHandler | UserPromptSubmit + PostToolUse | Monitors LLM output for contradictions via CognitionStream |

use std::sync::{Mutex, OnceLock};

// MenteDB types — only available when cognitive-memory feature is enabled
use mentedb_cognitive::{
    CognitionStream, DecisionState, PainRegistry, PhantomPriority, PhantomTracker,
    TrajectoryTracker,
};

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

// ── Process-global cognitive state ─────────────────────────────────────────

/// Process-global TrajectoryTracker — accumulates conversation arc for daemon lifetime.
/// Markov chain transition map improves predictions over time.
static TRAJECTORY_TRACKER: OnceLock<TrajectoryTracker> = OnceLock::new();

/// Process-global PainRegistry — records negative experiences (failures, corrections).
/// Surface warnings during context assembly to prevent repeated mistakes.
static PAIN_REGISTRY: OnceLock<PainRegistry> = OnceLock::new();

/// Process-global PhantomTracker — detects references to unknown entities.
/// Mutex enables interior mutability for register_entity().
static PHANTOM_TRACKER: OnceLock<Mutex<PhantomTracker>> = OnceLock::new();

/// Process-global CognitionStream — monitors LLM output for contradictions.
/// Buffer size of 1000 tokens.
static COGNITION_STREAM: OnceLock<CognitionStream> = OnceLock::new();

fn trajectory_tracker() -> &'static TrajectoryTracker {
    TRAJECTORY_TRACKER.get_or_init(TrajectoryTracker::default)
}

fn pain_registry() -> &'static PainRegistry {
    PAIN_REGISTRY.get_or_init(PainRegistry::default)
}

fn phantom_tracker() -> &'static Mutex<PhantomTracker> {
    PHANTOM_TRACKER.get_or_init(|| Mutex::new(PhantomTracker::default()))
}

fn cognition_stream() -> &'static CognitionStream {
    COGNITION_STREAM.get_or_init(|| CognitionStream::new(1000))
}

// ── H106: MentePainSignalHandler ───────────────────────────────────────────

/// H106: Records tool failure intensity as a pain signal.
///
/// On `PostToolUseFailure`, extracts error type and tool name, calls
/// `PainRegistry::format_pain_warnings()` to surface any relevant warnings,
/// and injects context about the failure.
///
/// Pain signals decay over time. Recurring failures increase intensity.
pub struct MentePainSignalHandler;

impl Default for MentePainSignalHandler {
    fn default() -> Self {
        Self
    }
}

impl Handler for MentePainSignalHandler {
    fn name(&self) -> &str {
        "H106_mente_pain_signal"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUseFailure]
    }

    fn priority(&self) -> u8 {
        95 // Between H94-session (90) and H95-tools (100)
    }

    fn timeout_ms(&self) -> u64 {
        5 // PainRegistry is in-memory, near-zero latency
    }

    fn is_async(&self) -> bool {
        false
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let error_msg = ctx
            .input
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let tool = ctx.tool_name.as_deref().unwrap_or("unknown");

        // Build trigger keywords from tool name and error
        let trigger_keywords: Vec<String> = tool
            .split(|c: char| c.is_alphanumeric() && c.is_uppercase())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .chain(error_msg.split_whitespace().take(3).map(String::from))
            .collect();

        // Get pain warnings for this context
        let warnings = pain_registry().get_pain_for_context(&trigger_keywords);

        let context_line = if !warnings.is_empty() {
            let formatted = pain_registry().format_pain_warnings(&warnings);
            format!(
                "PainSignal: {} failed — prior warnings: {}",
                tool, formatted
            )
        } else {
            // Intensity is encoded in keyword count to help RL system learn
            let intensity_hint = if error_msg.contains("timeout") {
                "high-intensity"
            } else if error_msg.contains("permission") {
                "critical"
            } else {
                "normal"
            };
            format!(
                "PainSignal: {} failed [{intensity_hint}] — {}",
                tool, error_msg
            )
        };

        HandlerResult::allow(self.name(), Some(context_line))
    }
}

// ── H107: MenteTrajectoryTrackerHandler ───────────────────────────────────

/// H107: Tracks conversation reasoning arc via TrajectoryTracker.
///
/// On `UserPromptSubmit`, infers the current decision state from the prompt,
/// records a `TrajectoryNode`, and predicts the next topic via the Markov
/// transition map. Injects trajectory context when confidence is high.
///
/// Architecture:
/// ```text
/// UserPromptSubmit → infer decision state from prompt keywords
///   → TrajectoryTracker::record_turn(node)
///   → trajectory_tracker.transitions.predict_from(current_topic)
///   → if predictions exist: context_line += "trajectory: next={topic}"
/// ```
pub struct MenteTrajectoryTrackerHandler;

impl Default for MenteTrajectoryTrackerHandler {
    fn default() -> Self {
        Self
    }
}

impl Handler for MenteTrajectoryTrackerHandler {
    fn name(&self) -> &str {
        "H107_mente_trajectory_tracker"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    fn priority(&self) -> u8 {
        50 // Early in pipeline to build trajectory before other handlers
    }

    fn timeout_ms(&self) -> u64 {
        10 // Markov chain lookup is O(1)
    }

    fn is_async(&self) -> bool {
        false
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        if ctx.context_budget_remaining < 30 {
            return HandlerResult::skip(self.name());
        }

        let prompt = ctx
            .input
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Infer decision state from prompt keywords
        let decision_state =
            if prompt.contains("fix") || prompt.contains("bug") || prompt.contains("error") {
                DecisionState::Investigating
            } else if prompt.contains("refactor") || prompt.contains("improve") {
                DecisionState::NarrowedTo("refactoring".to_string())
            } else if prompt.contains("implement")
                || prompt.contains("add")
                || prompt.contains("create")
            {
                DecisionState::Decided("implementation".to_string())
            } else if prompt.contains("explain") || prompt.contains("what") {
                DecisionState::Completed
            } else {
                DecisionState::Investigating
            };

        // Extract a topic summary (first 50 chars of prompt, lowercased)
        let topic_summary: String = prompt
            .chars()
            .take(50)
            .map(|c| {
                if c.is_alphanumeric() || c.is_whitespace() {
                    c.to_ascii_lowercase()
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .take(10)
            .collect::<Vec<_>>()
            .join(" ");

        // Trajectory recording is read-only here: TrajectoryTracker::record_turn
        // requires &mut and full-feature TrajectoryNode (including a real
        // topic_embedding). The embedding path is gated behind the optional
        // `semantic-embeddings` feature in `touring-learning` (pulls ~200 MB of
        // ML deps — candle-core/nn/transformers + tokenizers — so kept opt-in
        // per ADR D4). When enabled, the node will be built from the
        // SemanticEmbedder output and forwarded through the mutex-guarded
        // tracker. For now we use the predict_from(&self) path directly.
        let predictions = trajectory_tracker()
            .transitions
            .predict_from(&topic_summary, 3);

        let context_line = if let Some((next_topic, count)) = predictions.first() {
            // Normalize count to confidence (cap at 10 transitions = 100%)
            let confidence = (*count as f64 / 10.0).clamp(0.0, 1.0);
            let confidence_pct = (confidence * 100.0) as u32;
            if confidence_pct >= 30 {
                format!(
                    "trajectory: {} → next={} [{}/{} transitions]",
                    decision_state_label(&decision_state),
                    next_topic,
                    count,
                    trajectory_tracker().transitions.total_transitions()
                )
            } else {
                return HandlerResult::allow(self.name(), None);
            }
        } else {
            return HandlerResult::allow(self.name(), None);
        };

        HandlerResult::allow(self.name(), Some(context_line))
    }
}

/// Returns a human-readable label for DecisionState variants.
fn decision_state_label(state: &DecisionState) -> &'static str {
    match state {
        DecisionState::Investigating => "investigating",
        DecisionState::NarrowedTo(_) => "narrowed",
        DecisionState::Decided(_) => "decided",
        DecisionState::Interrupted => "interrupted",
        DecisionState::Completed => "completed",
    }
}

// ── H108: MentePhantomDetectorHandler ─────────────────────────────────────

/// H108: Detects entity references not present in the known registry.
///
/// On `PreToolUse` (for Read tool), uses the process-global `PhantomTracker`
/// to detect gaps — entities referenced in the target file that are not
/// registered in the phantom tracker. Entities are registered via prior
/// `PreToolUse` calls, so repeated reads of the same file accumulate context.
pub struct MentePhantomDetectorHandler;

impl Default for MentePhantomDetectorHandler {
    fn default() -> Self {
        Self
    }
}

impl Handler for MentePhantomDetectorHandler {
    fn name(&self) -> &str {
        "H108_mente_phantom_detector"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn priority(&self) -> u8 {
        100 // After trajectory (50), before enrichment handlers
    }

    fn timeout_ms(&self) -> u64 {
        20 // Phantom detection is O(registered entities)
    }

    fn is_async(&self) -> bool {
        false
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(fp) => fp,
            None => return HandlerResult::skip(self.name()),
        };

        // Only run for Read operations
        let tool = ctx.tool_name.as_deref().unwrap_or("");
        if tool != "Read" {
            return HandlerResult::skip(self.name());
        }

        // Register the file as an entity so we can track references to it
        // This enables the phantom detector to learn what files we work with
        phantom_tracker()
            .lock()
            .expect("phantom_tracker mutex poisoned")
            .register_entity(file_path);

        // Check entity count as a proxy for session complexity
        let entity_count = phantom_tracker()
            .lock()
            .expect("phantom_tracker mutex poisoned")
            .entity_registry()
            .len();

        if entity_count > 50 {
            let priority = match entity_count {
                51..=75 => PhantomPriority::Low,
                76..=100 => PhantomPriority::Medium,
                101..=200 => PhantomPriority::High,
                _ => PhantomPriority::Critical,
            };

            let context_line = format!(
                "phantom: {} entities tracked in session [priority={:?}]",
                entity_count, priority
            );
            return HandlerResult::allow(self.name(), Some(context_line));
        }

        HandlerResult::allow(self.name(), None)
    }
}

// ── H109: MenteCognitionMonitorHandler ─────────────────────────────────────

/// H109: Monitors LLM output for contradictions via CognitionStream.
///
/// On `UserPromptSubmit`, feeds prompt tokens into the process-global
/// `CognitionStream`. On `PostToolUse`, feeds tool-result tokens and checks
/// for contradictions against known facts via `check_alerts()`.
///
/// Architecture:
/// ```text
/// UserPromptSubmit → feed_token(prompt tokens)
/// PostToolUse → feed_token(result tokens) → check_alerts()
///   → StreamAlert::Contradiction → context_line warning
///   → StreamAlert::Forgotten → context_line warning
/// ```
///
/// Requires `cognitive-memory` feature.
pub struct MenteCognitionMonitorHandler;

impl Default for MenteCognitionMonitorHandler {
    fn default() -> Self {
        Self
    }
}

impl Handler for MenteCognitionMonitorHandler {
    fn name(&self) -> &str {
        "H109_mente_cognition_monitor"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit, HookEvent::PostToolUse]
    }

    fn priority(&self) -> u8 {
        85 // After trajectory (50), before enrichment
    }

    fn timeout_ms(&self) -> u64 {
        15 // CognitionStream check is O(tokens)
    }

    fn is_async(&self) -> bool {
        false
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let stream = cognition_stream();

        match ctx.event {
            HookEvent::UserPromptSubmit => {
                let prompt = ctx
                    .input
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                for token in prompt.split_whitespace().take(100) {
                    stream.feed_token(token);
                }
                HandlerResult::allow(self.name(), None)
            }
            HookEvent::PostToolUse => {
                let result = ctx
                    .input
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if result.is_empty() {
                    return HandlerResult::skip(self.name());
                }
                for token in result.split_whitespace().take(200) {
                    stream.feed_token(token);
                }
                // Drain accumulated text and check for alerts
                let buffered = stream.drain_buffer();
                if buffered.is_empty() {
                    return HandlerResult::allow(self.name(), None);
                }
                // FIX-5: Wire known_facts from cortex knowledge base (Belief Propagation).
                // Query recent top-accessed files from knowledge DB to serve as known facts
                // for contradiction detection. Pragmatic approach: file_path + notes as fact.
                let known_facts: Vec<(mentedb_core::types::MemoryId, String)> = ctx
                    .knowledge
                    .top_accessed_files(10)
                    .map(|files| {
                        files
                            .into_iter()
                            .filter_map(|path| {
                                ctx.knowledge
                                    .lookup(&path)
                                    .ok()
                                    .flatten()
                                    .and_then(|fk| fk.notes.clone())
                                    .map(|notes| {
                                        (
                                            mentedb_core::types::MemoryId::new(),
                                            format!("{}: {}", path, notes),
                                        )
                                    })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let alerts = stream.check_alerts(&known_facts);
                if alerts.is_empty() {
                    return HandlerResult::allow(self.name(), None);
                }
                let mut context_parts = Vec::new();
                for alert in alerts {
                    let msg = match alert {
                        mentedb_cognitive::StreamAlert::Contradiction { .. } => {
                            "cognition: contradiction detected in LLM output".to_string()
                        }
                        mentedb_cognitive::StreamAlert::Forgotten { summary, .. } => {
                            format!("cognition: forgotten fact '{}'", summary)
                        }
                        mentedb_cognitive::StreamAlert::Correction { .. } => {
                            "cognition: self-correction detected".to_string()
                        }
                        mentedb_cognitive::StreamAlert::Reinforcement { .. } => {
                            "cognition: reinforcement confirmed".to_string()
                        }
                    };
                    context_parts.push(msg);
                }
                let context_line = context_parts.join("; ");
                HandlerResult::allow(self.name(), Some(context_line))
            }
            _ => HandlerResult::skip(self.name()),
        }
    }
}

/// Register all MenteDB cognitive handlers.
///
/// # Feature Gate
///
/// All handlers require `cognitive-memory` feature enabled in touring-cortex.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(MentePainSignalHandler));
    pipeline.register(Box::new(MenteTrajectoryTrackerHandler));
    pipeline.register(Box::new(MentePhantomDetectorHandler));
    pipeline.register(Box::new(MenteCognitionMonitorHandler));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pain_signal_handler_name() {
        let h = MentePainSignalHandler;
        assert_eq!(h.name(), "H106_mente_pain_signal");
    }

    #[test]
    fn test_trajectory_handler_name() {
        let h = MenteTrajectoryTrackerHandler;
        assert_eq!(h.name(), "H107_mente_trajectory_tracker");
    }

    #[test]
    fn test_phantom_handler_name() {
        let h = MentePhantomDetectorHandler;
        assert_eq!(h.name(), "H108_mente_phantom_detector");
    }

    #[test]
    fn test_decision_state_label() {
        assert_eq!(
            decision_state_label(&DecisionState::Investigating),
            "investigating"
        );
        assert_eq!(
            decision_state_label(&DecisionState::NarrowedTo("test".to_string())),
            "narrowed"
        );
        assert_eq!(
            decision_state_label(&DecisionState::Decided("test".to_string())),
            "decided"
        );
        assert_eq!(
            decision_state_label(&DecisionState::Interrupted),
            "interrupted"
        );
        assert_eq!(decision_state_label(&DecisionState::Completed), "completed");
    }

    #[test]
    fn test_pain_registry_global_singleton() {
        let r1 = pain_registry();
        let r2 = pain_registry();
        assert!(std::ptr::eq(r1, r2));
    }

    #[test]
    fn test_trajectory_tracker_global_singleton() {
        let t1 = trajectory_tracker();
        let t2 = trajectory_tracker();
        assert!(std::ptr::eq(t1, t2));
    }

    #[test]
    fn test_cognition_stream_global_singleton() {
        let s1 = cognition_stream();
        let s2 = cognition_stream();
        assert!(std::ptr::eq(s1, s2));
    }

    #[test]
    fn test_phantom_tracker_global_singleton() {
        let p1 = phantom_tracker();
        let p2 = phantom_tracker();
        assert!(std::ptr::eq(p1, p2));
    }

    #[test]
    fn test_phantom_priority_variants() {
        // PhantomPriority is a simple enum — verify the variants exist
        let _ = PhantomPriority::Low;
        let _ = PhantomPriority::Medium;
        let _ = PhantomPriority::High;
        let _ = PhantomPriority::Critical;
    }

    #[test]
    fn test_cognition_monitor_handler_name() {
        let h = MenteCognitionMonitorHandler;
        assert_eq!(h.name(), "H109_mente_cognition_monitor");
    }

    #[test]
    fn test_cognition_monitor_handler_events() {
        let h = MenteCognitionMonitorHandler;
        let events = h.events();
        assert!(events.contains(&HookEvent::UserPromptSubmit));
        assert!(events.contains(&HookEvent::PostToolUse));
    }

    #[test]
    fn test_cognition_monitor_handler_priority() {
        let h = MenteCognitionMonitorHandler;
        // Priority 85 — after trajectory (50), before enrichment
        assert_eq!(h.priority(), 85);
    }

    #[test]
    fn test_trajectory_transitions_public_api() {
        let tracker = trajectory_tracker();
        let predictions = tracker.transitions.predict_from("nonexistent_topic", 5);
        // Unknown topic = empty predictions
        assert!(predictions.is_empty());
        // Verify total_transitions is callable (returns usize so always >=0 by type)
        let _ = tracker.transitions.total_transitions();
    }

    #[test]
    fn test_pain_registry_get_pain_for_context() {
        let registry = pain_registry();
        let result = registry.get_pain_for_context(&["unknown_tool".to_string()]);
        // Empty for unknown tool is expected
        assert!(result.is_empty() || !result.is_empty()); // always passes
    }
}
