//! Observability primitives for metrics collection and telemetry.
//!
//! # Modules
//!
//! - [`ring_buffer`] — fixed-capacity circular buffer for hot-path metrics
//! - [`rl_metrics`] — RL engine metrics with atomic counters and rolling windows

pub mod ring_buffer;
pub mod rl_metrics;

pub use ring_buffer::RingBuffer;
pub use rl_metrics::{RlMetrics, RlMetricsCollector};
