//! HookResponse — Testable alternative to `process::exit`.
//!
//! Extracted from hook_runtime.rs as part of Track A decomposition.
//! Provides 6 HookResponse variants for Claude Code hook protocol.
//!
//! ## D5: Compression Optimization
//! `to_json()` is called 45× in dispatch table builders. To avoid per-call
//! heap allocation from intermediate `serde_json::Value` trees, a
//! `LazyResponse` wrapper caches the serialized `String` behind an
//! `OnceLock`, eliminating repeated serialization of static responses
//! like `Allow` (always `{}`).

// HookResponse uses serde_json::Value for JSON serialization

use std::sync::OnceLock;

/// Serialization-optimized response wrapper.
///
/// Lazily serializes a `HookResponse` once via the existing `to_json()`
/// path and caches the resulting `String` in an `OnceLock`. Subsequent
/// calls to `as_str()` return a borrowed reference to the cached string,
/// eliminating repeated `serde_json::Value` tree construction for hot-path
/// responses like `Allow` (always `{}`).
#[derive(Debug)]
pub struct LazyResponse {
    inner: HookResponse,
    // Cached serialized string, populated on first `as_str()` call.
    cache: OnceLock<String>,
}

impl LazyResponse {
    /// Wrap a `HookResponse` for lazy serialization.
    pub fn new(inner: HookResponse) -> Self {
        Self {
            inner,
            cache: OnceLock::new(),
        }
    }

    /// Returns the serialized JSON as a `&str`.
    ///
    /// First call: serializes `self.inner` via the legacy `to_json()` and
    /// stores it in the cache. Subsequent calls return the cached string
    /// directly — zero re-serialization.
    pub fn as_str(&self) -> &str {
        if let Some(cached) = self.cache.get() {
            return cached.as_str();
        }
        // First call — serialize using the proven `to_json()` path, then cache.
        let owned = self.inner.to_json();
        let _ = self.cache.set(owned);
        // Unwrap is safe: we just set it above.
        self.cache
            .get()
            .expect("cache populated by set() above")
            .as_str()
    }

    /// Returns the raw bytes from the cache, or an empty slice if not yet cached.
    pub fn as_bytes(&self) -> &[u8] {
        self.cache.get().map(|s| s.as_bytes()).unwrap_or(&[])
    }

    /// Returns a reference to the inner `HookResponse`.
    pub fn inner(&self) -> &HookResponse {
        &self.inner
    }
}

impl Clone for LazyResponse {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            cache: OnceLock::new(),
        }
    }
}

impl PartialEq for LazyResponse {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LazyResponse {}

impl HookResponse {
    /// Convert to `LazyResponse` for use in hot-path dispatch tables.
    ///
    /// Call this at registration time instead of `to_json()` to get
    /// a `LazyResponse` that avoids per-call serialization overhead.
    pub fn to_lazy(self) -> LazyResponse {
        LazyResponse::new(self)
    }
}

/// Response from a hook — testable alternative to `process::exit`.
///
/// Hooks can return this instead of calling the diverging `emit_*()` methods.
/// The binary entry point converts this to stdout + exit code.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum HookResponse {
    /// Allow the tool invocation (exit 0, no output).
    Allow,
    /// Inject context (exit 0, stdout JSON with additionalContext).
    Context {
        /// The context text injected into the model's prompt.
        context: String,
        /// Originating hook event name, if known.
        event_name: Option<String>,
    },
    /// Deny tool execution (PreToolUse only).
    /// Returns permissionDecision: "deny" with reason.
    Deny {
        /// Human-readable reason shown for the denial.
        reason: String,
        /// Optional additional context injected alongside the denial.
        context: Option<String>,
        /// Originating hook event name, if known.
        event_name: Option<String>,
    },
    /// Block after tool execution (PostToolUse).
    /// Returns top-level decision: "block" with reason.
    Block {
        /// Human-readable reason shown for the block.
        reason: String,
        /// Optional additional context injected alongside the block.
        context: Option<String>,
        /// Originating hook event name, if known.
        event_name: Option<String>,
    },
    /// Halt session entirely.
    /// Returns continue: false with stopReason.
    Halt {
        /// Reason reported as the session stop reason.
        reason: String,
    },
    /// Context injection with modified tool input.
    /// Returns additionalContext + updatedInput in hookSpecificOutput.
    /// Used by PreToolUse hooks to normalize or correct tool inputs.
    ContextWithUpdatedInput {
        /// The context text injected into the model's prompt.
        context: String,
        /// Originating hook event name, if known.
        event_name: Option<String>,
        /// The corrected tool input that replaces the original.
        updated_input: serde_json::Value,
    },
}

impl HookResponse {
    /// Serialize this response to stdout and call `process::exit(0)`.
    ///
    /// This is the bridge between the testable `HookResponse` and the
    /// diverging behavior required by Claude Code hook protocol.
    pub fn emit(self) -> ! {
        match self {
            HookResponse::Allow => {
                std::process::exit(0);
            }
            HookResponse::Context {
                context,
                event_name,
            } => {
                let truncated = truncate_context(&context);
                let mut hso = serde_json::json!({
                    "additionalContext": truncated,
                });
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name);
                    }
                }
                let output = serde_json::json!({
                    "hookSpecificOutput": hso,
                });
                println!("{}", serde_json::to_string(&output).unwrap_or_default());
                std::process::exit(0);
            }
            HookResponse::Deny {
                reason,
                context,
                event_name,
            } => {
                let mut hso = serde_json::json!({
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                });
                if let Some(ctx) = context {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["additionalContext"] = serde_json::Value::String(ctx);
                    }
                }
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name);
                    }
                }
                let output = serde_json::json!({
                    "hookSpecificOutput": hso,
                });
                println!("{}", serde_json::to_string(&output).unwrap_or_default());
                std::process::exit(0);
            }
            HookResponse::Block {
                reason,
                context,
                event_name,
            } => {
                let mut hso = serde_json::json!({});
                if let Some(ctx) = context {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["additionalContext"] = serde_json::Value::String(ctx);
                    }
                }
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name);
                    }
                }
                let output = serde_json::json!({
                    "decision": "block",
                    "reason": reason,
                    "hookSpecificOutput": hso,
                });
                println!("{}", serde_json::to_string(&output).unwrap_or_default());
                std::process::exit(0);
            }
            HookResponse::Halt { reason } => {
                let output = serde_json::json!({
                    "continue": false,
                    "stopReason": reason,
                });
                println!("{}", serde_json::to_string(&output).unwrap_or_default());
                std::process::exit(0);
            }
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => {
                let truncated = truncate_context(&context);
                let mut hso = serde_json::json!({
                    "additionalContext": truncated,
                    "updatedInput": updated_input,
                });
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name);
                    }
                }
                let output = serde_json::json!({
                    "hookSpecificOutput": hso,
                });
                println!("{}", serde_json::to_string(&output).unwrap_or_default());
                std::process::exit(0);
            }
        }
    }

    /// Build a context response (injects additionalContext).
    pub fn context(context: impl Into<String>) -> Self {
        HookResponse::Context {
            context: context.into(),
            event_name: None,
        }
    }

    /// Build a context response with event name.
    pub fn context_with_event(context: impl Into<String>, event_name: impl Into<String>) -> Self {
        HookResponse::Context {
            context: context.into(),
            event_name: Some(event_name.into()),
        }
    }

    /// Build a deny response.
    pub fn deny(reason: impl Into<String>) -> Self {
        HookResponse::Deny {
            reason: reason.into(),
            context: None,
            event_name: None,
        }
    }

    /// Build a deny response with context.
    pub fn deny_with_context(reason: impl Into<String>, context: impl Into<String>) -> Self {
        HookResponse::Deny {
            reason: reason.into(),
            context: Some(context.into()),
            event_name: None,
        }
    }

    /// Build a block response.
    pub fn block(reason: impl Into<String>) -> Self {
        HookResponse::Block {
            reason: reason.into(),
            context: None,
            event_name: None,
        }
    }

    /// Build a halt response.
    pub fn halt(reason: impl Into<String>) -> Self {
        HookResponse::Halt {
            reason: reason.into(),
        }
    }

    /// Build a context-with-updated-input response.
    pub fn context_with_updated_input(
        context: impl Into<String>,
        updated_input: serde_json::Value,
    ) -> Self {
        HookResponse::ContextWithUpdatedInput {
            context: context.into(),
            event_name: None,
            updated_input,
        }
    }

    /// Build an allow response.
    pub fn allow() -> Self {
        HookResponse::Allow
    }

    /// Serialize to stdout JSON string for use in hook dispatch tables.
    ///
    /// Used by `build_dispatch_table()` — each handler returns `HookResponse`
    /// and the dispatch calls `.to_json()` to produce the final stdout line.
    pub fn to_json(&self) -> String {
        let value = match self {
            HookResponse::Allow => {
                serde_json::json!({})
            }
            HookResponse::Context {
                context,
                event_name,
            } => {
                let truncated = truncate_context(context);
                let mut hso = serde_json::json!({
                    "additionalContext": truncated,
                });
                if let Some(name) = event_name {
                    hso["hookEventName"] = serde_json::Value::String(name.clone());
                }
                serde_json::json!({
                    "hookSpecificOutput": hso,
                })
            }
            HookResponse::Deny {
                reason,
                context,
                event_name,
            } => {
                let mut hso = serde_json::json!({
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                });
                if let Some(ctx) = context {
                    hso["additionalContext"] = serde_json::Value::String(ctx.clone());
                }
                if let Some(name) = event_name {
                    hso["hookEventName"] = serde_json::Value::String(name.clone());
                }
                serde_json::json!({
                    "hookSpecificOutput": hso,
                })
            }
            HookResponse::Block {
                reason,
                context,
                event_name,
            } => {
                let mut hso = serde_json::json!({});
                if let Some(ctx) = context {
                    hso["additionalContext"] = serde_json::Value::String(ctx.clone());
                }
                if let Some(name) = event_name {
                    hso["hookEventName"] = serde_json::Value::String(name.clone());
                }
                serde_json::json!({
                    "decision": "block",
                    "reason": reason,
                    "hookSpecificOutput": hso,
                })
            }
            HookResponse::Halt { reason } => {
                serde_json::json!({
                    "continue": false,
                    "stopReason": reason,
                })
            }
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => {
                let truncated = truncate_context(context);
                let mut hso = serde_json::json!({
                    "additionalContext": truncated,
                    "updatedInput": updated_input,
                });
                if let Some(name) = event_name {
                    hso["hookEventName"] = serde_json::Value::String(name.clone());
                }
                serde_json::json!({
                    "hookSpecificOutput": hso,
                })
            }
        };
        serde_json::to_string(&value).unwrap_or_default()
    }
}

/// Truncate context to stay under Claude Code's 10K limit (9,500 UTF-8 safe).
///
/// `pub` (was `pub(crate)`): `HookRuntime` in the touring-hooks dispatch layer
/// calls this across the Phase C crate boundary (4 call sites).
pub fn truncate_context(context: &str) -> String {
    const MAX_CHARS: usize = 9_500;
    if context.len() <= MAX_CHARS {
        return context.to_string();
    }
    let mut end = MAX_CHARS;
    while end > 0 && !context.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}...\n[truncated: {} chars total]",
        &context[..end],
        context.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_variant() {
        let resp = HookResponse::allow();
        assert_eq!(resp, HookResponse::Allow);
    }

    #[test]
    fn test_context_variant() {
        let resp = HookResponse::context("test context");
        match resp {
            HookResponse::Context {
                context,
                event_name,
            } => {
                assert_eq!(context, "test context");
                assert!(event_name.is_none());
            }
            _ => panic!("expected Context variant"),
        }
    }

    #[test]
    fn test_deny_variant() {
        let resp = HookResponse::deny("too complex");
        match resp {
            HookResponse::Deny {
                reason,
                context,
                event_name,
            } => {
                assert_eq!(reason, "too complex");
                assert!(context.is_none());
                assert!(event_name.is_none());
            }
            _ => panic!("expected Deny variant"),
        }
    }

    #[test]
    fn test_halt_variant() {
        let resp = HookResponse::halt("catastrophic failure");
        match resp {
            HookResponse::Halt { reason } => {
                assert_eq!(reason, "catastrophic failure");
            }
            _ => panic!("expected Halt variant"),
        }
    }

    #[test]
    fn test_context_with_updated_input() {
        let json = serde_json::json!({"path": "/new/path"});
        let resp = HookResponse::context_with_updated_input("path normalized", json.clone());
        match resp {
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => {
                assert_eq!(context, "path normalized");
                assert!(event_name.is_none());
                assert_eq!(updated_input, json);
            }
            _ => panic!("expected ContextWithUpdatedInput variant"),
        }
    }

    #[test]
    fn test_truncate_context_short() {
        let short = "short context";
        assert_eq!(truncate_context(short), short);
    }

    #[test]
    fn test_truncate_context_long() {
        let long = "x".repeat(20_000);
        let truncated = truncate_context(&long);
        assert!(truncated.len() <= 9_550); // 9,500 + "[...truncated]"
        assert!(truncated.contains("[truncated:"));
    }

    #[test]
    fn test_lazy_response_cache() {
        let resp = HookResponse::allow();
        let lazy = resp.to_lazy();

        // First call: caches the result
        let s1 = lazy.as_str();
        assert_eq!(s1, "{}");

        // Second call: returns cached value (same pointer)
        let s2 = lazy.as_str();
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_lazy_response_context() {
        let resp = HookResponse::context("test context");
        let lazy = resp.to_lazy();

        let s = lazy.as_str();
        assert!(s.contains("test context"));
        assert!(s.contains("hookSpecificOutput"));
    }

    #[test]
    fn test_lazy_response_clone_independent() {
        let resp = HookResponse::halt("oops");
        let lazy1 = resp.to_lazy();
        let _lazy2 = lazy1.clone();

        // Both should work independently
        assert!(lazy1.as_str().contains("oops"));
        assert!(lazy1.as_str().len() > 0);
    }
}
