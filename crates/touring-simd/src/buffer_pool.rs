//! Thread-local buffer pool for zero-allocation batch operations.
//!
//! Reuses Vec allocations across repeated calls to avoid heap churn
//! in hot loops (e.g., batch similarity computation).

use std::alloc::{Layout, alloc, dealloc};
use std::cell::RefCell;

thread_local! {
    static F32_BUF: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    static NIBBLE_BUF: RefCell<NibbleBlock> = const { RefCell::new(NibbleBlock::empty()) };
}

/// Aligned nibble storage block with 64-byte AVX-512 alignment.
struct NibbleBlock {
    /// Pointer to allocated memory, 64-byte aligned.
    ptr: *mut u8,
    /// Allocated byte capacity (always even, accounts for nibble packing).
    capacity_bytes: usize,
}

impl NibbleBlock {
    const fn empty() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            capacity_bytes: 0,
        }
    }

    /// Ensures the block holds at least `min_bytes` nibble-packed bytes (rounded up to even).
    fn ensure_capacity(&mut self, min_bytes: usize) {
        let needed = if min_bytes % 2 == 0 {
            min_bytes
        } else {
            min_bytes + 1
        };
        if self.capacity_bytes >= needed {
            return;
        }
        // Free existing if non-null
        if !self.ptr.is_null() {
            unsafe {
                dealloc(
                    self.ptr,
                    Layout::from_size_align_unchecked(self.capacity_bytes, 64),
                );
            }
        }
        // Allocate new with 64-byte alignment
        let layout = Layout::from_size_align(needed, 64).expect("invalid layout");
        self.ptr = unsafe { alloc(layout) };
        self.capacity_bytes = needed;
    }
}

impl Drop for NibbleBlock {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                dealloc(
                    self.ptr,
                    Layout::from_size_align_unchecked(self.capacity_bytes, 64),
                );
            }
        }
    }
}

/// Execute `f` with a borrowed f32 buffer of at least `size` elements.
///
/// The buffer is reused across calls on the same thread, avoiding allocation
/// after the first call with sufficient size.
///
/// # Panics
///
/// Panics if `f` attempts to borrow the buffer recursively (non-reentrant).
pub fn with_f32_buffer<F, R>(size: usize, f: F) -> R
where
    F: FnOnce(&mut [f32]) -> R,
{
    F32_BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        buf.resize(size, 0.0);
        f(&mut buf)
    })
}

/// Execute `f` with a borrowed nibble buffer of at least `size_elements` elements.
///
/// Nibble arrays pack 2 elements per byte. The buffer is 64-byte aligned
/// (AVX-512 requirement) and reused across calls on the same thread.
///
/// # Safety
///
/// The caller must not hold the returned reference across any yield point
/// (await, lock acquisition, or other resumption boundary).
///
/// # Panics
///
/// Panics if `f` attempts to borrow the buffer recursively (non-reentrant).
pub fn with_nibble_buffer<F, R>(size_elements: usize, f: F) -> R
where
    F: FnOnce(&mut [u8]) -> R,
{
    NIBBLE_BUF.with(|cell| {
        let mut nibble_buf = cell.borrow_mut();
        // Nibble storage: 2 elements per byte, rounded up to even
        let needed_bytes = nibble_byte_count(size_elements);
        nibble_buf.ensure_capacity(needed_bytes);
        // SAFETY: ptr is guaranteed 64-byte aligned and capacity_bytes >= needed_bytes
        let slice =
            unsafe { std::slice::from_raw_parts_mut(nibble_buf.ptr, nibble_buf.capacity_bytes) };
        f(slice)
    })
}

/// Returns the byte count needed to store `n` nibbles (rounded up to even).
#[inline]
pub fn nibble_byte_count(n: usize) -> usize {
    n.div_ceil(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_reuse() {
        // First call allocates
        with_f32_buffer(100, |buf| {
            assert_eq!(buf.len(), 100);
            buf[0] = 42.0;
        });

        // Second call reuses (capacity should be >= 100)
        with_f32_buffer(50, |buf| {
            assert_eq!(buf.len(), 50);
        });

        // Larger call grows
        with_f32_buffer(200, |buf| {
            assert_eq!(buf.len(), 200);
        });
    }

    #[test]
    fn test_nibble_basic_storage() {
        with_nibble_buffer(32, |buf| {
            // Block size 32 elements = 16 bytes nibble storage
            assert_eq!(buf.len(), 16);
            buf[0] = 0x12;
            buf[1] = 0x34;
        });

        // Verify data persisted across calls
        with_nibble_buffer(32, |buf| {
            assert_eq!(buf[0], 0x12);
            assert_eq!(buf[1], 0x34);
        });
    }

    #[test]
    fn test_nibble_byte_count() {
        assert_eq!(nibble_byte_count(0), 0);
        assert_eq!(nibble_byte_count(1), 1);
        assert_eq!(nibble_byte_count(2), 1);
        assert_eq!(nibble_byte_count(3), 2);
        assert_eq!(nibble_byte_count(32), 16);
        assert_eq!(nibble_byte_count(33), 17);
    }

    #[test]
    fn test_nibble_alignment() {
        with_nibble_buffer(64, |buf| {
            // SAFETY: ptr must be 64-byte aligned for AVX-512
            let addr = buf.as_ptr() as usize;
            assert_eq!(
                addr % 64,
                0,
                "nibble buffer ptr not 64-byte aligned: address {:#x}",
                addr
            );
        });
    }
}
