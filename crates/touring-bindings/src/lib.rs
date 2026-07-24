//! `touring_bindings` — Unified language-bindings layer for Touring.
//!
//! Fuses seven historically-separate crates under one namespace:
//!
//! | Module      | Origin crate            | Contents                                       |
//! |-------------|-------------------------|------------------------------------------------|
//! | [`python`]  | `touring-python`        | PyO3 bindings → `claude_learning_kernel`       |
//! | [`wasm`]    | `touring-wasm`          | wasmtime inferlet runtime + WASM plugin pool   |
//! | [`capnp`]   | `touring-capnp-server`  | Cap'n Proto RPC server, holon registry         |
//! | [`web`]     | `touring-web`           | Leptos 0.8 web UI (health, orphans, wiring)    |
//! | `desktop`   | `touring-desktop-ui`    | egui 0.34 desktop UI with wiring graph viewer  |
//! | `postgis`   | `touring-geopostgis`    | geozero EWKB PostGIS bridge                    |
//!
//! `touring-web-server` is mounted as [`web::server`] (sub-module of `web`).
//!
//! ## Feature flags
//!
//! All binding modules are opt-in; `default = []` (empty).
//!
//! | Feature             | Enables                              |
//! |---------------------|--------------------------------------|
//! | `bind-python`       | [`python`] module (PyO3 + numpy)     |
//! | `bind-wasm`         | [`wasm`] module (wasmtime)           |
//! | `bind-capnp`        | [`capnp`] module (Cap'n Proto RPC)   |
//! | `bind-web`          | [`web`] + [`web::server`] (Leptos)   |
//! | `bind-desktop`      | `desktop` module (egui)              |
//! | `bind-postgis`      | `postgis` module (geozero sync)      |
//! | `bind-postgis-async`| `postgis` + sqlx async API           |
//!
//! ## Migration
//!
//! Source crates remain as one-file shim crates that re-export from this
//! canonical home:
//!
//! ```text
//! touring_bindings::python::X          →  touring_bindings::python::X
//! touring_bindings::wasm::X            →  touring_bindings::wasm::X
//! touring_bindings::capnp::X    →  touring_bindings::capnp::X
//! touring_bindings::web::X             →  touring_bindings::web::X
//! touring_bindings::web::server::X      →  touring_bindings::web::server::X
//! touring_bindings::desktop::X      →  touring_bindings::desktop::X
//! touring_bindings::postgis::X      →  touring_bindings::postgis::X
//! ```
//!
//! W7 of `touring-premium-refactor-2026`.

// Leptos 0.8 deeply-nested view futures (EliteShell wrapping every route,
// SPEC 2026-06-12 W1) exceed the default query depth when computing layout
// in release builds — the standard remedy per the rustc help message.
#![recursion_limit = "256"]
// DOC-06 (2026-06-13): `missing_docs` ratcheted to `deny` — every hand-written
// public item across all binding flavors (python/wasm/capnp/web/desktop/postgis) is
// documented (verified under `--all-features`). The ~855 capnpc-GENERATED items in
// `holon_core_capnp`/`holon_generator_capnp` carry a module-scoped
// `#[allow(missing_docs)]` (machine output, not hand-documentable). `not(test)`
// exempts test-only fixtures.
#![cfg_attr(not(test), deny(missing_docs))]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (lib-only clippy under
// bind-web/bind-desktop/bind-capnp/bind-python = 0 prod unwraps after the 2 infallible
// idiom fixes: desktop `eframe::run_native().unwrap()` + web `web_sys::window()
// .unwrap()` → `.expect(..)`) — lock against future bare unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

// NOTE: `unsafe` is permitted — PyO3 and wasmtime modules carry unsafe FFI.
// `module_inception` may trigger when a submodule shares a name with its parent.
#[allow(clippy::module_inception)]
#[cfg(feature = "bind-python")]
pub mod python;

#[cfg(feature = "bind-wasm")]
pub mod wasm;

// Cap'n Proto generated modules must live at crate root so that the
// capnpc-generated code can reference them as `crate::holon_core_capnp::`.
/// Cap'n Proto bindings generated from `schemas/holon-core.capnp` by `capnpc`.
#[cfg(feature = "bind-capnp")]
#[allow(clippy::all, clippy::unwrap_used, warnings, missing_docs)] // capnpc-generated code is not hand-documentable (unwrap_used is in clippy::restriction, not clippy::all)
pub mod holon_core_capnp {
    include!(concat!(env!("OUT_DIR"), "/holon_core_capnp.rs"));
}

/// Cap'n Proto bindings generated from `schemas/holon-generator.capnp` by `capnpc`.
#[cfg(feature = "bind-capnp")]
#[allow(clippy::all, clippy::unwrap_used, warnings, missing_docs)] // capnpc-generated code is not hand-documentable (unwrap_used is in clippy::restriction, not clippy::all)
pub mod holon_generator_capnp {
    include!(concat!(env!("OUT_DIR"), "/holon_generator_capnp.rs"));
}

#[cfg(feature = "bind-capnp")]
pub mod capnp;

#[cfg(feature = "bind-web")]
pub mod web;

#[cfg(feature = "bind-desktop")]
pub mod desktop;

#[cfg(feature = "bind-postgis")]
pub mod postgis;

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_modules_accessible() {
        let _ = std::mem::size_of::<()>();
    }
}
