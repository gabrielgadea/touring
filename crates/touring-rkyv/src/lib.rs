//! Zero-copy serialization templates for touring crates.
//!
//! Centralizes common `#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]`
//! patterns used across touring-index, touring-hooks, touring-learning, and touring-cognitive.
//!
//! All 13 template types use `#[archive(check_bytes)]` for byte validation on
//! deserialization, ensuring data integrity for cross-crate IPC and persistence.
//!
//! # Usage
//!
//! Consumer crates should add `touring-rkyv` as a dependency and use:
//! ```ignore
//! use touring_rkyv::{Archive, Serialize, Deserialize};
//! use touring_rkyv::templates::{ArchivedHookEvent, ArchivedSymbol};
//! ```
//!
//! # Crates Using These Templates
//!
//! - `touring-index` — symbol index snapshots
//! - `touring-hooks` — dependency graph snapshots (ArchivedIndexSnapshot)
//! - `touring-learning` — RL state snapshots (QTable, LinUCB, ESAA event records, CRDT graphs)
//!
//! # Note on touring-cognitive
//!
//! touring-cognitive defines its own local GoTSnapshot/GotNodeSnapshot types
//! in `snapshot.rs` because they capture engine-specific state (max_depth,
//! beam_width, pheromone_trails) semantically different from the IPC templates.
//! See `touring-cognitive/src/snapshot.rs` for details.

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free leaf — lock against
// future bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod ipc;
pub mod saga_ipc;
pub mod templates;

// Re-export IPC types so downstream crates can write:
//   use touring_rkyv::{IpcRequest, IpcResponse, frame_request, unframe};
pub use ipc::{
    ArchivedIpcRequest, ArchivedIpcResponse, FrameError, IPC_FRAME_HEADER_LEN, IPC_MAGIC,
    IpcRequest, IpcResponse, frame_request, frame_response, unframe,
};

// Re-export saga coordination types for distributed 2PC
pub use saga_ipc::{SAGA_FRAME_LEN, SAGA_MAGIC, SagaError, SagaMessage, frame_saga, unframe_saga};

// Re-export rkyv derive macros so consumer crates can use:
//   use touring_rkyv::{Archive, Serialize, Deserialize};
// instead of:
//   use rkyv::{Archive, Serialize, Deserialize};
pub use rkyv::{Archive, Deserialize, Serialize};

// Re-export commonly used rkyv functions and modules
pub use rkyv::AlignedVec;
pub use rkyv::archived_root;
pub use rkyv::check_archived_root;
pub use rkyv::ser;
pub use rkyv::to_bytes;
