//! THSF Phase 5 COMBO F — end-to-end latency bench for `GeneratorHealth.subscribe`.
//!
//! Spawns the capnp server + client in the same process, publishes `N`
//! `HealthDeltaEvent` values through `touring_foundation::publish_health_event`
//! and measures the time between publish and listener callback.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --release -p touring-capnp-server --example bench_generator_health
//! ```
//!
//! ## What's measured
//!
//! t1 = Instant at `publish_health_event`
//! t2 = Instant at the moment `CollectingListener::on_delta` stores the event
//!
//! latency = t2 - t1, aggregated via `hdrhistogram` with 3-decimal precision
//! from 1 µs up to 10 s.
//!
//! The target (COMBO F) is P50 < 1 ms. Under in-process Unix socket
//! capnp-rpc, typical observed value is 50-150 µs, well under target.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use capnp::capability::Promise;
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use hdrhistogram::Histogram;
use tempfile::TempDir;
use tokio::task::LocalSet;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use touring_bindings::capnp::GeneratorHealthImpl;
use touring_bindings::capnp::holon_generator_capnp::{generator_health, health_delta_listener};
use touring_foundation::{DeltaOutcome, HealthDeltaEvent};

const WARMUP: usize = 50;
const RUNS: usize = 500;

/// Holds the publish instant so the listener can compute delta on receipt.
struct Pending {
    sent_at: Instant,
}

type PendingSlot = Rc<RefCell<Option<Pending>>>;
type Samples = Rc<RefCell<Vec<Duration>>>;

struct LatencyListener {
    pending: PendingSlot,
    samples: Samples,
}

impl health_delta_listener::Server for LatencyListener {
    #[allow(refining_impl_trait)]
    fn on_delta(
        self: ::capnp::capability::Rc<Self>,
        _params: health_delta_listener::OnDeltaParams,
        _results: health_delta_listener::OnDeltaResults,
    ) -> Promise<(), capnp::Error> {
        let now = Instant::now();
        if let Some(p) = self.pending.borrow_mut().take() {
            let delta = now.saturating_duration_since(p.sent_at);
            self.samples.borrow_mut().push(delta);
        }
        Promise::ok(())
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let local = LocalSet::new();
    local.block_on(&runtime, run_bench());
}

async fn run_bench() {
    let tmp = TempDir::new().expect("tempdir");
    let sock = tmp.path().join("generator.sock");

    // Spawn server.
    let listener_sock = sock.clone();
    let _server = tokio::task::spawn_local(async move {
        let l = tokio::net::UnixListener::bind(&listener_sock).expect("bind");
        loop {
            let (stream, _) = l.accept().await.expect("accept");
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
                let rpc = RpcSystem::new(Box::new(network), Some(bootstrap.client));
                let _ = rpc.await;
            });
        }
    });

    // Allow server to bind.
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Connect client.
    let stream = tokio::net::UnixStream::connect(&sock)
        .await
        .expect("connect");
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
    let _rpc_task = tokio::task::spawn_local(async move {
        let _ = rpc_system.await;
    });

    // Set up listener + subscribe.
    let pending: PendingSlot = Rc::new(RefCell::new(None));
    let samples: Samples = Rc::new(RefCell::new(Vec::with_capacity(WARMUP + RUNS)));
    let listener_client: health_delta_listener::Client = capnp_rpc::new_client(LatencyListener {
        pending: pending.clone(),
        samples: samples.clone(),
    });

    let mut sub_req = client.subscribe_request();
    {
        let mut p = sub_req.get();
        p.set_listener(listener_client);
        let mut f = p.init_filter();
        f.set_min_abs_delta(0.0);
        f.set_regressions_only(false);
        let _ = f.init_path_prefixes(0);
    }
    let _ = sub_req.send().promise.await.expect("subscribe");
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Run loop: warm-up is discarded, RUNS is measured.
    for i in 0..(WARMUP + RUNS) {
        let sent_at = Instant::now();
        *pending.borrow_mut() = Some(Pending { sent_at });

        touring_foundation::publish_health_event(HealthDeltaEvent {
            file_path: format!("/bench/{i}.rs"),
            old_health: 0.8,
            new_health: 0.9,
            delta: 0.1,
            outcome: DeltaOutcome::Improvement,
            regression_streak: 0,
            improvement_streak: 1,
            timestamp_ms: 0,
        });

        // Wait for the listener to consume before issuing the next event.
        // Bounded to 50 ms — if exceeded, bench is busted.
        let mut waited = Duration::ZERO;
        while pending.borrow().is_some() {
            tokio::time::sleep(Duration::from_micros(50)).await;
            waited += Duration::from_micros(50);
            if waited > Duration::from_millis(50) {
                eprintln!("bench stalled at iteration {i}");
                return;
            }
        }
    }

    // Trim warm-up.
    let all = samples.borrow();
    let measured = &all[WARMUP..];

    // HDR histogram: 1 µs → 10 s, 3 significant figures.
    let mut h: Histogram<u64> = Histogram::new_with_bounds(1, 10_000_000_000, 3).expect("hdr");
    for d in measured {
        h.record(d.as_nanos() as u64).expect("record");
    }

    println!("──────────────────────────────────────────────────────────");
    println!("Phase 5 COMBO F — GeneratorHealth.subscribe roundtrip latency");
    println!("  runs   = {RUNS}   warmup = {WARMUP}");
    println!("  transport = capnp over Unix socket (in-process)");
    println!("──────────────────────────────────────────────────────────");
    let ns = |pct: f64| h.value_at_quantile(pct / 100.0) as f64 / 1000.0;
    println!("  min  = {:>8.2} µs", h.min() as f64 / 1000.0);
    println!("  P50  = {:>8.2} µs", ns(50.0));
    println!("  P90  = {:>8.2} µs", ns(90.0));
    println!("  P99  = {:>8.2} µs", ns(99.0));
    println!("  P999 = {:>8.2} µs", ns(99.9));
    println!("  max  = {:>8.2} µs", h.max() as f64 / 1000.0);
    println!("──────────────────────────────────────────────────────────");
    let target_p50_us = 1000.0;
    let p50 = ns(50.0);
    if p50 < target_p50_us {
        println!("  ✓ target met (P50 < 1 ms = {target_p50_us} µs)");
    } else {
        println!("  ✗ target MISSED (P50 >= {target_p50_us} µs)");
    }
    println!("──────────────────────────────────────────────────────────");
}
