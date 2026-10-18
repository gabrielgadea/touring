//! touring_pyright — VGP layer 2: the project's own type checker as a warm, bounded verifier.
//!
//! Born as the pyright layer (extracted from `touring-lsp` so that the hook crates can depend on it
//! without closing the package cycle `touring-lsp →(optional, feature lsp-bridge) touring-hooks →
//! touring-dispatch → touring-hook-handlers → touring-lsp`; cargo counts an optional edge even when
//! its feature is off), and generalised by VL0 §6 to every language the engine speaks: the crate still
//! depends on no `touring-*` crate, and `touring-lsp` re-exports `client`, `pool` and `pyright`, so
//! `touring_lsp::pool::checar` keeps compiling.
//!
//! ## Architecture
//!   - [`achado`]: the neutral result ([`achado::L2Resultado`], [`achado::L2Achado`]) and the one
//!     severity policy.
//!   - [`client`]: a synchronous stdio LSP client (std + `serde_json`, no async runtime): framing,
//!     request/notify, answers to the server's own requests, version-matched `publishDiagnostics`,
//!     LSP 3.17 pull diagnostics and the rust-analyzer quiescence flag.
//!   - [`servidor`]: [`servidor::Servidor`] — pyright · rust-analyzer · tsserver · deno — and how a
//!     project is matched to one.
//!   - [`pyright`]: discovery of the project's `pyright-langserver`, the diagnostic → finding ports
//!     (`negacao`, `escopo`) and [`pyright::PyrightSession`].
//!   - [`pool`]: the long-lived registry that keeps one warm session per `(project, verifier)` for the
//!     hook daemon (idle reaping, caps, backoff, background warm-up, kill switches).
//!
//! Private: `lsp_nucleo` (the LSP check shared by pyright, rust-analyzer and deno), `rust_analyzer`,
//! `tsserver`, `deno` and `ts_diag` (the TypeScript code → finding table shared by tsserver and deno).

#![deny(missing_docs)]
#![forbid(unsafe_code)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod achado;
pub mod client;
mod deno;
mod lsp_nucleo;
pub mod pool;
pub mod pyright;
mod rust_analyzer;
pub mod servidor;
#[cfg(test)]
mod testutil;
mod ts_diag;
mod tsserver;
