//! Bridge module — connects touring-cognitive with touring-hooks knowledge.
//!
//! Defines the `KnowledgeSource` trait as the abstraction boundary between
//! the cognitive engine (predictions, graph, MCTS) and the hooks system
//! (SQLite knowledge DB, file relations, error patterns).
//!
//! `CognitiveRuntime` orchestrates both layers into a unified runtime that
//! auto-populates the semantic graph from accumulated hook knowledge.

use crate::reasoning::adaptive_engine::AdaptiveEngine;
use crate::reasoning::coedit_predictor::CoEditPredictor;
use crate::reasoning::focus_cache::FocusCache;
use crate::reasoning::nexus::{CognitiveCtx, CognitiveNexus};
use crate::reasoning::persistence::GraphPersistence;
use crate::reasoning::reasoning_engine::{ReasoningQuery, ReasoningResult};
use crate::reasoning::rl_bridge::RlBridge;
use crate::reasoning::semantic_graph::{MemoryNode, NodeType, SemanticGraph};
use crate::reasoning::session_predictor::{SessionPredictor, ToolInvocation};
use std::sync::{Arc, RwLock};
use std::time::Instant;

// ---------------------------------------------------------------------------
// KnowledgeSource trait — abstraction over hooks' knowledge DB
// ---------------------------------------------------------------------------
//
// The trait and its 6 record types were relocated to `touring-foundation`
// (the workspace kernel) so that `touring-storage` can host
// `impl KnowledgeSource for ThreadSafeKnowledgeDB` without forming the
// storage→intelligence→analysis→code→storage Cargo cycle. Re-exported here so
// every existing consumer (`touring_intelligence::reasoning::bridge::*`)
// resolves to the same items unchanged (A5 Path-A step-4, 2026-06-16).
pub use touring_foundation::knowledge_source::{
    BashOutcomeRecord, CoEditPair, EditRecord, FileRelation, FileRisk, GotchaRecord,
    KnowledgeSource,
};

// ---------------------------------------------------------------------------
// Enriched CognitiveCtx — extended with hooks knowledge
// ---------------------------------------------------------------------------

/// Extended cognitive context with hooks knowledge integration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EnrichedCtx {
    /// Base cognitive context (prediction + memory).
    #[serde(flatten)]
    pub base: CognitiveCtx,

    /// Risk score for the current file (0.0 = safe, 1.0 = high risk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_score: Option<f64>,

    /// Files likely needing co-editing (from CoEditPredictor + hooks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_files: Option<Vec<String>>,

    /// Active gotchas/warnings for the current file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gotchas: Option<Vec<String>>,

    /// Number of files that depend on the current file (blast radius hint).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_count: Option<u32>,

    /// Recent failed bash commands for proactive risk awareness.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bash_failures: Option<Vec<String>>,
}

impl EnrichedCtx {
    /// True if both base and enriched fields are empty.
    pub fn is_empty(&self) -> bool {
        self.base.is_empty()
            && self.risk_score.is_none()
            && self.related_files.is_none()
            && self.gotchas.is_none()
            && self.dependent_count.is_none()
            && self.bash_failures.is_none()
    }
}

// ---------------------------------------------------------------------------
// CognitiveRuntime — unified orchestrator
// ---------------------------------------------------------------------------

/// S4: TTL for cached coedit pairs (60 seconds).
const COEDIT_CACHE_TTL_SECS: u64 = 60;

/// S4: Cached coedit pairs with expiry timestamp.
#[derive(Debug)]
struct CoEditCache {
    pairs: Vec<CoEditPair>,
    fetched_at: Instant,
}

/// Unified runtime combining CognitiveNexus with a KnowledgeSource.
///
/// Auto-populates the semantic graph from hooks knowledge on startup,
/// and provides enriched context resolution that combines predictions
/// with accumulated knowledge (risk, gotchas, co-edits).
///
/// S14: Optionally integrates an AdaptiveEngine for bandit-based
/// reasoning engine selection (MCTS vs GoT vs Hybrid) by CILA level.
pub struct CognitiveRuntime {
    nexus: CognitiveNexus,
    graph: Arc<SemanticGraph>,
    predictor: Arc<SessionPredictor>,
    focus_cache: Arc<FocusCache>,
    coedit_predictor: CoEditPredictor,
    knowledge: Option<Arc<dyn KnowledgeSource>>,
    /// S4: Cached coedit pairs with TTL to avoid repeated O(N) scans.
    coedit_cache: RwLock<Option<CoEditCache>>,
    /// S14: Adaptive reasoning engine with bandit selection.
    adaptive_engine: Option<AdaptiveEngine>,
    /// S15: RL bridge for Q-table access during reasoning resolution.
    /// Allows `resolve_reasoning` to inform engine selection with learned Q-values.
    qtable_bridge: Option<Arc<dyn RlBridge>>,
}

impl std::fmt::Debug for CognitiveRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CognitiveRuntime")
            .field("nexus", &self.nexus)
            .field("graph", &self.graph)
            .field("predictor", &self.predictor)
            .field("focus_cache", &self.focus_cache)
            .field("coedit_predictor", &self.coedit_predictor)
            .field("knowledge", &self.knowledge.as_ref().map(|_| "..."))
            .field("coedit_cache", &"RwLock<Option<CoEditCache>>")
            .field(
                "adaptive_engine",
                &self.adaptive_engine.as_ref().map(|_| "AdaptiveEngine"),
            )
            .field(
                "qtable_bridge",
                &self.qtable_bridge.as_ref().map(|_| "RlBridge"),
            )
            .finish()
    }
}

impl CognitiveRuntime {
    /// Create a runtime with only the cognitive engine (no hooks integration).
    pub fn new_standalone(persistence: Arc<GraphPersistence>) -> Self {
        let graph = Arc::new(SemanticGraph::new(persistence));
        let predictor = Arc::new(SessionPredictor::new());
        let nexus = CognitiveNexus::new(graph.clone(), predictor.clone());
        Self {
            nexus,
            graph,
            predictor,
            focus_cache: Arc::new(FocusCache::new()),
            coedit_predictor: CoEditPredictor::new(),
            knowledge: None,
            coedit_cache: RwLock::new(None),
            adaptive_engine: None,
            qtable_bridge: None,
        }
    }

    /// Create a runtime connected to a hooks knowledge source.
    pub fn new_with_knowledge(
        persistence: Arc<GraphPersistence>,
        knowledge: Arc<dyn KnowledgeSource>,
    ) -> Self {
        let graph = Arc::new(SemanticGraph::new(persistence));
        let predictor = Arc::new(SessionPredictor::new());
        let nexus = CognitiveNexus::new(graph.clone(), predictor.clone());
        let mut runtime = Self {
            nexus,
            graph,
            predictor,
            focus_cache: Arc::new(FocusCache::new()),
            coedit_predictor: CoEditPredictor::new(),
            knowledge: Some(knowledge),
            coedit_cache: RwLock::new(None),
            adaptive_engine: None,
            qtable_bridge: None,
        };
        runtime.populate_from_knowledge();
        runtime
    }

    /// S14: Attach an AdaptiveEngine for bandit-based reasoning selection.
    ///
    /// When set, `resolve_with_reasoning()` will use this engine to select
    /// between MCTS/GoT/Hybrid based on CILA level and historical outcomes.
    pub fn set_adaptive_engine(&mut self, engine: AdaptiveEngine) {
        self.adaptive_engine = Some(engine);
    }

    /// S15: Attach an RL bridge for Q-table access during reasoning resolution.
    ///
    /// When set, `resolve_reasoning()` will query top-K Q-values for the
    /// query state and inject them into the reasoning context before search.
    pub fn set_qtable_bridge(&mut self, bridge: Arc<dyn RlBridge>) {
        self.qtable_bridge = Some(bridge);
    }

    /// S14/S15: Resolve a reasoning query through the adaptive engine.
    ///
    /// S15: If a Q-table bridge is configured, top-K Q-values for the query
    /// state are injected into the context before engine selection, allowing
    /// RL-informed reasoning even when the adaptive engine does not directly
    /// use an RlBridge.
    ///
    /// Returns None if no adaptive engine is configured or if the CILA level
    /// is too low (L0-L1) to warrant reasoning search.
    pub fn resolve_reasoning(&self, query: &ReasoningQuery) -> Option<ReasoningResult> {
        let enriched_query = self.enrich_with_qvalues(query);
        self.adaptive_engine.as_ref()?.search(&enriched_query)
    }

    /// S15: Enrich a ReasoningQuery with top-K Q-values from the RL bridge.
    ///
    /// If no bridge is configured or the table has no entry for the state,
    /// returns the query unchanged.
    fn enrich_with_qvalues(&self, query: &ReasoningQuery) -> ReasoningQuery {
        let Some(bridge) = &self.qtable_bridge else {
            return query.clone();
        };
        let top_actions = bridge.top_k_actions(query.root_state, 3);
        if top_actions.is_empty() {
            return query.clone();
        }
        // Serialize top-(action, q_value) pairs into context.
        let q_context: std::collections::HashMap<String, String> = top_actions
            .iter()
            .enumerate()
            .map(|(i, (action, q))| (format!("q_action_{}", i), format!("{},{}", action, q)))
            .collect();
        let mut ctx = query.context.clone();
        ctx.insert("qtable_top_k".to_string(), format!("{}", top_actions.len()));
        for (k, v) in q_context {
            ctx.insert(k, v);
        }
        ReasoningQuery {
            root_state: query.root_state,
            description: query.description.clone(),
            candidate_actions: query.candidate_actions.clone(),
            cila_level: query.cila_level,
            context: ctx,
        }
    }

    /// Access the adaptive engine (if configured).
    pub fn adaptive_engine(&self) -> Option<&AdaptiveEngine> {
        self.adaptive_engine.as_ref()
    }

    /// Populate the semantic graph from hooks knowledge.
    ///
    /// Feeds file_relations as graph edges, creating File nodes for
    /// each unique path and Related edges weighted by relation type.
    pub fn populate_from_knowledge(&mut self) {
        let knowledge = match &self.knowledge {
            Some(k) => k,
            None => return,
        };

        let relations = knowledge.file_relations();
        let mut seen_nodes = std::collections::HashSet::new();

        for rel in &relations {
            // Ensure source node exists
            if seen_nodes.insert(rel.source_path.clone()) {
                let node = MemoryNode {
                    id: rel.source_path.clone(),
                    label: rel.source_path.clone(),
                    node_type: NodeType::File,
                    embedding: Vec::new(),
                    metadata: serde_json::json!({"source": "hooks"}),
                    last_accessed: 0.0,
                    access_count: 0,
                };
                let _ = self.graph.add_node(node);
            }

            // Ensure target node exists
            if seen_nodes.insert(rel.target_path.clone()) {
                let node = MemoryNode {
                    id: rel.target_path.clone(),
                    label: rel.target_path.clone(),
                    node_type: NodeType::File,
                    embedding: Vec::new(),
                    metadata: serde_json::json!({"source": "hooks"}),
                    last_accessed: 0.0,
                    access_count: 0,
                };
                let _ = self.graph.add_node(node);
            }

            // Add edge
            let weight = match rel.relation_type.as_str() {
                "imports" => 0.8,
                "contains" => 0.9,
                _ => 0.5,
            };
            let _ = self
                .graph
                .add_edge(&rel.source_path, &rel.target_path, weight);
        }

        // S13: Train TF-IDF vectorizer on node labels so that future
        // resolve() calls produce semantically meaningful embeddings.
        let labels: Vec<&str> = seen_nodes.iter().map(|s| s.as_str()).collect();
        self.nexus.train_vectorizer(&labels);

        tracing::info!(
            nodes = seen_nodes.len(),
            edges = relations.len(),
            vectorizer_docs = labels.len(),
            "populated semantic graph from hooks knowledge"
        );
    }

    /// Feed recent edit history into the SessionPredictor.
    pub fn feed_edit_history(&self) {
        let knowledge = match &self.knowledge {
            Some(k) => k,
            None => return,
        };

        let edits = knowledge.recent_edits(64);
        for edit in &edits {
            let tool_name = match edit.edit_type.as_str() {
                "insert" | "replace" => "Edit",
                "delete" => "Edit",
                _ => "Write",
            };
            self.predictor.record(ToolInvocation {
                tool_name: tool_name.to_string(),
                timestamp_ms: 0, // historical, no real timestamp
                success: edit.error_pattern.is_none(),
            });
        }
    }

    /// Resolve enriched context for a tool invocation.
    ///
    /// Combines base cognitive context (prediction + memory) with
    /// hooks knowledge (risk, gotchas, co-edits, dependents).
    /// S2: blast_radius is now computed from dependents_of, enabling
    /// the third RRF signal that was previously always empty.
    #[tracing::instrument(skip(self))]
    pub async fn resolve_enriched(
        &self,
        tool_name: &str,
        file_path: Option<&str>,
        query_hint: &str,
    ) -> EnrichedCtx {
        let base = self.nexus.resolve(tool_name, query_hint).await;

        let knowledge = match &self.knowledge {
            Some(k) => k,
            None => {
                return EnrichedCtx {
                    base,
                    ..Default::default()
                };
            }
        };

        let file_path = match file_path {
            Some(p) => p,
            None => {
                return EnrichedCtx {
                    base,
                    ..Default::default()
                };
            }
        };

        // Knowledge enrichment (synchronous — KnowledgeSource methods are sync)
        let risk = knowledge.file_risk(file_path);
        let gotchas = knowledge.gotchas_for_file(file_path);
        let dependents = knowledge.dependents_of(file_path);

        // S4: Use cached coedit pairs
        let coedits = self.get_coedit_pairs_cached();

        // Build co-edit prediction
        let coedit_ranked: Vec<(String, f64)> = coedits
            .iter()
            .filter(|c| c.file1 == file_path || c.file2 == file_path)
            .map(|c| {
                let other = if c.file1 == file_path {
                    &c.file2
                } else {
                    &c.file1
                };
                (other.clone(), c.weight)
            })
            .collect();

        let import_ranked: Vec<(String, f64)> = dependents
            .iter()
            .enumerate()
            .map(|(i, f)| (f.clone(), 1.0 / (i as f64 + 1.0)))
            .collect();

        // S2: Compute blast_radius from dependents — each dependent file
        // is ranked by inverse position (most direct dependents first).
        // This was previously &[] which disabled 33% of the RRF fusion.
        let blast_radius: Vec<(String, f64)> = dependents
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let score = 1.0 / (i as f64 + 1.0);
                (f.clone(), score)
            })
            .collect();

        let predicted_files =
            self.coedit_predictor
                .predict(&coedit_ranked, &import_ranked, &blast_radius, 5);

        let related = if predicted_files.is_empty() {
            None
        } else {
            Some(predicted_files.into_iter().map(|(f, _)| f).collect())
        };

        let gotcha_texts: Vec<String> = gotchas.iter().map(|g| g.gotcha.clone()).collect();
        let gotchas_field = if gotcha_texts.is_empty() {
            None
        } else {
            Some(gotcha_texts)
        };

        let dep_count = if dependents.is_empty() {
            None
        } else {
            Some(dependents.len() as u32)
        };

        let risk_field = if risk.risk_score > 0.0 {
            Some(risk.risk_score)
        } else {
            None
        };

        // EC21: Expose recent bash failures for proactive risk awareness.
        // Filters the last 5 bash outcomes to failed commands only.
        let bash_failures: Option<Vec<String>> = {
            let failures: Vec<String> = knowledge
                .recent_bash_outcomes(5)
                .into_iter()
                .filter(|o| !o.success)
                .map(|o| o.command_short.clone())
                .collect();
            if failures.is_empty() {
                None
            } else {
                Some(failures)
            }
        };

        EnrichedCtx {
            base,
            risk_score: risk_field,
            related_files: related,
            gotchas: gotchas_field,
            dependent_count: dep_count,
            bash_failures,
        }
    }

    /// Access the underlying graph (for testing or direct manipulation).
    pub fn graph(&self) -> &Arc<SemanticGraph> {
        &self.graph
    }

    /// Access the underlying predictor.
    pub fn predictor(&self) -> &Arc<SessionPredictor> {
        &self.predictor
    }

    /// Access the focus cache.
    pub fn focus_cache(&self) -> &Arc<FocusCache> {
        &self.focus_cache
    }

    /// Access the nexus.
    pub fn nexus(&self) -> &CognitiveNexus {
        &self.nexus
    }

    /// S4: Get coedit pairs with TTL cache (avoids repeated O(N) scans).
    ///
    /// Returns cached pairs if within TTL, otherwise fetches fresh from knowledge.
    /// Falls back to empty vec if no knowledge source is connected.
    fn get_coedit_pairs_cached(&self) -> Vec<CoEditPair> {
        // Fast path: check cache under read lock
        if let Ok(cache) = self.coedit_cache.read() {
            if let Some(ref cached) = *cache {
                if cached.fetched_at.elapsed().as_secs() < COEDIT_CACHE_TTL_SECS {
                    return cached.pairs.clone();
                }
            }
        }

        // Slow path: fetch fresh and update cache
        let knowledge = match &self.knowledge {
            Some(k) => k,
            None => return vec![],
        };
        let pairs = knowledge.coedit_pairs();

        if let Ok(mut cache) = self.coedit_cache.write() {
            *cache = Some(CoEditCache {
                pairs: pairs.clone(),
                fetched_at: Instant::now(),
            });
        }

        pairs
    }

    /// Access the knowledge source (if connected to hooks).
    pub fn knowledge_ref(&self) -> Option<&Arc<dyn KnowledgeSource>> {
        self.knowledge.as_ref()
    }

    /// Predict top-N likely next files and prefetch their graph context.
    ///
    /// Uses co-edit pairs from knowledge to predict which files will be
    /// accessed next, then warms the FocusCache with pre-computed graph
    /// neighborhoods for those files.
    #[tracing::instrument(skip(self))]
    pub fn prefetch_predicted(&self, current_file: &str, top_n: usize) {
        if self.knowledge.is_none() {
            return;
        }

        // S4: Use cached coedit pairs
        let coedits = self.get_coedit_pairs_cached();
        let predicted: Vec<String> = coedits
            .iter()
            .filter(|c| c.file1 == current_file || c.file2 == current_file)
            .map(|c| {
                if c.file1 == current_file {
                    c.file2.clone()
                } else {
                    c.file1.clone()
                }
            })
            .take(top_n)
            .collect();

        for file in &predicted {
            let node_count = self.graph.node_count();
            self.focus_cache
                .prefetch(file, format!("prefetched:{file}:nodes={node_count}"));
        }

        if !predicted.is_empty() {
            tracing::debug!(
                current = current_file,
                prefetched = predicted.len(),
                "prefetched graph context for predicted files"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock knowledge source for testing.
    struct MockKnowledge {
        relations: Vec<FileRelation>,
        coedits: Vec<CoEditPair>,
        gotchas: Vec<GotchaRecord>,
    }

    impl MockKnowledge {
        fn new() -> Self {
            Self {
                relations: vec![
                    FileRelation {
                        source_path: "src/main.py".into(),
                        target_path: "src/utils.py".into(),
                        relation_type: "imports".into(),
                    },
                    FileRelation {
                        source_path: "src/main.py".into(),
                        target_path: "src/config.py".into(),
                        relation_type: "imports".into(),
                    },
                ],
                coedits: vec![CoEditPair {
                    file1: "src/main.py".into(),
                    file2: "src/utils.py".into(),
                    weight: 3.0,
                }],
                gotchas: vec![GotchaRecord {
                    pattern: "src/main.py".into(),
                    gotcha: "Watch out: circular import risk with utils".into(),
                    severity: "warning".into(),
                    hit_count: 2,
                }],
            }
        }
    }

    impl KnowledgeSource for MockKnowledge {
        fn file_relations(&self) -> Vec<FileRelation> {
            self.relations.clone()
        }
        fn recent_bash_outcomes(&self, _limit: usize) -> Vec<BashOutcomeRecord> {
            vec![]
        }
        fn coedit_pairs(&self) -> Vec<CoEditPair> {
            self.coedits.clone()
        }
        fn gotchas_for_file(&self, file_path: &str) -> Vec<GotchaRecord> {
            self.gotchas
                .iter()
                .filter(|g| file_path.contains(&g.pattern))
                .cloned()
                .collect()
        }
        fn recent_edits(&self, _limit: usize) -> Vec<EditRecord> {
            vec![]
        }
        fn file_risk(&self, _file_path: &str) -> FileRisk {
            FileRisk {
                risk_score: 0.35,
                recent_failures: 2,
                gotcha_count: 1,
                dependent_count: 3,
            }
        }
        fn dependents_of(&self, _file_path: &str) -> Vec<String> {
            vec!["tests/test_main.py".into(), "src/app.py".into()]
        }
        fn file_count(&self) -> usize {
            3
        }
        fn relation_count(&self) -> usize {
            2
        }
    }

    #[test]
    fn test_standalone_runtime_creates() {
        let p = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        let rt = CognitiveRuntime::new_standalone(p);
        assert_eq!(rt.graph().node_count(), 0);
    }

    #[test]
    fn test_populate_from_knowledge() {
        let p = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        let k = Arc::new(MockKnowledge::new());
        let rt = CognitiveRuntime::new_with_knowledge(p, k);

        // 3 unique nodes: main.py, utils.py, config.py
        assert_eq!(rt.graph().node_count(), 3);
    }

    #[tokio::test]
    async fn test_resolve_enriched_with_knowledge() {
        let p = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        let k = Arc::new(MockKnowledge::new());
        let rt = CognitiveRuntime::new_with_knowledge(p, k);

        let ctx = rt
            .resolve_enriched("Edit", Some("src/main.py"), "editing main")
            .await;

        assert!(!ctx.is_empty());
        assert!(ctx.risk_score.is_some());
        assert_eq!(ctx.risk_score.unwrap(), 0.35);
        assert!(ctx.gotchas.is_some());
        assert!(ctx.dependent_count.is_some());
        assert_eq!(ctx.dependent_count.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_resolve_enriched_without_knowledge() {
        let p = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        let rt = CognitiveRuntime::new_standalone(p);

        let ctx = rt.resolve_enriched("Read", Some("any.rs"), "query").await;

        // No knowledge = only base context
        assert!(ctx.risk_score.is_none());
        assert!(ctx.gotchas.is_none());
    }

    #[tokio::test]
    async fn test_resolve_enriched_no_file_path() {
        let p = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        let k = Arc::new(MockKnowledge::new());
        let rt = CognitiveRuntime::new_with_knowledge(p, k);

        let ctx = rt.resolve_enriched("Bash", None, "ls -la").await;

        // No file path = no file-specific enrichment
        assert!(ctx.risk_score.is_none());
        assert!(ctx.gotchas.is_none());
    }

    #[test]
    fn test_enriched_ctx_serialization_skips_none() {
        let ctx = EnrichedCtx {
            base: CognitiveCtx::empty(),
            risk_score: Some(0.7),
            related_files: None,
            gotchas: None,
            dependent_count: None,
            bash_failures: None,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("risk_score"));
        assert!(!json.contains("related_files"));
        assert!(!json.contains("gotchas"));
    }
}
