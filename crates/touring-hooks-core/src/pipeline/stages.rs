//! Pipeline stages — adapter layer bridging `touring-flow` Stage types into `touring-hooks`.
//!
//! This module provides:
//! - Re-exports of the core `Stage`, `Filter`, `Transform`, `Timed`, `FanOut`, `FanIn`,
//!   `Inspect` types from `touring_flow` so callers in `touring-hooks` don't need a
//!   direct dependency on `touring-flow`'s public API.
//! - [`AsyncStageAdapter`] — an async wrapper that delegates to a synchronous stage
//!   by spawning it onto the Tokio blocking thread pool, so sync `Stage` implementors
//!   can be used in async pipelines without changes to their implementation.
//!
//! # Example
//!
//! ```ignore
//! use touring_hooks::pipeline::stages::{Stage, Filter, AsyncStageAdapter};
//! use touring_orchestration::flow::types::Item;
//!
//! // A sync filter stage (implements Stage<Item>)
//! let filter = Filter::new("evens", |item: &Item| item.id.ends_with("even"));
//!
//! // Wrap it in AsyncStageAdapter to use in an async context
//! let adapter = AsyncStageAdapter::new(filter);
//! ```

use std::sync::Arc;
use tokio::task;

use touring_orchestration::flow::error::{Error as FlowError, StageError};
pub use touring_orchestration::flow::stages::{
    FanIn, FanOut, Filter, Inspect, NamedStage, Stage, Timed, Transform,
};
use touring_orchestration::flow::types::Item;
pub use touring_orchestration::flow::{FlowPipeline, TouringFlowBuilder};

/// An async adapter that wraps a synchronous [`Stage`] and executes its [`process`][Stage::process]
/// on Tokio's blocking thread pool.
///
/// This allows sync `Stage` implementors (including bare closure `Fn(Item) -> Result<Item>`)
/// to be used in async pipelines without any changes to their underlying implementation.
///
/// # Type parameters
/// - `S`: the wrapped synchronous stage. Must implement `Stage<Item>` + `Send + 'static`.
///
/// # Example
///
/// ```ignore
/// use touring_hooks::pipeline::stages::{Stage, AsyncStageAdapter, Filter};
/// use touring_orchestration::flow::types::Item;
///
/// let sync_filter = Filter::new("evens", |item: &Item| item.id.ends_with("even"));
/// let async_adapter = AsyncStageAdapter::new(sync_filter);
/// ```
#[derive(Debug, Clone)]
pub struct AsyncStageAdapter<S> {
    inner: Arc<S>,
}

impl<S> AsyncStageAdapter<S>
where
    S: Stage<Item> + Send + 'static,
{
    /// Construct a new adapter wrapping the given synchronous stage.
    pub fn new(inner: S) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Process one item asynchronously by delegating to the inner sync stage.
    ///
    /// This spawns the work onto Tokio's blocking thread pool so the calling
    /// async task is not blocked.
    pub async fn process_async(&self, item: Item) -> Result<Item, FlowError> {
        let inner = self.inner.clone();
        task::spawn_blocking(move || inner.process(item))
            .await
            .map_err(|_| FlowError::Invariant("async task join error".into()))?
            .map_err(|se| match se {
                StageError::Filtered => FlowError::Invariant("item filtered".into()),
                StageError::FanOutEmpty => {
                    FlowError::Invariant("fan-out: no branches produced output".into())
                }
                StageError::FanOutMultiple(n) => {
                    FlowError::Invariant(format!("fan-out: expected 1 result, got {}", n))
                }
                StageError::Timeout => FlowError::Invariant("stage timed out".into()),
            })
    }
}

// Arc<S> is Send + Sync whenever S: Send + Sync. Since S: Stage<Item> + Send,
// Arc<S> gives us automatic Send + Sync — no manual impl needed.
// impl<S> Send for AsyncStageAdapter<S> {}
// impl<S> Sync for AsyncStageAdapter<S> {}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_orchestration::flow::stages::Filter;
    use touring_orchestration::flow::types::Item;

    #[tokio::test]
    async fn async_stage_adapter_wraps_filter() {
        let filter = Filter::new("evens", |item: &Item| item.id.ends_with("even"));
        let adapter = AsyncStageAdapter::new(filter);

        let item = Item::new("test_even", "label");
        let result = adapter.process_async(item).await;
        assert!(result.is_ok(), "expected ok, got {:?}", result);
    }

    #[tokio::test]
    async fn async_stage_adapter_rejects_filtered() {
        let filter = Filter::new("evens", |item: &Item| item.id.ends_with("even"));
        let adapter = AsyncStageAdapter::new(filter);

        let item = Item::new("test_odd", "label");
        let result = adapter.process_async(item).await;
        assert!(
            result.is_err(),
            "expected error for filtered item, got {:?}",
            result
        );
    }
}
