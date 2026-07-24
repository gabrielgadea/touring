//! EvaporationRateMetrics — external consumer surface for [`UnifiedPheromoneBus::evaporation_rate()`].

use crate::rl::aco::UnifiedPheromoneBus;
use std::sync::Arc;

/// Provides external read access to the current evaporation rate of a pheromone bus.
/// Consumed by: telemetry exporters, health dashboards, RL monitoring, and any
/// component that needs to observe the ACO parameter space without modifying it.
#[derive(Debug, Clone)]
pub struct EvaporationRateMetrics {
    bus: Arc<UnifiedPheromoneBus>,
}

impl EvaporationRateMetrics {
    /// Wrap a reference to a [`UnifiedPheromoneBus`].
    #[inline]
    pub fn new(bus: Arc<UnifiedPheromoneBus>) -> Self {
        Self { bus }
    }

    /// Current evaporation rate as a percentage (0.0–100.0).
    ///
    /// Mirrors `UnifiedPheromoneBus::evaporation_rate()`.
    #[inline]
    pub fn rate_percent(&self) -> f64 {
        self.bus.evaporation_rate() * 100.0
    }

    /// Current evaporation rate as a raw fraction (0.0–1.0).
    #[inline]
    pub fn rate_fraction(&self) -> f64 {
        self.bus.evaporation_rate()
    }

    /// Returns `true` if the rate is in the low range (< 20%).
    #[inline]
    pub fn is_low(&self) -> bool {
        self.bus.evaporation_rate() < 0.20
    }

    /// Returns `true` if the rate is in the medium range (20–50%).
    #[inline]
    pub fn is_medium(&self) -> bool {
        let r = self.bus.evaporation_rate();
        r >= 0.20 && r <= 0.50
    }

    /// Returns `true` if the rate is in the high range (> 50%).
    #[inline]
    pub fn is_high(&self) -> bool {
        self.bus.evaporation_rate() > 0.50
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::aco::UnifiedPheromoneBus;

    #[test]
    fn rate_percent_returns_valid_percentage() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.25));
        let metrics = EvaporationRateMetrics::new(bus);
        assert!((metrics.rate_percent() - 25.0).abs() < 1e-9);
    }

    #[test]
    fn rate_fraction_returns_exact_value() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.42));
        bus.set_evaporation_rate(0.42);
        let metrics = EvaporationRateMetrics::new(bus);
        assert!((metrics.rate_fraction() - 0.42).abs() < 1e-9);
    }

    #[test]
    fn is_low_medium_high() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.0));
        let metrics = EvaporationRateMetrics::new(Arc::clone(&bus));

        bus.set_evaporation_rate(0.10);
        assert!(metrics.is_low());
        assert!(!metrics.is_medium());
        assert!(!metrics.is_high());

        bus.set_evaporation_rate(0.35);
        assert!(!metrics.is_low());
        assert!(metrics.is_medium());
        assert!(!metrics.is_high());

        bus.set_evaporation_rate(0.75);
        assert!(!metrics.is_low());
        assert!(!metrics.is_medium());
        assert!(metrics.is_high());
    }
}
