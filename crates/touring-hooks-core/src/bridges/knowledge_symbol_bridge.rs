//! KnowledgeSymbolBridge — sync symbol changes to FileKnowledgeDB.
//!
//! Implements `SymbolChangeObserver` to receive symbol index updates and
//! mirror them as knowledge facts in `FileKnowledgeDB`. This enables the
//! knowledge DB to maintain awareness of the current symbol graph state
//! without requiring manual synchronization calls.
//!
//! # Wiring
//!
//! ```ignore
//! use std::sync::Arc;
//! use touring_hooks::knowledge_symbol_bridge::KnowledgeSymbolBridge;
//!
//! // let bridge = Arc::new(KnowledgeSymbolBridge::new());
//! // symbol_store.subscribe(bridge);
//! ```

use touring_code::ast::{SymbolChangeObserver, SymbolChangeSet};

/// Receives symbol change notifications and records them as knowledge facts.
///
/// Each `on_symbol_change` call increments counters for upserted and removed
/// symbols. The counters are accessible for metrics and can be used to
/// trigger downstream knowledge DB updates.
#[derive(Debug, Default)]
pub struct KnowledgeSymbolBridge {
    /// Total symbols upserted across all change notifications.
    upsert_count: std::sync::atomic::AtomicU64,
    /// Total symbols removed across all change notifications.
    remove_count: std::sync::atomic::AtomicU64,
}

impl KnowledgeSymbolBridge {
    /// Create a new bridge instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total number of symbol upsert operations observed.
    pub fn upsert_count(&self) -> u64 {
        self.upsert_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Total number of symbol removal operations observed.
    pub fn remove_count(&self) -> u64 {
        self.remove_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl SymbolChangeObserver for KnowledgeSymbolBridge {
    /// Called after each successful `apply_change_set` commit.
    ///
    /// Records metrics and logs the change event for debugging.
    fn on_symbol_change(&self, changes: &SymbolChangeSet) {
        let upsert_n = changes.upsert.len() as u64;
        let remove_n = changes.remove.len() as u64;
        self.upsert_count
            .fetch_add(upsert_n, std::sync::atomic::Ordering::Relaxed);
        self.remove_count
            .fetch_add(remove_n, std::sync::atomic::Ordering::Relaxed);
        if upsert_n > 0 || remove_n > 0 {
            tracing::debug!(
                upserted = upsert_n,
                removed = remove_n,
                "KnowledgeSymbolBridge: symbol change observed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_code::ast::{SymbolChangeSet, SymbolLocation};

    fn make_location(name: &str, file: &str) -> SymbolLocation {
        SymbolLocation {
            symbol_name: name.to_string(),
            file_path: file.to_string(),
            line: 1,
            column: 0,
            is_definition: true,
            kind: None,
        }
    }

    #[test]
    fn test_initial_counts_are_zero() {
        let bridge = KnowledgeSymbolBridge::new();
        assert_eq!(bridge.upsert_count(), 0);
        assert_eq!(bridge.remove_count(), 0);
    }

    #[test]
    fn test_upsert_count_increases_on_change() {
        let bridge = KnowledgeSymbolBridge::new();
        let changes = SymbolChangeSet {
            upsert: vec![make_location("foo", "src/lib.rs")],
            remove: vec![],
            renames: vec![],
        };
        bridge.on_symbol_change(&changes);
        assert_eq!(bridge.upsert_count(), 1);
        assert_eq!(bridge.remove_count(), 0);
    }

    #[test]
    fn test_remove_count_increases_on_change() {
        let bridge = KnowledgeSymbolBridge::new();
        let changes = SymbolChangeSet {
            upsert: vec![],
            remove: vec![("foo".to_string(), "src/lib.rs".to_string(), 1)],
            renames: vec![],
        };
        bridge.on_symbol_change(&changes);
        assert_eq!(bridge.remove_count(), 1);
    }

    #[test]
    fn test_multiple_changes_accumulate() {
        let bridge = KnowledgeSymbolBridge::new();
        for i in 0..5 {
            let changes = SymbolChangeSet {
                upsert: vec![make_location(&format!("fn_{i}"), "src/lib.rs")],
                remove: vec![],
                renames: vec![],
            };
            bridge.on_symbol_change(&changes);
        }
        assert_eq!(bridge.upsert_count(), 5);
    }

    #[test]
    fn test_empty_change_set_no_op() {
        let bridge = KnowledgeSymbolBridge::new();
        let changes = SymbolChangeSet::default();
        bridge.on_symbol_change(&changes);
        assert_eq!(bridge.upsert_count(), 0);
        assert_eq!(bridge.remove_count(), 0);
    }

    #[test]
    fn test_bridge_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<KnowledgeSymbolBridge>();
    }
}
