//! Criterion benchmarks for SemanticGraph node/edge operations.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::path::PathBuf;
use std::sync::Arc;
use touring_intelligence::reasoning::persistence::GraphPersistence;
use touring_intelligence::reasoning::semantic_graph::{MemoryNode, NodeType, SemanticGraph};

fn make_persistence() -> Arc<GraphPersistence> {
    Arc::new(GraphPersistence::new(PathBuf::from(":memory:")))
}

fn make_node(id: &str, dim: usize) -> MemoryNode {
    let embedding: Vec<f32> = (0..dim).map(|i| ((i as f32) * 0.1).sin()).collect();
    MemoryNode {
        id: id.to_string(),
        label: format!("label_{id}"),
        node_type: NodeType::Symbol,
        embedding,
        metadata: serde_json::json!({}),
        last_accessed: 0.0,
        access_count: 0,
    }
}

fn bench_add_node(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_graph_add_node");

    for &count in &[100usize, 500] {
        group.bench_with_input(BenchmarkId::new("nodes", count), &count, |b, &n| {
            b.iter_batched(
                || SemanticGraph::new(make_persistence()),
                |graph| {
                    for i in 0..n {
                        graph.add_node(make_node(&format!("n_{i}"), 64)).unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_add_edge(c: &mut Criterion) {
    // Pre-populate graph with nodes
    let graph = SemanticGraph::new(make_persistence());
    for i in 0..200 {
        graph.add_node(make_node(&format!("n_{i}"), 32)).unwrap();
    }

    c.bench_function("semantic_graph_add_edge_200_nodes", |b| {
        b.iter_batched(
            || {
                let g = SemanticGraph::new(make_persistence());
                for i in 0..200 {
                    g.add_node(make_node(&format!("n_{i}"), 32)).unwrap();
                }
                g
            },
            |g| {
                for i in 0..199 {
                    g.add_edge(
                        black_box(&format!("n_{i}")),
                        black_box(&format!("n_{}", i + 1)),
                        black_box(1.0),
                    )
                    .unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_add_many_nodes_with_edges(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_graph_build");

    for &node_count in &[50usize, 200] {
        group.bench_with_input(
            BenchmarkId::new("nodes_with_chain", node_count),
            &node_count,
            |b, &n| {
                b.iter_batched(
                    make_persistence,
                    |persistence| {
                        let graph = SemanticGraph::new(persistence);
                        for i in 0..n {
                            graph.add_node(make_node(&format!("n_{i}"), 64)).unwrap();
                        }
                        // Add chain edges
                        for i in 0..n - 1 {
                            graph
                                .add_edge(&format!("n_{i}"), &format!("n_{}", i + 1), 1.0)
                                .unwrap();
                        }
                        graph.node_count()
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_edges_from(c: &mut Criterion) {
    let graph = SemanticGraph::new(make_persistence());
    for i in 0..100 {
        graph.add_node(make_node(&format!("n_{i}"), 16)).unwrap();
    }
    // Create a star topology: n_0 -> n_1..n_99
    for i in 1..100 {
        graph.add_edge("n_0", &format!("n_{i}"), 0.5).unwrap();
    }

    c.bench_function("semantic_graph_edges_from_99_edges", |b| {
        b.iter(|| graph.edges_from(black_box("n_0")));
    });
}

fn bench_node_count(c: &mut Criterion) {
    let graph = SemanticGraph::new(make_persistence());
    for i in 0..500 {
        graph.add_node(make_node(&format!("n_{i}"), 16)).unwrap();
    }

    c.bench_function("semantic_graph_node_count_500", |b| {
        b.iter(|| graph.node_count());
    });
}

criterion_group!(
    benches,
    bench_add_node,
    bench_add_edge,
    bench_add_many_nodes_with_edges,
    bench_edges_from,
    bench_node_count,
);
criterion_main!(benches);
