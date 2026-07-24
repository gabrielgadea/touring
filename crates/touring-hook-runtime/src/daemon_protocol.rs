//! Daemon actor protocol — the `ProjectCommand` envelope sent to each
//! per-project runtime task.
//!
//! Wave R+C I3 (2026-06-10): extracted from `daemon.rs` so the protocol type
//! can descend to the `touring-hook-runtime` layer together with its
//! consumers (`hook_runtime::cmd_tx`, `inferlets::cmd_tx`) at the next carve.
//! `daemon.rs` re-exports it, so `crate::daemon::ProjectCommand` paths are
//! unchanged.

use tokio::sync::oneshot;

/// Actor-pattern command sent to the per-project runtime task.
///
/// Replaces the legacy `Arc<Mutex<HookRuntime>>` contention model.
/// Clients send a command via the mpsc channel and await the oneshot
/// response — no kernel-level mutex, no lock poisoning, natural backpressure
/// via the bounded channel capacity.
///
/// Public so the `cmd_tx()` accessors on `ContextRuntime` and
/// `InferletService` can expose `mpsc::Sender<ProjectCommand>` without
/// leaking a private type from a public API.
pub enum ProjectCommand {
    /// Invoke a hook handler with the given payload, return output via oneshot.
    RunHook {
        /// Registry name of the hook handler to invoke.
        hook_name: String,
        /// JSON payload passed to the hook handler.
        payload: serde_json::Value,
        /// Channel on which the handler's serialized output is returned.
        response: oneshot::Sender<String>,
        /// Instant the command was enqueued. The actor measures
        /// `enqueued_at.elapsed()` at dequeue and records it as the
        /// `actor_queue_wait` gate-metric — the serialized-actor
        /// backpressure signal (2026-07-01).
        enqueued_at: std::time::Instant,
    },
    /// Flush WAL, persist RL/CRDT state, terminate the actor loop.
    Shutdown {
        /// Channel signalled once shutdown has completed.
        done: oneshot::Sender<()>,
    },
    /// PLN2: Direct saga coordinator call — bypasses hook table for 2PC ops.
    /// Replies with String (serde JSON) via oneshot.
    RunSaga {
        /// Name of the saga coordinator entry point to invoke.
        hook_name: String,
        /// JSON payload passed to the saga coordinator.
        payload: serde_json::Value,
        /// Channel on which the saga's serialized JSON result is returned.
        response: oneshot::Sender<String>,
    },
    /// WASM inferlet evaluation routed through the actor to bridge bare-thread
    /// actor context → daemon's Tokio multi-thread runtime (where Handle::current
    /// is valid). `block_in_place` inside `blocking_recv` context makes
    /// Handle::current() resolve correctly.
    RunInferlet {
        /// Which WASM inferlet to evaluate.
        kind: crate::inferlets::InferletKind,
        /// Serialized input string fed to the inferlet.
        input: String,
        /// Channel on which the typed plugin result (or error string) is returned.
        response: oneshot::Sender<Result<touring_bindings::wasm::PluginResult, String>>,
    },
}

impl ProjectCommand {
    /// Build a `RunInferlet` command paired with the receiver that will
    /// resolve when the actor finishes evaluating the inferlet. Callers
    /// send the `ProjectCommand` over the mpsc channel and `.await` the
    /// returned receiver for the typed result.
    ///
    /// Wires the `RunInferlet` variant into a constructable surface so
    /// it is no longer flagged as "never constructed" by cargo's
    /// dead-code analyzer — even when the higher-level CLI dispatcher
    /// is replaced or short-circuited in alternate build configurations.
    #[must_use]
    pub fn run_inferlet(
        kind: crate::inferlets::InferletKind,
        input: String,
    ) -> (
        Self,
        oneshot::Receiver<Result<touring_bindings::wasm::PluginResult, String>>,
    ) {
        let (tx, rx) = oneshot::channel();
        (
            Self::RunInferlet {
                kind,
                input,
                response: tx,
            },
            rx,
        )
    }
}
