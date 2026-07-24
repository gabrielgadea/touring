//! Serialized inferlet cache.
//!
//! Provides on-disk caching of compiled WASM inferlet binaries to avoid
//! recompilation on subsequent loads. The cache stores raw `.wasm` bytes
//! alongside a metadata header for versioning and integrity checking.
//!
//! Cache format:
//! ```text
//! [magic: 4 bytes][version: 2 bytes][name_len: 2 bytes][name: N bytes][wasm_bytes: ...]
//! ```
//!
//! # Security
//!
//! Cache files are only loaded if they pass a SHA-256 integrity check
//! embedded in the metadata. On cache miss or corruption, the inferlet
//! is recomputed from source WASM bytes.

use sha2::{Digest, Sha256};

/// Magic bytes identifying a serialized inferlet cache file.
const CACHE_MAGIC: &[u8; 4] = b"INF1";

/// Cache file format version.
const CACHE_VERSION: u16 = 1;

/// Magic + version + reserved + sha256 digest (4 + 2 + 2 + 32 = 40 bytes header)
const HEADER_SIZE: usize = 42;

/// A serialized inferlet cached on disk.
#[derive(Debug, Clone)]
pub struct SerializedInferlet {
    /// Logical name of the inferlet (e.g., "always_success", "memory").
    pub name: String,
    /// Compiled WASM bytes for this inferlet.
    pub wasm_bytes: Vec<u8>,
    /// SHA-256 of the wasm_bytes (for integrity verification on load).
    pub digest: [u8; 32],
}

impl SerializedInferlet {
    /// Serialize this inferlet into a byte vector suitable for writing to disk.
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_SIZE + self.wasm_bytes.len());

        // Magic bytes
        out.extend_from_slice(CACHE_MAGIC);

        // name_len (2 bytes, big-endian) + version (2 bytes, big-endian)
        out.extend_from_slice(&(self.name.len() as u16).to_be_bytes());
        out.extend_from_slice(&CACHE_VERSION.to_be_bytes());

        // Reserved 2 bytes (for future use)
        out.extend_from_slice(&[0u8; 2]);

        // SHA-256 digest of WASM bytes
        out.extend_from_slice(&self.digest);

        // Name (UTF-8)
        out.extend_from_slice(self.name.as_bytes());

        // WASM payload
        out.extend_from_slice(&self.wasm_bytes);

        out
    }

    /// Deserialize from disk bytes.
    ///
    /// Returns `None` if the magic bytes are wrong, version mismatches,
    /// digest doesn't match, or the name is not valid UTF-8.
    pub fn deserialize(data: &[u8]) -> Option<Self> {
        if data.len() < HEADER_SIZE {
            return None;
        }

        // Check magic
        if &data[0..4] != CACHE_MAGIC {
            return None;
        }

        let name_len = u16::from_be_bytes([data[4], data[5]]) as usize;
        let version = u16::from_be_bytes([data[6], data[7]]);

        if version != CACHE_VERSION {
            // Version mismatch — cache is stale
            return None;
        }

        // Digest starts at offset 10 (after magic[4] + name_len[2] + version[2] + reserved[2])
        let digest: [u8; 32] = data[10..42].try_into().ok()?;

        let name = String::from_utf8(data[HEADER_SIZE..HEADER_SIZE + name_len].to_vec()).ok()?;

        let wasm_start = HEADER_SIZE + name_len;
        let wasm_bytes = data[wasm_start..].to_vec();

        // Verify digest
        let computed = Sha256::digest(&wasm_bytes);
        let computed: [u8; 32] = computed.into();
        if computed != digest {
            return None;
        }

        Some(Self {
            name,
            wasm_bytes,
            digest,
        })
    }

    /// Create a new serialized inferlet from raw WASM bytes.
    pub fn new(name: impl Into<String>, wasm_bytes: Vec<u8>) -> Self {
        let digest: [u8; 32] = Sha256::digest(&wasm_bytes).into();
        Self {
            name: name.into(),
            wasm_bytes,
            digest,
        }
    }

    /// Load from a cache file at the given path.
    ///
    /// Returns `None` if the file doesn't exist or is invalid/corrupt.
    pub fn load_from_file(path: &std::path::Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        Self::deserialize(&data)
    }

    /// Write this inferlet to a cache file at the given path.
    ///
    /// Creates parent directories if they don't exist.
    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.serialize())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let original = SerializedInferlet::new(
            "always_success",
            vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
        );
        let serialized = original.serialize();
        let deserialized = SerializedInferlet::deserialize(&serialized)
            .expect("deserialization should succeed for valid data");
        assert_eq!(deserialized.name, "always_success");
        assert_eq!(deserialized.wasm_bytes, original.wasm_bytes);
    }

    #[test]
    fn test_corrupt_data_returns_none() {
        let corrupt = vec![0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00];
        assert!(SerializedInferlet::deserialize(&corrupt).is_none());
    }

    #[test]
    fn test_magic_mismatch_returns_none() {
        let original = SerializedInferlet::new("memory", vec![0x00, 0x61, 0x73, 0x6d]);
        let mut serialized = original.serialize();
        serialized[0] = 0xFF; // Corrupt magic
        assert!(SerializedInferlet::deserialize(&serialized).is_none());
    }

    #[test]
    fn test_file_save_load() {
        let inferlet = SerializedInferlet::new("pattern", vec![0x00, 0x61, 0x73, 0x6d, 0x01]);
        let path = std::env::temp_dir().join("test_inferlet_cache.inf1");

        inferlet
            .save_to_file(&path)
            .expect("save_to_file should succeed in test environment");

        let loaded = SerializedInferlet::load_from_file(&path)
            .expect("load_from_file should succeed after successful save");

        assert_eq!(loaded.name, "pattern");
        assert_eq!(loaded.wasm_bytes, inferlet.wasm_bytes);

        std::fs::remove_file(path).ok();
    }
}
