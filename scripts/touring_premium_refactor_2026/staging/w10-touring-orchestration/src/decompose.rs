//! `decompose` — DAG creation, subtask deps, ready/finalize
//!
//! W10 PLACEHOLDER. Extracted from existing crates in W10.2-W10.5.

use crate::{OrchError, OrchResult};

/// TODO: real impl extracted from existing crates.
pub struct DecomposeService {}

impl DecomposeService {
    /// Create new service.
    pub fn new() -> OrchResult<Self> { Ok(Self {}) }
}

impl Default for DecomposeService {
    fn default() -> Self { Self::new().expect("default") }
}
