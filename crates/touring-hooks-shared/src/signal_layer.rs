//! Signal-layer **vocabulary** — the [`SignalContext`] / [`SignalLayer`] /
//! [`LayerMetrics`] contract shared by every signal producer.
//!
//! Session B F4-pre of the `touring-ceg` extraction (2026-06-10): these types
//! were extracted from `touring-hooks::shared::signal_pipeline` (which keeps
//! the heavy `SignalPipeline` engine and its `touring-analysis` dependencies
//! in the parent) so leaf-side producers like [`crate::ast_grep_signal`] can
//! implement [`SignalLayer`] without reaching back into the parent crate.
//! `signal_pipeline` re-exports them, so every historical
//! `crate::shared::signal_pipeline::{SignalContext, SignalLayer}` path still
//! resolves unchanged.

/// Context passed to each signal layer for enrichment.
pub struct SignalContext<'a> {
    /// Relative file path being processed.
    pub file_path: &'a str,
    /// Source content (if available — empty for bash hooks).
    pub source: &'a str,
    /// CILA complexity level (0-6).
    pub cila_level: usize,
    /// Hook name (pre_read, pre_edit, pre_write, pre_bash).
    pub hook_name: &'a str,
    /// Opaque extension data — layers can downcast if needed.
    pub extensions: &'a dyn std::any::Any,
}

impl<'a> SignalContext<'a> {
    /// Create a minimal context for testing.
    pub fn new(file_path: &'a str, source: &'a str) -> Self {
        Self {
            file_path,
            source,
            cila_level: 3,
            hook_name: "test",
            extensions: &(),
        }
    }

    /// Create context with specific CILA level.
    pub fn with_cila(mut self, level: usize) -> Self {
        self.cila_level = level;
        self
    }

    /// Create context with specific hook name.
    pub fn with_hook(mut self, name: &'a str) -> Self {
        self.hook_name = name;
        self
    }
}

/// Metadata about a layer's execution.
#[derive(Debug, Clone)]
pub struct LayerMetrics {
    /// Layer name (for observability).
    pub name: &'static str,
    /// Number of signals produced.
    pub signal_count: usize,
    /// Execution time in microseconds.
    pub duration_us: u64,
}

/// A single signal enrichment layer.
///
/// Implement this trait for each type of signal (dependents, gotchas,
/// blast radius, etc.). The pipeline calls `enrich()` on each layer
/// and collects the scored signals.
pub trait SignalLayer: Send + Sync {
    /// Unique name for this layer (used in metrics/logging).
    fn name(&self) -> &'static str;

    /// Produce scored signals for the given context.
    ///
    /// Returns empty vec if this layer has nothing relevant to contribute.
    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)>;

    /// Whether this layer should run for the given CILA level.
    ///
    /// Default: always run. Override to skip expensive layers at low CILA.
    fn should_run(&self, _cila_level: usize) -> bool {
        true
    }
}
