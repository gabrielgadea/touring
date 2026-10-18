//! touring_pyright — VGP layer 2: the project's own type checker as a warm, bounded verifier.
//!
//! Extracted from `touring-lsp` so that the hook crates can depend on it without closing the package
//! cycle `touring-lsp →(optional, feature lsp-bridge) touring-hooks → touring-dispatch →
//! touring-hook-handlers → touring-lsp` (cargo counts an optional edge even when its feature is off).
//! `touring-lsp` re-exports these three modules, so `touring_lsp::pool::checar` keeps compiling.
//!
//! ## Architecture
//!   - [`client`]: a synchronous stdio LSP client (std + `serde_json`, no async runtime): framing,
//!     request/notify, answers to the server's own requests, version-matched `publishDiagnostics`.
//!   - [`pyright`]: discovery of the project's `pyright-langserver`, the diagnostic → finding
//!     ports (`negacao`, `escopo`, `severidade`) and [`pyright::PyrightSession`].
//!   - [`pool`]: the long-lived registry that keeps one warm session per project root for the hook
//!     daemon (idle reaping, a cap on live sessions, backoff, kill switch).

#![deny(missing_docs)]
#![forbid(unsafe_code)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod client;
pub mod pool;
pub mod pyright;
