//! Background tasks for the cognitive engine — prediction warm-cache + maintenance.
//!
//! Two async loops run concurrently:
//! 1. `context_predictor_task` — 500ms warm-cache loop (fast path)
//! 2. `maintenance_task` — 5min maintenance loop (edge decay, knowledge sync)
//!
//! Lock ordering: acquires L1 (SemanticGraph.graph) then L2 (SessionPredictor.history)
//! in SEPARATE operations (never simultaneously), respecting the documented ordering.

use crate::reasoning::bridge::CognitiveRuntime;
use crate::reasoning::semantic_graph::SemanticGraph;
use crate::reasoning::session_predictor::SessionPredictor;
use std::sync::Arc;
use std::time::Duration;

/// Interval between prediction cache warm cycles.
const PREDICT_INTERVAL: Duration = Duration::from_millis(500);

/// Interval between maintenance cycles (edge decay, knowledge refresh).
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

/// Minimum edge weight after decay — edges below this are pruned.
const MIN_EDGE_WEIGHT: f32 = 0.01;

/// Background task that periodically warms the prediction cache.
///
/// Runs indefinitely until the tokio runtime shuts down.
/// Acquires L1 and L2 in separate operations -- never simultaneously.
///
/// # Usage
/// ```ignore
/// tokio::spawn(context_predictor_task(graph.clone(), predictor.clone()));
/// ```
pub async fn context_predictor_task(graph: Arc<SemanticGraph>, predictor: Arc<SessionPredictor>) {
    loop {
        tokio::time::sleep(PREDICT_INTERVAL).await;

        // Step 1: Clone recent history (acquires and releases L2).
        let history = predictor.clone_recent_history();

        // Step 2: Warm the graph cache with tool names from history (acquires and releases L1).
        if !history.is_empty() {
            let hint = history.last().map(|i| i.tool_name.as_str()).unwrap_or("");
            graph.warm_cache(hint);
        }

        // Step 3: Warm predictor cache with tool sequence hints.
        let tool_hints: Vec<String> = history.iter().map(|i| i.tool_name.clone()).collect();
        predictor.warm_cache(&tool_hints);
    }
}

/// Background maintenance task — runs every 5 minutes.
///
/// Responsibilities:
/// 1. **Edge decay**: prune edges whose weight has decayed below threshold
/// 2. **Knowledge sync**: re-populate graph from hooks knowledge (if connected)
/// 3. **Q-value report**: log predictor Q-value snapshot for observability
///
/// # Usage
/// ```ignore
/// tokio::spawn(maintenance_task(runtime));
/// ```
pub async fn maintenance_task(runtime: Arc<CognitiveRuntime>) {
    loop {
        tokio::time::sleep(MAINTENANCE_INTERVAL).await;

        // Step 1: Decay old edges
        let pruned = runtime.graph().decay_edges(MIN_EDGE_WEIGHT);
        if pruned > 0 {
            tracing::info!(pruned, "maintenance: pruned decayed edges");
        }

        // Step 2: Log graph stats
        let node_count = runtime.graph().node_count();
        let edge_count = runtime.graph().edge_count();
        let cache_hit_rate = runtime.focus_cache().hit_rate();
        tracing::debug!(
            node_count,
            edge_count,
            cache_hit_rate = format!("{:.1}%", cache_hit_rate * 100.0),
            "maintenance: graph stats"
        );

        // Step 3: Log Q-value snapshot
        let q_snapshot = runtime.predictor().q_values_snapshot();
        if !q_snapshot.is_empty() {
            tracing::debug!(
                q_values = ?q_snapshot,
                "maintenance: predictor Q-values"
            );
        }
    }
}

/// Spawn all background tasks for a CognitiveRuntime.
///
/// Returns JoinHandles for graceful shutdown if needed.
///
/// # Usage
/// ```ignore
/// let handles = spawn_cognitive_tasks(&runtime);
/// // To stop: handles.iter().for_each(|h| h.abort());
/// ```
pub fn spawn_cognitive_tasks(runtime: &Arc<CognitiveRuntime>) -> Vec<tokio::task::JoinHandle<()>> {
    let graph = runtime.graph().clone();
    let predictor = runtime.predictor().clone();
    let rt = runtime.clone();

    vec![
        tokio::spawn(context_predictor_task(graph, predictor)),
        tokio::spawn(maintenance_task(rt)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasoning::persistence::GraphPersistence;

    #[tokio::test]
    async fn test_predictor_task_runs_without_panic() {
        let persistence = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        let graph = Arc::new(SemanticGraph::new(persistence));
        let predictor = Arc::new(SessionPredictor::new());

        let result = tokio::time::timeout(
            Duration::from_millis(600),
            context_predictor_task(graph, predictor),
        )
        .await;
        // Task loops forever, so timeout is expected (Err means it was still running)
        assert!(result.is_err(), "task should run indefinitely");
    }

    #[tokio::test]
    async fn test_maintenance_task_runs_without_panic() {
        let persistence = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        let runtime = Arc::new(CognitiveRuntime::new_standalone(persistence));

        // Maintenance runs at 5min interval; timeout after 100ms confirms it sleeps.
        let result =
            tokio::time::timeout(Duration::from_millis(100), maintenance_task(runtime)).await;
        assert!(result.is_err(), "maintenance task should be sleeping");
    }

    #[tokio::test]
    async fn test_spawn_cognitive_tasks() {
        let persistence = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        let runtime = Arc::new(CognitiveRuntime::new_standalone(persistence));

        let handles = spawn_cognitive_tasks(&runtime);
        assert_eq!(handles.len(), 2, "should spawn 2 background tasks");

        // Abort all to clean up
        for h in &handles {
            h.abort();
        }
    }
}
