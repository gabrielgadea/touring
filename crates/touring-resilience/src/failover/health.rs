//! Health status enum for failover checks.

use serde::{Deserialize, Serialize};

/// Health status for a service (primary or backup).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Health {
    /// Service is fully healthy.
    #[default]
    Healthy,
    /// Service is degraded but functional.
    Degraded,
    /// Service is unavailable.
    Unhealthy,
}

impl Health {
    /// True if the service is considered functional (healthy or degraded).
    pub fn is_functional(self) -> bool {
        matches!(self, Health::Healthy | Health::Degraded)
    }
    /// Convert to a u8 for counters (0=healthy, 1=degraded, 2=unhealthy).
    pub fn as_u8(self) -> u8 {
        match self {
            Health::Healthy => 0,
            Health::Degraded => 1,
            Health::Unhealthy => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn health_functional() {
        assert!(Health::Healthy.is_functional());
        assert!(Health::Degraded.is_functional());
        assert!(!Health::Unhealthy.is_functional());
    }
    #[test]
    fn health_as_u8() {
        assert_eq!(Health::Healthy.as_u8(), 0);
        assert_eq!(Health::Degraded.as_u8(), 1);
        assert_eq!(Health::Unhealthy.as_u8(), 2);
    }
}
