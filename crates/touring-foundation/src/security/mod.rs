//! Security analysis foundation types.
//!
//! Provides zero-dependency foundation types for the security analysis layer:
//! - [`vulnerability`] — [`VulnMatch`] and [`VulnerabilityPattern`] trait

pub mod vulnerability;

pub use vulnerability::{VulnMatch, VulnerabilityPattern};
