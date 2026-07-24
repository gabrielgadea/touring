//! D41 — CGM E2E Integration Tests
//!
//! End-to-end integration tests wiring all 8 CGM pub symbols per REGRA #0.
//! Tests: CodeGraph creation, GraphAttentionLayer::from_graph + compress,
//! export_to_scip, and full round-trip serialization of CgmScip* types.

use super::{
    CgmScipDocument, CgmScipExport, CgmScipOccurrence, CgmScipSymbol, CodeGraph,
    GraphAttentionConfig, GraphAttentionLayer, GraphEdge, GraphNode, export_to_scip,
};

/// Test wiring all CGM pub symbols via REGRA #0.
/// Verifies: CodeGraph + GraphAttentionLayer integration + SCIP export round-trip.
#[test]
fn test_cgm_full_integration_wiring_all_symbols() {
    // === GraphAttentionConfig (struct) ===
    let config = GraphAttentionConfig {
        embedding_dim: 128,
        num_heads: 4,
        dropout: 0.1,
        compression_ratio: 512,
    };
    assert_eq!(config.embedding_dim, 128);
    assert_eq!(config.num_heads, 4);

    // === GraphNode + GraphEdge + CodeGraph (structs) ===
    let mut graph = CodeGraph::new();
    graph.add_node(GraphNode {
        id: "fn:main".to_string(),
        features: vec![1.0; 128],
        node_type: "function".to_string(),
    });
    graph.add_node(GraphNode {
        id: "struct:User".to_string(),
        features: vec![0.5; 128],
        node_type: "struct".to_string(),
    });
    graph.add_edge(GraphEdge {
        source: "fn:main".to_string(),
        target: "struct:User".to_string(),
        weight: 1.0,
        edge_type: "calls".to_string(),
    });

    assert_eq!(graph.num_nodes(), 2);
    assert_eq!(graph.num_edges(), 1);

    // === GraphAttentionLayer::from_graph + compress ===
    let mut layer = GraphAttentionLayer::new(config);
    layer.from_graph(&graph);
    let compressed = layer.compress("fn:main", &graph);
    assert_eq!(compressed.len(), 128);

    // === export_to_scip (fn) — CgmScipSymbol + CgmScipOccurrence + CgmScipDocument + CgmScipExport ===
    let symbols: Vec<(String, String, String)> = vec![
        (
            "main.rs".to_string(),
            "my_pkg".to_string(),
            "main".to_string(),
        ),
        (
            "user.rs".to_string(),
            "my_pkg".to_string(),
            "User".to_string(),
        ),
    ];
    let occurrences: Vec<(usize, usize, String, String, String)> = vec![
        (
            0,
            10,
            "main.rs".to_string(),
            "my_pkg".to_string(),
            "main".to_string(),
        ),
        (
            20,
            30,
            "user.rs".to_string(),
            "my_pkg".to_string(),
            "User".to_string(),
        ),
    ];

    let export = export_to_scip("test.rs", "rust", &symbols, &occurrences);
    assert_eq!(export.version, "2.0.0");
    assert_eq!(export.documents.len(), 1);
    assert_eq!(export.documents[0].symbols.len(), 2);
    assert_eq!(export.documents[0].occurrences.len(), 2);

    // === CgmScipExport JSON round-trip (CgmScipDocument + CgmScipOccurrence + CgmScipSymbol) ===
    let json = export.to_json().expect("SCIP JSON serialization failed");
    let parsed = CgmScipExport::from_json(&json).expect("SCIP JSON deserialization failed");
    assert_eq!(parsed.documents.len(), 1);
    assert_eq!(parsed.documents[0].path, "test.rs");
    assert_eq!(parsed.documents[0].language, "rust");
    assert_eq!(parsed.documents[0].symbols.len(), 2);
    assert_eq!(parsed.documents[0].occurrences.len(), 2);

    // === CgmScipSymbol individual construction + to_scip_string ===
    let sym = CgmScipSymbol::new("mod.rs", "crate", "foo");
    assert_eq!(sym.document, "mod.rs");
    assert_eq!(sym.package, "crate");
    assert_eq!(sym.descriptor, "foo");
    assert_eq!(sym.to_scip_string(), "mod.rs crate foo");

    // === CgmScipDocument add_symbol + add_occurrence ===
    let mut doc = CgmScipDocument::new("src/lib.rs", "rust");
    doc.add_symbol(CgmScipSymbol::new("src/lib.rs", "crate", "lib"));
    doc.add_occurrence(CgmScipOccurrence {
        range: (5, 10),
        symbol: CgmScipSymbol::new("src/lib.rs", "crate", "lib"),
        syntax_kind: Some("function".to_string()),
    });
    assert_eq!(doc.symbols.len(), 1);
    assert_eq!(doc.occurrences.len(), 1);

    // === CgmScipExport add_document ===
    let mut export2 = CgmScipExport::new();
    export2.add_document(doc);
    assert_eq!(export2.documents.len(), 1);
}

/// Test GraphAttentionLayer::attention_weights wiring.
#[test]
fn test_graph_attention_weights_wiring() {
    let config = GraphAttentionConfig::default();
    let mut graph = CodeGraph::new();
    graph.add_node(GraphNode {
        id: "a".to_string(),
        features: vec![1.0; 128],
        node_type: "function".to_string(),
    });
    graph.add_node(GraphNode {
        id: "b".to_string(),
        features: vec![1.0; 128],
        node_type: "function".to_string(),
    });
    graph.add_edge(GraphEdge {
        source: "a".to_string(),
        target: "b".to_string(),
        weight: 0.8,
        edge_type: "calls".to_string(),
    });

    let mut layer = GraphAttentionLayer::new(config);
    layer.from_graph(&graph);
    let weights = layer.attention_weights("a", &graph);
    assert_eq!(weights.len(), 1);
    assert_eq!(weights[0].0, "b");
    assert!((weights[0].1 - 1.0).abs() < 1e-6);
}

/// Test CodeGraph::neighbors wiring.
#[test]
fn test_code_graph_neighbors_wiring() {
    let mut graph = CodeGraph::new();
    graph.add_node(GraphNode {
        id: "x".to_string(),
        features: vec![],
        node_type: "test".to_string(),
    });
    graph.add_node(GraphNode {
        id: "y".to_string(),
        features: vec![],
        node_type: "test".to_string(),
    });
    graph.add_edge(GraphEdge {
        source: "x".to_string(),
        target: "y".to_string(),
        weight: 0.5,
        edge_type: "ref".to_string(),
    });

    let neighbors = graph.neighbors("x");
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].0, "y");
    assert!((neighbors[0].1 - 0.5).abs() < 1e-6);
}

/// Test CgmScipExport metadata round-trip.
#[test]
fn test_cgm_scip_export_with_metadata() {
    let mut export = CgmScipExport::new();
    export
        .metadata
        .insert("generator".to_string(), "touring-core".to_string());
    export
        .metadata
        .insert("version".to_string(), "1.0".to_string());

    let json = export.to_json().expect("SCIP JSON serialization failed");
    let parsed = CgmScipExport::from_json(&json).expect("SCIP JSON deserialization failed");
    assert_eq!(
        parsed
            .metadata
            .get("generator")
            .expect("metadata has generator key"),
        "touring-core"
    );
    assert_eq!(
        parsed
            .metadata
            .get("version")
            .expect("metadata has version key"),
        "1.0"
    );
}

/// Test empty CodeGraph + GraphAttentionLayer default wiring.
#[test]
fn test_cgm_empty_graph_wiring() {
    let graph = CodeGraph::new();
    assert_eq!(graph.num_nodes(), 0);
    assert_eq!(graph.num_edges(), 0);

    let layer = GraphAttentionLayer::default();
    let compressed = layer.compress("nonexistent", &graph);
    assert_eq!(compressed.len(), 128); // embedding_dim from default config
    assert!(compressed.iter().all(|&v| v == 0.0)); // all zeros for empty neighbors
}

/// Test CgmScipDocument + CgmScipExport empty round-trip.
#[test]
fn test_cgm_scip_empty_document_roundtrip() {
    let doc = CgmScipDocument::new("empty.rs", "rust");
    let mut export = CgmScipExport::new();
    export.add_document(doc);

    let json = export.to_json().expect("SCIP JSON serialization failed");
    let parsed = CgmScipExport::from_json(&json).expect("SCIP JSON deserialization failed");
    assert_eq!(parsed.documents.len(), 1);
    assert_eq!(parsed.documents[0].symbols.len(), 0);
    assert_eq!(parsed.documents[0].occurrences.len(), 0);
}
