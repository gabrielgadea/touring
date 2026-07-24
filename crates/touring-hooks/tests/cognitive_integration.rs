//! End-to-end integration test for the cognitive loop.
//!
//! Validates the full flow:
//! 1. HookRuntime initializes with cognitive engine
//! 2. Knowledge DB receives file relations and gotchas
//! 3. CognitiveRuntime populates semantic graph from knowledge
//! 4. SessionPredictor records tool invocations
//! 5. Predictions improve with more data
//! 6. Enriched context includes risk, gotchas, and predictions

use tempfile::TempDir;
use touring_hooks::HookRuntime;

#[test]
fn test_cognitive_loop_init_and_knowledge_bridge() {
    let dir = TempDir::new().unwrap();
    let project_root = dir.path();

    // Create required directory structure
    std::fs::create_dir_all(project_root.join(".claude/data")).unwrap();

    let mut runtime = HookRuntime::new(project_root).expect("runtime should initialize");

    // Initialize cognitive engine
    runtime.init_cognitive();
    assert!(
        runtime.cognitive.is_some(),
        "cognitive should be initialized"
    );

    // Record some file relations in the knowledge DB
    use touring_hooks::FileRelation;
    runtime
        .ctx
        .knowledge
        .upsert_relation(&FileRelation {
            source: "src/main.rs".into(),
            target: "src/utils.rs".into(),
            relation_type: "imports".into(),
        })
        .unwrap();
    runtime
        .ctx
        .knowledge
        .upsert_relation(&FileRelation {
            source: "src/main.rs".into(),
            target: "src/config.rs".into(),
            relation_type: "imports".into(),
        })
        .unwrap();

    // Add a gotcha
    runtime
        .ctx
        .knowledge
        .add_gotcha(
            "src/main.rs",
            "Circular import risk with utils",
            "warning",
            None,
        )
        .unwrap();

    // Re-init cognitive to pick up the new knowledge
    runtime.init_cognitive();
    let cognitive = runtime.cognitive.as_ref().unwrap();

    // Verify graph was populated from knowledge
    assert!(
        cognitive.graph().node_count() >= 2,
        "graph should have nodes from knowledge relations, got {}",
        cognitive.graph().node_count()
    );
}

#[test]
fn test_session_predictor_learns_from_tool_sequence() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".claude/data")).unwrap();

    let mut runtime = HookRuntime::new(dir.path()).unwrap();
    runtime.init_cognitive();
    let cognitive = runtime.cognitive.as_ref().unwrap();

    // Simulate a Read → Edit → Read → Edit pattern
    for _ in 0..10 {
        cognitive
            .predictor()
            .record(touring_intelligence::reasoning::ToolInvocation {
                tool_name: "Read".into(),
                timestamp_ms: 0,
                success: true,
            });
        cognitive
            .predictor()
            .record(touring_intelligence::reasoning::ToolInvocation {
                tool_name: "Edit".into(),
                timestamp_ms: 0,
                success: true,
            });
    }

    // After Read, the predictor should strongly predict Edit
    let predictions = cognitive.predictor().predict_top_k("Read", 3);
    assert!(
        !predictions.is_empty(),
        "should have predictions after training"
    );
    let first = predictions
        .first()
        .expect("predictions non-empty (checked above)");
    assert_eq!(
        first.0, "Edit",
        "after repeated Read→Edit, should predict Edit next"
    );
    assert!(
        first.1 > 0.3,
        "prediction confidence should be meaningful, got {}",
        first.1
    );
}

#[test]
fn test_cognitive_graph_compaction() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".claude/data")).unwrap();

    let mut runtime = HookRuntime::new(dir.path()).unwrap();
    runtime.init_cognitive();
    let cognitive = runtime.cognitive.as_ref().unwrap();

    // Add many nodes
    for i in 0..20 {
        let node = touring_intelligence::reasoning::semantic_graph::MemoryNode {
            id: format!("file_{i}.rs"),
            label: format!("File {i}"),
            node_type: touring_intelligence::reasoning::semantic_graph::NodeType::File,
            embedding: vec![],
            metadata: serde_json::json!(null),
            last_accessed: 0.0, // very old
            access_count: 0,
        };
        let _ = cognitive.graph().add_node(node);
    }

    assert_eq!(cognitive.graph().node_count(), 20);

    // Compact to 10
    let removed = cognitive.graph().compact(10);
    assert!(
        removed >= 10,
        "should remove at least 10 nodes, removed {removed}"
    );
    assert!(
        cognitive.graph().node_count() <= 10,
        "should have at most 10 nodes after compaction, got {}",
        cognitive.graph().node_count()
    );
}

#[test]
fn test_save_cognitive_state() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".claude/data")).unwrap();

    let mut runtime = HookRuntime::new(dir.path()).unwrap();
    runtime.init_cognitive();

    // Save should succeed even with empty graph
    let result = runtime.save_cognitive_state();
    assert!(result.is_ok(), "save_cognitive_state should succeed");
}

#[test]
fn test_focus_cache_prefetch_integration() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".claude/data")).unwrap();

    let mut runtime = HookRuntime::new(dir.path()).unwrap();
    runtime.init_cognitive();
    let cognitive = runtime.cognitive.as_ref().unwrap();

    // Prefetch warms the cache
    cognitive
        .focus_cache()
        .prefetch("predicted.rs", "context_data".into());

    // Cache should have the entry
    assert!(!cognitive.focus_cache().is_empty());

    // Retrieving should be a hit (not a miss)
    let result = cognitive
        .focus_cache()
        .get_or_compute("predicted.rs", || "should_not_compute".into());
    assert_eq!(result, "context_data");
    assert_eq!(cognitive.focus_cache().hit_count(), 1);
}
