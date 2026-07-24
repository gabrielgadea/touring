//! `touring_storage` — unified storage layer for Touring.
//!
//! Fuses five historically-separate storage crates into a single crate with
//! feature-gated heavy backends. Each submodule preserves the public API of
//! its origin crate, which is kept available through one-file re-export shim
//! crates for one minor version.
//!
//! - [`vfs`] — virtual filesystem overlay for multi-source file management
//!   (was `touring-vfs`).
//! - [`salsa`] — salsa incremental memoization for blast radius, wiring
//!   chains, and file-knowledge queries (was `touring-incremental-salsa`).
//! - [`vec`](mod@vec) — vector store abstraction with sqlite-vec / Qdrant / Postgres
//!   backends (was `touring-vector-store`).
//! - [`embeddings`] — embedding provider abstraction: Candle BGE, FastEmbed,
//!   Voyage AI (was `touring-embeddings`).
//! - [`hybrid_search`] — hybrid BM25 + vector search with reranking and
//!   query-intent detection (was `touring-search-fusion`).
//!
//! The `touring-index` crate (incremental symbol indexing) is intentionally
//! *not* fused here: it depends on `touring-ast` / `touring-semantics`, which
//! makes it an intelligence-layer crate. Fusing it would create a Cargo
//! dependency cycle (`touring-code -> touring-vfs -> touring-storage ->
//! touring-ast -> touring-code`). It is deferred to W6 (touring-intelligence).

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (1 fix: vfs/file_set.rs
// `strip_prefix(prefix_trimmed).unwrap()` → `.expect(..)` [starts_with-guarded]; the
// ~143 `.unwrap()` in `knowledge/tests.rs` are the A5-relocated FileKnowledgeDB test
// suite — `#[cfg(test)] mod tests` in knowledge.rs:301) — lock against future bare
// unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

/// Errors re-export so knowledge submodules can use `crate::errors::Result`.
/// Mirrors the path used in the origin crate (touring-hooks-core).
#[cfg(feature = "knowledge")]
pub(crate) mod errors {
    pub use touring_foundation::Result;
}

/// GPU embedding client — async client to an external embedding service
/// (relocated from `touring-foundation::embedding` in A4 P4; distinct from the
/// [`embeddings`] provider abstraction). Gated by the `gpu-embeddings` feature.
#[cfg(feature = "gpu-embeddings")]
pub mod embedding;
pub mod embeddings;
/// Functional wiring — purpose-based chain detection for [`knowledge::FileKnowledgeDB`].
/// Gated by the `knowledge` feature.
#[cfg(feature = "knowledge")]
pub mod functional_wiring;
pub mod hybrid_search;
/// Knowledge DB — SQLite-backed file-knowledge graph, wiring, and functional-chain layers.
/// Gated by the `knowledge` feature (requires rusqlite + sha2 + chrono + touring-intelligence).
#[cfg(feature = "knowledge")]
pub mod knowledge;
/// Wiring persistence layer for [`knowledge::FileKnowledgeDB`].
/// Gated by the `knowledge` feature.
#[cfg(feature = "knowledge")]
pub mod knowledge_wiring;
pub mod salsa;
pub mod vec;
pub mod vfs;
