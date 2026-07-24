//! Errors raised by the metric rules engine.

use std::io;
use std::path::PathBuf;

/// Errors raised when loading or evaluating metric rules.
#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    /// Reading the TOML file failed.
    #[error("failed reading rules file {path}: {source}")]
    Read {
        /// Path that failed to open.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// TOML deserialisation failed.
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
    /// `applies_to` glob is malformed.
    #[error("invalid applies_to glob `{glob}` in rule `{rule}`: {source}")]
    Glob {
        /// Rule that owned the bad glob.
        rule: String,
        /// The bad glob string.
        glob: String,
        /// Underlying glob error.
        #[source]
        source: glob::PatternError,
    },
    /// Rule has internal inconsistencies (duplicate name, schema version, …).
    #[error("rule `{rule}` is invalid: {reason}")]
    Invalid {
        /// Rule that failed validation.
        rule: String,
        /// Why it failed.
        reason: String,
    },
    /// Schema `version` field is unsupported.
    #[error("unsupported rules schema version `{version}` (supported: 1.0)")]
    UnsupportedVersion {
        /// The version we found.
        version: String,
    },
}

/// Result alias used throughout the rules engine.
pub type Result<T> = std::result::Result<T, RulesError>;
