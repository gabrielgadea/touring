//! Shared utilities for touring-capnp-server examples.
//!
//! Included by `bench_d34`, `bench_generator_health`, and `client_demo` via
//! `#[path = "util.rs"] mod util;` to eliminate copy-paste across example
//! binaries.  Because the same source is compiled into multiple binaries,
//! some items may be unused in individual binaries — that is expected.
#![allow(dead_code, unused_imports)]

use std::path::Path;
use std::time::Duration;

use capnp::capability::Client;
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use tokio::net::UnixStream;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Retry [`UnixStream::connect`] up to `attempts` times, sleeping `delay`
/// between each attempt.  Returns the connected stream or an error that names
/// the socket path and attempt count.
pub async fn connect_with_retry(
    sock: &Path,
    attempts: usize,
    delay: Duration,
) -> anyhow::Result<UnixStream> {
    let mut last_err: Option<std::io::Error> = None;
    for _ in 0..attempts {
        match UnixStream::connect(sock).await {
            Ok(s) => return Ok(s),
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(delay).await;
            }
        }
    }
    Err(anyhow::anyhow!(
        "connect({}) failed after {} attempts: {}",
        sock.display(),
        attempts,
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown".into())
    ))
}

/// Split `stream`, wrap it in a Cap'n Proto two-party [`RpcSystem`] using
/// `side`, and optionally install a `bootstrap` capability.
///
/// - Pass `bootstrap = None` for a **client** connection.
/// - Pass `Some(cap.client)` for a **server** that serves `cap` as the
///   bootstrap interface.
///
/// The caller is responsible for bootstrapping the desired capability type
/// (`.bootstrap(Side::Server)`) and driving the returned `RpcSystem` as a
/// local task (e.g. `tokio::task::spawn_local(rpc)`).
pub fn make_rpc_system(
    stream: UnixStream,
    side: rpc_twoparty_capnp::Side,
    bootstrap: Option<Client>,
) -> RpcSystem<rpc_twoparty_capnp::Side> {
    let (reader, writer) = stream.into_split();
    let network = twoparty::VatNetwork::new(
        futures::io::BufReader::new(reader.compat()),
        futures::io::BufWriter::new(writer.compat_write()),
        side,
        Default::default(),
    );
    RpcSystem::new(Box::new(network), bootstrap)
}
