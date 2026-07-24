//! ACP shim layer — re-export of the acp module.
//!
//! This module is only available when the `acp-protocol` feature is enabled.
//! The actual ACP implementation lives in `protocol/acp.rs` (a single file module
//! placed here to keep the protocol types self-contained).

pub mod acp;
pub use acp::*;
