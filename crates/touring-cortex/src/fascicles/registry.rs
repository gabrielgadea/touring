//! Registry — fascicle registration and lifecycle management
//!
//! Manages the registration, activation, and deactivation of fascicle instances.
//! Provides O(1) lookup via moka W-TinyLFU cache for handler resolution.

use moka::sync::Cache;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::fascicles::channels::DirectChannel;
use crate::fascicles::evidence::Evidence;

/// Handler name identifier — used for O(1) lookup in the registry.
pub type HandlerName = String;

/// Handler channel wrapper — Arc-wrapped channel for Clone semantics.
/// DirectChannel contains Receiver which is not Clone, so we wrap in Arc.
pub type HandlerChannel = Arc<DirectChannel<Evidence>>;

/// HandlerRegistry — central registry for fascicle handler management.
///
/// Provides thread-safe O(1) lookup of handlers via moka W-TinyLFU cache.
/// Replaces linear scan of all Cortex handlers.
///
/// Uses moka's W-TinyLFU eviction policy for optimal performance under
/// Zipfian workloads (benchmark score 92.1).
#[derive(Debug)]
pub struct HandlerRegistry {
    /// W-TinyLFU cache for handlers (name → channel).
    handlers: Cache<HandlerName, HandlerChannel>,
    /// Manual entry count since moka doesn't expose len().
    count: AtomicUsize,
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HandlerRegistry {
    /// Creates a new empty registry.
    ///
    /// Default capacity of 10_000 entries with no TTL (permanent cache).
    pub fn new() -> Self {
        Self {
            handlers: Cache::new(10_000),
            count: AtomicUsize::new(0),
        }
    }

    /// Registers a handler with the given name.
    ///
    /// Returns the previous handler if one was already registered under this name.
    pub fn register(&self, name: HandlerName, channel: HandlerChannel) -> Option<HandlerChannel> {
        // Get existing value BEFORE inserting (moka insert returns () not old value)
        let old = self.handlers.get(&name).map(|v| Arc::clone(&v));
        let existed = old.is_some();

        self.handlers.insert(name, channel);

        if !existed {
            self.count.fetch_add(1, Ordering::Relaxed);
        }

        old
    }

    /// Looks up a handler by name.
    ///
    /// Returns a clone of the handler channel if found.
    pub fn lookup(&self, name: &str) -> Option<HandlerChannel> {
        self.handlers.get(name).map(|v| Arc::clone(&v))
    }

    /// Returns the number of registered handlers.
    pub fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    /// Returns true if the registry contains no handlers.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new_is_empty() {
        let registry = HandlerRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_register_and_lookup() {
        let registry = HandlerRegistry::new();
        let channel: HandlerChannel = Arc::new(DirectChannel::new(256));

        let prev = registry.register("test-handler".to_string(), channel.clone());
        assert!(prev.is_none());

        let found = registry.lookup("test-handler");
        assert!(found.is_some());
    }

    #[test]
    fn test_registry_replace_existing() {
        let registry = HandlerRegistry::new();
        let channel1: HandlerChannel = Arc::new(DirectChannel::new(256));
        let channel2: HandlerChannel = Arc::new(DirectChannel::new(128));

        registry.register("handler".to_string(), channel1.clone());
        let prev = registry.register("handler".to_string(), channel2.clone());

        assert!(prev.is_some());
    }

    #[test]
    fn test_registry_lookup_missing() {
        let registry = HandlerRegistry::new();
        let found = registry.lookup("nonexistent");
        assert!(found.is_none());
    }

    #[test]
    fn test_registry_len() {
        let registry = HandlerRegistry::new();
        assert_eq!(registry.len(), 0);

        let channel: HandlerChannel = Arc::new(DirectChannel::new(256));
        registry.register("h1".to_string(), channel);
        assert_eq!(registry.len(), 1);

        let channel: HandlerChannel = Arc::new(DirectChannel::new(256));
        registry.register("h2".to_string(), channel);
        assert_eq!(registry.len(), 2);
    }
}
