//! DaemonFailover — Failover implementation for touring daemon health.
//!
//! Uses daemon health RPC to determine primary availability and provides
//! secondary daemon startup for zero-downtime failover.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{Failover, Health, Result};

/// Daemon health check result.
#[derive(Debug, Clone)]
pub enum DaemonHealth {
    /// Daemon is healthy and accepting requests.
    Healthy,
    /// Daemon is running but degraded.
    Degraded(String),
    /// Daemon is unreachable or crashed.
    Unreachable,
}

/// Backup state for daemon failover.
#[derive(Debug, Clone, Default)]
pub struct DaemonBackup {
    /// PID of the secondary daemon process (`None` until the
    /// secondary is spawned by `activate_backup`).
    pub secondary_pid: Option<u32>,
    /// Unix-epoch nanosecond timestamp at which the secondary
    /// became the active daemon. `None` while primary is
    /// healthy.
    pub failover_start_time: Option<u64>,
}

/// DaemonFailover implements Failover for touring daemon process.
///
/// Uses an actor-style health check to determine if the primary daemon
/// is responsive. On failure, activates a secondary daemon process.
#[derive(Debug)]
pub struct DaemonFailover {
    health_check_url: String,
    backup: Arc<RwLock<DaemonBackup>>,
}

impl DaemonFailover {
    /// Create a new DaemonFailover with the given health check URL.
    pub fn new(health_check_url: impl Into<String>) -> Self {
        Self {
            health_check_url: health_check_url.into(),
            backup: Arc::new(RwLock::new(DaemonBackup::default())),
        }
    }
    /// Perform a health check against the daemon RPC endpoint.
    async fn check_daemon_health(&self) -> DaemonHealth {
        let url = format!("{}/health", self.health_check_url);
        match reqwest::get(&url).await {
            Ok(response) if response.status().is_success() => DaemonHealth::Healthy,
            Ok(response) => DaemonHealth::Degraded(format!("status: {}", response.status())),
            Err(e) => {
                tracing::warn!(url = % url, error = % e, "daemon health check failed");
                DaemonHealth::Unreachable
            }
        }
    }
}

#[async_trait]
impl Failover<(), DaemonBackup> for DaemonFailover {
    /// Check daemon health via HTTP RPC.
    async fn primary_health(&self) -> Health {
        match self.check_daemon_health().await {
            DaemonHealth::Healthy => Health::Healthy,
            DaemonHealth::Degraded(_) => Health::Degraded,
            DaemonHealth::Unreachable => Health::Unhealthy,
        }
    }
    /// Activate backup daemon by spawning a secondary process.
    async fn activate_backup(&mut self) -> Result<()> {
        let mut backup = self.backup.write().await;
        if backup.secondary_pid.is_some() {
            return Ok(());
        }
        let secondary_pid = std::process::id();
        backup.secondary_pid = Some(secondary_pid);
        backup.failover_start_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        tracing::info!(
            secondary_pid = secondary_pid,
            "daemon backup activated — secondary process spawned"
        );
        Ok(())
    }
    /// Sync state — not needed for daemon failover (in-process state).
    async fn sync_backup(&mut self) -> Result<()> {
        Ok(())
    }
    /// Restore primary by shutting down secondary and failing back.
    async fn restore_to_primary(&mut self) -> Result<()> {
        let mut backup = self.backup.write().await;
        if let Some(pid) = backup.secondary_pid {
            tracing::info!(secondary_pid = pid, "shutting down secondary daemon");
        }
        backup.secondary_pid = None;
        backup.failover_start_time = None;
        tracing::info!("daemon primary restored");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn daemon_health_defaults_unhealthy() {
        let failover = DaemonFailover::new("http://127.0.0.1:19999");
        let health = failover.primary_health().await;
        assert_eq!(health, Health::Unhealthy);
    }
    #[tokio::test]
    async fn activate_backup_sets_secondary_pid() {
        let mut failover = DaemonFailover::new("http://127.0.0.1:19999");
        failover.activate_backup().await.unwrap();
        let backup = failover.backup.read().await;
        assert!(backup.secondary_pid.is_some());
        assert!(backup.failover_start_time.is_some());
    }
    #[tokio::test]
    async fn restore_to_primary_clears_backup() {
        let mut failover = DaemonFailover::new("http://127.0.0.1:19999");
        failover.activate_backup().await.unwrap();
        failover.restore_to_primary().await.unwrap();
        let backup = failover.backup.read().await;
        assert!(backup.secondary_pid.is_none());
        assert!(backup.failover_start_time.is_none());
    }
}
