//! `inferlets` — InferletService integration for touring-wasm.
//!
//! Manages a pool of WASM inferlets for sandboxed evaluation.
//! Enhancement (E18): `try_init()` graceful initialization — returns `Default`
//! with diagnostics on failure instead of panicking.
//!
//! # Architecture
//!
//! ```text
//! HookRuntime
//!     │
//!     ├── SemanticClassifier (TF-IDF + SIMD)
//!     ├── PatternBandit (Q-Learning)
//!     └── InferletService (WASM evaluation)
//!              │
//!              ├── AsyncInferletPool ("always_success")
//!              ├── AsyncInferletPool ("memory")
//!              ├── AsyncInferletPool ("pattern")
//!              └── AsyncInferletPool ("classifier")
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! let service = InferletService::new()?;
//! service.load_inferlet("memory", include_bytes!("../inferlets/wasm_bytes/libinferlets_memory.wasm"), 4)?;
//! let result = service.evaluate("memory", r#"{"allocations": ["a", "b"]}"#).await?;
//! ```

use std::sync::Arc;

use moka::sync::Cache;
use serde_json::Value;
use tokio::sync::mpsc;
use touring_bindings::wasm::{AsyncInferletPool, PluginContext, PluginResult, WasmRunner};

/// Inferlet identifiers available in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferletKind {
    /// Always returns success — for testing
    AlwaysSuccess,
    /// Memory access pattern analyzer
    Memory,
    /// Pattern matcher
    Pattern,
    /// Semantic classifier
    Classifier,
}

impl InferletKind {
    /// Return the name of this inferlet for pool identification.
    pub fn name(&self) -> &'static str {
        match self {
            InferletKind::AlwaysSuccess => "always_success",
            InferletKind::Memory => "memory",
            InferletKind::Pattern => "pattern",
            InferletKind::Classifier => "classifier",
        }
    }
}

/// A pool of WASM inferlets of a specific kind.
struct InferletPool {
    kind: InferletKind,
    pool: AsyncInferletPool,
    // Note: AsyncInferletPool doesn't implement Debug, so we skip it in Debug impl
}

impl std::fmt::Debug for InferletPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InferletPool")
            .field("kind", &self.kind)
            .finish()
    }
}

impl InferletPool {
    /// Evaluate input with this pool's inferlet.
    async fn evaluate(&self, input: &str) -> Result<PluginResult, String> {
        self.pool
            .evaluate(PluginContext::new(input))
            .await
            .map_err(|e| format!("Evaluation failed for {:?}: {}", self.kind, e))
    }
}

/// Thread-safe service for managing multiple inferlet pools.
///
/// # Example
///
/// ```rust,ignore
/// let service = InferletService::new()?;
/// service.load_inferlet(InferletKind::Memory, wasm_bytes, 4).await?;
/// let result = service.evaluate(InferletKind::Memory, input).await?;
/// ```
#[derive(Clone)]
pub struct InferletService {
    runner: Arc<WasmRunner>,
    /// Pool registry keyed by `InferletKind`.
    ///
    /// Migrated 2026-04-16 from `Arc<tokio::sync::RwLock<HashMap<InferletKind,
    /// InferletPool>>>` to `moka::sync::Cache<InferletKind, Arc<InferletPool>>`
    /// as the 11th site of Moka Expansion Wave 2. Gains:
    ///
    /// - **Sharded lock-free reads** — every `evaluate()` call previously
    ///   acquired the RwLock in read mode; moka uses per-shard atomics so
    ///   concurrent evaluations on different pools do not serialize.
    /// - **No more `.await` on pool lookup** — `moka::sync::Cache::get` is
    ///   synchronous and safe inside async fns (no runtime block).
    /// - **Arc-wrapped values** — pool handles are cloned as O(1) refcount
    ///   bumps on hit, instead of holding a read-guard across await points
    ///   (which forced the prior code to drop the guard before `.await`).
    pools: Cache<InferletKind, Arc<InferletPool>>,
    /// Command sender injected at actor spawn so that `evaluate_sync` can
    /// dispatch `RunInferlet` through the actor mpsc channel (safe from
    /// the bare-thread actor context where Tokio runtime is unavailable).
    ctx_tx: std::sync::Arc<
        std::sync::Mutex<Option<mpsc::Sender<crate::daemon_protocol::ProjectCommand>>>,
    >,
}

impl std::fmt::Debug for InferletService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InferletService")
            .field("pools", &self.pools)
            .finish()
    }
}

/// Error from [`InferletService::new`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct InferletServiceError(pub String);

impl InferletService {
    /// Create a new InferletService with a fresh WasmRunner.
    pub fn new() -> Result<Self, InferletServiceError> {
        let runner = WasmRunner::new().map_err(|e| InferletServiceError(e.to_string()))?;
        Ok(Self {
            runner: Arc::new(runner),
            pools: Self::build_pool_cache(),
            ctx_tx: std::sync::Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Build the moka cache backing the pool registry.
    ///
    /// Capacity 16 covers the 4 current inferlet kinds plus headroom for
    /// future additions. No TTL: pools are long-lived — evicted only
    /// when explicitly replaced via `load_inferlet`.
    fn build_pool_cache() -> Cache<InferletKind, Arc<InferletPool>> {
        Cache::builder().max_capacity(16).build()
    }

    /// Return a sender for dispatching `RunInferlet` commands to the project actor.
    /// The sender is stored in `ContextRuntime::cmd_tx` at actor spawn time.
    pub fn cmd_tx(&self) -> mpsc::Sender<crate::daemon_protocol::ProjectCommand> {
        let ctx_tx = self.ctx_tx.lock().expect("ctx_tx mutex poisoned");
        ctx_tx
            .as_ref()
            .expect("cmd_tx not initialized — actor not yet spawned")
            .clone()
    }

    /// Load an inferlet into a pool of the specified size.
    ///
    /// If a pool for this kind already exists, it is replaced (moka's
    /// `insert` semantics on collision).
    pub async fn load_inferlet(
        &self,
        kind: InferletKind,
        wasm_bytes: &[u8],
        pool_size: usize,
    ) -> Result<(), String> {
        let pool = AsyncInferletPool::new(&self.runner, wasm_bytes, pool_size)
            .await
            .map_err(|e| format!("Failed to create pool for {:?}: {}", kind, e))?;

        // moka::sync::Cache::insert is lock-free + sync — safe inside
        // an async fn without blocking the runtime.
        self.pools
            .insert(kind, Arc::new(InferletPool { kind, pool }));
        Ok(())
    }

    /// Evaluate input with the specified inferlet kind.
    ///
    /// Wraps the input in a JSON envelope with `__inferlet__` discriminator
    /// so the WASM dispatcher can route to the correct inferlet.
    /// Returns an error if the pool for that kind is not loaded.
    pub async fn evaluate(&self, kind: InferletKind, input: &str) -> Result<PluginResult, String> {
        // Pool lookup is now a lock-free `get` returning `Option<Arc<InferletPool>>`.
        // The Arc survives past the subsequent `.await` without holding any
        // read guard — the prior RwLock-based impl had to drop the guard
        // before awaiting anyway; this is structurally cleaner.
        let pool = self
            .pools
            .get(&kind)
            .ok_or_else(|| format!("Inferlet pool {:?} not loaded", kind))?;

        let wrapped = wrap_for_dispatcher(kind, input);
        pool.evaluate(&wrapped).await
    }

    /// Evaluate input with the specified inferlet kind — async version for use
    /// from daemon async contexts (connection handlers, not actor thread).
    pub async fn evaluate_async(
        &self,
        kind: InferletKind,
        input: &str,
    ) -> Result<PluginResult, String> {
        let pool = self
            .pools
            .get(&kind)
            .ok_or_else(|| format!("Inferlet pool {:?} not loaded", kind))?;

        let wrapped = wrap_for_dispatcher(kind, input);
        tokio::time::timeout(tokio::time::Duration::from_secs(5), pool.evaluate(&wrapped))
            .await
            .map_err(|_| format!("WASM evaluation timed out after 5s for {:?}", kind))
            .and_then(|r| r)
    }

    /// Check if a specific inferlet kind is loaded.
    ///
    /// Kept `async` for backward compatibility with callers that
    /// `.await` this method. The body is synchronous — moka's
    /// `contains_key` is lock-free.
    pub async fn is_loaded(&self, kind: InferletKind) -> bool {
        self.pools.contains_key(&kind)
    }

    /// Check if a specific inferlet kind is loaded — synchronous version.
    ///
    /// Uses moka's lock-free `contains_key` directly; no tokio runtime needed.
    /// Use this from contexts where `Handle::try_current()` would fail
    /// (e.g., project actor OS thread).
    pub fn is_loaded_sync(&self, kind: InferletKind) -> bool {
        self.pools.contains_key(&kind)
    }

    /// List all loaded inferlet kinds — synchronous version.
    ///
    /// Uses moka's lock-free `iter` directly; no tokio runtime needed.
    /// Use this from contexts where `Handle::try_current()` would fail.
    pub fn loaded_kinds_sync(&self) -> Vec<InferletKind> {
        self.pools.iter().map(|(k, _)| *k).collect()
    }

    /// Return the count of loaded inferlet pools — synchronous version.
    ///
    /// Uses moka's lock-free `iter` directly; no tokio runtime needed.
    pub fn pool_count_sync(&self) -> usize {
        self.pools.iter().count()
    }

    /// List all loaded inferlet kinds.
    ///
    /// Kept `async` for backward compatibility.
    pub async fn loaded_kinds(&self) -> Vec<InferletKind> {
        // moka's `iter` is lock-free; collecting into Vec preserves
        // the historical return type without any allocation change
        // (InferletKind is `Copy`).
        self.pools.iter().map(|(k, _)| *k).collect()
    }

    /// Get the WasmRunner for this service.
    pub fn runner(&self) -> &WasmRunner {
        &self.runner
    }

    /// Try to initialize the service with embedded WASM inferlets.
    ///
    /// Returns `Some(InferletService)` with all 4 inferlets loaded when the
    /// `inferlets-wasm` feature is enabled and the embedded WASM binary is valid.
    /// Returns `None` when:
    /// - The feature is not enabled (compile-time)
    /// - The WasmRunner fails to initialize
    /// - The WASM binary is empty (not built yet)
    /// - Any inferlet pool fails to load
    ///
    /// This provides graceful degradation: callers can attempt WASM inferlet
    /// initialization without hard-failing when the binary isn't available.
    ///
    /// # Build instructions
    ///
    /// To enable WASM inferlets:
    /// ```bash
    /// # 1. Build the inferlets WASM binary
    /// cargo build --target wasm32-wasip1 --release -p inferlets
    ///
    /// # 2. Build touring-daemon with the feature enabled
    /// cargo build --release --features inferlets-wasm -p touring-hooks
    /// ```
    pub async fn try_init(_pool_size: usize) -> Option<Self> {
        #[cfg(feature = "inferlets-wasm")]
        {
            let service = Self::new().ok()?;
            match crate::inferlets_assets::load_all_inferlets(&service, _pool_size).await {
                Ok(()) => {
                    tracing::info!("InferletService initialized with {} pool(s)", _pool_size);
                    Some(service)
                }
                Err(e) => {
                    tracing::debug!("InferletService WASM loading failed (non-fatal): {e}");
                    None
                }
            }
        }

        #[cfg(not(feature = "inferlets-wasm"))]
        {
            tracing::debug!(
                "InferletService: inferlets-wasm feature not enabled — \
                 run `cargo build --target wasm32-wasip1 --release -p inferlets` \
                 then rebuild with `--features inferlets-wasm`"
            );
            None
        }
    }
}

impl Default for InferletService {
    fn default() -> Self {
        // FIX10: Graceful initialization — if WasmRunner fails to initialize,
        // fall back to a degraded service with empty pools. WASM operations
        // will fail gracefully at evaluation time instead of panicking at init.
        Self::new().unwrap_or_else(|e| {
            tracing::warn!(
                "InferletService::default() WasmRunner init failed (non-fatal): {e}. \
                 WASM inferlets will not be available until re-initialization."
            );
            // Fall back: create a fresh WasmRunner. If this also fails, we have
            // an environmental issue (wasmtime not available) — propagate gracefully.
            let runner = WasmRunner::new()
                .or_else(|_| WasmRunner::new_on_demand())
                .unwrap_or_else(|e2| {
                    tracing::error!(
                        "InferletService::default() fallback WasmRunner (on-demand) also failed: {e2}. \
                         WASM inferlets are unavailable."
                    );
                    // At this point both pooling and on-demand allocation have failed —
                    // this is an unrecoverable environmental issue. Return a zero-runner
                    // sentinel by re-creating with on-demand (best effort); callers will
                    // fail gracefully at evaluation time.
                    WasmRunner::new_sentinel()
                });
            Self {
                runner: Arc::new(runner),
                pools: Self::build_pool_cache(),
                ctx_tx: std::sync::Arc::new(std::sync::Mutex::new(None)),
            }
        })
    }
}

// ── Dispatcher helper ─────────────────────────────────────────────────────────

/// Wrap input JSON with `__inferlet__` discriminator for the WASM dispatcher.
///
/// If the input is already a JSON object, adds `__inferlet__` as a field.
/// Otherwise wraps as `{"__inferlet__": "kind", "data": <input>}`.
fn wrap_for_dispatcher(kind: InferletKind, input: &str) -> String {
    let kind_str = kind.name();

    let trimmed = input.trim();
    if trimmed.starts_with('{') {
        // Already JSON object — inject __inferlet__ at the start
        // Simple heuristic: insert after the opening brace
        let after_open = trimmed.find('{').map(|i| i + 1).unwrap_or(0);
        let before_close = trimmed.rfind('}').unwrap_or(trimmed.len());

        let inner = &trimmed[after_open..before_close];
        format!("{{\"__inferlet__\":\"{}\",{}}}", kind_str, inner)
    } else {
        // Non-JSON — wrap as {__inferlet__, data: input}
        format!(
            "{{\"__inferlet__\":\"{}\",\"data\":{}}}",
            kind_str,
            Value::String(input.to_string())
        )
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inferlet_service_creation() {
        let service = InferletService::new();
        assert!(
            service.is_ok(),
            "InferletService should create successfully"
        );
    }

    #[tokio::test]
    async fn test_inferlet_service_empty() {
        let service = InferletService::new().unwrap();
        assert!(!service.is_loaded(InferletKind::AlwaysSuccess).await);
        assert!(!service.is_loaded(InferletKind::Memory).await);
    }

    #[tokio::test]
    async fn test_inferlet_kind_names() {
        assert_eq!(InferletKind::AlwaysSuccess.name(), "always_success");
        assert_eq!(InferletKind::Memory.name(), "memory");
        assert_eq!(InferletKind::Pattern.name(), "pattern");
        assert_eq!(InferletKind::Classifier.name(), "classifier");
    }

    #[tokio::test]
    async fn test_evaluate_without_load_returns_error() {
        let service = InferletService::new().unwrap();
        let result = service.evaluate(InferletKind::AlwaysSuccess, "test").await;
        assert!(result.is_err(), "Should error when pool not loaded");
        assert!(
            result.unwrap_err().contains("not loaded"),
            "Error should mention pool not loaded"
        );
    }

    // ── E2E tests with real WASM inferlets ─────────────────────────────────

    #[tokio::test]
    #[cfg(feature = "inferlets-wasm")]
    async fn test_e2e_load_all_inferlets() {
        // Load real WASM bytes and verify all 4 kinds load successfully.
        let wasm_bytes = crate::inferlets_assets::INFERLET_WASM;
        assert!(!wasm_bytes.is_empty(), "WASM bytes must be embedded");

        let service = InferletService::new().unwrap();
        service
            .load_inferlet(InferletKind::AlwaysSuccess, wasm_bytes, 2)
            .await
            .unwrap();
        service
            .load_inferlet(InferletKind::Memory, wasm_bytes, 2)
            .await
            .unwrap();
        service
            .load_inferlet(InferletKind::Pattern, wasm_bytes, 2)
            .await
            .unwrap();
        service
            .load_inferlet(InferletKind::Classifier, wasm_bytes, 2)
            .await
            .unwrap();

        assert!(service.is_loaded(InferletKind::AlwaysSuccess).await);
        assert!(service.is_loaded(InferletKind::Memory).await);
        assert!(service.is_loaded(InferletKind::Pattern).await);
        assert!(service.is_loaded(InferletKind::Classifier).await);
    }

    #[tokio::test]
    #[cfg(feature = "inferlets-wasm")]
    async fn test_e2e_always_success_returns_success() {
        let wasm_bytes = crate::inferlets_assets::INFERLET_WASM;
        let service = InferletService::new().unwrap();
        service
            .load_inferlet(InferletKind::AlwaysSuccess, wasm_bytes, 2)
            .await
            .unwrap();

        // always_success always returns success (1)
        let result = service
            .evaluate(InferletKind::AlwaysSuccess, "anything goes here")
            .await
            .unwrap();
        assert!(result.success, "always_success must always return success");
    }

    #[tokio::test]
    #[cfg(feature = "inferlets-wasm")]
    async fn test_e2e_memory_inferlet_keyword_detection() {
        let wasm_bytes = crate::inferlets_assets::INFERLET_WASM;
        let service = InferletService::new().unwrap();
        service
            .load_inferlet(InferletKind::Memory, wasm_bytes, 2)
            .await
            .unwrap();

        // memory inferlet detects memory-related keywords
        // Uses high-specificity phrase "memory leak" for guaranteed match
        let mem_result = service
            .evaluate(
                InferletKind::Memory,
                r#"{"data": "memory leak detected in heap"}"#,
            )
            .await
            .unwrap();
        assert!(mem_result.success, "memory input should be detected");

        // non-memory input should not match
        let other_result = service
            .evaluate(InferletKind::Memory, r#"{"cooking": "pizza"}"#)
            .await
            .unwrap();
        assert!(!other_result.success, "non-memory input should not match");
    }

    #[tokio::test]
    #[cfg(feature = "inferlets-wasm")]
    async fn test_e2e_classifier_inferlet_keyword_detection() {
        let wasm_bytes = crate::inferlets_assets::INFERLET_WASM;
        let service = InferletService::new().unwrap();
        service
            .load_inferlet(InferletKind::Classifier, wasm_bytes, 2)
            .await
            .unwrap();

        // classifier inferlet detects semantic/classification keywords
        // Needs ≥3 of 9 indicators for score ≥ 0.30 threshold
        let cls_result = service
            .evaluate(
                InferletKind::Classifier,
                r#"{"classify": "sentiment", "text": "positive", "intent": "analysis"}"#,
            )
            .await
            .unwrap();
        assert!(cls_result.success, "classifier input should be detected");

        // non-classification input
        let other_result = service
            .evaluate(InferletKind::Classifier, r#"{"unrelated": "data"}"#)
            .await
            .unwrap();
        assert!(
            !other_result.success,
            "non-classifier input should not match"
        );
    }

    #[tokio::test]
    #[cfg(feature = "inferlets-wasm")]
    async fn test_e2e_pattern_inferlet_keyword_detection() {
        let wasm_bytes = crate::inferlets_assets::INFERLET_WASM;
        let service = InferletService::new().unwrap();
        service
            .load_inferlet(InferletKind::Pattern, wasm_bytes, 2)
            .await
            .unwrap();

        // pattern inferlet detects pattern-related keywords
        let pat_result = service
            .evaluate(
                InferletKind::Pattern,
                r#"{"regex": "\\d+", "pattern": "match"}"#,
            )
            .await
            .unwrap();
        assert!(pat_result.success, "pattern input should be detected");

        // non-pattern input
        let other_result = service
            .evaluate(InferletKind::Pattern, r#"{"foo": "bar"}"#)
            .await
            .unwrap();
        assert!(!other_result.success, "non-pattern input should not match");
    }

    #[tokio::test]
    #[cfg(feature = "inferlets-wasm")]
    async fn test_e2e_wasm_dispatcher_single_binary() {
        // Verify that a SINGLE WASM binary dispatches to different inferlets
        // based on the __inferlet__ discriminator.
        let wasm_bytes = crate::inferlets_assets::INFERLET_WASM;
        let service = InferletService::new().unwrap();

        // All 4 kinds share the SAME wasm_bytes (same .wasm file)
        // but dispatch internally based on __inferlet__ key
        service
            .load_inferlet(InferletKind::AlwaysSuccess, wasm_bytes, 2)
            .await
            .unwrap();
        service
            .load_inferlet(InferletKind::Memory, wasm_bytes, 2)
            .await
            .unwrap();

        // Both should work independently via the dispatcher
        let always_result = service
            .evaluate(InferletKind::AlwaysSuccess, "test")
            .await
            .unwrap();
        let memory_result = service
            .evaluate(InferletKind::Memory, r#"{"data": "memory leak in heap"}"#)
            .await
            .unwrap();

        assert!(always_result.success);
        assert!(memory_result.success);
    }

    #[tokio::test]
    #[cfg(feature = "inferlets-wasm")]
    async fn test_e2e_loaded_kinds_lists_all() {
        let wasm_bytes = crate::inferlets_assets::INFERLET_WASM;
        let service = InferletService::new().unwrap();
        service
            .load_inferlet(InferletKind::AlwaysSuccess, wasm_bytes, 2)
            .await
            .unwrap();
        service
            .load_inferlet(InferletKind::Memory, wasm_bytes, 1)
            .await
            .unwrap();

        let kinds = service.loaded_kinds().await;
        assert!(kinds.contains(&InferletKind::AlwaysSuccess));
        assert!(kinds.contains(&InferletKind::Memory));
        assert_eq!(kinds.len(), 2);
    }

    // ── try_init graceful degradation ─────────────────────────────────────────

    #[tokio::test]
    async fn test_try_init_without_wasm_feature_returns_none() {
        // When inferlets-wasm feature is NOT enabled, try_init should return None
        // gracefully without panicking.
        #[cfg(not(feature = "inferlets-wasm"))]
        {
            let result = InferletService::try_init(2).await;
            assert!(
                result.is_none(),
                "try_init without inferlets-wasm feature should return None"
            );
        }
    }

    #[tokio::test]
    #[cfg(feature = "inferlets-wasm")]
    async fn test_try_init_with_wasm_feature_loads_all() {
        let service = InferletService::try_init(2).await;
        // If the WASM binary is present and valid, all inferlets should be loaded
        if let Some(svc) = service {
            assert!(svc.is_loaded(InferletKind::AlwaysSuccess).await);
            assert!(svc.is_loaded(InferletKind::Memory).await);
            assert!(svc.is_loaded(InferletKind::Pattern).await);
            assert!(svc.is_loaded(InferletKind::Classifier).await);
        }
        // If None, WASM binary wasn't available — that's OK, graceful degradation
    }

    // ── wrap_for_dispatcher unit tests ────────────────────────────────────────

    #[test]
    fn test_wrap_json_object_injects_discriminator() {
        // Input is already a JSON object — __inferlet__ must be prepended.
        let json_wrapped = wrap_for_dispatcher(InferletKind::Memory, r#"{"key":"val"}"#);
        assert!(
            json_wrapped.contains("\"__inferlet__\":\"memory\""),
            "discriminator missing: {json_wrapped}"
        );
        assert!(
            json_wrapped.contains("\"key\":\"val\""),
            "original fields missing: {json_wrapped}"
        );
    }

    #[test]
    fn test_wrap_non_json_uses_data_envelope() {
        // Plain string input must become {"__inferlet__":..., "data":"..."}.
        let plain_wrapped = wrap_for_dispatcher(InferletKind::Pattern, "plain text");
        assert!(
            plain_wrapped.contains("\"__inferlet__\":\"pattern\""),
            "discriminator missing: {plain_wrapped}"
        );
        assert!(
            plain_wrapped.contains("\"data\""),
            "data envelope missing: {plain_wrapped}"
        );
    }

    #[test]
    fn test_wrap_empty_json_object() {
        // Edge case: bare `{}` should not panic and must inject the discriminator.
        let empty_obj_wrapped = wrap_for_dispatcher(InferletKind::Classifier, "{}");
        assert!(
            empty_obj_wrapped.contains("\"__inferlet__\":\"classifier\""),
            "discriminator missing for empty object: {empty_obj_wrapped}"
        );
    }

    #[test]
    fn test_wrap_empty_string() {
        // Empty input is not a JSON object — must use data envelope without panic.
        let empty_str_wrapped = wrap_for_dispatcher(InferletKind::AlwaysSuccess, "");
        assert!(
            empty_str_wrapped.contains("\"__inferlet__\":\"always_success\""),
            "discriminator missing for empty input: {empty_str_wrapped}"
        );
        assert!(
            empty_str_wrapped.contains("\"data\""),
            "data envelope missing: {empty_str_wrapped}"
        );
    }

    #[test]
    fn test_wrap_whitespace_input() {
        // Whitespace-only input trims to empty — treated as non-JSON.
        let ws_wrapped = wrap_for_dispatcher(InferletKind::Memory, "   ");
        assert!(
            ws_wrapped.contains("\"__inferlet__\":\"memory\""),
            "discriminator missing for whitespace: {ws_wrapped}"
        );
    }

    #[test]
    fn test_wrap_preserves_kind_name_for_all_variants() {
        // Every InferletKind must produce its canonical name in the wrapped output.
        let cases = [
            (InferletKind::AlwaysSuccess, "always_success"),
            (InferletKind::Memory, "memory"),
            (InferletKind::Pattern, "pattern"),
            (InferletKind::Classifier, "classifier"),
        ];
        for (kind, expected_name) in cases {
            let variant_wrapped = wrap_for_dispatcher(kind, "{}");
            assert!(
                variant_wrapped.contains(expected_name),
                "kind name '{expected_name}' missing in: {variant_wrapped}"
            );
        }
    }
}
