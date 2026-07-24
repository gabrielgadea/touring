//! SpanContext — Distributed tracing context for hook chain observability.
//!
//! Provides per-hop timing and trace identification across the full hook chain:
//! `pre_read → pre_edit → post_edit → post_write`.
//!
//! Inspired by Wingfoil's `Traced<T,L>` per-hop latency stamping pattern.
//! Backward compatible via `Option<SpanContext>` — zero cost when disabled.
//!
//! # Usage
//!
//! ```ignore
//! use touring_hooks::shared::span_context::{SpanContext, new_trace_id};
//!
//! let mut span = SpanContext::new(new_trace_id());
//! span.record_layer("pre_read", 100, 150); // enter=100us, exit=150us
//! ```

use std::time::Instant;

/// Globally unique trace identifier — monotonically increasing u64.
pub type TraceId = u64;

/// A single hop in the hook chain with enter/exit timestamps.
#[derive(Debug, Clone)]
pub struct LayerHop {
    /// Human-readable layer name (e.g., "pre_read", "pre_edit", "post_edit").
    pub layer: &'static str,
    /// Microsecond timestamp when the layer entered.
    pub enter_us: u64,
    /// Microsecond timestamp when the layer exited.
    pub exit_us: u64,
}

impl LayerHop {
    /// Duration in microseconds for this hop.
    pub fn duration_us(&self) -> u64 {
        self.exit_us.saturating_sub(self.enter_us)
    }
}

/// A complete trace — a sequence of layer hops from pre_read to post_write.
#[derive(Debug, Clone)]
pub struct SpanContext {
    /// Unique trace identifier for this hook chain invocation.
    pub trace_id: TraceId,
    /// Ordered sequence of layer hops.
    pub hops: Vec<LayerHop>,
    /// Monotonic start time of this trace (used as reference for hop timestamps).
    start: Instant,
}

impl SpanContext {
    /// Create a new trace with the given trace ID.
    pub fn new(trace_id: TraceId) -> Self {
        Self {
            trace_id,
            hops: Vec::with_capacity(4),
            start: Instant::now(),
        }
    }

    /// Record a layer hop — called at enter and exit of each hook.
    ///
    /// `layer` is a static string (no heap allocation).
    /// `enter_us` and `exit_us` are absolute microsecond timestamps from `Instant`.
    pub fn record_layer(&mut self, layer: &'static str, enter_us: u64, exit_us: u64) {
        self.hops.push(LayerHop {
            layer,
            enter_us,
            exit_us,
        });
    }

    /// Returns the number of hops recorded.
    pub fn hop_count(&self) -> usize {
        self.hops.len()
    }

    /// Returns the total duration of this trace in microseconds.
    pub fn total_duration_us(&self) -> u64 {
        self.hops
            .last()
            .map(|last| {
                last.exit_us
                    .saturating_sub(self.hops.first().map(|h| h.enter_us).unwrap_or(0))
            })
            .unwrap_or(0)
    }

    /// Returns a reference to all hops (immutable view).
    pub fn hops(&self) -> &[LayerHop] {
        &self.hops
    }

    /// Returns the elapsed time since trace creation in microseconds.
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
}

/// Generate a new globally unique trace ID.
///
/// Uses a monotonic counter seeded at process start.
/// Thread-safe via atomic increment.
static TRACE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Returns a new unique trace ID — monotonically increasing, never zero.
pub fn new_trace_id() -> TraceId {
    let id = TRACE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if id == 0 {
        // Wrap-around guard — extremely unlikely in practice
        TRACE_COUNTER.store(1, std::sync::atomic::Ordering::Relaxed);
        return 1;
    }
    id
}

/// Capture a timestamp in microseconds from `Instant::now()`.
///
/// Use this for enter/exit timestamps in `SpanContext::record_layer`.
pub fn timestamp_us() -> u64 {
    Instant::now().elapsed().as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_trace_id_increments() {
        let id1 = new_trace_id();
        let id2 = new_trace_id();
        assert!(id2 > id1);
    }

    #[test]
    fn span_records_hops() {
        let mut span = SpanContext::new(1);
        span.record_layer("pre_read", 100, 150);
        span.record_layer("pre_edit", 200, 250);

        assert_eq!(span.hop_count(), 2);
        assert_eq!(span.hops[0].layer, "pre_read");
        assert_eq!(span.hops[0].duration_us(), 50);
        assert_eq!(span.hops[1].layer, "pre_edit");
        assert_eq!(span.hops[1].duration_us(), 50);
    }

    #[test]
    fn span_total_duration() {
        let mut span = SpanContext::new(42);
        span.record_layer("pre_read", 0, 100);
        span.record_layer("post_edit", 200, 300);

        // total = last_exit - first_enter = 300 - 0 = 300
        assert_eq!(span.total_duration_us(), 300);
    }

    #[test]
    fn layer_hop_duration() {
        let hop = LayerHop {
            layer: "test",
            enter_us: 10,
            exit_us: 100,
        };
        assert_eq!(hop.duration_us(), 90);
    }

    #[test]
    fn empty_span_total_duration() {
        let span = SpanContext::new(1);
        assert_eq!(span.total_duration_us(), 0);
    }

    #[test]
    fn span_timestamp_capture() {
        let t1 = timestamp_us();
        let span = SpanContext::new(new_trace_id());
        let t2 = timestamp_us();
        // elapsed_us() should be >= t2 - t1 at creation, but small
        assert!(span.elapsed_us() <= t2.saturating_sub(t1) + 1000);
    }
}
