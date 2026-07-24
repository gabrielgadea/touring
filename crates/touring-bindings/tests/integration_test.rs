//! Integration tests for `touring_bindings`.
//!
//! These run via `cargo test -p touring-bindings`. Each binding module is
//! opt-in via `bind-*` feature flags; default = empty (no modules active).
//! Per-binding tests live in the `[[test]]` entries in Cargo.toml.

#[test]
fn smoke_crate_compiles() {
    // Verifies the crate compiles with default (empty) features.
    let _ = std::mem::size_of::<()>();
}
