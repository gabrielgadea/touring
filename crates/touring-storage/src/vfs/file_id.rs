//! FileId — opaque u32 identifier for VFS files.
//!
//! FileId is NOT a path. It is a stable handle that survives renames.
//! The VFS layer maintains the mapping Path <→ FileId internally.

use serde::{Deserialize, Serialize};

/// Opaque u32 identifier for a file in the VFS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(u32);

impl FileId {
    /// Construct a `FileId` from a raw u32 index.
    pub const fn new(id: u32) -> Self {
        FileId(id)
    }

    /// Return the raw u32 index backing this `FileId`.
    pub const fn index(self) -> u32 {
        self.0
    }

    /// Sentinel `FileId` (`u32::MAX`) used to mark an absent or invalid file.
    pub const INVALID: Self = Self(u32::MAX);
}

impl std::fmt::Display for FileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FileId({})", self.0)
    }
}

impl Default for FileId {
    fn default() -> Self {
        Self::INVALID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_id_new_and_index() {
        let id = FileId::new(42);
        assert_eq!(id.index(), 42);
    }

    #[test]
    fn file_id_eq() {
        let a = FileId::new(1);
        let b = FileId::new(1);
        let c = FileId::new(2);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn file_id_display() {
        let id = FileId::new(99);
        assert_eq!(format!("{}", id), "FileId(99)");
    }

    #[test]
    fn file_id_default() {
        assert_eq!(FileId::default(), FileId::INVALID);
    }

    #[test]
    fn file_id_clone() {
        let a = FileId::new(5);
        let b = a;
        let c = a;
        assert_eq!(b, c);
    }
}
