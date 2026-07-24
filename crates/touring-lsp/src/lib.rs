//! touring_lsp — LSP server bridging Touring cross-file capabilities (tower-lsp).
//!
//! Exposes Touring's working cross-file analysis (references, rename) to editors
//! over the Language Server Protocol.
//!
//! ## Architecture
//!   - [`mapping`]: pure, dependency-free translation between LSP-shaped
//!     requests and the Touring backend payloads. Always compiled; fully
//!     unit-tested without a live server.
//!   - `server` (feature `lsp-bridge`): the async `tower_lsp::LanguageServer`
//!     impl. Gated so a default workspace build does NOT pull `tower-lsp`.
//!   - [`error`] / [`types`]: typed errors and shared data types.
//!
//! ## Reachability
//! Built with `--features lsp-bridge`, an editor launches the server over stdio
//! via the binary in `src/bin/touring-lsp.rs` (also feature-gated). The backend
//! depends on `touring-hooks` (never the reverse), owning its own `HookRuntime`.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod error;
pub mod mapping;
pub mod types;

#[cfg(feature = "lsp-bridge")]
pub mod server;

pub use error::{Error, Result};
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_re_exports_compile() {
        let _err: Result<()> = Ok(());
    }

    #[test]
    fn mapping_is_reachable_in_default_build() {
        // Proves the pure mapping layer is part of the default (no-feature) API.
        let payload = mapping::references_request_payload(
            "src/x.rs",
            mapping::LspPosition {
                line: 0,
                character: 0,
            },
        );
        assert_eq!(payload["scope"], "workspace");
    }
}
