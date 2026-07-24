//! Thread-local QueryCursor pool for tree-sitter.
//!
//! Eliminates per-query allocation by reusing QueryCursors.
//! Each cursor is reset via `set_match_limit` before reuse.
//!
//! # Usage
//!
//! ```ignore
//! use crate::cursor_pool::with_cursor;
//!
//! let matches = with_cursor(|cursor| {
//!     cursor.matches(&query, tree.root_node(), source.as_bytes())
//!         .collect::<Vec<_>>()
//! });
//! ```

use std::cell::RefCell;
use touring_code::ast::tree_sitter::QueryCursor;

thread_local! {
    static CURSOR_POOL: RefCell<Vec<QueryCursor>> = const { RefCell::new(Vec::new()) };
}

/// Execute a closure with a pooled `QueryCursor`.
///
/// Borrows a cursor from the thread-local pool (or creates a new one),
/// executes the closure, and returns the cursor to the pool.
///
/// The cursor has `set_match_limit(256)` applied by default to prevent
/// pathological queries from consuming unbounded memory.
pub fn with_cursor<F, R>(f: F) -> R
where
    F: FnOnce(&mut QueryCursor) -> R,
{
    let mut cursor = CURSOR_POOL
        .with(|pool| pool.borrow_mut().pop())
        .unwrap_or_default();
    cursor.set_match_limit(256);
    let result = f(&mut cursor);
    CURSOR_POOL.with(|pool| pool.borrow_mut().push(cursor));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_cursor_creates_and_returns() {
        // First call creates a new cursor
        let result = with_cursor(|_cursor| 42);
        assert_eq!(result, 42);

        // Second call reuses the pooled cursor
        let result2 = with_cursor(|_cursor| 99);
        assert_eq!(result2, 99);
    }

    #[test]
    fn test_pool_reuse() {
        // Force 3 cursors into the pool
        with_cursor(|_| {});
        with_cursor(|_| {});
        with_cursor(|_| {});

        // Pool should have cursors available (no way to check count,
        // but this exercises the reuse path)
        let result = with_cursor(|_cursor| "reused");
        assert_eq!(result, "reused");
    }
}
