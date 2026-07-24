//! Cognitive, Inferlets, ToolPrediction, CrdtGraph, Evolution, and MetricsExport implementations.

use touring_intelligence::reasoning::EnrichedCtx;
use touring_intelligence::rl::EvolutionAnalyzer;
use touring_intelligence::rl::rl::tiny_transformer::ToolPredictor as ToolPredictorTrait;
use touring_intelligence::rl::rl::tiny_transformer::{PredictionContext, ToolPrediction};

use super::traits::{Cognitive, CrdtGraph, Evolution, Inferlets, MetricsExport, ToolPredictor};
use crate::inferlets::{InferletKind, InferletService};
use crate::metrics::RuntimeMetrics;
use crate::runtime::HookRuntime;

impl Cognitive for HookRuntime {
    fn init_cognitive(&mut self) {
        let data_dir = self.project_root.join(".claude").join("data");
        let graph_path = data_dir.join("cognitive_graph.json");
        let persistence = std::sync::Arc::new(
            touring_intelligence::reasoning::GraphPersistence::new(graph_path),
        );

        let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(&self.project_root);
        let knowledge_db = match crate::knowledge::ThreadSafeKnowledgeDB::new(&db_path) {
            Ok(db) => db,
            Err(e) => {
                tracing::warn!("cognitive init failed (knowledge DB): {e}");
                let mut cognitive =
                    touring_intelligence::reasoning::CognitiveRuntime::new_standalone(persistence);
                cognitive.set_adaptive_engine(
                    touring_intelligence::reasoning::AdaptiveEngine::with_defaults(),
                );
                self.cognitive = Some(cognitive);
                self.enrichment_active = true;
                return;
            }
        };

        let knowledge_arc: std::sync::Arc<
            dyn touring_intelligence::reasoning::bridge::KnowledgeSource,
        > = std::sync::Arc::new(knowledge_db);
        let mut cognitive = touring_intelligence::reasoning::CognitiveRuntime::new_with_knowledge(
            persistence,
            knowledge_arc,
        );
        // Wire AdaptiveEngine so cli_mcts_search → CognitiveRuntime::resolve_reasoning works
        cognitive
            .set_adaptive_engine(touring_intelligence::reasoning::AdaptiveEngine::with_defaults());
        self.cognitive = Some(cognitive);
        self.enrichment_active = true;
        tracing::info!("cognitive engine initialized with AdaptiveEngine (MCTS + Hybrid)");
    }

    async fn resolve_cognitive_context(
        &self,
        tool_name: &str,
        file_path: Option<&str>,
        query_hint: &str,
    ) -> Option<EnrichedCtx> {
        let cognitive = self.cognitive.as_ref()?;
        Some(
            cognitive
                .resolve_enriched(tool_name, file_path, query_hint)
                .await,
        )
    }

    fn save_cognitive_state(&self) -> Result<(), String> {
        if let Some(ref cognitive) = self.cognitive {
            let removed = cognitive.graph().compact(1000);
            if removed > 0 {
                tracing::info!(removed, "compacted cognitive graph before save");
            }
            tracing::info!("cognitive state checkpointed");
        }
        Ok(())
    }

    fn cognitive_ref(&self) -> Option<&touring_intelligence::reasoning::CognitiveRuntime> {
        self.cognitive.as_ref()
    }
}

impl Inferlets for HookRuntime {
    async fn init_inferlets(&mut self, _pool_size: usize) {
        #[cfg(feature = "inferlets-wasm")]
        {
            use crate::inferlets_assets::load_all_inferlets;

            let service = match InferletService::new() {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        "InferletService creation failed: {e} — WASM inferlets disabled"
                    );
                    return;
                }
            };

            match load_all_inferlets(&service, 4).await {
                Ok(()) => {
                    tracing::info!("WASM inferlets loaded");
                    self.ctx.inferlet_service = Some(service);
                }
                Err(e) => {
                    tracing::warn!("WASM inferlet loading partially failed: {e}");
                }
            }
        }

        #[cfg(not(feature = "inferlets-wasm"))]
        {
            let _ = self;
            tracing::debug!("inferlets-wasm feature not enabled");
        }
    }

    async fn evaluate_inferlet(
        &self,
        kind: InferletKind,
        input: &str,
    ) -> Option<touring_bindings::wasm::PluginResult> {
        let service = self.ctx.inferlet_service.as_ref()?;
        service.evaluate(kind, input).await.ok()
    }

    fn inferlet_service_ref(&self) -> Option<&InferletService> {
        self.ctx.inferlet_service.as_ref()
    }
}

impl ToolPredictor for HookRuntime {
    fn predict_next_tools(&self, tool_history: &[String], cila_level: u8) -> Vec<ToolPrediction> {
        let Some(ref predictor) = self.learning.predictor else {
            return vec![];
        };
        let ctx = PredictionContext {
            recent_tools: tool_history.to_vec(),
            cila_level,
            session_id: String::new(),
        };
        predictor.predict(&ctx, 3)
    }

    fn predictor_ref(
        &self,
    ) -> Option<&touring_intelligence::rl::rl::tiny_transformer::TinyTransformerPredictor> {
        self.learning.predictor.as_ref()
    }
}

impl CrdtGraph for HookRuntime {
    fn record_file_relation(&mut self, from_file: &str, to_file: &str, relation: &str) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use touring_intelligence::rl::memory::crdt_graph::{CrdtNodeId, NodeWeight};

        fn hash_path(path: &str) -> CrdtNodeId {
            let mut hasher = DefaultHasher::new();
            path.hash(&mut hasher);
            hasher.finish()
        }

        let graph = self.learning.crdt_graph.get_or_insert_with(
            touring_intelligence::rl::memory::crdt_graph::CrdtSemanticGraph::new,
        );
        let from_id = hash_path(from_file);
        let to_id = hash_path(to_file);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        graph.add_node(
            1,
            from_id,
            NodeWeight {
                label: from_file.to_string(),
                score: 1.0,
                updated_at: now,
            },
        );
        graph.add_node(
            1,
            to_id,
            NodeWeight {
                label: to_file.to_string(),
                score: 1.0,
                updated_at: now,
            },
        );
        graph.add_edge(1, from_id, to_id, relation);
    }

    fn save_crdt_graph(&self) -> Result<(), String> {
        if let Some(ref graph) = self.learning.crdt_graph {
            let path = self.project_root.join(".claude/data/crdt_graph.rkyv");
            graph
                .save_to_mmap(&path)
                .map_err(|e| format!("Failed to save CRDT graph: {e}"))?;
        }
        Ok(())
    }

    fn load_crdt_graph(&mut self) -> Result<(), String> {
        let path = self.project_root.join(".claude/data/crdt_graph.rkyv");
        if !path.exists() {
            // Cold start on first run is legal — no persisted file yet.
            return Ok(());
        }
        match touring_intelligence::rl::memory::crdt_graph::CrdtSemanticGraph::load_from_mmap(&path)
        {
            Ok(graph) => {
                self.learning.crdt_graph = Some(graph);
                tracing::info!(path = %path.display(), "crdt_graph: warm-start load succeeded");
                Ok(())
            }
            Err(e) => Err(format!(
                "Failed to load CRDT graph from {}: {e}",
                path.display()
            )),
        }
    }

    fn crdt_graph_ref(
        &self,
    ) -> Option<&touring_intelligence::rl::memory::crdt_graph::CrdtSemanticGraph> {
        self.learning.crdt_graph.as_ref()
    }
}

impl Evolution for HookRuntime {
    fn evolution_analyzer_ref(&self) -> Option<&EvolutionAnalyzer> {
        self.learning.evolution_analyzer.as_ref()
    }
}

impl MetricsExport for HookRuntime {
    fn export_metrics(&self, qtable: Option<&touring_intelligence::rl::QTable>) -> RuntimeMetrics {
        use crate::metrics::{BanditMetrics, CacheMetrics, HookMetrics, RlMetrics};

        let hooks = if let Some(ref qa) = self.ctx.quality_assessment {
            let stats = &qa.streaming_stats;
            HookMetrics {
                total_hooks_fired: stats.total(),
                success_count: stats.success_count,
                failure_count: stats.failure_count,
                avg_latency_ms: stats.avg_latency_ms(),
                max_latency_ms: stats.max_latency_ms,
                success_rate: stats.success_rate(),
            }
        } else {
            HookMetrics::default()
        };

        let rl = qtable.map(|qt| {
            let m = qt.metrics();
            RlMetrics {
                td_error_ema: m.td_error_ema(),
                avg_reward: m.avg_reward(),
                total_updates: m.total_updates(),
                is_converging: m.is_converging(),
                is_diverging: m.is_diverging(),
            }
        });

        let bandit = self.learning.bandit.as_ref().map(|b| {
            let snapshot = b.export_snapshot();
            BanditMetrics {
                total_pulls: b.total_pulls(),
                num_arms: b.num_arms(),
                bandit_type: snapshot.bandit_type,
            }
        });

        let cache = CacheMetrics {
            hit_rate: self.cache_hit_rate(),
        };

        let cognitive = self
            .cognitive
            .as_ref()
            .map(|rt| crate::metrics::CognitiveMetrics {
                graph_node_count: rt.graph().node_count(),
                graph_edge_count: rt.graph().edge_count(),
                focus_cache_hit_rate: rt.focus_cache().hit_rate(),
                prediction_accuracy: self
                    .predictor_ref()
                    .and_then(|p| p.accuracy())
                    .unwrap_or(0.0),
                is_connected: true,
                analysis_quality: None,
            });

        RuntimeMetrics {
            hooks,
            rl,
            bandit,
            cognitive,
            cache,
            session_turn: self.session_turn(),
        }
    }
}
