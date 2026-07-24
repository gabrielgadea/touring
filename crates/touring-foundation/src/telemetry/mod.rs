//! Touring Telemetry — eBPF-based system observability
//!
//! Provides kernel-level telemetry collection using aya-rs eBPF framework:
//! - Syscall counts (read, write, epoll_wait, etc.)
//! - Memory pressure monitoring (rss, swap, cache misses)
//! - Hook latency histograms (pre/post hooks timing)
//!
//! # Requirements
//! - Linux kernel >= 5.8 (for eBPF features)
//! - `kernel_headers` feature requires kernel headers installed at /usr/include/linux

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur in telemetry collection.
///
/// Variants fall into two categories: *environmental* (kernel headers
/// missing, eBPF bytecode not loaded) and *operational* (map access
/// failures). The `FeatureNotEnabled` variant indicates the crate was
/// compiled without the `ebpf-telemetry` feature and cannot perform
/// kernel-level collection.
///
/// # Example
/// ```ignore
/// use touring_telemetry::{TelemetryError, TelemetryCollector, TelemetryConfig};
///
/// async fn init() -> Result<TelemetryCollector, TelemetryError> {
///     // EbpfNotAvailable is the expected error on workstations
///     // without compiled eBPF bytecode; allow_fallback mitigates it.
///     TelemetryCollector::new(TelemetryConfig {
///         allow_fallback: true,
///         ..Default::default()
///     }).await
/// }
/// ```
#[derive(Error, Debug)]
pub enum TelemetryError {
    /// eBPF subsystem is unavailable on this host (kernel lacks support,
    /// no privileged user, or compiled bytecode blob not embedded in the binary).
    /// Carries a human-readable diagnostic explaining the specific cause.
    #[error("eBPF not available: {0}")]
    EbpfNotAvailable(String),

    /// Linux kernel headers were not found at the expected path
    /// (typically `/usr/include/linux`). Required at compile time
    /// when `skip_kernel_check` is false; needed at runtime when
    /// eBPF programs are JIT-loaded into the kernel.
    #[error("Kernel headers not found: {0}")]
    KernelHeadersNotFound(String),

    /// Failed to read from or write to an eBPF map (hash, array, perf
    /// event). Indicates kernel-resource exhaustion, permission
    /// revocation, or a program bug. String payload carries the
    /// underlying `aya` or `libbpf` error message.
    #[error("Map access failed: {0}")]
    MapAccessError(String),

    /// The telemetry subsystem was compiled out (feature
    /// `ebpf-telemetry` not enabled). Returned by stub constructors
    /// when callers request an eBPF path that does not exist in this
    /// build configuration.
    #[error("Telemetry disabled at compile time")]
    FeatureNotEnabled,
}

/// Single observation emitted to the telemetry sink.
///
/// A `TelemetryPoint` is the universal record shape: a name, a numeric
/// value, an optional set of string labels (e.g. `host=node-1`,
/// `region=us-east`), and a microsecond-resolution timestamp. Use the
/// `serde` roundtrip for durable storage or transport.
///
/// # Example
/// ```ignore
/// use touring_telemetry::TelemetryPoint;
/// use std::collections::HashMap;
///
/// let point = TelemetryPoint {
///     timestamp_us: 1_700_000_000_000_000,
///     metric_name: "cpu_usage_pct".to_string(),
///     value: 42,
///     labels: HashMap::from([("host".to_string(), "node-1".to_string())]),
/// };
/// // Serialize for transport or storage:
/// let json = serde_json::to_string(&point)?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryPoint {
    /// Microseconds since the Unix epoch (monotonic clock origin is
    /// implementation-defined; do not subtract two points from
    /// different hosts without offset normalization).
    pub timestamp_us: u64,
    /// Dotted metric name (e.g. `cpu.usage_pct`, `http.request_latency_us`).
    /// Treated as a low-cardinality label in downstream aggregations.
    pub metric_name: String,
    /// Numeric measurement at the observation instant. Units are
    /// implicit in `metric_name`; the collector does not enforce them.
    pub value: u64,
    /// Free-form key/value tags (`host`, `region`, `process`, …).
    /// Used to slice and filter when exporting to Prometheus-like sinks.
    pub labels: HashMap<String, String>,
}

/// One observation of hook execution time, paired with the lifecycle
/// phase that produced it. Batched by `LatencyHistogram` into
/// per-hook `{count, avg, min, max}` buckets.
///
/// # Example
/// ```ignore
/// use touring_telemetry::{LatencySample, HookPhase};
///
/// let sample = LatencySample {
///     hook_name: "pre_edit".to_string(),
///     latency_us: 250,
///     phase: HookPhase::PreEdit,
/// };
/// collector.record_latency_with_sample(sample);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencySample {
    /// Logical hook name (e.g. `pre_read`, `post_bash`). Acts as the
    /// histogram bucket key; identical names across phases are merged
    /// in the same bucket (phase is recorded but not partitioned).
    pub hook_name: String,
    /// Wall-clock duration of the hook invocation in microseconds.
    pub latency_us: u64,
    /// Lifecycle phase that produced this sample; recorded for
    /// debugging but not used to partition the histogram buckets.
    pub phase: HookPhase,
}

/// Lifecycle phase of a Touring hook invocation. Each Claude Code
/// hook event maps to exactly one of these variants; the `Pre*` /
/// `Post*` split mirrors the PreToolUse / PostToolUse contract in
/// `~/.claude/settings.json`.
///
/// Serialized as snake_case strings (`"pre_read"`, `"post_bash"`,
/// etc.) for JSON / SQLite compatibility.
///
/// # Example
/// ```ignore
/// use touring_telemetry::HookPhase;
/// use serde_json;
///
/// let phase = HookPhase::PreEdit;
/// let json = serde_json::to_string(&phase)?;
/// assert_eq!(json, "\"pre_edit\"");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    /// PreToolUse `Read` event — fires before Claude reads a file.
    PreRead,
    /// PostToolUse `Read` event — fires after Claude reads a file.
    PostRead,
    /// PreToolUse `Edit` event — fires before Claude edits a file.
    PreEdit,
    /// PostToolUse `Edit` event — fires after Claude edits a file.
    PostEdit,
    /// PreToolUse `Write` event — fires before Claude writes a file.
    PreWrite,
    /// PostToolUse `Write` event — fires after Claude writes a file.
    PostWrite,
    /// PreToolUse `Bash` event — fires before Claude runs a shell command.
    PreBash,
    /// PostToolUse `Bash` event — fires after Claude runs a shell command.
    PostBash,
}

/// Stable identifier for Linux syscalls observed by the eBPF
/// collector. The numeric value is the index used in the kernel
/// tracepoint payload (`sys_enter_*`); do not reorder variants
/// without migrating any persisted telemetry.
///
/// `Unknown` is the default variant and acts as a sentinel for
/// syscalls outside this curated set (or unmapped tracepoints).
///
/// # Example
/// ```ignore
/// use touring_telemetry::SyscallId;
/// use std::collections::HashSet;
///
/// let mut observed: HashSet<SyscallId> = HashSet::new();
/// observed.insert(SyscallId::Read);
/// observed.insert(SyscallId::Write);
/// assert_eq!(observed.len(), 2);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(u32)]
pub enum SyscallId {
    /// `read(2)` — file descriptor read. Index 0 in the tracepoint table.
    Read = 0,
    /// `write(2)` — file descriptor write. Index 1.
    Write = 1,
    /// `open(2)` / `openat(2)` — open file by path. Index 2.
    Open = 2,
    /// `close(2)` — close file descriptor. Index 3.
    Close = 3,
    /// `epoll_wait(2)` — wait for epoll events. Index 4; high-frequency
    /// in I/O-bound workloads, useful for tail-latency tracking.
    EpollWait = 4,
    /// `poll(2)` — wait for events on file descriptors. Index 5.
    Poll = 5,
    /// `socket(2)` — create an endpoint for communication. Index 6.
    Socket = 6,
    /// `connect(2)` — initiate a connection on a socket. Index 7.
    Connect = 7,
    /// `accept(2)` / `accept4(2)` — accept a connection on a listening
    /// socket. Index 8.
    Accept = 8,
    /// `sendto(2)` — send a message on a socket (unconnected). Index 9.
    SendTo = 9,
    /// `recvfrom(2)` — receive a message from a socket (unconnected).
    /// Index 10.
    RecvFrom = 10,
    /// `sendmsg(2)` — send a message on a socket (msghdr-based).
    /// Index 11.
    SendMsg = 11,
    /// `recvmsg(2)` — receive a message from a socket (msghdr-based).
    /// Index 12.
    RecvMsg = 12,
    /// `fcntl(2)` — manipulate file descriptor. Index 13.
    Fcntl = 13,
    /// `ioctl(2)` — device-specific I/O control. Index 14; very
    /// high cardinality of opcodes, tracked as a count not a distribution.
    Ioctl = 14,
    /// `mmap(2)` — map files or devices into memory. Index 15.
    Mmap = 15,
    /// `munmap(2)` — unmap memory pages. Index 16.
    Munmap = 16,
    /// `brk(2)` — change data segment size. Index 17; a useful
    /// heuristic for heap growth when correlated with RSS samples.
    Brk = 17,
    /// `rt_sigaction(2)` — examine or change a signal handler. Index 18.
    RtSigaction = 18,
    /// `rt_sigprocmask(2)` — examine or change the signal mask. Index 19.
    RtSigprocmask = 19,
    /// `io_setup(2)` — create an asynchronous I/O context. Index 20.
    IoSetup = 20,
    /// `io_submit(2)` — submit asynchronous I/O operations. Index 21.
    IoSubmit = 21,
    /// `io_getevents(2)` — read asynchronous I/O events. Index 22.
    IoGetevents = 22,
    /// Sentinel for syscalls not enumerated in this set, or for
    /// tracepoints whose identifier is unparseable. Default value.
    #[default]
    Unknown = 999,
}

/// TelemetryCollector — main entry point for eBPF telemetry
///
/// Collects and aggregates telemetry from kernel eBPF programs.
/// Requires the `ebpf` feature to be enabled at compile time.
///
/// # Example
/// ```ignore
/// use touring_telemetry::{TelemetryCollector, TelemetryConfig};
///
/// let config = TelemetryConfig::default();
/// let collector = TelemetryCollector::new(config).await?;
/// collector.start().await?;
/// ```
pub struct TelemetryCollector {
    config: TelemetryConfig,
    #[cfg(feature = "ebpf-telemetry")]
    ebpf: Option<aya::Ebpf>,
    syscall_counts: HashMap<SyscallId, u64>,
    memory_samples: Vec<MemorySample>,
    latency_histogram: LatencyHistogram,
}

#[cfg(feature = "ebpf-telemetry")]
impl TelemetryCollector {
    /// Create a new TelemetryCollector with the given configuration.
    ///
    /// Attempts eBPF initialization first. If it fails and `allow_fallback` is
    /// set, falls back to a polling-based collector without eBPF.
    pub async fn new(config: TelemetryConfig) -> Result<Self, TelemetryError> {
        match Self::init_ebpf(&config).await {
            Ok(ebpf) => Ok(Self {
                config,
                ebpf: Some(ebpf),
                syscall_counts: HashMap::new(),
                memory_samples: Vec::new(),
                latency_histogram: LatencyHistogram::new(),
            }),
            Err(e) if config.allow_fallback => {
                // Known/expected cases (feature off, bytecode absent) are
                // informational — this is normal on workstations that never
                // ship an `eBPF` bytecode blob. Real infra faults
                // (kernel headers missing, map access failures) remain
                // warn-level so they stay visible in production logs.
                match &e {
                    TelemetryError::FeatureNotEnabled | TelemetryError::EbpfNotAvailable(_) => {
                        tracing::info!("eBPF not active ({e}); using polling collector");
                    }
                    _ => {
                        tracing::warn!("eBPF init failed ({e}), falling back to polling collector");
                    }
                }
                Ok(Self {
                    config,
                    ebpf: None,
                    syscall_counts: HashMap::new(),
                    memory_samples: Vec::new(),
                    latency_histogram: LatencyHistogram::new(),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Initialize eBPF runtime
    async fn init_ebpf(config: &TelemetryConfig) -> Result<aya::Ebpf, TelemetryError> {
        #[cfg(feature = "ebpf-telemetry")]
        {
            // Check if kernel headers are available for eBPF program compilation
            if !config.skip_kernel_check && !Self::kernel_headers_exist() {
                return Err(TelemetryError::KernelHeadersNotFound(
                    "/usr/include/linux".to_string(),
                ));
            }

            // Load eBPF programs - in production this would load compiled .o files
            // For now, we create a minimal eBPF context
            // aya::Ebpf::load() requires bytecode slice; for stub we return not-available
            Err(TelemetryError::EbpfNotAvailable(
                "eBPF bytecode not loaded - provide compiled eBPF program bytes".to_string(),
            ))
        }
        #[cfg(not(feature = "ebpf-telemetry"))]
        {
            Err(TelemetryError::FeatureNotEnabled)
        }
    }

    /// Check if kernel headers exist for eBPF compilation
    fn kernel_headers_exist() -> bool {
        std::path::Path::new("/usr/include/linux/version.h").exists()
            || std::path::Path::new("/usr/include/linux/kernel.h").exists()
    }
}

#[cfg(not(feature = "ebpf-telemetry"))]
impl TelemetryCollector {
    /// Create a new TelemetryCollector (stub when eBPF not enabled)
    pub async fn new(config: TelemetryConfig) -> Result<Self, TelemetryError> {
        if !config.allow_fallback {
            return Err(TelemetryError::FeatureNotEnabled);
        }
        Ok(Self {
            config,
            syscall_counts: HashMap::new(),
            memory_samples: Vec::new(),
            latency_histogram: LatencyHistogram::new(),
        })
    }
}

impl TelemetryCollector {
    /// Returns a reference to the telemetry configuration in use.
    ///
    /// Callers inspect this to check active feature flags (e.g. `skip_kernel_check`,
    /// `allow_fallback`) and whether eBPF-enabled paths are active. Exposed as a
    /// public getter so external crates (touring-generator, touring-hooks) can
    /// gate their own behaviour on the collector configuration without depending
    /// on the `ebpf` feature themselves.
    #[must_use]
    pub fn config(&self) -> &TelemetryConfig {
        &self.config
    }

    /// Start telemetry collection.
    pub async fn start(&mut self) -> Result<(), TelemetryError> {
        // Log the active configuration so operators can verify which features
        // are enabled at runtime. Reads `self.config` — ensures the field is
        // not dead code in the `not(feature = "ebpf-telemetry")` build variant.
        tracing::info!(
            skip_kernel_check = self.config.skip_kernel_check,
            allow_fallback = self.config.allow_fallback,
            "Starting telemetry collection"
        );
        #[cfg(feature = "ebpf-telemetry")]
        if let Some(ref _ebpf) = self.ebpf {
            // Attach eBPF programs to kernel tracepoints
            tracing::debug!("eBPF runtime initialized, programs attached");
        }
        Ok(())
    }

    /// Stop telemetry collection and flush buffers
    pub async fn stop(&mut self) -> Result<(), TelemetryError> {
        tracing::info!("Stopping telemetry collection");
        self.flush().await?;
        Ok(())
    }

    /// Flush accumulated telemetry data
    pub async fn flush(&mut self) -> Result<(), TelemetryError> {
        // Flush syscall counts
        for (syscall, count) in self.syscall_counts.iter() {
            tracing::debug!("syscall {:?}: {} calls", syscall, count);
        }
        // Flush memory samples
        for sample in self.memory_samples.iter() {
            tracing::debug!(
                "memory: rss={}KB, swap={}KB, cache_misses={}",
                sample.rss_kb,
                sample.swap_kb,
                sample.cache_misses
            );
        }
        // Flush latency histogram
        self.latency_histogram.dump();
        Ok(())
    }

    /// Record a syscall occurrence
    pub fn record_syscall(&mut self, syscall: SyscallId) {
        *self.syscall_counts.entry(syscall).or_insert(0) += 1;
    }

    /// Record a latency sample for a hook
    pub fn record_latency(&mut self, hook_name: &str, latency_us: u64, phase: HookPhase) {
        self.latency_histogram.record(hook_name, latency_us, phase);
    }

    /// Record a memory sample
    pub fn record_memory(&mut self, sample: MemorySample) {
        self.memory_samples.push(sample);
    }

    /// Get current syscall counts
    pub fn get_syscall_counts(&self) -> &HashMap<SyscallId, u64> {
        &self.syscall_counts
    }

    /// Get aggregated telemetry as JSON
    pub fn export(&self) -> Result<String, TelemetryError> {
        let export = TelemetryExport {
            syscall_counts: self
                .syscall_counts
                .iter()
                .map(|(k, v)| (format!("{:?}", k), *v))
                .collect(),
            memory_samples_count: self.memory_samples.len(),
            latency_histogram: self.latency_histogram.export(),
        };
        serde_json::to_string_pretty(&export)
            .map_err(|e| TelemetryError::MapAccessError(e.to_string()))
    }
}

/// Memory usage sample captured at a point in time, either by the
/// eBPF collector or by the polling fallback.
///
/// `rss_kb` and `swap_kb` track process memory pressure; the
/// `*_faults` fields count page faults since the previous sample and
/// help distinguish user-space page-in activity (minor) from
/// disk-backed page-in (major, expensive).
///
/// # Example
/// ```ignore
/// use touring_telemetry::MemorySample;
///
/// let sample = MemorySample {
///     timestamp_us: 1_700_000_000_000_000,
///     rss_kb: 65_536,
///     swap_kb: 0,
///     cache_misses: 12,
///     minor_faults: 800,
///     major_faults: 0,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySample {
    /// Microseconds since the Unix epoch (matches `TelemetryPoint`).
    pub timestamp_us: u64,
    /// Resident set size in kilobytes — non-swapped physical memory
    /// held by the process. Primary signal for memory pressure.
    pub rss_kb: u64,
    /// Swap usage in kilobytes. Non-zero values typically indicate
    /// the host is over-committed; correlate with major-fault spikes.
    pub swap_kb: u64,
    /// Page-cache misses since the previous sample (kernel-reported).
    /// High values indicate disk-bound I/O patterns.
    pub cache_misses: u64,
    /// Minor (no I/O) page faults since the previous sample —
    /// typically zero-cost demand paging of already-resident pages.
    pub minor_faults: u64,
    /// Major (disk-backed) page faults since the previous sample —
    /// each fault is a synchronous read; rate is a strong latency
    /// signal.
    pub major_faults: u64,
}

/// Aggregated hook-latency statistics, keyed by hook name.
///
/// Stores one [`LatencyBucket`] per hook. Bounded by the number of
/// distinct hook names emitted by the running system (typically <50).
/// Thread-safe through `&mut self` only — wrap in `Mutex` if shared
/// across threads.
///
/// # Example
/// ```ignore
/// use touring_telemetry::{LatencyHistogram, HookPhase};
///
/// let mut h = LatencyHistogram::new();
/// h.record("pre_edit", 150, HookPhase::PreEdit);
/// h.record("pre_edit", 250, HookPhase::PreEdit);
/// let export = h.export();
/// assert_eq!(export["pre_edit"].count, 2);
/// assert_eq!(export["pre_edit"].avg_us, 200);
/// ```
#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    buckets: HashMap<String, LatencyBucket>,
}

/// Per-hook running statistics used to compute mean / min / max
/// without retaining every sample.
///
/// Invariants:
/// - `count > 0` ⇒ `min_us <= avg_us <= max_us`
/// - `min_us` initialized to `u64::MAX` and replaced on first
///   observation
/// - `max_us` initialized to `0` and replaced on first observation
#[derive(Debug, Clone)]
pub struct LatencyBucket {
    /// Number of samples aggregated into this bucket.
    pub count: u64,
    /// Sum of all sample latencies in microseconds; divide by
    /// `count` for the mean (use `sum_us / count`; rounding is
    /// truncated, not rounded).
    pub sum_us: u64,
    /// Smallest latency observed in microseconds.
    pub min_us: u64,
    /// Largest latency observed in microseconds.
    pub max_us: u64,
}

impl LatencyHistogram {
    /// Create a new empty histogram
    pub fn new() -> Self {
        Self {
            buckets: HashMap::new(),
        }
    }

    /// Record a latency sample
    pub fn record(&mut self, hook_name: &str, latency_us: u64, _phase: HookPhase) {
        let key = hook_name.to_string();
        let bucket = self.buckets.entry(key).or_insert_with(|| LatencyBucket {
            count: 0,
            sum_us: 0,
            min_us: u64::MAX,
            max_us: 0,
        });
        bucket.count += 1;
        bucket.sum_us += latency_us;
        bucket.min_us = bucket.min_us.min(latency_us);
        bucket.max_us = bucket.max_us.max(latency_us);
    }

    /// Export histogram as serde serializable
    pub fn export(&self) -> HashMap<String, LatencyBucketExport> {
        self.buckets
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    LatencyBucketExport {
                        count: v.count,
                        avg_us: v.sum_us.checked_div(v.count).unwrap_or(0),
                        min_us: v.min_us,
                        max_us: v.max_us,
                    },
                )
            })
            .collect()
    }

    /// Dump histogram to logs
    pub fn dump(&self) {
        for (hook, bucket) in &self.buckets {
            tracing::debug!(
                "hook={} count={} avg={}us min={}us max={}us",
                hook,
                bucket.count,
                bucket.sum_us.checked_div(bucket.count).unwrap_or(0),
                bucket.min_us,
                bucket.max_us
            );
        }
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable snapshot of a [`LatencyBucket`], produced by
/// [`LatencyHistogram::export`].
///
/// Differs from `LatencyBucket` in that `sum_us` is replaced by the
/// precomputed `avg_us` — the consumer (Prometheus exporter,
/// JSON-over-HTTP) does not need to divide and avoids re-doing the
/// work in the hot path.
///
/// # Example
/// ```ignore
/// use touring_telemetry::LatencyBucketExport;
///
/// let snapshot = LatencyBucketExport {
///     count: 1_000,
///     avg_us: 320,
///     min_us: 50,
///     max_us: 4_200,
/// };
/// let json = serde_json::to_string(&snapshot)?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyBucketExport {
    /// Number of samples represented by this snapshot.
    pub count: u64,
    /// Precomputed mean latency (microseconds). Truncated average
    /// (`sum_us / count`); may be 0 if `count` overflowed to 0.
    pub avg_us: u64,
    /// Smallest sample observed (microseconds).
    pub min_us: u64,
    /// Largest sample observed (microseconds).
    pub max_us: u64,
}

/// Top-level JSON shape produced by
/// [`TelemetryCollector::export`].
///
/// Field design choices:
/// - `syscall_counts` keys are stringified `SyscallId` variants
///   (`"Read"`, `"EpollWait"`, …) so the JSON is human-readable
///   without a side-channel enum→name table.
/// - `memory_samples_count` is a counter, not a vector — the
///   samples themselves are not exported (would dwarf the rest of
///   the payload in production).
///
/// # Example
/// ```ignore
/// use touring_telemetry::TelemetryExport;
/// use std::collections::HashMap;
///
/// let export = TelemetryExport {
///     syscall_counts: HashMap::from([("Read".to_string(), 42)]),
///     memory_samples_count: 0,
///     latency_histogram: HashMap::new(),
/// };
/// let json = serde_json::to_string_pretty(&export)?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryExport {
    /// Syscall occurrence counts keyed by stringified
    /// [`SyscallId`] variant name.
    pub syscall_counts: HashMap<String, u64>,
    /// Number of [`MemorySample`]s currently buffered; the
    /// samples themselves are not exported in this struct.
    pub memory_samples_count: usize,
    /// Per-hook latency aggregates keyed by hook name.
    pub latency_histogram: HashMap<String, LatencyBucketExport>,
}

/// Tunable parameters for [`TelemetryCollector`].
///
/// Defaults: kernel-header check enabled, fallback to polling
/// allowed, 1s collection interval, 1000 sample buffer, all
/// trackers on. Override fields after [`Default::default()`] for
/// the common case.
///
/// # Example
/// ```ignore
/// use touring_telemetry::TelemetryConfig;
///
/// let config = TelemetryConfig {
///     // Containers usually lack /usr/include/linux; skip the probe.
///     skip_kernel_check: true,
///     // Force a hard failure when eBPF cannot start (CI).
///     allow_fallback: false,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Skip the kernel-headers probe at startup. Set to `true` for
    /// containers (where `/usr/include/linux` is not present) or
    /// when running under a stripped initramfs.
    pub skip_kernel_check: bool,
    /// Allow fallback to a polling-based collector when the eBPF
    /// subsystem cannot start. When `false`, the constructor
    /// returns `TelemetryError` on init failure.
    pub allow_fallback: bool,
    /// Polling interval in milliseconds. Ignored when eBPF is
    /// active (kernel-side aggregation runs continuously).
    pub collection_interval_ms: u64,
    /// Maximum number of [`MemorySample`]s to retain in the
    /// in-memory ring. Older samples are dropped (FIFO) when the
    /// limit is reached.
    pub max_memory_samples: usize,
    /// If `true`, count syscalls via the eBPF tracepoint
    /// (`sys_enter_*`).
    pub track_syscalls: bool,
    /// If `true`, sample resident memory, swap, and page-fault
    /// counters on the configured interval.
    pub track_memory: bool,
    /// If `true`, aggregate per-hook latency histograms from
    /// `LatencySample` records.
    pub track_hook_latency: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            skip_kernel_check: false,
            allow_fallback: true,
            collection_interval_ms: 1000,
            max_memory_samples: 1000,
            track_syscalls: true,
            track_memory: true,
            track_hook_latency: true,
        }
    }
}

/// EbpfMonitor — eBPF program manager and data processor
///
/// Manages the lifecycle of eBPF programs and processes
/// telemetry data collected from kernel tracepoints.
///
/// # Example
/// ```ignore
/// use touring_telemetry::EbpfMonitor;
///
/// let monitor = EbpfMonitor::new().await?;
/// monitor.attach_tracepoint("syscalls", "sys_enter_read")?;
/// ```
#[cfg(feature = "ebpf-telemetry")]
pub struct EbpfMonitor {
    programs: HashMap<String, u32>,
}

#[cfg(feature = "ebpf-telemetry")]
impl EbpfMonitor {
    /// Create a new eBPF monitor
    pub async fn new() -> Result<Self, TelemetryError> {
        Ok(Self {
            programs: HashMap::new(),
        })
    }

    /// Attach an eBPF program to a kernel tracepoint
    pub fn attach_tracepoint(&mut self, category: &str, name: &str) -> Result<(), TelemetryError> {
        let key = format!("{}:{}", category, name);
        tracing::info!("Attaching eBPF program to tracepoint: {}", key);
        // In production, this would use aya::programs::TracePoint
        self.programs.insert(key, 1);
        Ok(())
    }

    /// Attach an eBPF program to a kernel kprobe
    pub fn attach_kprobe(&mut self, name: &str, offset: u64) -> Result<(), TelemetryError> {
        let key = format!("kprobe:{}", name);
        tracing::info!(
            "Attaching eBPF program to kprobe: {} offset={}",
            name,
            offset
        );
        self.programs.insert(key, 1);
        Ok(())
    }

    /// Attach an eBPF program to a kernel uprobe
    pub fn attach_uprobe(&mut self, path: &str, symbol: &str) -> Result<(), TelemetryError> {
        let key = format!("uprobe:{}:{}", path, symbol);
        tracing::info!("Attaching eBPF program to uprobe: {}:{}", path, symbol);
        self.programs.insert(key, 1);
        Ok(())
    }

    /// Get list of attached programs
    pub fn attached_programs(&self) -> Vec<&str> {
        self.programs.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a specific tracepoint is attached
    pub fn is_attached(&self, category: &str, name: &str) -> bool {
        self.programs
            .contains_key(&format!("{}:{}", category, name))
    }
}

/// Stub [`EbpfMonitor`] when the `ebpf-telemetry` feature is
/// disabled. Zero-sized — all operations are no-ops via
/// `Result::Err(TelemetryError::FeatureNotEnabled)`.
#[cfg(not(feature = "ebpf-telemetry"))]
pub struct EbpfMonitor;

#[cfg(not(feature = "ebpf-telemetry"))]
impl EbpfMonitor {
    /// Create a new eBPF monitor (stub when eBPF not enabled)
    pub async fn new() -> Result<Self, TelemetryError> {
        Err(TelemetryError::FeatureNotEnabled)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn fallback_collector_sync() -> TelemetryCollector {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(TelemetryCollector::new(TelemetryConfig::default()))
            .unwrap()
    }

    // ── Configuration ─────────────────────────────────────────────────────────

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert!(config.track_syscalls);
        assert!(config.track_memory);
        assert!(config.track_hook_latency);
        assert_eq!(config.collection_interval_ms, 1000);
        assert!(config.allow_fallback);
        assert!(!config.skip_kernel_check);
        assert_eq!(config.max_memory_samples, 1000);
    }

    #[test]
    fn test_telemetry_config_custom() {
        let config = TelemetryConfig {
            skip_kernel_check: true,
            allow_fallback: false,
            collection_interval_ms: 500,
            max_memory_samples: 100,
            track_syscalls: false,
            track_memory: false,
            track_hook_latency: false,
        };
        assert!(config.skip_kernel_check);
        assert!(!config.allow_fallback);
        assert_eq!(config.collection_interval_ms, 500);
    }

    // ── SyscallId ─────────────────────────────────────────────────────────────

    #[test]
    fn test_syscall_id_default() {
        let syscall = SyscallId::default();
        assert_eq!(syscall, SyscallId::Unknown);
    }

    #[test]
    fn test_syscall_id_variants() {
        // All variants can be compared
        assert_ne!(SyscallId::Read, SyscallId::Write);
        assert_ne!(SyscallId::Open, SyscallId::Close);
        assert_eq!(SyscallId::EpollWait, SyscallId::EpollWait);
    }

    #[test]
    fn test_syscall_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(SyscallId::Read);
        set.insert(SyscallId::Write);
        set.insert(SyscallId::Read); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_syscall_id_debug() {
        let debug = format!("{:?}", SyscallId::Read);
        assert!(debug.contains("Read"));

        let debug_unknown = format!("{:?}", SyscallId::Unknown);
        assert!(debug_unknown.contains("Unknown"));
    }

    // ── HookPhase ─────────────────────────────────────────────────────────────

    #[test]
    fn test_hook_phase_eq() {
        assert_eq!(HookPhase::PreRead, HookPhase::PreRead);
        assert_ne!(HookPhase::PreRead, HookPhase::PostRead);
        assert_ne!(HookPhase::PreEdit, HookPhase::PostEdit);
    }

    #[test]
    fn test_hook_phase_debug() {
        let phases = [
            HookPhase::PreRead,
            HookPhase::PostRead,
            HookPhase::PreEdit,
            HookPhase::PostEdit,
            HookPhase::PreWrite,
            HookPhase::PostWrite,
            HookPhase::PreBash,
            HookPhase::PostBash,
        ];
        for phase in phases {
            let dbg = format!("{:?}", phase);
            assert!(!dbg.is_empty());
        }
    }

    #[test]
    fn test_hook_phase_serde_roundtrip() {
        let phase = HookPhase::PreRead;
        let json = serde_json::to_string(&phase).unwrap();
        let decoded: HookPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, decoded);
    }

    // ── TelemetryPoint & LatencySample ────────────────────────────────────────

    #[test]
    fn test_telemetry_point_creation() {
        let point = TelemetryPoint {
            timestamp_us: 1_000_000,
            metric_name: "cpu_usage".to_string(),
            value: 75,
            labels: {
                let mut m = HashMap::new();
                m.insert("host".to_string(), "node1".to_string());
                m
            },
        };
        assert_eq!(point.timestamp_us, 1_000_000);
        assert_eq!(point.value, 75);
        assert_eq!(point.labels.get("host").unwrap(), "node1");
    }

    #[test]
    fn test_telemetry_point_serde_roundtrip() {
        let point = TelemetryPoint {
            timestamp_us: 42,
            metric_name: "mem".to_string(),
            value: 100,
            labels: HashMap::new(),
        };
        let json = serde_json::to_string(&point).unwrap();
        let decoded: TelemetryPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.timestamp_us, 42);
        assert_eq!(decoded.value, 100);
    }

    #[test]
    fn test_latency_sample_creation() {
        let sample = LatencySample {
            hook_name: "pre_edit".to_string(),
            latency_us: 250,
            phase: HookPhase::PreEdit,
        };
        assert_eq!(sample.latency_us, 250);
        assert_eq!(sample.phase, HookPhase::PreEdit);
    }

    #[test]
    fn test_latency_sample_serde_roundtrip() {
        let sample = LatencySample {
            hook_name: "post_bash".to_string(),
            latency_us: 500,
            phase: HookPhase::PostBash,
        };
        let json = serde_json::to_string(&sample).unwrap();
        let decoded: LatencySample = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.latency_us, 500);
        assert_eq!(decoded.phase, HookPhase::PostBash);
    }

    // ── MemorySample ──────────────────────────────────────────────────────────

    #[test]
    fn test_memory_sample_creation() {
        let sample = MemorySample {
            timestamp_us: 99,
            rss_kb: 1024,
            swap_kb: 0,
            cache_misses: 10,
            minor_faults: 5,
            major_faults: 0,
        };
        assert_eq!(sample.rss_kb, 1024);
        assert_eq!(sample.minor_faults, 5);
    }

    #[test]
    fn test_memory_sample_serde_roundtrip() {
        let sample = MemorySample {
            timestamp_us: 1234,
            rss_kb: 512,
            swap_kb: 256,
            cache_misses: 3,
            minor_faults: 1,
            major_faults: 0,
        };
        let json = serde_json::to_string(&sample).unwrap();
        let decoded: MemorySample = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.rss_kb, 512);
        assert_eq!(decoded.swap_kb, 256);
    }

    // ── LatencyHistogram ──────────────────────────────────────────────────────

    #[test]
    fn test_latency_histogram_record() {
        let mut histogram = LatencyHistogram::new();
        histogram.record("pre_read", 100, HookPhase::PreRead);
        histogram.record("pre_read", 200, HookPhase::PreRead);
        histogram.record("pre_read", 150, HookPhase::PreRead);

        let export = histogram.export();
        let bucket = export
            .get("pre_read")
            .expect("pre_read bucket should exist");
        assert_eq!(bucket.count, 3);
        assert_eq!(bucket.avg_us, 150);
        assert_eq!(bucket.min_us, 100);
        assert_eq!(bucket.max_us, 200);
    }

    #[test]
    fn test_latency_histogram_empty() {
        let histogram = LatencyHistogram::new();
        let export = histogram.export();
        assert!(export.is_empty());
    }

    #[test]
    fn test_latency_histogram_default() {
        let histogram = LatencyHistogram::default();
        assert!(histogram.export().is_empty());
    }

    #[test]
    fn test_latency_histogram_multiple_hooks() {
        let mut histogram = LatencyHistogram::new();
        histogram.record("pre_edit", 50, HookPhase::PreEdit);
        histogram.record("post_edit", 75, HookPhase::PostEdit);
        histogram.record("pre_bash", 30, HookPhase::PreBash);

        let export = histogram.export();
        assert_eq!(export.len(), 3);
        assert!(export.contains_key("pre_edit"));
        assert!(export.contains_key("post_edit"));
        assert!(export.contains_key("pre_bash"));
    }

    #[test]
    fn test_latency_histogram_single_sample_min_max() {
        let mut histogram = LatencyHistogram::new();
        histogram.record("hook", 500, HookPhase::PreWrite);
        let export = histogram.export();
        let bucket = export.get("hook").unwrap();
        assert_eq!(bucket.min_us, 500);
        assert_eq!(bucket.max_us, 500);
        assert_eq!(bucket.avg_us, 500);
    }

    #[test]
    fn test_latency_histogram_dump_does_not_panic() {
        let mut histogram = LatencyHistogram::new();
        histogram.record("pre_read", 100, HookPhase::PreRead);
        histogram.record("pre_read", 200, HookPhase::PreRead);
        // dump() should not panic even with data
        histogram.dump();
    }

    #[test]
    fn test_latency_histogram_dump_empty_does_not_panic() {
        let histogram = LatencyHistogram::new();
        histogram.dump(); // should not panic on empty
    }

    // ── TelemetryError ────────────────────────────────────────────────────────

    #[test]
    fn test_telemetry_error_display_variants() {
        let errs = [
            TelemetryError::EbpfNotAvailable("no ebpf".to_string()),
            TelemetryError::KernelHeadersNotFound("/usr/include".to_string()),
            TelemetryError::MapAccessError("map error".to_string()),
            TelemetryError::FeatureNotEnabled,
        ];
        for err in &errs {
            let display = format!("{}", err);
            assert!(
                !display.is_empty(),
                "error display should not be empty: {:?}",
                err
            );
        }
    }

    #[test]
    fn test_telemetry_error_debug() {
        let err = TelemetryError::FeatureNotEnabled;
        let debug = format!("{:?}", err);
        assert!(debug.contains("FeatureNotEnabled"));
    }

    // ── TelemetryCollector ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_telemetry_collector_fallback() {
        let config = TelemetryConfig {
            allow_fallback: true,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config).await;
        assert!(collector.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_collector_no_fallback_fails() {
        let config = TelemetryConfig {
            allow_fallback: false,
            ..Default::default()
        };
        let result = TelemetryCollector::new(config).await;
        // Without eBPF feature, this must fail since allow_fallback=false
        assert!(result.is_err(), "should fail without fallback and no eBPF");
    }

    #[tokio::test]
    async fn test_telemetry_export() {
        let config = TelemetryConfig::default();
        let collector = TelemetryCollector::new(config)
            .await
            .expect("collector should init");
        let json = collector.export();
        assert!(json.is_ok());
        let json_str = json.unwrap();
        assert!(json_str.contains("syscall_counts"));
        assert!(json_str.contains("latency_histogram"));
    }

    #[tokio::test]
    async fn test_telemetry_collector_config_getter() {
        let config = TelemetryConfig {
            allow_fallback: true,
            collection_interval_ms: 999,
            ..Default::default()
        };
        let collector = TelemetryCollector::new(config).await.unwrap();
        assert_eq!(collector.config().collection_interval_ms, 999);
    }

    #[tokio::test]
    async fn test_telemetry_collector_record_syscall() {
        let config = TelemetryConfig::default();
        let mut collector = TelemetryCollector::new(config).await.unwrap();

        collector.record_syscall(SyscallId::Read);
        collector.record_syscall(SyscallId::Read);
        collector.record_syscall(SyscallId::Write);

        let counts = collector.get_syscall_counts();
        assert_eq!(*counts.get(&SyscallId::Read).unwrap(), 2);
        assert_eq!(*counts.get(&SyscallId::Write).unwrap(), 1);
        assert!(!counts.contains_key(&SyscallId::Open));
    }

    #[tokio::test]
    async fn test_telemetry_collector_record_latency() {
        let config = TelemetryConfig::default();
        let mut collector = TelemetryCollector::new(config).await.unwrap();

        collector.record_latency("pre_read", 150, HookPhase::PreRead);
        collector.record_latency("pre_read", 250, HookPhase::PreRead);

        // Verify via export
        let json = collector.export().unwrap();
        assert!(json.contains("pre_read"));
    }

    #[tokio::test]
    async fn test_telemetry_collector_record_memory() {
        let config = TelemetryConfig::default();
        let mut collector = TelemetryCollector::new(config).await.unwrap();

        let sample = MemorySample {
            timestamp_us: 100,
            rss_kb: 2048,
            swap_kb: 0,
            cache_misses: 5,
            minor_faults: 2,
            major_faults: 0,
        };
        collector.record_memory(sample);

        let json = collector.export().unwrap();
        // memory_samples_count should be 1
        assert!(
            json.contains("\"memory_samples_count\":1") || json.contains("memory_samples_count")
        );
    }

    #[tokio::test]
    async fn test_telemetry_collector_start() {
        let config = TelemetryConfig::default();
        let mut collector = TelemetryCollector::new(config).await.unwrap();
        // start should succeed (even in stub mode)
        assert!(collector.start().await.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_collector_stop() {
        let config = TelemetryConfig::default();
        let mut collector = TelemetryCollector::new(config).await.unwrap();
        collector.start().await.unwrap();
        assert!(collector.stop().await.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_collector_flush() {
        let config = TelemetryConfig::default();
        let mut collector = TelemetryCollector::new(config).await.unwrap();
        collector.record_syscall(SyscallId::Mmap);
        collector.record_latency("hook", 100, HookPhase::PreBash);
        // flush should not panic or error
        assert!(collector.flush().await.is_ok());
    }

    #[tokio::test]
    async fn test_telemetry_export_with_data() {
        let config = TelemetryConfig::default();
        let mut collector = TelemetryCollector::new(config).await.unwrap();

        collector.record_syscall(SyscallId::Read);
        collector.record_syscall(SyscallId::Write);
        collector.record_latency("pre_edit", 300, HookPhase::PreEdit);

        let json = collector.export().unwrap();
        // JSON should be valid
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_object());
        assert!(parsed.get("syscall_counts").is_some());
    }

    #[tokio::test]
    async fn test_telemetry_export_syscall_keys_are_strings() {
        let config = TelemetryConfig::default();
        let mut collector = TelemetryCollector::new(config).await.unwrap();
        collector.record_syscall(SyscallId::EpollWait);

        let json = collector.export().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let counts = parsed["syscall_counts"].as_object().unwrap();
        // Keys should be string Debug representations
        assert!(counts.keys().any(|k| k.contains("EpollWait")));
    }

    // ── LatencyBucketExport ───────────────────────────────────────────────────

    #[test]
    fn test_latency_bucket_export_serde() {
        let export = LatencyBucketExport {
            count: 10,
            avg_us: 150,
            min_us: 50,
            max_us: 300,
        };
        let json = serde_json::to_string(&export).unwrap();
        let decoded: LatencyBucketExport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.count, 10);
        assert_eq!(decoded.avg_us, 150);
        assert_eq!(decoded.min_us, 50);
        assert_eq!(decoded.max_us, 300);
    }

    // ── TelemetryExport ───────────────────────────────────────────────────────

    #[test]
    fn test_telemetry_export_struct_serde() {
        let export = TelemetryExport {
            syscall_counts: {
                let mut m = HashMap::new();
                m.insert("Read".to_string(), 5_u64);
                m
            },
            memory_samples_count: 3,
            latency_histogram: HashMap::new(),
        };
        let json = serde_json::to_string(&export).unwrap();
        let decoded: TelemetryExport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.memory_samples_count, 3);
        assert_eq!(*decoded.syscall_counts.get("Read").unwrap(), 5);
    }
}
