//! H91: ReasoningContextEnricher
//!
//! First production use of `touring_intelligence::reasoning::HybridReasoningEngine`, connecting the
//! MCTS+GoT reasoning engine into the cortex context injection pipeline.
//!
//! On PreToolUse: builds a `ReasoningQuery` from the incoming tool name, runs MCTS+GoT
//! search over 4 candidate actions, and injects a strategy hint when the engine
//! returns a high-confidence (`>= 0.7`) recommendation.
//!
//! Architecture:
//! ```text
//! PreToolUse:
//!   tool_name → root_state (deterministic hash)
//!   ReasoningQuery { root_state, actions=[0,1,2,3], cila_level=0 }
//!   HybridReasoningEngine::search(&query) → Option<ReasoningResult>
//!   if Some(result) && confidence >= 0.7:
//!     best_action → strategy string
//!     inject_context("reasoning: {strategy} (conf={:.2})")
//!   else:
//!     skip
//! ```

use touring_intelligence::reasoning::{HybridReasoningEngine, ReasoningEngine, ReasoningQuery};

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

/// Minimum context budget required before running the reasoning enricher.
const MIN_BUDGET: usize = 50;

/// Confidence threshold: only inject context when the engine is at least this confident.
const MIN_CONFIDENCE: f64 = 0.7;

/// Candidate action labels mapped to strategy names.
const ACTION_LABELS: [&str; 4] = [
    "read_more_context",
    "validate_inputs",
    "proceed_directly",
    "enrich_context",
];

/// H91: Reasoning context enricher — MCTS+GoT strategy hint injection.
///
/// Dependency tier: 0 (pure computation — no DB, no I/O).
/// Priority: 195 (after H90 drift at 190, before other handlers at 250+).
pub struct ReasoningContextEnricher {
    engine: HybridReasoningEngine,
}

impl Default for ReasoningContextEnricher {
    fn default() -> Self {
        Self::new()
    }
}

impl ReasoningContextEnricher {
    pub fn new() -> Self {
        Self {
            engine: HybridReasoningEngine::new(),
        }
    }
}

impl Handler for ReasoningContextEnricher {
    fn name(&self) -> &str {
        "H91_reasoning_enricher"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn priority(&self) -> u8 {
        195 // After H90 drift (190), before other handlers (250+)
    }

    fn dependency_tier(&self) -> u8 {
        0 // T0: pure computation — no DB required
    }

    fn timeout_ms(&self) -> u64 {
        30
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Skip if context budget is too low.
        if ctx.context_budget_remaining < MIN_BUDGET {
            return HandlerResult::skip(self.name());
        }

        // Compute a deterministic root state from the tool name.
        let tool_name = ctx.tool_name.as_deref().unwrap_or("");
        let root_state: u64 = tool_name
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(u64::from(b)));

        // Build the reasoning query with 4 candidate actions.
        let query = ReasoningQuery::new(root_state, tool_name)
            .with_actions(vec![0u64, 1, 2, 3])
            .with_cila_level(0);

        // Run MCTS+GoT reasoning search.
        let result = self.engine.search(&query);

        match result {
            Some(r) if r.confidence >= MIN_CONFIDENCE => {
                // Map best_action index to a strategy label.
                let strategy = match r.best_action {
                    0 => ACTION_LABELS[0],
                    1 => ACTION_LABELS[1],
                    2 => ACTION_LABELS[2],
                    3 => ACTION_LABELS[3],
                    _ => return HandlerResult::skip(self.name()),
                };

                let context = format!(
                    "reasoning: {strategy} (conf={:.2})",
                    r.confidence
                );
                HandlerResult::allow(self.name(), Some(context))
            }
            _ => HandlerResult::skip(self.name()),
        }
    }
}

/// Register H91 ReasoningContextEnricher.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(ReasoningContextEnricher::new()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_intelligence::reasoning::{HybridReasoningEngine, ReasoningEngine, ReasoningQuery};

    // ── trait property tests ──────────────────────────────────────────────

    #[test]
    fn test_handler_name() {
        let h = ReasoningContextEnricher::new();
        assert_eq!(h.name(), "H91_reasoning_enricher");
    }

    #[test]
    fn test_handler_priority_195() {
        let h = ReasoningContextEnricher::new();
        assert_eq!(h.priority(), 195);
        assert!(
            h.priority() > 190,
            "must run after H90 drift (190)"
        );
    }

    #[test]
    fn test_handler_events_pretooluse() {
        let h = ReasoningContextEnricher::new();
        assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    }

    #[test]
    fn test_handler_not_critical() {
        let h = ReasoningContextEnricher::new();
        assert!(!h.is_critical(), "reasoning enricher is not critical");
    }

    #[test]
    fn test_handler_timeout_30ms() {
        let h = ReasoningContextEnricher::new();
        assert_eq!(h.timeout_ms(), 30);
    }

    #[test]
    fn test_handler_dependency_tier_0() {
        let h = ReasoningContextEnricher::new();
        assert_eq!(h.dependency_tier(), 0, "T0: pure computation — no DB");
    }

    // ── reasoning engine behavioral tests ────────────────────────────────

    #[test]
    fn test_reasoning_engine_produces_result() {
        // HybridReasoningEngine with 4 candidate actions should return Some.
        let engine = HybridReasoningEngine::new();
        let query = ReasoningQuery::new(12345u64, "test_tool")
            .with_actions(vec![0u64, 1, 2, 3])
            .with_cila_level(0);
        let result = engine.search(&query);
        assert!(
            result.is_some(),
            "HybridReasoningEngine should return Some for a valid query"
        );
        let r = result.expect("result must be present");
        assert!(r.best_action <= 3, "best_action must be one of [0,1,2,3]");
        assert!(
            (0.0..=1.0).contains(&r.confidence),
            "confidence must be in [0.0, 1.0], got {}",
            r.confidence
        );
    }

    #[test]
    fn test_root_state_hash_deterministic() {
        // Same tool_name must produce the same root_state every time.
        let compute_hash = |name: &str| -> u64 {
            name.bytes()
                .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(u64::from(b)))
        };
        let tool = "Read";
        assert_eq!(
            compute_hash(tool),
            compute_hash(tool),
            "hash must be deterministic for the same tool name"
        );
    }

    #[test]
    fn test_root_state_hash_different() {
        // Different tool_names must produce different root_states.
        let compute_hash = |name: &str| -> u64 {
            name.bytes()
                .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(u64::from(b)))
        };
        let hash_read = compute_hash("Read");
        let hash_write = compute_hash("Write");
        let hash_bash = compute_hash("Bash");
        assert_ne!(hash_read, hash_write, "Read vs Write must differ");
        assert_ne!(hash_read, hash_bash, "Read vs Bash must differ");
        assert_ne!(hash_write, hash_bash, "Write vs Bash must differ");
    }
}
