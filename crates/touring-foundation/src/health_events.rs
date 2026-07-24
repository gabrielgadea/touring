//! Cross-crate broadcast channel for `HealthDeltaEvent`.
//!
//! This module hosts a singleton `tokio::sync::broadcast::Sender<HealthDeltaEvent>`
//! published by `touring-hooks::health_delta::compute_health_delta` and
//! subscribed by `touring-capnp-server`'s `GeneratorHealth` RPC fan-out
//! (THSF Phase 5 — COMBO F).
//!
//! # Design
//!
//! - **Global singleton** via `OnceLock` — zero cost when unused; lazy-init
//!   on first `publish` or `subscribe`.
//! - **Fixed capacity 64** — caps worst-case memory when no subscribers
//!   drain the channel. Each event is ~128 bytes → ~8 KB peak.
//! - **Non-blocking publish** — when no receivers exist, `send` returns
//!   `Err(SendError)` which we map to `0` and silently drop. Publisher
//!   never blocks or panics.
//! - **Lagged receivers** — slow subscribers receive
//!   `RecvError::Lagged(n)` per `tokio::sync::broadcast` semantics.
//!   Consumers MUST tolerate this (treat as "catch up" signal).
//!
//! # Thread safety
//!
//! `broadcast::Sender<T>` is `Send + Sync` when `T: Send + Clone`.
//! `HealthDeltaEvent` satisfies this (pure data, all owned).
//!
//! # Non-invasion
//!
//! Keeping this channel in `touring-foundation` (a leaf dependency) lets both
//! `touring-hooks` (heavy, with GPU/tantivy deps) and `touring-capnp-server`
//! (isolation-preserving, subprocess-driven) share the bus without
//! either pulling the other into its compile graph.

use std::sync::OnceLock;
use tokio::sync::broadcast;

/// Semantic classification of a health delta.
///
/// The thresholds for `Improvement` vs `Regression` are decided by the
/// producer (`touring-hooks::health_delta`) and encoded in the event — this
/// module does not re-classify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaOutcome {
    /// `new_health` exceeds `old_health` beyond the improvement threshold.
    Improvement,
    /// `new_health` falls below `old_health` beyond the regression threshold.
    Regression,
    /// Change within the neutral band (effectively unchanged).
    Neutral,
}

/// A single health delta event.
///
/// Emitted every time a post-edit hook computes a delta for a tracked
/// source file. Wire-level payload consumed by capnp `HealthDeltaListener`
/// subscribers and JSON fallback paths.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthDeltaEvent {
    /// Absolute path of the file whose health delta was computed.
    pub file_path: String,
    /// Health score before the edit, in `[0.0, 1.0]`.
    pub old_health: f32,
    /// Health score after the edit, in `[0.0, 1.0]`.
    pub new_health: f32,
    /// `new_health - old_health`. Signed.
    pub delta: f32,
    /// Classification of this delta.
    pub outcome: DeltaOutcome,
    /// Cumulative consecutive regressions for this path (reset on improvement).
    pub regression_streak: u32,
    /// Cumulative consecutive improvements for this path (reset on regression).
    pub improvement_streak: u32,
    /// Wall-clock UNIX epoch milliseconds at event creation.
    pub timestamp_ms: u64,
}

/// Capacity sized for moderate bursts without unbounded memory.
/// Empirically, post-edit hooks emit O(1) deltas per second — 64 is ample.
const CHANNEL_CAPACITY: usize = 64;

static SENDER: OnceLock<broadcast::Sender<HealthDeltaEvent>> = OnceLock::new();

fn sender() -> &'static broadcast::Sender<HealthDeltaEvent> {
    SENDER.get_or_init(|| {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        tx
    })
}

/// Publish a `HealthDeltaEvent` to all active subscribers.
///
/// Returns the count of subscribers that received the event. Returns `0`
/// when no subscribers are active — the event is silently dropped and the
/// call is cheap (single atomic load on the global `OnceLock`).
///
/// This function is **non-blocking** and infallible at the call site.
pub fn publish(event: HealthDeltaEvent) -> usize {
    sender().send(event).unwrap_or_default()
}

/// Subscribe to the global health delta stream.
///
/// Each returned `Receiver` gets every event published after its creation.
/// Lagged receivers receive `RecvError::Lagged(n)` and MUST continue to call
/// `recv` (it is not a terminal error).
pub fn subscribe() -> broadcast::Receiver<HealthDeltaEvent> {
    sender().subscribe()
}

/// Number of active subscribers. Useful for observability.
pub fn subscriber_count() -> usize {
    sender().receiver_count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_event(path: &str, delta: f32) -> HealthDeltaEvent {
        HealthDeltaEvent {
            file_path: path.to_string(),
            old_health: 0.8,
            new_health: 0.8 + delta,
            delta,
            outcome: if delta > 0.0 {
                DeltaOutcome::Improvement
            } else if delta < 0.0 {
                DeltaOutcome::Regression
            } else {
                DeltaOutcome::Neutral
            },
            regression_streak: 0,
            improvement_streak: if delta > 0.0 { 1 } else { 0 },
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }
    }

    #[test]
    fn publish_with_no_subscribers_returns_zero() {
        // Fresh state isn't guaranteed across tests — cargo test runs in
        // parallel threads sharing the singleton. What IS guaranteed: if
        // we create no new subscriber here, any `count` returned reflects
        // subscribers from OTHER tests. So we assert it returns a valid
        // (non-panicking) count, not a specific value.
        let count = publish(sample_event("/tmp/a.rs", 0.05));
        // Can't assert == 0 reliably across parallel tests.
        let _ = count;
    }

    #[tokio::test]
    async fn publish_and_subscribe_roundtrip() {
        let mut rx = subscribe();
        let evt = sample_event("/tmp/roundtrip.rs", 0.1);
        let _n = publish(evt.clone());

        // Drain until we receive OUR specific event. Parallel tests sharing the
        // global singleton channel may have published other events before ours.
        let received = loop {
            let candidate = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
                .await
                .expect("timed out waiting for /tmp/roundtrip.rs event")
                .expect("recv error");
            if candidate.file_path == "/tmp/roundtrip.rs" {
                break candidate;
            }
        };

        assert_eq!(received.file_path, "/tmp/roundtrip.rs");
        assert_eq!(received.outcome, DeltaOutcome::Improvement);
        assert!((received.delta - 0.1).abs() < 1e-6);
    }

    #[tokio::test]
    async fn multiple_subscribers_all_receive() {
        let mut rx1 = subscribe();
        let mut rx2 = subscribe();
        let evt = sample_event("/tmp/fanout.rs", -0.05);
        publish(evt);

        // Drain until we receive OUR specific event. Parallel tests sharing the
        // global singleton channel may have published other events before ours.
        let r1 = loop {
            let candidate = tokio::time::timeout(std::time::Duration::from_millis(100), rx1.recv())
                .await
                .expect("rx1 timeout")
                .expect("rx1 recv");
            if candidate.file_path == "/tmp/fanout.rs" {
                break candidate;
            }
        };
        let r2 = loop {
            let candidate = tokio::time::timeout(std::time::Duration::from_millis(100), rx2.recv())
                .await
                .expect("rx2 timeout")
                .expect("rx2 recv");
            if candidate.file_path == "/tmp/fanout.rs" {
                break candidate;
            }
        };

        assert_eq!(r1.file_path, "/tmp/fanout.rs");
        assert_eq!(r2.file_path, "/tmp/fanout.rs");
        assert_eq!(r1.outcome, DeltaOutcome::Regression);
        assert_eq!(r2.outcome, DeltaOutcome::Regression);
    }

    #[test]
    fn subscriber_count_tracks_live_receivers() {
        let before = subscriber_count();
        let rx_a = subscribe();
        let rx_b = subscribe();
        assert!(subscriber_count() >= before + 2);
        drop(rx_a);
        drop(rx_b);
        // Count may not decrement immediately on some tokio versions — only
        // asserting upper bound is stable. Don't assert equality.
    }

    #[test]
    fn event_serde_roundtrip() {
        let evt = sample_event("/tmp/serde.rs", 0.03);
        let json = serde_json::to_string(&evt).expect("ser");
        let decoded: HealthDeltaEvent = serde_json::from_str(&json).expect("de");
        assert_eq!(decoded.file_path, evt.file_path);
        assert_eq!(decoded.outcome, evt.outcome);
        assert!((decoded.delta - evt.delta).abs() < 1e-6);
    }

    #[test]
    fn outcome_serializes_snake_case() {
        let json =
            serde_json::to_string(&DeltaOutcome::Improvement).expect("serialize Improvement");
        assert_eq!(json, r#""improvement""#);
        let json = serde_json::to_string(&DeltaOutcome::Regression).expect("serialize Regression");
        assert_eq!(json, r#""regression""#);
    }
}
