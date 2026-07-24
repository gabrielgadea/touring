//! Failover module — cross-subsystem resilience coordinator.
//!
//! Generalizes the circuit breaker pattern into a unified Failover trait with
//! primary/backup coordination. Currently supports tantivy, daemon, and
//! vector-store subsystems.
//!
//! # Architecture
//!
//! - [`trait Failover`] — async trait for primary/backup health + transitions
//! - [`FailoverCoordinator`] — manages multiple failover services with periodic health checks
//! - [`Health`] — health status enum (Healthy, Degraded, Unhealthy)
//! - [`FailoverState`] — shared state across all failover services
//!
//! # Example
//!
//! ```
//! use touring_resilience::failover::{Failover, FailoverCoordinator, Health, FailoverError};
//! use async_trait::async_trait;
//!
//! #[derive(Default)]
//! struct TantivyFailover { primary_ok: bool }
//!
//! #[async_trait]
//! impl Failover<(), ()> for TantivyFailover {
//!     async fn primary_health(&self) -> Health {
//!         if self.primary_ok { Health::Healthy } else { Health::Unhealthy }
//!     }
//!     async fn activate_backup(&mut self) -> Result<(), FailoverError> { Ok(()) }
//!     async fn sync_backup(&mut self) -> Result<(), FailoverError> { Ok(()) }
//!     async fn restore_to_primary(&mut self) -> Result<(), FailoverError> { Ok(()) }
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod coordinator;
pub mod health;
pub mod impl_daemon;
pub mod impl_tantivy;
pub mod impl_vector_store;

pub use coordinator::FailoverCoordinator;
pub use health::Health;

/// Errors that can occur during failover operations.
#[derive(Debug, thiserror::Error)]
pub enum FailoverError {
    /// The primary service's health check returned an error.
    /// String is the underlying transport or health-probe message.
    #[error("primary health check failed: {0}")]
    PrimaryHealthCheck(String),
    /// Activating the backup service failed (e.g. open read-only
    /// indices, start fallback daemon). String is the underlying
    /// cause.
    #[error("backup activation failed: {0}")]
    BackupActivation(String),
    /// Replicating state from primary to backup failed mid-sync.
    /// String is the underlying cause.
    #[error("backup sync failed: {0}")]
    BackupSync(String),
    /// Restoring the primary and deactivating the backup failed.
    /// String is the underlying cause.
    #[error("restore failed: {0}")]
    RestoreFailed(String),
    /// No backup is configured for this service. Returned when
    /// callers attempt `activate_backup` without a registered
    /// standby.
    #[error("backup not available")]
    BackupNotAvailable,
}

/// Result type alias for [`FailoverError`] — used by every method on
/// the [`Failover`] trait and by [`FailoverCoordinator`].
pub type Result<T> = std::result::Result<T, FailoverError>;

/// Async trait for failover-capable services.
/// Implementors provide primary health checks and backup activation logic.
///
/// # Dyn Safety
/// This trait is dyn-compatible (object-safe) because all methods use
/// `#[async_trait]` which transforms async fns into `Pin<Box<dyn Future>>`.
#[async_trait]
pub trait Failover<P: Send + Sync, B: Send + Sync>: Send + Sync {
    /// Check primary service health.
    async fn primary_health(&self) -> Health;
    /// Activate the backup service (e.g., open read-only indices).
    async fn activate_backup(&mut self) -> Result<()>;
    /// Sync state from primary to backup (optional — some services skip this).
    async fn sync_backup(&mut self) -> Result<()> {
        let _ = &self;
        Ok(())
    }
    /// Restore primary service and deactivate backup.
    async fn restore_to_primary(&mut self) -> Result<()>;
}

/// Shared failover state for metrics and coordination.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FailoverState {
    /// Number of services currently on backup.
    pub active_count: u32,
    /// Total transition count (primary→backup or backup→primary).
    pub transition_count: u32,
    /// Number of successful recoveries (backup→primary).
    pub recovery_count: u32,
    /// Per-service health snapshots.
    pub services: Vec<ServiceState>,
}

/// Per-service snapshot used by [`FailoverState::services`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    /// Logical service name (e.g. `"tantivy"`, `"daemon"`,
    /// `"vector_store"`).
    pub name: String,
    /// Latest known health of the primary.
    pub current_health: Health,
    /// Whether the service is currently operating on its backup.
    pub is_on_backup: bool,
    /// Number of consecutive `Healthy` polls. Used to gate
    /// automatic restore-to-primary on a hysteresis threshold.
    pub consecutive_healthy: u32,
}

/// Metrics snapshot for `touring gate-metrics` integration.
///
/// Cheap to clone and serialise; produced by
/// `FailoverState::metrics`
/// and consumed by the metrics collector.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FailoverMetrics {
    /// Number of services currently on backup. Counter
    /// `failover_active_count` in `gate-metrics`.
    pub failover_active_count: u32,
    /// Total primary↔backup transitions observed. Counter
    /// `failover_transition_count`.
    pub failover_transition_count: u32,
    /// Successful backup→primary recoveries. Counter
    /// `failover_recovery_count`.
    pub failover_recovery_count: u32,
}

impl From<&FailoverState> for FailoverMetrics {
    fn from(state: &FailoverState) -> Self {
        Self {
            failover_active_count: state.active_count,
            failover_transition_count: state.transition_count,
            failover_recovery_count: state.recovery_count,
        }
    }
}
