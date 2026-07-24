//! TantivyFailover — Failover implementation for tantivy search index.
//!
//! Provides failover capability for tantivy-backed search indices with
//! shadow index pattern for zero-downtime primary elections.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{Failover, Health, Result};
use crate::failover::FailoverState;

/// Persistence provider trait for tantivy index operations.
pub trait PersistenceProvider: Send + Sync {
    /// Returns true if the primary index is accessible and readable.
    fn is_primary_accessible(&self) -> bool;

    /// Open a read-only shadow of the primary index.
    fn open_shadow(&self) -> Result<()>;

    /// Returns true if shadow index is currently open.
    fn is_shadow_open(&self) -> bool;

    /// Copy incremental document updates from primary to shadow.
    fn copy_incremental_to_shadow(&self) -> Result<u64>;

    /// Promote shadow index to primary (atomic swap).
    fn promote_shadow_to_primary(&self) -> Result<()>;
}

/// Backup data structure for tantivy failover.
#[derive(Debug, Clone, Default)]
pub struct TantivyBackup {
    /// Path to the shadow index directory replicated from the
    /// primary. The backup serves reads from this directory
    /// while the primary is unavailable.
    pub shadow_path: String,
    /// Highest commit seq number observed at the time of the
    /// last successful sync. Useful for lag estimation.
    pub last_sync_seq: u64,
}

/// TantivyFailover implements Failover for tantivy-backed search.
#[derive(Debug)]
pub struct TantivyFailover<P: PersistenceProvider> {
    provider: P,
    backup: Arc<RwLock<TantivyBackup>>,
    #[allow(dead_code)]
    state: FailoverState,
}

impl<P: PersistenceProvider> TantivyFailover<P> {
    /// Create a new TantivyFailover with the given persistence provider.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            backup: Arc::new(RwLock::new(TantivyBackup::default())),
            state: FailoverState::default(),
        }
    }
}

#[async_trait]
impl<P: PersistenceProvider + 'static> Failover<P, TantivyBackup> for TantivyFailover<P> {
    /// Check primary tantivy index health by testing accessibility.
    async fn primary_health(&self) -> Health {
        if self.provider.is_primary_accessible() {
            Health::Healthy
        } else {
            Health::Unhealthy
        }
    }

    /// Activate the backup (shadow) index by opening it read-only.
    async fn activate_backup(&mut self) -> Result<()> {
        if self.provider.is_shadow_open() {
            return Ok(());
        }
        self.provider.open_shadow()?;
        let backup = self.backup.read().await;
        tracing::info!(
            shadow_path = %backup.shadow_path,
            last_sync_seq = backup.last_sync_seq,
            "tantivy shadow index activated"
        );
        Ok(())
    }

    /// Sync incremental document updates from primary to shadow.
    async fn sync_backup(&mut self) -> Result<()> {
        let docs_synced = self.provider.copy_incremental_to_shadow()?;
        let mut backup = self.backup.write().await;
        backup.last_sync_seq += docs_synced;
        tracing::debug!(
            docs_synced = docs_synced,
            "tantivy incremental sync complete"
        );
        Ok(())
    }

    /// Restore primary service by promoting shadow to primary.
    async fn restore_to_primary(&mut self) -> Result<()> {
        self.provider.promote_shadow_to_primary()?;
        let mut backup = self.backup.write().await;
        backup.last_sync_seq = 0;
        tracing::info!("tantivy primary restored from shadow");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockProvider {
        primary_accessible: AtomicBool,
        shadow_open: AtomicBool,
    }

    impl MockProvider {
        fn new(primary_accessible: bool) -> Self {
            Self {
                primary_accessible: AtomicBool::new(primary_accessible),
                shadow_open: AtomicBool::new(false),
            }
        }
    }

    impl PersistenceProvider for MockProvider {
        fn is_primary_accessible(&self) -> bool {
            self.primary_accessible.load(Ordering::SeqCst)
        }

        fn open_shadow(&self) -> Result<()> {
            self.shadow_open.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn is_shadow_open(&self) -> bool {
            self.shadow_open.load(Ordering::SeqCst)
        }

        fn copy_incremental_to_shadow(&self) -> Result<u64> {
            Ok(42)
        }

        fn promote_shadow_to_primary(&self) -> Result<()> {
            self.shadow_open.store(false, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn tantivy_health_healthy() {
        let provider = MockProvider::new(true);
        let failover = TantivyFailover::new(provider);
        let health = failover.primary_health().await;
        assert_eq!(health, Health::Healthy);
    }

    #[tokio::test]
    async fn tantivy_health_unhealthy() {
        let provider = MockProvider::new(false);
        let failover = TantivyFailover::new(provider);
        let health = failover.primary_health().await;
        assert_eq!(health, Health::Unhealthy);
    }

    #[tokio::test]
    async fn activate_backup_opens_shadow() {
        let provider = MockProvider::new(false);
        let mut failover = TantivyFailover::new(provider);
        failover.activate_backup().await.unwrap();
        assert!(failover.provider.is_shadow_open());
    }
}
