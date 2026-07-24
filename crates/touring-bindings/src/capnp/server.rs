//! [`HolonRegistry`] trait implementation — discovery entry point.
//!
//! A single [`TouringCapnpServer`] is bound to a Unix socket and handed
//! to `capnp_rpc::RpcSystem`; clients reach individual holons via
//! [`get_holon`](crate::holon_core_capnp::holon_registry::Server::get_holon)
//! (promise-pipelined) or enumerate the full holarchy via
//! `list_holons` / `find_by_capability`.

use std::path::PathBuf;

use capnp::capability::Promise;

use crate::capnp::SPEC_VERSION;
use crate::capnp::discover::{self, ManifestRecord};
use crate::capnp::holon_core_capnp::{holon, holon_registry};
use crate::capnp::holon_impl::TouringHolonImpl;

/// RPC server for the THSF Holon Registry.
///
/// Holds the holarchy scan root. Each RPC call re-discovers manifests
/// from disk so the server always reflects the latest filesystem state
/// (at the cost of a walk per request — negligible for typical 30-holon
/// holarchies; caching is a Fase 3 D3.4 optimization).
#[derive(Debug, Clone)]
pub struct TouringCapnpServer {
    /// Root of the THSF holarchy. Defaults to `/home/gabrielgadea`
    /// to match the Fase 1+2 baseline.
    pub root: PathBuf,
}

impl TouringCapnpServer {
    /// Build a new server rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Default server rooted at `$HOME` when available, or `/` otherwise.
    pub fn default_root() -> Self {
        let root = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::new(root)
    }

    /// Scan the filesystem (blocking). Should be called inside
    /// `spawn_blocking` when used from async trait bodies.
    fn scan(&self) -> Vec<ManifestRecord> {
        discover::discover_holons(&self.root, discover::DEFAULT_DEPTH)
    }
}

impl holon_registry::Server for TouringCapnpServer {
    fn list_holons(
        self: ::capnp::capability::Rc<Self>,
        _params: holon_registry::ListHolonsParams,
        mut results: holon_registry::ListHolonsResults,
    ) -> Promise<(), capnp::Error> {
        let holons = self.scan();
        let mut list_builder = results.get().init_holons(holons.len() as u32);
        for (i, m) in holons.iter().enumerate() {
            let mut b = list_builder.reborrow().get(i as u32);
            fill_holon_info(&mut b, m);
        }
        Promise::ok(())
    }

    fn get_holon(
        self: ::capnp::capability::Rc<Self>,
        params: holon_registry::GetHolonParams,
        mut results: holon_registry::GetHolonResults,
    ) -> Promise<(), capnp::Error> {
        let name = match params.get().and_then(|p| p.get_name()) {
            Ok(reader) => match reader.to_str() {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return Promise::err(capnp::Error::failed(format!(
                        "get_holon: name is not valid UTF-8: {e}"
                    )));
                }
            },
            Err(e) => {
                return Promise::err(capnp::Error::failed(format!(
                    "get_holon: failed to read params: {e}"
                )));
            }
        };

        // Verify the holon actually exists before handing out a capability.
        let exists = self.scan().iter().any(|m| m.name == name);
        if !exists {
            return Promise::err(capnp::Error::failed(format!(
                "get_holon: no holon named '{name}' under {}",
                self.root.display()
            )));
        }

        let impl_ = TouringHolonImpl::new(name, self.root.clone());
        let client: holon::Client = capnp_rpc::new_client(impl_);
        results.get().set_holon(client);
        Promise::ok(())
    }

    fn find_by_capability(
        self: ::capnp::capability::Rc<Self>,
        params: holon_registry::FindByCapabilityParams,
        mut results: holon_registry::FindByCapabilityResults,
    ) -> Promise<(), capnp::Error> {
        let capability = match params.get().and_then(|p| p.get_capability()) {
            Ok(r) => match r.to_str() {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return Promise::err(capnp::Error::failed(format!(
                        "find_by_capability: bad UTF-8: {e}"
                    )));
                }
            },
            Err(e) => {
                return Promise::err(capnp::Error::failed(format!("find_by_capability: {e}")));
            }
        };

        let all = self.scan();
        let matches: Vec<&ManifestRecord> = discover::providers_of(&all, &capability);
        let mut list_builder = results.get().init_holons(matches.len() as u32);
        for (i, m) in matches.iter().enumerate() {
            let mut b = list_builder.reborrow().get(i as u32);
            fill_holon_info(&mut b, m);
        }
        Promise::ok(())
    }

    fn symbiosis(
        self: ::capnp::capability::Rc<Self>,
        _params: holon_registry::SymbiosisParams,
        mut results: holon_registry::SymbiosisResults,
    ) -> Promise<(), capnp::Error> {
        // For the MVP, symbiosis is a degenerate local cycle: enumerate
        // holons and report a zero-handshake summary. A richer server-
        // side symbiosis can be wired in Fase 3 D3.4 without changing
        // the wire contract.
        let holons = self.scan();
        let mut b = results.get().init_report();
        b.set_holons_discovered(holons.len() as u32);
        b.set_handshakes_attempted(0);
        b.set_handshakes_accepted(0);
        b.set_handshakes_rejected(0);
        b.set_elapsed_ms(0);
        let _ = b.init_edges(0);
        Promise::ok(())
    }

    fn spec_version(
        self: ::capnp::capability::Rc<Self>,
        _params: holon_registry::SpecVersionParams,
        mut results: holon_registry::SpecVersionResults,
    ) -> Promise<(), capnp::Error> {
        results.get().set_version(SPEC_VERSION);
        Promise::ok(())
    }
}

// ---------------------------------------------------------------------------
// Shared helpers used by both server & holon_impl
// ---------------------------------------------------------------------------

/// Flatten a [`ManifestRecord`] into the Cap'n Proto `HolonInfo` builder.
/// Kept pub(crate) so `TouringHolonImpl::info` can call it.
pub(crate) fn fill_holon_info(
    builder: &mut crate::capnp::holon_core_capnp::holon_info::Builder,
    m: &ManifestRecord,
) {
    builder.set_name(m.name.as_str());
    builder.set_version(m.version.as_str());
    builder.set_description(m.description.as_str());
    builder.set_manifest_path(m.manifest_path.to_string_lossy().as_ref());
    builder.set_autonomy_guarantee(m.autonomy_guarantee);

    let mut offers_b = builder.reborrow().init_offers(m.offers.len() as u32);
    for (i, o) in m.offers.iter().enumerate() {
        let mut c = offers_b.reborrow().get(i as u32);
        c.set_name(o.name.as_str());
        c.set_description("");
        c.set_version(o.version.as_str());
        c.set_schema_hash("");
        c.set_adapter(adapter_from_str(&o.adapter));
    }

    let mut req_b = builder.reborrow().init_requires(m.requires.len() as u32);
    for (i, r) in m.requires.iter().enumerate() {
        req_b.set(i as u32, r.name.as_str());
    }
}

/// Map the lowercase adapter string from TOML into the Cap'n Proto enum.
pub(crate) fn adapter_from_str(s: &str) -> crate::capnp::holon_core_capnp::Adapter {
    use crate::capnp::holon_core_capnp::Adapter;
    match s {
        "capnp" => Adapter::Capnp,
        "wasm" => Adapter::Wasm,
        _ => Adapter::Cli,
    }
}
