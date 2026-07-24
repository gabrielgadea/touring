//! Tantivy Stream Actor — real-time event-sourced upsert pipeline.
//!
//! Provides a non-blocking `try_send_symbol` API that hooks (post_edit,
//! post_write) call to enqueue `SymbolDoc` updates without stalling the
//! hook request path. A background tokio task drains the channel, batching
//! upserts and committing every `COMMIT_BATCH` docs or `COMMIT_INTERVAL`
//! (whichever comes first).
//!
//! # Backpressure
//!
//! The channel is bounded at `CHANNEL_CAP`. When the actor is falling behind,
//! `try_send` returns an error and the caller falls back to the synchronous
//! direct-upsert path (recording a `tantivy_stream_backpressure_drop` metric).
//! This keeps hook latency bounded regardless of indexing throughput.
//!
//! # Lifecycle
//!
//! 1. `spawn_stream_actor()` — called once from `run_daemon_async()` within
//!    the tokio context. Safe to call multiple times (subsequent calls no-op).
//! 2. `try_send_symbol(doc)` — called from hook handlers (any thread context).
//!    Uses `Sender::try_send` which is sync and does not block.
//! 3. On daemon shutdown the channel sender is dropped (daemon exits), which
//!    closes the receiver and causes the actor loop to drain and exit cleanly.

use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::shared::gate_metrics;
use crate::tantivy_index::SymbolDoc;

/// Bounded channel capacity. Under load the actor commits at ~250ms/batch
/// (5k docs), so 2048 entries = ~8s of burst headroom before dropping.
const CHANNEL_CAP: usize = 2_048;

/// Flush after this many docs regardless of timer.
const COMMIT_BATCH: usize = 5_000;

/// Flush on a recurring timer even when buffer < COMMIT_BATCH.
const COMMIT_INTERVAL: Duration = Duration::from_secs(2);

/// Global stream sender — `None` until `spawn_stream_actor` is called.
static STREAM_TX: OnceLock<mpsc::Sender<SymbolDoc>> = OnceLock::new();

/// Attempt to enqueue a [`SymbolDoc`] for async Tantivy indexing.
///
/// Returns `true` when the doc was accepted into the channel (fast path).
/// Returns `false` when backpressure drops it — the caller should fall back
/// to synchronous `upsert_symbol` to preserve index freshness.
///
/// # eBPF Backpressure
///
/// When the global circuit breaker is open (e.g. triggered by kernel-level memory
/// pressure from eBPF `MemoryPressureSignal`: page faults greater than 1k or L3 cache misses
/// greater than 10k), the function immediately drops the document and returns `false`. This
/// prevents the Tantivy indexer from amplifying memory pressure during a kernel-level
/// anomaly. The caller's synchronous fallback path also respects the circuit.
///
/// This function is synchronous and safe to call from any thread, including
/// non-tokio project actor threads.
pub fn try_send_symbol(doc: SymbolDoc) -> bool {
    // eBPF backpressure gate: shed indexing load when the circuit is open.
    if crate::circuit_breaker::is_open() {
        gate_metrics::record_tantivy_stream_backpressure_drop();
        return false;
    }
    let Some(tx) = STREAM_TX.get() else {
        return false;
    };
    match tx.try_send(doc) {
        Ok(()) => {
            gate_metrics::record_tantivy_stream_enqueued();
            true
        }
        Err(_) => {
            gate_metrics::record_tantivy_stream_backpressure_drop();
            false
        }
    }
}

/// Returns `true` if the stream actor is running (sender is initialized).
pub fn is_active() -> bool {
    STREAM_TX.get().is_some()
}

/// Spawn the background actor that drains the stream channel.
///
/// Must be called from within a tokio runtime context (e.g. `run_daemon_async`).
/// Subsequent calls are silently ignored — the `OnceLock` ensures single init.
/// No-ops if `global_tantivy()` returns `None` (tantivy not available).
pub fn spawn_stream_actor() {
    // Idempotent guard — OnceLock already set means actor is live.
    if STREAM_TX.get().is_some() {
        return;
    }
    // Only spin up when the tantivy index is accessible.
    if crate::tantivy_index::global_tantivy().is_none() {
        tracing::debug!("tantivy stream actor skipped — global tantivy not available");
        return;
    }
    let (tx, rx) = mpsc::channel(CHANNEL_CAP);
    // Race-safe: if two callers reach this concurrently, the loser's tx
    // is dropped, closing the channel — that's fine, winner's actor runs.
    if STREAM_TX.set(tx).is_err() {
        return;
    }
    tokio::spawn(run_actor(rx));
    tracing::info!(
        channel_cap = CHANNEL_CAP,
        commit_batch = COMMIT_BATCH,
        commit_interval_secs = COMMIT_INTERVAL.as_secs(),
        "tantivy stream actor spawned"
    );
}

/// Actor loop — drains the channel, batches, and commits to the Tantivy index.
async fn run_actor(mut rx: mpsc::Receiver<SymbolDoc>) {
    let mut buffer: Vec<SymbolDoc> = Vec::with_capacity(COMMIT_BATCH);
    let mut ticker = tokio::time::interval(COMMIT_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;
            doc = rx.recv() => {
                match doc {
                    Some(d) => {
                        buffer.push(d);
                        if buffer.len() >= COMMIT_BATCH {
                            flush_buffer(&mut buffer);
                        }
                    }
                    None => {
                        // Channel closed (daemon shutting down) — drain remaining.
                        if !buffer.is_empty() {
                            flush_buffer(&mut buffer);
                        }
                        tracing::debug!("tantivy stream actor channel closed, exiting");
                        break;
                    }
                }
            }
            _ = ticker.tick() => {
                if !buffer.is_empty() {
                    flush_buffer(&mut buffer);
                }
            }
        }
    }
}

/// Flush all buffered docs to the global Tantivy index and commit.
///
/// Called from the actor task — never from hook threads. Uses the
/// `&'static TantivyIndex` singleton directly (no Arc needed).
fn flush_buffer(buffer: &mut Vec<SymbolDoc>) {
    let Some(idx) = crate::tantivy_index::global_tantivy() else {
        buffer.clear();
        return;
    };

    let count = buffer.len() as u64;
    for doc in buffer.drain(..) {
        if let Err(e) = idx.upsert_symbol(&doc) {
            tracing::debug!("tantivy stream: upsert error: {e}");
        }
    }
    if let Err(e) = idx.commit() {
        tracing::debug!("tantivy stream: commit error: {e}");
        return;
    }

    gate_metrics::record_tantivy_stream_flush(count);
    tracing::debug!(docs = count, "tantivy stream: flushed batch");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::gate_metrics;

    #[test]
    fn try_send_returns_false_when_actor_not_started() {
        // Without spawn_stream_actor, STREAM_TX is None → returns false.
        // Note: this test is order-sensitive — if another test ran spawn first
        // this would be true. The actor is guarded by OnceLock so we just
        // verify the return is consistent with is_active().
        let doc = SymbolDoc {
            symbol_name: "test".to_string(),
            file_path: "test.rs".to_string(),
            symbol_kind: "fn".to_string(),
            module_path: None,
            docstring: None,
            line_number: 1,
            language: "rust".to_string(),
            visibility: None,
            crate_name: None,
            blake3_hash: None,
            import_count: None,
            export_count: None,
            cognitive_score: None,
            functional_signature: None,
            community_id: None,
        };
        let active = is_active();
        let sent = try_send_symbol(doc);
        // If not active, must return false. If active (actor already running
        // from a parallel test), may return true — both are correct.
        if !active {
            assert!(
                !sent,
                "try_send must return false when actor is not running"
            );
        }
    }

    #[test]
    fn stream_metrics_record_functions_compile_and_run() {
        let before_enq = gate_metrics::global()
            .tantivy_stream_enqueued_count
            .load(std::sync::atomic::Ordering::Relaxed);
        gate_metrics::record_tantivy_stream_enqueued();
        let after_enq = gate_metrics::global()
            .tantivy_stream_enqueued_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(after_enq, before_enq + 1);

        let before_drop = gate_metrics::global()
            .tantivy_stream_backpressure_drop_count
            .load(std::sync::atomic::Ordering::Relaxed);
        gate_metrics::record_tantivy_stream_backpressure_drop();
        let after_drop = gate_metrics::global()
            .tantivy_stream_backpressure_drop_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(after_drop, before_drop + 1);

        let before_flush = gate_metrics::global()
            .tantivy_stream_flush_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let before_docs = gate_metrics::global()
            .tantivy_stream_flush_docs_count
            .load(std::sync::atomic::Ordering::Relaxed);
        gate_metrics::record_tantivy_stream_flush(42);
        let after_flush = gate_metrics::global()
            .tantivy_stream_flush_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let after_docs = gate_metrics::global()
            .tantivy_stream_flush_docs_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(after_flush, before_flush + 1);
        assert_eq!(after_docs, before_docs + 42);
    }

    #[test]
    fn gate_metrics_snapshot_includes_stream_fields() {
        let snap = gate_metrics::GateMetricsSnapshot::capture();
        // Fields must serialize without error (they have #[serde(default)]).
        let json = serde_json::to_string(&snap).expect("snapshot serializes");
        assert!(json.contains("tantivy_stream_enqueued_count"));
        assert!(json.contains("tantivy_stream_backpressure_drop_count"));
        assert!(json.contains("tantivy_stream_flush_count"));
        assert!(json.contains("tantivy_stream_flush_docs_count"));
    }
}
