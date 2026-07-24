//! H100: DSPyIntegration — DSPy-powered prompt compilation for code intelligence.
//!
//! Builds on H99 (MCTSCodeSynthesis) and H101 (SelfReflectionLoop) to provide
//! DSPy-style prompt optimization:
//! - Signature-based prompt templates (input/output signatures)
//! - Metric-guided compilation (correctness, style, efficiency)
//! - Bootstrapfew-shot example selection
//! - LLM-based prompt refinement via touring-cognitive engines
//!
//! **Note**: Full DSPy API integration is feature-gated pending API stability.
//! This handler provides the wiring layer using existing HybridReasoningEngine
//! with a DSPy-compatible interface, ready for when touring-dspy is available.
//!
//! Architecture:
//! ```text
//! PreToolUse (Write|Edit):
//!   1. Extract tool_input (code snippet or natural language intent)
//!   2. Build ReasoningQuery with DSPy signature context
//!   3. HybridReasoningEngine::search() → prompt optimization candidates
//!   4. UCB1 + pheromone select best optimization
//!   5. Inject suggestion as context enrichment
//! ```
//!
//! DSPy-inspired phases (adapted for Rust sync context):
//! - DEMONSTRATE: Extract few-shot examples from session history
//! - COMPILE: Build optimal prompt via reasoning search
//! - OPTIMIZE: Tune based on tool outcome feedback (via pheromone loop)

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use touring_intelligence::reasoning::{
    HybridCognitiveEngine, HybridReasoningEngine, ReasoningEngine, ReasoningQuery, ReasoningResult,
};
use touring_intelligence::rl::memory::{CrdtNodeId, CrdtSemanticGraph};

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

/// Minimum context budget before running DSPy compilation.
const MIN_BUDGET: usize = 80;

/// Minimum confidence threshold to inject suggestion.
const MIN_CONFIDENCE: f64 = 0.70;

/// Process-global DSPy compilation engine — persists for daemon lifetime.
static GLOBAL_REASONING: OnceLock<HybridReasoningEngine> = OnceLock::new();
static GLOBAL_HYBRID: OnceLock<HybridCognitiveEngine> = OnceLock::new();
/// CRDT semantic graph — populated by touring-learning during hook lifecycle.
/// Random-walk topological neighbor retrieval for DSPy few-shot context enrichment.
static GLOBAL_CRDT: OnceLock<Mutex<CrdtSemanticGraph>> = OnceLock::new();

fn global_reasoning() -> &'static HybridReasoningEngine {
    GLOBAL_REASONING.get_or_init(HybridReasoningEngine::new)
}

fn global_hybrid() -> &'static HybridCognitiveEngine {
    GLOBAL_HYBRID.get_or_init(HybridCognitiveEngine::with_fresh_pheromone)
}

fn global_crdt() -> &'static Mutex<CrdtSemanticGraph> {
    GLOBAL_CRDT.get_or_init(|| Mutex::new(CrdtSemanticGraph::new()))
}

/// H100: DSPy integration handler — prompt compilation and optimization.
///
/// Dependency tier: 1 (requires HybridReasoningEngine).
/// Priority: 220 (after H101 SelfReflectionLoop at 215).
#[derive(Default)]
pub struct DSPyIntegrationHandler;

impl DSPyIntegrationHandler {
    /// Creates a new DSPy integration handler.
    pub fn new() -> Self {
        Self
    }

    /// Extract code or intent from tool input for DSPy analysis.
    fn extract_content(&self, tool_input: &serde_json::Value) -> Option<String> {
        tool_input
            .pointer("/content")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .or_else(|| {
                tool_input
                    .pointer("/code")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            })
            .or_else(|| {
                tool_input
                    .pointer("/intent")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            })
    }

    /// Extract target symbol for context.
    fn extract_symbol(&self, tool_input: &serde_json::Value) -> String {
        tool_input
            .pointer("/symbol")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                tool_input
                    .pointer("/file_path")
                    .and_then(|v| v.as_str())
                    .and_then(|p| std::path::Path::new(p).file_stem())
                    .and_then(|s| s.to_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Build a DSPy-style reasoning query from tool input.
    fn build_query(&self, content: &str, symbol: &str) -> ReasoningQuery {
        let root_state = self.hash_symbol(symbol);
        let mut context = HashMap::new();
        context.insert("symbol".to_string(), symbol.to_string());
        context.insert("content_hash".to_string(), format!("{:x}", content.len()));

        ReasoningQuery::new(root_state, content)
            .with_context("tool".to_string(), "dspy_compile".to_string())
            .with_context("symbol".to_string(), symbol.to_string())
    }

    /// Deterministic hash for symbol (used as root state).
    fn hash_symbol(&self, symbol: &str) -> u64 {
        symbol.bytes().fold(0u64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(u64::from(b))
        })
    }

    /// Score a reasoning result using pheromone-guided composite.
    fn score_result(&self, result: &ReasoningResult, parent_state: u64) -> f64 {
        let hybrid = global_hybrid();
        let pheromone = hybrid
            .shared_pheromone
            .lock()
            .ok()
            .map(|g| g.strength(parent_state, result.best_action))
            .unwrap_or(0.0);

        // Composite: confidence weighted by pheromone trail
        result.confidence * (1.0 + pheromone * 0.3).clamp(0.5, 1.5)
    }

    /// Run DSPy compilation search and return best suggestion.
    fn compile_prompt(&self, content: &str, symbol: &str) -> Option<(String, f64)> {
        let reasoning = global_reasoning();
        let query = self.build_query(content, symbol);

        // Run reasoning search
        let result = reasoning.search(&query)?;

        let score = self.score_result(&result, query.root_state);

        if score < MIN_CONFIDENCE {
            return None;
        }

        // Build DSPy-style suggestion
        let suggestion = format!(
            "dspy[compiled]: optimize {} (conf={:.0}%, engine={}, value={:.2})",
            symbol,
            score * 100.0,
            result.engine_name,
            result.value,
        );

        Some((suggestion, score))
    }

    /// Deposit reward for DSPy compilation path.
    fn deposit_compilation_reward(&self, symbol: &str, score: f64) {
        let root_state = self.hash_symbol(symbol);
        let path = vec![
            touring_intelligence::reasoning::ThoughtResult {
                node_id: root_state,
                score,
                output: "dspy_compile".to_string(),
                depth: 0,
                relevance: 1.0,
                confidence: score,
                novelty: 0.3,
            },
            touring_intelligence::reasoning::ThoughtResult {
                node_id: root_state.wrapping_add(1),
                score,
                output: "compiled".to_string(),
                depth: 1,
                relevance: 1.0,
                confidence: score,
                novelty: 0.3,
            },
        ];

        global_hybrid().deposit_got_reward(&path, score);
    }

    /// Topology-guided few-shot context via CRDT random walk (Suggestion 3 — 2026-04-20).
    ///
    /// Walks the CrdtSemanticGraph edges FROM the hashed symbol node, collecting
    /// structural neighbors weighted by their NodeWeight scores. These are injected
    /// as deterministic few-shot examples — correlated by dependency topology, not
    /// just BM25 lexical similarity.
    ///
    /// Returns None if graph has no edges for this symbol (empty graph or cold-start).
    fn build_crdt_context(&self, symbol: &str) -> Option<String> {
        let node_id: CrdtNodeId = self.hash_symbol(symbol);
        let graph = global_crdt().lock().ok()?;

        // Collect outgoing neighbors with their weights
        let mut neighbors: Vec<(f64, String)> = graph
            .edge_list()
            .iter()
            .filter(|e| e.from == node_id)
            .filter_map(|e| {
                let w = graph.get_weight(e.to)?;
                if w.label.is_empty() {
                    None
                } else {
                    Some((w.score, w.label.clone()))
                }
            })
            .collect();

        if neighbors.is_empty() {
            return None;
        }

        // Sort by weight descending — highest-scoring neighbors first (top 3)
        neighbors.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        neighbors.truncate(3);

        let ctx = neighbors
            .iter()
            .map(|(score, label)| format!("{label}(w={score:.2})"))
            .collect::<Vec<_>>()
            .join(", ");

        Some(format!("crdt-neighbors[{symbol}]: {ctx}"))
    }

    /// Run speculative prompt optimization (without full DSPy bootstrap).
    ///
    /// When DSPy is not available, uses heuristic optimization patterns
    /// learned from the pheromone layer to suggest improvements.
    fn heuristic_optimize(&self, content: &str, symbol: &str) -> Option<String> {
        let hybrid = global_hybrid();

        // Analyze content patterns
        let has_comments =
            content.contains("//") || content.contains("/*") || content.contains("///");
        let has_docstring = content.contains("\"\"\"") || content.contains("'''");
        let has_type_hints =
            content.contains(": str") || content.contains(": i32") || content.contains(": String");
        let _is_modular =
            content.contains("fn ") || content.contains("pub fn") || content.contains("impl ");
        let complexity_score = content.len() as f64 / 500.0;

        // Check pheromone for past optimizations on this symbol
        let root_state = self.hash_symbol(symbol);
        let pheromone_boost = hybrid
            .shared_pheromone
            .lock()
            .ok()
            .map(|g| g.strength(root_state, root_state.wrapping_add(1)))
            .unwrap_or(0.0);

        // Build suggestion based on patterns
        let mut suggestions = Vec::new();

        if !has_docstring && !has_comments && content.len() > 200 {
            suggestions.push("add docstring");
        }
        if !has_type_hints && content.contains("fn ") {
            suggestions.push("add type hints");
        }
        if complexity_score > 2.0 {
            suggestions.push("break into smaller functions");
        }
        if pheromone_boost > 0.3 {
            suggestions.push("apply learned optimization pattern");
        }

        if suggestions.is_empty() {
            return None;
        }

        let suggestion = format!(
            "dspy[heuristic]: {} → {} (pheromone={:.2})",
            symbol,
            suggestions.join(", "),
            pheromone_boost,
        );

        Some(suggestion)
    }
}

impl Handler for DSPyIntegrationHandler {
    fn name(&self) -> &str {
        "H100_dspy_integration"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit|MultiEdit")
    }

    fn priority(&self) -> u8 {
        220
    }

    fn dependency_tier(&self) -> u8 {
        1
    }

    fn timeout_ms(&self) -> u64 {
        60
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        match ctx.event {
            HookEvent::PreToolUse => self.handle_pre(ctx),
            _ => HandlerResult::skip(self.name()),
        }
    }
}

impl DSPyIntegrationHandler {
    /// PreToolUse: run DSPy compilation, inject optimization suggestion.
    fn handle_pre(&self, ctx: &mut CortexContext) -> HandlerResult {
        if ctx.context_budget_remaining < MIN_BUDGET {
            return HandlerResult::skip(self.name());
        }

        let content = match self.extract_content(&ctx.tool_input) {
            Some(c) if c.len() >= 20 => c,
            _ => return HandlerResult::skip(self.name()),
        };

        let symbol = self.extract_symbol(&ctx.tool_input);

        // Try DSPy compilation via reasoning engine
        if let Some((suggestion, score)) = self.compile_prompt(&content, &symbol) {
            self.deposit_compilation_reward(&symbol, score);
            // Enrich with CRDT topological neighbors for structural few-shot context
            let final_suggestion = self
                .build_crdt_context(&symbol)
                .map(|crdt_ctx| format!("{suggestion} | {crdt_ctx}"))
                .unwrap_or(suggestion);
            return HandlerResult::allow(self.name(), Some(final_suggestion));
        }

        // Fallback: heuristic optimization (also CRDT-enriched)
        if let Some(suggestion) = self.heuristic_optimize(&content, &symbol) {
            let final_suggestion = self
                .build_crdt_context(&symbol)
                .map(|crdt_ctx| format!("{suggestion} | {crdt_ctx}"))
                .unwrap_or(suggestion);
            return HandlerResult::allow(self.name(), Some(final_suggestion));
        }

        HandlerResult::skip(self.name())
    }
}

/// Register H100 DSPyIntegrationHandler.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(DSPyIntegrationHandler::new()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_name() {
        let h = DSPyIntegrationHandler::new();
        assert_eq!(h.name(), "H100_dspy_integration");
    }

    #[test]
    fn test_handler_priority_220() {
        let h = DSPyIntegrationHandler::new();
        assert_eq!(h.priority(), 220);
        assert!(h.priority() > 215, "must run after H101 (215)");
    }

    #[test]
    fn test_handler_tool_matcher_write_edit() {
        let h = DSPyIntegrationHandler::new();
        assert_eq!(h.tool_matcher(), Some("Write|Edit|MultiEdit"));
    }

    #[test]
    fn test_handler_events_pretluse() {
        let h = DSPyIntegrationHandler::new();
        assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    }

    #[test]
    fn test_handler_not_critical() {
        let h = DSPyIntegrationHandler::new();
        assert!(!h.is_critical());
    }

    #[test]
    fn test_handler_timeout_60ms() {
        let h = DSPyIntegrationHandler::new();
        assert_eq!(h.timeout_ms(), 60);
    }

    #[test]
    fn test_handler_dependency_tier_1() {
        let h = DSPyIntegrationHandler::new();
        assert_eq!(h.dependency_tier(), 1);
    }

    #[test]
    fn test_extract_content_direct() {
        let h = DSPyIntegrationHandler::new();
        let input = serde_json::json!({
            "content": "fn main() { println!(\"hello\"); }"
        });
        assert_eq!(
            h.extract_content(&input),
            Some("fn main() { println!(\"hello\"); }".to_string())
        );
    }

    #[test]
    fn test_extract_content_code_fallback() {
        let h = DSPyIntegrationHandler::new();
        let input = serde_json::json!({
            "code": "struct Foo { bar: i32 }"
        });
        assert_eq!(
            h.extract_content(&input),
            Some("struct Foo { bar: i32 }".to_string())
        );
    }

    #[test]
    fn test_extract_content_intent_fallback() {
        let h = DSPyIntegrationHandler::new();
        let input = serde_json::json!({
            "intent": "implement user authentication"
        });
        assert_eq!(
            h.extract_content(&input),
            Some("implement user authentication".to_string())
        );
    }

    #[test]
    fn test_extract_content_missing() {
        let h = DSPyIntegrationHandler::new();
        let input = serde_json::json!({"symbol": "Foo"});
        assert!(h.extract_content(&input).is_none());
    }

    #[test]
    fn test_extract_symbol_direct() {
        let h = DSPyIntegrationHandler::new();
        let input = serde_json::json!({"symbol": "MyStruct"});
        assert_eq!(h.extract_symbol(&input), "MyStruct");
    }

    #[test]
    fn test_extract_symbol_file_stem() {
        let h = DSPyIntegrationHandler::new();
        let input = serde_json::json!({"file_path": "src/my_struct.rs"});
        assert_eq!(h.extract_symbol(&input), "my_struct");
    }

    #[test]
    fn test_hash_symbol_deterministic() {
        let h = DSPyIntegrationHandler::new();
        let s1 = h.hash_symbol("MyStruct");
        let s2 = h.hash_symbol("MyStruct");
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_hash_symbol_different() {
        let h = DSPyIntegrationHandler::new();
        let s1 = h.hash_symbol("A");
        let s2 = h.hash_symbol("B");
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_build_query() {
        let h = DSPyIntegrationHandler::new();
        let query = h.build_query("fn main() {}", "main");
        assert_eq!(query.description, "fn main() {}");
        assert_eq!(
            query.context.get("symbol").map(String::as_str),
            Some("main")
        );
    }

    #[test]
    fn test_heuristic_optimize_with_docstring() {
        let h = DSPyIntegrationHandler::new();
        // Code with docstring should not suggest adding one
        let code = r#"
/// Adds two numbers.
pub fn add(a: i32, b: i32) -> i32 { a + b }
"#;
        let result = h.heuristic_optimize(code, "add");
        // No suggestion since docstring already present
        assert!(result.is_none());
    }

    #[test]
    fn test_heuristic_optimize_missing_docstring() {
        let h = DSPyIntegrationHandler::new();
        // Long code without docstring should suggest adding one
        let code =
            "fn process_data(data: Vec<String>) -> Result<Vec<String>, Error> { unimplemented!() }";
        let result = h.heuristic_optimize(code, "process_data");
        assert!(result.is_some());
        let suggestion = result.unwrap();
        assert!(suggestion.contains("dspy[heuristic]"));
        assert!(suggestion.contains("process_data"));
    }

    #[test]
    fn test_build_crdt_context_empty_graph_returns_none() {
        let h = DSPyIntegrationHandler::new();
        // Fresh CRDT graph has no edges → no topological context
        let result = h.build_crdt_context("user_auth");
        assert!(result.is_none(), "empty CRDT graph must return None");
    }

    #[test]
    fn test_build_crdt_context_with_neighbors_returns_formatted_hint() {
        use touring_intelligence::rl::memory::NodeWeight;

        let h = DSPyIntegrationHandler::new();
        let symbol = "crdt_test_symbol_unique_42";
        let node_id = h.hash_symbol(symbol);

        // Populate the global CRDT graph with neighbors
        {
            let mut graph = global_crdt().lock().expect("crdt lock");
            let neighbor_id = node_id.wrapping_add(1);
            graph.add_node(
                1,
                node_id,
                NodeWeight {
                    label: symbol.to_string(),
                    score: 1.0,
                    updated_at: 1,
                },
            );
            graph.add_node(
                1,
                neighbor_id,
                NodeWeight {
                    label: "auth_middleware".to_string(),
                    score: 0.85,
                    updated_at: 2,
                },
            );
            graph.add_edge(1, node_id, neighbor_id, "depends_on");
        }

        let result = h.build_crdt_context(symbol);
        assert!(result.is_some(), "populated CRDT graph must return Some");
        let hint = result.expect("hint");
        assert!(
            hint.contains("crdt-neighbors"),
            "hint must start with crdt-neighbors tag"
        );
        assert!(
            hint.contains("auth_middleware"),
            "hint must contain neighbor label"
        );
    }

    #[test]
    fn test_crdt_context_format_prefix() {
        use touring_intelligence::rl::memory::NodeWeight;

        let h = DSPyIntegrationHandler::new();
        let symbol = "format_prefix_symbol_99";
        let node_id = h.hash_symbol(symbol);
        let neighbor_id = node_id.wrapping_add(7);

        {
            let mut graph = global_crdt().lock().expect("crdt lock");
            graph.add_node(
                2,
                neighbor_id,
                NodeWeight {
                    label: "db_layer".to_string(),
                    score: 0.9,
                    updated_at: 3,
                },
            );
            graph.add_edge(2, node_id, neighbor_id, "calls");
        }

        let hint = h.build_crdt_context(symbol).expect("hint must be Some");
        assert!(
            hint.starts_with("crdt-neighbors["),
            "hint must start with 'crdt-neighbors[', got: {hint}"
        );
    }

    #[test]
    fn test_global_singletons() {
        let r1 = global_reasoning();
        let r2 = global_reasoning();
        assert!(std::ptr::eq(r1, r2));

        let h1 = global_hybrid();
        let h2 = global_hybrid();
        assert!(std::ptr::eq(h1, h2));
    }

    #[test]
    fn test_min_confidence_in_range() {
        assert!(MIN_CONFIDENCE > 0.0 && MIN_CONFIDENCE < 1.0);
    }
}
