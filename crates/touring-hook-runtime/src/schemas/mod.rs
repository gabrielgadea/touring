//! Schemas for hook payloads and MCP parameters.
//! Feature D (2026-04-24) — Schema Validation Layer

/// Type-safe payload structs for Touring hook handlers.
pub mod hook_payloads;
/// Type-safe parameter structs for MCP tool handlers.
pub mod mcp_params;
/// Dispatch-time validation helpers for hook and MCP payloads.
pub mod validation;

pub use hook_payloads::*;
pub use mcp_params::*;
pub use validation::{format_validation_errors, validate_payload, validation_deny};
