//! End-to-end integration tests for touring-cortex runtime.

#[test]
fn cortex_fascicle_dispatcher_creation() {
    use std::sync::mpsc;
    use touring_cortex::fascicles::dispatcher::FascicleDispatcher;
    use touring_cortex::fascicles::registry::HandlerRegistry;

    let (_tx, rx) = mpsc::sync_channel(256);
    let _dispatcher = FascicleDispatcher::new(HandlerRegistry::new(), rx);
}

#[test]
fn cortex_pipeline_creation() {
    use touring_cortex::pipeline::Pipeline;

    let _pipeline = Pipeline::new();
}

#[test]
fn cortex_hook_event_parse() {
    use touring_cortex::types::HookEvent;

    let events = vec![
        "session-start",
        "session-stop",
        "pre-read",
        "post-read",
        "pre-bash",
        "post-bash",
        "pre-edit",
        "post-edit",
        "pre-compact",
        "post-compact",
    ];

    for event_str in events {
        let parsed = HookEvent::parse(event_str);
        assert!(parsed.is_some(), "should parse '{}'", event_str);
    }
}

#[test]
fn cortex_hook_event_unknown() {
    use touring_cortex::types::HookEvent;

    let unknown = HookEvent::parse("completely-unknown-event");
    assert!(unknown.is_none(), "unknown event should return None");
}

#[test]
fn cortex_handler_result_skip() {
    use touring_cortex::types::HandlerResult;

    let result = HandlerResult::skip("test-handler");
    assert!(result.context_lines.is_empty());
    assert_eq!(result.handler_name, "test-handler");
}

#[test]
fn cortex_handler_result_allow() {
    use touring_cortex::types::HandlerResult;

    // Context lines are collected by splitting on newlines
    let result = HandlerResult::allow("test-handler", Some("single context line".to_string()));
    assert_eq!(result.context_lines.len(), 1);
    assert_eq!(result.handler_name, "test-handler");
}

#[test]
fn cortex_handler_result_block() {
    use touring_cortex::types::HandlerResult;

    let result = HandlerResult::block("test-handler", "blocked for reason".to_string());
    assert!(result.context_lines.is_empty());
    assert_eq!(result.handler_name, "test-handler");
}

#[test]
fn cortex_fusion_rrf_empty() {
    use touring_cortex::fusion::reciprocal_rank_fusion;

    let empty: Vec<(Vec<String>, f64)> = vec![];
    let result = reciprocal_rank_fusion(&empty, 60.0);
    assert!(result.is_empty());
}

#[test]
fn cortex_fusion_rrf_single() {
    use touring_cortex::fusion::reciprocal_rank_fusion;

    // T=String, each list is (Vec<String>, weight)
    let single: Vec<(Vec<String>, f64)> = vec![(vec!["doc1".to_string()], 1.0)];
    let result = reciprocal_rank_fusion(&single, 60.0);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, "doc1");
}

#[test]
fn cortex_handler_registry_empty() {
    use touring_cortex::fascicles::registry::HandlerRegistry;

    let registry = HandlerRegistry::new();
    assert_eq!(registry.len(), 0, "empty registry should have 0 handlers");
}

#[test]
fn cortex_evidence_new() {
    use rustc_hash::FxHashMap;
    use std::sync::Arc;
    use touring_cortex::fascicles::evidence::{Confidence, Evidence, Priority};

    let confidence = Confidence::new(0.95);
    let priority = Priority::new(128);
    let source: Arc<str> = Arc::from("test-source");
    let payload = FxHashMap::default();

    let _evidence = Evidence::new(confidence, priority, source, payload);
}

#[test]
fn cortex_evidence_with_defaults() {
    use std::sync::Arc;
    use touring_cortex::fascicles::evidence::Evidence;

    let source: Arc<str> = Arc::from("test-handler");
    let evidence = Evidence::with_defaults(source);
    let _confidence: f64 = evidence.confidence_value();
    let _priority: u8 = evidence.priority_value();
}

#[test]
fn cortex_confidence_bounds() {
    use touring_cortex::fascicles::evidence::Confidence;

    let high = Confidence::new(0.9);
    let low = Confidence::new(-0.5);
    let too_high = Confidence::new(1.5);

    assert!((high.value() - 0.9).abs() < 1e-10);
    assert!((low.value() - 0.0).abs() < 1e-10); // clamped to 0
    assert!((too_high.value() - 1.0).abs() < 1e-10); // clamped to 1
}

#[test]
fn cortex_priority_bounds() {
    use touring_cortex::fascicles::evidence::Priority;

    let normal = Priority::new(100);
    let zero = Priority::new(0);
    let max = Priority::new(255);

    assert_eq!(normal.value(), 100);
    assert_eq!(zero.value(), 0);
    assert_eq!(max.value(), 255);
}

#[test]
fn cortex_fusion_rrf_multiple_lists() {
    use touring_cortex::fusion::reciprocal_rank_fusion;

    // Correct: Vec<(Vec<String>, f64)> where f64 is the weight per list
    let lists: Vec<(Vec<String>, f64)> = vec![
        (vec!["doc1".to_string(), "doc2".to_string()], 1.0),
        (vec!["doc2".to_string(), "doc3".to_string()], 1.0),
    ];
    let result = reciprocal_rank_fusion(&lists, 60.0);
    // Should return results for all unique items
    assert!(!result.is_empty());
    assert_eq!(result.len(), 3, "should have 3 unique docs");
    // All scores should be finite
    for (name, score) in &result {
        assert!(score.is_finite(), "score for {:?} should be finite", name);
    }
}

#[test]
fn cortex_hook_event_debug() {
    use touring_cortex::types::HookEvent;

    let event = HookEvent::parse("pre-read").unwrap();
    // Should format without panic
    let debug = format!("{:?}", event);
    assert!(!debug.is_empty());
}
