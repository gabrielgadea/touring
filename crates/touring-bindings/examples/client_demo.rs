//! Example client — exercises the exact snippet documented in the
//! `touring-capnp-server` crate README/`lib.rs` usage block:
//!
//! ```rust,no_run
//! let stream = tokio::net::UnixStream::connect(sock).await?;
//! let (r, w) = stream.into_split();
//! let network = twoparty::VatNetwork::new(
//!     r.compat(), w.compat_write(), Side::Client, Default::default());
//! let mut rpc = RpcSystem::new(Box::new(network), None);
//! let registry: holon_registry::Client = rpc.bootstrap(Side::Server);
//! ```
//!
//! Beyond the snippet, this example actually drives two RPC methods
//! (`spec_version`, `list_holons`) and prints a single-line JSON
//! summary. It's used by the E2E scripts to prove the release binary
//! daemon can be consumed from a separate process.
//!
//! # Usage
//!
//! ```bash
//! # Start the daemon in another shell first.
//! TOURING_CAPNP_SOCKET=/tmp/holon-demo.sock \
//! TOURING_CAPNP_ROOT=/tmp/empty-holarchy \
//! ./target/release/touring-capnp &
//!
//! ./target/release/examples/client_demo /tmp/holon-demo.sock
//! ```
//!
//! Exit codes: 0 success; 1 connect/RPC error; 2 bad args.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use tokio::net::UnixStream;
use tokio::task::LocalSet;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use touring_bindings::capnp::holon_core_capnp::holon_registry;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let sock = match parse_args() {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("usage: client_demo <unix-socket-path>\nerror: {msg}");
            return std::process::ExitCode::from(2);
        }
    };

    let local = LocalSet::new();
    let result = local.run_until(async move { run_demo(&sock).await }).await;

    match result {
        Ok(summary) => {
            println!("{summary}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("client_demo FAILED: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

fn parse_args() -> Result<PathBuf, String> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(format!(
            "expected 1 argument (socket path), got {}",
            args.len() - 1
        ));
    }
    Ok(PathBuf::from(&args[1]))
}

/// Connect + drive 2 RPCs + return one-line JSON summary.
async fn run_demo(sock: &std::path::Path) -> anyhow::Result<String> {
    // Small retry loop — daemon bind may race with our start-up.
    let stream = connect_with_retry(sock, 10, Duration::from_millis(100)).await?;

    let (reader, writer) = stream.into_split();
    let reader = reader.compat();
    let writer = writer.compat_write();

    let network = twoparty::VatNetwork::new(
        futures::io::BufReader::new(reader),
        futures::io::BufWriter::new(writer),
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    );
    let mut rpc = RpcSystem::new(Box::new(network), None);
    let registry: holon_registry::Client = rpc.bootstrap(rpc_twoparty_capnp::Side::Server);

    // Drive the RPC system in the background (needed for awaiting replies).
    let _driver = tokio::task::spawn_local(rpc);

    // 1. spec_version round-trip
    let spec_reply = registry.spec_version_request().send().promise.await?;
    let spec = spec_reply.get()?.get_version()?.to_str()?.to_string();

    // 2. list_holons round-trip
    let list_reply = registry.list_holons_request().send().promise.await?;
    let holons = list_reply.get()?.get_holons()?;
    let names: Vec<String> = (0..holons.len())
        .map(|i| match holons.get(i).get_name() {
            Ok(reader) => reader
                .to_str()
                .map(str::to_string)
                .unwrap_or_else(|_| "<?>".to_string()),
            Err(_) => "<?>".to_string(),
        })
        .collect();

    // Emit single-line JSON for easy shell parsing.
    let summary = format!(
        r#"{{"spec_version":"{}","holons_count":{},"holons":{}}}"#,
        spec,
        holons.len(),
        serde_json::to_string(&names).unwrap_or_else(|_| "[]".into())
    );
    Ok(summary)
}

/// Retry `UnixStream::connect` up to `attempts` times with `delay`.
async fn connect_with_retry(
    sock: &std::path::Path,
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
