//! VFS — main virtual filesystem implementation.
//!
//! The VFS maintains an in-memory overlay of files with optional real-FS fallback.
//! FileId is the primary key — stable across renames.
//!
//! Layers (upper wins):
//! 1. In-memory edits (VfsOverlay)
//! 2. Canonical FS (real file system)
//!
//! Thread-safe: uses parking_lot RwLock internally.

use crate::vfs::abs_path::{AbsPath, AbsPathBuf};
use crate::vfs::file_id::FileId;
use bytes::Bytes;
use std::collections::HashMap;
use thiserror::Error;

/// Errors returned by virtual filesystem operations.
#[derive(Error, Debug)]
pub enum VfsError {
    #[error("file not found: {0}")]
    NotFound(AbsPathBuf),
    #[error("file id not found: {0}")]
    FileIdNotFound(FileId),
    #[error("path exists: {0}")]
    AlreadyExists(AbsPathBuf),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("path is not absolute: {0}")]
    NotAbsolute(String),
}

/// Source of a file's content within the [`Vfs`].
#[derive(Debug, Clone)]
pub enum VfsContent {
    /// In-memory overlay content (takes precedence)
    Overlay(Bytes),
    /// Real FS file content
    FileSystem,
}

/// In-memory virtual filesystem mapping paths to stable [`FileId`]s and their content.
pub struct Vfs {
    /// Maps FileId → content
    files: parking_lot::RwLock<HashMap<FileId, VfsContent>>,
    /// Maps path string → FileId
    path_to_id: parking_lot::RwLock<HashMap<String, FileId>>,
    /// Maps FileId → path (for reverse lookup)
    id_to_path: parking_lot::RwLock<HashMap<FileId, String>>,
    /// Next FileId to allocate
    next_id: parking_lot::RwLock<u32>,
}

impl Vfs {
    /// Create an empty `Vfs` with no files registered.
    pub fn new() -> Self {
        Vfs {
            files: parking_lot::RwLock::new(HashMap::new()),
            path_to_id: parking_lot::RwLock::new(HashMap::new()),
            id_to_path: parking_lot::RwLock::new(HashMap::new()),
            next_id: parking_lot::RwLock::new(0),
        }
    }

    fn alloc_id(&self) -> FileId {
        let mut guard = self.next_id.write();
        let id = FileId::new(*guard);
        *guard += 1;
        id
    }

    /// Add a file from the real filesystem (lazy — content not loaded until read)
    pub fn add_file_system(&self, path: &AbsPath) -> Result<FileId, VfsError> {
        let path_str = path.as_str().to_string();
        if !path_str.starts_with('/') {
            return Err(VfsError::NotAbsolute(path_str.clone()));
        }

        // Check if already exists
        {
            let p2i = self.path_to_id.read();
            if let Some(id) = p2i.get(&path_str) {
                return Ok(*id);
            }
        }

        let id = self.alloc_id();

        let mut files = self.files.write();
        let mut p2i = self.path_to_id.write();
        let mut i2p = self.id_to_path.write();

        files.insert(id, VfsContent::FileSystem);
        p2i.insert(path_str.clone(), id);
        i2p.insert(id, path_str);

        Ok(id)
    }

    /// Add in-memory overlay content (takes precedence over FS)
    pub fn add_overlay(
        &self,
        path: &AbsPath,
        content: impl Into<Bytes>,
    ) -> Result<FileId, VfsError> {
        let path_str = path.as_str().to_string();
        if !path_str.starts_with('/') {
            return Err(VfsError::NotAbsolute(path_str.clone()));
        }

        let id = {
            let mut p2i = self.path_to_id.write();
            if let Some(id) = p2i.get(&path_str) {
                *id
            } else {
                let id = self.alloc_id();
                p2i.insert(path_str.clone(), id);
                self.id_to_path.write().insert(id, path_str.clone());
                id
            }
        };

        let mut files = self.files.write();
        files.insert(id, VfsContent::Overlay(content.into()));

        Ok(id)
    }

    /// Remove a path entirely (overlay or FS)
    pub fn remove(&self, path: &AbsPath) -> Result<(), VfsError> {
        let path_str = path.as_str().to_string();

        let id = {
            let p2i = self.path_to_id.read();
            p2i.get(&path_str).copied()
        };

        let id = id.ok_or_else(|| VfsError::NotFound(path.to_buf()))?;

        let mut files = self.files.write();
        let mut p2i = self.path_to_id.write();
        let mut i2p = self.id_to_path.write();

        files.remove(&id);
        p2i.remove(&path_str);
        i2p.remove(&id);

        Ok(())
    }

    /// Check if a path exists (overlay or FS)
    pub fn exists(&self, path: &AbsPath) -> bool {
        let p2i = self.path_to_id.read();
        p2i.contains_key(path.as_str())
    }

    /// Get file ID for a path
    pub fn file_id(&self, path: &AbsPath) -> Option<FileId> {
        let p2i = self.path_to_id.read();
        p2i.get(path.as_str()).copied()
    }

    /// Get path for a file ID
    pub fn path(&self, id: FileId) -> Option<AbsPathBuf> {
        let i2p = self.id_to_path.read();
        i2p.get(&id)
            .map(|s| AbsPathBuf::from_maybe_unsafe(s.clone()))
    }

    /// Read file content — overlay preferred, FS fallback
    pub fn read(&self, path: &AbsPath) -> Result<Bytes, VfsError> {
        let path_str = path.as_str().to_string();

        let content = {
            let files = self.files.read();
            let p2i = self.path_to_id.read();
            if let Some(id) = p2i.get(&path_str) {
                files.get(id).cloned()
            } else {
                None
            }
        };

        match content {
            Some(VfsContent::Overlay(bytes)) => Ok(bytes.clone()),
            Some(VfsContent::FileSystem) => std::fs::read(path.as_str())
                .map(Bytes::from)
                .map_err(VfsError::from),
            None => Err(VfsError::NotFound(path.to_buf())),
        }
    }

    /// Write file content to overlay
    pub fn write(&self, path: &AbsPath, content: impl Into<Bytes>) -> Result<(), VfsError> {
        self.add_overlay(path, content)?;
        Ok(())
    }

    /// List all paths in the VFS
    pub fn paths(&self) -> Vec<AbsPathBuf> {
        let p2i = self.path_to_id.read();
        p2i.keys()
            .map(|s| AbsPathBuf::from_maybe_unsafe(s.clone()))
            .collect()
    }

    /// Number of files
    pub fn len(&self) -> usize {
        let files = self.files.read();
        files.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Vfs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vfs_new() {
        let vfs = Vfs::new();
        assert!(vfs.is_empty());
    }

    #[test]
    fn vfs_add_overlay() {
        let vfs = Vfs::new();
        let path = AbsPath::from_absolute("/mem/foo.txt").unwrap();
        let id = vfs.add_overlay(path, Bytes::from_static(b"hello")).unwrap();
        assert_eq!(id.index(), 0);
        assert!(vfs.exists(path));
    }

    #[test]
    fn vfs_read_overlay() {
        let vfs = Vfs::new();
        let path = AbsPath::from_absolute("/mem/bar.txt").unwrap();
        vfs.add_overlay(path, "world").unwrap();
        let content = vfs.read(path).unwrap();
        assert_eq!(content.as_ref(), b"world");
    }

    #[test]
    fn vfs_remove() {
        let vfs = Vfs::new();
        let path = AbsPath::from_absolute("/mem/rm.txt").unwrap();
        vfs.add_overlay(path, "data").unwrap();
        vfs.remove(path).unwrap();
        assert!(!vfs.exists(path));
    }

    #[test]
    fn vfs_add_overlay_twice_same_path() {
        let vfs = Vfs::new();
        let path = AbsPath::from_absolute("/mem/same.txt").unwrap();
        let id1 = vfs.add_overlay(path, "v1").unwrap();
        let id2 = vfs.add_overlay(path, "v2").unwrap();
        assert_eq!(id1, id2);
        let content = vfs.read(path).unwrap();
        assert_eq!(content.as_ref(), b"v2");
    }

    #[test]
    fn vfs_not_found() {
        let vfs = Vfs::new();
        let path = AbsPath::from_absolute("/nonexistent").unwrap();
        let result = vfs.read(path);
        assert!(matches!(result, Err(VfsError::NotFound(_))));
    }

    #[test]
    fn vfs_paths() {
        let vfs = Vfs::new();
        vfs.add_overlay(AbsPath::from_absolute("/a").unwrap(), "a")
            .unwrap();
        vfs.add_overlay(AbsPath::from_absolute("/b").unwrap(), "b")
            .unwrap();
        let paths = vfs.paths();
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn vfs_file_id_lookup() {
        let vfs = Vfs::new();
        let path = AbsPath::from_absolute("/lookup/test.txt").unwrap();
        let id = vfs.add_overlay(path, "content").unwrap();
        assert_eq!(vfs.file_id(path), Some(id));
        assert_eq!(vfs.path(id), Some(path.to_buf()));
    }

    #[test]
    fn vfs_overlay_wins_over_fs() {
        let vfs = Vfs::new();
        let path = AbsPath::from_absolute("/tmp/touring-vfs-overlay-wins").unwrap();
        // Write a real file
        std::fs::write(path.as_str(), "fs-content").unwrap();
        vfs.add_file_system(path).unwrap();
        // Now add overlay
        vfs.add_overlay(path, "overlay-content").unwrap();
        // Overlay should win
        let content = vfs.read(path).unwrap();
        assert_eq!(content.as_ref(), b"overlay-content");
        // Clean up
        std::fs::remove_file(path.as_str()).ok();
    }
}
