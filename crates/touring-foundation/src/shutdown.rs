//! Graceful shutdown handler for touring-daemon and long-running services.
//!
//! Provides a watch-channel-based shutdown signal that integrates with
//! tokio's `ctrl_c` handler. Services can wait on the receiver to drain
//! before exiting.
//!
//! # Example
//!
//! ```ignore
//! let shutdown = Shutdown::new();
//! let handle = tokio::spawn(async move {
//!     tokio::select! {
//!         result = server.run() => result,
//!         _ = shutdown.recv() => Err("shutdown".into()),
//!     }
//! });
//! ```

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::signal::ctrl_c;
use tokio::sync::watch;

/// Graceful shutdown signal container.
///
/// `Shutdown` owns the sender side of a watch channel and an atomic flag
/// that tracks whether `shutdown()` has been called. All receivers created
/// via [`Shutdown::recv()`] share the same flag, so any of them can
/// detect a shutdown request.
#[derive(Debug)]
pub struct Shutdown {
    tx: watch::Sender<()>,
    triggered: Arc<AtomicBool>,
}

impl Shutdown {
    /// Create a new shutdown signal.
    pub fn new() -> Self {
        let (tx, _) = watch::channel(());
        Self {
            tx,
            triggered: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns `true` if `shutdown()` has been called.
    #[inline]
    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        self.triggered.load(Ordering::Relaxed)
    }

    /// Returns a future that resolves when shutdown is triggered.
    #[inline]
    pub fn recv(&self) -> ShutdownRecv {
        ShutdownRecv {
            rx: self.tx.subscribe(),
            triggered: self.triggered.clone(),
        }
    }

    /// Trigger shutdown — notifies all receivers.
    ///
    /// This is idempotent.
    #[inline]
    pub fn shutdown(&self) {
        self.triggered.store(true, Ordering::Relaxed);
        // send() is infallible on a watch channel; receivers that have
        // already been dropped simply ignore the signal.
        let _ = self.tx.send(());
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

/// Future that resolves when the shutdown signal is set.
#[derive(Debug)]
pub struct ShutdownRecv {
    rx: watch::Receiver<()>,
    triggered: Arc<AtomicBool>,
}

impl std::future::Future for ShutdownRecv {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<()> {
        // Clone the Arc so we don't hold &mut self while accessing triggered
        let triggered = self.triggered.clone();

        // Fast path: check the atomic flag
        if triggered.load(Ordering::Relaxed) {
            return std::task::Poll::Ready(());
        }

        // Get the Changed future (borrows &mut self.rx) and poll it
        let changed_fut = self.rx.changed();
        futures::pin_mut!(changed_fut);
        match changed_fut.poll(cx) {
            std::task::Poll::Ready(Ok(())) => {
                triggered.store(true, Ordering::Relaxed);
                std::task::Poll::Ready(())
            }
            std::task::Poll::Ready(Err(_)) => {
                // Sender was dropped without calling shutdown — treat as shutdown
                triggered.store(true, Ordering::Relaxed);
                std::task::Poll::Ready(())
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl Default for ShutdownRecv {
    fn default() -> Self {
        let (tx, rx) = watch::channel(());
        drop(tx);
        Self {
            rx,
            triggered: Arc::new(AtomicBool::new(true)),
        }
    }
}

/// Spawn a background task that listens for Ctrl+C and triggers shutdown.
///
/// The background task runs inside the current Tokio runtime and is
/// automatically cancelled when the runtime shuts down.
pub async fn spawn_ctrl_c_handler() -> Result<Shutdown, std::io::Error> {
    let shutdown = Shutdown::new();
    let triggered = shutdown.triggered.clone();

    tokio::spawn(async move {
        let _ = ctrl_c().await;
        tracing::info!("Received Ctrl+C — initiating graceful shutdown");
        triggered.store(true, Ordering::Relaxed);
    });

    Ok(shutdown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;

    #[tokio::test]
    async fn test_shutdown_triggered() {
        let shutdown = Shutdown::new();
        assert!(!shutdown.is_shutdown());

        shutdown.shutdown();

        // recv() should complete immediately after shutdown
        let timeout = tokio::time::timeout(std::time::Duration::from_millis(100), shutdown.recv());
        timeout
            .await
            .expect("shutdown should complete within 100ms");
        assert!(shutdown.is_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_recv_after_drop() {
        let shutdown = Shutdown::new();
        drop(shutdown); // drop sender without calling shutdown()

        // recv() should complete because sender was dropped
        let timeout = tokio::time::timeout(std::time::Duration::from_millis(100), async {
            let recv = Shutdown::new().recv();
            recv.await;
        });
        timeout
            .await
            .expect("recv should complete when sender is dropped");
    }

    #[test]
    fn test_shutdown_default() {
        let _ = Shutdown::default();
    }

    #[test]
    fn test_shutdown_recv_default() {
        let recv = ShutdownRecv::default();
        // Default receiver is already "closed" — should return Ready immediately
        assert!(std::pin::pin!(recv).now_or_never().is_some());
    }

    #[tokio::test]
    async fn test_is_shutdown() {
        let shutdown = Shutdown::new();
        assert!(!shutdown.is_shutdown());

        shutdown.shutdown();
        assert!(shutdown.is_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_idempotent() {
        let shutdown = Shutdown::new();
        shutdown.shutdown();
        shutdown.shutdown(); // should not panic

        let timeout = tokio::time::timeout(std::time::Duration::from_millis(100), shutdown.recv());
        timeout
            .await
            .expect("idempotent shutdown should still resolve");
    }
}
