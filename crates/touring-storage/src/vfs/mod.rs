//! Touring VFS — virtual filesystem overlay for multi-source file management.
//!
//! Provides:
//! - [`FileId`] — opaque u32 identifier stable across renames
//! - [`AbsPath`] / [`AbsPathBuf`] — absolute path types with validation
//! - [`Vfs`] — main virtual filesystem (memory overlay + optional FS)
//! - [`VfsOverlay`] — layered overlay (upper wins)
//! - [`FileSet`] — query by path, FileId, or glob
//! - `VfsWatcher` — file system change notifications (optional `watcher` feature)
//! - [`Projector`] — bidirectional file <-> graph sync engine (optional `sync_projector` feature)

mod abs_path;
mod file_id;
mod file_set;
mod manifest;
mod overlay;
#[allow(clippy::module_inception)]
mod vfs;

pub mod projector;
pub mod watcher;

pub use abs_path::{AbsPath, AbsPathBuf};
pub use file_id::FileId;
pub use file_set::FileSet;
pub use manifest::content_hash;
pub use manifest::{FileManifest, MoveDetectionResult, MoveEvent};
pub use overlay::VfsOverlay;
pub use projector::{Change, NoopProjector, Projector, ProjectorError, SymbolId};
pub use vfs::Vfs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_id_construction() {
        let id = FileId::new(1);
        assert_eq!(id.index(), 1);
    }

    #[test]
    fn abs_path_from_str() {
        let p = AbsPath::from_absolute("/tmp/foo").unwrap();
        assert_eq!(p.as_str(), "/tmp/foo");
    }

    #[test]
    fn abs_path_buf_owned() {
        let p = AbsPathBuf::try_from("/tmp/bar".to_string()).unwrap();
        assert_eq!(p.as_str(), "/tmp/bar");
    }

    #[test]
    fn abs_path_rejects_relative() {
        let result = AbsPath::from_absolute("relative/path");
        assert!(result.is_err());
    }

    #[test]
    fn file_id_invalid() {
        let id = FileId::INVALID;
        assert_eq!(id.index(), u32::MAX);
    }
}
