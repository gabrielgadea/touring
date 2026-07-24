//! Snapshot tests for `compute_complexity_for_source` output stability.
//!
//! Cyclomatic complexity feeds the cognitive_score enrichment pipeline
//! (file_knowledge.db) and the Tantivy `cognitive_score_x1000` field.
//! Drift in branch counting would ripple into wiring scoring and LinUCB
//! reward shaping. Snapshots pin the contract against grammar bumps.
//!
//! Review: `cargo insta review -p touring-ast`.

use touring_code::ast::complexity::compute_complexity_for_source;
use touring_code::ast::languages::Lang;

#[test]
fn snapshot_rust_complexity_branches() {
    let source = r#"
fn simple() -> i32 { 42 }

fn branching(x: i32) -> i32 {
    if x > 0 {
        if x > 10 { 1 } else { 2 }
    } else {
        match x {
            -1 => 3,
            -2 => 4,
            _ => 5,
        }
    }
}

fn loops(n: usize) -> usize {
    let mut sum = 0;
    for i in 0..n {
        if i % 2 == 0 {
            sum += i;
        }
    }
    while sum > 100 {
        sum -= 1;
    }
    sum
}
"#;
    let mut result =
        compute_complexity_for_source(source, Lang::Rust).expect("complexity compute must succeed");
    result.sort();
    insta::assert_yaml_snapshot!("rust_complexity_branches", result);
}

#[test]
fn snapshot_python_complexity_branches() {
    let source = r#"
def simple():
    return 42

def branching(x):
    if x > 0:
        if x > 10:
            return 1
        return 2
    elif x == 0:
        return 0
    else:
        return -1

def loops(n):
    total = 0
    for i in range(n):
        if i % 2 == 0:
            total += i
    while total > 100:
        total -= 1
    return total
"#;
    let mut result = compute_complexity_for_source(source, Lang::Python)
        .expect("complexity compute must succeed");
    result.sort();
    insta::assert_yaml_snapshot!("python_complexity_branches", result);
}
