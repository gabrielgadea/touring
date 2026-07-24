//! Multi-workspace Sentrux signal federation.
//!
//! Sentrux Master Plan Wave 3 P7 (2026-05-09). This sub-module
//! aggregates [`touring_analysis::quality::signal::WorkspaceQualitySignal`]
//! snapshots from N workspaces into a single
//! `FederationSummary` suitable for fleet-wide quality dashboards
//! (cross-repo monorepos, mono-org dashboards, CI fan-in).
//!
//! # Layering
//!
//! ```text
//! per-workspace ── compute_quality_signal ──► WorkspaceQualitySignal
//!                                                  │
//!                                                  ▼
//!                              FederationEntry { id, root, signal, ts }
//!                                                  │
//!                                                  ▼ N entries
//!                                            aggregate(entries)
//!                                                  │
//!                                                  ▼
//!                                          FederationSummary
//! ```
//!
//! The aggregator is pure / deterministic — see
//! `aggregator` for the math (arithmetic mean,
//! min/max identification, bottleneck histogram, per-axis means).
//!
//! # Quick start
//!
//! ```ignore
//! use touring_hooks::shared::federation::{aggregate, FederationEntry};
//! use touring_analysis::quality::signal::compute_quality_signal;
//!
//! let entries: Vec<FederationEntry> = workspaces
//!     .iter()
//!     .map(|ws| FederationEntry {
//!         workspace_id: ws.id.clone(),
//!         workspace_root: ws.root.clone(),
//!         signal: compute_quality_signal(&ws.workspace),
//!         timestamp_ms: now_ms(),
//!     })
//!     .collect();
//! let summary = aggregate(&entries);
//! println!("federated signal: {}/10000", summary.aggregate_signal_0_10000);
//! ```

pub mod aggregator;
pub mod types;

pub use aggregator::aggregate;
pub use types::{AvgRootCauses, FederationEntry, FederationSummary, bottleneck_label};
