//! `/proc/meminfo` reader.
//!
//! Parses the kernel format:
//! ```text
//! MemTotal:       65478972 kB
//! MemFree:        30275564 kB
//! MemAvailable:   43234556 kB
//! SwapTotal:      20970996 kB
//! SwapFree:       20970996 kB
//! ```
//!
//! Only the five fields above are extracted; all other lines are ignored.

use std::fs;

use crate::sentinel::error::PressureReadError;

const MEMINFO_PATH: &str = "/proc/meminfo";

/// Raw values parsed from `/proc/meminfo` (all in kB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeminfoReading {
    /// Total usable RAM, in kB.
    pub total_kb: u64,
    /// Estimate of available memory for starting new applications, in kB.
    pub available_kb: u64,
    /// Amount of free memory, in kB.
    pub free_kb: u64,
    /// Total swap space, in kB.
    pub swap_total_kb: u64,
    /// Unused swap space, in kB.
    pub swap_free_kb: u64,
}

/// Read memory info from `/proc/meminfo`.
pub fn read_meminfo() -> Result<MeminfoReading, PressureReadError> {
    let content = fs::read_to_string(MEMINFO_PATH).map_err(PressureReadError::MeminfoUnreadable)?;
    parse_meminfo(&content)
}

/// Parse meminfo content from a string (testable without `/proc`).
pub fn parse_meminfo(content: &str) -> Result<MeminfoReading, PressureReadError> {
    const FIELDS: [(&str, usize); 5] = [
        ("MemTotal:", 0),
        ("MemAvailable:", 1),
        ("MemFree:", 2),
        ("SwapTotal:", 3),
        ("SwapFree:", 4),
    ];
    const FIELD_NAMES: [&str; 5] = [
        "MemTotal",
        "MemAvailable",
        "MemFree",
        "SwapTotal",
        "SwapFree",
    ];
    let mut slots = [None::<u64>; 5];
    for line in content.lines() {
        for &(prefix, idx) in &FIELDS {
            if slots[idx].is_none()
                && let Some(val) = extract_kb(line, prefix)?
            {
                slots[idx] = Some(val);
            }
        }
    }
    let vals = require_all(&slots, &FIELD_NAMES)?;
    Ok(MeminfoReading {
        total_kb: vals[0],
        available_kb: vals[1],
        free_kb: vals[2],
        swap_total_kb: vals[3],
        swap_free_kb: vals[4],
    })
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// If `line` starts with `prefix`, parse and return the numeric kB value.
///
/// Returns `None` when the prefix does not match, `Some(Err)` on parse
/// failure, `Some(Ok(v))` on success.
fn extract_kb(line: &str, prefix: &'static str) -> Result<Option<u64>, PressureReadError> {
    let rest = match line.strip_prefix(prefix) {
        Some(r) => r,
        None => return Ok(None),
    };
    let num_str = rest.split_whitespace().next().unwrap_or("");
    let v = num_str
        .parse::<u64>()
        .map_err(|_| PressureReadError::ParseError {
            file: MEMINFO_PATH,
            field: prefix,
            value: num_str.to_owned(),
        })?;
    Ok(Some(v))
}

/// Convert a fixed-size slot array to concrete values, erroring on any `None`.
fn require_all(
    slots: &[Option<u64>; 5],
    names: &[&'static str; 5],
) -> Result<[u64; 5], PressureReadError> {
    let mut out = [0u64; 5];
    for (i, slot) in slots.iter().enumerate() {
        out[i] = slot.ok_or(PressureReadError::MissingField {
            field: names[i],
            path: MEMINFO_PATH,
        })?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const VALID_MEMINFO: &str = "\
MemTotal:       65478972 kB\n\
MemFree:        30275564 kB\n\
MemAvailable:   43234556 kB\n\
Buffers:         3615644 kB\n\
Cached:          9771756 kB\n\
SwapCached:            0 kB\n\
SwapTotal:      20970996 kB\n\
SwapFree:       20970996 kB\n\
Dirty:               128 kB\n";
    #[test]
    fn parse_valid_meminfo() {
        let r = parse_meminfo(VALID_MEMINFO).expect("should parse valid meminfo");
        assert_eq!(r.total_kb, 65_478_972);
        assert_eq!(r.available_kb, 43_234_556);
        assert_eq!(r.free_kb, 30_275_564);
        assert_eq!(r.swap_total_kb, 20_970_996);
        assert_eq!(r.swap_free_kb, 20_970_996);
    }
    #[test]
    fn parse_malformed_value_returns_error() {
        let bad = "\
MemTotal:       BADVAL kB\n\
MemFree:        0 kB\n\
MemAvailable:   0 kB\n\
SwapTotal:      0 kB\n\
SwapFree:       0 kB\n";
        let err = parse_meminfo(bad);
        assert!(
            matches!(err, Err(PressureReadError::ParseError { .. })),
            "expected ParseError, got: {err:?}"
        );
    }
    #[test]
    fn parse_missing_swap_free_returns_error() {
        let no_swap_free = "\
MemTotal:       65478972 kB\n\
MemFree:        30275564 kB\n\
MemAvailable:   43234556 kB\n\
SwapTotal:      20970996 kB\n";
        let err = parse_meminfo(no_swap_free);
        assert!(
            matches!(
                err,
                Err(PressureReadError::MissingField {
                    field: "SwapFree",
                    ..
                })
            ),
            "expected MissingField SwapFree, got: {err:?}"
        );
    }
    #[test]
    fn parse_missing_mem_total_returns_error() {
        let no_total = "\
MemFree:        30275564 kB\n\
MemAvailable:   43234556 kB\n\
SwapTotal:      20970996 kB\n\
SwapFree:       20970996 kB\n";
        let err = parse_meminfo(no_total);
        assert!(
            matches!(
                err,
                Err(PressureReadError::MissingField {
                    field: "MemTotal",
                    ..
                })
            ),
            "expected MissingField MemTotal, got: {err:?}"
        );
    }
    #[test]
    fn parse_extra_fields_ignored() {
        let extra = "\
MemTotal:       65478972 kB\n\
MemFree:        30275564 kB\n\
MemAvailable:   43234556 kB\n\
HugePages_Total: 0\n\
AnonHugePages:   204800 kB\n\
SwapTotal:      20970996 kB\n\
SwapFree:       20970996 kB\n";
        let r = parse_meminfo(extra).expect("extra fields must not cause errors");
        assert_eq!(r.total_kb, 65_478_972);
    }
    /// Live smoke test — only on Linux where `/proc` exists.
    #[test]
    #[cfg(target_os = "linux")]
    fn live_read_meminfo_sanity() {
        let r = read_meminfo().expect("live meminfo should be readable");
        assert!(r.total_kb > 0, "total_kb must be positive");
        assert!(r.available_kb <= r.total_kb, "available <= total");
        assert!(r.free_kb <= r.total_kb, "free <= total");
        assert!(r.swap_free_kb <= r.swap_total_kb, "swap_free <= swap_total");
    }
}
