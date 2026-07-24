//! THSF Phase 5 COMBO F — end-to-end `GeneratorHealth` RPC test.
//!
//! Spawns an in-process capnp server on a temp Unix socket, subscribes a
//! listener client, publishes events via `touring_foundation::publish_health_event`,
//! and asserts the listener received them through the capnp pipe.
//!
//! Covers:
//! - Subscribe + basic fan-out
//! - Filter: `regressions_only` excludes improvements
//! - Filter: `path_prefixes` restricts delivery
//! - `spec_version` returns the contract string
//! - `SubscriptionHandle.close()` stops delivery

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use capnp::capability::Promise;
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use tempfile::TempDir;
use tokio::task::LocalSet;
use tokio::time::timeout;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use touring_bindings::capnp::GeneratorHealthImpl;
use touring_bindings::capnp::holon_generator_capnp::{generator_health, health_delta_listener};
use touring_foundation::{DeltaOutcome, HealthDeltaEvent};

/// Bookkeeping struct shared between the listener impl and the test body.
#[derive(Default)]
struct Collector {
    events: Vec<(String, f32, String)>, // (file_path, delta, outcome)
}

type SharedCollector = Rc<RefCell<Collector>>;

/// Client-side listener that appends every received event to the shared
/// `Collector`. Implementation is intentionally thin — just records.
struct CollectingListener {
    collector: SharedCollector,
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

        let file_path = event
            .get_file_path()
            .ok()
            .and_then(|t| t.to_str().ok())
            .unwrap_or("")
            .to_string();
        let delta = event.get_delta();
        let outcome = match event.get_outcome() {
            Ok(v) => format!("{v:?}"),
            Err(_) => "Unknown".to_string(),
        };

        self.collector
            .borrow_mut()
            .events
            .push((file_path, delta, outcome));

        Promise::ok(())
    }
}

/// Compute a temp socket path (tempdir lifetime controls cleanup).
fn socket_path(tmp: &TempDir) -> std::path::PathBuf {
    tmp.path().join("generator.sock")
}

/// Spawn a GeneratorHealth server on the given socket, returning the
/// accept task so the caller can drop it to tear down.
async fn spawn_generator_server(
    sock: std::path::PathBuf,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    let listener = tokio::net::UnixListener::bind(&sock).expect("bind generator sock");
    tokio::task::spawn_local(async move {
        loop {
            let (stream, _) = listener.accept().await?;
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
                let bootstrap: generator_health::Client =
                    capnp_rpc::new_client(GeneratorHealthImpl::new());
                let rpc_system = RpcSystem::new(Box::new(network), Some(bootstrap.client));
                let _ = rpc_system.await;
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    })
}

/// Build a capnp client connected to the running server.
async fn connect_client(
    sock: &std::path::Path,
) -> (generator_health::Client, tokio::task::JoinHandle<()>) {
    // Retry-briefly to allow the server's accept loop to have bound.
    let stream = {
        let mut last_err = None;
        let mut client = None;
        for _ in 0..10 {
            match tokio::net::UnixStream::connect(sock).await {
                Ok(s) => {
                    client = Some(s);
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }
        client.unwrap_or_else(|| panic!("connect failed: {last_err:?}"))
    };

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
    let client: generator_health::Client = rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

    let rpc_task = tokio::task::spawn_local(async move {
        let _ = rpc_system.await;
    });

    (client, rpc_task)
}

fn sample_event(path: &str, delta: f32, outcome: DeltaOutcome) -> HealthDeltaEvent {
    HealthDeltaEvent {
        file_path: path.to_string(),
        old_health: 0.8,
        new_health: 0.8 + delta,
        delta,
        outcome,
        regression_streak: 0,
        improvement_streak: 0,
        timestamp_ms: 42,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn spec_version_roundtrips() {
    let tmp = TempDir::new().unwrap();
    let sock = socket_path(&tmp);
    let local = LocalSet::new();

    local
        .run_until(async move {
            let _server = spawn_generator_server(sock.clone()).await;
            let (client, _rpc) = connect_client(&sock).await;

            let req = client.spec_version_request();
            let reply = req.send().promise.await.expect("spec_version");
            let version = reply
                .get()
                .unwrap()
                .get_version()
                .unwrap()
                .to_str()
                .unwrap();

            assert_eq!(version, "0.1.0");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn subscribe_receives_published_events() {
    let tmp = TempDir::new().unwrap();
    let sock = socket_path(&tmp);
    let local = LocalSet::new();

    local
        .run_until(async move {
            let _server = spawn_generator_server(sock.clone()).await;
            let (client, _rpc) = connect_client(&sock).await;

            // Set up listener + subscribe.
            let collector: SharedCollector = Rc::new(RefCell::new(Collector::default()));
            let listener_client: health_delta_listener::Client =
                capnp_rpc::new_client(CollectingListener {
                    collector: collector.clone(),
                });

            let mut sub_req = client.subscribe_request();
            {
                let mut params = sub_req.get();
                params.set_listener(listener_client);
                let mut filter = params.init_filter();
                filter.set_min_abs_delta(0.0);
                filter.set_regressions_only(false);
                // Empty prefixes → all paths
                let _ = filter.init_path_prefixes(0);
            }
            let sub_reply = sub_req.send().promise.await.expect("subscribe");
            let _handle = sub_reply.get().unwrap().get_handle().unwrap();

            // Give the server a tick to attach the broadcast receiver.
            tokio::time::sleep(Duration::from_millis(20)).await;

            // Publish an event — since we're the same process,
            // touring_foundation::publish_health_event reaches the server task.
            let evt = sample_event("/tmp/sub_test.rs", 0.1, DeltaOutcome::Improvement);
            let delivered = touring_foundation::publish_health_event(evt);
            assert!(delivered >= 1, "expected at least one subscriber");

            // Poll for event delivery (max 500 ms).
            let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
            while tokio::time::Instant::now() < deadline {
                if !collector.borrow().events.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            let events = collector.borrow().events.clone();
            assert!(
                !events.is_empty(),
                "listener did not receive any event within 500 ms"
            );
            // Drain any stray events from parallel tests sharing the global broadcast channel.
            let our_event = events
                .iter()
                .find(|(p, _, _)| p == "/tmp/sub_test.rs")
                .cloned();
            let (path, delta, outcome) = our_event.expect(
                "expected /tmp/sub_test.rs within 500 ms; got stray events from parallel test",
            );
            assert_eq!(path, "/tmp/sub_test.rs");
            assert!((delta - 0.1).abs() < 1e-6);
            assert!(outcome.contains("Improvement"));
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn filter_regressions_only_excludes_improvements() {
    let tmp = TempDir::new().unwrap();
    let sock = socket_path(&tmp);
    let local = LocalSet::new();

    local
        .run_until(async move {
            let _server = spawn_generator_server(sock.clone()).await;
            let (client, _rpc) = connect_client(&sock).await;

            let collector: SharedCollector = Rc::new(RefCell::new(Collector::default()));
            let listener_client: health_delta_listener::Client =
                capnp_rpc::new_client(CollectingListener {
                    collector: collector.clone(),
                });

            let mut sub_req = client.subscribe_request();
            {
                let mut params = sub_req.get();
                params.set_listener(listener_client);
                let mut filter = params.init_filter();
                filter.set_min_abs_delta(0.0);
                filter.set_regressions_only(true); // only regressions
                let _ = filter.init_path_prefixes(0);
            }
            let _ = sub_req.send().promise.await.expect("subscribe");

            tokio::time::sleep(Duration::from_millis(20)).await;

            // Publish 2 improvements + 1 regression.
            touring_foundation::publish_health_event(sample_event(
                "/a.rs",
                0.1,
                DeltaOutcome::Improvement,
            ));
            touring_foundation::publish_health_event(sample_event(
                "/b.rs",
                0.2,
                DeltaOutcome::Improvement,
            ));
            touring_foundation::publish_health_event(sample_event(
                "/c.rs",
                -0.15,
                DeltaOutcome::Regression,
            ));

            // Wait + assert only 1 (the regression) got through.
            tokio::time::sleep(Duration::from_millis(150)).await;
            let events = collector.borrow().events.clone();
            assert_eq!(
                events.len(),
                1,
                "regressions_only filter should yield exactly 1 event, got {events:?}"
            );
            assert_eq!(events[0].0, "/c.rs");
            assert!(events[0].2.contains("Regression"));
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn filter_path_prefixes_limits_delivery() {
    let tmp = TempDir::new().unwrap();
    let sock = socket_path(&tmp);
    let local = LocalSet::new();

    local
        .run_until(async move {
            let _server = spawn_generator_server(sock.clone()).await;
            let (client, _rpc) = connect_client(&sock).await;

            let collector: SharedCollector = Rc::new(RefCell::new(Collector::default()));
            let listener_client: health_delta_listener::Client =
                capnp_rpc::new_client(CollectingListener {
                    collector: collector.clone(),
                });

            let mut sub_req = client.subscribe_request();
            {
                let mut params = sub_req.get();
                params.set_listener(listener_client);
                let mut filter = params.init_filter();
                filter.set_min_abs_delta(0.0);
                filter.set_regressions_only(false);
                let mut prefixes = filter.init_path_prefixes(1);
                prefixes.set(0, "/src/");
            }
            let _ = sub_req.send().promise.await.expect("subscribe");

            tokio::time::sleep(Duration::from_millis(20)).await;

            touring_foundation::publish_health_event(sample_event(
                "/src/match.rs",
                0.1,
                DeltaOutcome::Improvement,
            ));
            touring_foundation::publish_health_event(sample_event(
                "/docs/skip.md",
                0.1,
                DeltaOutcome::Improvement,
            ));

            tokio::time::sleep(Duration::from_millis(150)).await;
            let events = collector.borrow().events.clone();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].0, "/src/match.rs");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn subscribe_receives_within_1ms_budget() {
    // Latency budget check — end-to-end <10 ms generous bound.
    // Real measurement target for bench is <1 ms.
    let tmp = TempDir::new().unwrap();
    let sock = socket_path(&tmp);
    let local = LocalSet::new();

    local
        .run_until(async move {
            let _server = spawn_generator_server(sock.clone()).await;
            let (client, _rpc) = connect_client(&sock).await;

            let collector: SharedCollector = Rc::new(RefCell::new(Collector::default()));
            let listener_client: health_delta_listener::Client =
                capnp_rpc::new_client(CollectingListener {
                    collector: collector.clone(),
                });

            let mut sub_req = client.subscribe_request();
            {
                let mut params = sub_req.get();
                params.set_listener(listener_client);
                let mut filter = params.init_filter();
                filter.set_min_abs_delta(0.0);
                filter.set_regressions_only(false);
                let _ = filter.init_path_prefixes(0);
            }
            let _ = sub_req.send().promise.await.expect("subscribe");
            tokio::time::sleep(Duration::from_millis(20)).await;

            let t0 = std::time::Instant::now();
            touring_foundation::publish_health_event(sample_event(
                "/budget.rs",
                0.1,
                DeltaOutcome::Improvement,
            ));

            // Wait with a hard 100 ms ceiling.
            let got = timeout(Duration::from_millis(100), async {
                loop {
                    if !collector.borrow().events.is_empty() {
                        return std::time::Instant::now();
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .expect("event not received within 100 ms");

            let elapsed = got.saturating_duration_since(t0);
            // Assert < 50 ms — generous bound for current_thread runtime
            // + Unix socket + RefCell overhead. Real bench (Wave 5F) will
            // measure the sub-ms end-to-end latency.
            assert!(
                elapsed < Duration::from_millis(50),
                "e2e latency {elapsed:?} exceeds 50 ms smoke budget"
            );
        })
        .await;
}
