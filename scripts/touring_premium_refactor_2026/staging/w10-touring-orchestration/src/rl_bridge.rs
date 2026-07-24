//! `rl_bridge` — reward injection + bandit consultation
//!
//! W10 PLACEHOLDER. Extracted from existing crates in W10.2-W10.5.

use crate::{OrchError, OrchResult};

/// TODO: real impl extracted from existing crates.
pub struct RlBridgeService {}

impl RlBridgeService {
    /// Create new service.
    pub fn new() -> OrchResult<Self> { Ok(Self {}) }
}

impl Default for RlBridgeService {
    fn default() -> Self { Self::new().expect("default") }
}
