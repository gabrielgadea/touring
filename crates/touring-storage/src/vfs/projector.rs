//! Projector — bidirectional file <-> graph sync engine for touring-vfs.
//!
//! The [`Projector`] trait defines a sync interface between the VFS layer
//! ([`FileId`]s) and the symbol graph ([`SymbolId`]s). Implementations can
//! optionally wire into touring-vfs via the `sync_projector` feature.

use crate::vfs::file_id::FileId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Opaque u32 identifier for a symbol in the projector's graph view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId(u32);

impl SymbolId {
    /// Sentinel `SymbolId` (`u32::MAX`) marking an absent or invalid symbol.
    pub const INVALID: Self = SymbolId(u32::MAX);
    /// Construct a `SymbolId` from a raw u32 index.
    pub const fn new(id: u32) -> Self {
        SymbolId(id)
    }
    /// Return the raw u32 index backing this `SymbolId`.
    pub fn index(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for SymbolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SymbolId({})", self.0)
    }
}

/// Errors that can occur while synchronizing files and the symbol graph.
#[derive(Debug, Error)]
pub enum ProjectorError {
    /// The referenced file id was not present in the VFS.
    #[error("file {0} not found in VFS")]
    FileNotFound(FileId),
    /// The referenced symbol id was not present in the graph.
    #[error("symbol {0} not found in graph")]
    SymbolNotFound(SymbolId),
    /// File and graph states disagreed and could not be reconciled.
    #[error("sync conflict: {0}")]
    Conflict(String),
    /// The symbol graph backend was not available for the operation.
    #[error("graph unavailable: {0}")]
    GraphUnavailable(String),
}

/// A change detected between file and graph states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Change {
    /// A new file appeared in the VFS.
    FileCreated(FileId),
    /// A file was removed from the VFS.
    FileDeleted(FileId),
    /// A file was renamed, keeping its identity.
    FileRenamed {
        /// File id before the rename.
        old: FileId,
        /// File id after the rename.
        new: FileId,
    },
    /// A new symbol was added to the graph.
    SymbolCreated(SymbolId),
    /// A symbol was removed from the graph.
    SymbolDeleted(SymbolId),
    /// A symbol moved from one file to another.
    SymbolMoved {
        /// The symbol that moved.
        id: SymbolId,
        /// File the symbol moved out of.
        from_file: FileId,
        /// File the symbol moved into.
        to_file: FileId,
    },
}

/// Bidirectional sync engine between VFS files and symbol graph.
///
/// # Invariants
/// - `sync_file_to_graph` is idempotent: calling it twice with the same `FileId`
///   must produce the same graph state.
/// - `sync_graph_to_file` is idempotent: calling it twice with the same `SymbolId`
///   must produce the same file state.
/// - `diff` always returns changes relative to the last successful sync.
pub trait Projector: Send + Sync {
    /// Sync a single file's symbols into the graph.
    fn sync_file_to_graph(&self, file: FileId) -> Result<(), ProjectorError>;

    /// Sync a single symbol's definition back to its file.
    fn sync_graph_to_file(&self, symbol: SymbolId) -> Result<(), ProjectorError>;

    /// Return all changes detected since the last sync.
    fn diff(&self) -> Vec<Change>;
}

/// No-op projector that never syncs anything.
pub struct NoopProjector;

impl Projector for NoopProjector {
    fn sync_file_to_graph(&self, _file: FileId) -> Result<(), ProjectorError> {
        Ok(())
    }
    fn sync_graph_to_file(&self, _symbol: SymbolId) -> Result<(), ProjectorError> {
        Ok(())
    }
    fn diff(&self) -> Vec<Change> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_id_construction() {
        let id = SymbolId::new(42);
        assert_eq!(id.index(), 42);
    }

    #[test]
    fn symbol_id_invalid() {
        assert_eq!(SymbolId::INVALID.index(), u32::MAX);
    }

    #[test]
    fn noop_projector_no_changes() {
        let p = NoopProjector;
        assert!(p.diff().is_empty());
        assert!(p.sync_file_to_graph(FileId::new(1)).is_ok());
        assert!(p.sync_graph_to_file(SymbolId::new(1)).is_ok());
    }

    #[test]
    fn change_debug() {
        let c = Change::FileCreated(FileId::new(1));
        assert!(format!("{:?}", c).contains("FileCreated"));
    }
}
