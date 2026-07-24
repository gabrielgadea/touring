//! BFS exact blast radius strategy.
//!
//! Performs BFS directly over `SymbolIndex::reverse_deps` to compute real
//! hop distances per affected file — rather than delegating to
//! `blast_radius_with_depth()` which only exposes the maximum distance.

use super::{AffectedFile, BlastRadiusResult, BlastRadiusStrategy, LatencyTier};
use crate::engine::AnalysisConfig;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use touring_code::ast::SymbolIndex;

/// BFS exact blast radius strategy.
///
/// Algorithm: breadth-first search over `SymbolIndex::reverse_deps` starting
/// from `start_file`. Each hop increments `distance` by 1. BFS guarantees
/// shortest-path distances (unlike DFS which may report inflated distances).
///
/// Complexity: O(V + E) where V = files in reverse dependency graph, E = edges.
/// Depth is bounded by `AnalysisConfig::blast_depth` (0 = unbounded → `usize::MAX`).
///
/// Publicly constructable — use `BlastRadiusEngine::bfs_only` as a convenience
/// factory, or wrap in `Box<dyn BlastRadiusStrategy>` for custom engine composition.
pub struct BfsStrategy {
    index: Arc<SymbolIndex>,
}

impl BfsStrategy {
    /// Create a new BFS strategy wrapping the given symbol index.
    pub fn new(index: Arc<SymbolIndex>) -> Self {
        Self { index }
    }
}

/// Return the crate-root prefix of `path` (first 2 slash-delimited segments).
///
/// Examples:
/// - `"crates/touring-ast/src/store.rs"` → `"crates/touring-ast/"`
/// - `"crates/touring-hooks/lib.rs"` → `"crates/touring-hooks/"`
/// - `"no_slash"` → `""` (no prefix — treat as no-filter)
fn crate_root(path: &str) -> &str {
    let mut depth = 0usize;
    for (i, ch) in path.char_indices() {
        if ch == '/' {
            depth += 1;
            if depth == 2 {
                return &path[..=i];
            }
        }
    }
    // Fewer than 2 slashes — cannot determine crate boundary; return empty
    // so callers treat it as "no filter".
    ""
}

impl BlastRadiusStrategy for BfsStrategy {
    fn name(&self) -> &'static str {
        "bfs_exact"
    }

    fn compute(&self, start_file: &str, config: &AnalysisConfig) -> BlastRadiusResult {
        let start = std::time::Instant::now();

        let depth = if config.blast_depth == 0 {
            usize::MAX
        } else {
            config.blast_depth
        };

        // BFS over reverse_deps to compute real hop distance per affected file.
        // `blast_radius_with_depth()` internally computes distance_map but does
        // not expose it — this implementation carries it to callers.
        let mut affected_files: Vec<AffectedFile> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();

        visited.insert(start_file.to_string());
        queue.push_back((start_file.to_string(), 0));

        while let Some((current, hop)) = queue.pop_front() {
            if hop >= depth {
                continue;
            }
            if let Some(importers) = self.index.reverse_deps.get(&current) {
                for importer in importers {
                    if visited.insert(importer.clone()) {
                        affected_files.push(AffectedFile {
                            path: importer.clone(),
                            distance: hop + 1,
                            co_edit_weight: 0.0,
                        });
                        queue.push_back((importer.clone(), hop + 1));
                    }
                }
            }
        }

        // When cross-crate analysis is disabled, restrict results to files within
        // the same crate as start_file. Crate root = first 2 path segments
        // (e.g. "crates/touring-ast/" for "crates/touring-ast/src/store.rs").
        if !config.cross_crate {
            let root = crate_root(start_file);
            affected_files.retain(|f| root.is_empty() || f.path.starts_with(root));
        }

        affected_files.sort_by_key(|f| f.distance);

        BlastRadiusResult {
            start_file: start_file.to_string(),
            affected_files,
            strategy_used: "bfs_exact".to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        }
    }

    fn latency_tier(&self) -> LatencyTier {
        LatencyTier::Medium
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::AnalysisConfig;
    use touring_code::ast::SymbolIndex;

    fn make_index_with_chain() -> SymbolIndex {
        let mut idx = SymbolIndex::new();
        // a.rs → b.rs → c.rs (2 hops)
        idx.reverse_deps
            .insert("a.rs".to_string(), vec!["b.rs".to_string()]);
        idx.reverse_deps
            .insert("b.rs".to_string(), vec!["c.rs".to_string()]);
        idx
    }

    #[test]
    fn test_bfs_real_distances() {
        let idx = Arc::new(make_index_with_chain());
        let strategy = BfsStrategy::new(idx);
        let config = AnalysisConfig::standard();

        let result = strategy.compute("a.rs", &config);

        let b = result.affected_files.iter().find(|f| f.path == "b.rs");
        let c = result.affected_files.iter().find(|f| f.path == "c.rs");

        assert!(b.is_some(), "b.rs should be affected");
        assert_eq!(b.expect("b.rs").distance, 1, "b.rs is 1 hop from a.rs");
        assert!(c.is_some(), "c.rs should be affected");
        assert_eq!(c.expect("c.rs").distance, 2, "c.rs is 2 hops from a.rs");
    }

    #[test]
    fn test_bfs_sorted_by_distance() {
        let idx = Arc::new(make_index_with_chain());
        let strategy = BfsStrategy::new(idx);
        let config = AnalysisConfig::standard();

        let result = strategy.compute("a.rs", &config);

        for pair in result.affected_files.windows(2) {
            let (a, b) = (
                pair.first().expect("windows(2) first"),
                pair.get(1).expect("windows(2) second"),
            );
            assert!(
                a.distance <= b.distance,
                "results must be sorted by distance"
            );
        }
    }

    #[test]
    fn test_bfs_empty_graph() {
        let idx = Arc::new(SymbolIndex::new());
        let strategy = BfsStrategy::new(idx);
        let result = strategy.compute("a.rs", &AnalysisConfig::standard());

        assert!(
            result.affected_files.is_empty(),
            "no reverse deps = no affected files"
        );
    }

    #[test]
    fn test_bfs_depth_limit() {
        let mut idx = SymbolIndex::new();
        idx.reverse_deps
            .insert("a.rs".to_string(), vec!["b.rs".to_string()]);
        idx.reverse_deps
            .insert("b.rs".to_string(), vec!["c.rs".to_string()]);

        let strategy = BfsStrategy::new(Arc::new(idx));
        let mut config = AnalysisConfig::standard();
        config.blast_depth = 1;

        let result = strategy.compute("a.rs", &config);

        assert!(
            result.affected_files.iter().any(|f| f.path == "b.rs"),
            "b.rs within depth 1"
        );
        assert!(
            !result.affected_files.iter().any(|f| f.path == "c.rs"),
            "c.rs beyond depth 1"
        );
    }

    #[test]
    fn test_bfs_cross_crate_filter() {
        let mut idx = SymbolIndex::new();
        // same-crate dep: mylib/a.rs → mylib/b.rs
        idx.reverse_deps.insert(
            "crates/mylib/src/a.rs".to_string(),
            vec![
                "crates/mylib/src/b.rs".to_string(),
                "crates/other/src/c.rs".to_string(),
            ],
        );

        let strategy = BfsStrategy::new(Arc::new(idx));

        // cross_crate=false (standard): only same-crate files returned
        let config_no_cross = AnalysisConfig::standard(); // cross_crate: false
        let result = strategy.compute("crates/mylib/src/a.rs", &config_no_cross);
        assert!(
            result
                .affected_files
                .iter()
                .any(|f| f.path == "crates/mylib/src/b.rs"),
            "same-crate file must be included when cross_crate=false"
        );
        assert!(
            !result
                .affected_files
                .iter()
                .any(|f| f.path == "crates/other/src/c.rs"),
            "cross-crate file must be filtered when cross_crate=false"
        );

        // cross_crate=true (deep): all files returned
        let config_with_cross = AnalysisConfig::deep(); // cross_crate: true
        let result2 = strategy.compute("crates/mylib/src/a.rs", &config_with_cross);
        assert!(
            result2
                .affected_files
                .iter()
                .any(|f| f.path == "crates/other/src/c.rs"),
            "cross-crate file must be included when cross_crate=true"
        );
    }

    #[test]
    fn test_crate_root_extraction() {
        assert_eq!(
            crate_root("crates/touring-ast/src/store.rs"),
            "crates/touring-ast/"
        );
        assert_eq!(
            crate_root("crates/touring-hooks/lib.rs"),
            "crates/touring-hooks/"
        );
        assert_eq!(crate_root("no_slash"), "");
        assert_eq!(crate_root("one/slash_only"), "");
    }
}
