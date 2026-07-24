//! [`Holon`] trait implementation.
//!
//! Each Cap'n Proto `Holon` capability served by this process is backed
//! by a [`TouringHolonImpl`] instance. Invocations are forwarded to a
//! subprocess `holon invoke` call, preserving the exact CLI-adapter
//! semantics of Fase 1+2 while gaining typed transport via Cap'n Proto.

use std::path::PathBuf;
use std::time::Duration;

use capnp::capability::Promise;

use crate::capnp::discover::{self, ManifestRecord};
use crate::capnp::holon_core_capnp::holon;
use crate::capnp::server::{adapter_from_str, fill_holon_info};

/// In-process representation of a single holon served over RPC.
#[derive(Debug, Clone)]
pub struct TouringHolonImpl {
    /// Identity name of the holon this server instance represents.
    pub name: String,
    /// Holarchy root directory scanned to resolve this holon's manifest.
    pub root: PathBuf,
}

impl TouringHolonImpl {
    /// Construct a new impl bound to `name` under `root`.
    pub fn new(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
        }
    }

    /// Scan the filesystem for the target manifest. Returns `None` when
    /// the holon no longer exists (filesystem mutation races).
    fn find_self(&self) -> Option<ManifestRecord> {
        discover::discover_holons(&self.root, discover::DEFAULT_DEPTH)
            .into_iter()
            .find(|m| m.name == self.name)
    }
}

impl holon::Server for TouringHolonImpl {
    fn list_capabilities(
        self: ::capnp::capability::Rc<Self>,
        _params: holon::ListCapabilitiesParams,
        mut results: holon::ListCapabilitiesResults,
    ) -> Promise<(), capnp::Error> {
        let Some(m) = self.find_self() else {
            return Promise::err(capnp::Error::failed(format!(
                "list_capabilities: holon '{}' not found under {}",
                self.name,
                self.root.display()
            )));
        };

        let mut list_b = results.get().init_capabilities(m.offers.len() as u32);
        for (i, o) in m.offers.iter().enumerate() {
            let mut c = list_b.reborrow().get(i as u32);
            c.set_name(o.name.as_str());
            c.set_description("");
            c.set_version(o.version.as_str());
            c.set_schema_hash("");
            c.set_adapter(adapter_from_str(&o.adapter));
        }
        Promise::ok(())
    }

    fn invoke(
        self: ::capnp::capability::Rc<Self>,
        params: holon::InvokeParams,
        mut results: holon::InvokeResults,
    ) -> Promise<(), capnp::Error> {
        // Extract the request payload before we move into the async block.
        let holon_name = self.name.clone();
        let root = self.root.clone();

        let (capability, args_bytes, requester, timeout_ms) = match extract_invoke(params) {
            Ok(t) => t,
            Err(e) => return Promise::err(e),
        };

        Promise::from_future(async move {
            let start = std::time::Instant::now();
            let timeout = if timeout_ms == 0 {
                Duration::from_secs(30)
            } else {
                Duration::from_millis(u64::from(timeout_ms))
            };

            // Args are JSON bytes per the MVP wire contract.
            let args_str = match std::str::from_utf8(&args_bytes) {
                Ok(s) if !s.is_empty() => s.to_string(),
                Ok(_) => "{}".to_string(),
                Err(e) => {
                    return Err(capnp::Error::failed(format!(
                        "invoke: args bytes are not valid UTF-8: {e}"
                    )));
                }
            };

            let invoke_fut = tokio::process::Command::new("holon")
                .arg("invoke")
                .arg("--root")
                .arg(&root)
                .arg("--requester")
                .arg(&requester)
                .arg(&holon_name)
                .arg(&capability)
                .arg(&args_str)
                .output();

            let output = match tokio::time::timeout(timeout, invoke_fut).await {
                Ok(Ok(o)) => o,
                Ok(Err(e)) => {
                    return Err(capnp::Error::failed(format!("invoke: spawn failed: {e}")));
                }
                Err(_) => {
                    return Err(capnp::Error::failed(format!(
                        "invoke: timeout after {}ms",
                        timeout.as_millis()
                    )));
                }
            };

            let elapsed_ms: u32 = start.elapsed().as_millis().try_into().unwrap_or(u32::MAX);
            let exit_code: i32 = output.status.code().unwrap_or(-1);

            let mut b = results.get().init_response();
            b.set_exit_code(exit_code);
            b.set_stdout(&output.stdout[..]);
            b.set_stderr(&output.stderr[..]);
            b.set_duration_ms(elapsed_ms);
            b.set_logged(exit_code == 0);
            b.set_schema_hash_out("");
            Ok(())
        })
    }

    fn info(
        self: ::capnp::capability::Rc<Self>,
        _params: holon::InfoParams,
        mut results: holon::InfoResults,
    ) -> Promise<(), capnp::Error> {
        let Some(m) = self.find_self() else {
            return Promise::err(capnp::Error::failed(format!(
                "info: holon '{}' not found under {}",
                self.name,
                self.root.display()
            )));
        };
        let mut b = results.get().init_info();
        fill_holon_info(&mut b, &m);
        Promise::ok(())
    }
}

/// Extract the scalar fields from an `InvokeParams` reader.
///
/// Returns `(capability, args_bytes, requester, timeout_ms)`.
fn extract_invoke(
    params: holon::InvokeParams,
) -> Result<(String, Vec<u8>, String, u32), capnp::Error> {
    let reader = params
        .get()
        .map_err(|e| capnp::Error::failed(format!("invoke: failed to read params: {e}")))?;
    let request = reader
        .get_request()
        .map_err(|e| capnp::Error::failed(format!("invoke: missing request: {e}")))?;

    let capability = request
        .get_capability()
        .map_err(|e| capnp::Error::failed(format!("invoke: capability read: {e}")))?
        .to_str()
        .map_err(|e| capnp::Error::failed(format!("invoke: capability UTF-8: {e}")))?
        .to_string();

    let args_bytes: Vec<u8> = request.get_args().map(<[u8]>::to_vec).unwrap_or_default();

    let requester = request
        .get_requester()
        .ok()
        .and_then(|r| r.to_str().ok().map(str::to_string))
        .unwrap_or_else(|| "capnp-client".to_string());

    let timeout_ms: u32 = request.get_timeout_ms();

    Ok((capability, args_bytes, requester, timeout_ms))
}
