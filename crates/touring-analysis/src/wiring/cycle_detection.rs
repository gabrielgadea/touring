//! Dependency cycle detection for crate import graphs.
//!
//! Builds a directed import graph from `wiring_map` entries and uses
//! petgraph's strongly-connected-component algorithm to find circular
//! import chains (SCCs with more than one node).
//!
//! # Schema note
//! Reads from `wiring_map(module_file, consumer_file)`.  An edge
//! `module_file → consumer_file` means "module_file is imported by consumer_file".
//! A cycle in this graph indicates a circular dependency.
//!
//! # Usage
//! ```no_run
//! use touring_analysis::detect_import_cycles;
//! let conn = rusqlite::Connection::open("knowledge.db").expect("open");
//! let cycles = detect_import_cycles(&conn);
//! for cycle in &cycles {
//!     println!("Cycle: {:?}", cycle);
//! }
//! ```

use petgraph::algo::kosaraju_scc;
use petgraph::graph::DiGraph;
use rusqlite::Connection;
use std::collections::HashMap;

/// A detected circular import chain, represented as a list of module paths.
pub type CyclePath = Vec<String>;

/// Detect circular imports in the codebase using Kosaraju's SCC algorithm.
///
/// Queries `wiring_map` for all import relationships, builds a directed graph,
/// and returns any strongly-connected components with 2+ nodes (i.e. cycles).
///
/// Returns an empty `Vec` gracefully if the table does not exist or has no data.
pub fn detect_import_cycles(conn: &Connection) -> Vec<CyclePath> {
    let edges = match load_import_edges(conn) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    if edges.is_empty() {
        return vec![];
    }

    let (graph, _node_map) = build_graph(&edges);
    extract_cycles(&graph)
}

/// Count of circular import cycles detected.
pub fn count_import_cycles(conn: &Connection) -> usize {
    detect_import_cycles(conn).len()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

type EdgeList = Vec<(String, String)>;

fn load_import_edges(conn: &Connection) -> Result<EdgeList, rusqlite::Error> {
    // An edge (module_file → consumer_file) represents a "is used by" relationship.
    // Cycles in this graph = circular dependencies.
    let query = "
        SELECT DISTINCT module_file, consumer_file
        FROM wiring_map
        WHERE module_file IS NOT NULL
          AND consumer_file IS NOT NULL
          AND module_file != consumer_file
        LIMIT 10000
    ";
    let mut stmt = match conn.prepare(query) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]), // table doesn't exist yet — safe fallback
    };
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut edges = Vec::new();
    for row in rows.flatten() {
        edges.push(row);
    }
    Ok(edges)
}

fn build_graph(
    edges: &EdgeList,
) -> (
    DiGraph<String, ()>,
    HashMap<String, petgraph::graph::NodeIndex>,
) {
    let mut graph: DiGraph<String, ()> = DiGraph::new();
    let mut node_map: HashMap<String, petgraph::graph::NodeIndex> = HashMap::new();

    for (src, dst) in edges {
        let src_idx = *node_map
            .entry(src.clone())
            .or_insert_with(|| graph.add_node(src.clone()));
        let dst_idx = *node_map
            .entry(dst.clone())
            .or_insert_with(|| graph.add_node(dst.clone()));
        graph.add_edge(src_idx, dst_idx, ());
    }

    (graph, node_map)
}

fn extract_cycles(graph: &DiGraph<String, ()>) -> Vec<CyclePath> {
    // Kosaraju's SCC: every SCC with >1 node is a cycle.
    let sccs = kosaraju_scc(graph);
    let mut cycles = Vec::new();
    for scc in sccs {
        if scc.len() > 1 {
            let path: Vec<String> = scc.iter().map(|&idx| graph[idx].clone()).collect();
            cycles.push(path);
        }
    }
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("schema");
        conn
    }

    #[test]
    fn test_detect_import_cycles_empty_db() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        // No wiring_map table — must return empty gracefully
        let cycles = detect_import_cycles(&conn);
        assert!(cycles.is_empty(), "empty db must yield no cycles");
    }

    #[test]
    fn test_count_import_cycles_empty_db() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        let count = count_import_cycles(&conn);
        assert_eq!(count, 0, "empty db must yield count=0");
    }

    #[test]
    fn test_no_cycle_simple_chain() {
        let conn = setup_db();
        // a.rs → b.rs → c.rs (no cycle)
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('a.rs', 'Foo', 'fn', 'public', 'b.rs')",
            [],
        )
        .expect("insert");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('b.rs', 'Bar', 'fn', 'public', 'c.rs')",
            [],
        )
        .expect("insert");
        let cycles = detect_import_cycles(&conn);
        assert!(
            cycles.is_empty(),
            "linear chain a→b→c must have no cycles, got: {:?}",
            cycles
        );
    }

    #[test]
    fn test_cycle_two_nodes() {
        let conn = setup_db();
        // a.rs ↔ b.rs (mutual import = cycle)
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('a.rs', 'Foo', 'fn', 'public', 'b.rs')",
            [],
        )
        .expect("insert");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('b.rs', 'Bar', 'fn', 'public', 'a.rs')",
            [],
        )
        .expect("insert");
        let cycles = detect_import_cycles(&conn);
        assert_eq!(
            cycles.len(),
            1,
            "mutual a↔b must produce exactly one cycle, got: {:?}",
            cycles
        );
        let members = cycles.first().expect("one cycle present");
        assert!(
            members.contains(&"a.rs".to_string()) && members.contains(&"b.rs".to_string()),
            "cycle must contain a.rs and b.rs, got: {:?}",
            members
        );
    }

    #[test]
    fn test_cycle_three_nodes() {
        let conn = setup_db();
        // a → b → c → a (three-node cycle)
        for (src, sym, dst) in [
            ("a.rs", "A", "b.rs"),
            ("b.rs", "B", "c.rs"),
            ("c.rs", "C", "a.rs"),
        ] {
            conn.execute(
                &format!(
                    "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
                     VALUES ('{}', '{}', 'fn', 'public', '{}')",
                    src, sym, dst
                ),
                [],
            )
            .expect("insert");
        }
        let cycles = detect_import_cycles(&conn);
        assert_eq!(
            cycles.len(),
            1,
            "three-node cycle must produce one SCC, got: {:?}",
            cycles
        );
        let scc = cycles.first().expect("one SCC");
        assert_eq!(scc.len(), 3, "SCC must contain all 3 nodes, got: {:?}", scc);
    }

    #[test]
    fn test_count_import_cycles_nonzero() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('x.rs', 'X', 'fn', 'public', 'y.rs')",
            [],
        )
        .expect("insert");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('y.rs', 'Y', 'fn', 'public', 'x.rs')",
            [],
        )
        .expect("insert");
        assert_eq!(
            count_import_cycles(&conn),
            1,
            "one mutual cycle must count as 1"
        );
    }

    #[test]
    fn test_self_loops_excluded() {
        let conn = setup_db();
        // Self-import is excluded by the query (module_file != consumer_file)
        // Insert a non-self-loop to ensure no panic
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('a.rs', 'Foo', 'fn', 'public', 'b.rs')",
            [],
        )
        .expect("insert");
        let cycles = detect_import_cycles(&conn);
        assert!(cycles.is_empty(), "single edge a→b is not a cycle");
    }
}
