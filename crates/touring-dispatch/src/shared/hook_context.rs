// HookContext — Unified context struct for all Touring hooks.
// Enables consistent context propagation and hook chaining.

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

use crate::knowledge::FileKnowledgeDB;
use crate::shared::session_bus::SessionBus;

/// Metadata about the hook execution context.
#[derive(Clone, Debug)]
pub struct HookMeta {
    pub timestamp: DateTime<Utc>,
    pub session_id: Uuid,
    pub file_path: Option<PathBuf>,
    pub tool_name: Option<String>,
}

impl HookMeta {
    pub fn now() -> Self {
        Self {
            timestamp: Utc::now(),
            session_id: Uuid::nil(),
            file_path: None,
            tool_name: None,
        }
    }

    pub fn with_session_id(mut self, session_id: Uuid) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_file_path(mut self, file_path: PathBuf) -> Self {
        self.file_path = Some(file_path);
        self
    }

    pub fn with_tool_name(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }
}

/// Shared services available to hooks.
#[derive(Clone, Debug)]
pub struct HookServices<'a> {
    pub knowledge: &'a FileKnowledgeDB,
    pub session_bus: &'a SessionBus,
}

/// Unified context passed to all hook handlers.
///
/// Provides:
/// - `hook_name`: hook identifier ("pre_read", "pre_edit", etc.)
/// - `payload`: raw JSON input
/// - `last`: output from previous hook in the chain
/// - `meta`: execution metadata
/// - `services`: shared services (knowledge DB, symbol index, session bus)
#[derive(Clone, Debug)]
pub struct HookContext<'a> {
    pub hook_name: &'static str,
    pub payload: &'a Value,
    pub last: Option<Value>,
    pub meta: HookMeta,
    pub services: HookServices<'a>,
}

impl<'a> HookContext<'a> {
    /// Create a new HookContext for a hook.
    pub fn new(hook_name: &'static str, payload: &'a Value, services: HookServices<'a>) -> Self {
        Self {
            hook_name,
            payload,
            last: None,
            meta: HookMeta::now(),
            services,
        }
    }

    /// Create with a pre-populated `last` field (for chained hooks).
    pub fn with_last(mut self, last: Value) -> Self {
        self.last = Some(last);
        self
    }

    /// Extract a string field from the payload.
    pub fn get_str(&self, key: &str) -> Option<&'a str> {
        self.payload.get(key).and_then(|v| v.as_str())
    }

    /// Extract a u64 field from the payload.
    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.payload.get(key).and_then(|v| v.as_u64())
    }

    /// Extract a bool field from the payload.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.payload.get(key).and_then(|v| v.as_bool())
    }

    /// Extract a f64 field from the payload.
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.payload.get(key).and_then(|v| v.as_f64())
    }

    /// Extract an i64 field from the payload.
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.payload.get(key).and_then(|v| v.as_i64())
    }

    /// Check if payload has a key.
    pub fn has_key(&self, key: &str) -> bool {
        self.payload.get(key).is_some()
    }

    /// Chain completion reward — inject RL reward when full chain completes.
    ///
    /// Called by the final hook in a chain (e.g., `post_edit` after `pre_edit`).
    /// Rewards the full chain path, not individual steps.
    pub fn chain_completion_reward(&self, quality_score: f64) -> Option<(String, f64)> {
        // Only reward from the terminal hook of a chain
        match self.hook_name {
            "post_edit" | "post_write" | "post_bash" | "session_stop" => Some((
                format!("chain_completion:{}", self.hook_name),
                quality_score,
            )),
            _ => None,
        }
    }
}

// Note: tests require a real HookRuntime + FileKnowledgeDB + SessionBus
// which is only available in integration tests. Unit tests skipped for now.
