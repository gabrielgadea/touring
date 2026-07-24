//! CLI cognitive handlers (`cli_cognitive_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::runtime::HookRuntime;

/// Reports cognitive runtime health as JSON: whether the graph and predictor are initialized and non-empty.
pub fn cli_cognitive_metrics(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let cognitive_healthy = rt.cognitive.is_some();
    let graph_empty = rt.cognitive.as_ref().map_or(true, |c| {
        let g = c.graph();
        g.node_count() == 0 || g.edge_count() == 0
    });
    if graph_empty {
        tracing::warn!(
            code = "COGNITIVE_EMPTY",
            severity = "warning",
            message =
                "Cognitive graph empty for project — cognitive features may not function correctly",
            "W-cognitive: empty cognitive graph detected"
        );
    }
    let metrics = if cognitive_healthy {
        serde_json::json!(
            { "initialized" : true, "has_graph" : ! graph_empty, "has_predictor" : rt
            .learning.predictor.is_some(), }
        )
    } else {
        serde_json::json!(
            { "initialized" : false, "has_graph" : false, "has_predictor" : false, }
        )
    };
    metrics.to_string()
}
/// Reports per-engine status (cognitive runtime, predictor, CRDT graph, online RL) as JSON.
pub fn cli_cognitive_engines(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let cognitive_status = if rt.cognitive.is_some() {
        "healthy"
    } else {
        "not_initialized"
    };
    let cognitive_graph = if rt.cognitive.is_some() {
        "healthy"
    } else {
        "inactive"
    };
    let predictor_status = if rt.learning.predictor.is_some() {
        "healthy"
    } else {
        "inactive"
    };
    let crdt_status = if rt.learning.crdt_graph.is_some() {
        "healthy"
    } else {
        "inactive"
    };
    let online_rl_status = if rt.learning.online_rl.is_some() {
        "healthy"
    } else {
        "inactive"
    };
    let engines = serde_json::json!({
        "cognitive_runtime": { "status": cognitive_status, "graph": cognitive_graph },
        "predictor": { "status": predictor_status },
        "crdt_graph": { "status": crdt_status },
        "online_rl": { "status": online_rl_status },
    });
    engines.to_string()
}
