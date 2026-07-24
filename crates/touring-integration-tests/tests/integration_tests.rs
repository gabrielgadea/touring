//! Integration tests: touring-analysis pipeline wired to touring-ast SymbolIndex.

use std::sync::Arc;
use touring_analysis::{
    AnalysisConfig, AnalysisPipeline, AnalysisPipelineBuilder, adaptive_pool_size,
};
use touring_code::ast::{Lang, SymbolIndex};

const RUST_SNIPPET: &str = r#"
use std::collections::HashMap;

pub struct DataStore {
    data: HashMap<String, Vec<u8>>,
    capacity: usize,
}

impl DataStore {
    pub fn new(capacity: usize) -> Self {
        Self { data: HashMap::new(), capacity }
    }

    pub fn insert(&mut self, key: String, value: Vec<u8>) -> bool {
        if self.data.len() >= self.capacity {
            return false;
        }
        self.data.insert(key, value);
        true
    }

    pub fn get(&self, key: &str) -> Option<&Vec<u8>> {
        self.data.get(key)
    }
}

pub enum StoreError {
    Full,
    NotFound(String),
}
"#;

/// Verifies that `SymbolIndex::index_file` finds all expected symbols and that a
/// `AnalysisPipeline` built with `with_symbol_index` accepts it without panicking.
#[test]
fn test_analysis_pipeline_symbol_index_wiring() {
    // Build a SymbolIndex from an in-memory Rust snippet.
    let mut index = SymbolIndex::new();
    index
        .index_file("src/data_store.rs", RUST_SNIPPET, Lang::Rust)
        .expect("index_file must not fail on valid Rust");

    // Verify expected symbols are present.
    let ds = index.find_symbol("DataStore");
    assert!(!ds.is_empty(), "SymbolIndex must contain DataStore");

    let insert = index.find_symbol("insert");
    assert!(!insert.is_empty(), "SymbolIndex must contain insert");

    let stats = index.stats();
    assert!(stats.total_files >= 1, "stats.total_files must be >= 1");
    assert!(
        stats.total_symbols >= 4,
        "stats.total_symbols must be >= 4 (struct + 3 methods)"
    );

    // Wire the index into an AnalysisPipeline and confirm run() returns a valid report.
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    let shared_index = Arc::new(index);

    let pipeline = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .with_symbol_index(shared_index)
        .build();

    // run() must not panic and must return a score in [0, 1].
    let report = pipeline.run(".");
    assert!(
        report.composite_score >= 0.0 && report.composite_score <= 1.0,
        "composite_score {} out of [0,1]",
        report.composite_score
    );
}

/// Verifies `adaptive_pool_size` returns sensible values across a range of inputs.
#[test]
fn test_adaptive_pool_size_enrichment() {
    // Zero crates → pool size must be >= 1 (never 0, would deadlock).
    let zero = adaptive_pool_size(0);
    assert!(zero >= 1, "adaptive_pool_size(0) must be >= 1, got {zero}");

    // Small project: 4 crates → formula min(4*2, avail_par) → never exceeds hw parallelism.
    let small = adaptive_pool_size(4);
    assert!(
        small >= 1,
        "adaptive_pool_size(4) must be >= 1, got {small}"
    );

    // Large project: 100 crates → capped by hardware parallelism (> 0, <= 1024).
    let large = adaptive_pool_size(100);
    assert!(
        (1..=1024).contains(&large),
        "adaptive_pool_size(100)={large} out of [1,1024]"
    );

    // Monotonicity: larger crate_count must not decrease pool size below single-crate.
    let single = adaptive_pool_size(1);
    assert!(
        large >= single,
        "adaptive_pool_size must not decrease as crate_count grows"
    );
}

/// Verifies that `AnalysisPipeline::new` + `run_knowledge` does not panic with
/// a fresh in-memory database (empty state — graceful-default path).
#[test]
fn test_pipeline_graceful_empty_state() {
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());

    // run_knowledge must return a HealthDimension without panicking.
    let dim = pipeline.run_knowledge();
    // Score may be 0.0 (empty DB) but must be in [0, 1].
    assert!(
        dim.score >= 0.0 && dim.score <= 1.0,
        "run_knowledge score {} out of [0,1]",
        dim.score
    );
}
