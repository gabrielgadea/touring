//! Ranking and Drift Detection — Wilson score ranking and pattern drift detection.
//!
//! Unified from touring/src/learning/ranking.rs (559 LOC)

pub mod cusum;
pub mod drift_monitor;
pub mod ebpf_observer;
pub mod page_hinkley;
pub mod wilson;

pub use cusum::{DriftDirection, DriftSignal, LatencyDriftDetector};
pub use drift_monitor::{DriftMonitor, DriftReport};
pub use ebpf_observer::EbpfObserver;
pub use page_hinkley::PageHinkley;
pub use wilson::{DriftDetector, DriftResult, RankedItem, WilsonRanker, WilsonScore};
