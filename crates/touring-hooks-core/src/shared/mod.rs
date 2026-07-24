//! Shared facade for touring-hooks-core (daemon-lib-rearch Phase C, 2026-06-10).
//!
//! The engine modules carved from `touring-hooks` reference
//! `crate::shared::<submodule>` exactly as they did in the parent. Seven of
//! those submodules live in the `touring-hooks-shared` leaf crate and are
//! re-exported below; `session_bus` is the one shared submodule that belongs
//! to the core layer itself — it consumes `ann_memory`
//! (touring-hooks-prediction), and the prediction crate already depends on
//! the leaf, so relocating session_bus to the leaf would create a dependency
//! cycle (prediction → shared → prediction).

pub use touring_hooks_shared::async_runtime;
pub use touring_hooks_shared::feature_flags;
pub use touring_hooks_shared::gate_metrics;
pub use touring_hooks_shared::hook_events;
pub use touring_hooks_shared::moka_policies;
pub use touring_hooks_shared::mpatch_preview;
pub use touring_hooks_shared::query_cache; // knowledge_wiring invalidates the wiring-modules query cache
pub use touring_hooks_shared::result_ext;
// Mirror the parent façade: `crate::shared::ResultExt` is the historical
// import path used by the engine modules (branch_fs et al.).
pub(crate) use result_ext::ResultExt;

pub mod session_bus;
