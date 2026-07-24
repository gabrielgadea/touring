//! Reusable serve loop for the `HolonRegistry` + `GeneratorHealth` RPCs.
//!
//! Both the standalone `touring-capnp` binary and the in-daemon
//! embedding (`touring-hooks::capnp_embed`, THSF Phase 5 Opt A) funnel
//! through [`serve_with_shutdown`]. Exposing the logic as a library
//! function is what preserves the in-process `touring-core::health_events`
//! broadcast singleton across producer + consumer — see
//! `docs/2026-04-24-thsf-fase5-generator-symbiotic.md`.
//!
//! # Lifecycle
//!
//! The function binds both Unix sockets, spawns two local-task accept
//! loops, races them against the caller-supplied `shutdown` future, and
//! cleans up the socket files before returning. Because `RpcSystem` is
//! `!Send`, callers MUST invoke this from inside a `LocalSet`:
//!
//! ```no_run
//! use std::path::PathBuf;
//! use tokio::task::LocalSet;
//! use touring_bindings::capnp::embed::{serve_with_shutdown, EmbedConfig};
//!
//! # async fn example() {
//! let cfg = EmbedConfig {
//!     socket_path: PathBuf::from("/tmp/reg.sock"),
//!     generator_socket_path: PathBuf::from("/tmp/gen.sock"),
//!     root: PathBuf::from("/home/user"),
//! };
//! let local = LocalSet::new();
//! local
//!     .run_until(async move {
//!         let _ = serve_with_shutdown(cfg, async { /* never */ }).await;
//!     })
//!     .await;
//! # }
//! ```

use std::future::Future;
use std::path::PathBuf;

use anyhow::{Context, Result};
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use tokio::net::UnixListener;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::capnp::holon_core_capnp::holon_registry;
use crate::capnp::holon_generator_capnp::generator_health;
use crate::capnp::{GeneratorHealthImpl, TouringCapnpServer};

/// Configuration surface for the embedded serve loop. Minimal — no env
/// resolution here; callers decide how to populate paths.
#[derive(Debug, Clone)]
pub struct EmbedConfig {
    /// Registry RPC Unix socket path (`HolonRegistry`).
    pub socket_path: PathBuf,
    /// Generator RPC Unix socket path (`GeneratorHealth` — Phase 5).
    pub generator_socket_path: PathBuf,
    /// Holarchy scan root handed to `TouringCapnpServer::new`.
    pub root: PathBuf,
}

/// Serve both RPC interfaces until the `shutdown` future resolves or one
/// of the accept loops errors fatally.
///
/// Both sockets are removed on exit (best-effort). Returning `Ok(())`
/// signals a clean shutdown; `Err(_)` wraps the fatal accept-loop error.
///
/// # Errors
///
/// - Parent directory creation fails (non-existent parent + insufficient
///   permissions).
/// - `UnixListener::bind` fails on either socket (another daemon holds
///   the path).
/// - Accept loop errors propagate from the first failing listener.
pub async fn serve_with_shutdown<S>(cfg: EmbedConfig, shutdown: S) -> Result<()>
where
    S: Future<Output = ()>,
{
    // Ensure parent directory exists (sockets share the parent). Best-effort:
    // failure to create is fatal; failure to remove stale sockets is recovered
    // by the bind below (returns EADDRINUSE anyway).
    if let Some(parent) = cfg.socket_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create_dir_all({})", parent.display()))?;
    }
    for path in [&cfg.socket_path, &cfg.generator_socket_path] {
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    let registry_listener = UnixListener::bind(&cfg.socket_path)
        .with_context(|| format!("UnixListener::bind({})", cfg.socket_path.display()))?;
    let generator_listener = UnixListener::bind(&cfg.generator_socket_path).with_context(|| {
        format!(
            "UnixListener::bind({})",
            cfg.generator_socket_path.display()
        )
    })?;

    eprintln!(
        "[capnp-server] embedded listening registry={} generator={}",
        cfg.socket_path.display(),
        cfg.generator_socket_path.display(),
    );

    let server = TouringCapnpServer::new(cfg.root.clone());

    let registry_accept = async move {
        loop {
            let (stream, _peer) = registry_listener.accept().await?;
            let server = server.clone();
            tokio::task::spawn_local(async move {
                if let Err(e) = serve_registry_connection(stream, server).await {
                    eprintln!("[capnp-server] registry conn error: {e:?}");
                }
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    let generator_accept = async move {
        loop {
            let (stream, _peer) = generator_listener.accept().await?;
            tokio::task::spawn_local(async move {
                if let Err(e) = serve_generator_connection(stream).await {
                    eprintln!("[capnp-server] generator conn error: {e:?}");
                }
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    let outcome = tokio::select! {
        r = registry_accept => r,
        r = generator_accept => r,
        _ = shutdown => {
            eprintln!("[capnp-server] shutdown signal — exiting");
            Ok(())
        }
    };

    let _ = std::fs::remove_file(&cfg.socket_path);
    let _ = std::fs::remove_file(&cfg.generator_socket_path);
    outcome
}

/// Serve a single registry client (`HolonRegistry` bootstrap).
async fn serve_registry_connection(
    stream: tokio::net::UnixStream,
    server: TouringCapnpServer,
) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let reader = reader.compat();
    let writer = writer.compat_write();

    let network = twoparty::VatNetwork::new(
        futures::io::BufReader::new(reader),
        futures::io::BufWriter::new(writer),
        rpc_twoparty_capnp::Side::Server,
        Default::default(),
    );

    let bootstrap: holon_registry::Client = capnp_rpc::new_client(server);
    let rpc_system = RpcSystem::new(Box::new(network), Some(bootstrap.client));
    rpc_system
        .await
        .map_err(|e| anyhow::anyhow!("RpcSystem error: {e}"))
}

/// Serve a single generator client (`GeneratorHealth` bootstrap).
async fn serve_generator_connection(stream: tokio::net::UnixStream) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let reader = reader.compat();
    let writer = writer.compat_write();

    let network = twoparty::VatNetwork::new(
        futures::io::BufReader::new(reader),
        futures::io::BufWriter::new(writer),
        rpc_twoparty_capnp::Side::Server,
        Default::default(),
    );

    let bootstrap: generator_health::Client = capnp_rpc::new_client(GeneratorHealthImpl::new());
    let rpc_system = RpcSystem::new(Box::new(network), Some(bootstrap.client));
    rpc_system
        .await
        .map_err(|e| anyhow::anyhow!("GeneratorHealth RpcSystem error: {e}"))
}
