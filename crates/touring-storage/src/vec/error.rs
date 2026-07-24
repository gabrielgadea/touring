use thiserror::Error;

/// Errors that can occur when operating on a vector store.
#[derive(Debug, Error)]
pub enum VectorStoreError {
    /// No vector-store backend feature was enabled at compile time.
    #[error(
        "backend not available — enable one of the feature flags: sqlite-vec, qdrant, or in-memory"
    )]
    BackendNotAvailable,

    /// Establishing a connection to the backend failed.
    #[error("failed to connect to backend: {0}")]
    ConnectionFailed(String),

    /// The requested collection does not exist in the backend.
    #[error("collection not found: {0}")]
    CollectionNotFound(String),

    /// Inserting or updating points in the backend failed.
    #[error("failed to upsert points: {0}")]
    UpsertFailed(String),

    /// A similarity search against the backend failed.
    #[error("search failed: {0}")]
    SearchFailed(String),

    /// Deleting points from the backend failed.
    #[error("delete failed: {0}")]
    DeleteFailed(String),

    /// A vector's length did not match the collection's configured dimension.
    #[error("invalid dimension: expected {expected} but got {actual}")]
    InvalidDimension {
        /// Dimension the collection expects.
        expected: usize,
        /// Dimension actually supplied by the caller.
        actual: usize,
    },

    /// Persisting the collection to or loading it from disk failed.
    #[error("persistence error: {0}")]
    PersistenceError(String),
}
