//! Trait `ReasoningEngine` — unified interface for reasoning strategies (S7).
//!
//! Provides a common `search()` contract for MCTS, GoT, Hybrid, and future
//! adaptive engines. Enables strategy-pattern composition, generic testing
//! via `Box<dyn ReasoningEngine>`, and bandit-based selection (S14).

use std::collections::HashMap;

/// A query submitted to a reasoning engine.
#[derive(Debug, Clone)]
pub struct ReasoningQuery {
    /// Root state identifier.
    pub root_state: u64,
    /// Textual description of the problem (used by GoT).
    pub description: String,
    /// Available actions / candidate expansions (used by MCTS).
    pub candidate_actions: Vec<u64>,
    /// CILA complexity level (0-6) for adaptive budget.
    pub cila_level: u8,
    /// Optional key-value context (e.g., file paths, tool names).
    pub context: HashMap<String, String>,
}

impl ReasoningQuery {
    /// Create a minimal query with just a root state and description.
    pub fn new(root_state: u64, description: impl Into<String>) -> Self {
        Self {
            root_state,
            description: description.into(),
            candidate_actions: Vec::new(),
            cila_level: 3,
            context: HashMap::new(),
        }
    }

    /// Builder: set candidate actions.
    pub fn with_actions(mut self, actions: Vec<u64>) -> Self {
        self.candidate_actions = actions;
        self
    }

    /// Builder: set CILA level.
    pub fn with_cila_level(mut self, level: u8) -> Self {
        self.cila_level = level;
        self
    }

    /// Builder: add a context key-value pair.
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

/// Result returned by a reasoning engine search.
#[derive(Debug, Clone)]
pub struct ReasoningResult {
    /// Best action / recommendation.
    pub best_action: u64,
    /// Confidence in the recommendation (0.0–1.0).
    pub confidence: f64,
    /// Estimated value / quality of the best action.
    pub value: f64,
    /// Engine-specific metadata (e.g., tree depth, nodes explored).
    pub metadata: HashMap<String, f64>,
    /// Name of the engine that produced this result.
    pub engine_name: String,
}

impl ReasoningResult {
    /// Create a minimal result.
    pub fn new(
        best_action: u64,
        confidence: f64,
        value: f64,
        engine_name: impl Into<String>,
    ) -> Self {
        Self {
            best_action,
            confidence,
            value,
            metadata: HashMap::new(),
            engine_name: engine_name.into(),
        }
    }

    /// Builder: add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, val: f64) -> Self {
        self.metadata.insert(key.into(), val);
        self
    }
}

/// Unified trait for all reasoning engines in touring-cognitive.
///
/// Implementors: `MCTSReasoningEngine`, `GotReasoningEngine`, `HybridReasoningEngine`.
/// Enables:
/// - Strategy pattern switching at runtime
/// - `Box<dyn ReasoningEngine>` for generic composition
/// - Bandit-based adaptive selection (S14)
pub trait ReasoningEngine: Send + Sync {
    /// Human-readable name of this engine (e.g., "MCTS", "GoT", "Hybrid").
    fn name(&self) -> &str;

    /// Execute a search and return a result.
    ///
    /// Returns `None` if the engine cannot produce a result for the given query
    /// (e.g., no candidate actions for MCTS, no graph nodes for GoT).
    fn search(&self, query: &ReasoningQuery) -> Option<ReasoningResult>;

    /// Return the engine's current configuration as key-value pairs.
    fn config(&self) -> HashMap<String, String>;
}

// ---------------------------------------------------------------------------
// MCTS adapter
// ---------------------------------------------------------------------------

/// `ReasoningEngine` adapter for `MCTSEngine`.
pub struct MCTSReasoningEngine {
    config: crate::reasoning::mcts::MCTSConfig,
}

impl MCTSReasoningEngine {
    /// Create with default config.
    pub fn new() -> Self {
        Self {
            config: crate::reasoning::mcts::MCTSConfig::default(),
        }
    }

    /// Create with CILA-adaptive config.
    pub fn for_cila_level(level: u8) -> Self {
        Self {
            config: crate::reasoning::mcts::MCTSConfig::for_cila_level(level),
        }
    }

    /// Create with explicit config.
    pub fn with_config(config: crate::reasoning::mcts::MCTSConfig) -> Self {
        Self { config }
    }
}

impl Default for MCTSReasoningEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ReasoningEngine for MCTSReasoningEngine {
    fn name(&self) -> &str {
        "MCTS"
    }

    fn search(&self, query: &ReasoningQuery) -> Option<ReasoningResult> {
        let engine = crate::reasoning::mcts::MCTSEngine::new(self.config.clone());
        let actions = query.candidate_actions.clone();
        if actions.is_empty() {
            return None;
        }
        let result = engine.search(
            query.root_state,
            |_state| actions.clone(),
            |_state, _action| 0.5, // default uniform reward
        )?;
        Some(
            ReasoningResult::new(result.best_action, result.confidence, result.value, "MCTS")
                .with_metadata("tree_depth", result.tree_depth as f64)
                .with_metadata("total_rollouts", result.total_rollouts as f64),
        )
    }

    fn config(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("max_depth".into(), self.config.max_depth.to_string());
        m.insert("num_rollouts".into(), self.config.num_rollouts.to_string());
        m.insert(
            "exploration_constant".into(),
            format!("{:.4}", self.config.exploration_constant),
        );
        m.insert("discount".into(), format!("{:.4}", self.config.discount));
        m
    }
}

// ---------------------------------------------------------------------------
// Hybrid adapter
// ---------------------------------------------------------------------------

/// `ReasoningEngine` adapter for `HybridCognitiveEngine`.
///
/// Uses pheromone-guided scoring over candidate ThoughtResults.
pub struct HybridReasoningEngine {
    engine: crate::reasoning::hybrid_engine::HybridCognitiveEngine,
}

impl HybridReasoningEngine {
    /// Create with a fresh pheromone layer.
    pub fn new() -> Self {
        Self {
            engine: crate::reasoning::hybrid_engine::HybridCognitiveEngine::with_fresh_pheromone(),
        }
    }
}

impl Default for HybridReasoningEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ReasoningEngine for HybridReasoningEngine {
    fn name(&self) -> &str {
        "Hybrid"
    }

    fn search(&self, query: &ReasoningQuery) -> Option<ReasoningResult> {
        // Build synthetic ThoughtResults from candidate actions
        let candidates: Vec<crate::reasoning::got::ThoughtResult> = query
            .candidate_actions
            .iter()
            .map(|&action| crate::reasoning::got::ThoughtResult {
                node_id: action,
                score: 0.5,
                output: String::new(),
                depth: 0,
                relevance: 1.0,
                confidence: 0.5,
                novelty: 1.0,
            })
            .collect();

        let best = self
            .engine
            .select_next_node(&candidates, query.root_state)?;
        Some(ReasoningResult::new(
            best.node_id,
            best.score,
            best.score,
            "Hybrid",
        ))
    }

    fn config(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("beam_width".into(), self.engine.beam_width.to_string());
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reasoning_query_builder() {
        let q = ReasoningQuery::new(42, "test query")
            .with_actions(vec![1, 2, 3])
            .with_cila_level(4)
            .with_context("file", "main.rs");
        assert_eq!(q.root_state, 42);
        assert_eq!(q.description, "test query");
        assert_eq!(q.candidate_actions, vec![1, 2, 3]);
        assert_eq!(q.cila_level, 4);
        assert_eq!(q.context.get("file").unwrap(), "main.rs");
    }

    #[test]
    fn test_reasoning_result_builder() {
        let r = ReasoningResult::new(10, 0.95, 0.8, "TestEngine").with_metadata("depth", 5.0);
        assert_eq!(r.best_action, 10);
        assert_eq!(r.confidence, 0.95);
        assert_eq!(r.engine_name, "TestEngine");
        assert_eq!(r.metadata.get("depth").copied(), Some(5.0));
    }

    #[test]
    fn test_mcts_reasoning_engine_name() {
        let engine = MCTSReasoningEngine::new();
        assert_eq!(engine.name(), "MCTS");
    }

    #[test]
    fn test_mcts_reasoning_engine_search() {
        let engine = MCTSReasoningEngine::new();
        let query = ReasoningQuery::new(1, "plan").with_actions(vec![10, 20, 30]);
        let result = engine.search(&query);
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.confidence > 0.0);
        assert_eq!(r.engine_name, "MCTS");
        assert!(r.metadata.contains_key("tree_depth"));
    }

    #[test]
    fn test_mcts_reasoning_engine_empty_actions() {
        let engine = MCTSReasoningEngine::new();
        let query = ReasoningQuery::new(1, "plan"); // no actions
        assert!(engine.search(&query).is_none());
    }

    #[test]
    fn test_mcts_reasoning_engine_cila_level() {
        let engine = MCTSReasoningEngine::for_cila_level(4);
        let config = engine.config();
        assert_eq!(config.get("num_rollouts").unwrap(), "100");
        assert_eq!(config.get("max_depth").unwrap(), "7");
    }

    #[test]
    fn test_hybrid_reasoning_engine_name() {
        let engine = HybridReasoningEngine::new();
        assert_eq!(engine.name(), "Hybrid");
    }

    #[test]
    fn test_hybrid_reasoning_engine_search() {
        let engine = HybridReasoningEngine::new();
        let query = ReasoningQuery::new(0, "select best").with_actions(vec![1, 2, 3]);
        let result = engine.search(&query);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.engine_name, "Hybrid");
    }

    #[test]
    fn test_hybrid_reasoning_engine_empty_actions() {
        let engine = HybridReasoningEngine::new();
        let query = ReasoningQuery::new(0, "select best"); // no actions
        assert!(engine.search(&query).is_none());
    }

    #[test]
    fn test_dyn_reasoning_engine() {
        // Test that Box<dyn ReasoningEngine> works (strategy pattern)
        let engines: Vec<Box<dyn ReasoningEngine>> = vec![
            Box::new(MCTSReasoningEngine::new()),
            Box::new(HybridReasoningEngine::new()),
        ];
        let query = ReasoningQuery::new(1, "test").with_actions(vec![10, 20]);
        for engine in &engines {
            let _ = engine.search(&query);
            assert!(!engine.name().is_empty());
            assert!(!engine.config().is_empty());
        }
    }

    #[test]
    fn test_mcts_default() {
        let engine = MCTSReasoningEngine::default();
        assert_eq!(engine.name(), "MCTS");
    }

    #[test]
    fn test_hybrid_default() {
        let engine = HybridReasoningEngine::default();
        assert_eq!(engine.name(), "Hybrid");
    }
}
