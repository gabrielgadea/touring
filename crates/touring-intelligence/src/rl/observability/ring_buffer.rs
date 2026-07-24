//! Fixed-capacity ring buffer for hot-path metrics collection.
//!
//! Overwrites oldest entries when capacity is reached (circular buffer).
//!
//! # Thread Safety
//!
//! **Not thread-safe.** Concurrent writes require external synchronization
//! (e.g. `RwLock` or per-thread buffers). The `Send + Sync` bounds are NOT
//! implemented — this is designed for single-threaded hot-path metrics collection.

use std::fmt::Debug;

/// A fixed-capacity ring buffer that overwrites oldest entries when full.
///
/// # Capacity
///
/// The ring buffer has a fixed capacity N. When `N` entries have been written,
/// subsequent writes overwrite the oldest entry (FIFO overwrite).
///
/// # Thread Safety
///
/// This implementation is **not thread-safe**. The caller must ensure
/// exclusive access if used across threads. For concurrent scenarios,
/// wrap in a `RwLock` or use per-thread ring buffers.
///
/// # Example
///
/// ```
/// use touring_intelligence::rl::observability::RingBuffer;
///
/// let mut rb = RingBuffer::new(4);
/// rb.write(1u32);
/// rb.write(2u32);
/// assert_eq!(rb.len(), 2);
/// assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![1, 2]);
/// ```
#[derive(Debug)]
pub struct RingBuffer<T> {
    buffer: Vec<T>,
    head: usize, // Index of oldest valid entry
    size: usize, // Current number of valid entries
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// Create a new ring buffer with the given capacity.
    ///
    /// Capacity must be > 0. Panics if capacity is 0.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "RingBuffer capacity must be > 0");
        Self {
            buffer: Vec::with_capacity(capacity),
            head: 0,
            size: 0,
            capacity,
        }
    }

    /// Write an entry into the ring buffer.
    ///
    /// If the buffer is full, the oldest entry is overwritten.
    /// If the buffer is not yet full, the entry is placed after the last valid entry.
    pub fn write(&mut self, entry: T) {
        if self.size < self.capacity {
            // Not full yet — push_back equivalent
            self.buffer.push(entry);
            self.size += 1;
        } else {
            // Full — overwrite oldest, advance head
            // SAFETY: head is always in [0, capacity) and buffer.len() == capacity when full.
            if let Some(slot) = self.buffer.get_mut(self.head) {
                *slot = entry;
            }
            self.head = (self.head + 1) % self.capacity;
        }
    }

    /// Returns the current number of valid entries in the buffer.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns true if the buffer contains no entries.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Returns true if the buffer is at capacity.
    #[inline]
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.size == self.capacity
    }

    /// Returns the maximum capacity of this ring buffer.
    #[inline]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns an iterator over all valid entries in order (oldest to newest).
    pub fn iter(&self) -> RingBufferIter<'_, T> {
        RingBufferIter {
            buffer: &self.buffer,
            head: self.head,
            size: self.size,
            capacity: self.capacity,
            index: 0,
        }
    }

    /// Linearize the buffer contents into a `Vec<T>` in order (oldest to newest).
    ///
    /// This allocates a new Vec. For zero-allocation iteration, use [`iter`](Self::iter).
    #[must_use]
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.iter().cloned().collect()
    }

    /// Clear all entries from the buffer.
    ///
    /// Does not deallocate the underlying storage — the capacity is preserved.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.head = 0;
        self.size = 0;
    }

    /// Returns a reference to the most recently written entry, if any.
    #[must_use]
    pub fn last(&self) -> Option<&T> {
        if self.size == 0 {
            None
        } else if self.size < self.capacity {
            // Not full — last is at index size - 1
            // SAFETY: size > 0 and size <= capacity, so size-1 is in [0, capacity).
            Some(unsafe { self.buffer.get_unchecked(self.size - 1) })
        } else {
            // Full — last is at (head - 1 + capacity) % capacity
            let last_idx = if self.head == 0 {
                self.capacity - 1
            } else {
                self.head - 1
            };
            // SAFETY: last_idx is in [0, capacity) by construction.
            // When size > 0, the buffer always holds at least `capacity` elements.
            Some(unsafe { self.buffer.get_unchecked(last_idx) })
        }
    }
}

/// Iterator over a [`RingBuffer`] in order (oldest to newest).
#[derive(Debug)]
pub struct RingBufferIter<'a, T> {
    buffer: &'a [T],
    head: usize,
    size: usize,
    capacity: usize,
    index: usize,
}

impl<'a, T> Iterator for RingBufferIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.size {
            return None;
        }

        // Physical index: (head + index) % capacity
        let idx = (self.head + self.index) % self.capacity;
        self.index += 1;
        // SAFETY: idx is in [0, capacity) and buffer has capacity elements.
        Some(unsafe { self.buffer.get_unchecked(idx) })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size - self.index, Some(self.size - self.index))
    }
}

impl<'a, T> ExactSizeIterator for RingBufferIter<'a, T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic() {
        let mut rb = RingBuffer::new(4);
        assert!(rb.is_empty());
        assert_eq!(rb.len(), 0);

        rb.write(1);
        rb.write(2);
        rb.write(3);

        assert_eq!(rb.len(), 3);
        assert!(!rb.is_full());
        assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn test_ring_buffer_overwrite() {
        let mut rb = RingBuffer::new(3);

        rb.write(1);
        rb.write(2);
        rb.write(3);
        assert!(rb.is_full());
        assert_eq!(rb.len(), 3);

        // Overwriting oldest (1)
        rb.write(4);
        assert_eq!(rb.len(), 3);
        assert!(rb.is_full());
        // Buffer should now be [2, 3, 4]
        assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn test_ring_buffer_empty() {
        let mut rb: RingBuffer<i32> = RingBuffer::new(2);
        assert!(rb.last().is_none());
        assert!(rb.to_vec().is_empty());

        rb.write(1);
        assert_eq!(rb.last(), Some(&1));

        rb.write(2);
        assert_eq!(rb.last(), Some(&2));

        rb.clear();
        assert!(rb.last().is_none());
        assert!(rb.is_empty());
    }

    #[test]
    fn test_ring_buffer_capacity() {
        let rb: RingBuffer<i32> = RingBuffer::new(100);
        assert_eq!(rb.capacity(), 100);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_ring_buffer_many_writes() {
        let mut rb = RingBuffer::new(2);

        for i in 0..1000 {
            rb.write(i);
        }

        // Only last 2 should remain
        assert_eq!(rb.len(), 2);
        assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![998, 999]);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn test_ring_buffer_zero_capacity() {
        let _rb: RingBuffer<i32> = RingBuffer::new(0);
    }

    #[test]
    fn test_new_empty() {
        let rb: RingBuffer<i32> = RingBuffer::new(4);
        assert!(rb.is_empty(), "new buffer must be empty");
        assert_eq!(rb.len(), 0);
        assert_eq!(rb.capacity(), 4);
    }

    #[test]
    fn test_write_one() {
        let mut rb = RingBuffer::new(4);
        rb.write(42);
        assert_eq!(rb.len(), 1);
        assert!(!rb.is_empty());
        assert_eq!(rb.last(), Some(&42));
    }

    #[test]
    fn test_to_vec_matches_iter() {
        let mut rb = RingBuffer::new(4);
        rb.write(10);
        rb.write(20);
        rb.write(30);
        let via_iter: Vec<i32> = rb.iter().copied().collect();
        let via_to_vec = rb.to_vec();
        assert_eq!(via_iter, via_to_vec, "to_vec() must match iter() output");
    }

    #[test]
    fn test_capacity_one_keeps_last() {
        let mut rb = RingBuffer::new(1);
        rb.write(1);
        rb.write(2);
        rb.write(3);
        assert_eq!(rb.len(), 1);
        assert_eq!(
            rb.last(),
            Some(&3),
            "capacity-1 buffer keeps only the most recent entry"
        );
    }
}
