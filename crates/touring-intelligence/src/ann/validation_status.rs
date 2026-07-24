//! Shared validation status type used by cross_validator and rlm_integration.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Validation status of an assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    /// Confirmed by multiple sources
    Confirmed,
    /// Supported by one source
    Supported,
    /// No additional evidence found
    Unverified,
    /// Contradiction detected
    Contradicted,
    /// Invalid normative reference
    InvalidReference,
}

impl fmt::Display for ValidationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationStatus::Confirmed => write!(f, "CONFIRMED"),
            ValidationStatus::Supported => write!(f, "SUPPORTED"),
            ValidationStatus::Unverified => write!(f, "UNVERIFIED"),
            ValidationStatus::Contradicted => write!(f, "CONTRADICTED"),
            ValidationStatus::InvalidReference => write!(f, "INVALID_REFERENCE"),
        }
    }
}
