//! Cycle detection in dependency graphs — DFS 3-color algorithm.

use std::collections::HashMap;

use super::SymbolIndex;

impl SymbolIndex {
    /// Detect dependency cycles in the project.
    ///
    /// Returns a list of cycles, where each cycle is represented as a vector
    /// of file paths forming the cycle (e.g., `["A", "B", "C", "A"]`).
    ///
    /// Uses DFS with 3-color marking: White (unvisited) -> Gray (in-stack) -> Black (done).
    /// When a Gray node is revisited, a cycle is extracted from the current DFS stack.
    #[must_use]
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        // DFS 3-color constants
        const WHITE: u8 = 0;

        let mut color: HashMap<&str, u8> = HashMap::new();
        let mut path: Vec<&str> = Vec::new();
        let mut cycles: Vec<Vec<String>> = Vec::new();

        for file in self.dependencies.keys() {
            if color.get(file.as_str()).copied().unwrap_or(WHITE) == WHITE {
                self.dfs_detect_cycles(file.as_str(), &mut color, &mut path, &mut cycles);
            }
        }
        cycles
    }

    /// DFS helper for cycle detection. Tracks 3-color state per node.
    ///
    /// Color encoding: 0 = White (unvisited), 1 = Gray (in-stack), 2 = Black (done).
    fn dfs_detect_cycles<'a>(
        &'a self,
        node: &'a str,
        color: &mut HashMap<&'a str, u8>,
        path: &mut Vec<&'a str>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        const GRAY: u8 = 1;
        const BLACK: u8 = 2;

        color.insert(node, GRAY);
        path.push(node);

        if let Some(edges) = self.dependencies.get(node) {
            for edge in edges {
                let neighbor = edge.to.as_str();
                match color.get(neighbor).copied().unwrap_or(0) {
                    0 => {
                        // White: unvisited — recurse
                        self.dfs_detect_cycles(neighbor, color, path, cycles);
                    }
                    GRAY => {
                        // Back-edge: cycle found — extract from stack
                        if let Some(start) = path.iter().position(|&n| n == neighbor) {
                            let mut cycle: Vec<String> = path
                                .get(start..)
                                .unwrap_or_default()
                                .iter()
                                .map(|s| s.to_string())
                                .collect();
                            cycle.push(neighbor.to_string()); // close the cycle
                            cycles.push(cycle);
                        }
                    }
                    _ => {} // BLACK: already fully processed, skip
                }
            }
        }

        path.pop();
        color.insert(node, BLACK);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::super::{DependencyEdge, SymbolIndex};

    #[test]
    fn test_detect_cycles_no_cycles() {
        let mut idx = SymbolIndex::new();
        // A -> B -> C (linear, no cycle)
        idx.dependencies.insert(
            "a.py".to_string(),
            vec![DependencyEdge {
                from: "a.py".to_string(),
                to: "b.py".to_string(),
                symbols: vec![],
                co_edit_weight: 0.0,
            }],
        );
        idx.dependencies.insert(
            "b.py".to_string(),
            vec![DependencyEdge {
                from: "b.py".to_string(),
                to: "c.py".to_string(),
                symbols: vec![],
                co_edit_weight: 0.0,
            }],
        );

        let cycles = idx.detect_cycles();
        assert!(cycles.is_empty(), "Expected no cycles, got: {cycles:?}");
    }

    #[test]
    fn test_detect_cycles_simple_cycle() {
        let mut idx = SymbolIndex::new();
        // A -> B -> A (cycle)
        idx.dependencies.insert(
            "a.py".to_string(),
            vec![DependencyEdge {
                from: "a.py".to_string(),
                to: "b.py".to_string(),
                symbols: vec![],
                co_edit_weight: 0.0,
            }],
        );
        idx.dependencies.insert(
            "b.py".to_string(),
            vec![DependencyEdge {
                from: "b.py".to_string(),
                to: "a.py".to_string(),
                symbols: vec![],
                co_edit_weight: 0.0,
            }],
        );

        let cycles = idx.detect_cycles();
        assert_eq!(cycles.len(), 1, "Expected exactly 1 cycle, got: {cycles:?}");
        let cycle = &cycles[0];
        // Cycle should be closed: last element == first element
        assert_eq!(
            cycle.first(),
            cycle.last(),
            "Cycle must be closed: {cycle:?}"
        );
        // Cycle should contain both a.py and b.py
        assert!(cycle.contains(&"a.py".to_string()));
        assert!(cycle.contains(&"b.py".to_string()));
    }

    #[test]
    fn test_detect_cycles_self_loop() {
        let mut idx = SymbolIndex::new();
        // A -> A (self-loop)
        idx.dependencies.insert(
            "a.py".to_string(),
            vec![DependencyEdge {
                from: "a.py".to_string(),
                to: "a.py".to_string(),
                symbols: vec![],
                co_edit_weight: 0.0,
            }],
        );

        let cycles = idx.detect_cycles();
        assert_eq!(
            cycles.len(),
            1,
            "Expected exactly 1 self-loop cycle, got: {cycles:?}"
        );
        assert_eq!(cycles[0], vec!["a.py", "a.py"]);
    }

    #[test]
    fn test_detect_cycles_triangle() {
        let mut idx = SymbolIndex::new();
        // A -> B -> C -> A (triangle cycle)
        idx.dependencies.insert(
            "a.py".to_string(),
            vec![DependencyEdge {
                from: "a.py".to_string(),
                to: "b.py".to_string(),
                symbols: vec![],
                co_edit_weight: 0.0,
            }],
        );
        idx.dependencies.insert(
            "b.py".to_string(),
            vec![DependencyEdge {
                from: "b.py".to_string(),
                to: "c.py".to_string(),
                symbols: vec![],
                co_edit_weight: 0.0,
            }],
        );
        idx.dependencies.insert(
            "c.py".to_string(),
            vec![DependencyEdge {
                from: "c.py".to_string(),
                to: "a.py".to_string(),
                symbols: vec![],
                co_edit_weight: 0.0,
            }], // triangle
        );

        let cycles = idx.detect_cycles();
        assert_eq!(
            cycles.len(),
            1,
            "Expected 1 triangle cycle, got: {cycles:?}"
        );
        let cycle = &cycles[0];
        assert_eq!(cycle.len(), 4); // [A, B, C, A]
        assert_eq!(cycle.first(), cycle.last());
    }

    #[test]
    fn test_detect_cycles_empty_graph() {
        let idx = SymbolIndex::new();
        assert!(idx.detect_cycles().is_empty());
    }
}
