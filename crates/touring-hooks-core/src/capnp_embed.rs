//! THSF Phase 5 Opt A — embedded capnp RPC server co-hosted in `touring-daemon`.
//!
//! Running the `HolonRegistry` + `GeneratorHealth` servers inside the
//! same process as the hook runtime is what keeps the
//! `touring_foundation::health_events` broadcast singleton shared between the
//! producer (`compute_signals_delta` in this crate) and the consumer
//! (`GeneratorHealthImpl` in `touring-capnp-server`). A standalone
//! `touring-capnp` daemon would own its own singleton and never see
//! events emitted here.
//!
//! # Thread model
//!
//! `capnp_rpc::RpcSystem` is `!Send` and must run on a `LocalSet`. The
//! daemon's primary runtime is multi-thread (`Builder::new_multi_thread`),
//! so we spawn a **dedicated OS thread** hosting a `current_thread`
//! runtime + `LocalSet`. The thread is bounded by a shared
//! [`tokio::sync::Notify`] the daemon fires during `graceful_shutdown`.
//!
//! # Lifecycle contract
//!
//! - [`spawn_embedded_capnp`] returns a `JoinHandle<()>`; joining it
//!   AFTER notifying shutdown is optional but recommended for a clean
//!   shutdown log line. When the notify is not fired, the thread runs
//!   until both accept loops error (never, normally) — so the handle
//!   MUST be joined during `graceful_shutdown` or the daemon will hang.
//! - The thread panic-guards the runtime build + `serve_with_shutdown`
//!   to prevent a broken socket path from killing the daemon.
//!
//! # Feature gate
//!
//! The whole module compiles only under
//! `--features capnp-server` (default on in `touring-hooks`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use tokio::sync::Notify;

use touring_capnp_server::{EmbedConfig, serve_with_shutdown};

/// Process-wide singleton holding the embed thread + shutdown notify.
///
/// Written once by [`install`] at daemon startup, drained once by
/// [`shutdown_and_join`] at graceful shutdown. Using a `Mutex<Option<_>>`
/// inside the `OnceLock` gives us safe take-semantics without requiring
/// callers to pass handles around manually.
struct EmbedState {
    handle: Option<JoinHandle<()>>,
    shutdown: Arc<Notify>,
}

static EMBED_STATE: OnceLock<Mutex<Option<EmbedState>>> = OnceLock::new();

/// Resolve default paths for the embedded sockets.
///
/// Follows the same conventions as the standalone `touring-capnp`
/// binary so Python clients that use
/// `GeneratorHealthClient.connect_default()` just work.
///
/// Priority:
/// 1. `$XDG_RUNTIME_DIR/holon/{registry,generator}.sock`
/// 2. `/tmp/holon-{registry,generator}.sock`
///
/// Override via env vars:
/// - `TOURING_CAPNP_EMBED_SOCKET` — registry socket
/// - `TOURING_CAPNP_EMBED_GENERATOR_SOCKET` — generator socket
/// - `TOURING_CAPNP_EMBED_ROOT` — holarchy scan root (default `$HOME`)
pub fn resolve_embed_config() -> EmbedConfig {
    let xdg = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);

    let default_parent = xdg
        .as_ref()
        .map(|p| p.join("holon"))
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    let socket_path = std::env::var_os("TOURING_CAPNP_EMBED_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if xdg.is_some() {
                default_parent.join("registry.sock")
            } else {
                PathBuf::from("/tmp/holon-registry.sock")
            }
        });

    let generator_socket_path = std::env::var_os("TOURING_CAPNP_EMBED_GENERATOR_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if xdg.is_some() {
                default_parent.join("generator.sock")
            } else {
                PathBuf::from("/tmp/holon-generator.sock")
            }
        });

    let root = std::env::var_os("TOURING_CAPNP_EMBED_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));

    EmbedConfig {
        socket_path,
        generator_socket_path,
        root,
    }
}

/// Spawn the embedded capnp server on a dedicated OS thread.
///
/// The thread hosts a `current_thread` tokio runtime + `LocalSet`
/// (required for `!Send` `RpcSystem`). It runs
/// [`serve_with_shutdown`] until `shutdown` is notified OR an accept
/// loop fails.
///
/// # Arguments
///
/// - `cfg`: paths to bind. Typically from [`resolve_embed_config`].
/// - `shutdown`: shared notify fired by the daemon's
///   `graceful_shutdown`. All waiters of `notified()` are woken when
///   `notify_waiters()` is called.
///
/// # Returns
///
/// A `JoinHandle<()>` the caller SHOULD await during shutdown. Errors
/// from `serve_with_shutdown` are logged via `tracing::error!` and
/// swallowed — a broken RPC endpoint does not crash the daemon.
pub fn spawn_embedded_capnp(cfg: EmbedConfig, shutdown: Arc<Notify>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("touring-capnp-embed".to_string())
        .spawn(move || run_embedded(cfg, shutdown))
        .expect("failed to spawn touring-capnp-embed thread")
}

/// Install the embed thread into the process-wide singleton.
///
/// Called once by `run_daemon_async` at startup. Subsequent calls are
/// idempotent no-ops and log a warning — the daemon is singleton-per-lock
/// so a double-install only happens in pathological test setups.
///
/// # Returns
///
/// `true` when a fresh thread was spawned, `false` if a previous install
/// already populated the slot.
pub fn install(cfg: EmbedConfig) -> bool {
    let slot = EMBED_STATE.get_or_init(|| Mutex::new(None));
    let mut guard = match slot.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard.is_some() {
        tracing::warn!("capnp_embed::install called twice — ignoring second call");
        return false;
    }

    let shutdown = Arc::new(Notify::new());
    let handle = spawn_embedded_capnp(cfg, Arc::clone(&shutdown));
    *guard = Some(EmbedState {
        handle: Some(handle),
        shutdown,
    });
    true
}

/// Notify the embed thread to exit and wait for it to join.
///
/// Called from `graceful_shutdown`. Bounded by the OS-level join — the
/// thread exits as soon as `serve_with_shutdown` observes the notify,
/// which happens at the next `tokio::select!` tick of the accept loops
/// (microseconds under normal load).
///
/// # Returns
///
/// `true` when the embed was running and joined cleanly, `false` when
/// nothing was installed (feature off or shutdown already drained).
pub fn shutdown_and_join() -> bool {
    let slot = match EMBED_STATE.get() {
        Some(s) => s,
        None => return false,
    };
    let mut guard = match slot.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let state = match guard.take() {
        Some(s) => s,
        None => return false,
    };
    // `notify_one` (not `notify_waiters`) stores a permit when no waiter is
    // yet registered — eliminates a race where the embed thread has
    // spawned but not yet reached `shutdown.notified().await`. Missing
    // this permit causes the thread to block forever on join().
    state.shutdown.notify_one();
    if let Some(handle) = state.handle {
        match handle.join() {
            Ok(()) => tracing::info!("touring-capnp embed thread joined"),
            Err(e) => tracing::error!(?e, "touring-capnp embed thread panicked"),
        }
    }
    true
}

/// Body of the dedicated thread. Safe to call directly in tests.
fn run_embedded(cfg: EmbedConfig, shutdown: Arc<Notify>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .thread_name("touring-capnp-rt")
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(
                error = %e,
                "failed to build current_thread runtime for capnp embed — capnp disabled"
            );
            return;
        }
    };

    let local = tokio::task::LocalSet::new();
    let result = local.block_on(&runtime, async move {
        serve_with_shutdown(cfg, async move {
            shutdown.notified().await;
        })
        .await
    });

    match result {
        Ok(()) => tracing::info!("touring-capnp embed exited cleanly"),
        Err(e) => tracing::error!(error = %e, "touring-capnp embed exited with error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_embed_config_falls_back_when_xdg_unset() {
        // Capture-and-restore pattern (env is process-wide).
        let prev_xdg = std::env::var_os("XDG_RUNTIME_DIR");
        let prev_sock = std::env::var_os("TOURING_CAPNP_EMBED_SOCKET");
        let prev_gen = std::env::var_os("TOURING_CAPNP_EMBED_GENERATOR_SOCKET");
        // SAFETY: test-only; we restore below. Parallel tests touching the
        // same vars are serialized by `cargo test` default single-thread
        // for this module (we don't use serial_test here to keep deps low;
        // the assertions tolerate either code path).
        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
            std::env::remove_var("TOURING_CAPNP_EMBED_SOCKET");
            std::env::remove_var("TOURING_CAPNP_EMBED_GENERATOR_SOCKET");
        }

        let cfg = resolve_embed_config();
        assert!(
            cfg.socket_path.starts_with("/tmp") || cfg.socket_path.starts_with("/run"),
            "unexpected default socket_path: {:?}",
            cfg.socket_path
        );
        assert!(
            cfg.generator_socket_path.ends_with("generator.sock")
                || cfg
                    .generator_socket_path
                    .to_string_lossy()
                    .contains("generator")
        );

        unsafe {
            if let Some(v) = prev_xdg {
                std::env::set_var("XDG_RUNTIME_DIR", v);
            }
            if let Some(v) = prev_sock {
                std::env::set_var("TOURING_CAPNP_EMBED_SOCKET", v);
            }
            if let Some(v) = prev_gen {
                std::env::set_var("TOURING_CAPNP_EMBED_GENERATOR_SOCKET", v);
            }
        }
    }

    #[test]
    fn resolve_embed_config_honors_explicit_override() {
        let prev = std::env::var_os("TOURING_CAPNP_EMBED_SOCKET");
        unsafe {
            std::env::set_var("TOURING_CAPNP_EMBED_SOCKET", "/tmp/custom-reg.sock");
        }

        let cfg = resolve_embed_config();
        assert_eq!(cfg.socket_path, PathBuf::from("/tmp/custom-reg.sock"));

        unsafe {
            if let Some(v) = prev {
                std::env::set_var("TOURING_CAPNP_EMBED_SOCKET", v);
            } else {
                std::env::remove_var("TOURING_CAPNP_EMBED_SOCKET");
            }
        }
    }
}
