//! `AssistCatalog` — registry of all assist handlers.

use super::{AssistHandler, AssistId};
use std::collections::BTreeMap;

/// Registry mapping assist IDs to handler functions.
#[derive(Debug, Default)]
pub struct AssistCatalog {
    handlers: BTreeMap<AssistId, AssistHandler>,
}

impl AssistCatalog {
    /// Create an empty catalog with no handlers registered.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an assist handler.
    pub fn register(&mut self, id: AssistId, handler: AssistHandler) {
        self.handlers.insert(id, handler);
    }

    /// Get a handler by ID.
    pub fn get(&self, id: AssistId) -> Option<&AssistHandler> {
        self.handlers.get(id)
    }

    /// Get all registered handler IDs.
    pub fn ids(&self) -> Vec<AssistId> {
        self.handlers.keys().copied().collect()
    }

    /// Get the total count of registered handlers.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Returns `true` if no handlers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}
