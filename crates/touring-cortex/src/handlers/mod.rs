//! Cortex Handlers — All hook handlers organized by domain.
//!
//! Split from `builtin_handlers.rs` for maintainability:
//! - `neural.rs`: Original 10 neural hook wrappers (pre/post read/bash/edit, session, classify, pii)
//! - `enforcement.rs`: Strategy enforcer, data structure validator
//! - `intelligence.rs`: ACO phase0, DSPy perception, prompt recall
//! - `lifecycle.rs`: Pre/post-compact, subagent start/stop, teammate idle, task completed,
//!   post-tool-failure, stop-failure, session-end, config-change

pub mod cognitive;
pub mod drift;
pub mod dspy;
pub mod enforcement;
pub mod enrichment;
pub mod evolution;
/// H104: IncrementalIndexing — previously orphaned (file-present but never
/// registered). Wired into `register_all` on 2026-04-16 during Moka
/// Expansion Wave 2 (the migration that would otherwise have been dead
/// code by construction).
pub mod incremental_indexing;
pub mod integration;
pub mod intelligence;
pub mod learning;
pub mod lifecycle;
pub mod neural;
pub mod quality;
pub mod quality_50;
pub mod ranking;
pub mod rules;
pub mod session;
pub mod test_generation;
pub mod tools;
pub mod wasm;

/// H106-H108: MenteDB cognitive handlers — Curva-U attention + Delta-Aware
/// Serving + Belief Propagation. Requires `cognitive-memory` feature.
/// OFF by default per ADR D4 until v1.0 review completes.
#[cfg(feature = "cognitive-memory")]
pub mod mente;

/// H100: DSPy prompt compilation + CrdtSemanticGraph topological few-shot
/// enrichment. Previously orphaned (file present, never in module tree).
/// Wired 2026-04-20 — Suggestion 3 (CRDT-guided DSPy compilation).
pub mod dspy_compile;

use crate::pipeline::Pipeline;
use std::collections::HashSet;

/// E5-S2: Handler feature flags — runtime enable/disable without recompilation.
///
/// When a `HandlerConfig` is provided to `register_all_filtered`, handlers whose
/// names appear in `disabled_handlers` are skipped during registration.
/// This allows per-project tuning via configuration files or env vars.
#[derive(Debug, Clone, Default)]
pub struct HandlerConfig {
    /// Set of handler names to disable (case-sensitive, exact match).
    pub disabled_handlers: HashSet<String>,
}

impl HandlerConfig {
    /// Create config with no disabled handlers (all enabled).
    pub fn all_enabled() -> Self {
        Self::default()
    }

    /// Create config from an iterator of disabled handler names.
    pub fn with_disabled(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let disabled_handlers = names.into_iter().map(Into::into).collect();
        Self { disabled_handlers }
    }

    /// Check if a handler is enabled.
    pub fn is_enabled(&self, name: &str) -> bool {
        !self.disabled_handlers.contains(name)
    }

    /// Create config from a comma-separated env var value.
    ///
    /// Example: `TOURING_DISABLED_HANDLERS="H65,H66,H70"`
    pub fn from_env_var(value: &str) -> Self {
        Self::with_disabled(
            value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from),
        )
    }
}

/// Total number of builtin handlers registered by `register_all`.
///
/// v9.2: 31 handlers (10 neural + 8 enforcement + 3 intelligence + 10 lifecycle)
/// v10.0: 49 handlers (+2 learning + 3 session + 5 tools + 2 intelligence + 3 lifecycle + 4 evolution)
/// v10.1: 51 handlers (+1 PermissionRequestHandler in lifecycle)
/// v10.2: 54 handlers (+3 quality: CodeStandardsEnforcer, PostQualityGate, ComplianceCollector)
/// v10.3: 56 handlers (+2 DSPy bridge: DspyQualityBridge, DspySessionOptimizer)
/// v10.4: 59 handlers (+2 tools: AcoGoalTracker, FailureRecoveryOrchestrator; +1 lifecycle: VgpLearning)
/// v10.5: 58 handlers (-1: ShadowLintGate removed, superseded by CodeStandardsEnforcer)
/// v10.6: 68 handlers (+10 enrichment: H60-H69 migrated from Python project hooks)
/// v10.7: 69 handlers (+1 H70 CodeFirstPipelineGuard — TACO principle as native Rust)
/// v10.8: 73 handlers (+3 Cipher RL-connected: H71-H73, +1 ObserverLearningLoop: H74)
/// v10.9: 75 handlers (+2 rules: H75 RulesContextRouter, H76 RulesHealthMonitor)
/// v11.0: 81 handlers (+6 lifecycle: H77 Notification, H78 Setup, H79 Elicitation,
///         H80 ElicitationResult, H81 CwdChanged, H82 FileChanged)
/// v11.1: 82 handlers (+1 integration: H83 IntegrationCompletenessHandler — wiring audit)
/// v11.2: 86 handlers (+1 test_generation: H102 TestGenerationHandler — MCTS test synthesis;
///         +1 wasm: H105 TypedEvaluateHandler — typed WASM plugin evaluation)
/// v11.3: 89 handlers (+1 cognitive: H85 CognitivePredictorHandler — Markov+EMA tool prediction;
///         +1 drift: H90 DriftAuditHandler — KS-test distribution-level drift detection;
///         +1 ranking: H86 ContextualRankerHandler — BM25 context re-ranking)
/// v11.4: 90 handlers (+1 incremental_indexing: H104 — previously orphaned
///         moka-backed file-content cache + O(log N) delta symbol extraction,
///         wired 2026-04-16 during Moka Expansion Wave 2)
/// v11.5: 91 handlers (+1 dspy_compile: H100 DSPyIntegrationHandler — previously
///         orphaned; wired 2026-04-20 with CrdtSemanticGraph topological enrichment)
pub const BUILTIN_HANDLER_COUNT: usize = 91;

/// v11.6: 96 handlers (+4 mente: H106-H109 via cognitive-memory feature)
///   H105=wasm, H106=pain_signal, H107=trajectory_tracker, H108=phantom_detector, H109=cognition_monitor
#[cfg(feature = "cognitive-memory")]
pub const BUILTIN_HANDLER_COUNT_WITH_MENTE: usize = 96;

#[cfg(not(feature = "cognitive-memory"))]
pub const BUILTIN_HANDLER_COUNT_WITH_MENTE: usize = 91;

/// Register all builtin handlers in the pipeline.
///
/// Order: neural (sensory) → enforcement → intelligence → lifecycle → learning → session → tools → evolution
pub fn register_all(pipeline: &mut Pipeline) {
    neural::register(pipeline);
    enforcement::register(pipeline);
    intelligence::register(pipeline);
    lifecycle::register(pipeline);
    learning::register(pipeline);
    session::register(pipeline);
    tools::register(pipeline);
    evolution::register(pipeline);
    quality::register(pipeline);
    enrichment::register(pipeline);
    rules::register(pipeline);
    integration::register(pipeline);
    test_generation::register(pipeline);
    wasm::register(pipeline);
    cognitive::register(pipeline);
    drift::register(pipeline);
    ranking::register(pipeline);
    incremental_indexing::register(pipeline);
    dspy_compile::register(pipeline);
    #[cfg(feature = "cognitive-memory")]
    mente::register(pipeline);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_handlers_registered() {
        let mut pipeline = Pipeline::new();
        register_all(&mut pipeline);
        let expected = if cfg!(feature = "cognitive-memory") {
            BUILTIN_HANDLER_COUNT_WITH_MENTE
        } else {
            BUILTIN_HANDLER_COUNT
        };
        assert_eq!(
            pipeline.handler_count(),
            expected,
            "Expected {} handlers, got {}",
            expected,
            pipeline.handler_count()
        );
    }

    // ── E5-S2: Handler feature flags tests ──────────────────────────

    #[test]
    fn test_handler_config_all_enabled() {
        let config = HandlerConfig::all_enabled();
        assert!(config.is_enabled("any_handler"));
        assert!(config.disabled_handlers.is_empty());
    }

    #[test]
    fn test_handler_config_with_disabled() {
        let config = HandlerConfig::with_disabled(["H65", "H70"]);
        assert!(!config.is_enabled("H65"));
        assert!(!config.is_enabled("H70"));
        assert!(config.is_enabled("H60"));
        assert!(config.is_enabled("other"));
    }

    #[test]
    fn test_handler_config_from_env_var() {
        let config = HandlerConfig::from_env_var("H65, H66, H70");
        assert!(!config.is_enabled("H65"));
        assert!(!config.is_enabled("H66"));
        assert!(!config.is_enabled("H70"));
        assert!(config.is_enabled("H60"));
    }

    #[test]
    fn test_handler_config_from_env_var_empty() {
        let config = HandlerConfig::from_env_var("");
        assert!(config.disabled_handlers.is_empty());
    }

    #[test]
    fn test_handler_config_from_env_var_trailing_comma() {
        let config = HandlerConfig::from_env_var("H65,H66,");
        assert_eq!(config.disabled_handlers.len(), 2);
        assert!(!config.is_enabled("H65"));
        assert!(!config.is_enabled("H66"));
    }
}
