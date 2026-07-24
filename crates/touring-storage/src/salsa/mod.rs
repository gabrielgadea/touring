//! `touring-incremental-salsa` — Salsa POC for incremental memoization.
//!
//! Provides tracked queries for:
//! - `blast_radius_for_file` — transitive dependency blast radius
//! - `wiring_chains_from` — symbol wiring chain tree
//! - `file_knowledge_extended` — extended file knowledge metadata

pub mod bench;
pub mod db;
pub mod ingest;
pub mod queries;

pub use db::DatabaseImpl;
pub use queries::{BlastRadiusResult, ChainTree, ExtendedKnowledge, FileKey};

// Re-export the real tracked queries + the salsa-input edge model so downstream
// crates (touring-server actors) can drive the incremental engine directly.
pub use bench::{BenchResult, run_benchmark};
pub use db::{EventCounter, FileMeta, FileText, ModuleDecl, SymbolDef, SymbolKind, SymbolUse};
pub use queries::blast::{DefIndex, UseGraph, blast_radius_for_file, direct_consumers};

// Production input-population seam: lets a real data source (the AST SymbolIndex /
// SymbolStore in touring-code) drive the incremental engine via plain salsa-free
// structs, keeping touring-storage a leaf crate (no dependency cycle).
pub use ingest::{IngestDef, IngestGraph, IngestUse, PopulatedInputs, blast_for, populate_inputs};

/// Re-export salsa for use in downstream crates.
pub use salsa;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blast_radius_result_debug() {
        let result = BlastRadiusResult {
            file_id: FileKey::new(0),
            direct_deps: vec![],
            transitive_deps: vec![],
            dep_count: 0,
            max_depth: 0,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("BlastRadiusResult"));
    }

    #[test]
    fn chain_tree_empty() {
        let tree = ChainTree::default();
        assert!(tree.root.is_none());
        assert!(tree.nodes.is_empty());
    }

    #[test]
    fn extended_knowledge_default() {
        let ek = ExtendedKnowledge::default();
        assert_eq!(ek.file_id, FileKey::default());
        assert_eq!(ek.complexity_score, 0.0);
    }
}
