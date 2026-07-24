//! Channels — inter-fascicle communication channels
//!
//! Provides message-passing primitives for communication between fascicles.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

/// DirectChannel is a wrapper around mpsc channel with dropped message counting.
///
/// Provides inter-fascicle communication with bounded capacity and
/// dropped message tracking when sends fail due to a full channel.
///
/// The receiver is stored behind a `Mutex<Option<...>>` so that `DirectChannel<T>`
/// implements `Sync`, allowing it to be placed inside `Arc` across threads.
///
/// # Type Parameters
/// * `T` - The message type, must be sendable across thread boundaries and cloneable
pub struct DirectChannel<T: Send + Sync + Clone + 'static> {
    sender: SyncSender<T>,
    receiver: Mutex<Option<Receiver<T>>>,
    capacity: usize,
    dropped: AtomicUsize,
}

impl<T: Send + Sync + Clone + 'static> std::fmt::Debug for DirectChannel<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectChannel")
            .field("capacity", &self.capacity)
            .field("dropped", &self.dropped.load(Ordering::Relaxed))
            .finish()
    }
}

impl<T: Send + Sync + Clone + 'static> DirectChannel<T> {
    /// Creates a new DirectChannel with the specified capacity.
    ///
    /// Uses `sync_channel` internally to provide bounded capacity.
    /// The default capacity is 256 messages.
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = sync_channel(capacity);
        Self {
            sender,
            receiver: Mutex::new(Some(receiver)),
            capacity,
            dropped: AtomicUsize::new(0),
        }
    }

    /// Returns a clone of the sender for producing messages.
    pub fn sender(&self) -> SyncSender<T> {
        self.sender.clone()
    }

    /// Takes and returns the receiver endpoint.
    ///
    /// # Panics
    /// Panics if the receiver has already been taken.
    pub fn receiver(&self) -> Receiver<T> {
        self.receiver
            .lock()
            .expect("receiver mutex is not poisoned")
            .take()
            .expect("receiver already taken - DirectChannel is single-consumer")
    }

    /// Returns the number of messages that were dropped due to a full channel.
    pub fn dropped_count(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Attempts to send a message, incrementing dropped count on failure.
    ///
    /// Returns `Ok(())` if the message was sent, `Err(T)` if the channel
    /// was full and the message was dropped.
    pub fn try_send(&self, msg: T) -> Result<(), T> {
        match self.sender.try_send(msg) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(v)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Err(v)
            }
            Err(TrySendError::Disconnected(v)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Err(v)
            }
        }
    }
}

impl<T: Send + Sync + Clone + 'static> Default for DirectChannel<T> {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_channel_new() {
        let channel: DirectChannel<i32> = DirectChannel::new(256);
        assert_eq!(channel.dropped_count(), 0);
        let _ = channel.sender();
    }

    #[test]
    fn test_direct_channel_default_capacity() {
        let channel: DirectChannel<String> = DirectChannel::default();
        assert_eq!(channel.dropped_count(), 0);
    }

    #[test]
    fn test_direct_channel_sender_clone() {
        let channel: DirectChannel<u32> = DirectChannel::new(10);
        let sender1 = channel.sender();
        let sender2 = channel.sender();
        // Both cloned senders should be able to send (verify they work)
        let recv = channel.receiver();
        let _ = sender1.send(1);
        let _ = sender2.send(2);
        drop(sender1);
        drop(sender2);
        // Verify both values received
        assert_eq!(recv.recv(), Ok(1));
        assert_eq!(recv.recv(), Ok(2));
    }

    #[test]
    fn test_direct_channel_try_send_success() {
        let channel: DirectChannel<i32> = DirectChannel::new(1);
        let result = channel.try_send(42);
        assert!(result.is_ok());
        assert_eq!(channel.dropped_count(), 0);
    }

    #[test]
    fn test_direct_channel_try_send_dropped() {
        let channel: DirectChannel<i32> = DirectChannel::new(1);
        // Fill the channel
        let _ = channel.try_send(1);
        // Next send should fail and be counted as dropped
        let result = channel.try_send(2);
        assert!(result.is_err());
        assert_eq!(channel.dropped_count(), 1);
    }

    #[test]
    fn test_direct_channel_receiver_taken() {
        let channel: DirectChannel<i32> = DirectChannel::new(10);
        let _receiver = channel.receiver();
        // Trying to take receiver again panics — verify it panics
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| channel.receiver()));
        assert!(result.is_err());
    }
}
