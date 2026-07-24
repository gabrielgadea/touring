//! Handler trait — the contract every Cortex hook handler implements.

use crate::context::CortexContext;
use crate::types::{HandlerResult, HookEvent};

/// Every hook handler implements this trait.
///
/// Handlers are registered in the pipeline and executed in order for matching events.
/// They must be `Send + Sync` for thread safety.
pub trait Handler: Send + Sync {
    /// Handler name for logging and metrics.
    fn name(&self) -> &str;

    /// Which events this handler cares about.
    fn events(&self) -> &[HookEvent];

    /// Optional tool matcher pattern (e.g., "Read", "Write|Edit|MultiEdit", "Bash").
    ///
    /// - `None` means this handler matches ALL tools for its events.
    /// - `Some(pattern)` means only execute when the current tool matches.
    ///   Pattern is pipe-separated: "Write|Edit|MultiEdit".
    fn tool_matcher(&self) -> Option<&str> {
        None
    }

    /// Whether this handler runs asynchronously (fire-and-forget).
    ///
    /// Async handlers execute after all sync handlers complete.
    /// Their results are still merged but they cannot block the pipeline.
    fn is_async(&self) -> bool {
        false
    }

    /// E5-S1: Handler execution priority (lower = runs first).
    ///
    /// Default is 128 (middle). Override to control ordering:
    /// - 0-31: critical infrastructure (session, classification)
    /// - 32-63: high priority (enforcement, security)
    /// - 64-127: normal priority (enrichment, context)
    /// - 128-191: default (most handlers)
    /// - 192-255: low priority (metrics, learning, evolution)
    fn priority(&self) -> u8 {
        128
    }

    /// E4-S3/E5-S4: Whether this handler is critical (never skipped on budget exhaustion).
    ///
    /// Critical handlers always execute regardless of remaining context budget.
    /// Override for handlers that must always run (e.g., security enforcement, blocking).
    fn is_critical(&self) -> bool {
        false
    }

    /// E5-S6: Whether this handler requires cache invalidation after execution.
    ///
    /// Handlers that modify the file system or knowledge DB (e.g., FileChanged)
    /// should return true so the Pipeline clears the filter_cache on the next call.
    fn requires_cache_invalidation(&self) -> bool {
        false
    }

    /// E4-S2: Dependency tier for graceful degradation.
    ///
    /// When a required resource is unavailable, handlers in higher tiers
    /// are skipped gracefully instead of erroring:
    /// - T0: No dependencies — always works (pure logic, input parsing)
    /// - T1: Requires knowledge DB (file metadata, symbols, imports)
    /// - T2: Requires RLM (QTable, Wilson scores, memory entries)
    /// - T3: Requires semantic recall (FTS5 chunks)
    fn dependency_tier(&self) -> u8 {
        0 // T0: no dependencies by default
    }

    /// E5-S5: Handler execution timeout in milliseconds.
    ///
    /// If execution exceeds this timeout, the circuit breaker is tripped.
    /// Default is 50ms. Override for handlers that legitimately take longer
    /// (e.g., external API calls, heavy computation).
    fn timeout_ms(&self) -> u64 {
        50
    }

    /// Execute the handler, returning a result.
    ///
    /// MUST NOT call `std::process::exit()` or print to stdout directly.
    /// Return a `HandlerResult` instead.
    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult;
}
