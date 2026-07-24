//! Snapshot tests for `CallGraph` mutual-recursion / SCC shape.
//!
//! Rationale: touring-analysis runs Tarjan SCC on the CallGraph output to
//! detect cycles (recursion, mutual recursion, layered feedback). If the
//! (caller, callee) tuple shape drifts across tree-sitter grammar bumps,
//! the SCC detector silently degrades — missed cycles become missed
//! wiring orphans. These snapshots lock the cycle-participating edges
//! so any regression shows up as a diff on the YAML.
//!
//! Review new/changed snapshots with: `cargo insta review -p touring-ast`.

use touring_code::ast::call_graph::{CallGraph, build_call_graph};
use touring_code::ast::languages::Lang;

/// Extract only the `(caller, callee)` pairs — line numbers shift on every
/// whitespace change but the semantic relation is what SCC operates on.
fn edges(graph: &CallGraph) -> Vec<(String, String)> {
    let mut e: Vec<(String, String)> = graph
        .sites
        .iter()
        .map(|s| (s.caller.clone(), s.callee.clone()))
        .collect();
    e.sort();
    e.dedup();
    e
}

#[test]
fn snapshot_rust_mutual_recursion_two_node_scc() {
    // Two-node SCC: ping <-> pong. Minimum cycle that exercises Tarjan's
    // back-edge detection on non-trivial input.
    let source = r#"
fn ping(n: u32) {
    if n > 0 {
        pong(n - 1);
    }
}

fn pong(n: u32) {
    if n > 0 {
        ping(n - 1);
    }
}
"#;
    let graph = build_call_graph(source, Lang::Rust);
    insta::assert_yaml_snapshot!("rust_mutual_recursion_scc", edges(&graph));
}

#[test]
fn snapshot_rust_three_node_cycle_plus_tail() {
    // Three-node SCC (a -> b -> c -> a) plus a `tail` callee that is NOT
    // in the SCC. Ensures edge emission is complete and Tarjan can still
    // differentiate cycle members from sinks.
    let source = r#"
fn a() { b(); }
fn b() { c(); }
fn c() { a(); tail(); }
fn tail() {}
"#;
    let graph = build_call_graph(source, Lang::Rust);
    insta::assert_yaml_snapshot!("rust_three_node_cycle_plus_tail", edges(&graph));
}

#[test]
fn snapshot_python_self_recursion() {
    // Single-node SCC — self-loop. The simplest cycle shape. If this edge
    // disappears, the cycle detector can no longer flag direct recursion.
    let source = r#"
def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)
"#;
    let graph = build_call_graph(source, Lang::Python);
    insta::assert_yaml_snapshot!("python_self_recursion", edges(&graph));
}
