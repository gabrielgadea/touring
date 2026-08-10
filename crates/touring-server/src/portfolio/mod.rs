//! Capability Portfolio — the miner plus a re-export of the shared core.
//!
//! The types, lexicon, ranking and storage live in `touring-foundation` so the
//! PreToolUse hook (`touring-cli`) can query the portfolio without depending on
//! this crate. Only [`miner`], which needs a filesystem walker, stays here.

pub mod keyword;
pub mod miner;
pub mod semantic;

pub use touring_foundation::portfolio::{
    CapabilityEntry, CapabilityKind, Evidence, ExternalLens, PortfolioAnswer, ScoredCapability,
    Verdict, feedback, lexicon, query, store,
};
