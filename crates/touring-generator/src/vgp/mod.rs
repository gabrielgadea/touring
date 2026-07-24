//! VGP — Verified Generation Protocol engine.

pub mod engine;
pub mod fuzzy;

pub use engine::{SymbolKey, VgpEngine, VgpLookupResult};
pub use fuzzy::{FuzzySearcher, NoopFuzzySearcher};
