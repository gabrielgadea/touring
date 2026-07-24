//! touring-ast — Code intelligence via tree-sitter.
//!
//! Provides AST parsing for Python, Rust, TypeScript, JavaScript.
//! Symbol extraction with enriched metadata (kind enum, parent, docstrings,
//! decorators, async, visibility, complexity), surgery (body replacement),
//! dependency graph, blast radius, and symbol store.
//!
//! # Feature Flags
//!
//! - `more-languages` — enables Go and Java parsing support via
//!   `tree-sitter-go` and `tree-sitter-java`.
//! - `async-pipeline` — enables [`AsyncSharedPipeline`] which wraps
//!   `IncrementalPipeline` in a `tokio::sync::Mutex` for async runtime
//!   compatibility.

// Tier A (2026-04-19): test-only prelude with pretty_assertions shadow.
#[cfg(test)]
pub(crate) mod test_util;

pub mod api_cascade;
pub mod call_graph;
pub mod complexity;
pub mod document;
pub mod error;
pub mod polyglot_semantic;
pub mod rust_semantic;
// Wave 5 (2026-04-18) — one-shot `semantic + public-API + format`
// workflow helper. See src/code_gen_workflow.rs. Consumed by
// touring-hooks post_edit, touring-python, and external agents.
pub mod code_gen_workflow;
pub use code_gen_workflow::{CodeGenWorkflow, WorkflowReport};
pub mod file_heat;
/// Package-aware wiring extraction for Go (`go:<import-path>` keys) — gated with
/// the Go grammar behind `more-languages` (P-H of the polyglot-parity plan).
#[cfg(feature = "more-languages")]
pub mod go_wiring;
pub mod graph;
pub mod import_resolver;
pub mod incremental_pipeline;
pub mod languages;
pub mod learning_loop;
pub mod manifest;
pub mod module_tree;
pub mod node_types;
pub mod parser;
/// Sentrux Master Plan Wave 3 P6 (2026-05-09) — diff-based InputEdit
/// synthesis that potentiates [`parser::IncrementalParser`] without
/// requiring callers to track edits manually.
pub mod parser_diff;
pub mod quality;
pub mod revision;
pub mod scope_map;
pub mod semantic_search;
pub mod speculate;
pub mod ssr;
pub mod store;
pub mod surgery;
pub mod symbol_detail;
pub mod symbols;
pub mod watcher;
pub mod wiring;

pub use file_heat::{FileHeat, HeatMap};
/// Re-export tree_sitter for downstream crates that need QueryCursor / Node / etc.
/// Wave 5 (2026-05-23): VP-Scout Chain 4b cross-shim discovery confirmed live
/// consumers via `touring_code::ast::tree_sitter` (the touring-ast shim re-exports
/// touring-code). The re-export is NOT orphan; keep wired. See REGRA #0.
pub use tree_sitter;

/// Extract text from a tree-sitter node. Zero-copy via byte range.
#[inline]
pub(crate) fn node_text<'a>(source: &'a str, node: tree_sitter::Node) -> &'a str {
    &source[node.byte_range()]
}

pub use complexity::{
    compute_complexity, compute_complexity_for_source, compute_complexity_for_source_from_tree,
    compute_complexity_from_tree, enrich_symbols_with_complexity,
    enrich_symbols_with_complexity_from_tree,
};
pub use error::{AstError, AstResult, AstResultExt, TracedAstError};
pub use graph::{
    BlastRadius, BlastRadiusOutput, DependencyEdge, EnrichedBlastRadius, ImpactCategory,
    ImportInfo, IndexStats, SHARD_COUNT, SymbolIndex, SymbolLocation,
    compute_enriched_blast_radius,
};
#[cfg(feature = "async-pipeline")]
pub use incremental_pipeline::AsyncSharedPipeline;
pub use incremental_pipeline::{
    IncrementalEditResult, IncrementalPipeline, PrioritizedPipeline, SharedPipeline,
};
pub use languages::Lang;
pub use parser::{IncrementalParser, ParsedFile, ParserPool, SharedTree};
pub use semantic_search::SemanticSymbolIndex;
pub use ssr::{
    SsrApplyResult, SsrBatchResult, SsrError, SsrRule, apply_ssr_batch, apply_ssr_rule,
    prebuilt_rules, surgery_ssr, vgp_gate,
};
pub use store::{RenameCandidate, StoreStats, SymbolChangeObserver, SymbolChangeSet, SymbolStore};
pub use surgery::{
    MAX_RECURSION_DEPTH, SurgeryError, format_rust_code, format_rust_code_best_effort,
    replace_symbol_body, replace_symbol_body_for_file, replace_symbol_body_with_lang,
    validate_syntax,
};
pub use symbols::{
    Symbol, SymbolKind, SymbolPath, Visibility, count_params, extract_symbols,
    extract_symbols_batch, extract_symbols_from_file, extract_symbols_with_pool,
    filter_by_complexity, filter_by_kind, find_by_name, find_clones, find_depth_zero_colon,
};
pub use watcher::{FileEvent, FileEventKind, FileWatcher, WatcherError};
pub use wiring::{
    ImportSuggestion, PackageInfo, PubSymbol, SymbolDiff, WorkspaceInfo, detect_reexports,
    detect_unresolved_references, diff_pub_symbols, extract_pub_symbols, suggest_imports,
};

// ─── Quality analysis (code smell detection, complexity, severity) ───────────
pub use quality::{AntiPatternHit, QualityReport, Severity, analyze_quality};

// ─── Speculate re-exports (extract_cfg_gated_pub_items) ──────────────────────
pub use speculate::extract_cfg_gated_pub_items;

// ─── Strategy modules (VGP v2 / Context-Tree / Scope / Import / Speculate / CallGraph / Learning) ───
pub use call_graph::{CallGraph, CallSite, build_call_graph};
pub use import_resolver::{ImportResolver, ResolvedImport, extract_imports_resolved};
pub use learning_loop::{GenerationEvent, LearningLoop};
pub use module_tree::{ModuleNode, ModuleTree};
pub use node_types::{
    LanguageNodeTypes, NodeTypeInfo, importance_threshold, node_types_for_language,
};
pub use scope_map::{ScopeEntry, ScopeKind, ScopeMap, build_scope_map};
pub use speculate::{CfgGatedItem, LayerResult, SpeculateResult, ValidationLayer, speculate_v2};
pub use symbol_detail::{MemberKind, SymbolDetail, extract_symbol_details};

// ─── Format preservation (C.4 — comment-preserving formatter) ─────────────
pub mod format;
pub use format::{
    Gap, PreservingFormatter, SnippetProvider, format_preserve, has_rustfmt_skip, is_idempotent,
};

// ─── E2E Integration Tests ──────────────────────────────────────────────

#[cfg(test)]
mod e2e_tests {
    use super::*;

    #[test]
    fn test_full_pipeline_e2e() {
        let source = r#"
use std::collections::HashMap;

pub struct DataStore {
    data: HashMap<String, Vec<u8>>,
    capacity: usize,
}

pub enum StoreError {
    Full,
    NotFound(String),
    Corrupt { key: String, reason: String },
}

impl DataStore {
    pub fn new(capacity: usize) -> Self {
        Self { data: HashMap::new(), capacity }
    }

    pub fn insert(&mut self, key: String, value: Vec<u8>) -> Result<(), StoreError> {
        if self.data.len() >= self.capacity {
            return Err(StoreError::Full);
        }
        self.data.insert(key, value);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<&Vec<u8>, StoreError> {
        self.data.get(key).ok_or_else(|| StoreError::NotFound(key.to_string()))
    }
}
"#;

        // Strategy 1: VGP v2 — extract struct fields
        let details = extract_symbol_details(source, "DataStore");
        assert!(
            details.iter().any(|d| d.name == "data"),
            "VGP v2: field 'data' not found"
        );
        assert!(
            details.iter().any(|d| d.name == "capacity"),
            "VGP v2: field 'capacity' not found"
        );

        // Strategy 1: VGP v2 — extract enum variants
        let enum_details = extract_symbol_details(source, "StoreError");
        assert!(
            enum_details.iter().any(|d| d.name == "Full"),
            "VGP v2: variant 'Full' not found"
        );
        assert!(
            enum_details.iter().any(|d| d.name == "NotFound"),
            "VGP v2: variant 'NotFound' not found"
        );
        assert!(
            enum_details.iter().any(|d| d.name == "Corrupt"),
            "VGP v2: variant 'Corrupt' not found"
        );

        // Strategy 1: VGP v2 — extract impl methods
        let methods = extract_symbol_details(source, "DataStore");
        assert!(
            methods
                .iter()
                .any(|d| d.name == "new" && d.kind == MemberKind::Method),
            "VGP v2: method 'new' not found"
        );
        assert!(
            methods
                .iter()
                .any(|d| d.name == "insert" && d.kind == MemberKind::Method),
            "VGP v2: method 'insert' not found"
        );
        assert!(
            methods
                .iter()
                .any(|d| d.name == "get" && d.kind == MemberKind::Method),
            "VGP v2: method 'get' not found"
        );

        // Strategy 2: ModuleTree
        let tree = ModuleTree::build_from_source(source, "lib.rs");
        assert_eq!(tree.root.name, "lib.rs");

        // Strategy 3: ImportResolver
        let resolver = extract_imports_resolved(source, Lang::Rust);
        assert!(
            !resolver.imports.is_empty(),
            "ImportResolver: no imports found"
        );
        assert!(
            resolver.imports.iter().any(|i| i.path.contains("HashMap")),
            "ImportResolver: HashMap not detected"
        );

        // Strategy 4: ScopeMap
        let scope = build_scope_map(source, Lang::Rust);
        // The source has let bindings inside methods — scope should find them
        let _ = scope;

        // Strategy 5: Speculate v2
        let result = speculate_v2(source, Lang::Rust, Some(&details), Some(&resolver));
        assert!(
            result.composite_score > 0.5,
            "Speculate v2: score too low: {}",
            result.composite_score
        );
        assert_eq!(
            result.layers.len(),
            6,
            "Speculate v2: expected 6 layers, got {}",
            result.layers.len()
        );

        // Strategy 6: CallGraph
        let graph = build_call_graph(source, Lang::Rust);
        // HashMap::new is called inside DataStore::new
        let new_callers = graph.callers_of("new");
        let _ = new_callers; // May or may not match — no crash

        // Strategy 7: LearningLoop
        let mut lloop = LearningLoop::new();
        lloop.record_event(GenerationEvent {
            symbol_name: "DataStore".to_string(),
            language: "rust".to_string(),
            success: true,
            strategy_used: "vgp_v2".to_string(),
            timestamp_ms: 1000,
        });
        lloop.reward("DataStore", 1.0);
        assert_eq!(lloop.event_count, 1);

        eprintln!("=== E2E PIPELINE COMPLETE: PASSED ===");
        eprintln!(
            "VGP v2: {} fields/variants/methods extracted",
            details.len() + enum_details.len()
        );
        eprintln!("ImportResolver: {} imports found", resolver.imports.len());
        eprintln!(
            "Speculate v2: score={:.2}, layers={}",
            result.composite_score,
            result.layers.len()
        );
        eprintln!("LearningLoop: {} event recorded", lloop.event_count);
    }
}
