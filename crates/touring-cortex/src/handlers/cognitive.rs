//! H85: CognitivePredictorHandler
//!
//! Bridges touring-cognitive's SessionPredictor into the cortex pipeline.
//! On UserPromptSubmit, reads recent edit history and predicts the next
//! likely tool invocation, injecting it as anticipatory context.
//!
//! The SessionPredictor combines two signals:
//! - Markov transitions: which tool typically follows which
//! - EMA Q-values: which tools tend to succeed
//!
//! Prediction score = transition_probability × (0.5 + 0.5 × Q-value)

use touring_intelligence::reasoning::{SessionPredictor, ToolInvocation};

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

/// Minimum confidence percentage to inject a prediction into context.
const MIN_CONFIDENCE_PCT: u32 = 40;

/// Number of recent edits to feed into the predictor on each invocation.
const HISTORY_WINDOW: usize = 20;

/// Number of top predictions to request (used for future multi-prediction extension).
const TOP_K: usize = 2;

/// H85: Cognitive next-tool predictor.
///
/// Uses touring-cognitive's Markov + EMA SessionPredictor to anticipate
/// the next tool the user will invoke. Injects the top prediction as context,
/// allowing downstream handlers to pre-warm relevant data.
///
/// Architecture:
/// ```text
/// knowledge.recent_edits_all → ToolInvocation[] → SessionPredictor.record
///                                                          ↓
///                                               predict_top_k(current_tool)
///                                                          ↓
///                                         context_line + RL drift feedback
/// ```
pub struct CognitivePredictorHandler {
    /// Markov + EMA predictor with interior RwLock — safe to hold behind &self.
    predictor: SessionPredictor,
}

impl Default for CognitivePredictorHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitivePredictorHandler {
    /// Creates a handler with a freshly initialized session predictor.
    pub fn new() -> Self {
        Self {
            predictor: SessionPredictor::new(),
        }
    }
}

impl Handler for CognitivePredictorHandler {
    fn name(&self) -> &str {
        "H85_cognitive_predictor"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    fn priority(&self) -> u8 {
        155 // Between H84 IndexHotFiles (160) and H83-wasm TypedEvaluate (140)
    }

    fn dependency_tier(&self) -> u8 {
        1 // T1: requires knowledge DB
    }

    fn timeout_ms(&self) -> u64 {
        30 // Fast SQLite query + Markov computation
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        if ctx.context_budget_remaining < 40 {
            return HandlerResult::skip(self.name());
        }

        // Read recent edit events as tool invocation proxies
        let edits = match ctx.knowledge.recent_edits_all(HISTORY_WINDOW) {
            Ok(e) if !e.is_empty() => e,
            _ => return HandlerResult::skip(self.name()),
        };

        // Use a stable timestamp for all records in this batch
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Feed edits into predictor (oldest-first for correct transition ordering).
        // `record` uses interior RwLock so &self is sufficient.
        for edit in edits.iter().rev() {
            self.predictor.record(ToolInvocation {
                tool_name: edit.edit_type.clone(),
                timestamp_ms: now_ms,
                success: edit.error_pattern.is_none(),
            });
        }

        // Most recent edit type is the "current tool" we transition FROM
        let current_tool = match edits.first() {
            Some(e) => e.edit_type.as_str(),
            None => return HandlerResult::skip(self.name()),
        };

        // Predict next tools via Markov × Q-value scoring
        let predictions = self.predictor.predict_top_k(current_tool, TOP_K);

        let (next_tool, confidence) = match predictions.first() {
            Some(p) => (&p.0, p.1),
            None => return HandlerResult::skip(self.name()),
        };
        let confidence_pct = (confidence * 100.0) as u32;

        // Only surface high-confidence predictions to avoid noise
        if confidence_pct < MIN_CONFIDENCE_PCT {
            return HandlerResult::skip(self.name());
        }

        // Report prediction confidence to RL drift monitor for health tracking
        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.drift_record("cognitive_predictor_confidence", confidence);
        }

        let context_line = format!("cognitive prediction: next={next_tool} [{confidence_pct}%]");

        HandlerResult::allow(self.name(), Some(context_line))
    }
}

/// Register H85 CognitivePredictorHandler.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(CognitivePredictorHandler::new()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_name() {
        let h = CognitivePredictorHandler::new();
        assert_eq!(h.name(), "H85_cognitive_predictor");
    }

    #[test]
    fn test_handler_events() {
        let h = CognitivePredictorHandler::new();
        let events = h.events();
        assert!(events.contains(&HookEvent::UserPromptSubmit));
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_handler_priority_between_h84_and_wasm() {
        let h = CognitivePredictorHandler::new();
        // Must run after wasm (140) but before H84 (160)
        assert!(h.priority() > 140, "should run after H83-wasm");
        assert!(h.priority() < 160, "should run before H84 IndexHotFiles");
        assert_eq!(h.priority(), 155);
    }

    #[test]
    fn test_handler_dependency_tier() {
        let h = CognitivePredictorHandler::new();
        assert_eq!(h.dependency_tier(), 1, "requires knowledge DB (T1)");
    }

    #[test]
    fn test_handler_timeout_is_fast() {
        let h = CognitivePredictorHandler::new();
        assert!(h.timeout_ms() <= 50, "should be a fast operation");
    }

    #[test]
    fn test_handler_not_critical() {
        let h = CognitivePredictorHandler::new();
        assert!(!h.is_critical());
    }

    #[test]
    fn test_handler_not_async() {
        let h = CognitivePredictorHandler::new();
        assert!(!h.is_async());
    }

    #[test]
    fn test_handler_no_cache_invalidation() {
        let h = CognitivePredictorHandler::new();
        assert!(!h.requires_cache_invalidation());
    }

    #[test]
    fn test_predictor_has_interior_mutability() {
        // Verifies that SessionPredictor can be recorded from &self (no &mut needed).
        let h = CognitivePredictorHandler::new();
        h.predictor.record(ToolInvocation {
            tool_name: "Edit".to_string(),
            timestamp_ms: 1000,
            success: true,
        });
        h.predictor.record(ToolInvocation {
            tool_name: "Bash".to_string(),
            timestamp_ms: 2000,
            success: true,
        });
        // After Edit→Bash transition, predict_next("Edit") should return Some("Bash")
        let prediction = h.predictor.predict_next("Edit");
        assert!(
            prediction.is_some(),
            "should predict after recording transitions"
        );
        let (tool, confidence) = prediction.unwrap();
        assert_eq!(tool, "Bash");
        assert!(confidence > 0.0);
    }

    #[test]
    fn test_predict_top_k() {
        let h = CognitivePredictorHandler::new();
        // Record multiple transitions to build Q-table
        for _ in 0..3 {
            h.predictor.record(ToolInvocation {
                tool_name: "Read".to_string(),
                timestamp_ms: 1000,
                success: true,
            });
            h.predictor.record(ToolInvocation {
                tool_name: "Edit".to_string(),
                timestamp_ms: 2000,
                success: true,
            });
        }
        let predictions = h.predictor.predict_top_k("Read", 2);
        assert!(!predictions.is_empty());
        assert_eq!(predictions[0].0, "Edit");
    }
}
