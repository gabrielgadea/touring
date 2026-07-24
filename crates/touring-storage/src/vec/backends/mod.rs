//! Pluggable vector store backends.

#[cfg(feature = "sqlite-vec")]
pub mod sqlite_vec;

#[cfg(feature = "qdrant")]
pub mod qdrant;

#[cfg(feature = "postgres")]
pub mod postgres;
