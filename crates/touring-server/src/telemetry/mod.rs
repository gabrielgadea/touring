//! Telemetry subsystem -- eBPF or tracing-based, selected at compile time.
//!
//! When `ebpf-telemetry` feature is enabled AND the system supports it,
//! eBPF probes collect syscall-level metrics. Otherwise, falls back to
//! standard `tracing` instrumentation.

pub mod mcp_overhead;

/// Telemetry handle -- represents an active telemetry session.
#[derive(Debug)]
pub struct TelemetryHandle {
    /// Which backend is currently active.
    pub backend: TelemetryBackend,
}

/// Which telemetry backend is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryBackend {
    /// Standard tracing-subscriber based telemetry (always available).
    Tracing,
    /// eBPF-based syscall telemetry (Linux + CAP_BPF required).
    #[cfg(all(target_os = "linux", feature = "ebpf-telemetry"))]
    Ebpf,
}

impl std::fmt::Display for TelemetryBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tracing => write!(f, "tracing"),
            #[cfg(all(target_os = "linux", feature = "ebpf-telemetry"))]
            Self::Ebpf => write!(f, "eBPF"),
        }
    }
}

/// Start telemetry with the best available backend.
///
/// Tries eBPF first (if feature enabled and on Linux), falls back to tracing.
pub fn start_telemetry() -> TelemetryHandle {
    #[cfg(all(target_os = "linux", feature = "ebpf-telemetry"))]
    {
        match start_ebpf_telemetry() {
            Ok(handle) => return handle,
            Err(e) => {
                tracing::warn!("eBPF unavailable ({}), falling back to tracing", e);
            }
        }
    }

    start_tracing_telemetry()
}

/// Start standard tracing-based telemetry.
fn start_tracing_telemetry() -> TelemetryHandle {
    tracing::info!("Telemetry: using tracing backend");
    TelemetryHandle {
        backend: TelemetryBackend::Tracing,
    }
}

/// Start eBPF-based telemetry (Linux only).
#[cfg(all(target_os = "linux", feature = "ebpf-telemetry"))]
fn start_ebpf_telemetry() -> Result<TelemetryHandle, String> {
    // Check for CAP_BPF capability.
    // In production, this would load eBPF programs via aya.
    // For now, return an error to demonstrate graceful degradation.
    Err("eBPF probe loading not yet implemented".to_string())
}

/// Explicit tracing fallback for non-Linux or when eBPF is disabled.
#[cfg(not(all(target_os = "linux", feature = "ebpf-telemetry")))]
fn _fallback_telemetry() -> TelemetryHandle {
    start_tracing_telemetry()
}

// -- Cache line padding for portability ------------------------------------------------

/// Cache line size -- 128B for aarch64 (Apple Silicon), 64B for x86-64.
#[cfg(target_arch = "aarch64")]
pub(crate) const CACHE_LINE_SIZE: usize = 128;

/// Cache line size -- 64B for x86-64.
#[cfg(not(target_arch = "aarch64"))]
pub const CACHE_LINE_SIZE: usize = 64;

/// Padded atomic cursor -- prevents false sharing between cores.
#[repr(align(128))] // Conservative -- works on both architectures
#[derive(Debug)]
pub struct PaddedAtomicCursor(pub std::sync::atomic::AtomicUsize);

impl PaddedAtomicCursor {
    /// Create a new padded cursor with the given initial value.
    pub fn new(val: usize) -> Self {
        Self(std::sync::atomic::AtomicUsize::new(val))
    }

    /// Load the current value (relaxed ordering).
    pub fn load(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Store a value (relaxed ordering).
    pub fn store(&self, val: usize) {
        self.0.store(val, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_telemetry_falls_back_to_tracing() {
        let handle = start_telemetry();
        // Without eBPF feature or on non-Linux, should always use tracing
        assert_eq!(handle.backend, TelemetryBackend::Tracing);
    }

    #[test]
    fn test_telemetry_backend_display() {
        assert_eq!(TelemetryBackend::Tracing.to_string(), "tracing");
    }

    #[test]
    fn test_cache_line_size() {
        // Should be either 64 or 128 depending on architecture
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(CACHE_LINE_SIZE == 64 || CACHE_LINE_SIZE == 128);
        }
    }

    #[test]
    fn test_padded_atomic_cursor() {
        let cursor = PaddedAtomicCursor::new(42);
        assert_eq!(cursor.load(), 42);
        cursor.store(100);
        assert_eq!(cursor.load(), 100);
    }

    #[test]
    fn test_padded_cursor_alignment() {
        // PaddedAtomicCursor should be at least 128 bytes (cache line padded)
        assert!(std::mem::align_of::<PaddedAtomicCursor>() >= 128);
    }

    #[test]
    fn test_ebpf_feature_graceful_degradation() {
        // Even when eBPF feature is compiled, start_telemetry always
        // returns a valid handle (eBPF probe loading is not yet implemented).
        // This test verifies the invariant: start_telemetry() NEVER panics.
        let handle = start_telemetry();
        // On any platform without eBPF, should fall back to tracing
        assert_eq!(handle.backend, TelemetryBackend::Tracing);
        // Verify the display representation is deterministic
        assert_eq!(format!("{}", handle.backend), "tracing");
    }
}
