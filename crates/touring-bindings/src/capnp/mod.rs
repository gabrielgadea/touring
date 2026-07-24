//! THSF Fase 3 — Cap'n Proto typed federation server.
//!
//! This crate hosts a [`HolonRegistry`] RPC server that speaks Cap'n Proto
//! over a Unix-domain socket (default: `$XDG_RUNTIME_DIR/holon/registry.sock`).
//!
//! # Module layout
//!
//! - [`holon_core_capnp`]: auto-generated bindings from
//!   `schemas/holon-core.capnp` (produced by `build.rs` via `capnpc`).
//! - [`server`]: [`TouringCapnpServer`] — implements `HolonRegistry`.
//!   Enumerates discoverable holons via filesystem scan + delegates
//!   per-holon work to [`TouringHolonImpl`].
//! - [`holon_impl`]: [`TouringHolonImpl`] — implements `Holon`. Each
//!   capability invocation is forwarded to a subprocess `touring`
//!   invocation (matching the Fase 1 CLI adapter semantics), keeping
//!   the RPC layer a thin typed skin over the existing CLI.
//!
//! # Subprocess delegation rationale
//!
//! [`TouringHolonImpl::invoke`] (once implemented) spawns a
//! `holon invoke …` subprocess instead of importing `touring-hooks` as
//! a dependency. This preserves crate-level isolation, avoids pulling
//! `HookRuntime`'s `!Sync` actor into this server, and means the RPC
//! facade behaves identically to the filesystem-based adapter in
//! Fase 1+2.
//!
//! # Status (D3.2 MVP, 2026-04-23)
//!
//! - Schema compiles via `build.rs`.
//! - Trait impls are currently stubs returning
//!   `Promise::err("not_implemented")` except for `spec_version` which
//!   already returns the compile-time [`SPEC_VERSION`].
//! - Next session wires `RpcSystem` + the Unix socket accept loop and
//!   implements the trait method bodies.

#![allow(clippy::all)]
#![allow(warnings)]

// Generated capnp modules live at the crate root (lib.rs) so that the
// capnpc-generated code can reference them as `crate::holon_core_capnp::`.
// Re-export them here for convenience within this module.
pub use crate::holon_core_capnp;
pub use crate::holon_generator_capnp;

pub mod discover;
pub mod embed;
pub mod generator_health;
pub mod holon_impl;
pub mod server;

pub use embed::{EmbedConfig, serve_with_shutdown};
pub use generator_health::GeneratorHealthImpl;
pub use holon_impl::TouringHolonImpl;
pub use server::TouringCapnpServer;

/// Spec version string. MUST remain in lockstep with breaking changes
/// to the canonical schema at
/// `~/.claude/tools/holon/schemas/capnp/holon-core.capnp`.
pub const SPEC_VERSION: &str = "1.0.0";

/// Default Unix socket path for the RPC server, constructed from
/// `XDG_RUNTIME_DIR`. Returns `None` when the env var is unset.
pub fn default_socket_path() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|xdg| {
        let mut p = std::path::PathBuf::from(xdg);
        p.push("holon");
        p.push("registry.sock");
        p
    })
}
