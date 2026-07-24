//! Streaming MCTS — background search that refines results with each iteration.
//!
//! Wraps MCTSEngine in a tokio task that continuously searches the tree.
//! Callers can request the best-so-far result at any time via `watch` channel.

use crate::reasoning::mcts::{MCTSConfig, MCTSEngine, MCTSResult};
use std::sync::Arc;
use tokio::sync::watch;

/// Handle to a streaming MCTS search running in the background.
pub struct StreamingMCTS {
    /// Latest best result (updated after each search batch).
    result_rx: watch::Receiver<Option<MCTSResult>>,
    /// Shutdown signal — dropping this stops the background task.
    _shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl StreamingMCTS {
    /// Start a background MCTS search. Returns a handle for reading results.
    ///
    /// Each iteration runs a full MCTS search and publishes the result.
    /// The background task yields between iterations to avoid monopolizing CPU.
    pub fn spawn(
        config: MCTSConfig,
        expand_fn: Arc<dyn Fn(u64) -> Vec<u64> + Send + Sync + 'static>,
        reward_fn: Arc<dyn Fn(u64, u64) -> f64 + Send + Sync + 'static>,
        root_state: u64,
    ) -> Self {
        let (result_tx, result_rx) = watch::channel(None);
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            let engine = MCTSEngine::new(config);

            loop {
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }

                // Run search (CPU-bound but fast: ~50 rollouts × 5 depth)
                let result = engine.search(root_state, &*expand_fn, &*reward_fn);

                match result {
                    Some(mcts_result) => {
                        if result_tx.send(Some(mcts_result)).is_err() {
                            break; // receiver dropped
                        }
                    }
                    None => break, // no candidates = nothing to search
                }

                // Yield to other tasks before next iteration
                tokio::task::yield_now().await;
            }
        });

        Self {
            result_rx,
            _shutdown_tx: shutdown_tx,
        }
    }

    /// Get the best result found so far. Returns None if no search has completed.
    pub fn best_so_far(&self) -> Option<MCTSResult> {
        self.result_rx.borrow().clone()
    }

    /// Check if a result is available without blocking.
    pub fn has_result(&self) -> bool {
        self.result_rx.borrow().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_streaming_mcts_produces_result() {
        let expand: Arc<dyn Fn(u64) -> Vec<u64> + Send + Sync> =
            Arc::new(|state| vec![state + 1, state + 2, state + 3]);
        let reward: Arc<dyn Fn(u64, u64) -> f64 + Send + Sync> =
            Arc::new(|_state, action| action as f64 / 10.0);

        let handle = StreamingMCTS::spawn(MCTSConfig::default(), expand, reward, 0);

        // Wait briefly for background search to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let result = handle.best_so_far();
        assert!(result.is_some(), "should have a result after 100ms");
        let r = result.unwrap();
        assert!(r.confidence > 0.0);
        assert!(r.total_rollouts > 0);
    }

    #[tokio::test]
    async fn test_streaming_mcts_shutdown_on_drop() {
        let expand: Arc<dyn Fn(u64) -> Vec<u64> + Send + Sync> = Arc::new(|state| vec![state + 1]);
        let reward: Arc<dyn Fn(u64, u64) -> f64 + Send + Sync> = Arc::new(|_, _| 0.5);

        let handle = StreamingMCTS::spawn(MCTSConfig::default(), expand, reward, 0);
        drop(handle);
        // Should not panic or hang
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}
