//! Route components for touring-web.
//!
//! Each route fetches data from the touring CLI and renders a UI panel.

pub mod dashboard;
pub mod health;
pub mod memory;
pub mod orphans;
pub mod search;
pub mod wiring;
pub mod workspace;

pub mod quality;
pub mod quality_diff;
pub mod quality_rules;

pub mod federation;

// Wave 4 (2026-06-12) — zip-artboard pages wired to real CLI endpoints.
pub mod chains;
pub mod cognitive;
pub mod hooks;
pub mod plans;
pub mod sessions;
pub mod settings;

// Elite W4 (SPEC 2026-06-12 §6) — new surfaces.
pub mod mcp;
pub mod speculate;
pub mod wiring_impact;

// W6 (SPEC §6.5) — tri-pane inspector composing existing endpoints.
pub mod inspector;
