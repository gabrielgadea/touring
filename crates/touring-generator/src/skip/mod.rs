//! `touring_generator::skip` — Frozen-region markers for generator and post-edit.
//!
//! Parses `// touring:skip-region` … `// touring:skip-end` comment markers to
//! define regions that the generator typestate must not touch. Also supports
//! `#[touring::skip]` attribute for Rust files.
//!
//! # Markers
//!
//! - Line comments: `// touring:skip-region` / `// touring:skip-end`
//! - Block comments: `/* touring:skip-region */` ... `/* touring:skip-end */`
//! - Rust attributes: `#[touring::skip]`
//!
//! # Behavior
//!
//! `SkipContext` is consulted during `PlanExecutor<Rendered>` → typestate
//! transition. If a proposed edit overlaps a `SkipRegion`, the stage
//! aborts with diagnostic `Q-310 RegionFrozen` (blocking).
//!
//! `post_edit` hook re-validates after manual Edit tool writes — emits
//! `W-115 SkippedRegionWritten` (warning, non-blocking).

mod parser;

pub use parser::{ByteSpan, SkipContext, SkipRegion, SkipStyle};

/// Diagnostic codes for skip-region violations.
pub mod codes {
    /// Q-310: Generator attempted to edit a frozen skip-region (blocking).
    pub const Q_310_REGION_FROZEN: &str = "Q-310";
    /// W-115: Manual Edit tool wrote into a skip-region (warning only).
    pub const W_115_SKIPPED_REGION_WRITTEN: &str = "W-115";
}
