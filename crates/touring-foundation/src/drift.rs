//! Unified drift detection trait and signal types.
//!
//! Provides a common interface for concept drift detection algorithms
//! across the Touring workspace.
//!
//! ## Implementations
//!
//! - `touring_simd::statistics::CusumDetector` — CUSUM algorithm in touring-simd
//! - `touring_learning::ranking::DriftDetector` — Sliding window in touring-learning

/// Drift detection signal indicating the severity of detected drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftSignal {
    /// No drift detected — process is stable.
    None,
    /// Warning: potential drift detected, monitoring increased.
    Warning,
    /// Significant drift detected — process has shifted.
    Drift,
}

/// Unified trait for drift detection algorithms.
///
/// Implementors maintain internal state and report drift signals
/// based on sequential observations.
pub trait DriftDetector {
    /// Process a new observation and return drift signal.
    fn detect(&mut self, value: f32) -> DriftSignal;

    /// Reset detector internal state.
    fn reset(&mut self);
}
