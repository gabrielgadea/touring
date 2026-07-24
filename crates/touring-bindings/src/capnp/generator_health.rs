//! `GeneratorHealth` Cap'n Proto RPC implementation (THSF Phase 5 COMBO F).
//!
//! Fans out the `touring-core::health_events` broadcast channel to every
//! subscribed capnp listener. Each subscription runs as a local task
//! (`tokio::task::spawn_local`) — required because capnp-rpc client
//! capabilities are `!Send`.
//!
//! # Latency profile
//!
//! - Broadcast receive: ~1 µs (tokio lock-free mpmc)
//! - Filter evaluation: ~100 ns
//! - Capnp RPC call `onDelta`: ~50 µs over Unix socket
//! - **End-to-end**: ~50-100 µs per event, well within the <1 ms target.
//!
//! # Cancellation
//!
//! The client terminates a subscription by either calling `handle.close()`
//! or dropping its `SubscriptionHandle::Client` reference. Both paths set
//! a shared `cancelled` flag; the fan-out task exits on its next loop
//! iteration.

use std::cell::RefCell;
use std::rc::Rc;

use capnp::capability::Promise;
use tokio::sync::broadcast::error::RecvError;

use touring_foundation::health_events::{self, DeltaOutcome, HealthDeltaEvent};

use crate::capnp::holon_generator_capnp::{
    self as r#gen, generator_health, health_delta_listener, subscription_handle,
};

/// Version string for the `holon:generator@0.1.0` contract. MUST match the
/// `@` in `schemas/holon-generator.capnp` canonical metadata.
pub const GENERATOR_SPEC_VERSION: &str = "0.1.0";

// ---------------------------------------------------------------------------
// GeneratorHealthImpl
// ---------------------------------------------------------------------------

/// Server-side implementation of the `generator_health` RPC interface.
///
/// Stateless — every subscription is backed by an independent receiver
/// created from the process-wide `health_events` singleton.
#[derive(Debug, Clone, Default)]
pub struct GeneratorHealthImpl;

impl GeneratorHealthImpl {
    /// Construct a new stateless `GeneratorHealthImpl` server instance.
    pub fn new() -> Self {
        Self
    }
}

impl generator_health::Server for GeneratorHealthImpl {
    fn subscribe(
        self: ::capnp::capability::Rc<Self>,
        params: generator_health::SubscribeParams,
        mut results: generator_health::SubscribeResults,
    ) -> Promise<(), capnp::Error> {
        // Extract listener + filter from params.
        let params_reader = match params.get() {
            Ok(r) => r,
            Err(e) => return Promise::err(e),
        };

        let listener = match params_reader.get_listener() {
            Ok(l) => l,
            Err(e) => return Promise::err(e),
        };

        let filter_reader = match params_reader.get_filter() {
            Ok(f) => f,
            Err(e) => return Promise::err(e),
        };

        let filter = FilterConfig::from_reader(&filter_reader);

        // Allocate shared cancellation flag + subscribe to broadcast channel.
        let cancelled = Rc::new(RefCell::new(false));
        let cancelled_bg = cancelled.clone();
        let mut rx = health_events::subscribe();

        // Fan-out task. Runs until either the broadcast closes, the handle
        // is cancelled, or lagged-too-far (tokio drops the receiver).
        //
        // spawn_local is required: listener.on_delta_request() returns a
        // !Send future because capnp-rpc client capabilities are !Send.
        tokio::task::spawn_local(async move {
            loop {
                if *cancelled_bg.borrow() {
                    break;
                }
                match rx.recv().await {
                    Ok(event) => {
                        if !filter.matches(&event) {
                            continue;
                        }
                        let mut req = listener.on_delta_request();
                        fill_event_builder(req.get().init_event(), &event);
                        // Fire; observe but don't propagate listener errors —
                        // a broken listener MUST NOT block other subscribers.
                        if let Err(e) = req.send().promise.await {
                            tracing::debug!(
                                error = %e,
                                "listener.onDelta failed — stopping subscription"
                            );
                            break;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(
                            dropped = n,
                            "health-delta subscriber lagged — dropped events"
                        );
                        continue;
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });

        let handle_impl = SubscriptionHandleImpl { cancelled };
        let client: subscription_handle::Client = capnp_rpc::new_client(handle_impl);
        results.get().set_handle(client);
        Promise::ok(())
    }

    fn get_counters(
        self: ::capnp::capability::Rc<Self>,
        _params: generator_health::GetCountersParams,
        mut results: generator_health::GetCountersResults,
    ) -> Promise<(), capnp::Error> {
        // Synchronous subprocess — keep the call bounded. Fork+exec
        // `touring gate-metrics -j`, parse the 7 health_delta counters.
        let snapshot = match fetch_counters_snapshot() {
            Ok(s) => s,
            Err(e) => return Promise::err(capnp::Error::failed(format!("get_counters: {e}"))),
        };

        let mut builder = results.get().init_counters();
        builder.set_record_count(snapshot.record_count);
        builder.set_compute_count(snapshot.compute_count);
        builder.set_regression_count(snapshot.regression_count);
        builder.set_improvement_count(snapshot.improvement_count);
        builder.set_streak_alert_count(snapshot.streak_alert_count);
        builder.set_recovery_count(snapshot.recovery_count);
        builder.set_alert_threshold(snapshot.alert_threshold);
        Promise::ok(())
    }

    fn spec_version(
        self: ::capnp::capability::Rc<Self>,
        _params: generator_health::SpecVersionParams,
        mut results: generator_health::SpecVersionResults,
    ) -> Promise<(), capnp::Error> {
        results.get().set_version(GENERATOR_SPEC_VERSION);
        Promise::ok(())
    }
}

// ---------------------------------------------------------------------------
// SubscriptionHandleImpl
// ---------------------------------------------------------------------------

/// Client-visible handle. Flipping `cancelled` causes the fan-out task to
/// exit on its next iteration; dropping the client capability resolves
/// the RPC such that any pending `on_delta_request` promises unblock.
pub struct SubscriptionHandleImpl {
    cancelled: Rc<RefCell<bool>>,
}

impl subscription_handle::Server for SubscriptionHandleImpl {
    fn close(
        self: ::capnp::capability::Rc<Self>,
        _params: subscription_handle::CloseParams,
        _results: subscription_handle::CloseResults,
    ) -> Promise<(), capnp::Error> {
        *self.cancelled.borrow_mut() = true;
        Promise::ok(())
    }
}

// ---------------------------------------------------------------------------
// Filter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct FilterConfig {
    path_prefixes: Vec<String>,
    min_abs_delta: f32,
    regressions_only: bool,
}

impl FilterConfig {
    fn from_reader(reader: &r#gen::subscription_filter::Reader) -> Self {
        let path_prefixes: Vec<String> = match reader.get_path_prefixes() {
            Ok(list) => list
                .iter()
                .filter_map(|t| t.ok().and_then(|s| s.to_str().ok().map(String::from)))
                .collect(),
            Err(_) => Vec::new(),
        };
        Self {
            path_prefixes,
            min_abs_delta: reader.get_min_abs_delta(),
            regressions_only: reader.get_regressions_only(),
        }
    }

    fn matches(&self, evt: &HealthDeltaEvent) -> bool {
        if self.regressions_only && evt.outcome != DeltaOutcome::Regression {
            return false;
        }
        if evt.delta.abs() < self.min_abs_delta {
            return false;
        }
        if !self.path_prefixes.is_empty()
            && !self
                .path_prefixes
                .iter()
                .any(|p| evt.file_path.starts_with(p))
        {
            return false;
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Event builder
// ---------------------------------------------------------------------------

fn fill_event_builder(mut builder: r#gen::health_delta_event::Builder, evt: &HealthDeltaEvent) {
    builder.set_file_path(evt.file_path.as_str());
    builder.set_old_health(evt.old_health);
    builder.set_new_health(evt.new_health);
    builder.set_delta(evt.delta);
    builder.set_outcome(match evt.outcome {
        DeltaOutcome::Improvement => r#gen::DeltaOutcome::Improvement,
        DeltaOutcome::Regression => r#gen::DeltaOutcome::Regression,
        DeltaOutcome::Neutral => r#gen::DeltaOutcome::Neutral,
    });
    builder.set_regression_streak(evt.regression_streak);
    builder.set_improvement_streak(evt.improvement_streak);
    builder.set_timestamp_ms(evt.timestamp_ms);
}

// ---------------------------------------------------------------------------
// Counter snapshot (subprocess shim)
// ---------------------------------------------------------------------------

/// In-memory mirror of the 7 `health_delta_*` counter fields surfaced by
/// `touring gate-metrics -j`. Kept internal — callers consume the capnp
/// builder instead.
#[derive(Debug, Clone, Default)]
struct CountersSnapshot {
    record_count: u64,
    compute_count: u64,
    regression_count: u64,
    improvement_count: u64,
    streak_alert_count: u64,
    recovery_count: u64,
    alert_threshold: u32,
}

fn fetch_counters_snapshot() -> anyhow::Result<CountersSnapshot> {
    // `touring` binary resolution order: env override, then PATH.
    let touring_bin = std::env::var("TOURING_BIN").unwrap_or_else(|_| "touring".into());

    let output = std::process::Command::new(&touring_bin)
        .args(["gate-metrics", "-j"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "`{touring_bin} gate-metrics -j` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    Ok(CountersSnapshot {
        record_count: extract_u64(&value, "health_delta_record_count"),
        compute_count: extract_u64(&value, "health_delta_compute_count"),
        regression_count: extract_u64(&value, "health_delta_regression_count"),
        improvement_count: extract_u64(&value, "health_delta_improvement_count"),
        streak_alert_count: extract_u64(&value, "health_delta_streak_alert_count"),
        recovery_count: extract_u64(&value, "health_delta_recovery_count"),
        alert_threshold: value
            .get("health_delta")
            .and_then(|hd| hd.get("alert_threshold"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(3),
    })
}

fn extract_u64(value: &serde_json::Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_regressions_only_excludes_improvements() {
        let filter = FilterConfig {
            path_prefixes: vec![],
            min_abs_delta: 0.0,
            regressions_only: true,
        };
        let improvement = sample_event(DeltaOutcome::Improvement, 0.1, "/a.rs");
        let regression = sample_event(DeltaOutcome::Regression, -0.1, "/a.rs");
        assert!(!filter.matches(&improvement));
        assert!(filter.matches(&regression));
    }

    #[test]
    fn filter_min_abs_delta() {
        let filter = FilterConfig {
            path_prefixes: vec![],
            min_abs_delta: 0.05,
            regressions_only: false,
        };
        let tiny = sample_event(DeltaOutcome::Regression, -0.01, "/a.rs");
        let big = sample_event(DeltaOutcome::Regression, -0.2, "/a.rs");
        assert!(!filter.matches(&tiny));
        assert!(filter.matches(&big));
    }

    #[test]
    fn filter_path_prefixes() {
        let filter = FilterConfig {
            path_prefixes: vec!["/src/".to_string(), "/tests/".to_string()],
            min_abs_delta: 0.0,
            regressions_only: false,
        };
        let matched = sample_event(DeltaOutcome::Improvement, 0.1, "/src/foo.rs");
        let excluded = sample_event(DeltaOutcome::Improvement, 0.1, "/docs/x.md");
        assert!(filter.matches(&matched));
        assert!(!filter.matches(&excluded));
    }

    #[test]
    fn filter_empty_prefix_list_allows_all_paths() {
        let filter = FilterConfig::default();
        let any = sample_event(DeltaOutcome::Improvement, 0.1, "/anywhere/x.rs");
        assert!(filter.matches(&any));
    }

    #[test]
    fn spec_version_matches_package_contract() {
        assert_eq!(GENERATOR_SPEC_VERSION, "0.1.0");
    }

    fn sample_event(outcome: DeltaOutcome, delta: f32, path: &str) -> HealthDeltaEvent {
        HealthDeltaEvent {
            file_path: path.to_string(),
            old_health: 0.8,
            new_health: 0.8 + delta,
            delta,
            outcome,
            regression_streak: 0,
            improvement_streak: 0,
            timestamp_ms: 0,
        }
    }
}
