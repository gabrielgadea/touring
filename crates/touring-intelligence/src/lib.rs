//! `touring_intelligence` — Unified intelligence layer for Touring.
//!
//! Fuses four historically-separate crates under one namespace:
//!
//! | Module        | Origin crate       | Contents                                       |
//! |---------------|--------------------|------------------------------------------------|
//! | [`reasoning`] | `touring-cognitive`| MCTS, GoT, ACO, pensieve, BM25, session graph  |
//! | [`rl`]        | `touring-learning` | QTable, LinUCB, bandit, clustering, evolution  |
//! | [`ann`]       | `touring-antt`     | ANN index, reranker, semantic chunker          |
//! | [`index`]     | `touring-index`    | Incremental symbol indexing, file watcher      |
//!
//! ## Migration
//!
//! Source crates remain as one-file shim crates that re-export from this
//! canonical home. Consumers should migrate imports:
//!
//! ```text
//! use touring_intelligence::reasoning::X   →   use touring_intelligence::reasoning::X
//! use touring_intelligence::rl::X    →   use touring_intelligence::rl::X
//! use touring_intelligence::ann::X        →   use touring_intelligence::ann::X
//! use touring_intelligence::index::X       →   use touring_intelligence::index::X
//! ```
//!
//! W6 of `touring-premium-refactor-2026`.

// NOTE: `unsafe` is permitted — the fused `reasoning` module (ex-touring-cognitive)
// carries `unsafe impl Send/Sync` for GPU/wgpu pipeline types.
// `module_inception` may trigger when a submodule shares a name with its parent dir.

// DOC-06 (2026-06-13): `missing_docs` ratcheted to `deny` — all ~920 public items
// across the fused `reasoning`/`rl`/`ann`/`index` modules are now documented, so
// the fused crate no longer predates the doc-coverage policy it now enforces.
#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (6 fixes: 2 aco-graph
// map-gets → expect [contains_key/insert-guarded], 2 pheromone strip_prefix → expect
// [starts_with-guarded], 2 e2e_audit asserts → expect) — lock against future bare
// unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod ann;
pub mod index;
pub mod reasoning;
pub mod rl;

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_modules_accessible() {
        // Verify the 4 module paths compile.
        let _ = std::mem::size_of::<()>();
    }
}
