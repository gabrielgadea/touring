//! CognitiveNexus — coordinates SemanticGraph + SessionPredictor into CognitiveCtx.
//!
//! Lock ordering: L1 (SemanticGraph.graph) before L2 (SessionPredictor.history)
//! applies only when a SINGLE THREAD holds BOTH locks simultaneously.
//! In `resolve()`, each spawn_blocking task holds exactly ONE lock — no ordering
//! constraint applies between the two concurrent tasks spawned via tokio::join!().

use crate::reasoning::semantic_graph::SemanticGraph;
use crate::reasoning::session_predictor::SessionPredictor;
use crate::reasoning::tfidf::TfIdfVectorizer;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Cognitive context injected into MCP tool responses.
/// All fields are Option — serialization skips None values.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CognitiveCtx {
    /// Predicted next tool/action label, if the predictor produced one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_next: Option<String>,
    /// Confidence in `predicted_next`, in the range 0.0 to 1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prediction_confidence: Option<f32>,
    /// Surfaced memory/context snippet relevant to the current state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_surface: Option<String>,
    /// Suggested next tool to invoke, derived from the semantic graph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_next_tool: Option<String>,
}

impl CognitiveCtx {
    /// Return an empty context (all fields None).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Return true if all fields are None (nothing to inject).
    pub fn is_empty(&self) -> bool {
        self.predicted_next.is_none()
            && self.prediction_confidence.is_none()
            && self.memory_surface.is_none()
            && self.suggested_next_tool.is_none()
    }
}

/// Coordinates SemanticGraph and SessionPredictor to produce CognitiveCtx.
///
/// S13: Uses TF-IDF embeddings instead of pseudo-embeddings for graph retrieval.
/// The vectorizer is trained on node labels as they are added to the graph,
/// and produces semantically meaningful embeddings for similarity search.
pub struct CognitiveNexus {
    graph: Arc<SemanticGraph>,
    predictor: Arc<SessionPredictor>,
    /// S13: TF-IDF vectorizer for producing semantic embeddings from text.
    vectorizer: Arc<RwLock<TfIdfVectorizer>>,
}

impl std::fmt::Debug for CognitiveNexus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CognitiveNexus")
            .field("graph", &self.graph)
            .field("predictor", &self.predictor)
            .field("vectorizer", &"TfIdfVectorizer")
            .finish()
    }
}

impl CognitiveNexus {
    /// Create a new CognitiveNexus with the given graph and predictor.
    pub fn new(graph: Arc<SemanticGraph>, predictor: Arc<SessionPredictor>) -> Self {
        Self {
            graph,
            predictor,
            vectorizer: Arc::new(RwLock::new(TfIdfVectorizer::new())),
        }
    }

    /// Train the TF-IDF vectorizer on a corpus of document strings.
    ///
    /// Should be called after populating the semantic graph (e.g., from knowledge
    /// source node labels) so that IDF statistics reflect the actual corpus.
    pub fn train_vectorizer(&self, documents: &[&str]) {
        if let Ok(mut v) = self.vectorizer.write() {
            for doc in documents {
                v.add_document(doc);
            }
        }
    }

    /// Access the vectorizer (for external training or embedding).
    pub fn vectorizer(&self) -> &Arc<RwLock<TfIdfVectorizer>> {
        &self.vectorizer
    }

    /// Predict the next tool given the current tool name.
    ///
    /// Delegates to `SessionPredictor::predict_next` for Track B lure generation.
    /// Returns the tool name and confidence score.
    pub fn predict_next(&self, current_tool: &str) -> Option<(String, f64)> {
        self.predictor.predict_next(current_tool)
    }

    /// Record a tool invocation for the session predictor.
    ///
    /// Updates transition counts and Q-values for future predictions.
    pub fn record_invocation(&self, tool_name: &str, success: bool) {
        use crate::reasoning::session_predictor::ToolInvocation;
        let invocation = ToolInvocation {
            tool_name: tool_name.to_string(),
            success,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };
        self.predictor.record(invocation);
        self.predictor.register_outcome(tool_name, success);
    }

    /// Timeout for each spawn_blocking task (graph retrieval / prediction).
    const RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

    /// Resolve cognitive context for a tool invocation.
    ///
    /// Uses tokio::join!() to parallelize:
    /// - graph retrieval (L1 lock)
    /// - session prediction (L2 lock)
    ///
    /// Each spawn_blocking task is wrapped in a 2-second timeout. On timeout
    /// or panic, the corresponding field is left as None and a warning is logged.
    ///
    /// Returns CognitiveCtx with populated fields where available.
    pub async fn resolve(&self, tool_name: &str, query_hint: &str) -> CognitiveCtx {
        let graph = Arc::clone(&self.graph);
        let predictor = Arc::clone(&self.predictor);
        let vectorizer = Arc::clone(&self.vectorizer);
        let tool_name_owned = tool_name.to_string();
        let query_hint_owned = query_hint.to_string();

        // Parallel resolution: graph retrieval + session prediction,
        // each bounded by RESOLVE_TIMEOUT to prevent indefinite hangs.
        let (memory_result, prediction_result) = tokio::join!(
            tokio::time::timeout(
                Self::RESOLVE_TIMEOUT,
                tokio::task::spawn_blocking(move || {
                    // S13: Use TF-IDF embedding instead of pseudo-embedding.
                    // The vectorizer produces a semantically meaningful dense vector
                    // from the query hint, enabling real similarity-based retrieval.
                    let embedding = match vectorizer.read() {
                        Ok(v) => v.embed(&query_hint_owned),
                        _ => {
                            // Fallback: zero vector if lock poisoned
                            vec![0.0; 128]
                        }
                    };
                    graph
                        .retrieve_by_embedding(&embedding, 1)
                        .into_iter()
                        .next()
                        .map(|n| n.label)
                }),
            ),
            tokio::time::timeout(
                Self::RESOLVE_TIMEOUT,
                tokio::task::spawn_blocking(move || predictor.predict_next(&tool_name_owned)),
            )
        );

        let memory_surface = match memory_result {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                tracing::warn!("graph spawn_blocking panicked: {e}");
                None
            }
            Err(_) => {
                tracing::warn!(
                    "graph retrieval timed out after {}s, returning empty context",
                    Self::RESOLVE_TIMEOUT.as_secs()
                );
                None
            }
        };
        let prediction = match prediction_result {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                tracing::warn!("predictor spawn_blocking panicked: {e}");
                None
            }
            Err(_) => {
                tracing::warn!(
                    "session prediction timed out after {}s, returning empty context",
                    Self::RESOLVE_TIMEOUT.as_secs()
                );
                None
            }
        };

        CognitiveCtx {
            predicted_next: prediction.as_ref().map(|(t, _)| t.clone()),
            prediction_confidence: prediction.as_ref().map(|(_, c)| *c as f32),
            memory_surface,
            suggested_next_tool: None, // populated by Track A suggest_next_tool_simple
        }
    }
}

impl Default for CognitiveNexus {
    fn default() -> Self {
        use crate::reasoning::persistence::GraphPersistence;
        let persistence = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        Self {
            graph: Arc::new(SemanticGraph::new(persistence)),
            predictor: Arc::new(SessionPredictor::new()),
            vectorizer: Arc::new(RwLock::new(TfIdfVectorizer::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cognitive_ctx_empty() {
        let ctx = CognitiveCtx::empty();
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_cognitive_ctx_with_prediction() {
        let ctx = CognitiveCtx {
            predicted_next: Some("Edit".to_string()),
            prediction_confidence: Some(0.85),
            memory_surface: None,
            suggested_next_tool: None,
        };
        assert!(!ctx.is_empty());
    }

    #[test]
    fn test_cognitive_ctx_serialization_skips_none() {
        let ctx = CognitiveCtx {
            predicted_next: Some("Read".to_string()),
            prediction_confidence: None,
            memory_surface: None,
            suggested_next_tool: None,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("predicted_next"));
        assert!(!json.contains("memory_surface"));
    }

    #[test]
    fn test_nexus_default() {
        let nexus = CognitiveNexus::default();
        let _ = nexus;
    }

    #[tokio::test]
    async fn test_nexus_resolve_empty_state() {
        let nexus = CognitiveNexus::default();
        let ctx = nexus.resolve("Read", "test query").await;
        assert!(ctx.predicted_next.is_none());
        assert!(ctx.prediction_confidence.is_none());
        assert!(ctx.suggested_next_tool.is_none());
    }
}
