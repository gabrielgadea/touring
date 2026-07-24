//! A.3 Idempotency Gate — `format(format(x)) == format(x)` validator.
//!
//! Detects when a proposed Rust edit would produce different output when
//! formatted twice — a signal that the formatter is not idempotent on that
//! input, which indicates low-quality or unstable generated code.
//!
//! Wired into `pre_edit` after the proposed `new_string` is extracted.
//! The check runs only on Rust (`.rs`) files when idempotency is enabled
//! in the feature flags config.
//!
//! # Diagnostic
//!
//! - `Q-220 NonIdempotentFormat` — emitted when double-format diverges
//! - Counter: `idempotency_violations_count` in gate-metrics

use std::path::Path;
use tracing::warn;

/// Q-220 diagnostic code — non-idempotent format output.
pub const Q_220_NONIDEMPOTENT: &str = "Q-220";

/// Feature-gated idempotency configuration.
///
/// Controls whether the idempotency check runs and which languages are checked.
#[derive(Debug, Clone)]
pub struct IdempotencyConfig {
    /// When `false`, the idempotency check is a no-op (bypassed).
    pub enabled: bool,
    /// File extensions to check. Currently only `["rs"]` is supported.
    /// Expandable to Python/TypeScript when tree-sitter formatters are wired.
    pub langs: Vec<&'static str>,
}

impl Default for IdempotencyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            langs: vec!["rs"],
        }
    }
}

impl IdempotencyConfig {
    /// Returns `true` if the given file extension should be checked.
    #[inline]
    pub fn should_check(&self, ext: &str) -> bool {
        self.enabled && self.langs.contains(&ext)
    }
}

/// Check whether formatting `content` twice produces the same output.
///
/// Returns `Ok(())` when idempotent (format is stable), `Err(Q220Payload)`
/// when the two formatted outputs diverge.
///
/// The check is a no-op for non-Rust files or when idempotency is disabled.
pub fn check_idempotency(
    file_path: &Path,
    content: &str,
    config: &IdempotencyConfig,
) -> Result<(), Q220Payload> {
    if content.is_empty() {
        return Ok(());
    }

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if !config.should_check(ext) {
        return Ok(());
    }

    // First format pass.
    let (first, was_formatted) = touring_code::ast::surgery::format_rust_code_best_effort(content);
    if !was_formatted {
        // Could not parse as valid Rust — skip the idempotency check.
        return Ok(());
    }

    // Second format pass on the already-formatted output.
    let (second, second_formatted) =
        touring_code::ast::surgery::format_rust_code_best_effort(&first);
    if !second_formatted {
        // Should not happen if first pass produced valid Rust.
        return Ok(());
    }

    if first != second {
        let payload = Q220Payload {
            file_path: file_path.to_string_lossy().to_string(),
            first_len: first.len(),
            second_len: second.len(),
            diff_bytes: byte_diff(&first, &second),
        };
        warn!(
            code = Q_220_NONIDEMPOTENT,
            file_path = %payload.file_path,
            first_len = payload.first_len,
            second_len = payload.second_len,
            "Q-220 NonIdempotentFormat: format(format(x)) != format(x) — {} bytes differ",
            payload.diff_bytes,
        );
        return Err(payload);
    }

    Ok(())
}

/// Returns the number of bytes that differ between `a` and `b`.
fn byte_diff(a: &str, b: &str) -> usize {
    a.bytes()
        .zip(b.bytes())
        .filter(|(x, y)| x != y)
        .count()
        .max(if a.len() > b.len() {
            a.len() - b.len()
        } else {
            b.len() - a.len()
        })
}

/// Payload emitted when an idempotency violation is detected.
#[derive(Debug, Clone)]
pub struct Q220Payload {
    /// Path of the file whose double-format diverged.
    pub file_path: String,
    /// Byte length of the output after the first format pass.
    pub first_len: usize,
    /// Byte length of the output after the second format pass.
    pub second_len: usize,
    /// Number of bytes that differ between the two formatted outputs.
    pub diff_bytes: usize,
}

impl std::fmt::Display for Q220Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Q-220: file '{}' format idempotency violation — first format {} bytes, second format {} bytes, {} bytes differ",
            self.file_path, self.first_len, self.second_len, self.diff_bytes
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_content_is_idempotent() {
        let cfg = IdempotencyConfig::default();
        let r = check_idempotency(Path::new("foo.rs"), "", &cfg);
        assert!(r.is_ok());
    }

    #[test]
    fn disabled_config_is_noop() {
        let cfg = IdempotencyConfig {
            enabled: false,
            langs: vec!["rs"],
        };
        let r = check_idempotency(Path::new("foo.rs"), "fn main() {}", &cfg);
        assert!(r.is_ok());
    }

    #[test]
    fn non_rust_file_is_skipped() {
        let cfg = IdempotencyConfig::default();
        // format_rust_code_best_effort on Python would return unchanged
        let r = check_idempotency(Path::new("foo.py"), "def foo(): pass", &cfg);
        assert!(r.is_ok());
    }

    #[test]
    fn well_formed_rust_is_idempotent() {
        let cfg = IdempotencyConfig::default();
        // This is already formatted — double format should be identical.
        let src = "fn main() {\n    println!(\"hello\");\n}\n";
        let r = check_idempotency(Path::new("foo.rs"), src, &cfg);
        assert!(
            r.is_ok(),
            "well-formed Rust should be idempotent; got Err: {:?}",
            r
        );
    }

    #[test]
    fn unformatted_rust_becomes_idempotent_after_first_format() {
        let cfg = IdempotencyConfig::default();
        // Unformatted Rust: extra spaces, but valid — first format cleans it,
        // second format is stable.
        let src = "fn main() {   println!(\"hello\");   }\n";
        let r = check_idempotency(Path::new("foo.rs"), src, &cfg);
        assert!(
            r.is_ok(),
            "unformatted but valid Rust should stabilize after first format; got Err: {:?}",
            r
        );
    }

    #[test]
    fn q220_payload_contains_useful_fields() {
        let payload = Q220Payload {
            file_path: "src/main.rs".to_string(),
            first_len: 100,
            second_len: 105,
            diff_bytes: 5,
        };
        let s = payload.to_string();
        assert!(s.contains("Q-220"));
        assert!(s.contains("src/main.rs"));
        assert!(s.contains("100"));
        assert!(s.contains("5"));
    }
}
