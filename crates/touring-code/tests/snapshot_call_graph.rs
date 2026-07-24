//! Pilot snapshot tests for `CallGraph` output stability.
//!
//! Rationale: `CallGraph` is consumed by touring-analysis (Tarjan SCC cycle
//! detection) and file_digest_signal. A silent shape change in CallSite
//! would silently degrade wiring analysis. Snapshots lock the output format
//! across tree-sitter version bumps and grammar updates.
//!
//! Review new snapshots with: `cargo insta review -p touring-ast`.

use touring_code::ast::call_graph::{CallGraph, build_call_graph};
use touring_code::ast::languages::Lang;

/// Normalize a CallGraph for snapshot comparison:
/// - Sort sites by (caller, callee, line) for deterministic ordering.
/// - Return only the stable triple — line numbers shift across unrelated
///   tests but the (caller, callee) relation is the semantic invariant.
fn normalize(graph: &CallGraph) -> Vec<(String, String, usize)> {
    let mut sites: Vec<(String, String, usize)> = graph
        .sites
        .iter()
        .map(|s| (s.caller.clone(), s.callee.clone(), s.line))
        .collect();
    sites.sort();
    sites
}

#[test]
fn snapshot_rust_simple_call_graph() {
    let source = r#"
fn main() {
    helper();
    other_fn(42);
}

fn helper() {
    inner();
}

fn inner() {}

fn other_fn(_x: i32) {
    helper();
}
"#;
    let graph = build_call_graph(source, Lang::Rust);
    insta::assert_yaml_snapshot!("rust_simple", normalize(&graph));
}

#[test]
fn snapshot_python_nested_calls() {
    let source = r#"
def main():
    helper()
    other_fn(42)

def helper():
    inner()

def inner():
    pass

def other_fn(x):
    helper()
"#;
    let graph = build_call_graph(source, Lang::Python);
    insta::assert_yaml_snapshot!("python_nested", normalize(&graph));
}
