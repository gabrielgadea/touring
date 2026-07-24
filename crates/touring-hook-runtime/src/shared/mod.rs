//! Shared facade for touring-hook-runtime (Wave R+C carve, 2026-06-10).
//!
//! The four session engines carved from the dispatch layer live here;
//! the leaf submodules they reference re-export from `touring-hooks-shared`
//! (and `session_bus` from the core), preserving every historical
//! `crate::shared::<submodule>` path byte-for-byte.

pub use touring_hooks_core::shared::session_bus;
pub use touring_hooks_shared::api_cascade_bridge;
pub use touring_hooks_shared::cascade_queue;
pub use touring_hooks_shared::detect_language;
pub use touring_hooks_shared::feature_flags;
pub use touring_hooks_shared::gate_metrics; // Wave H: ceg_adapter records verdict counters
pub use touring_hooks_shared::hook_events;
pub use touring_hooks_shared::span_context;

/// Shared quality scoring utilities for hook handlers.
pub mod quality;
// Wave H (2026-06-10): shared by daemon.rs (dispatch) + post_write/post_edit
// (handlers). Tantivy-pure (consumes tantivy_index::SymbolDoc/global_tantivy)
// — gated like its dependency (the monolith's default-on feature masked this).
/// Shared file re-indexing helpers driven by post-edit and post-write hooks.
pub mod reindex;
pub mod session_context;
pub mod signals;
#[cfg(feature = "tantivy-fts")]
pub mod tantivy_stream;
