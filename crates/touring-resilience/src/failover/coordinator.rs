//! FailoverCoordinator — manages multiple Failover services with periodic health checks.

use super::{Failover, FailoverState, Health};
use std::sync::{Arc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};

/// Minimum consecutive healthy checks before restoring primary.
const RECOVERY_THRESHOLD: u32 = 3;

/// Health check interval.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Wrapper to make Failover cloneable for sharing across async tasks.
struct SharedFailover {
    inner: Arc<dyn Failover<(), ()> + Send + Sync>,
    name: String,
}

impl SharedFailover {
    fn new(name: String, inner: Box<dyn Failover<(), ()> + Send + Sync>) -> Self {
        Self {
            inner: Arc::from(inner),
            name,
        }
    }
    fn name(&self) -> &str {
        &self.name
    }
    async fn health(&self) -> Health {
        self.inner.primary_health().await
    }
}

impl Clone for SharedFailover {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            name: self.name.clone(),
        }
    }
}

/// Coordinates multiple failover services with periodic health monitoring.
pub struct FailoverCoordinator {
    services: Arc<Mutex<Vec<SharedFailover>>>,
    state: Arc<RwLock<FailoverState>>,
    monitor_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl Drop for FailoverCoordinator {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.monitor_handle.write()
            && let Some(handle) = guard.take()
        {
            handle.abort();
        }
    }
}

impl FailoverCoordinator {
    /// Create a new coordinator with no services.
    pub fn new() -> Self {
        Self {
            services: Arc::new(Mutex::new(Vec::new())),
            state: Arc::new(RwLock::new(FailoverState::default())),
            monitor_handle: Arc::new(RwLock::new(None)),
        }
    }
    /// Register a service with the coordinator.
    pub fn register<S: Failover<(), ()> + Send + Sync + 'static>(
        &mut self,
        name: &str,
        service: S,
    ) {
        if let Ok(mut services) = self.services.lock() {
            services.push(SharedFailover::new(name.to_string(), Box::new(service)));
        }
    }
    /// Start periodic health monitoring (does not block).
    pub fn start_monitoring(&mut self) {
        if self
            .monitor_handle
            .read()
            .ok()
            .map(|g| g.is_some())
            .unwrap_or(false)
        {
            return;
        }
        let services = Arc::clone(&self.services);
        let state = Arc::clone(&self.state);
        let handle = tokio::spawn(async move {
            let mut ticker = interval(HEALTH_CHECK_INTERVAL);
            let mut consecutive_healthy: Vec<u32> = Vec::new();
            loop {
                ticker.tick().await;
                let current_services: Vec<SharedFailover> = {
                    let guard = services.lock().unwrap_or_else(|e| e.into_inner());
                    guard.clone()
                };
                let count = current_services.len();
                if count == 0 {
                    continue;
                }
                {
                    let mut fail_state = state.write().unwrap_or_else(|e| e.into_inner());
                    if fail_state.services.len() != count {
                        fail_state
                            .services
                            .resize_with(count, || super::ServiceState {
                                name: String::new(),
                                current_health: Health::Healthy,
                                is_on_backup: false,
                                consecutive_healthy: 0,
                            });
                    }
                }
                if consecutive_healthy.len() < count {
                    consecutive_healthy.resize(count, 0);
                }
                for (i, service) in current_services.iter().enumerate() {
                    let health = service.health().await;
                    let mut fail_state = state.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(svc) = fail_state.services.get_mut(i) {
                        svc.name = service.name().to_string();
                        svc.current_health = health;
                    }
                    let on_backup = fail_state
                        .services
                        .get(i)
                        .map(|s| s.is_on_backup)
                        .unwrap_or(false);
                    if health.is_functional() {
                        consecutive_healthy[i] += 1;
                        if on_backup && consecutive_healthy[i] >= RECOVERY_THRESHOLD {
                            fail_state.recovery_count += 1;
                            fail_state.transition_count += 1;
                            if let Some(svc) = fail_state.services.get_mut(i) {
                                svc.is_on_backup = false;
                            }
                            consecutive_healthy[i] = 0;
                        }
                    } else {
                        consecutive_healthy[i] = 0;
                        if !on_backup {
                            fail_state.active_count += 1;
                            fail_state.transition_count += 1;
                            if let Some(svc) = fail_state.services.get_mut(i) {
                                svc.is_on_backup = true;
                            }
                        }
                    }
                }
            }
        });
        if let Ok(mut guard) = self.monitor_handle.write() {
            *guard = Some(handle);
        }
    }
    /// Get a snapshot of current failover state.
    pub fn state(&self) -> FailoverState {
        self.state.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Default for FailoverCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failover::Result as FailoverResult;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct MockService {
        healthy: AtomicBool,
        backup_activated: AtomicBool,
    }
    impl MockService {
        fn new(healthy: bool) -> Self {
            Self {
                healthy: AtomicBool::new(healthy),
                backup_activated: AtomicBool::new(false),
            }
        }
    }
    #[async_trait]
    impl Failover<(), ()> for MockService {
        async fn primary_health(&self) -> Health {
            if self.healthy.load(Ordering::SeqCst) {
                Health::Healthy
            } else {
                Health::Unhealthy
            }
        }
        async fn activate_backup(&mut self) -> FailoverResult<()> {
            self.backup_activated.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn sync_backup(&mut self) -> FailoverResult<()> {
            Ok(())
        }
        async fn restore_to_primary(&mut self) -> FailoverResult<()> {
            self.backup_activated.store(false, Ordering::SeqCst);
            Ok(())
        }
    }
    #[test]
    fn coordinator_state_default() {
        let coord = FailoverCoordinator::new();
        let state = coord.state();
        assert_eq!(state.active_count, 0);
        assert_eq!(state.transition_count, 0);
        assert_eq!(state.services.len(), 0);
    }
    #[tokio::test]
    async fn failover_trait_object_safe() {
        fn _assert_dyn(_: Arc<dyn Failover<(), ()> + Send + Sync>) {}
        let svc = MockService::new(true);
        let boxed: Arc<dyn Failover<(), ()> + Send + Sync> = Arc::new(svc);
        let health = boxed.primary_health().await;
        assert!(matches!(health, Health::Healthy));
    }
}
