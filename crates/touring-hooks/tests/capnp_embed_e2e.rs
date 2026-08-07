//! THSF Phase 5 Opt A — E2E test that proves the in-daemon embed path.
//!
//! Spawns `capnp_embed::install` on a temp socket, connects a capnp
//! client, publishes a `HealthDeltaEvent` via `touring_foundation::publish_health_event`
//! in the same process, and asserts the client receives the event
//! through the capnp RPC pipe.
//!
//! This is the crucial test that validates the co-hosted broadcast
//! channel actually works — the E2E tests in `touring-capnp-server`
//! spawn the server directly (bypassing `install`), so they don't
//! exercise the thread-based embed lifecycle.

#![cfg(feature = "capnp-server")]

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use capnp::capability::Promise;
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use tempfile::TempDir;
use tokio::task::LocalSet;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use touring_capnp_server::EmbedConfig;
use touring_capnp_server::holon_generator_capnp::{generator_health, health_delta_listener};
use touring_foundation::{DeltaOutcome, HealthDeltaEvent};

struct Collector {
    events: Vec<(String, f32)>,
}

type Shared = Rc<RefCell<Collector>>;

struct CollectingListener {
    collector: Shared,
}

impl health_delta_listener::Server for CollectingListener {
    #[allow(refining_impl_trait)]
    fn on_delta(
        self: ::capnp::capability::Rc<Self>,
        params: health_delta_listener::OnDeltaParams,
        _results: health_delta_listener::OnDeltaResults,
    ) -> Promise<(), capnp::Error> {
        let reader = match params.get() {
            Ok(r) => r,
            Err(e) => return Promise::err(e),
        };
        let event = match reader.get_event() {
            Ok(e) => e,
            Err(e) => return Promise::err(e),
        };
        let path = event
            .get_file_path()
            .ok()
            .and_then(|t| t.to_str().ok())
            .unwrap_or("")
            .to_string();
        let delta = event.get_delta();
        self.collector.borrow_mut().events.push((path, delta));
        Promise::ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// `install()` guarda EMBED_STATE num OnceLock GLOBAL ao processo: o primeiro a
// chamar vence e liga os sockets no SEU tempdir; o segundo recebe `false`. Em
// paralelo a ordem é indeterminada — e quando o vencedor é `install_is_idempotent`,
// o tempdir dele é destruído no fim do teste, levando junto o diretório onde a
// thread de embed tentava ligar. O próprio código já sinalizava o risco
// ("non-deterministic with respect to test ordering"); reprovou no
// `cargo test --workspace` de 03/08/2026. `#[serial]` torna a ordem determinística:
// quem roda primeiro liga e valida; quem roda depois recebe `false` e sai cedo.
#[serial_test::serial(capnp_embed_state)]
async fn embed_install_serves_capnp_and_delivers_events() {
    let tmp = TempDir::new().expect("tempdir");
    let sock_reg = tmp.path().join("registry.sock");
    let sock_gen = tmp.path().join("generator.sock");

    // --- Install embed thread (this is the path used by run_daemon_async).
    let cfg = EmbedConfig {
        socket_path: sock_reg.clone(),
        generator_socket_path: sock_gen.clone(),
        root: tmp.path().to_path_buf(),
    };
    let installed = touring_hooks::capnp_embed::install(cfg);
    // install() returns false if EMBED_STATE was already initialized
    // (e.g. by install_is_idempotent running in the same process).
    // In that case no embed thread runs and sockets are not created.
    if !installed {
        // Cannot test the embed path when already initialized — test is
        // non-deterministic with respect to test ordering.
        return;
    }

    // Espera o socket APARECER em vez de dormir um orçamento fixo.
    //
    // Eram 100 ms fixos, o que basta numa máquina ociosa e não basta sob
    // `cargo test --workspace` — o teste reprovou em 03/08/2026 com "generator
    // socket should be bound", acusando a thread de embed quando a causa era o
    // relógio. É a mesma correção aplicada ao `PrivateDaemon` nesta sessão:
    // condição, não cronômetro. O teto de 5 s mantém a falha real detectável.
    for _ in 0..100 {
        if sock_gen.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        sock_gen.exists(),
        "generator socket should be bound when install() succeeded (esperado até 5s)"
    );

    // --- Client runs on a LocalSet in this test thread.
    let local = LocalSet::new();
    local
        .run_until(async {
            let stream = tokio::net::UnixStream::connect(&sock_gen)
                .await
                .expect("connect generator.sock");
            let (reader, writer) = stream.into_split();
            let reader = reader.compat();
            let writer = writer.compat_write();
            let network = twoparty::VatNetwork::new(
                futures::io::BufReader::new(reader),
                futures::io::BufWriter::new(writer),
                rpc_twoparty_capnp::Side::Client,
                Default::default(),
            );
            let mut rpc_system = RpcSystem::new(Box::new(network), None);
            let client: generator_health::Client =
                rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);
            let _rpc_task = tokio::task::spawn_local(async move {
                let _ = rpc_system.await;
            });

            // Subscribe.
            let collector: Shared = Rc::new(RefCell::new(Collector { events: vec![] }));
            let listener_client: health_delta_listener::Client =
                capnp_rpc::new_client(CollectingListener {
                    collector: collector.clone(),
                });

            let mut sub = client.subscribe_request();
            {
                let mut p = sub.get();
                p.set_listener(listener_client);
                let mut f = p.init_filter();
                f.set_min_abs_delta(0.0);
                f.set_regressions_only(false);
                let _ = f.init_path_prefixes(0);
            }
            let _reply = sub.send().promise.await.expect("subscribe");

            // Give embed thread time to register the receiver.
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Publish in the test process. touring_foundation::publish_health_event
            // reaches the embed thread because install() spawned it in the
            // same process — the OnceLock broadcast singleton is shared.
            let evt = HealthDeltaEvent {
                file_path: "/embed/test.rs".to_string(),
                old_health: 0.8,
                new_health: 0.9,
                delta: 0.1,
                outcome: DeltaOutcome::Improvement,
                regression_streak: 0,
                improvement_streak: 1,
                timestamp_ms: 12345,
            };
            let _n = touring_foundation::publish_health_event(evt);

            // Poll for delivery (max 500 ms).
            let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
            while tokio::time::Instant::now() < deadline {
                if !collector.borrow().events.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            let got = collector.borrow().events.clone();
            assert!(
                !got.is_empty(),
                "client should receive event via embed thread within 500 ms"
            );
            assert_eq!(got[0].0, "/embed/test.rs");
            assert!((got[0].1 - 0.1).abs() < 1e-6);
        })
        .await;

    // Clean shutdown — notify embed thread + join.
    let stopped = touring_hooks::capnp_embed::shutdown_and_join();
    assert!(stopped, "shutdown_and_join should take the installed state");
    let stopped_again = touring_hooks::capnp_embed::shutdown_and_join();
    assert!(!stopped_again, "second shutdown should be no-op");
}

/// Assert that `install` is idempotent — second call returns false without
/// spawning a new thread. Separate test keeps the first from leaking into
/// this one (EMBED_STATE is singleton; we still call `shutdown_and_join`).
#[test]
#[serial_test::serial(capnp_embed_state)]
fn install_is_idempotent() {
    use tempfile::TempDir;

    let tmp = TempDir::new().expect("tempdir");
    let cfg_a = EmbedConfig {
        socket_path: tmp.path().join("a-reg.sock"),
        generator_socket_path: tmp.path().join("a-gen.sock"),
        root: PathBuf::from("/"),
    };
    let first = touring_hooks::capnp_embed::install(cfg_a);
    // first may be false when the OTHER test already ran on the same
    // process (EMBED_STATE survives across tests in the same binary).
    // We tolerate either outcome; what MUST hold is that the second call
    // returns false.
    let cfg_b = EmbedConfig {
        socket_path: tmp.path().join("b-reg.sock"),
        generator_socket_path: tmp.path().join("b-gen.sock"),
        root: PathBuf::from("/"),
    };
    let second = touring_hooks::capnp_embed::install(cfg_b);
    assert!(
        !second,
        "second install() must be a no-op; got {second} (first={first})"
    );
    // Cleanup.
    let _ = touring_hooks::capnp_embed::shutdown_and_join();
}
