//! HNSW approximate-nearest-neighbor blast radius strategy.
//!
//! Uses `touring_simd::ann::hnsw::HnswIndex` to find the top-K
//! most structurally similar files to a changed file, returning them
//! as approximate blast radius candidates.
//!
//! # Embedding
//!
//! File paths are converted to f32 vectors via `path_hash_embedding()`,
//! the same technique used by `pre_read` ANN recall in touring-hooks.
//! This clusters files by directory structure — siblings have similar vectors.
//!
//! # When to use
//!
//! `HnswStrategy` uses `LatencyTier::Slow`. `BlastRadiusEngine` selects it
//! only on deep analysis paths (no `budget_ms` cap). On the hook path,
//! `BfsStrategy` takes priority.
//!
//! # Feature gate
//!
//! Requires feature `ann-blast` which activates `touring-simd/ann`.

use super::{AffectedFile, BlastRadiusResult, BlastRadiusStrategy, LatencyTier};
use crate::engine::AnalysisConfig;
use touring_code::ast::SymbolIndex;
use touring_simd::ann::AnnIndex;
use touring_simd::ann::hnsw::{HnswConfig, HnswIndex};

/// Embedding dimension for path-hash vectors.
/// Must be consistent with pre_read ANN recall dim.
pub const HNSW_EMBED_DIM: usize = 64;

/// Maximum files to insert into HNSW index (guards construction time on large codebases).
const MAX_HNSW_FILES: usize = 5000;

/// FNV-1a path-hash embedding — maps a file path to a normalized 64-dim f32 vector.
///
/// Splits the path on `/` and hashes each segment independently using FNV-1a.
/// Earlier segments (directories) receive higher weight (`1.0 / (seg_idx + 1.0)`),
/// clustering files by directory structure — siblings have similar vectors.
///
/// # Relationship to touring-hooks path_hash_embedding
///
/// `touring-hooks/src/ann_memory.rs` also implements a `path_hash_embedding` with the
/// same name and output shape (64-dim, L2-normalized), but the two differ in detail:
/// - hooks version additionally splits on `['.', '_', '-']` and applies position mixing
/// - this version splits only on `'/'` and uses a single dim-index per segment
///
/// They produce the same *clustering behaviour* (directory proximity) but are NOT
/// numerically identical. Do not mix embeddings from the two crates in the same index.
pub fn path_hash_embedding(path: &str) -> Vec<f32> {
    let mut vec = vec![0.0f32; HNSW_EMBED_DIM];
    for (seg_idx, seg) in path.split('/').enumerate() {
        // FNV-1a hash of segment bytes
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in seg.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x00000100_000001b3);
        }
        let dim_idx = (hash as usize) % HNSW_EMBED_DIM;
        // Weight earlier segments (directory structure) more heavily
        let weight = 1.0 / (seg_idx as f32 + 1.0);
        // dim_idx is always < HNSW_EMBED_DIM by construction; use get_mut for clippy safety
        if let Some(v) = vec.get_mut(dim_idx) {
            *v += weight;
        }
    }
    // L2-normalise via SIMD dot-product (touring-simd::simd_utils::matrix).
    // Note: retains the prior `max(1e-9)` guard behaviour through the
    // helper's internal `norm > 1e-10` check (tighter guard, same intent:
    // leave near-zero vectors unchanged rather than amplifying noise).
    touring_simd::simd_utils::matrix::l2_normalize_inplace(&mut vec);
    vec
}

/// The files the HNSW index is built over: every key of `reverse_deps` and every file that
/// imports one, each once, in path order, at most `MAX_HNSW_FILES` of them.
///
/// One ordered set does the dedup and the sort in a single pass, so the cost is
/// O((n + e) log n) for `n` files and `e` edges. This was a `Vec::contains` inside the loop
/// over every edge, O(e * n): free while `reverse_deps` was empty in production (the dependency
/// graph was never loaded), ~600 ms at opt-level 0 once the wiring's 76.9 thousand file edges
/// are loaded (C8 probe, 12/11/2026). The result is the same list the old code produced.
fn indexed_files(symbol_index: &SymbolIndex) -> Vec<String> {
    let mut unique: std::collections::BTreeSet<&String> =
        symbol_index.reverse_deps.keys().collect();
    for importers in symbol_index.reverse_deps.values() {
        unique.extend(importers.iter());
    }
    unique.into_iter().take(MAX_HNSW_FILES).cloned().collect()
}

/// HNSW approximate blast radius strategy.
///
/// On construction, builds an HNSW index over all files known to the
/// `SymbolIndex`. On `compute()`, queries the index for top-K similar
/// files and returns them as `AffectedFile` entries with `distance = 1`
/// (ANN does not produce hop distances) and `co_edit_weight` from the
/// HNSW similarity score.
///
/// Use `BlastRadiusEngine::hnsw_only` as a convenience factory, or construct
/// directly and wrap in `Box<dyn BlastRadiusStrategy>` for custom compositions.
pub struct HnswStrategy {
    /// Underlying HNSW index, pre-built from SymbolIndex.
    index: HnswIndex,
    /// All file paths in insertion order (index → path).
    files: Vec<String>,
    /// Number of nearest neighbors to return.
    k: usize,
}

impl HnswStrategy {
    /// Build an HNSW strategy from the given SymbolIndex.
    ///
    /// Inserts all files tracked in `index.reverse_deps` keys and their
    /// dependent files. Index construction is O(n log n).
    /// Files are capped at `MAX_HNSW_FILES` to guard against slow construction.
    pub fn new(symbol_index: &SymbolIndex, k: usize) -> Self {
        // Wave 23: use the path-hash-tuned preset (REGRA #0 potencializar).
        // Embeddings are produced by `path_hash_embedding(path)` (64-dim deterministic
        // path hashes), so the lower `ef_construction=100` / `ef_search=20` preset
        // gives ~2x faster build with negligible recall loss at this dimensionality.
        let mut hnsw = HnswIndex::new(HnswConfig::for_path_hashes());

        let files = indexed_files(symbol_index);
        for (id, path) in files.iter().enumerate() {
            let embedding = path_hash_embedding(path);
            hnsw.insert(id, embedding);
        }

        Self {
            index: hnsw,
            files,
            k,
        }
    }
}

impl BlastRadiusStrategy for HnswStrategy {
    fn name(&self) -> &'static str {
        "hnsw_ann"
    }

    fn compute(&self, start_file: &str, config: &AnalysisConfig) -> BlastRadiusResult {
        let start = std::time::Instant::now();

        if self.index.is_empty() {
            return BlastRadiusResult {
                start_file: start_file.to_string(),
                affected_files: vec![],
                strategy_used: "hnsw_ann".to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
                truncated: false,
            };
        }

        let query_vec = path_hash_embedding(start_file);
        // Use blast_depth as k if non-zero, otherwise use self.k
        let effective_k = if config.blast_depth > 0 {
            config.blast_depth.min(self.files.len())
        } else {
            self.k.min(self.files.len())
        };
        let results = self.index.search(&query_vec, effective_k);

        let affected_files: Vec<AffectedFile> = results
            .into_iter()
            .filter_map(|r| {
                let path = self.files.get(r.index)?;
                // Exclude the start file itself
                if path == start_file {
                    return None;
                }
                Some(AffectedFile {
                    path: path.clone(),
                    distance: 1, // ANN does not compute hop distance
                    co_edit_weight: r.score.clamp(0.0, 1.0),
                })
            })
            .collect();

        BlastRadiusResult {
            start_file: start_file.to_string(),
            affected_files,
            strategy_used: "hnsw_ann".to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        }
    }

    fn latency_tier(&self) -> LatencyTier {
        // Slow tier: selected only on deep analysis (no budget cap)
        LatencyTier::Slow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::AnalysisConfig;
    use touring_code::ast::SymbolIndex;

    fn make_index_with_files() -> SymbolIndex {
        let mut idx = SymbolIndex::new();
        idx.reverse_deps.insert(
            "src/lib.rs".to_string(),
            vec!["src/main.rs".to_string(), "src/utils.rs".to_string()],
        );
        idx.reverse_deps.insert(
            "src/utils.rs".to_string(),
            vec!["tests/utils_test.rs".to_string()],
        );
        idx
    }

    #[test]
    fn test_hnsw_strategy_name() {
        let idx = make_index_with_files();
        let strategy = HnswStrategy::new(&idx, 5);
        assert_eq!(strategy.name(), "hnsw_ann");
    }

    #[test]
    fn test_hnsw_latency_tier_is_slow() {
        let idx = make_index_with_files();
        let strategy = HnswStrategy::new(&idx, 5);
        assert_eq!(strategy.latency_tier(), LatencyTier::Slow);
    }

    #[test]
    fn test_hnsw_returns_neighbors_excluding_start_file() {
        let idx = make_index_with_files();
        let strategy = HnswStrategy::new(&idx, 10);
        let result = strategy.compute("src/lib.rs", &AnalysisConfig::deep());
        assert_eq!(result.strategy_used, "hnsw_ann");
        assert!(
            result.affected_files.iter().all(|f| f.path != "src/lib.rs"),
            "start file must not appear in affected_files"
        );
    }

    #[test]
    fn test_hnsw_empty_index_returns_empty_result() {
        let idx = SymbolIndex::new();
        let strategy = HnswStrategy::new(&idx, 5);
        let result = strategy.compute("src/lib.rs", &AnalysisConfig::deep());
        assert!(result.affected_files.is_empty());
        assert_eq!(result.strategy_used, "hnsw_ann");
    }

    /// The list the code built before the ordered set: a `Vec` filled behind a `contains`
    /// scan, then sorted, deduplicated and cut. It is the oracle `indexed_files` must equal.
    fn legacy_indexed_files(idx: &SymbolIndex) -> Vec<String> {
        let mut files: Vec<String> = idx.reverse_deps.keys().cloned().collect();
        for deps in idx.reverse_deps.values() {
            for dep in deps {
                if !files.contains(dep) {
                    files.push(dep.clone());
                }
            }
        }
        files.sort();
        files.dedup();
        files.truncate(MAX_HNSW_FILES);
        files
    }

    /// `edges` importer edges over `names` files, drawn by a fixed LCG. Producers come from
    /// the first third of the names only, importers from all of them: producers and importers
    /// overlap, one importer can repeat under a producer, and two thirds of the files are
    /// importers that never appear as a key.
    fn graph_of(names: usize, edges: usize) -> SymbolIndex {
        let name = |i: usize| format!("crates/c{}/src/m{i}.rs", i % 37);
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 33) as usize) % names
        };
        let mut idx = SymbolIndex::new();
        for _ in 0..edges {
            let producer = name(next() % (names / 3));
            let importer = name(next());
            idx.reverse_deps.entry(producer).or_default().push(importer);
        }
        idx
    }

    #[test]
    fn indexed_files_equals_the_old_list_on_a_graph_with_overlap_and_repeats() {
        let idx = graph_of(300, 2_000);
        let files = indexed_files(&idx);
        assert_eq!(files, legacy_indexed_files(&idx));
        // The fixture is not trivial: importers that never appear as a key are in the list,
        // and the same importer is listed more than once under some producers.
        assert!(
            files.len() > idx.reverse_deps.len(),
            "importer-only files must be indexed"
        );
        let listed: usize = idx.reverse_deps.values().map(Vec::len).sum();
        assert!(
            listed > files.len(),
            "edges outnumber files, so repeats exist"
        );
        assert!(files.windows(2).all(|w| w[0] < w[1]), "sorted and unique");
    }

    #[test]
    fn indexed_files_keeps_the_same_files_when_the_cap_cuts() {
        let idx = graph_of(6_000, 12_000);
        let mut every: std::collections::BTreeSet<&String> = idx.reverse_deps.keys().collect();
        every.extend(idx.reverse_deps.values().flatten());
        assert!(
            every.len() > MAX_HNSW_FILES,
            "the fixture must exceed the cap to test it"
        );

        let files = indexed_files(&idx);
        assert_eq!(files.len(), MAX_HNSW_FILES);
        assert_eq!(files, legacy_indexed_files(&idx));
    }

    #[test]
    fn indexed_files_of_an_empty_index_is_empty() {
        assert!(indexed_files(&SymbolIndex::new()).is_empty());
    }

    #[test]
    fn test_path_hash_embedding_is_normalized() {
        let vec = path_hash_embedding("src/analysis/blast_radius/mod.rs");
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "embedding must be L2-normalized, got {norm}"
        );
    }

    #[test]
    fn test_path_hash_embedding_different_paths_differ() {
        let a = path_hash_embedding("src/lib.rs");
        let b = path_hash_embedding("tests/integration.rs");
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        assert!(dot < 0.99, "different-dir paths should differ: dot={dot}");
    }
}
