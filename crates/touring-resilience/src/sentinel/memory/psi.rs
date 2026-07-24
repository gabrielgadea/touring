//! PSI (Pressure Stall Information) reader for `/proc/pressure/memory`.
//!
//! Parses the kernel format:
//! ```text
//! some avg10=0.00 avg60=0.00 avg300=0.00 total=89
//! full avg10=0.00 avg60=0.00 avg300=0.00 total=57
//! ```
//!
//! Available on Linux kernels ≥ 4.20 with PSI enabled (`CONFIG_PSI=y`).
//! Returns [`PressureReadError::PsiUnavailable`] on older kernels or when
//! PSI is disabled at boot (`psi=0`).

use std::fs;

use crate::sentinel::error::PressureReadError;

const PSI_PATH: &str = "/proc/pressure/memory";

/// Raw PSI averages parsed from `/proc/pressure/memory`.
#[derive(Debug, Clone, PartialEq)]
pub struct PsiReading {
    /// `some` stall avg over the last 10 s (percent).
    pub some_avg10: f32,
    /// `some` stall avg over the last 60 s (percent).
    pub some_avg60: f32,
    /// `some` stall avg over the last 300 s (percent).
    pub some_avg300: f32,
    /// `full` stall avg over the last 10 s (percent).
    pub full_avg10: f32,
    /// `full` stall avg over the last 60 s (percent).
    pub full_avg60: f32,
    /// `full` stall avg over the last 300 s (percent).
    pub full_avg300: f32,
}

/// Read PSI data from `/proc/pressure/memory`.
///
/// Returns `Err(PressureReadError::PsiUnavailable)` when the file is
/// absent (kernel < 4.20 or `psi=0` boot option). All other I/O errors
/// are propagated via `MeminfoUnreadable`.
pub fn read_psi() -> Result<PsiReading, PressureReadError> {
    let content = match fs::read_to_string(PSI_PATH) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(PressureReadError::PsiUnavailable);
        }
        Err(e) => return Err(PressureReadError::MeminfoUnreadable(e)),
    };
    parse_psi(&content)
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Find `key=VALUE` in `line` and parse VALUE as f32.
///
/// Returns `None` if the key is absent, `Err` if the value is non-numeric.
fn extract_avg(line: &str, key: &'static str) -> Result<Option<f32>, PressureReadError> {
    let needle = {
        let mut found_val: Option<&str> = None;
        for token in line.split_whitespace() {
            if let Some(val) = token.strip_prefix(key).and_then(|s| s.strip_prefix('=')) {
                found_val = Some(val);
                break;
            }
        }
        found_val
    };
    match needle {
        None => Ok(None),
        Some(val_str) => {
            let v = val_str
                .parse::<f32>()
                .map_err(|_| PressureReadError::ParseError {
                    file: PSI_PATH,
                    field: key,
                    value: val_str.to_owned(),
                })?;
            Ok(Some(v))
        }
    }
}

/// Require a named avg field from `line`; error with `field_name` if absent.
fn require_avg(
    line: &str,
    key: &'static str,
    field_name: &'static str,
) -> Result<f32, PressureReadError> {
    extract_avg(line, key)?.ok_or(PressureReadError::MissingField {
        field: field_name,
        path: PSI_PATH,
    })
}

/// Find the first line in `content` that starts with `prefix`.
fn find_line<'a>(content: &'a str, prefix: &str) -> Option<&'a str> {
    content
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with(prefix))
}

// ── Public parser ────────────────────────────────────────────────────────────

/// Parse PSI content from a string (testable without `/proc`).
///
/// Expects two lines: one starting with `some ` and one with `full `,
/// each containing `avg10=X avg60=X avg300=X`.
pub fn parse_psi(content: &str) -> Result<PsiReading, PressureReadError> {
    let some_line = find_line(content, "some ").ok_or(PressureReadError::MissingField {
        field: "some line",
        path: PSI_PATH,
    })?;
    let full_line = find_line(content, "full ").ok_or(PressureReadError::MissingField {
        field: "full line",
        path: PSI_PATH,
    })?;
    Ok(PsiReading {
        some_avg10: require_avg(some_line, "avg10", "some avg10")?,
        some_avg60: require_avg(some_line, "avg60", "some avg60")?,
        some_avg300: require_avg(some_line, "avg300", "some avg300")?,
        full_avg10: require_avg(full_line, "avg10", "full avg10")?,
        full_avg60: require_avg(full_line, "avg60", "full avg60")?,
        full_avg300: require_avg(full_line, "avg300", "full avg300")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const VALID_PSI: &str = "\
some avg10=0.00 avg60=0.00 avg300=0.00 total=89\n\
full avg10=0.00 avg60=0.00 avg300=0.00 total=57\n";
    const VALID_PSI_NONZERO: &str = "\
some avg10=12.34 avg60=5.67 avg300=1.23 total=99\n\
full avg10=8.90 avg60=3.45 avg300=0.12 total=77\n";
    #[test]
    fn parse_valid_zero_pressure() {
        let r = parse_psi(VALID_PSI).expect("should parse valid PSI");
        assert!((r.some_avg10 - 0.0).abs() < f32::EPSILON);
        assert!((r.some_avg60 - 0.0).abs() < f32::EPSILON);
        assert!((r.some_avg300 - 0.0).abs() < f32::EPSILON);
        assert!((r.full_avg10 - 0.0).abs() < f32::EPSILON);
        assert!((r.full_avg60 - 0.0).abs() < f32::EPSILON);
        assert!((r.full_avg300 - 0.0).abs() < f32::EPSILON);
    }
    #[test]
    fn parse_nonzero_pressure() {
        let r = parse_psi(VALID_PSI_NONZERO).expect("should parse non-zero PSI");
        assert!((r.some_avg10 - 12.34).abs() < 0.001);
        assert!((r.some_avg60 - 5.67).abs() < 0.001);
        assert!((r.full_avg10 - 8.90).abs() < 0.001);
        assert!((r.full_avg300 - 0.12).abs() < 0.001);
    }
    #[test]
    fn parse_malformed_float_returns_error() {
        let bad = "some avg10=abc avg60=0.00 avg300=0.00 total=0\n\
                   full avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        let err = parse_psi(bad);
        assert!(
            matches!(err, Err(PressureReadError::ParseError { .. })),
            "expected ParseError, got: {err:?}"
        );
    }
    #[test]
    fn parse_missing_full_line_returns_error() {
        let incomplete = "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        let err = parse_psi(incomplete);
        assert!(
            matches!(err, Err(PressureReadError::MissingField { field, .. }) if field
            .contains("full")),
            "expected MissingField for full line, got: {err:?}"
        );
    }
    #[test]
    fn parse_missing_avg60_returns_error() {
        let bad = "some avg10=0.00 avg300=0.00 total=0\n\
                   full avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        let err = parse_psi(bad);
        assert!(
            matches!(err, Err(PressureReadError::MissingField { field, .. }) if field
            .contains("avg60")),
            "expected MissingField for avg60, got: {err:?}"
        );
    }
    #[test]
    fn parse_extra_whitespace_and_blank_lines() {
        let content = "\n\nsome avg10=1.00 avg60=2.00 avg300=3.00 total=5\n\n\
                       full avg10=0.10 avg60=0.20 avg300=0.30 total=1\n\n";
        let r = parse_psi(content).expect("should tolerate extra blank lines");
        assert!((r.some_avg10 - 1.0).abs() < 0.001);
        assert!((r.full_avg300 - 0.30).abs() < 0.001);
    }
    /// Live smoke test — only on Linux where `/proc` exists.
    #[test]
    #[cfg(target_os = "linux")]
    fn live_read_psi_does_not_panic() {
        match read_psi() {
            Ok(r) => {
                assert!(r.some_avg10 >= 0.0);
                assert!(r.full_avg10 >= 0.0);
            }
            Err(PressureReadError::PsiUnavailable) => {}
            Err(e) => {
                eprintln!("live PSI read error (non-fatal in CI): {e}");
            }
        }
    }
}
