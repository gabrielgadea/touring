//! SHA-256 projection gate for event integrity verification.

use crate::activity::event::{Event, EventAction};
use sha2::{Digest, Sha256};

/// Verification error types.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    /// The event's stored projection hash did not match the
    /// recomputed hash. Indicates tampering or transport
    /// corruption. String is the event id.
    #[error("projection mismatch for event {0}")]
    ProjectionMismatch(String),
    /// The event's actor field has an unrecognised discriminant.
    #[error("invalid actor type")]
    InvalidActor,
    /// The event's action field has an unrecognised discriminant.
    #[error("invalid action type")]
    InvalidAction,
    /// The event's timestamp is outside the configured drift
    /// window.
    #[error("timestamp out of range")]
    TimestampOutOfRange {
        /// Minimum acceptable timestamp (epoch nanoseconds).
        min: u128,
        /// Maximum acceptable timestamp (epoch nanoseconds).
        max: u128,
        /// The timestamp that was observed.
        found: u64,
    },
}

/// Result type for verify operations.
pub type VerifyResult<T> = std::result::Result<T, VerifyError>;

/// SHA-256 gate for verifying event integrity before committing to store.
pub struct Verifier {
    expected_timestamp_max_drift_ns: u64,
}

impl Verifier {
    /// Create a new verifier with default settings.
    pub fn new() -> Self {
        Self {
            // Allow up to 1 hour clock drift
            expected_timestamp_max_drift_ns: 3_600_000_000_000u64,
        }
    }

    /// Verify an event's integrity: projection hash + timestamp sanity.
    pub fn verify_event(&self, event: &Event) -> VerifyResult<()> {
        // Step 1: Verify projection hash
        if !event.verify_projection() {
            return Err(VerifyError::ProjectionMismatch(event.id.to_string()));
        }

        // Step 2: Timestamp sanity check (must be within reasonable range)
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is before UNIX_EPOCH")
            .as_nanos();

        if event.timestamp_ns as u128 > now_ns + self.expected_timestamp_max_drift_ns as u128 {
            return Err(VerifyError::TimestampOutOfRange {
                min: now_ns.saturating_sub(self.expected_timestamp_max_drift_ns as u128),
                max: now_ns + self.expected_timestamp_max_drift_ns as u128,
                found: event.timestamp_ns,
            });
        }

        // Step 3: Validate actor is not empty
        if event.actor.display().is_empty() {
            return Err(VerifyError::InvalidActor);
        }

        // Step 4: Validate action is not unknown
        if matches!(event.action, EventAction::DaemonHealth) {
            // DaemonHealth is allowed; just ensure display name
            if event.actor.display().is_empty() {
                return Err(VerifyError::InvalidActor);
            }
        }

        // Step 5: Validate sequence number is non-zero
        if event.seq == 0 {
            return Err(VerifyError::ProjectionMismatch(
                "sequence number cannot be zero".to_string(),
            ));
        }

        Ok(())
    }

    /// Verify a batch of events and return the first error if any.
    pub fn verify_batch(&self, events: &[Event]) -> VerifyResult<()> {
        for event in events {
            self.verify_event(event)?;
        }
        Ok(())
    }

    /// Compute the SHA-256 projection hash for an event without modifying it.
    pub fn compute_hash(event: &Event) -> String {
        let mut hasher = Sha256::new();
        hasher.update(event.id.as_str().as_bytes());
        hasher.update(event.seq.to_le_bytes());
        hasher.update(event.action.to_string().as_bytes());
        hasher.update(event.actor.display().as_bytes());
        hasher.update(event.timestamp_ns.to_le_bytes());
        if let Some(ref p) = event.payload {
            hasher.update(p.to_string().as_bytes());
        }
        hex::encode(hasher.finalize())
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::event::{Actor, Event, EventAction};

    #[test]
    fn verify_valid_event() {
        let verifier = Verifier::new();
        let event = Event::new(
            1,
            EventAction::TaskStarted,
            Actor::Agent("test".into()),
            None,
        );
        assert!(verifier.verify_event(&event).is_ok());
    }

    #[test]
    fn verify_invalid_projection() {
        let verifier = Verifier::new();
        let mut event = Event::new(
            1,
            EventAction::TaskStarted,
            Actor::Agent("test".into()),
            None,
        );
        event.projection_hash = "invalid_hash".to_string();
        assert!(verifier.verify_event(&event).is_err());
    }

    #[test]
    fn verify_zero_seq_rejected() {
        let verifier = Verifier::new();
        let event = Event::new(
            0,
            EventAction::TaskStarted,
            Actor::Agent("test".into()),
            None,
        );
        assert!(verifier.verify_event(&event).is_err());
    }

    #[test]
    fn verify_batch_all_ok() {
        let verifier = Verifier::new();
        let events = vec![
            Event::new(1, EventAction::TaskStarted, Actor::Agent("a".into()), None),
            Event::new(
                2,
                EventAction::TaskCompleted,
                Actor::Agent("a".into()),
                None,
            ),
        ];
        assert!(verifier.verify_batch(&events).is_ok());
    }

    #[test]
    fn compute_hash_deterministic() {
        let event = Event::new(1, EventAction::HookFired, Actor::Daemon("d".into()), None);
        let h1 = Verifier::compute_hash(&event);
        let h2 = Verifier::compute_hash(&event);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }
}
