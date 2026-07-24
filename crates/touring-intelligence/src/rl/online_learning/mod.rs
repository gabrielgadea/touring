//! Online learning second layer: FTRL (Follow The Regularized Leader).
//!
//! Learns feature importance for the LinUCB reward prediction.
//! Complements the QTable TD(λ) and LinUCB bandit with per-feature
//! importance weights that adapt incrementally online.
//!
//! # Architecture
//!
//! ```text
//! tool_execution
//!      │
//!      ▼
//! FtrlLayer::update(features, reward)
//!      │   ├── fit_with(Some(model), &single_row_dataset)
//!      │   └── returns predicted reward (before update)
//!      ▼
//! feature weights updated (sparse, L1 promotes zeros)
//! ```
//!
//! # Feature gate
//!
//! The FTRL layer is compiled only when the `ftrl` feature is enabled:
//! ```toml
//! touring-learning = { features = ["ftrl"] }
//! ```

#[cfg(feature = "ftrl")]
pub mod ftrl;

#[cfg(feature = "ftrl")]
pub use ftrl::{FtrlConfig, FtrlLayer};
