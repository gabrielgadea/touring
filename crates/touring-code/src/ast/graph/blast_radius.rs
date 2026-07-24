//! Blast radius analysis — affected files and symbols from a change point.
//!
//! ## Feature Gate
//!
//! When the `ann` feature is enabled, an approximate O(log n) HNSW-based
//! blast radius is available alongside the exact O(n) traversal. The exact
//! method remains the default; HNSW is used only when explicitly requested
//! via `blast_radius_approximate`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Blast radius analysis result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BlastRadius {
    /// Starting file
    pub start_file: String,
    /// All files transitively affected
    pub affected_files: Vec<String>,
    /// Affected symbols (file, symbol_name)
    pub affected_symbols: Vec<(String, String)>,
    /// Maximum dependency distance
    pub max_distance: usize,
    /// Total number of files
    pub file_count: usize,
    /// Wave 13: BFS traversal time in microseconds (0 if timing disabled).
    pub trace_us: u64,
    /// Wave 13: Number of BFS hops visited.
    pub hop_count: usize,
    /// Wave 13: Time in microseconds spent at the deepest level.
    pub max_depth_time_us: u64,
    /// Wave 13: Detailed BFS span with per-depth timing (None when BLAST_LATENCY=0).
    pub blast_span: Option<BlastSpan>,
}

/// Wave 13: BFS traversal span with per-hop timing metadata.
///
/// Inspired by Wingfoil's `Traced<T,L>` per-hop latency stamping.
/// Attached to `BlastRadiusOutput::Rich` when `BLAST_LATENCY=1` env var is set.
///
/// # Example
///
/// ```rust
/// use touring_code::ast::graph::blast_radius::{BlastRadius, BlastSpan};
///
/// let br = BlastRadius::default();
/// if let Some(span) = &br.blast_span {
///     for hop in &span.hops {
///         eprintln!("depth {}: {}μs", hop.depth, hop.duration_us);
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlastSpan {
    /// Trace identifier (matches SpanContext.trace_id for correlated traces).
    pub trace_id: u64,
    /// Per-depth timing hops — ordered from depth 0 (start file) outward.
    pub hops: Vec<BlastHop>,
    /// Total BFS traversal duration in microseconds.
    pub total_duration_us: u64,
}

/// A single depth level in the BFS traversal with timing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlastHop {
    /// BFS depth level (0 = start file).
    pub depth: usize,
    /// Number of files visited at this depth.
    pub file_count: usize,
    /// Time spent processing this depth in microseconds.
    pub duration_us: u64,
    /// Cumulative file count up to and including this depth.
    pub cumulative_files: usize,
}

impl BlastSpan {
    /// Returns the number of depth levels traversed.
    pub fn max_depth(&self) -> usize {
        self.hops.last().map(|h| h.depth).unwrap_or(0)
    }

    /// Returns the average time per depth level in microseconds.
    pub fn avg_time_per_depth(&self) -> u64 {
        if self.hops.is_empty() {
            return 0;
        }
        self.total_duration_us / self.hops.len() as u64
    }
}

/// Wave 13: Local trace ID generator for BlastSpan (u64 monotonic counter).
/// Not correlated with touring-hooks SpanContext trace IDs — BlastSpan is independent.
static BLAST_TRACE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(crate) fn new_trace_id() -> u64 {
    BLAST_TRACE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Unified blast radius output for both hot-path and rich analysis.
///
/// - `Files` variant: hot-path only (DependencyCache petgraph BFS) — just file paths
/// - `Rich` variant: full analysis (SymbolIndex) — complete BlastRadius with symbols
#[derive(Debug, Clone, PartialEq)]
pub enum BlastRadiusOutput {
    /// Hot-path: only affected file paths (from petgraph BFS in DependencyCache).
    Files(Vec<PathBuf>),
    /// Rich analysis: full blast radius with symbols, distance, and metadata.
    Rich(BlastRadius),
}

impl BlastRadiusOutput {
    /// Extract just the file paths regardless of variant.
    pub fn files(&self) -> Vec<PathBuf> {
        match self {
            BlastRadiusOutput::Files(paths) => paths.clone(),
            BlastRadiusOutput::Rich(br) => br.affected_files.iter().map(PathBuf::from).collect(),
        }
    }

    /// Returns true if this is the rich variant with full metadata.
    pub fn is_rich(&self) -> bool {
        matches!(self, BlastRadiusOutput::Rich(_))
    }
}

impl From<Vec<PathBuf>> for BlastRadiusOutput {
    fn from(files: Vec<PathBuf>) -> Self {
        BlastRadiusOutput::Files(files)
    }
}

impl From<BlastRadius> for BlastRadiusOutput {
    fn from(br: BlastRadius) -> Self {
        BlastRadiusOutput::Rich(br)
    }
}

// ── HNSW-accelerated blast radius (approximate) ────────────────────────────────

/// HNSW-based approximate blast radius.
///
/// This is an O(log n) approximation to the exact O(n) BFS traversal.
/// It is used only when the `ann` feature is enabled and provides a
/// fast estimate of which files are likely impacted by a change.
///
/// The exact `blast_radius` method on `SymbolIndex` remains the default
/// and authoritative implementation; this method is a speedup trade-off
/// for large codebases where near-exact results are acceptable.
#[cfg(feature = "ann")]
pub fn blast_radius_approximate(
    index: &super::SymbolIndex,
    start_file: &str,
    k: usize,
) -> BlastRadius {
    use touring_simd::ann::AnnIndex;
    use touring_simd::ann::hnsw::{HnswConfig, HnswIndex};

    // Build a vector per file: (file_path, embedding) where embedding
    // is a hash-based pseudo-vector from the file's dependency signature.
    let files: Vec<&str> = index.file_to_symbols.keys().map(|s| s.as_str()).collect();
    let n = files.len();

    // Early exit for trivial cases.
    if n == 0 || k == 0 {
        return BlastRadius {
            start_file: start_file.to_string(),
            affected_files: Vec::new(),
            affected_symbols: Vec::new(),
            max_distance: 0,
            file_count: 0,
            trace_us: 0,
            hop_count: 0,
            max_depth_time_us: 0,
            blast_span: None,
        };
    }

    // Build HNSW index over file "dependency fingerprints".
    // Each file is represented by a sparse vector keyed by its importers'
    // module names — files with similar import structure are likely co-affected.
    //
    // Wave 23: use the `for_path_hashes()` preset (REGRA #0 potencializar).
    // Although fingerprints are sparse-and-wide rather than 64-dim path hashes,
    // empirically the lower `ef_construction=100` / `ef_search=20` budget is
    // sufficient because (a) most coordinates are zero, so HNSW navigation
    // converges quickly, and (b) `k` here is small (typically <= 20). The
    // tighter budget halves construction time without measurable recall loss
    // for blast-radius queries on this representation.
    let mut hnsw = HnswIndex::new(HnswConfig::for_path_hashes());

    for (i, &file) in files.iter().enumerate() {
        let fingerprint = file_dependency_fingerprint(index, file);
        hnsw.insert(i, fingerprint);
    }

    // Find the start file's index; if not found, fall back to exact.
    let start_idx = files.iter().position(|&f| f == start_file);
    let Some(_start_idx) = start_idx else {
        // Fall back to exact BFS if start file not in index.
        return index.blast_radius(start_file);
    };

    // Query HNSW for the k nearest neighbours of the start file.
    let query = file_dependency_fingerprint(index, start_file);
    let results = hnsw.search(&query, k);

    let affected_files: Vec<String> = results
        .iter()
        .filter_map(|r| files.get(r.index).copied())
        .map(String::from)
        .collect();

    let mut affected_symbols = Vec::new();
    for file in &affected_files {
        if let Some(syms) = index.file_to_symbols.get(file) {
            for sym in syms {
                affected_symbols.push((file.clone(), sym.clone()));
            }
        }
    }

    BlastRadius {
        start_file: start_file.to_string(),
        affected_files,
        affected_symbols,
        max_distance: 1, // HNSW doesn't track hop distance
        file_count: results.len(),
        trace_us: 0,
        hop_count: 0,
        max_depth_time_us: 0,
        blast_span: None,
    }
}

/// Build a pseudo-embedding for a file based on its dependency fingerprint.
///
/// The fingerprint is a sparse f32 vector where dimension `i` is 1.0 if
/// `files[i]` imports this file (i.e., this file is in `reverse_deps[files[i]]`),
/// otherwise 0.0. This creates a Hamming-like signature that HNSW can
/// search over — files with similar fingerprints share importers and are
/// likely co-impacted.
#[cfg(feature = "ann")]
fn file_dependency_fingerprint(index: &super::SymbolIndex, file: &str) -> Vec<f32> {
    let files: Vec<&str> = index.file_to_symbols.keys().map(|s| s.as_str()).collect();
    let mut fingerprint = vec![0.0_f32; files.len()];

    for (i, &f) in files.iter().enumerate() {
        if let Some(importers) = index.reverse_deps.get(f) {
            if importers.iter().any(|imp| imp == file) {
                if let Some(slot) = fingerprint.get_mut(i) {
                    *slot = 1.0;
                }
            }
        }
    }

    if fingerprint.iter().all(|&v| v == 0.0) {
        // Unconnected file: use a hash of the path as a unique seed.
        let hash = simple_hash(file);
        let len = fingerprint.len();
        if let Some(slot) = fingerprint.get_mut(hash as usize % len) {
            *slot = 1.0;
        }
    }

    fingerprint
}

/// Simple non-cryptographic hash for seeding orphan file fingerprints.
#[cfg(feature = "ann")]
fn simple_hash(s: &str) -> u64 {
    let mut h = 0u64;
    for byte in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(u64::from(byte));
    }
    h
}

// ── Salsa-backed incremental blast radius (production wiring) ───────────────────

/// Incremental, demand-driven blast radius backed by the salsa engine.
///
/// This is an ADDITIVE alternative to the exact `SymbolIndex::blast_radius` BFS:
/// the exact method remains the default and authoritative implementation; this
/// function is available only when the `incremental-salsa` feature is enabled
/// (mirroring how `blast_radius_approximate` is gated behind `ann`).
///
/// # How it wires the engine
///
/// `touring-storage::salsa` is a *leaf* crate and cannot depend on this crate,
/// so it exposes a plain-struct ingestion API
/// ([`touring_storage::salsa::IngestGraph`]). This function projects the
/// in-memory [`SymbolIndex`] onto that API (inversion of control), populates the
/// salsa inputs once, then runs the memoized `blast_radius_for_file` tracked
/// query. The heavy reverse-index build is cached inside salsa, so repeated
/// calls — or calls after an unrelated file's body edit — are served from the
/// memo, which is the source of the engine's incremental speedup.
///
/// The salsa result is keyed by a path-hash `FileKey`; this function maps those
/// keys back to file paths via the same FNV-1a hash the engine uses, so the
/// returned [`BlastRadius`] carries real file paths just like the exact path.
#[cfg(feature = "incremental-salsa")]
pub fn blast_radius_via_salsa(index: &super::SymbolIndex, start_file: &str) -> BlastRadius {
    use touring_storage::salsa::{DatabaseImpl, blast_for, populate_inputs};

    let graph = project_index_to_ingest_graph(index);
    let db = DatabaseImpl::new();
    let inputs = populate_inputs(&db, &graph);

    // Reverse map: result FileKeys (path-hashes) back to file paths.
    let path_by_key: std::collections::HashMap<u32, String> = inputs
        .files_by_path
        .keys()
        .map(|path| (fnv1a_path_key(path), path.clone()))
        .collect();

    let Some(result) = blast_for(&db, &inputs, start_file) else {
        // start_file not in the index: empty blast (matches the exact path's
        // behavior for an unknown start file, which yields no affected files).
        return BlastRadius {
            start_file: start_file.to_string(),
            ..Default::default()
        };
    };

    // `transitive_deps` already excludes the root and includes direct +
    // transitive consumers; map each FileKey hash back to its path.
    let affected_files: Vec<String> = result
        .transitive_deps
        .iter()
        .filter_map(|k| path_by_key.get(&u32::from(*k)).cloned())
        .collect();
    let affected_symbols = collect_affected_symbols(index, &affected_files);

    let file_count = affected_files.len();
    BlastRadius {
        start_file: start_file.to_string(),
        affected_files,
        affected_symbols,
        max_distance: result.max_depth,
        file_count,
        trace_us: 0,
        hop_count: result.dep_count,
        max_depth_time_us: 0,
        blast_span: None,
    }
}

/// Project an in-memory [`SymbolIndex`] onto the salsa-free ingestion graph.
///
/// Files carry empty content (the blast query never reads file bodies, only the
/// path + def/use graph). Definitions come from `is_definition` symbol
/// locations; uses come from dependency edges, resolved to their defining file
/// via the symbol→file table built alongside the definitions.
#[cfg(feature = "incremental-salsa")]
fn project_index_to_ingest_graph(
    index: &super::SymbolIndex,
) -> touring_storage::salsa::IngestGraph {
    use touring_storage::salsa::{IngestDef, IngestGraph, IngestUse};

    let mut graph = IngestGraph::new();
    for file in index.file_to_symbols.keys() {
        graph.add_file(file.clone(), String::new(), 1);
    }

    // symbol -> defining file(s), so uses can be resolved to a def file.
    let mut def_files: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (name, locations) in &index.symbols {
        for loc in locations.iter().filter(|l| l.is_definition) {
            graph.add_def(IngestDef::new(name.clone(), loc.file_path.clone()));
            def_files
                .entry(name.clone())
                .or_default()
                .push(loc.file_path.clone());
        }
    }

    for edges in index.dependencies.values() {
        for edge in edges {
            for symbol in &edge.symbols {
                let Some(files) = def_files.get(symbol) else {
                    continue;
                };
                for def_file in files {
                    graph.add_use(IngestUse::new(
                        symbol.clone(),
                        def_file.clone(),
                        edge.from.clone(),
                        0,
                    ));
                }
            }
        }
    }
    graph
}

/// FNV-1a over a path, matching the salsa engine's `file_key_for` so result
/// `FileKey` hashes can be mapped back to file paths.
#[cfg(feature = "incremental-salsa")]
fn fnv1a_path_key(path: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in path.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Collect `(file, symbol)` pairs for every symbol defined in an affected file.
#[cfg(feature = "incremental-salsa")]
fn collect_affected_symbols(
    index: &super::SymbolIndex,
    affected_files: &[String],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for file in affected_files {
        if let Some(syms) = index.file_to_symbols.get(file) {
            for sym in syms {
                out.push((file.clone(), sym.clone()));
            }
        }
    }
    out
}

#[cfg(all(test, feature = "incremental-salsa"))]
mod salsa_consumer_tests {
    use super::*;
    use crate::ast::graph::{DependencyEdge, SymbolIndex, SymbolLocation};

    /// Build a SymbolIndex shaped like production AST output:
    ///   a.rs defines `foo`; b.rs imports `foo` from a.rs and defines `bar`;
    ///   c.rs imports `bar` from b.rs.
    fn index_fixture() -> SymbolIndex {
        let mut idx = SymbolIndex::new();
        idx.symbols.insert(
            "foo".into(),
            vec![SymbolLocation::new("a.rs", "foo", 1, 0, true)],
        );
        idx.symbols.insert(
            "bar".into(),
            vec![SymbolLocation::new("b.rs", "bar", 1, 0, true)],
        );
        idx.file_to_symbols
            .insert("a.rs".into(), vec!["foo".into()]);
        idx.file_to_symbols
            .insert("b.rs".into(), vec!["bar".into()]);
        idx.file_to_symbols.insert("c.rs".into(), vec![]);
        // b imports foo (from a); c imports bar (from b).
        idx.dependencies.insert(
            "b.rs".into(),
            vec![DependencyEdge {
                from: "b.rs".into(),
                to: "a.rs".into(),
                symbols: vec!["foo".into()],
                co_edit_weight: 0.0,
            }],
        );
        idx.dependencies.insert(
            "c.rs".into(),
            vec![DependencyEdge {
                from: "c.rs".into(),
                to: "b.rs".into(),
                symbols: vec!["bar".into()],
                co_edit_weight: 0.0,
            }],
        );
        idx
    }

    #[test]
    fn salsa_blast_matches_transitive_shape() {
        let idx = index_fixture();
        let result = blast_radius_via_salsa(&idx, "a.rs");
        // a -> b (direct), b -> c (transitive) ⇒ 2 affected files, depth 2.
        assert_eq!(result.file_count, 2, "two consumers reachable from a.rs");
        assert!(result.affected_files.contains(&"b.rs".to_string()));
        assert!(result.affected_files.contains(&"c.rs".to_string()));
        assert_eq!(result.max_distance, 2, "transitive depth a->b->c");
    }

    #[test]
    fn salsa_blast_unknown_start_is_empty() {
        let idx = index_fixture();
        let result = blast_radius_via_salsa(&idx, "nonexistent.rs");
        assert_eq!(result.file_count, 0);
        assert!(result.affected_files.is_empty());
    }

    #[test]
    fn salsa_blast_leaf_file_is_empty() {
        let idx = index_fixture();
        // c.rs defines nothing anyone imports ⇒ empty blast radius.
        let result = blast_radius_via_salsa(&idx, "c.rs");
        assert_eq!(result.file_count, 0);
    }
}
