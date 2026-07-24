//! Pipeline — Builds and executes a handler pipeline for a given event.

use crate::circuit_breaker::CircuitBreakerRegistry;
use crate::context::CortexContext;
use crate::handler::Handler;
use crate::metrics::PipelineMetrics;
use crate::types::{CortexOutput, Decision, HandlerResult, HookEvent};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{OnceLock, RwLock};

/// E5-S3: Trait for extracting handler metadata WITHOUT instantiating the handler.
/// This allows lazy evaluation: the handler is only created on first matching event.
pub trait LazyExtractable: Send + Sync {
    /// Handler name for logging and metrics.
    fn name(&self) -> &str;
    /// Which events this handler cares about.
    fn events(&self) -> &[HookEvent];
    /// Optional tool matcher pattern.
    fn tool_matcher(&self) -> Option<&str>;
    /// Create the actual handler instance.
    fn instantiate(self) -> Box<dyn Handler>;
}

/// E5-S3: Wrapper that defers handler instantiation until first matching event.
/// H is the concrete extractor type. Uses OnceLock for exactly-once init on first access.
pub struct LazyHandler<H: LazyExtractable> {
    extractor: H,
    handler: OnceLock<Box<dyn Handler>>,
}

impl<H: LazyExtractable + Clone + Send + Sync> LazyHandler<H> {
    /// Create a new lazy handler wrapper.
    fn new(extractor: H) -> Self {
        Self {
            extractor,
            handler: OnceLock::new(),
        }
    }

    /// Get or instantiate the underlying handler.
    fn get_or_init(&self) -> &dyn Handler {
        // Clone H to move into instantiate, then forget the clone (only used once)
        // This works because the OnceLock guarantees instantiate runs exactly once
        self.handler
            .get_or_init(|| {
                // Clone: H: Clone, H: Send + Sync (from trait bounds)
                let extractor_clone = H::clone(&self.extractor);
                extractor_clone.instantiate()
            })
            .as_ref()
    }
}

impl<H: LazyExtractable + Clone + Send + Sync> Handler for LazyHandler<H> {
    fn name(&self) -> &str {
        self.extractor.name()
    }

    fn events(&self) -> &[HookEvent] {
        self.extractor.events()
    }

    fn tool_matcher(&self) -> Option<&str> {
        self.extractor.tool_matcher()
    }

    fn is_async(&self) -> bool {
        self.get_or_init().is_async()
    }

    fn priority(&self) -> u8 {
        self.get_or_init().priority()
    }

    fn is_critical(&self) -> bool {
        self.get_or_init().is_critical()
    }

    fn dependency_tier(&self) -> u8 {
        self.get_or_init().dependency_tier()
    }

    fn timeout_ms(&self) -> u64 {
        self.get_or_init().timeout_ms()
    }

    fn requires_cache_invalidation(&self) -> bool {
        self.get_or_init().requires_cache_invalidation()
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        self.get_or_init().execute(ctx)
    }
}

/// Cache key for handler filter results.
/// Cache key for handler result deduplication.
///
/// Composed of `(HookEvent, Option<tool_name>)`. Used to look up
/// previously computed `HandlerResult` entries in the LRU cache.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilterCacheKey {
    event: HookEvent,
    tool_name: Option<String>,
}

impl FilterCacheKey {
    pub(crate) fn new(event: HookEvent, tool_name: Option<&str>) -> Self {
        Self {
            event,
            tool_name: tool_name.map(String::from),
        }
    }
}

/// LRU cache for handler filter results.
///
/// Caches the indices of matching handlers per (HookEvent, tool_name) key.
/// Read-through pattern: on cache miss, computes indices and stores result.
#[derive(Debug)]
pub(crate) struct FilterCache {
    cache: RwLock<LruCache<FilterCacheKey, Vec<usize>>>,
}

impl FilterCache {
    /// Create a new FilterCache with the given capacity.
    pub(crate) fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::MIN);
        Self {
            cache: RwLock::new(LruCache::new(capacity)),
        }
    }

    /// Get cached indices, or compute and store them if missing.
    ///
    /// Uses `peek()` (read-only) for cache check to avoid requiring `&mut self`
    /// on the underlying LruCache, then acquires write lock only for insertion.
    pub(crate) fn get_or_compute<F>(&self, key: FilterCacheKey, compute: F) -> Vec<usize>
    where
        F: FnOnce() -> Vec<usize>,
    {
        // Fast path: read-lock peek (no LRU update)
        {
            let cache = self.cache.read().expect("RwLock poisoned");
            if let Some(indices) = cache.peek(&key) {
                let result = indices.clone();
                drop(cache); // release read lock before touch() acquires write lock
                // EC62: E5-S7 — update LRU order after peek() hit so hot entries
                // are not evicted by cold entries inserted after this read.
                self.touch(&key);
                return result;
            }
        }
        // Slow path: compute, then write-lock insert
        let indices = compute();
        let mut cache = self.cache.write().expect("RwLock poisoned");
        cache.put(key, indices.clone());
        indices
    }

    /// Clear all cached entries.
    ///
    /// EC62: vestigial annotation removed — called from Pipeline::execute() at
    /// lines 377 and 433 (E5-S6: cache invalidation after handlers that mutate state).
    pub(crate) fn clear(&self) {
        let mut cache = self.cache.write().expect("RwLock poisoned");
        cache.clear();
    }

    /// Return the number of cached entries.
    pub(crate) fn len(&self) -> usize {
        let cache = self.cache.read().expect("RwLock poisoned");
        cache.len()
    }

    /// Check if cache is empty.
    ///
    /// EC62: test-only helper — cargo check does not compile #[cfg(test)] modules,
    /// so the caller at line 1145 is invisible to the dead_code lint. Annotation
    /// is intentional: method is kept for unit test assertions on cache state.
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        let cache = self.cache.read().expect("RwLock poisoned");
        cache.is_empty()
    }

    /// E5-S7: Touch a key to update its LRU order without recomputing.
    ///
    /// Called from `get_or_compute` after a `peek()` hit so that hot entries
    /// (frequently accessed but not recently inserted) stay warm in the LRU.
    /// Without this, `peek()` hits don't move entries to the MRU position,
    /// allowing cold entries inserted later to evict them unexpectedly.
    pub(crate) fn touch(&self, key: &FilterCacheKey) {
        let mut cache = self.cache.write().expect("RwLock poisoned");
        if let Some(indices) = cache.pop(key) {
            cache.put(key.clone(), indices);
        }
    }
}

/// Builds and executes a handler pipeline for a given event.
pub struct Pipeline {
    handlers: Vec<Box<dyn Handler>>,
    filter_cache: Option<FilterCache>,
    circuit_breaker: CircuitBreakerRegistry,
}

impl std::fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cache_len = self.filter_cache.as_ref().map(|c| c.len());
        f.debug_struct("Pipeline")
            .field("handler_count", &self.handlers.len())
            .field("filter_cache_entries", &cache_len)
            .field(
                "circuit_breaker_handlers",
                &self.circuit_breaker.handler_count(),
            )
            .finish()
    }
}

impl Pipeline {
    /// Create a new empty pipeline (without filter cache).
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
            filter_cache: None,
            circuit_breaker: CircuitBreakerRegistry::new(),
        }
    }

    /// Create a new pipeline with filter cache of the given capacity.
    ///
    /// The cache stores handler indices per (HookEvent, tool_name) key,
    /// avoiding repeated linear scans on every `for_event` call.
    pub fn with_filter_cache(capacity: usize) -> Self {
        Self {
            handlers: Vec::new(),
            filter_cache: Some(FilterCache::new(capacity)),
            circuit_breaker: CircuitBreakerRegistry::new(),
        }
    }

    /// Register a handler in the pipeline.
    pub fn register(&mut self, handler: Box<dyn Handler>) {
        self.handlers.push(handler);
    }

    /// E5-S3: Register a lazy handler — instantiation is deferred until first matching event.
    ///
    /// Unlike `register()`, this does NOT increment `handler_count()` until the handler
    /// is actually instantiated on first `for_event()` call.
    pub fn register_lazy<H: LazyExtractable + Clone + Send + Sync + 'static>(
        &mut self,
        extractor: H,
    ) {
        self.handlers.push(Box::new(LazyHandler::new(extractor)));
    }

    /// Get the total number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Build a filtered list of handlers matching a specific event + tool.
    ///
    /// Returns references to matching handlers in registration order.
    ///
    /// When a filter cache is configured, results are cached per
    /// (HookEvent, tool_name) key to avoid repeated linear scans.
    pub(crate) fn for_event<'a>(
        &'a self,
        event: HookEvent,
        tool_name: Option<&str>,
    ) -> Vec<&'a dyn Handler> {
        let key = FilterCacheKey::new(event, tool_name);

        let indices = match &self.filter_cache {
            Some(cache) => {
                cache.get_or_compute(key, || self.compute_filter_indices(event, tool_name))
            }
            None => self.compute_filter_indices(event, tool_name),
        };

        indices
            .iter()
            .filter_map(|&i| self.handlers.get(i))
            .map(|h| h.as_ref())
            .collect()
    }

    /// Compute handler indices that match the given event + tool.
    fn compute_filter_indices(&self, event: HookEvent, tool_name: Option<&str>) -> Vec<usize> {
        self.handlers
            .iter()
            .enumerate()
            .filter(|(_, h)| {
                // Handler must care about this event
                if !h.events().contains(&event) {
                    return false;
                }

                // If handler has a tool_matcher AND context has a tool_name,
                // the tool must match
                if let (Some(matcher), Some(tool)) = (h.tool_matcher(), tool_name) {
                    matcher.split('|').any(|p| p.trim() == tool)
                } else {
                    // No matcher = matches all tools
                    // No tool in context = matches (non-tool events like SessionStart)
                    true
                }
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Run one handler and record its outcome: execute it, measure elapsed time,
    /// feed the circuit breaker (timeout vs success), and record pipeline metrics.
    ///
    /// Returns the completed [`HandlerResult`] (with `duration_ms` and
    /// `handler_name` populated) for the caller to merge. Shared verbatim by the
    /// sync and async handler loops so their execute/measure/record path stays
    /// identical.
    fn run_and_record(
        &self,
        handler: &dyn Handler,
        ctx: &mut CortexContext,
        pipeline_metrics: &mut PipelineMetrics,
    ) -> HandlerResult {
        let start = std::time::Instant::now();
        let mut result = handler.execute(ctx);
        let elapsed_ms = start.elapsed().as_millis() as u64;
        result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        result.handler_name = handler.name().to_string();

        // E5-S5: Record timeout or success for circuit breaker
        if elapsed_ms > handler.timeout_ms() {
            self.circuit_breaker.record_timeout(handler.name());
        } else {
            self.circuit_breaker.record_success(handler.name());
        }

        let decision_str = match &result.decision {
            Decision::Allow => "allow",
            Decision::Block(_) => "block",
            Decision::Skip => "skip",
        };
        let produced_context = !result.context_lines.is_empty();
        pipeline_metrics.record(
            handler.name(),
            result.duration_ms,
            decision_str,
            produced_context,
        );
        result
    }

    /// Execute all matching handlers in order.
    ///
    /// 1. Filter handlers for the event + tool
    /// 2. Sort by priority (lower = first) — E5-S1
    /// 3. Check dependency tier availability — E4-S2
    /// 4. Skip non-critical handlers when budget exhausted — E4-S3
    /// 5. Execute sync handlers in order (if any returns Block, stop)
    /// 6. Execute async handlers (fire-and-forget style)
    /// 7. Return consolidated output
    pub(crate) fn execute(&self, ctx: &mut CortexContext) -> CortexOutput {
        let tool_name = ctx.tool_name.as_deref();
        let mut matching = self.for_event(ctx.event, tool_name);

        // E5-S1: Sort handlers by priority (lower = first, stable sort preserves
        // registration order for same-priority handlers)
        matching.sort_by_key(|h| h.priority());

        // Separate sync and async handlers
        let (sync_handlers, async_handlers): (Vec<_>, Vec<_>) =
            matching.into_iter().partition(|h| !h.is_async());

        // E4-S2: Check which dependency tiers are available
        let has_knowledge = true; // knowledge DB is always passed via Arc
        let has_rlm = ctx.rlm.is_some();
        let has_recall = ctx.recall.is_some();

        let mut pipeline_metrics = PipelineMetrics::new();

        // Execute sync handlers — stop on Block
        for handler in sync_handlers {
            // E4-S2: Graceful degradation — skip handlers whose dependencies are unavailable
            let tier = handler.dependency_tier();
            if (tier >= 1 && !has_knowledge)
                || (tier >= 2 && !has_rlm)
                || (tier >= 3 && !has_recall)
            {
                pipeline_metrics.record(handler.name(), 0.0, "skip_degraded", false);
                continue;
            }

            // E4-S3: Budget overflow protection — skip non-critical handlers when budget is 0
            if ctx.context_budget_remaining == 0 && !handler.is_critical() {
                pipeline_metrics.record(handler.name(), 0.0, "skip_budget", false);
                continue;
            }

            // E5-S5: Circuit breaker — skip if handler is in cooldown
            if self.circuit_breaker.should_skip(handler.name()) {
                pipeline_metrics.record(handler.name(), 0.0, "skip_circuit", false);
                continue;
            }

            let result = self.run_and_record(handler, ctx, &mut pipeline_metrics);

            let is_block = matches!(result.decision, Decision::Block(_));
            ctx.merge_result(result);

            // E5-S6: If handler requires cache invalidation, clear the filter cache
            if ctx.needs_cache_invalidation {
                if let Some(ref cache) = self.filter_cache {
                    cache.clear();
                }
                ctx.needs_cache_invalidation = false;
            }

            if is_block {
                let mut output = ctx.take_output();
                output.metrics = Some(pipeline_metrics.to_json());
                return output;
            }
        }

        // Execute async handlers (still sequential in CLI model, but don't block on error)
        for handler in async_handlers {
            // E4-S2: Same graceful degradation for async handlers
            let tier = handler.dependency_tier();
            if (tier >= 1 && !has_knowledge)
                || (tier >= 2 && !has_rlm)
                || (tier >= 3 && !has_recall)
            {
                pipeline_metrics.record(handler.name(), 0.0, "skip_degraded", false);
                continue;
            }

            // E5-S5: Circuit breaker — skip if handler is in cooldown
            if self.circuit_breaker.should_skip(handler.name()) {
                pipeline_metrics.record(handler.name(), 0.0, "skip_circuit", false);
                continue;
            }

            let result = self.run_and_record(handler, ctx, &mut pipeline_metrics);

            ctx.merge_result(result);

            // E5-S6: If handler requires cache invalidation, clear the filter cache
            if ctx.needs_cache_invalidation {
                if let Some(ref cache) = self.filter_cache {
                    cache.clear();
                }
                ctx.needs_cache_invalidation = false;
            }
        }

        // Single Source of Truth: delegate to CortexContext::take_output()
        // which handles hookSpecificOutput vs systemMessage fallback correctly
        // for ALL event types (fixes missing systemMessage for SessionStart, Stop, etc.)
        let mut output = ctx.take_output();
        output.metrics = Some(pipeline_metrics.to_json());
        output
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HandlerResult;
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    #[allow(clippy::arc_with_non_send_sync)] // single-threaded test context
    fn make_test_knowledge() -> (TempDir, Arc<crate::runtime::KnowledgeRef>) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        (tmp, Arc::new(db))
    }

    /// A simple test handler that always allows with context.
    struct AllowHandler {
        name: &'static str,
        events: Vec<HookEvent>,
        tool_matcher: Option<&'static str>,
        async_handler: bool,
        context: Option<String>,
    }

    impl Handler for AllowHandler {
        fn name(&self) -> &str {
            self.name
        }

        fn events(&self) -> &[HookEvent] {
            &self.events
        }

        fn tool_matcher(&self) -> Option<&str> {
            self.tool_matcher
        }

        fn is_async(&self) -> bool {
            self.async_handler
        }

        fn execute(&self, _ctx: &mut CortexContext) -> HandlerResult {
            HandlerResult::allow(self.name, self.context.clone())
        }
    }

    /// A handler that blocks.
    struct BlockHandler {
        reason: String,
    }

    impl Handler for BlockHandler {
        fn name(&self) -> &str {
            "blocker"
        }

        fn events(&self) -> &[HookEvent] {
            &[HookEvent::PreToolUse]
        }

        fn execute(&self, _ctx: &mut CortexContext) -> HandlerResult {
            HandlerResult::block("blocker", self.reason.clone())
        }
    }

    #[test]
    fn test_empty_pipeline() {
        let pipeline = Pipeline::new();
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        let output = pipeline.execute(&mut ctx);
        // No handlers = no context, no block
        assert!(output.hook_specific_output.is_none());
        assert!(output.decision.is_none());
        assert!(!output.suppress_output);
    }

    #[test]
    fn test_pipeline_filtering_by_event() {
        let mut pipeline = Pipeline::new();

        pipeline.register(Box::new(AllowHandler {
            name: "pre_read",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: Some("Read"),
            async_handler: false,
            context: Some("pre_read context".to_string()),
        }));

        pipeline.register(Box::new(AllowHandler {
            name: "session",
            events: vec![HookEvent::SessionStart],
            tool_matcher: None,
            async_handler: false,
            context: Some("session context".to_string()),
        }));

        // PreToolUse should only match pre_read
        let matching = pipeline.for_event(HookEvent::PreToolUse, Some("Read"));
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].name(), "pre_read");

        // SessionStart should only match session
        let matching = pipeline.for_event(HookEvent::SessionStart, None);
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].name(), "session");

        // Stop should match nothing
        let matching = pipeline.for_event(HookEvent::Stop, None);
        assert!(matching.is_empty());
    }

    #[test]
    fn test_pipeline_filtering_by_tool() {
        let mut pipeline = Pipeline::new();

        pipeline.register(Box::new(AllowHandler {
            name: "read_handler",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: Some("Read"),
            async_handler: false,
            context: Some("read context".to_string()),
        }));

        pipeline.register(Box::new(AllowHandler {
            name: "edit_handler",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: Some("Write|Edit|MultiEdit"),
            async_handler: false,
            context: Some("edit context".to_string()),
        }));

        pipeline.register(Box::new(AllowHandler {
            name: "all_handler",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("all context".to_string()),
        }));

        // Read tool should match read_handler + all_handler
        let matching = pipeline.for_event(HookEvent::PreToolUse, Some("Read"));
        assert_eq!(matching.len(), 2);
        assert_eq!(matching[0].name(), "read_handler");
        assert_eq!(matching[1].name(), "all_handler");

        // Edit tool should match edit_handler + all_handler
        let matching = pipeline.for_event(HookEvent::PreToolUse, Some("Edit"));
        assert_eq!(matching.len(), 2);
        assert_eq!(matching[0].name(), "edit_handler");
        assert_eq!(matching[1].name(), "all_handler");

        // Write should match edit_handler (Write|Edit|MultiEdit) + all_handler
        let matching = pipeline.for_event(HookEvent::PreToolUse, Some("Write"));
        assert_eq!(matching.len(), 2);

        // Bash should only match all_handler
        let matching = pipeline.for_event(HookEvent::PreToolUse, Some("Bash"));
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].name(), "all_handler");
    }

    #[test]
    fn test_pipeline_execute_accumulates_context() {
        let mut pipeline = Pipeline::new();

        pipeline.register(Box::new(AllowHandler {
            name: "h1",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("signal1".to_string()),
        }));

        pipeline.register(Box::new(AllowHandler {
            name: "h2",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("signal2".to_string()),
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        let output = pipeline.execute(&mut ctx);
        let hso = output.hook_specific_output.unwrap();
        assert!(hso.additional_context.contains("signal1"));
        assert!(hso.additional_context.contains("signal2"));
        assert!(hso.additional_context.contains(" | "));
    }

    #[test]
    fn test_pipeline_block_stops_execution() {
        let mut pipeline = Pipeline::new();

        // This handler blocks
        pipeline.register(Box::new(BlockHandler {
            reason: "dangerous".to_string(),
        }));

        // This handler would add context — but should never execute
        pipeline.register(Box::new(AllowHandler {
            name: "should_not_run",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("this should not appear".to_string()),
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        let output = pipeline.execute(&mut ctx);
        assert_eq!(output.decision.as_deref(), Some("block"));
        assert_eq!(output.reason.as_deref(), Some("dangerous"));
        // The second handler's context should NOT be present
        if let Some(ref hso) = output.hook_specific_output {
            assert!(!hso.additional_context.contains("this should not appear"));
        }
    }

    #[test]
    fn test_pipeline_async_handlers_run_after_sync() {
        let mut pipeline = Pipeline::new();

        pipeline.register(Box::new(AllowHandler {
            name: "sync_handler",
            events: vec![HookEvent::PostToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("sync".to_string()),
        }));

        pipeline.register(Box::new(AllowHandler {
            name: "async_handler",
            events: vec![HookEvent::PostToolUse],
            tool_matcher: None,
            async_handler: true,
            context: Some("async".to_string()),
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PostToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );

        let output = pipeline.execute(&mut ctx);
        let hso = output.hook_specific_output.unwrap();
        // Both handlers ran
        assert!(hso.additional_context.contains("sync"));
        assert!(hso.additional_context.contains("async"));
    }

    #[test]
    fn test_handler_count() {
        let mut pipeline = Pipeline::new();
        assert_eq!(pipeline.handler_count(), 0);

        pipeline.register(Box::new(AllowHandler {
            name: "h1",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: None,
        }));
        assert_eq!(pipeline.handler_count(), 1);
    }

    // ── Additional pipeline tests ─────────────────────────────────────

    #[test]
    fn test_pipeline_default_equals_new() {
        let p1 = Pipeline::new();
        let p2 = Pipeline::default();
        assert_eq!(p1.handler_count(), p2.handler_count());
    }

    #[test]
    fn test_pipeline_debug_format() {
        let mut pipeline = Pipeline::new();
        pipeline.register(Box::new(AllowHandler {
            name: "h1",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: None,
        }));
        let debug = format!("{pipeline:?}");
        assert!(debug.contains("Pipeline"));
        assert!(debug.contains("handler_count"));
    }

    #[test]
    fn test_pipeline_no_matching_handlers_returns_empty() {
        let mut pipeline = Pipeline::new();
        pipeline.register(Box::new(AllowHandler {
            name: "session_handler",
            events: vec![HookEvent::SessionStart],
            tool_matcher: None,
            async_handler: false,
            context: Some("session".to_string()),
        }));
        // Ask for Stop event — no handlers registered for it
        let matching = pipeline.for_event(HookEvent::Stop, None);
        assert!(matching.is_empty());
    }

    #[test]
    fn test_pipeline_tool_matcher_pipe_separated_multi_match() {
        let mut pipeline = Pipeline::new();
        pipeline.register(Box::new(AllowHandler {
            name: "write_edit",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: Some("Write|Edit|MultiEdit"),
            async_handler: false,
            context: None,
        }));
        // All three tools should match
        assert_eq!(
            pipeline
                .for_event(HookEvent::PreToolUse, Some("Write"))
                .len(),
            1
        );
        assert_eq!(
            pipeline
                .for_event(HookEvent::PreToolUse, Some("Edit"))
                .len(),
            1
        );
        assert_eq!(
            pipeline
                .for_event(HookEvent::PreToolUse, Some("MultiEdit"))
                .len(),
            1
        );
        // Bash should not match
        assert_eq!(
            pipeline
                .for_event(HookEvent::PreToolUse, Some("Bash"))
                .len(),
            0
        );
    }

    #[test]
    fn test_pipeline_handler_no_matcher_matches_all_tools() {
        let mut pipeline = Pipeline::new();
        pipeline.register(Box::new(AllowHandler {
            name: "all_tools",
            events: vec![HookEvent::PostToolUse],
            tool_matcher: None,
            async_handler: false,
            context: None,
        }));
        // No tool_matcher means matches any tool name
        assert_eq!(
            pipeline
                .for_event(HookEvent::PostToolUse, Some("Read"))
                .len(),
            1
        );
        assert_eq!(
            pipeline
                .for_event(HookEvent::PostToolUse, Some("Bash"))
                .len(),
            1
        );
        assert_eq!(
            pipeline
                .for_event(HookEvent::PostToolUse, Some("Glob"))
                .len(),
            1
        );
        // Also matches with no tool name at all
        assert_eq!(pipeline.for_event(HookEvent::PostToolUse, None).len(), 1);
    }

    #[test]
    fn test_pipeline_handler_with_matcher_no_tool_still_matches() {
        // When tool_name is None in context (e.g. SessionStart), matcher handlers still run
        let mut pipeline = Pipeline::new();
        pipeline.register(Box::new(AllowHandler {
            name: "read_specific",
            events: vec![HookEvent::SessionStart],
            tool_matcher: Some("Read"),
            async_handler: false,
            context: None,
        }));
        // No tool_name → filter logic: if no tool in context → matches (see pipeline.rs line 61)
        let matching = pipeline.for_event(HookEvent::SessionStart, None);
        assert_eq!(matching.len(), 1);
    }

    #[test]
    fn test_pipeline_execute_metrics_present() {
        let mut pipeline = Pipeline::new();
        pipeline.register(Box::new(AllowHandler {
            name: "h1",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("ctx".to_string()),
        }));
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        let output = pipeline.execute(&mut ctx);
        // Metrics should always be present after execution
        assert!(output.metrics.is_some());
        let metrics = output.metrics.unwrap();
        assert_eq!(metrics["handlers_executed"], 1);
    }

    #[test]
    fn test_pipeline_execute_empty_returns_metrics() {
        let pipeline = Pipeline::new();
        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        let output = pipeline.execute(&mut ctx);
        // Even empty pipeline should have metrics
        assert!(output.metrics.is_some());
        let metrics = output.metrics.unwrap();
        assert_eq!(metrics["handlers_executed"], 0);
    }

    #[test]
    fn test_pipeline_sync_before_async_ordering() {
        // Sync handlers run first, then async — verify via context order
        use std::sync::{Arc as StdArc, Mutex};

        let order: StdArc<Mutex<Vec<&'static str>>> = StdArc::new(Mutex::new(Vec::new()));
        let order_clone = order.clone();

        struct OrderedHandler {
            n: &'static str,
            is_async_flag: bool,
            order: StdArc<Mutex<Vec<&'static str>>>,
        }

        impl Handler for OrderedHandler {
            fn name(&self) -> &str {
                self.n
            }
            fn events(&self) -> &[HookEvent] {
                &[HookEvent::PostToolUse]
            }
            fn is_async(&self) -> bool {
                self.is_async_flag
            }
            fn execute(&self, _ctx: &mut CortexContext) -> HandlerResult {
                self.order.lock().unwrap().push(self.n);
                HandlerResult::skip(self.n)
            }
        }

        let mut pipeline = Pipeline::new();
        // Register async first, then sync
        pipeline.register(Box::new(OrderedHandler {
            n: "async_first",
            is_async_flag: true,
            order: order.clone(),
        }));
        pipeline.register(Box::new(OrderedHandler {
            n: "sync_second",
            is_async_flag: false,
            order: order_clone,
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PostToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        pipeline.execute(&mut ctx);

        let executed = order.lock().unwrap();
        assert_eq!(executed.len(), 2);
        // Sync runs before async regardless of registration order
        assert_eq!(executed[0], "sync_second");
        assert_eq!(executed[1], "async_first");
    }

    #[test]
    fn test_pipeline_multiple_handlers_context_separator() {
        let mut pipeline = Pipeline::new();
        pipeline.register(Box::new(AllowHandler {
            name: "h1",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("alpha".to_string()),
        }));
        pipeline.register(Box::new(AllowHandler {
            name: "h2",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("beta".to_string()),
        }));
        pipeline.register(Box::new(AllowHandler {
            name: "h3",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("gamma".to_string()),
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        let output = pipeline.execute(&mut ctx);
        let hso = output.hook_specific_output.unwrap();
        // Multiple context lines joined by " | "
        assert!(hso.additional_context.contains("alpha"));
        assert!(hso.additional_context.contains("beta"));
        assert!(hso.additional_context.contains("gamma"));
        assert!(hso.additional_context.contains(" | "));
    }

    #[test]
    fn test_pipeline_block_early_exit_no_later_context() {
        let mut pipeline = Pipeline::new();

        // First: block
        pipeline.register(Box::new(BlockHandler {
            reason: "stop here".to_string(),
        }));
        // Second: would add context but should never run
        pipeline.register(Box::new(AllowHandler {
            name: "late_handler",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("late context".to_string()),
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        let output = pipeline.execute(&mut ctx);
        assert_eq!(output.decision.as_deref(), Some("block"));
        assert_eq!(output.reason.as_deref(), Some("stop here"));
        // Late handler context must NOT appear
        let ctx_text = output
            .hook_specific_output
            .as_ref()
            .map(|h| h.additional_context.as_str())
            .unwrap_or("");
        assert!(!ctx_text.contains("late context"));
    }

    #[test]
    fn test_pipeline_register_multiple_and_count() {
        let mut pipeline = Pipeline::new();
        for i in 0..5 {
            pipeline.register(Box::new(AllowHandler {
                name: "h",
                events: vec![HookEvent::SessionStart],
                tool_matcher: None,
                async_handler: false,
                context: Some(format!("ctx{i}")),
            }));
        }
        assert_eq!(pipeline.handler_count(), 5);
    }

    // ── Filter cache tests ─────────────────────────────────────────────────

    #[test]
    fn test_filter_cache_key_eq() {
        let k1 = FilterCacheKey::new(HookEvent::PreToolUse, Some("Read"));
        let k2 = FilterCacheKey::new(HookEvent::PreToolUse, Some("Read"));
        let k3 = FilterCacheKey::new(HookEvent::PreToolUse, Some("Edit"));
        let k4 = FilterCacheKey::new(HookEvent::PostToolUse, Some("Read"));
        let k5 = FilterCacheKey::new(HookEvent::PreToolUse, None);

        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
        assert_ne!(k1, k4);
        assert_ne!(k1, k5);
    }

    #[test]
    fn test_filter_cache_key_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(FilterCacheKey::new(HookEvent::PreToolUse, Some("Read")));
        set.insert(FilterCacheKey::new(HookEvent::PreToolUse, Some("Read"))); // dup
        set.insert(FilterCacheKey::new(HookEvent::PostToolUse, Some("Read")));
        set.insert(FilterCacheKey::new(HookEvent::PreToolUse, None));

        // 3 unique keys (dup collapsed)
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn test_pipeline_with_filter_cache() {
        let mut pipeline = Pipeline::with_filter_cache(100);

        pipeline.register(Box::new(AllowHandler {
            name: "read_handler",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: Some("Read"),
            async_handler: false,
            context: Some("read_ctx".to_string()),
        }));
        pipeline.register(Box::new(AllowHandler {
            name: "all_tools",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("all_ctx".to_string()),
        }));

        // First call — cache miss, computes
        let matching1 = pipeline.for_event(HookEvent::PreToolUse, Some("Read"));
        assert_eq!(matching1.len(), 2);
        assert_eq!(matching1[0].name(), "read_handler");
        assert_eq!(matching1[1].name(), "all_tools");

        // Second call — cache hit, same result
        let matching2 = pipeline.for_event(HookEvent::PreToolUse, Some("Read"));
        assert_eq!(matching2.len(), 2);
        assert_eq!(matching2[0].name(), "read_handler");
        assert_eq!(matching2[1].name(), "all_tools");

        // Different tool — cache miss, different result
        let matching3 = pipeline.for_event(HookEvent::PreToolUse, Some("Bash"));
        assert_eq!(matching3.len(), 1);
        assert_eq!(matching3[0].name(), "all_tools");
    }

    #[test]
    fn test_filter_cache_without_cache() {
        let mut pipeline = Pipeline::new(); // no cache
        pipeline.register(Box::new(AllowHandler {
            name: "h1",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("ctx".to_string()),
        }));

        // Works fine without cache
        let matching = pipeline.for_event(HookEvent::PreToolUse, Some("Bash"));
        assert_eq!(matching.len(), 1);
    }

    #[test]
    fn test_filter_cache_none_tool_name() {
        let mut pipeline = Pipeline::with_filter_cache(10);
        pipeline.register(Box::new(AllowHandler {
            name: "session",
            events: vec![HookEvent::SessionStart],
            tool_matcher: Some("Read"),
            async_handler: false,
            context: None,
        }));

        // SessionStart has no tool_name — handler with matcher still runs
        let matching = pipeline.for_event(HookEvent::SessionStart, None);
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].name(), "session");
    }

    #[test]
    fn test_filter_cache_debug() {
        let mut pipeline = Pipeline::with_filter_cache(50);
        pipeline.register(Box::new(AllowHandler {
            name: "h1",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: None,
        }));

        // Trigger some cache entries
        pipeline.for_event(HookEvent::PreToolUse, Some("Read"));
        pipeline.for_event(HookEvent::PreToolUse, Some("Edit"));
        pipeline.for_event(HookEvent::SessionStart, None);

        let debug = format!("{:?}", pipeline);
        assert!(debug.contains("filter_cache_entries"));
        assert!(debug.contains("Some(3)")); // 3 entries
    }

    #[test]
    fn test_filter_cache_clear() {
        let cache = FilterCache::new(100);
        let key = FilterCacheKey::new(HookEvent::PreToolUse, Some("Read"));
        cache.get_or_compute(key.clone(), || vec![1, 2, 3]);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_filter_cache_lru_eviction() {
        let cache = FilterCache::new(2); // capacity 2
        let key1 = FilterCacheKey::new(HookEvent::PreToolUse, Some("A"));
        let key2 = FilterCacheKey::new(HookEvent::PreToolUse, Some("B"));
        let key3 = FilterCacheKey::new(HookEvent::PreToolUse, Some("C"));

        cache.get_or_compute(key1.clone(), || vec![1]);
        cache.get_or_compute(key2.clone(), || vec![2]);
        assert_eq!(cache.len(), 2);

        // key3 evicts key1 (LRU)
        cache.get_or_compute(key3.clone(), || vec![3]);
        assert_eq!(cache.len(), 2);

        // key1 should be gone (evicted)
        let result = cache.get_or_compute(key1, || vec![999]);
        assert_eq!(result, &[999]); // computed new value
    }

    // ── E4-S3: Budget overflow protection tests ──────────────────────

    /// A handler that declares itself critical (never skipped on budget exhaustion).
    struct CriticalHandler {
        name: &'static str,
        context: String,
    }

    impl Handler for CriticalHandler {
        fn name(&self) -> &str {
            self.name
        }
        fn events(&self) -> &[HookEvent] {
            &[HookEvent::PreToolUse]
        }
        fn is_critical(&self) -> bool {
            true
        }
        fn execute(&self, _ctx: &mut CortexContext) -> HandlerResult {
            HandlerResult::allow(self.name, Some(self.context.clone()))
        }
    }

    #[test]
    fn test_budget_overflow_skips_non_critical() {
        let mut pipeline = Pipeline::new();
        // First handler: consumes all budget
        pipeline.register(Box::new(AllowHandler {
            name: "budget_eater",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("x".repeat(500)), // exceeds typical 300-char budget
        }));
        // Second handler: non-critical, should be skipped when budget=0
        pipeline.register(Box::new(AllowHandler {
            name: "should_skip",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("skipped_content".to_string()),
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        let output = pipeline.execute(&mut ctx);
        let hso = output.hook_specific_output.expect("should have output");
        assert!(hso.additional_context.contains(&"x".repeat(100)));
        assert!(
            !hso.additional_context.contains("skipped_content"),
            "Non-critical handler should be skipped when budget exhausted"
        );
    }

    #[test]
    fn test_budget_overflow_keeps_critical() {
        let mut pipeline = Pipeline::new();
        // First handler: consumes all budget
        pipeline.register(Box::new(AllowHandler {
            name: "budget_eater",
            events: vec![HookEvent::PreToolUse],
            tool_matcher: None,
            async_handler: false,
            context: Some("x".repeat(500)),
        }));
        // Second handler: critical, should still run
        pipeline.register(Box::new(CriticalHandler {
            name: "critical_must_run",
            context: "critical_output".to_string(),
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        let output = pipeline.execute(&mut ctx);
        let hso = output.hook_specific_output.expect("should have output");
        assert!(
            hso.additional_context.contains("critical_output"),
            "Critical handler must run even when budget exhausted"
        );
    }

    // ── E5-S1: Handler priority tests ────────────────────────────────

    /// Handler with configurable priority.
    struct PriorityHandler {
        name: &'static str,
        prio: u8,
        order: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl Handler for PriorityHandler {
        fn name(&self) -> &str {
            self.name
        }
        fn events(&self) -> &[HookEvent] {
            &[HookEvent::PreToolUse]
        }
        fn priority(&self) -> u8 {
            self.prio
        }
        fn execute(&self, _ctx: &mut CortexContext) -> HandlerResult {
            self.order.lock().expect("lock").push(self.name);
            HandlerResult::skip(self.name)
        }
    }

    #[test]
    fn test_priority_ordering() {
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut pipeline = Pipeline::new();

        // Register in reverse priority order
        pipeline.register(Box::new(PriorityHandler {
            name: "low",
            prio: 200,
            order: order.clone(),
        }));
        pipeline.register(Box::new(PriorityHandler {
            name: "high",
            prio: 10,
            order: order.clone(),
        }));
        pipeline.register(Box::new(PriorityHandler {
            name: "medium",
            prio: 128,
            order: order.clone(),
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        pipeline.execute(&mut ctx);

        let executed = order.lock().expect("lock");
        assert_eq!(executed.as_slice(), &["high", "medium", "low"]);
    }

    // ── E4-S2: Graceful degradation tests ────────────────────────────

    /// Handler with a specific dependency tier.
    struct TieredHandler {
        name: &'static str,
        tier: u8,
    }

    impl Handler for TieredHandler {
        fn name(&self) -> &str {
            self.name
        }
        fn events(&self) -> &[HookEvent] {
            &[HookEvent::PreToolUse]
        }
        fn dependency_tier(&self) -> u8 {
            self.tier
        }
        fn execute(&self, _ctx: &mut CortexContext) -> HandlerResult {
            HandlerResult::allow(self.name, Some(format!("{}_output", self.name)))
        }
    }

    #[test]
    fn test_graceful_degradation_skips_high_tier_without_rlm() {
        let mut pipeline = Pipeline::new();
        pipeline.register(Box::new(TieredHandler {
            name: "t0",
            tier: 0,
        }));
        pipeline.register(Box::new(TieredHandler {
            name: "t2",
            tier: 2,
        }));

        let (_tmp, knowledge) = make_test_knowledge();
        let input = serde_json::json!({});
        // Create context WITHOUT rlm (None) — T2 handlers should be skipped
        let mut ctx = CortexContext::from_input(
            HookEvent::PreToolUse,
            input,
            knowledge,
            std::path::PathBuf::from("/project"),
        );
        let output = pipeline.execute(&mut ctx);
        let hso = output.hook_specific_output.expect("should have output");
        assert!(
            hso.additional_context.contains("t0_output"),
            "T0 should run"
        );
        assert!(
            !hso.additional_context.contains("t2_output"),
            "T2 should be skipped without rlm"
        );
    }
}
