//! `touring-capnp` — standalone daemon serving `HolonRegistry` +
//! `GeneratorHealth` RPC.
//!
//! Since THSF Phase 5 Opt A (2026-04-24), the preferred deployment is
//! to embed this server inside `touring-daemon` via the
//! `touring-hooks/capnp-server` feature — that setup keeps the
//! `touring-core::health_events` broadcast channel in-process across
//! producer and consumer. This standalone binary remains available for
//! development, tests, and environments where `touring-daemon` is not
//! running (counter fetches + registry work; live `subscribe` delivery
//! will be empty, which is expected).
//!
//! # Usage
//!
//! ```bash
//! touring-capnp                                  # default XDG sockets
//! TOURING_CAPNP_SOCKET=/tmp/reg.sock touring-capnp
//! TOURING_CAPNP_GENERATOR_SOCKET=/tmp/gen.sock touring-capnp
//! touring-capnp --print-config                   # dump resolved config
//! ```
//!
//! # Graceful shutdown
//!
//! Ctrl+C triggers `tokio::signal::ctrl_c()`, which is the `shutdown`
//! future passed to [`serve_with_shutdown`]. Socket files are unlinked
//! on exit.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::task::LocalSet;

use touring_bindings::capnp::{default_socket_path, serve_with_shutdown, EmbedConfig, SPEC_VERSION};

/// Resolved runtime configuration produced from env + defaults.
/// Kept separate from [`EmbedConfig`] because the library type has no
/// opinion on env-var naming or defaults — that policy lives here.
#[derive(Debug)]
struct BinConfig {
    spec_version: String,
    socket_path: PathBuf,
    generator_socket_path: PathBuf,
    root: PathBuf,
}

impl BinConfig {
    fn from_env() -> Self {
        let socket_path = std::env::var_os("TOURING_CAPNP_SOCKET")
            .map(PathBuf::from)
            .or_else(default_socket_path)
            .unwrap_or_else(|| PathBuf::from("/tmp/holon-registry.sock"));

        let generator_socket_path = std::env::var_os("TOURING_CAPNP_GENERATOR_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|| default_generator_socket_path(&socket_path));

        let root = std::env::var_os("TOURING_CAPNP_ROOT")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));

        Self {
            spec_version: SPEC_VERSION.to_string(),
            socket_path,
            generator_socket_path,
            root,
        }
    }

    fn to_json(&self) -> String {
        format!(
            r#"{{"spec_version":"{}","socket_path":"{}","generator_socket_path":"{}","root":"{}"}}"#,
            self.spec_version,
            self.socket_path.display(),
            self.generator_socket_path.display(),
            self.root.display()
        )
    }

    fn into_embed_config(self) -> EmbedConfig {
        EmbedConfig {
            socket_path: self.socket_path,
            generator_socket_path: self.generator_socket_path,
            root: self.root,
        }
    }
}

/// Default `generator.sock` path derived from the registry socket parent.
fn default_generator_socket_path(registry: &Path) -> PathBuf {
    registry.with_file_name("generator.sock")
}

fn main() -> Result<()> {
    let cfg = BinConfig::from_env();

    if std::env::args().any(|a| a == "--print-config") {
        println!("{}", cfg.to_json());
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;
    let local = LocalSet::new();
    let embed_cfg = cfg.into_embed_config();
    local.block_on(&runtime, async move {
        serve_with_shutdown(embed_cfg, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
    })
}
