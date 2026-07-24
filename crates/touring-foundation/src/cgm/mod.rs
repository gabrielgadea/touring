//! D41 — Code Graph Model (CGM) for NeurIPS 2025
//!
//! Provides graph attention networks for code representation. This module is the
//! **intentional public API surface** for the Code Graph Model research project.
//!
//! # Architecture
//!
//! - [`graph_attention`] — Graph attention layer for code graph representation
//! - [`scip_export`] — SCIP export for interoperability
//!
//! # Naming Note
//!
//! The CgmScip* types (CgmScipSymbol, CgmScipOccurrence, CgmScipDocument,
//! CgmScipExport) are intentionally prefixed with "Cgm" to avoid homonimia with
//! touring-server's SCIP types. The "Cgm" prefix denotes "Code Graph Model".
//!
//! # Public API Surface (8 symbols)
//!
//! These symbols are part of the stable CGM API and are intended for use by
//! external consumers (research tooling, SCIP integrations, graph analysis):
//!
//! | Symbol | Type | Purpose |
//! |--------|------|---------|
//! | `GraphAttentionConfig` | struct | Configuration for graph attention layers |
//! | `GraphNode` | struct | Node in a code graph representation |
//! | `GraphEdge` | struct | Edge in a code graph representation |
//! | `CodeGraph` | struct | Full code graph with nodes and edges |
//! | `GraphAttentionLayer` | struct | Graph attention layer for code graph |
//! | `CgmScipExport` | struct | SCIP format exporter (0 internal consumers — intentional public API surface for external research consumers per D41 NeurIPS 2025) |
//! | `export_to_scip` | fn | Export CodeGraph to SCIP format |
//! | `CgmScipSymbol` | struct | SCIP symbol representation (0 internal consumers — intentional public API surface for external research consumers per D41 NeurIPS 2025) |
//! | `CgmScipOccurrence` | struct | SCIP occurrence (0 internal consumers — intentional public API surface for external research consumers per D41 NeurIPS 2025) |
//! | `CgmScipDocument` | struct | SCIP document (0 internal consumers — intentional public API surface for external research consumers per D41 NeurIPS 2025) |
//!
//! All symbols above are **exported via `touring_foundation::cgm`** and are part of
//! the stable public API. The `#[allow(dead_code)]` suppressions are intentional
//! — these symbols exist for the API contract and may have consumers outside
//! the touring workspace (research tools, SCIP integrations).

pub mod graph_attention;
pub mod scip_export;

#[cfg(test)]
mod integration_tests;

// Re-exports — stable CGM public API
pub use graph_attention::CodeGraph;
pub use graph_attention::GraphAttentionConfig;
pub use graph_attention::GraphAttentionLayer;
pub use graph_attention::GraphEdge;
pub use graph_attention::GraphNode;
pub use scip_export::{
    CgmScipDocument, CgmScipExport, CgmScipOccurrence, CgmScipSymbol, export_to_scip,
};
