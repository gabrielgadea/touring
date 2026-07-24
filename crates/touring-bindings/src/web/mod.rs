//! touring_web — Web UI crate for Touring with Leptos 0.8 + Axum.
//!
//! Theme: dark/light via CSS variables, toggled via `Theme` signal.

#![warn(missing_docs)]

pub mod components;
// Elite W1 (2026-06-12) — global WorkspaceCtx + RefreshBus contexts.
pub mod app;
pub mod ctx;
pub mod init;
pub mod models;
pub mod routes;
pub mod services;
pub mod theme;
// Axum backend — native only: tokio/mio cannot target wasm32, and the
// WASM bundle (trunk build via the touring-web shim) only needs the client.
#[cfg(not(target_arch = "wasm32"))]
pub mod server;

pub use theme::{Theme, apply_theme, theme_signal};
