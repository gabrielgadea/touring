//! Call Graph Aware Generation — Function call extraction.
//!
//! Builds a call graph from source code by extracting all function call
//! expressions. Enables querying callers of a given function to verify
//! that signature changes don't break existing call sites.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

use crate::ast::languages::Lang;
use crate::ast::node_text;
use crate::ast::parser::parse_thread_local;

// ─── Types ──────────────────────────────────────────────────────────────

/// A single function call site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSite {
    /// Name of the function containing the call
    pub caller: String,
    /// Name of the function being called
    pub callee: String,
    /// Line number of the call (1-indexed)
    pub line: usize,
    /// Number of arguments passed
    pub args_count: usize,
}

/// A call graph built from source code.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CallGraph {
    /// All call sites found in the source
    pub sites: Vec<CallSite>,
}

// Compile-time invariants: CallGraph must be safe to move across threads
// (rayon par_iter in consumers) and cloneable (CallGraph is returned by
// build_call_graph and often duplicated for snapshot comparisons).
static_assertions::assert_impl_all!(CallGraph: Send, Sync, Clone);
static_assertions::assert_impl_all!(CallSite: Send, Sync, Clone);

// ─── Tree-sitter queries ────────────────────────────────────────────────

const RUST_CALL_QUERY: &str = r#"
;; Direct function calls: foo(...)
(call_expression
  function: (identifier) @callee
  arguments: (arguments) @args)

;; Scoped calls: module::foo(...)
(call_expression
  function: (scoped_identifier
    name: (identifier) @callee)
  arguments: (arguments) @args)

;; Method calls: obj.method(...)
(call_expression
  function: (field_expression
    field: (field_identifier) @callee)
  arguments: (arguments) @args)
"#;

const PYTHON_CALL_QUERY: &str = r#"
;; Direct calls: foo(...)
(call
  function: (identifier) @callee
  arguments: (argument_list) @args)

;; Attribute calls: obj.method(...)
(call
  function: (attribute
    attribute: (identifier) @callee)
  arguments: (argument_list) @args)
"#;

const TS_CALL_QUERY: &str = r#"
;; Direct calls: foo(...)
(call_expression
  function: (identifier) @callee
  arguments: (arguments) @args)

;; Method calls: obj.method(...)
(call_expression
  function: (member_expression
    property: (property_identifier) @callee)
  arguments: (arguments) @args)
"#;

// ─── Implementation ─────────────────────────────────────────────────────

/// Build a call graph from source code.
///
/// Extracts all function/method call expressions and identifies the
/// caller context (enclosing function).
///
/// # Arguments
/// * `source` - Source code to analyze
/// * `lang` - Language to parse
///
/// # Returns
/// A [`CallGraph`] containing all found call sites.
pub fn build_call_graph(source: &str, lang: Lang) -> CallGraph {
    match lang {
        Lang::Rust => build_rust_call_graph(source),
        Lang::Python => build_python_call_graph(source),
        Lang::TypeScript | Lang::JavaScript => build_ts_call_graph(source, lang),
        _ => CallGraph::default(),
    }
}

impl CallGraph {
    /// Find all call sites that invoke a given function.
    ///
    /// # Arguments
    /// * `function_name` - Name of the function to search for
    ///
    /// # Returns
    /// References to all [`CallSite`]s where `function_name` is the callee.
    pub fn callers_of(&self, function_name: &str) -> Vec<&CallSite> {
        self.sites
            .iter()
            .filter(|s| s.callee == function_name)
            .collect()
    }

    /// Find all functions called by a given caller.
    pub fn callees_of(&self, caller_name: &str) -> Vec<&CallSite> {
        self.sites
            .iter()
            .filter(|s| s.caller == caller_name)
            .collect()
    }

    /// Get unique callee names.
    pub fn unique_callees(&self) -> Vec<String> {
        let mut names: Vec<String> = self.sites.iter().map(|s| s.callee.clone()).collect();
        names.sort();
        names.dedup();
        names
    }

    /// Detect cycles using Tarjan's SCC algorithm — O(|V| + |E|).
    ///
    /// Returns a list of strongly connected components with >1 node, plus
    /// any single-node SCCs that have an explicit self-loop (direct recursion).
    /// Each component is a `Vec<String>` of function names forming the cycle.
    ///
    /// An empty result means no cycles were found.
    ///
    /// # Algorithm
    ///
    /// Builds a directed graph from call sites, then runs Tarjan's SCC
    /// via `petgraph::algo::tarjan_scc`. Components of size 1 are only
    /// reported if the function calls itself (self-loop).
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        use petgraph::algo::tarjan_scc;
        use petgraph::graph::DiGraph;

        // Build name→index mapping and directed graph
        let mut name_to_idx: HashMap<&str, petgraph::graph::NodeIndex> = HashMap::new();
        let mut graph = DiGraph::<&str, ()>::new();

        // Collect all unique function names (callers + callees)
        let mut all_names: HashSet<&str> = HashSet::new();
        for site in &self.sites {
            all_names.insert(&site.caller);
            all_names.insert(&site.callee);
        }

        // Create nodes
        for name in &all_names {
            let idx = graph.add_node(*name);
            name_to_idx.insert(*name, idx);
        }

        // Create edges (caller → callee)
        let mut edge_set: HashSet<(petgraph::graph::NodeIndex, petgraph::graph::NodeIndex)> =
            HashSet::new();
        for site in &self.sites {
            if let (Some(&from), Some(&to)) = (
                name_to_idx.get(site.caller.as_str()),
                name_to_idx.get(site.callee.as_str()),
            ) && edge_set.insert((from, to))
            {
                graph.add_edge(from, to, ());
            }
        }

        // Run Tarjan SCC
        let sccs: Vec<Vec<petgraph::graph::NodeIndex>> = tarjan_scc(&graph);

        let mut cycles: Vec<Vec<String>> = Vec::new();
        for component in sccs {
            let is_cycle = if component.len() > 1 {
                true // mutual recursion cycle
            } else {
                // Single node — only report if self-loop exists
                let idx = component.first().copied().expect("component is non-empty");
                graph.contains_edge(idx, idx)
            };

            if is_cycle {
                let names: Vec<String> = component
                    .into_iter()
                    .map(|idx| graph[idx].to_string())
                    .collect();
                cycles.push(names);
            }
        }
        cycles
    }
}

fn build_rust_call_graph(source: &str) -> CallGraph {
    build_call_graph_impl(source, Lang::Rust, RUST_CALL_QUERY, "function_item")
}

fn build_python_call_graph(source: &str) -> CallGraph {
    build_call_graph_impl(
        source,
        Lang::Python,
        PYTHON_CALL_QUERY,
        "function_definition",
    )
}

fn build_ts_call_graph(source: &str, lang: Lang) -> CallGraph {
    // TS/JS use "function_declaration" for named functions
    build_call_graph_impl(source, lang, TS_CALL_QUERY, "function_declaration")
}

fn build_call_graph_impl(
    source: &str,
    lang: Lang,
    query_src: &str,
    fn_node_kind: &str,
) -> CallGraph {
    let tree = match parse_thread_local(source, lang) {
        Ok(t) => t,
        Err(_) => return CallGraph::default(),
    };

    let ts_lang = lang.tree_sitter_language();
    let query = match Query::new(&ts_lang, query_src) {
        Ok(q) => q,
        Err(_) => return CallGraph::default(),
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut sites = Vec::new();

    let callee_idx = query.capture_index_for_name("callee");
    let args_idx = query.capture_index_for_name("args");

    while let Some(m) = matches.next() {
        let callee_cap = callee_idx.and_then(|idx| m.captures.iter().find(|c| c.index == idx));
        let args_cap = args_idx.and_then(|idx| m.captures.iter().find(|c| c.index == idx));

        if let Some(callee_c) = callee_cap {
            let callee_name = node_text(source, callee_c.node).to_string();
            let line = callee_c.node.start_position().row + 1;

            // Count arguments
            let args_count = args_cap.map_or(0, |a| count_args(a.node));

            // Find enclosing function
            let caller = find_enclosing_function(callee_c.node, source, fn_node_kind);

            sites.push(CallSite {
                caller,
                callee: callee_name,
                line,
                args_count,
            });
        }
    }

    CallGraph { sites }
}

/// Find the enclosing function name for a given node.
fn find_enclosing_function(node: tree_sitter::Node, source: &str, fn_node_kind: &str) -> String {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == fn_node_kind {
            // Find the name child
            if let Some(name_node) = parent.child_by_field_name("name") {
                return node_text(source, name_node).to_string();
            }
        }
        current = parent.parent();
    }
    "<module>".to_string()
}

/// Count the number of direct child arguments in an arguments node.
fn count_args(args_node: tree_sitter::Node) -> usize {
    let mut count = 0;
    for i in 0..args_node.named_child_count() {
        if args_node.named_child(i as u32).is_some() {
            count += 1;
        }
    }
    count
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_graph_basic() {
        let src = r#"
fn main() {
    let result = compute(1, 2);
    println!("{}", format_result(result));
}

fn compute(a: u32, b: u32) -> u32 { a + b }
fn format_result(n: u32) -> String { n.to_string() }
"#;
        let graph = build_call_graph(src, Lang::Rust);
        let callers = graph.callers_of("compute");
        assert!(
            !callers.is_empty(),
            "compute should have callers: {:?}",
            graph.sites
        );
        assert!(
            callers.iter().any(|c| c.caller == "main"),
            "main should call compute: {:?}",
            callers
        );
    }

    #[test]
    fn test_call_graph_method_calls() {
        let src = r#"
fn process() {
    let s = String::new();
    let len = s.len();
}
"#;
        let graph = build_call_graph(src, Lang::Rust);
        assert!(
            graph.sites.iter().any(|s| s.callee == "len"),
            "len method call not found: {:?}",
            graph.sites
        );
    }

    #[test]
    fn test_callers_of_nonexistent() {
        let src = "fn main() {}";
        let graph = build_call_graph(src, Lang::Rust);
        let callers = graph.callers_of("nonexistent");
        assert!(callers.is_empty());
    }

    #[test]
    fn test_call_graph_empty() {
        let graph = build_call_graph("", Lang::Rust);
        assert!(graph.sites.is_empty());
    }

    #[test]
    fn test_call_graph_unsupported_lang() {
        let graph = build_call_graph("code", Lang::Bash);
        assert!(graph.sites.is_empty());
    }

    #[test]
    fn test_call_graph_python() {
        let src = r#"
def main():
    result = compute(1, 2)
    print(result)

def compute(a, b):
    return a + b
"#;
        let graph = build_call_graph(src, Lang::Python);
        let callers = graph.callers_of("compute");
        assert!(
            !callers.is_empty(),
            "compute should have callers in python: {:?}",
            graph.sites
        );
    }

    #[test]
    fn test_callees_of() {
        let src = r#"
fn main() {
    foo();
    bar();
}
fn foo() {}
fn bar() {}
"#;
        let graph = build_call_graph(src, Lang::Rust);
        let callees = graph.callees_of("main");
        assert!(
            callees.len() >= 2,
            "main should call at least foo and bar: {:?}",
            callees
        );
    }

    #[test]
    fn test_unique_callees() {
        let src = r#"
fn main() {
    foo();
    foo();
    bar();
}
fn foo() {}
fn bar() {}
"#;
        let graph = build_call_graph(src, Lang::Rust);
        let unique = graph.unique_callees();
        assert!(
            unique.contains(&"foo".to_string()),
            "unique should contain foo: {:?}",
            unique
        );
        assert!(
            unique.contains(&"bar".to_string()),
            "unique should contain bar: {:?}",
            unique
        );
    }

    // ─── TypeScript/JavaScript tests ────────────────────────────────

    #[test]
    fn test_call_graph_typescript() {
        let src = r#"
function main() {
    const result = compute(1, 2);
    console.log(result);
}

function compute(a: number, b: number): number {
    return a + b;
}
"#;
        let graph = build_call_graph(src, Lang::TypeScript);
        let callers = graph.callers_of("compute");
        assert!(
            !callers.is_empty(),
            "compute should have callers in TS: {:?}",
            graph.sites
        );
        assert!(
            callers.iter().any(|c| c.caller == "main"),
            "main should call compute in TS: {:?}",
            callers
        );
    }

    #[test]
    fn test_call_graph_ts_method_calls() {
        let src = r#"
function process() {
    const arr = [1, 2, 3];
    arr.push(4);
    console.log(arr);
}
"#;
        let graph = build_call_graph(src, Lang::TypeScript);
        assert!(
            graph.sites.iter().any(|s| s.callee == "push"),
            "push method call not found: {:?}",
            graph.sites
        );
        assert!(
            graph.sites.iter().any(|s| s.callee == "log"),
            "log method call not found: {:?}",
            graph.sites
        );
    }

    #[test]
    fn test_call_graph_javascript() {
        let src = r#"
function greet(name) {
    return formatName(name);
}

function formatName(n) {
    return n.toUpperCase();
}
"#;
        let graph = build_call_graph(src, Lang::JavaScript);
        let callers = graph.callers_of("formatName");
        assert!(
            !callers.is_empty(),
            "formatName should have callers in JS: {:?}",
            graph.sites
        );
    }

    #[test]
    fn test_call_graph_ts_empty() {
        let graph = build_call_graph("", Lang::TypeScript);
        assert!(graph.sites.is_empty());
    }

    // ─── Tarjan SCC cycle detection tests ──────────────────────────────

    #[test]
    fn test_detect_cycles_no_cycles() {
        let src = r#"
fn main() {
    foo();
}
fn foo() {
    bar();
}
fn bar() {}
"#;
        let graph = build_call_graph(src, Lang::Rust);
        let cycles = graph.detect_cycles();
        assert!(
            cycles.is_empty(),
            "linear call chain has no cycles: {:?}",
            cycles
        );
    }

    #[test]
    fn test_detect_cycles_mutual_recursion() {
        let src = r#"
fn ping() {
    pong();
}
fn pong() {
    ping();
}
"#;
        let graph = build_call_graph(src, Lang::Rust);
        let cycles = graph.detect_cycles();
        assert_eq!(cycles.len(), 1, "should detect one cycle: {:?}", cycles);
        let cycle = &cycles[0];
        assert!(
            cycle.contains(&"ping".to_string()),
            "cycle should contain ping"
        );
        assert!(
            cycle.contains(&"pong".to_string()),
            "cycle should contain pong"
        );
    }

    #[test]
    fn test_detect_cycles_self_recursion() {
        let src = r#"
fn factorial(n: u32) -> u32 {
    factorial(n - 1)
}
"#;
        let graph = build_call_graph(src, Lang::Rust);
        let cycles = graph.detect_cycles();
        assert_eq!(
            cycles.len(),
            1,
            "self-recursion should be detected: {:?}",
            cycles
        );
        assert_eq!(cycles[0], vec!["factorial"]);
    }

    #[test]
    fn test_detect_cycles_three_way() {
        let src = r#"
fn a() { b(); }
fn b() { c(); }
fn c() { a(); }
"#;
        let graph = build_call_graph(src, Lang::Rust);
        let cycles = graph.detect_cycles();
        assert_eq!(cycles.len(), 1, "three-way cycle: {:?}", cycles);
        let cycle = &cycles[0];
        assert_eq!(cycle.len(), 3);
        assert!(cycle.contains(&"a".to_string()));
        assert!(cycle.contains(&"b".to_string()));
        assert!(cycle.contains(&"c".to_string()));
    }

    #[test]
    fn test_detect_cycles_empty_graph() {
        let graph = CallGraph::default();
        assert!(graph.detect_cycles().is_empty());
    }

    #[test]
    fn test_detect_cycles_python_mutual() {
        let src = r#"
def even(n):
    return odd(n - 1)

def odd(n):
    return even(n - 1)
"#;
        let graph = build_call_graph(src, Lang::Python);
        let cycles = graph.detect_cycles();
        assert_eq!(cycles.len(), 1, "Python mutual recursion: {:?}", cycles);
    }
}
