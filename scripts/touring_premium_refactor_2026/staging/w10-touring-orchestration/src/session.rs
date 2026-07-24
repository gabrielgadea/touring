//! `session` — session lifecycle (start, assess, end)
//!
//! W10 PLACEHOLDER. Extracted from existing crates in W10.2-W10.5.

use crate::{OrchError, OrchResult};

/// TODO: real impl extracted from existing crates.
pub struct SessionService {}

impl SessionService {
    /// Create new service.
    pub fn new() -> OrchResult<Self> { Ok(Self {}) }
}

impl Default for SessionService {
    fn default() -> Self { Self::new().expect("default") }
}
