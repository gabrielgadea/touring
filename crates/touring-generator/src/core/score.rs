//! `NormalizedScore` newtype — guarantees values in [0.0, 1.0] at construction time.
//! Prevents score=2.5 passing a >=0.8 gate (Pln1 bug fixed in Pln2).

use crate::error::GenerateError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// A floating-point score guaranteed to be in the range [0.0, 1.0].
///
/// Use `NormalizedScore::new` for fallible construction or
/// `NormalizedScore::clamped` for saturating construction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
pub struct NormalizedScore(f64);

impl NormalizedScore {
    /// Score of 0.0 (minimum).
    pub const ZERO: Self = Self(0.0);
    /// Score of 1.0 (maximum / perfect).
    pub const ONE: Self = Self(1.0);
    /// Default threshold for auto-commit gate (0.8).
    pub const AUTO_COMMIT_THRESHOLD: Self = Self(0.8);

    /// Construct a score, returning an error if `value` is outside [0.0, 1.0].
    ///
    /// # Errors
    /// Returns [`GenerateError::ScoreOutOfRange`] if `value` is not finite or outside [0.0, 1.0].
    pub fn new(value: f64) -> Result<Self, GenerateError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(GenerateError::ScoreOutOfRange { value });
        }
        Ok(Self(value))
    }

    /// Construct a score by clamping to [0.0, 1.0] without error.
    ///
    /// Use for RL signals where over/underflow is expected and saturation
    /// is the correct semantic.
    #[inline]
    #[must_use]
    pub fn clamped(value: f64) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Return the inner f64 value.
    #[inline]
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }

    /// Returns true if this score meets or exceeds `threshold`.
    #[inline]
    #[must_use]
    pub fn passes(self, threshold: Self) -> bool {
        self.0 >= threshold.0
    }

    /// Returns true if this score is exactly 1.0.
    #[inline]
    #[must_use]
    pub fn is_perfect(self) -> bool {
        (self.0 - 1.0).abs() < f64::EPSILON
    }

    /// Returns true if this score is exactly 0.0.
    #[inline]
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.0 < f64::EPSILON
    }
}

impl fmt::Display for NormalizedScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.0)
    }
}

impl PartialEq for NormalizedScore {
    fn eq(&self, other: &Self) -> bool {
        (self.0 - other.0).abs() < f64::EPSILON
    }
}

impl Eq for NormalizedScore {}

impl Ord for NormalizedScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for NormalizedScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TryFrom<f64> for NormalizedScore {
    type Error = GenerateError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<NormalizedScore> for f64 {
    fn from(s: NormalizedScore) -> f64 {
        s.0
    }
}

impl Default for NormalizedScore {
    fn default() -> Self {
        Self::ZERO
    }
}
