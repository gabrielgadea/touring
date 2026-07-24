//! Checkpoint module — family-aware config fingerprinting.
//!
//! - [`fingerprint`] — CheckpointSettingsFingerprint for config compatibility detection

pub mod fingerprint;

pub use fingerprint::{ChangeImpact, CheckpointSettingsFingerprint, ConfigType};
