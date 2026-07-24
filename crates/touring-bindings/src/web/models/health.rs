//! Health dashboard model — parsed from `touring doctor -j`.

use serde::{Deserialize, Serialize};

/// Component health entry from `touring doctor -j`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorEntry {
    /// Component name (e.g. "daemon_socket", "knowledge_db").
    pub name: String,
    /// Health status ("ok" or "error").
    pub status: String,
    /// Additional detail about the component status.
    #[serde(default)]
    pub detail: String,
}

/// Full doctor report — list of component health entries.
pub type DoctorReport = Vec<DoctorEntry>;

/// Status dashboard — parsed from `touring status -j`.
#[derive(Debug, Clone, Deserialize)]
pub struct StatusDashboard {
    /// Index statistics (symbol counts).
    #[serde(rename = "index")]
    pub index: IndexStats,
    /// Wiring statistics (orphan counts).
    #[serde(rename = "wiring")]
    pub wiring: WiringStats,
    /// Learning system statistics (EMA reward).
    #[serde(rename = "learning")]
    pub learning: LearningStats,
    /// Overall composite health score (0.0–1.0).
    #[serde(rename = "composite_health_score")]
    pub composite_health_score: f64,
}

impl Default for StatusDashboard {
    fn default() -> Self {
        Self {
            index: IndexStats { symbol_count: 0 },
            wiring: WiringStats { orphan_count: 0 },
            learning: LearningStats { ema_reward: 0.0 },
            composite_health_score: 0.0,
        }
    }
}

/// Index statistics from `touring status -j`.
#[derive(Debug, Clone, Deserialize)]
pub struct IndexStats {
    /// Total number of indexed symbols.
    #[serde(rename = "symbol_count")]
    pub symbol_count: usize,
}

/// Wiring statistics from `touring status -j`.
#[derive(Debug, Clone, Deserialize)]
pub struct WiringStats {
    /// Number of orphan pub symbols (no consumers).
    #[serde(rename = "orphan_count")]
    pub orphan_count: usize,
}

/// Learning system statistics from `touring status -j`.
#[derive(Debug, Clone, Deserialize)]
pub struct LearningStats {
    /// Exponential moving average of RL reward signal.
    #[serde(rename = "ema_reward")]
    pub ema_reward: f64,
}

/// Unified health view for the UI — derived from doctor + status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthView {
    /// Whether the touring daemon is fully healthy.
    pub daemon_healthy: bool,
    /// Per-component health entries from `touring doctor`.
    pub components: Vec<DoctorEntry>,
    /// Total indexed symbol count.
    pub symbol_count: usize,
    /// Number of orphan pub symbols.
    pub orphan_count: usize,
    /// RL EMA reward value.
    pub ema_reward: f64,
    /// Overall composite health score (0.0–1.0).
    pub composite_score: f64,
}

impl HealthView {
    /// Constructs a `HealthView` from a doctor report and status dashboard.
    pub fn from_doctor_and_status(doctor: DoctorReport, status: StatusDashboard) -> Self {
        // Liveness reflects DAEMON components only — advisory entries such
        // as wiring_diagnostic legitimately report "warning" while the
        // daemon is up (cross-audit 2026-06-11, BUG-1: the hero showed
        // "DAEMON DOWN" next to a healthy daemon row).
        let daemon_healthy = doctor
            .iter()
            .filter(|e| matches!(e.name.as_str(), "daemon_socket" | "daemon_health"))
            .all(|e| e.status == "ok");
        Self {
            daemon_healthy,
            components: doctor,
            symbol_count: status.index.symbol_count,
            orphan_count: status.wiring.orphan_count,
            ema_reward: status.learning.ema_reward,
            composite_score: status.composite_health_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doctor_entry_serde_roundtrip() {
        let entry = DoctorEntry {
            name: "daemon_socket".to_string(),
            status: "ok".to_string(),
            detail: "/tmp/touring-daemon-1000.sock".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: DoctorEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, entry.name);
        assert_eq!(parsed.status, entry.status);
        assert_eq!(parsed.detail, entry.detail);
    }

    #[test]
    fn test_status_dashboard_default() {
        let dashboard = StatusDashboard::default();
        assert_eq!(dashboard.index.symbol_count, 0);
        assert_eq!(dashboard.wiring.orphan_count, 0);
        assert_eq!(dashboard.learning.ema_reward, 0.0);
        assert_eq!(dashboard.composite_health_score, 0.0);
    }

    #[test]
    fn test_health_view_from_doctor_and_status() {
        let doctor = vec![
            DoctorEntry {
                name: "daemon_socket".to_string(),
                status: "ok".to_string(),
                detail: "healthy".to_string(),
            },
            DoctorEntry {
                name: "knowledge_db".to_string(),
                status: "ok".to_string(),
                detail: "connected".to_string(),
            },
        ];
        let status = StatusDashboard {
            index: IndexStats { symbol_count: 1000 },
            wiring: WiringStats { orphan_count: 50 },
            learning: LearningStats { ema_reward: 0.15 },
            composite_health_score: 0.85,
        };
        let view = HealthView::from_doctor_and_status(doctor, status);
        assert!(view.daemon_healthy);
        assert_eq!(view.symbol_count, 1000);
        assert_eq!(view.orphan_count, 50);
        assert_eq!(view.composite_score, 0.85);
    }
}
