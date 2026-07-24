//! touring_identity — Entity Identity Registry — canonical name resolution with confidence scoring for Touring's multi-crate architecture. Provides EntityId, Entity, Criterion, EntityRelation, RelationKind types and SQLite DDL. D5.1 of Touring v8 Master Plan S5 Entity Identity Registry..
//!
//! Module structure:
//!   - [`error`]: typed errors via `thiserror`.
//!   - [`types`]: public data types.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free leaf — lock against
// future bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod error;
pub mod registry;
pub mod schema;
pub mod types;

pub use error::{Error, Result};
pub use registry::IdentityRegistry;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_re_exports_compile() {
        let _err: Result<()> = Ok(());
    }
}
