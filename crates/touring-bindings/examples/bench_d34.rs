//! THSF Fase 3 D3.4 — Rust runner for the latency benchmark.
//!
//! Measures the **protocol floor** (no Python binding overhead) for 3
//! Cap'n Proto scenarios and emits a JSON report on stdout:
//!
//! 1. `capnp.spec_version` — pure RPC; validates master-plan §5 target
//!    *capnp P50 < 1 ms*.
//! 2. `capnp.list_holons`  — RPC + filesystem walk + TOML parse.
//! 3. `capnp.invoke`       — RPC + subprocess delegation (end-to-end
//!    realistic; fork-dominated).
//!
//! The filesystem baseline (`fs.subprocess`) is measured by the Python
//! runner — including it here would duplicate work on a code path that
//! has zero Rust overhead.
//!
//! Percentile recording uses the `hdrhistogram` crate (range 1 μs … 60 s,
//! 3 significant digits) — constant memory, streaming-friendly.
//!
//! ```bash
//! # Start the daemon first with a populated holarchy.
//! ./target/release/touring-capnp &
//! ./target/release/examples/bench_d34 <socket> <holarchy> \
//!     [--runs N] [--warmup M] [--invoke-runs K] \
//!     [--invoke-holon NAME] [--invoke-capability CAP]
//! ```
//!
//! Exit codes: 0 success; 1 RPC error; 2 usage error.

use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use hdrhistogram::Histogram;
use tokio::net::UnixStream;
use tokio::task::LocalSet;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use touring_bindings::capnp::holon_core_capnp::holon_registry;

// ---------------------------------------------------------------------------
// CLI parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct BenchConfig {
    socket: PathBuf,
    holarchy: PathBuf,
    runs: usize,
    warmup: usize,
    invoke_runs: usize,
    invoke_holon: String,
    invoke_capability: String,
    scenarios: Vec<String>,
}

impl BenchConfig {
    fn from_args() -> Result<Self, String> {
        let args: Vec<String> = env::args().collect();
        if args.len() < 3 {
            return Err(format!(
                "expected 2 positional args (<socket> <holarchy>), got {}",
                args.len().saturating_sub(1)
            ));
        }
        let mut cfg = Self {
            socket: PathBuf::from(&args[1]),
            holarchy: PathBuf::from(&args[2]),
            runs: 1000,
            warmup: 100,
            invoke_runs: 100,
            invoke_holon: "bench-ping".into(),
            invoke_capability: "ping".into(),
            scenarios: vec!["spec_version".into(), "list_holons".into(), "invoke".into()],
        };
        let mut i = 3;
        while i < args.len() {
            let arg = args.get(i).map(String::as_str).unwrap_or("");
            let val = args.get(i + 1).cloned();
            match arg {
                "--runs" => {
                    cfg.runs = val
                        .ok_or("--runs needs value")?
                        .parse()
                        .map_err(|e| format!("--runs: {e}"))?
                }
                "--warmup" => {
                    cfg.warmup = val
                        .ok_or("--warmup needs value")?
                        .parse()
                        .map_err(|e| format!("--warmup: {e}"))?
                }
                "--invoke-runs" => {
                    cfg.invoke_runs = val
                        .ok_or("--invoke-runs needs value")?
                        .parse()
                        .map_err(|e| format!("--invoke-runs: {e}"))?
                }
                "--invoke-holon" => cfg.invoke_holon = val.ok_or("--invoke-holon needs value")?,
                "--invoke-capability" => {
                    cfg.invoke_capability = val.ok_or("--invoke-capability needs value")?
                }
                "--scenarios" => {
                    let csv = val.ok_or("--scenarios needs value")?;
                    cfg.scenarios = csv
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect();
                }
                other => return Err(format!("unknown flag: {other}")),
            }
            i += 2;
        }
        Ok(cfg)
    }
}

// ---------------------------------------------------------------------------
// Recorder wrapper — mirrors the Python PercentileRecorder protocol so
// that both runners serialise the same JSON shape.
// ---------------------------------------------------------------------------

struct Recorder {
    hist: Histogram<u64>,
}

impl Recorder {
    fn new() -> Self {
        // Range: 1μs .. 60s, 3 significant digits
        let hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)
            .expect("hdrhistogram bounds are valid");
        Self { hist }
    }

    fn record_us(&mut self, us: u64) {
        // Clamp into histogram bounds; sub-μs samples collapse to 1μs
        // (below our interest threshold anyway).
        let v = us.clamp(1, 60_000_000);
        let _ = self.hist.record(v);
    }

    fn summary_json(&self) -> serde_json::Value {
        if self.hist.len() == 0 {
            return serde_json::json!({ "n": 0 });
        }
        serde_json::json!({
            "n": self.hist.len(),
            "min_us": self.hist.min(),
            "max_us": self.hist.max(),
            "mean_us": self.hist.mean(),
            "p50_us": self.hist.value_at_quantile(0.50),
            "p95_us": self.hist.value_at_quantile(0.95),
            "p99_us": self.hist.value_at_quantile(0.99),
            "p999_us": self.hist.value_at_quantile(0.999),
            "stddev_us": self.hist.stdev(),
            "recorder": "hdrhistogram",
        })
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cfg = match BenchConfig::from_args() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!(
                "usage: bench_d34 <socket> <holarchy> [--runs N] [--warmup M] \
                 [--invoke-runs K] [--invoke-holon NAME] [--invoke-capability CAP] \
                 [--scenarios csv]\nerror: {msg}"
            );
            return std::process::ExitCode::from(2);
        }
    };

    let local = LocalSet::new();
    let result = local.run_until(run_benchmarks(cfg)).await;

    match result {
        Ok(report) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).unwrap_or_default()
            );
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("bench_d34 FAILED: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

async fn run_benchmarks(cfg: BenchConfig) -> anyhow::Result<serde_json::Value> {
    let t_total = Instant::now();
    let stream = connect_with_retry(&cfg.socket, 10, Duration::from_millis(100)).await?;

    let (reader, writer) = stream.into_split();
    let network = twoparty::VatNetwork::new(
        futures::io::BufReader::new(reader.compat()),
        futures::io::BufWriter::new(writer.compat_write()),
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    );
    let mut rpc = RpcSystem::new(Box::new(network), None);
    let registry: holon_registry::Client = rpc.bootstrap(rpc_twoparty_capnp::Side::Server);
    let _driver = tokio::task::spawn_local(rpc);

    // ---- Warmup (same signal as the measured scenarios) ------------------
    for _ in 0..cfg.warmup {
        let _ = registry.spec_version_request().send().promise.await?;
    }

    let mut scenarios = serde_json::Map::new();

    if cfg.scenarios.iter().any(|s| s == "spec_version") {
        eprintln!("[bench_d34][rs] capnp.spec_version");
        let mut rec = Recorder::new();
        for _ in 0..cfg.runs {
            let t0 = Instant::now();
            let _ = registry.spec_version_request().send().promise.await?;
            rec.record_us(elapsed_us(t0));
        }
        scenarios.insert("capnp.spec_version".into(), rec.summary_json());
    }

    if cfg.scenarios.iter().any(|s| s == "list_holons") {
        eprintln!("[bench_d34][rs] capnp.list_holons");
        let mut rec = Recorder::new();
        for _ in 0..cfg.runs {
            let t0 = Instant::now();
            let _ = registry.list_holons_request().send().promise.await?;
            rec.record_us(elapsed_us(t0));
        }
        scenarios.insert("capnp.list_holons".into(), rec.summary_json());
    }

    if cfg.scenarios.iter().any(|s| s == "invoke") {
        eprintln!("[bench_d34][rs] capnp.invoke");
        let mut rec = Recorder::new();
        let n = cfg.invoke_runs.min(cfg.runs);
        match bench_invoke(
            &registry,
            n,
            &cfg.invoke_holon,
            &cfg.invoke_capability,
            &mut rec,
        )
        .await
        {
            Ok(_) => {
                scenarios.insert("capnp.invoke".into(), rec.summary_json());
            }
            Err(e) => {
                scenarios.insert(
                    "capnp.invoke".into(),
                    serde_json::json!({ "error": e.to_string() }),
                );
            }
        }
    }

    // ---- Comparison summary ---------------------------------------------
    let comparison = build_comparison(&scenarios);

    let report = serde_json::json!({
        "runner": "rust",
        "crate_version": env!("CARGO_PKG_VERSION"),
        "config": {
            "runs": cfg.runs,
            "warmup": cfg.warmup,
            "invoke_runs": cfg.invoke_runs,
            "scenarios": cfg.scenarios,
            "socket": cfg.socket.display().to_string(),
            "holarchy": cfg.holarchy.display().to_string(),
        },
        "scenarios": scenarios,
        "comparison": comparison,
        "elapsed_sec": t_total.elapsed().as_secs_f64(),
    });
    Ok(report)
}

async fn bench_invoke(
    registry: &holon_registry::Client,
    n: usize,
    holon_name: &str,
    capability: &str,
    rec: &mut Recorder,
) -> anyhow::Result<()> {
    for _ in 0..n {
        // Resolve the holon capability (costs 1 RTT; excluded from timing
        // to isolate the invoke() RTT).
        let mut get_req = registry.get_holon_request();
        get_req.get().set_name(holon_name);
        let holon_cap = get_req.send().pipeline.get_holon();

        let t0 = Instant::now();
        let mut inv_req = holon_cap.invoke_request();
        {
            let mut request = inv_req.get().init_request();
            request.set_capability(capability);
            request.set_args(b"{}");
            request.set_requester("bench_d34");
            request.set_timeout_ms(30_000);
        }
        let _ = inv_req.send().promise.await?;
        rec.record_us(elapsed_us(t0));
    }
    Ok(())
}

fn build_comparison(scenarios: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let target_p50_us: u64 = 1_000; // master plan §5
    let spec_p50 = scenarios
        .get("capnp.spec_version")
        .and_then(|v| v.get("p50_us"))
        .and_then(|v| v.as_u64());
    let target_met = spec_p50.is_some_and(|v| v < target_p50_us);

    serde_json::json!({
        "target_p50_us": target_p50_us,
        "target_met": target_met,
        "spec_version_p50_us": spec_p50,
    })
}

async fn connect_with_retry(
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

fn elapsed_us(t0: Instant) -> u64 {
    u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX)
}
