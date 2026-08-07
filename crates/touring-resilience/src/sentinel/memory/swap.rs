//! `/proc/vmstat` reader for swap and page-fault counters.
//!
//! Parses the kernel format (one `key value` pair per line):
//! ```text
//! pgmajfault 117011
//! pswpin 0
//! pswpout 0
//! ```
//!
//! [`SwapState`] tracks the cumulative `pgmajfault` counter across ticks
//! and exposes deltas for thrashing detection.

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::sentinel::error::PressureReadError;

const VMSTAT_PATH: &str = "/proc/vmstat";

/// Raw counters parsed from `/proc/vmstat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmstatReading {
    /// Cumulative major page faults since boot.
    pub pgmajfault: u64,
    /// Pages swapped in since boot.
    pub pswpin: u64,
    /// Pages swapped out since boot.
    pub pswpout: u64,
}

/// Read vmstat counters from `/proc/vmstat`.
pub fn read_vmstat() -> Result<VmstatReading, PressureReadError> {
    let content = fs::read_to_string(VMSTAT_PATH).map_err(PressureReadError::VmstatUnreadable)?;
    parse_vmstat(&content)
}

/// Parse vmstat content from a string (testable without `/proc`).
pub fn parse_vmstat(content: &str) -> Result<VmstatReading, PressureReadError> {
    const FIELDS: [(&str, usize); 3] = [("pgmajfault", 0), ("pswpin", 1), ("pswpout", 2)];
    const FIELD_NAMES: [&str; 3] = ["pgmajfault", "pswpin", "pswpout"];
    let mut slots = [None::<u64>; 3];
    for line in content.lines() {
        for &(key, idx) in &FIELDS {
            if slots[idx].is_none()
                && let Some(v) = extract_vmstat_field(line, key)?
            {
                slots[idx] = Some(v);
            }
        }
    }
    let vals = require_vmstat_fields(&slots, &FIELD_NAMES)?;
    Ok(VmstatReading {
        pgmajfault: vals[0],
        pswpin: vals[1],
        pswpout: vals[2],
    })
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// If `line` is `"key VALUE"`, parse and return VALUE as u64.
///
/// Returns `None` when the key does not match, `Some(Err)` on parse failure.
fn extract_vmstat_field(line: &str, key: &'static str) -> Result<Option<u64>, PressureReadError> {
    let rest = match line.strip_prefix(key) {
        Some(r) => r,
        None => return Ok(None),
    };
    let num_str = rest.split_whitespace().next().unwrap_or("");
    if num_str.is_empty() {
        return Ok(None);
    }
    let v = num_str
        .parse::<u64>()
        .map_err(|_| PressureReadError::ParseError {
            file: VMSTAT_PATH,
            field: key,
            value: num_str.to_owned(),
        })?;
    Ok(Some(v))
}

/// Convert a fixed-size slot array to concrete values, erroring on any `None`.
fn require_vmstat_fields(
    slots: &[Option<u64>; 3],
    names: &[&'static str; 3],
) -> Result<[u64; 3], PressureReadError> {
    let mut out = [0u64; 3];
    for (i, slot) in slots.iter().enumerate() {
        out[i] = slot.ok_or(PressureReadError::MissingField {
            field: names[i],
            path: VMSTAT_PATH,
        })?;
    }
    Ok(out)
}

// ── SwapState ────────────────────────────────────────────────────────────────

/// Atomic tracker for the cumulative `pgmajfault` counter.
///
/// Stores the last observed value so callers can compute per-tick deltas
/// without holding a mutex. Suitable for use in a `static` or shared
/// reference across threads.
pub struct SwapState {
    last_pgmajfault: AtomicU64,
}

impl SwapState {
    /// Sentinel value for "not yet observed".
    const UNSET: u64 = u64::MAX;
    /// Thrashing threshold: pgmajfault delta per tick that suggests heavy
    /// memory pressure.
    pub const THRASHING_THRESHOLD_PER_TICK: u64 = 100;
    /// Create a new [`SwapState`] in the unobserved state.
    pub const fn new() -> Self {
        Self {
            last_pgmajfault: AtomicU64::new(Self::UNSET),
        }
    }
    /// Record `current_pgmajfault` and return the delta since the last call.
    ///
    /// Returns `0` on the first call (no previous baseline available) and
    /// also when the counter wraps (saturating subtract).
    pub fn observe(&self, current_pgmajfault: u64) -> u64 {
        let prev = self
            .last_pgmajfault
            .swap(current_pgmajfault, Ordering::Relaxed);
        if prev == Self::UNSET || current_pgmajfault < prev {
            0
        } else {
            current_pgmajfault - prev
        }
    }
}

impl Default for SwapState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const VALID_VMSTAT: &str = "\
nr_free_pages 7568891\n\
nr_zone_inactive_anon 219622\n\
pgmajfault 117011\n\
pswpin 0\n\
pswpout 0\n\
pgfault 12345678\n";
    #[test]
    fn parse_valid_vmstat() {
        let r = parse_vmstat(VALID_VMSTAT).expect("should parse valid vmstat");
        assert_eq!(r.pgmajfault, 117_011);
        assert_eq!(r.pswpin, 0);
        assert_eq!(r.pswpout, 0);
    }
    #[test]
    fn parse_malformed_value_returns_error() {
        let bad = "pgmajfault BADVAL\npswpin 0\npswpout 0\n";
        let err = parse_vmstat(bad);
        assert!(
            matches!(err, Err(PressureReadError::ParseError { .. })),
            "expected ParseError, got: {err:?}"
        );
    }
    #[test]
    fn parse_missing_pswpout_returns_error() {
        let missing = "pgmajfault 100\npswpin 0\n";
        let err = parse_vmstat(missing);
        assert!(
            matches!(
                err,
                Err(PressureReadError::MissingField {
                    field: "pswpout",
                    ..
                })
            ),
            "expected MissingField pswpout, got: {err:?}"
        );
    }
    #[test]
    fn parse_extra_fields_ignored() {
        let extra = "other_counter 99\npgmajfault 1\npswpin 2\npswpout 3\nmore 0\n";
        let r = parse_vmstat(extra).expect("extra fields must not cause errors");
        assert_eq!(r.pgmajfault, 1);
        assert_eq!(r.pswpin, 2);
        assert_eq!(r.pswpout, 3);
    }
    #[test]
    fn swap_state_first_observe_returns_zero() {
        let s = SwapState::new();
        assert_eq!(s.observe(1000), 0, "first call must return 0");
    }
    #[test]
    fn swap_state_delta_computed_correctly() {
        let s = SwapState::new();
        s.observe(1000);
        assert_eq!(s.observe(1050), 50);
        assert_eq!(s.observe(1050), 0);
        assert_eq!(s.observe(1151), 101);
    }
    #[test]
    fn swap_state_counter_wrap_returns_zero() {
        let s = SwapState::new();
        s.observe(u64::MAX - 1);
        assert_eq!(s.observe(0), 0, "counter wrap must not produce a spike");
    }
    #[test]
    fn swap_state_thrashing_threshold_constant() {
        assert_eq!(SwapState::THRASHING_THRESHOLD_PER_TICK, 100);
    }
    #[test]
    fn swap_state_default_same_as_new() {
        let s: SwapState = Default::default();
        assert_eq!(s.observe(42), 0, "Default::default() must start unobserved");
    }
    /// Live smoke test — only on Linux where `/proc` exists.
    #[test]
    #[cfg(target_os = "linux")]
    fn live_read_vmstat_sanity() {
        let r = read_vmstat().expect("live vmstat should be readable");
        let _ = r.pgmajfault;
    }
}
