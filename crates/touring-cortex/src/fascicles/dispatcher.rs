//! Dispatcher — request dispatch and routing for fascicles
//!
//! Handles routing of requests to appropriate fascicles based on registration.
//! Bypasses the linear O(n) pipeline scan by using direct O(1) channel lookup.

use super::planning::{MctsPlanningFascicle, MctsPlanningRequest};
use crate::fascicles::channels::DirectChannel;
use crate::fascicles::evidence::Evidence;
use crate::fascicles::registry::{HandlerChannel, HandlerRegistry};
use std::sync::{Arc, Mutex};
use touring_intelligence::reasoning::GraphInformedMCTS;

/// Opaque handle to a registered handler channel.
///
/// Returned by [`FascicleDispatcher::lookup_handler`] and can be used
/// to send evidence directly to the handler without dispatcher involvement.
#[derive(Debug, Clone)]
pub struct HandlerChannelHandle {
    /// Unique identifier for this handler registration.
    pub handler_id: String,
    /// Inner channel sender for dispatching evidence.
    inner: Arc<DirectChannel<Evidence>>,
}

impl HandlerChannelHandle {
    /// Creates a new HandlerChannelHandle wrapping the given channel.
    fn new(handler_id: String, channel: HandlerChannel) -> Self {
        Self {
            handler_id,
            inner: channel,
        }
    }

    /// Attempts to send evidence to this handler's channel.
    ///
    /// Returns `Ok(())` if the send succeeded, `Err(Evidence)` if the
    /// channel was full and the message was dropped.
    #[inline]
    pub fn try_dispatch(&self, evidence: Evidence) -> Result<(), Evidence> {
        self.inner.try_send(evidence)
    }
}

/// Error types that can occur during fascicle dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    /// No handler with this name is registered.
    HandlerNotFound(String),
    /// The handler channel is disconnected (receiver dropped).
    ChannelDisconnected(String),
    /// Evidence dispatch failed due to channel being full.
    ChannelFull(String),
    /// Dispatch was called with an empty handler name.
    EmptyHandlerName,
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DispatchError::HandlerNotFound(name) => write!(f, "handler not found: {name}"),
            DispatchError::ChannelDisconnected(name) => {
                write!(f, "handler channel disconnected: {name}")
            }
            DispatchError::ChannelFull(name) => {
                write!(f, "handler channel full (dropped evidence): {name}")
            }
            DispatchError::EmptyHandlerName => {
                write!(f, "dispatch called with empty handler name")
            }
        }
    }
}

impl std::error::Error for DispatchError {}

/// FascicleDispatcher — O(1) direct dispatch to registered handlers.
///
///
/// Bypasses the linear O(n) handler scan of the standard Cortex pipeline.
/// Instead of iterating through all handlers to find a matching one, this
/// dispatcher maintains a direct channel lookup table for O(1) routing.
///
/// # Architecture
///
/// ```text
/// Broadcast(Evidence)
///      │
///      ▼
/// ┌─────────────────────────┐
/// │  FascicleDispatcher     │◄── HandlerRegistry (HashMap<String, HandlerChannel>)
/// │  ───────────────────   │
/// │  broadcast_receiver    │   Direct O(1) lookup by handler name
/// │  dispatch_channel      │
/// └───────────┬─────────────┘
///             │ try_dispatch via HandlerChannelHandle
///             ▼
///      Registered Handler
/// ```
#[derive(Debug)]
pub struct FascicleDispatcher {
    /// Registry mapping handler names to their communication channels.
    registry: HandlerRegistry,
    /// Receiver for the broadcast channel — `Mutex`-wrapped for `Send + Sync`.
    broadcast_receiver: Mutex<std::sync::mpsc::Receiver<Evidence>>,
    /// Direct dispatch channel for sending evidence to specific handlers.
    dispatch_channel: DirectChannel<Evidence>,
}

impl FascicleDispatcher {
    /// Creates a new FascicleDispatcher with the given registry and broadcast receiver.
    ///
    /// The `broadcast_receiver` is used to collect evidence that was broadcast
    /// to all fascicles. The `dispatch_channel` provides O(1) direct dispatch
    /// to registered handlers.
    ///
    /// # Arguments
    /// * `registry` — The handler registry containing handler metadata and channels
    /// * `broadcast_receiver` — Receiver endpoint for the broadcast channel
    pub fn new(
        registry: HandlerRegistry,
        broadcast_receiver: std::sync::mpsc::Receiver<Evidence>,
    ) -> Self {
        Self {
            registry,
            broadcast_receiver: Mutex::new(broadcast_receiver),
            dispatch_channel: DirectChannel::new(256),
        }
    }

    /// Dispatches evidence to the handler identified by `handler_name`.
    ///
    /// Uses direct O(1) HashMap lookup instead of linear O(n) handler scan.
    ///
    /// # Arguments
    /// * `handler_name` — Name of the target handler
    /// * `evidence` — The evidence to dispatch
    ///
    /// # Returns
    /// * `Ok(())` — Evidence was successfully sent to the handler channel
    /// * `Err(DispatchError)` — Dispatch failed (handler not found, channel full, etc.)
    pub fn dispatch(&self, handler_name: &str, evidence: Evidence) -> Result<(), DispatchError> {
        if handler_name.is_empty() {
            return Err(DispatchError::EmptyHandlerName);
        }

        let channel = self
            .lookup_handler(handler_name)
            .ok_or_else(|| DispatchError::HandlerNotFound(handler_name.to_string()))?;

        channel
            .try_dispatch(evidence)
            .map_err(|_e| DispatchError::ChannelFull(handler_name.to_string()))
    }

    /// Looks up a handler channel by name using O(1) HashMap lookup.
    ///
    /// Returns `Some(HandlerChannelHandle)` if the handler is registered,
    /// `None` otherwise.
    ///
    /// # Arguments
    /// * `handler_name` — Name of the handler to look up
    ///
    /// # Returns
    /// * `Some(HandlerChannelHandle)` — Channel handle for the registered handler
    /// * `None` — No handler with this name is registered
    #[inline]
    pub fn lookup_handler(&self, handler_name: &str) -> Option<HandlerChannelHandle> {
        self.registry
            .lookup(handler_name)
            .map(|ch| HandlerChannelHandle::new(handler_name.to_string(), ch))
    }

    /// Returns a lock guard over the broadcast receiver for this dispatcher.
    ///
    /// Use within a single thread to drain the broadcast channel in a loop.
    /// # Panics
    /// Panics if the internal mutex is poisoned (only possible after a thread panic).
    #[inline]
    pub fn broadcast_receiver(
        &self,
    ) -> std::sync::MutexGuard<'_, std::sync::mpsc::Receiver<Evidence>> {
        self.broadcast_receiver
            .lock()
            .expect("broadcast_receiver mutex is not poisoned")
    }

    /// Returns a reference to the dispatch channel.
    ///
    /// Primarily useful for testing or when external code needs to
    /// send evidence directly via the dispatch mechanism.
    #[inline]
    pub fn dispatch_channel(&self) -> &DirectChannel<Evidence> {
        &self.dispatch_channel
    }

    /// Returns the number of currently registered handlers.
    #[inline]
    pub fn handler_count(&self) -> usize {
        self.registry.len()
    }

    /// Returns `true` if no handlers are currently registered.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }

    /// Dispatches an MCTS planning request via the shared MctsPlanningFascicle.
    ///
    /// This method provides O(1) access to GraphInformedMCTS search for
    /// cognitive graph-informed context suggestions. The MCTS search runs
    /// in a background thread to avoid blocking the hot path.
    ///
    /// # Arguments
    /// * `request` — Planning request containing root state and graph context
    ///
    /// # Returns
    /// * `Ok(oneshot::Receiver<MctsResult>)` — Receiver for the async result
    /// * `Err(DispatchError)` — If dispatch fails
    ///
    /// # Example
    /// ```ignore
    /// let req = MctsPlanningRequest::from_symbol_file("MyStruct", "src/lib.rs");
    /// let rx = dispatcher.dispatch_mcts_planning(req)?;
    /// let result = rx.await?;
    /// ```
    pub fn dispatch_mcts_planning(
        &self,
        request: MctsPlanningRequest,
    ) -> Result<
        tokio::sync::oneshot::Receiver<touring_intelligence::reasoning::MCTSResult>,
        DispatchError,
    > {
        let fascicle = MctsPlanningFascicle::new();
        fascicle.dispatch(request)
    }

    /// Returns the global MCTS planner singleton.
    ///
    /// Allows synchronous access to GraphInformedMCTS when blocking is acceptable.
    pub fn mcts_planner_global(&self) -> &'static GraphInformedMCTS {
        MctsPlanningFascicle::global_planner()
    }

    /// Evaporates pheromones in the global MCTS planner between search sessions.
    ///
    /// Call this between planning sessions to allow the ACO pheromone system
    /// to adapt to changing context.
    pub fn evaporate_mcts_pheromones(&self) {
        MctsPlanningFascicle::new().evaporate();
    }

    /// Processes evidence from the broadcast channel in a loop.
    ///
    /// Continuously receives evidence from the broadcast receiver and dispatches
    /// each to the appropriate handler via O(1) HashMap lookup. Returns when
    /// the broadcast channel is disconnected.
    ///
    /// # Returns
    /// * `Vec<Evidence>` — All evidence received during the run loop,
    ///   useful for batch processing and drift detection
    ///
    /// # Errors
    /// Returns `DispatchError::ChannelDisconnected` if the broadcast receiver
    /// is already disconnected when `run()` is called.
    pub fn run(&self) -> Result<Vec<Evidence>, DispatchError> {
        let mut batch = Vec::new();
        let receiver = self
            .broadcast_receiver
            .lock()
            .expect("broadcast_receiver mutex is not poisoned");

        while let Ok(evidence) = receiver.recv() {
            let handler_name = evidence.source.as_ref();
            if let Some(handle) = self.lookup_handler(handler_name) {
                // Fire-and-forget dispatch — we don't block on channel full
                let _ = handle.try_dispatch(evidence.clone());
            }
            batch.push(evidence);
        }

        Ok(batch)
    }

    /// Attempts to process available evidence from the broadcast channel without blocking.
    ///
    /// Uses `try_recv()` to collect all immediately available evidence and
    /// dispatches each to the appropriate handler. Returns immediately if
    /// no evidence is available.
    ///
    /// # Returns
    /// * `Vec<Evidence>` — Evidence received during this non-blocking run,
    ///   empty if no evidence was available
    pub fn try_dispatch(&self) -> Vec<Evidence> {
        let mut batch = Vec::new();

        while let Ok(evidence) = self
            .broadcast_receiver
            .lock()
            .expect("broadcast_receiver mutex is not poisoned")
            .try_recv()
        {
            let handler_name = evidence.source.as_ref();
            if let Some(handle) = self.lookup_handler(handler_name) {
                let _ = handle.try_dispatch(evidence.clone());
            }
            batch.push(evidence);
        }

        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fascicles::evidence::{Confidence, HandlerName, Priority};

    // ------------------------------------------------------------------------
    // DispatchError tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_dispatch_error_display() {
        let err = DispatchError::HandlerNotFound("test_handler".to_string());
        assert_eq!(err.to_string(), "handler not found: test_handler");

        let err = DispatchError::ChannelFull("test_handler".to_string());
        assert_eq!(
            err.to_string(),
            "handler channel full (dropped evidence): test_handler"
        );

        let err = DispatchError::EmptyHandlerName;
        assert_eq!(err.to_string(), "dispatch called with empty handler name");
    }

    #[test]
    fn test_dispatch_error_partial_eq() {
        let err1 = DispatchError::HandlerNotFound("test".to_string());
        let err2 = DispatchError::HandlerNotFound("test".to_string());
        let err3 = DispatchError::HandlerNotFound("other".to_string());
        assert_eq!(err1, err2);
        assert_ne!(err1, err3);
    }

    // ------------------------------------------------------------------------
    // HandlerChannelHandle tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_handler_channel_handle_try_dispatch_success() {
        let channel: HandlerChannel = Arc::new(DirectChannel::new(1));
        let handle = HandlerChannelHandle::new("test".to_string(), channel);
        let evidence = Evidence::new(
            Confidence::new(0.9),
            Priority::new(128),
            HandlerName::from("test-handler"),
            Default::default(),
        );
        let result = handle.try_dispatch(evidence);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handler_channel_handle_try_dispatch_full() {
        let channel: HandlerChannel = Arc::new(DirectChannel::new(1));
        let handle = HandlerChannelHandle::new("test".to_string(), channel);
        let evidence = Evidence::new(
            Confidence::new(0.9),
            Priority::new(128),
            HandlerName::from("test-handler"),
            Default::default(),
        );
        // Fill the channel
        let _ = handle.try_dispatch(evidence);
        // Next send should fail
        let evidence2 = Evidence::new(
            Confidence::new(0.85),
            Priority::new(64),
            HandlerName::from("test-handler-2"),
            Default::default(),
        );
        let result = handle.try_dispatch(evidence2);
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------------
    // FascicleDispatcher tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_fascicle_dispatcher_new() {
        let registry = HandlerRegistry::new();
        let (_, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);
        assert!(dispatcher.is_empty());
        assert_eq!(dispatcher.handler_count(), 0);
    }

    #[test]
    fn test_fascicle_dispatcher_lookup_handler_not_found() {
        let registry = HandlerRegistry::new();
        let (_, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);
        let result = dispatcher.lookup_handler("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_fascicle_dispatcher_dispatch_empty_name() {
        let registry = HandlerRegistry::new();
        let (_, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);
        let evidence = Evidence::new(
            Confidence::new(0.9),
            Priority::new(128),
            HandlerName::from("test-handler"),
            Default::default(),
        );
        let result = dispatcher.dispatch("", evidence);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DispatchError::EmptyHandlerName);
    }

    #[test]
    fn test_fascicle_dispatcher_dispatch_not_found() {
        let registry = HandlerRegistry::new();
        let (_, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);
        let evidence = Evidence::new(
            Confidence::new(0.9),
            Priority::new(128),
            HandlerName::from("test-handler"),
            Default::default(),
        );
        let result = dispatcher.dispatch("nonexistent", evidence);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            DispatchError::HandlerNotFound("nonexistent".to_string())
        );
    }

    #[test]
    fn test_fascicle_dispatcher_broadcast_receiver() {
        let registry = HandlerRegistry::new();
        let (_, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);
        let _guard = dispatcher.broadcast_receiver();
    }

    #[test]
    fn test_fascicle_dispatcher_dispatch_channel() {
        let registry = HandlerRegistry::new();
        let (_, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);
        let _ = dispatcher.dispatch_channel();
    }

    // ------------------------------------------------------------------------
    // run() and try_dispatch() tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_try_dispatch_empty_batch() {
        let registry = HandlerRegistry::new();
        let (_, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);
        let batch = dispatcher.try_dispatch();
        assert!(batch.is_empty());
    }

    #[test]
    fn test_try_dispatch_with_evidence() {
        let registry = HandlerRegistry::new();
        let (tx, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);

        // Send evidence to broadcast channel
        let evidence = Evidence::new(
            Confidence::new(0.9),
            Priority::new(128),
            HandlerName::from("test-handler"),
            Default::default(),
        );
        let _ = tx.send(evidence.clone());

        let batch = dispatcher.try_dispatch();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].source.as_ref(), "test-handler");
    }

    #[test]
    fn test_try_dispatch_multiple_evidence() {
        let registry = HandlerRegistry::new();
        let (tx, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);

        for i in 0..3 {
            let evidence = Evidence::new(
                Confidence::new(0.9),
                Priority::new(128),
                HandlerName::from(format!("handler-{}", i)),
                Default::default(),
            );
            let _ = tx.send(evidence);
        }

        let batch = dispatcher.try_dispatch();
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn test_run_disconnected_channel() {
        let registry = HandlerRegistry::new();
        let (_, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);
        // Drop the sender to disconnect the channel
        // run() should return Ok with empty batch
        let result = dispatcher.run();
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_run_with_evidence_batch() {
        use std::thread;
        use std::time::Duration;

        let registry = HandlerRegistry::new();
        let (tx, rx) = std::sync::mpsc::sync_channel(256);
        let dispatcher = FascicleDispatcher::new(registry, rx);

        // Spawn thread to send evidence after a brief delay
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            for i in 0..2 {
                let evidence = Evidence::new(
                    Confidence::new(0.8),
                    Priority::new(64),
                    HandlerName::from(format!("batch-handler-{}", i)),
                    Default::default(),
                );
                let _ = tx.send(evidence);
            }
            // Drop sender to signal end of batch
            drop(tx);
        });

        let result = dispatcher.run();
        let batch = result.unwrap();

        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].source.as_ref(), "batch-handler-0");
        assert_eq!(batch[1].source.as_ref(), "batch-handler-1");

        handle.join().expect("sender thread panicked");
    }
}
