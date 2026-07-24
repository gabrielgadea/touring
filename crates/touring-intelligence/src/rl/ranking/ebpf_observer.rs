//! eBPF observability interface (feature-gated stub).
//!
//! Provides a placeholder for kernel-level metrics collection via eBPF.
//! Currently a stub — activate feature `"ebpf"` for a real implementation
//! using the `aya` crate.
//!
//! # Feature Gate
//!
//! Without the `"ebpf"` feature (default), all operations are no-ops.
//! With `"ebpf"` enabled, the implementation would use `aya` to load
//! and interact with eBPF programs in the kernel.

/// eBPF observability interface.
///
/// Currently a stub — activate feature `"ebpf"` for real implementation.
/// Uses the `aya` crate when available for kernel-level metrics collection.
#[derive(Debug, Clone)]
pub struct EbpfObserver {
    enabled: bool,
    program_name: String,
}

impl EbpfObserver {
    /// Create a new eBPF observer for the given program name.
    #[must_use]
    pub fn new(program_name: impl Into<String>) -> Self {
        Self {
            enabled: false,
            program_name: program_name.into(),
        }
    }

    /// Attempt to load the eBPF program. Returns `false` if not supported.
    ///
    /// Without the `"ebpf"` feature, this always returns `false`.
    pub fn try_load(&mut self) -> bool {
        #[cfg(feature = "ebpf")]
        {
            // Real aya loading would go here:
            // let bpf = Bpf::load(include_bytes_aligned!("path/to/prog.o"))?;
            self.enabled = true;
            true
        }
        #[cfg(not(feature = "ebpf"))]
        {
            tracing::debug!(
                program = %self.program_name,
                "eBPF not compiled in — using stub observer"
            );
            false
        }
    }

    /// Whether the eBPF program is loaded and active.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The name of the eBPF program this observer targets.
    #[must_use]
    pub fn program_name(&self) -> &str {
        &self.program_name
    }

    /// Collect metrics from the eBPF program.
    ///
    /// Returns a list of `(metric_name, value)` pairs.
    /// Without the `"ebpf"` feature, always returns an empty `Vec`.
    #[must_use]
    pub fn collect_metrics(&self) -> Vec<(String, u64)> {
        if self.enabled {
            // Real implementation would read from BPF maps here
            vec![]
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ebpf_observer_new() {
        let obs = EbpfObserver::new("touring_latency");
        assert_eq!(obs.program_name(), "touring_latency");
        assert!(!obs.is_enabled());
    }

    #[test]
    fn test_try_load_behavior() {
        let mut obs = EbpfObserver::new("test_prog");
        #[cfg(feature = "ebpf")]
        {
            assert!(
                obs.try_load(),
                "ebpf feature enabled: try_load should succeed"
            );
            assert!(obs.is_enabled());
        }
        #[cfg(not(feature = "ebpf"))]
        {
            assert!(
                !obs.try_load(),
                "ebpf feature disabled: try_load should fail"
            );
            assert!(!obs.is_enabled());
        }
    }

    #[test]
    fn test_collect_metrics_returns_empty_without_ebpf() {
        let obs = EbpfObserver::new("test_prog");
        let metrics = obs.collect_metrics();
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_debug_format() {
        let obs = EbpfObserver::new("my_prog");
        let debug = format!("{obs:?}");
        assert!(debug.contains("EbpfObserver"));
        assert!(debug.contains("my_prog"));
    }

    #[test]
    fn test_clone() {
        let obs = EbpfObserver::new("cloneable");
        let cloned = obs.clone();
        assert_eq!(cloned.program_name(), "cloneable");
        assert!(!cloned.is_enabled());
    }
}
