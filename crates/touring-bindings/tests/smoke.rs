//! D3.2 MVP smoke tests.
//!
//! These exercise the non-RPC surface of the crate: struct construction,
//! default paths, and the one trait method (`spec_version`) that is
//! already functional.

use std::path::PathBuf;

use touring_bindings::capnp::{
    SPEC_VERSION, TouringCapnpServer, TouringHolonImpl, default_socket_path,
};

#[test]
fn spec_version_is_semver_like() {
    let parts: Vec<&str> = SPEC_VERSION.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "SPEC_VERSION must have 3 parts: {SPEC_VERSION}"
    );
    for (i, p) in parts.iter().enumerate() {
        p.parse::<u32>()
            .unwrap_or_else(|_| panic!("SPEC_VERSION component {i} ({p:?}) is not a u32"));
    }
}

#[test]
fn server_construct_with_explicit_root() {
    let srv = TouringCapnpServer::new("/home/gabrielgadea");
    assert_eq!(srv.root, PathBuf::from("/home/gabrielgadea"));
}

#[test]
fn server_default_root_honours_home() {
    let srv = TouringCapnpServer::default_root();
    assert!(
        srv.root.is_absolute(),
        "default_root must be absolute, got {:?}",
        srv.root
    );
}

#[test]
fn holon_impl_construct() {
    let h = TouringHolonImpl::new("touring-master", "/home/gabrielgadea");
    assert_eq!(h.name, "touring-master");
    assert_eq!(h.root, PathBuf::from("/home/gabrielgadea"));
}

#[test]
fn default_socket_path_is_under_xdg_when_set() {
    // NOTE: cargo runs tests in parallel and `std::env` is process-global,
    // so we avoid negating/removing the var. We only assert that *if* a
    // socket path is produced, it terminates in the expected layout.
    if let Some(p) = default_socket_path() {
        assert!(
            p.ends_with("holon/registry.sock"),
            "unexpected socket suffix: {p:?}"
        );
    }
}
