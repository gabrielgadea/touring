//! `touring-orchestration` — Layer 5 (Product) orchestration.
//!
//! Combines decompose (DAG), session (lifecycle), rl_bridge (reward).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "orch-decompose")]
pub mod decompose;
#[cfg(feature = "orch-session")]
pub mod session;
#[cfg(feature = "orch-rl-bridge")]
pub mod rl_bridge;
pub mod error;

pub use error::{OrchError, OrchResult};
