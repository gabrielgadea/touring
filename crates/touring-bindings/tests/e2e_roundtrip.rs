//! End-to-end RPC roundtrip — spawns the `HolonRegistry` server on an
//! in-process LocalSet, connects a cliente via UnixStream, and exercises
//! `list_holons` / `find_by_capability` / `get_holon` / `spec_version`.
//!
//! Does NOT exercise `Holon.invoke`, which requires the `holon` CLI
//! binary on `$PATH`. That path is covered by Fase 3 D3.3 clientes
//! Python + D3.4 benchmark.

use std::fs;
use std::path::PathBuf;

use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use tempfile::TempDir;
use tokio::task::LocalSet;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use touring_bindings::capnp::holon_core_capnp::holon_registry;
use touring_bindings::capnp::{SPEC_VERSION, TouringCapnpServer};

const MINIMAL_MANIFEST: &str = r#"
[holon.identity]
name = "demo-holon"
version = "1.0.0"
description = "integration test fixture"
autonomy_guarantee = true

[holon.offers.ping]
adapter = "cli"
adapter_cmd = "echo pong"
version = "1.0.0"
"#;

fn setup_holarchy(root: &std::path::Path) {
    let dir = root.join("demo");
    fs::create_dir_all(dir.join(".holon")).unwrap();
    fs::write(dir.join(".holon").join("manifest.toml"), MINIMAL_MANIFEST).unwrap();
}

/// Bind an abstract-like Unix socket under `tmp` and return its path.
fn socket_path(tmp: &TempDir) -> PathBuf {
    tmp.path().join("registry.sock")
}

/// Spawn the server on a [`LocalSet`]; return the spawned task.
async fn spawn_server(sock: PathBuf, root: PathBuf) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    let listener = tokio::net::UnixListener::bind(&sock).unwrap();
    let server = TouringCapnpServer::new(root);
    tokio::task::spawn_local(async move {
        loop {
            let (stream, _) = listener.accept().await?;
            let server = server.clone();
            tokio::task::spawn_local(async move {
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
                let rpc = RpcSystem::new(Box::new(network), Some(bootstrap.client));
                let _ = rpc.await;
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    })
}

/// Connect a client and return the bootstrapped [`holon_registry::Client`].
async fn connect(
    sock: &std::path::Path,
) -> (
    holon_registry::Client,
    tokio::task::JoinHandle<Result<(), capnp::Error>>,
) {
    let stream = tokio::net::UnixStream::connect(sock).await.unwrap();
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
    let driver = tokio::task::spawn_local(rpc);
    (registry, driver)
}

#[tokio::test(flavor = "current_thread")]
async fn spec_version_returns_compile_constant() {
    let local = LocalSet::new();
    local
        .run_until(async {
            let tmp = TempDir::new().unwrap();
            let sock = socket_path(&tmp);
            let _srv = spawn_server(sock.clone(), tmp.path().into()).await;

            // Give the listener a chance to start
            tokio::task::yield_now().await;

            let (registry, _rpc_driver) = connect(&sock).await;
            let req = registry.spec_version_request();
            let response = req.send().promise.await.unwrap();
            let version = response
                .get()
                .unwrap()
                .get_version()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            assert_eq!(version, SPEC_VERSION);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn list_holons_finds_demo_manifest() {
    let local = LocalSet::new();
    local
        .run_until(async {
            let tmp = TempDir::new().unwrap();
            setup_holarchy(tmp.path());
            let sock = socket_path(&tmp);
            let _srv = spawn_server(sock.clone(), tmp.path().into()).await;
            tokio::task::yield_now().await;

            let (registry, _driver) = connect(&sock).await;
            let response = registry.list_holons_request().send().promise.await.unwrap();
            let holons = response.get().unwrap().get_holons().unwrap();
            assert_eq!(holons.len(), 1, "expected 1 holon");
            let first = holons.get(0);
            let name = first.get_name().unwrap().to_str().unwrap().to_string();
            assert_eq!(name, "demo-holon");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn find_by_capability_returns_matching_holon() {
    let local = LocalSet::new();
    local
        .run_until(async {
            let tmp = TempDir::new().unwrap();
            setup_holarchy(tmp.path());
            let sock = socket_path(&tmp);
            let _srv = spawn_server(sock.clone(), tmp.path().into()).await;
            tokio::task::yield_now().await;

            let (registry, _driver) = connect(&sock).await;
            let mut req = registry.find_by_capability_request();
            req.get().set_capability("ping");
            let response = req.send().promise.await.unwrap();
            let holons = response.get().unwrap().get_holons().unwrap();
            assert_eq!(holons.len(), 1);

            // Non-matching capability returns zero.
            let mut miss_req = registry.find_by_capability_request();
            miss_req.get().set_capability("nonexistent");
            let miss = miss_req.send().promise.await.unwrap();
            assert_eq!(miss.get().unwrap().get_holons().unwrap().len(), 0);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn get_holon_returns_capability_and_info_works() {
    let local = LocalSet::new();
    local
        .run_until(async {
            let tmp = TempDir::new().unwrap();
            setup_holarchy(tmp.path());
            let sock = socket_path(&tmp);
            let _srv = spawn_server(sock.clone(), tmp.path().into()).await;
            tokio::task::yield_now().await;

            let (registry, _driver) = connect(&sock).await;

            // Promise pipelining: get_holon then info on returned cap.
            let mut get_req = registry.get_holon_request();
            get_req.get().set_name("demo-holon");
            let holon_client = get_req.send().pipeline.get_holon();

            let info_resp = holon_client.info_request().send().promise.await.unwrap();
            let info = info_resp.get().unwrap().get_info().unwrap();
            let name = info.get_name().unwrap().to_str().unwrap().to_string();
            assert_eq!(name, "demo-holon");
            assert_eq!(info.get_version().unwrap().to_str().unwrap(), "1.0.0");
            assert!(info.get_autonomy_guarantee());
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn get_holon_rejects_unknown_name() {
    let local = LocalSet::new();
    local
        .run_until(async {
            let tmp = TempDir::new().unwrap();
            setup_holarchy(tmp.path());
            let sock = socket_path(&tmp);
            let _srv = spawn_server(sock.clone(), tmp.path().into()).await;
            tokio::task::yield_now().await;

            let (registry, _driver) = connect(&sock).await;
            let mut req = registry.get_holon_request();
            req.get().set_name("no-such-holon");
            let result = req.send().promise.await;
            assert!(
                result.is_err(),
                "expected error for unknown holon, got Ok response"
            );
        })
        .await;
}
