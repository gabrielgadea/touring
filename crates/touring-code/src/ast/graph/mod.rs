//! AST Graph - Dependency analysis and cross-file symbol indexing
//!
//! Provides: import extraction, dependency graph, blast radius analysis

pub mod blast_radius;
pub mod cycles;
pub mod enriched;
pub mod imports;
pub mod method_calls;
pub mod pheromone;

pub use blast_radius::*;
pub use enriched::*;
pub use imports::*;
pub use method_calls::*;
pub use pheromone::*;
// cycles: methods on SymbolIndex, re-exported implicitly via the struct itself.

use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::ast::error::AstResult;
use crate::ast::languages::Lang;
use crate::ast::symbols::extract_symbols;

/// INS-A4: serde skip helper — omit `co_edit_weight` when 0.0.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// A location where a symbol is defined or referenced
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolLocation {
    /// File path (relative to project root)
    pub file_path: String,
    /// Symbol name
    pub symbol_name: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (0-indexed)
    pub column: usize,
    /// Whether this is a definition (true) or reference (false)
    pub is_definition: bool,
    /// Canonical symbol kind (`SymbolKind::as_str()`: `function`, `class`,
    /// `const`, `variable`, `method`, …). `None` when the producing path did
    /// not classify it (e.g. legacy rows, reference-only locations, or symbols
    /// loaded from a pre-`kind` index). Additive (`#[serde(default)]`) so RPC
    /// payloads and persisted snapshots stay backward/forward compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl SymbolLocation {
    /// Create a new symbol location (kind unset — use [`Self::with_kind`] to classify).
    pub fn new(
        file_path: impl Into<String>,
        symbol_name: impl Into<String>,
        line: usize,
        column: usize,
        is_definition: bool,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            symbol_name: symbol_name.into(),
            line,
            column,
            is_definition,
            kind: None,
        }
    }

    /// Builder: attach a canonical symbol kind (e.g. `SymbolKind::as_str()`).
    /// Chains after [`Self::new`] so the 5-arg constructor stays source-compatible
    /// with all existing call sites.
    #[must_use]
    pub fn with_kind(mut self, kind: Option<String>) -> Self {
        self.kind = kind;
        self
    }
}

/// Dependency edge between files
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyEdge {
    /// Source file (imports from)
    pub from: String,
    /// Target file (imports to)
    pub to: String,
    /// Imported symbols
    pub symbols: Vec<String>,
    /// INS-A4: co-edit weight — higher = files are frequently co-edited.
    /// Used as edge weight in weighted blast-radius (Dijkstra-style).
    /// Range [0.0, ∞). Defaults to 0.0 (no co-edit signal).
    #[serde(default, skip_serializing_if = "crate::ast::graph::is_zero_f64")]
    pub co_edit_weight: f64,
}

/// Number of logical shards used internally to pre-group file-keyed data
/// during bulk `index_files` operations. Public fields remain flat HashMaps
/// for backward compatibility with all consumers.
pub const SHARD_COUNT: usize = 16;

/// Compute a shard bucket index from a file path.
///
/// Hashes the first path component (typically the crate/package name) so that
/// files from the same crate land in the same bucket. This improves cache
/// locality during sequential merge after parallel extraction.
#[inline]
fn shard_for(file_path: &str) -> usize {
    let key = file_path.split('/').next().unwrap_or(file_path);
    let mut h: usize = 0;
    for b in key.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as usize);
    }
    h % SHARD_COUNT
}

/// Cross-file symbol index for project-wide searches
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolIndex {
    /// Map: symbol name -> locations
    pub symbols: HashMap<String, Vec<SymbolLocation>>,
    /// Map: file path -> symbols defined in that file
    pub file_to_symbols: HashMap<String, Vec<String>>,
    /// Dependency graph: file -> files it imports
    pub dependencies: HashMap<String, Vec<DependencyEdge>>,
    /// Reverse dependencies: file -> files that import it
    pub reverse_deps: HashMap<String, Vec<String>>,
}

impl SymbolIndex {
    /// Create empty symbol index
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all entries for idempotent reload — call before `load_into_index` to prevent duplicate accumulation
    pub fn clear(&mut self) {
        self.symbols.clear();
        self.file_to_symbols.clear();
        self.dependencies.clear();
        self.reverse_deps.clear();
    }

    /// Index a single file
    #[instrument(skip(self, source), fields(file = %file_path, lang = %lang.as_str()))]
    pub fn index_file(&mut self, file_path: &str, source: &str, lang: Lang) -> AstResult<()> {
        // Extract symbols from file
        let symbols = extract_symbols(source, lang)?;

        // Add symbol definitions
        for sym in &symbols {
            let location = SymbolLocation::new(file_path, &sym.name, sym.line, sym.column, true)
                .with_kind(Some(sym.kind.as_str().to_string()));

            self.symbols
                .entry(sym.name.clone())
                .or_default()
                .push(location);

            self.file_to_symbols
                .entry(file_path.to_string())
                .or_default()
                .push(sym.name.clone());
        }

        // Extract imports/dependencies
        let imports = extract_imports(source, lang);
        let mut edge = DependencyEdge {
            from: file_path.to_string(),
            to: String::new(),
            symbols: Vec::new(),
            co_edit_weight: 0.0,
        };

        for imp in imports {
            edge.to = imp.module_path;
            edge.symbols = imp.symbols;

            // Add to dependencies
            self.dependencies
                .entry(file_path.to_string())
                .or_default()
                .push(edge.clone());

            // Add to reverse dependencies
            self.reverse_deps
                .entry(edge.to.clone())
                .or_default()
                .push(file_path.to_string());
        }

        Ok(())
    }

    /// Remove a file from the symbol index.
    ///
    /// Removes all symbol definitions, file entries, and dependency edges
    /// associated with the given path.
    pub fn remove_file(&mut self, file_path: &str) {
        // Extract module name from file path (e.g., "utils.py" -> "utils")
        let module_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.rsplit_once('.'))
            .map(|(name, _)| name.to_string())
            .unwrap_or_else(|| file_path.to_string());

        // Remove symbol definitions
        self.symbols.retain(|_name, locations| {
            locations.retain(|loc| loc.file_path != file_path);
            !locations.is_empty()
        });

        // Remove file-to-symbols entry
        self.file_to_symbols.remove(file_path);

        // Remove outgoing dependency edges (file_path imports something)
        if let Some(edges) = self.dependencies.remove(file_path) {
            // For each removed edge, also remove from reverse_deps of target (module name)
            for edge in edges {
                if let Some(deps) = self.reverse_deps.get_mut(&edge.to) {
                    deps.retain(|f| f != file_path);
                }
            }
        }

        // Remove incoming dependency edges (files that import this module)
        // reverse_deps is keyed by MODULE NAME, not file path
        if let Some(importers) = self.reverse_deps.remove(&module_name) {
            for importer in importers {
                if let Some(edges) = self.dependencies.get_mut(&importer) {
                    // Remove edges that point to this module (by module name, not file path)
                    edges.retain(|e| e.to != module_name);
                }
            }
        }
    }

    /// Index multiple files using rayon for parallel symbol extraction.
    ///
    /// Phase 1 is fully parallelized (CPU-bound extraction via rayon).
    /// Phase 2 merges results sequentially, but pre-groups them by shard bucket
    /// (keyed on the first path component, i.e. crate/package name) so that
    /// consecutive HashMap insertions touch the same memory region — improving
    /// L2/L3 cache utilisation on large monorepos.
    #[instrument(skip(self, files), fields(file_count = files.len()))]
    pub fn index_files(&mut self, files: &[(&str, &str, Lang)]) -> AstResult<()> {
        if files.is_empty() {
            return Ok(());
        }

        // Phase 1: parallel extraction (CPU-bound, no &mut self needed)
        let mut extracted: Vec<AstResult<_>> = files
            .par_iter()
            .map(|(path, source, lang)| {
                let symbols = extract_symbols(source, *lang)?;
                let imports = extract_imports(source, *lang);
                Ok((path.to_string(), shard_for(path), symbols, imports))
            })
            .collect();

        // Phase 2: sort by shard bucket before sequential merge so that files
        // from the same crate/package are processed consecutively. This keeps
        // HashMap probing warm in CPU cache across related entries.
        extracted.sort_by_key(|r| r.as_ref().map(|(_, shard, _, _)| *shard).unwrap_or(0));

        // Pre-reserve capacity to avoid incremental rehashing during the merge.
        let file_count = files.len();
        self.file_to_symbols.reserve(file_count);
        self.dependencies.reserve(file_count);

        // Phase 3: sequential merge into the index (requires &mut self)
        for result in extracted {
            let (path, _shard, symbols, imports) = result?;

            for sym in &symbols {
                let location = SymbolLocation::new(&path, &sym.name, sym.line, sym.column, true)
                    .with_kind(Some(sym.kind.as_str().to_string()));
                self.symbols
                    .entry(sym.name.clone())
                    .or_default()
                    .push(location);
                self.file_to_symbols
                    .entry(path.clone())
                    .or_default()
                    .push(sym.name.clone());
            }

            for imp in imports {
                let edge = DependencyEdge {
                    from: path.clone(),
                    to: imp.module_path,
                    symbols: imp.symbols,
                    co_edit_weight: 0.0,
                };
                self.dependencies
                    .entry(path.clone())
                    .or_default()
                    .push(edge.clone());
                self.reverse_deps
                    .entry(edge.to.clone())
                    .or_default()
                    .push(path.clone());
            }
        }
        Ok(())
    }

    /// Find all locations of a symbol by name
    pub fn find_symbol(&self, name: &str) -> Vec<&SymbolLocation> {
        self.symbols
            .get(name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Find symbol with exact file match
    pub fn find_symbol_in_file(&self, name: &str, file_path: &str) -> Option<&SymbolLocation> {
        self.symbols
            .get(name)
            .and_then(|locations| locations.iter().find(|loc| loc.file_path == file_path))
    }

    /// Search symbols by prefix (for autocomplete)
    pub fn search_symbols(&self, prefix: &str) -> Vec<&SymbolLocation> {
        self.symbols
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .flat_map(|(_, locations)| locations.iter())
            .collect()
    }

    /// Get all symbols defined in a file
    pub fn get_file_symbols(&self, file_path: &str) -> Vec<&SymbolLocation> {
        self.file_to_symbols
            .get(file_path)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| self.find_symbol_in_file(name, file_path))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Calculate blast radius: all files transitively affected by a change to a file.
    ///
    /// Traverses the full reverse dependency graph with no depth limit.
    pub fn blast_radius(&self, start_file: &str) -> BlastRadius {
        self.blast_radius_with_depth(start_file, usize::MAX)
    }

    /// Calculate blast radius with a maximum traversal depth.
    ///
    /// P5.4: Limits BFS traversal to `max_depth` hops from the start file.
    /// This prevents runaway computation on deeply connected graphs.
    /// A `max_depth` of 0 returns only the start file itself.
    ///
    /// Wave 13: When `BLAST_LATENCY=1` env var is set, captures per-depth
    /// timing in `BlastRadius::blast_span` and `BlastRadius::trace_us`.
    pub fn blast_radius_with_depth(&self, start_file: &str, max_depth: usize) -> BlastRadius {
        let timing_enabled = std::env::var("BLAST_LATENCY").as_deref() == Ok("1");
        let start_time = if timing_enabled {
            Some(std::time::Instant::now())
        } else {
            None
        };

        let mut affected_files = HashSet::new();
        let mut affected_symbols = Vec::new();
        let mut distance_map = HashMap::new();

        let mut queue = VecDeque::new();
        queue.push_back((start_file.to_string(), 0));
        affected_files.insert(start_file.to_string());
        distance_map.insert(start_file.to_string(), 0);

        // Wave 13: per-depth timing tracking
        let mut depth_times: Vec<u64> = vec![0; max_depth.min(64)];
        let mut depth_file_counts: Vec<usize> = vec![0; max_depth.min(64)];
        let mut current_depth = 0usize;
        let mut depth_start_time = start_time;

        while let Some((current_file, distance)) = queue.pop_front() {
            // Wave 13: track time at depth boundary
            if timing_enabled {
                if distance > current_depth && current_depth < depth_times.len() {
                    if let Some(t) = depth_start_time {
                        depth_times[current_depth] = t.elapsed().as_micros() as u64;
                    }
                    current_depth = distance;
                    depth_start_time = Some(std::time::Instant::now());
                }
                if current_depth < depth_file_counts.len() {
                    depth_file_counts[current_depth] += 1;
                }
            }

            // Get files that import current_file (only if within depth limit)
            if distance < max_depth
                && let Some(importers) = self.reverse_deps.get(&current_file)
            {
                for importer in importers {
                    if !affected_files.contains(importer) {
                        affected_files.insert(importer.clone());
                        distance_map.insert(importer.clone(), distance + 1);
                        queue.push_back((importer.clone(), distance + 1));
                    }
                }
            }

            // Get symbols from this file
            if let Some(symbols) = self.file_to_symbols.get(&current_file) {
                for sym in symbols {
                    affected_symbols.push((current_file.clone(), sym.clone()));
                }
            }
        }

        // Wave 13: finalize timing
        let total_us = start_time
            .map(|t| t.elapsed().as_micros() as u64)
            .unwrap_or(0);
        let hop_count = depth_file_counts.iter().filter(|&&c| c > 0).count();
        let max_depth_time_us = depth_times.iter().copied().max().unwrap_or(0);

        // Build BlastSpan if timing enabled
        let blast_span = if timing_enabled {
            let hops: Vec<BlastHop> = depth_times
                .iter()
                .enumerate()
                .filter(|&(_, &t)| t > 0)
                .map(|(depth, &duration)| {
                    let cumulative = depth_file_counts[..=depth].iter().sum();
                    BlastHop {
                        depth,
                        file_count: depth_file_counts[depth],
                        duration_us: duration,
                        cumulative_files: cumulative,
                    }
                })
                .collect();
            Some(BlastSpan {
                trace_id: blast_radius::new_trace_id(),
                hops,
                total_duration_us: total_us,
            })
        } else {
            None
        };

        let file_count = affected_files.len();
        BlastRadius {
            start_file: start_file.to_string(),
            affected_files: affected_files.into_iter().collect(),
            affected_symbols,
            max_distance: distance_map.values().copied().max().unwrap_or(0),
            file_count,
            trace_us: total_us,
            hop_count,
            max_depth_time_us,
            blast_span,
        }
    }

    /// INS-A4: Weighted blast radius using `co_edit_weight` as Dijkstra edge cost.
    ///
    /// Files co-edited together frequently (high `co_edit_weight`) have lower
    /// effective distance and surface earlier in the result. Returns affected
    /// files sorted by ascending weighted distance (lowest cost = highest priority).
    ///
    /// Edge weight is `1.0 / (1.0 + co_edit_weight)` — higher co-edit weight
    /// means the edge is "shorter" (more correlated change).
    pub fn weighted_blast_radius(&self, start_file: &str) -> Vec<(String, f64)> {
        // Min-heap: (neg_distance, file) — BinaryHeap is a max-heap.
        #[derive(PartialEq)]
        struct State {
            cost: f64,
            file: String,
        }
        impl Eq for State {}
        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for State {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                // Reverse for min-heap: smaller cost = higher priority.
                other
                    .cost
                    .partial_cmp(&self.cost)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        }

        let mut dist: HashMap<String, f64> = HashMap::new();
        let mut heap = BinaryHeap::new();

        dist.insert(start_file.to_string(), 0.0);
        heap.push(State {
            cost: 0.0,
            file: start_file.to_string(),
        });

        while let Some(State { cost, file }) = heap.pop() {
            // Skip stale entries.
            if dist.get(&file).copied().unwrap_or(f64::INFINITY) < cost {
                continue;
            }
            // Traverse reverse deps (files that import `file`).
            if let Some(importers) = self.reverse_deps.get(&file) {
                for importer in importers {
                    // Find the edge weight from the dependency graph.
                    let edge_weight = self
                        .dependencies
                        .get(importer)
                        .and_then(|edges| edges.iter().find(|e| e.to == file))
                        .map(|e| 1.0 / (1.0 + e.co_edit_weight))
                        .unwrap_or(1.0);
                    let next_cost = cost + edge_weight;
                    let current_dist = dist.get(importer).copied().unwrap_or(f64::INFINITY);
                    if next_cost < current_dist {
                        dist.insert(importer.clone(), next_cost);
                        heap.push(State {
                            cost: next_cost,
                            file: importer.clone(),
                        });
                    }
                }
            }
        }

        let mut result: Vec<(String, f64)> = dist.into_iter().collect();
        result.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        result
    }

    /// Find shortest path of dependencies between two files
    pub fn dependency_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<String, String> = HashMap::new();

        queue.push_back(from.to_string());
        visited.insert(from.to_string());

        while let Some(current) = queue.pop_front() {
            if current == to {
                // Reconstruct path
                let mut path = vec![to.to_string()];
                let mut node = to.to_string();

                while let Some(p) = parent.get(&node) {
                    path.push(p.clone());
                    node = p.clone();
                }

                path.reverse();
                return Some(path);
            }

            // Get files imported by current
            if let Some(deps) = self.dependencies.get(&current) {
                for edge in deps {
                    if !visited.contains(&edge.to) {
                        visited.insert(edge.to.clone());
                        parent.insert(edge.to.clone(), current.clone());
                        queue.push_back(edge.to.clone());
                    }
                }
            }
        }

        None
    }

    /// Query symbols by name pattern and optional kind filter
    pub fn query_symbols(&self, name_pattern: &str, kind: Option<&str>) -> Vec<&SymbolLocation> {
        let pattern_lower = name_pattern.to_lowercase();
        self.symbols
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&pattern_lower))
            .flat_map(|(_, locations)| locations.iter())
            .filter(|loc| {
                if let Some(k) = kind {
                    // Filter by definition status as a proxy for kind
                    match k {
                        "definition" => loc.is_definition,
                        "reference" => !loc.is_definition,
                        _ => true,
                    }
                } else {
                    true
                }
            })
            .collect()
    }

    /// Get stats about the index
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            total_symbols: self.symbols.len(),
            total_locations: self.symbols.values().map(|v| v.len()).sum(),
            total_files: self.file_to_symbols.len(),
            total_dependencies: self.dependencies.values().map(|v| v.len()).sum(),
        }
    }
}

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Number of unique symbol names
    pub total_symbols: usize,
    /// Number of symbol locations (definitions + references)
    pub total_locations: usize,
    /// Number of indexed files
    pub total_files: usize,
    /// Number of dependency edges
    pub total_dependencies: usize,
}

/// Build a `SymbolIndex` from a slice of `(file_path, source, language)` triples.
///
/// Convenience wrapper around `SymbolIndex::index_files` for callers that have
/// all file data available up-front. Returns an error if any file fails to parse.
pub fn build_index_from_files(files: &[(&str, &str, Lang)]) -> AstResult<SymbolIndex> {
    let mut index = SymbolIndex::new();
    index.index_files(files)?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn test_symbol_index() {
        let mut index = SymbolIndex::new();

        let python_source = r#"
def hello():
    pass

class Foo:
    def bar(self):
        return 42
"#;

        index
            .index_file("test.py", python_source, Lang::Python)
            .unwrap();

        let locations = index.find_symbol("hello");
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file_path, "test.py");
        assert_eq!(locations[0].line, 2);
    }

    #[test]
    fn test_blast_radius() {
        let mut index = SymbolIndex::new();

        // File A exports symbol
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();

        // File B imports from A
        index
            .index_file(
                "b.py",
                "from a import helper\ndef use_helper(): helper()",
                Lang::Python,
            )
            .unwrap();

        // File C imports from B
        index
            .index_file(
                "c.py",
                "from b import use_helper\nuse_helper()",
                Lang::Python,
            )
            .unwrap();

        // Manual setup of reverse deps for test
        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);
        index
            .reverse_deps
            .insert("b.py".to_string(), vec!["c.py".to_string()]);

        let radius = index.blast_radius("a.py");
        assert!(radius.affected_files.contains(&"b.py".to_string()));
        assert!(radius.affected_files.contains(&"c.py".to_string()));
        assert_eq!(radius.max_distance, 2);
    }

    #[test]
    fn test_dependency_path() {
        let mut index = SymbolIndex::new();

        // Setup dependency chain
        index.dependencies.insert(
            "a.rs".to_string(),
            vec![DependencyEdge {
                from: "a.rs".to_string(),
                to: "b.rs".to_string(),
                symbols: vec!["helper".to_string()],
                co_edit_weight: 0.0,
            }],
        );
        index.dependencies.insert(
            "b.rs".to_string(),
            vec![DependencyEdge {
                from: "b.rs".to_string(),
                to: "c.rs".to_string(),
                symbols: vec!["utils".to_string()],
                co_edit_weight: 0.0,
            }],
        );

        let path = index.dependency_path("a.rs", "c.rs");
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path, vec!["a.rs", "b.rs", "c.rs"]);
    }

    // -- P5.4: blast_radius_with_depth tests --

    #[test]
    fn test_blast_radius_with_depth_zero() {
        let mut index = SymbolIndex::new();
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();
        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);

        // depth=0 means only the start file itself
        let radius = index.blast_radius_with_depth("a.py", 0);
        assert_eq!(radius.file_count, 1);
        assert!(radius.affected_files.contains(&"a.py".to_string()));
        assert!(!radius.affected_files.contains(&"b.py".to_string()));
        assert_eq!(radius.max_distance, 0);
    }

    #[test]
    fn test_blast_radius_with_depth_one() {
        let mut index = SymbolIndex::new();
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();
        index
            .index_file("b.py", "from a import helper", Lang::Python)
            .unwrap();
        index
            .index_file("c.py", "from b import helper", Lang::Python)
            .unwrap();

        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);
        index
            .reverse_deps
            .insert("b.py".to_string(), vec!["c.py".to_string()]);

        // depth=1: a.py + direct importers only (b.py), NOT c.py
        let radius = index.blast_radius_with_depth("a.py", 1);
        assert!(radius.affected_files.contains(&"a.py".to_string()));
        assert!(radius.affected_files.contains(&"b.py".to_string()));
        assert!(
            !radius.affected_files.contains(&"c.py".to_string()),
            "c.py is 2 hops away, should not be included at depth=1"
        );
        assert_eq!(radius.max_distance, 1);
    }

    #[test]
    fn test_blast_radius_with_depth_unlimited_matches_original() {
        let mut index = SymbolIndex::new();
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();

        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);
        index
            .reverse_deps
            .insert("b.py".to_string(), vec!["c.py".to_string()]);

        let full = index.blast_radius("a.py");
        let unlimited = index.blast_radius_with_depth("a.py", usize::MAX);

        assert_eq!(full.file_count, unlimited.file_count);
        assert_eq!(full.max_distance, unlimited.max_distance);
    }
}
