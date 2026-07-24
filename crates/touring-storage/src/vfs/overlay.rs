//! VfsOverlay — layered overlay where upper layer wins.
//!
//! Supports stacking multiple Vfs instances: the overlay sits on top
//! of a base Vfs, and lookups fall through to the base if not in overlay.

use crate::vfs::abs_path::{AbsPath, AbsPathBuf};
use crate::vfs::file_id::FileId;
use crate::vfs::vfs::{Vfs, VfsError};
use bytes::Bytes;
use std::collections::HashMap;

/// Layered virtual filesystem where the upper in-memory layer shadows a base [`Vfs`].
pub struct VfsOverlay {
    upper: HashMap<String, Bytes>,
    base: Option<Box<Vfs>>,
}

impl VfsOverlay {
    /// Create an empty overlay with no base layer.
    pub fn new() -> Self {
        VfsOverlay {
            upper: HashMap::new(),
            base: None,
        }
    }

    /// Create an overlay layered on top of the given base [`Vfs`].
    pub fn with_base(base: Vfs) -> Self {
        VfsOverlay {
            upper: HashMap::new(),
            base: Some(Box::new(base)),
        }
    }

    /// Add overlay content — takes precedence over base
    pub fn set(&mut self, path: &AbsPath, content: impl Into<Bytes>) {
        self.upper.insert(path.as_str().to_string(), content.into());
    }

    /// Remove overlay entry (falls through to base if present)
    pub fn remove(&mut self, path: &AbsPath) {
        self.upper.remove(path.as_str());
    }

    /// Check if path exists in overlay or base
    pub fn exists(&self, path: &AbsPath) -> bool {
        let s = path.as_str();
        self.upper.contains_key(s) || self.base.as_ref().is_some_and(|b| b.exists(path))
    }

    /// Read from overlay, fall back to base
    pub fn read(&self, path: &AbsPath) -> Result<Bytes, VfsError> {
        let s = path.as_str().to_string();
        if let Some(content) = self.upper.get(&s) {
            return Ok(content.clone());
        }
        if let Some(base) = &self.base {
            base.read(path)
        } else {
            Err(VfsError::NotFound(AbsPathBuf::from_maybe_unsafe(s)))
        }
    }

    /// List all paths (overlay + base)
    pub fn paths(&self) -> Vec<AbsPathBuf> {
        let mut result: Vec<_> = self
            .upper
            .keys()
            .map(|s| AbsPathBuf::from_maybe_unsafe(s.clone()))
            .collect();
        if let Some(base) = &self.base {
            for p in base.paths() {
                if !self.upper.contains_key(p.as_str()) {
                    result.push(p);
                }
            }
        }
        result
    }

    /// How many overlay entries
    pub fn overlay_len(&self) -> usize {
        self.upper.len()
    }

    /// Get path for a file ID from the base VFS
    pub fn path(&self, id: FileId) -> Option<AbsPathBuf> {
        self.base.as_ref().and_then(|b| b.path(id))
    }
}

impl Default for VfsOverlay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_new_is_empty() {
        let ov = VfsOverlay::new();
        assert_eq!(ov.overlay_len(), 0);
    }

    #[test]
    fn overlay_set_and_read() {
        let mut ov = VfsOverlay::new();
        let path = AbsPath::from_absolute("/overlay/test.txt").unwrap();
        ov.set(path, "content");
        let read = ov.read(path).unwrap();
        assert_eq!(read.as_ref(), b"content");
    }

    #[test]
    fn overlay_remove() {
        let mut ov = VfsOverlay::new();
        let path = AbsPath::from_absolute("/overlay/rm.txt").unwrap();
        ov.set(path, "data");
        ov.remove(path);
        assert!(!ov.exists(path));
    }

    #[test]
    fn overlay_with_base() {
        let base = Vfs::new();
        base.add_overlay(
            AbsPath::from_absolute("/base/file.txt").unwrap(),
            "base-content",
        )
        .unwrap();
        let ov = VfsOverlay::with_base(base);
        let path = AbsPath::from_absolute("/base/file.txt").unwrap();
        let content = ov.read(path).unwrap();
        assert_eq!(content.as_ref(), b"base-content");
    }

    #[test]
    fn overlay_wins_over_base() {
        let base = Vfs::new();
        base.add_overlay(AbsPath::from_absolute("/base/wins.txt").unwrap(), "base")
            .unwrap();
        let mut ov = VfsOverlay::with_base(base);
        let path = AbsPath::from_absolute("/base/wins.txt").unwrap();
        ov.set(path, "overlay");
        let content = ov.read(path).unwrap();
        assert_eq!(content.as_ref(), b"overlay");
    }

    #[test]
    fn overlay_paths_includes_base() {
        let base = Vfs::new();
        base.add_overlay(AbsPath::from_absolute("/base/paths.txt").unwrap(), "base")
            .unwrap();
        let mut ov = VfsOverlay::with_base(base);
        ov.set(AbsPath::from_absolute("/overlay/new.txt").unwrap(), "new");
        let paths = ov.paths();
        assert!(paths.iter().any(|p| p.as_str() == "/base/paths.txt"));
        assert!(paths.iter().any(|p| p.as_str() == "/overlay/new.txt"));
    }
}
