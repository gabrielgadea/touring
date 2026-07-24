//! BLAKE3 content hashing adapters.
//!
//! Provides content hashing via BLAKE3 with streaming support and
//! compatibility adapters. The sha2 adapter is preserved for existing
//! call sites (additive, non-destructive).

use blake3::Hasher;
use std::io::{self, Read};

/// Compute BLAKE3 hash of a string slice.
/// Use for short content where streaming is not needed.
#[inline]
pub fn str_hash(input: &str) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(input.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Compute BLAKE3 hash of a byte slice.
#[inline]
pub fn content_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(data);
    *hasher.finalize().as_bytes()
}

/// Streaming BLAKE3 hash - use for large content.
pub struct StreamingHasher(Hasher);

impl StreamingHasher {
    /// Create a new streaming hasher.
    #[inline]
    pub fn new() -> Self {
        Self(Hasher::new())
    }

    /// Update with more data.
    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    /// Finalize and return 32-byte hash.
    #[inline]
    pub fn finalize(self) -> [u8; 32] {
        *self.0.finalize().as_bytes()
    }
}

impl Default for StreamingHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Read all from a reader and compute BLAKE3 hash.
pub fn read_and_hash<R: Read>(reader: &mut R) -> io::Result<[u8; 32]> {
    let mut hasher = Hasher::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(buf.get(..n).unwrap_or_default());
    }
    Ok(*hasher.finalize().as_bytes())
}
