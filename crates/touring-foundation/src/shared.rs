//! touring-foundation shared types — minimal types shared across all crates.
//!
//! This module holds the foundation for cross-crate type sharing.
//! Only truly universal types belong here to keep the core crate lightweight.

pub mod circuit_breaker;
pub mod domain_circuit;
pub mod pool;
