//! Absolute path types with validation.
//!
//! `AbsPath` is a borrowed reference to an absolute, normalized path.
//! `AbsPathBuf` is the owned version.
//!
//! Invariant: paths must be absolute (start with `/`) and UTF-8.

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// Errors returned when validating or constructing an absolute path.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum PathError {
    /// The supplied path did not start with `/`.
    #[error("path is not absolute: {0}")]
    NotAbsolute(String),
    /// The supplied path was not valid UTF-8.
    #[error("path contains invalid UTF-8")]
    InvalidUtf8,
}

/// Borrowed absolute path — always starts with `/`, validated UTF-8.
#[derive(Debug, Eq)]
pub struct AbsPath {
    inner: str,
}

/// Owned absolute path buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbsPathBuf {
    /// Invariant: must start with `/` and be valid UTF-8
    inner: String,
}

impl AbsPath {
    /// Create from a string slice after validating it starts with `/`.
    pub fn from_absolute(s: &str) -> Result<&Self, PathError> {
        if !s.starts_with('/') {
            return Err(PathError::NotAbsolute(s.to_string()));
        }
        // Safety: AbsPath is just a str wrapper with same invariants
        Ok(unsafe { &*(s as *const str as *const AbsPath) })
    }

    /// Borrow the path as a string slice.
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Borrow the path as a standard [`Path`].
    pub fn as_std_path(&self) -> &Path {
        Path::new(&self.inner)
    }

    /// Clone this path into an owned [`AbsPathBuf`].
    pub fn to_buf(&self) -> AbsPathBuf {
        AbsPathBuf {
            inner: self.inner.to_string(),
        }
    }
}

impl AbsPathBuf {
    /// Create from a String, validating it starts with `/`.
    pub fn try_from(s: String) -> Result<Self, PathError> {
        if !s.starts_with('/') {
            return Err(PathError::NotAbsolute(s.clone()));
        }
        Ok(Self { inner: s })
    }

    /// Create without validation — caller ensures s starts with `/`.
    pub fn from_maybe_unsafe(s: String) -> Self {
        Self { inner: s }
    }

    /// Borrow this owned path as a [`AbsPath`] slice.
    pub fn as_path(&self) -> &AbsPath {
        // Safety: same invariants as AbsPath::from_absolute
        unsafe { &*(self.inner.as_str() as *const str as *const AbsPath) }
    }

    /// Borrow the path as a string slice.
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Consume the buffer and return the underlying owned `String`.
    pub fn into_string(self) -> String {
        self.inner
    }
}

impl std::fmt::Display for AbsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.inner)
    }
}

impl std::fmt::Display for AbsPathBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl PartialEq for AbsPath {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl PartialEq for AbsPathBuf {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for AbsPathBuf {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abs_path_accepts_absolute() {
        let p = AbsPath::from_absolute("/tmp/foo").unwrap();
        assert_eq!(p.as_str(), "/tmp/foo");
    }

    #[test]
    fn abs_path_rejects_relative() {
        let result = AbsPath::from_absolute("relative/path");
        assert_eq!(
            result,
            Err(PathError::NotAbsolute("relative/path".to_string()))
        );
    }

    #[test]
    fn abs_path_buf_try_from() {
        let p = AbsPathBuf::try_from("/tmp/bar".to_string()).unwrap();
        assert_eq!(p.as_str(), "/tmp/bar");
    }

    #[test]
    fn abs_path_buf_rejects_relative() {
        let result = AbsPathBuf::try_from("relative".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn abs_path_to_buf() {
        let p = AbsPath::from_absolute("/tmp/test").unwrap();
        let buf = p.to_buf();
        assert_eq!(buf.as_str(), "/tmp/test");
    }

    #[test]
    fn abs_path_eq() {
        let a = AbsPath::from_absolute("/a").unwrap();
        let b = AbsPath::from_absolute("/a").unwrap();
        let c = AbsPath::from_absolute("/b").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn abs_path_buf_eq() {
        let a = AbsPathBuf::try_from("/a".to_string()).unwrap();
        let b = AbsPathBuf::try_from("/a".to_string()).unwrap();
        let c = AbsPathBuf::try_from("/b".to_string()).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
