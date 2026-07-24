//! Global UI contexts — WorkspaceCtx + RefreshBus (SPEC 2026-06-12 §4.4).
//!
//! `WorkspaceCtx` carries the live project path + daemon status so the
//! titlebar breadcrumb and the sidebar footer stop prop-drilling.
//! `RefreshBus` is the single page-refresh tick: pages subscribe to
//! `tick` inside their `LocalResource` closures instead of owning
//! per-page timers. The interval is user-configurable in /settings.

use leptos::prelude::*;

/// Daemon connection state shown in the sidebar footer (el-pulse dot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DaemonStatus {
    /// Not yet probed.
    #[default]
    Unknown,
    /// All daemon components healthy.
    Healthy,
    /// Some components degraded.
    Degraded,
    /// API unreachable.
    Down,
}

impl DaemonStatus {
    /// Human label for the footer.
    pub fn label(self) -> &'static str {
        match self {
            DaemonStatus::Unknown => "probing…",
            DaemonStatus::Healthy => "daemon healthy",
            DaemonStatus::Degraded => "daemon degraded",
            DaemonStatus::Down => "daemon down",
        }
    }

    /// Color token for the status dot.
    pub fn color(self) -> &'static str {
        match self {
            DaemonStatus::Unknown => "var(--el-fg-5)",
            DaemonStatus::Healthy => "var(--el-pos)",
            DaemonStatus::Degraded => "var(--el-warn)",
            DaemonStatus::Down => "var(--el-neg)",
        }
    }
}

/// Workspace-level reactive context (SPEC §4.4 item 4).
#[derive(Clone, Copy)]
pub struct WorkspaceCtx {
    /// Project root path reported by `/api/status` (quality_signal.root).
    pub project_path: RwSignal<String>,
    /// Live daemon health classification.
    pub daemon_status: RwSignal<DaemonStatus>,
}

impl WorkspaceCtx {
    /// Create + provide the context. Call once from `App`.
    pub fn provide() -> Self {
        let ctx = WorkspaceCtx {
            project_path: RwSignal::new(String::new()),
            daemon_status: RwSignal::new(DaemonStatus::Unknown),
        };
        provide_context(ctx);
        ctx
    }

    /// Classify a `/api/status` JSON payload into a [`DaemonStatus`].
    pub fn classify_status(v: &serde_json::Value) -> DaemonStatus {
        let healthy = v
            .pointer("/daemon_health/healthy_count")
            .and_then(|x| x.as_u64());
        let total = v
            .pointer("/daemon_health/total_count")
            .and_then(|x| x.as_u64());
        match (healthy, total) {
            (Some(h), Some(t)) if t > 0 && h == t => DaemonStatus::Healthy,
            (Some(h), Some(t)) if t > 0 && h < t => DaemonStatus::Degraded,
            _ => DaemonStatus::Down,
        }
    }

    /// Extract the project root path from a `/api/status` payload.
    pub fn extract_root(v: &serde_json::Value) -> String {
        v.pointer("/quality_signal/root")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    }
}

/// Reactive helper — last path segment of the project root for the
/// breadcrumb ("…/rust" → "rust"). Empty input yields "workspace".
pub fn workspace_label(path: &str) -> String {
    let tail = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if tail.is_empty() {
        "workspace".to_string()
    } else {
        tail.to_string()
    }
}

/// Single refresh tick for every page (SPEC §4.4 item 5).
#[derive(Clone, Copy)]
pub struct RefreshBus {
    /// Monotonic tick — pages read this inside resource closures.
    pub tick: RwSignal<u64>,
    /// Interval in seconds (0 = paused). Configurable in /settings.
    pub interval_secs: RwSignal<u32>,
}

impl RefreshBus {
    /// Create + provide the bus. Call once from `App`.
    pub fn provide(default_secs: u32) -> Self {
        let bus = RefreshBus {
            tick: RwSignal::new(0),
            interval_secs: RwSignal::new(default_secs),
        };
        provide_context(bus);
        bus
    }

    /// Start the browser interval. Re-creates the timer whenever
    /// `interval_secs` changes. No-op outside wasm (CSR-only app).
    pub fn start(self) {
        #[cfg(target_arch = "wasm32")]
        {
            use std::time::Duration;
            Effect::new(move |prev_handle: Option<Option<IntervalHandle>>| {
                // Drop the previous interval when the cadence changes.
                if let Some(Some(h)) = prev_handle {
                    h.clear();
                }
                let secs = self.interval_secs.get();
                if secs == 0 {
                    return None;
                }
                set_interval_with_handle(
                    move || self.tick.update(|t| *t += 1),
                    Duration::from_secs(u64::from(secs)),
                )
                .ok()
            });
        }
    }
}

/// Convenience accessor — panics with a clear message when the context
/// is missing (programmer error: `App` must call `WorkspaceCtx::provide`).
pub fn use_workspace() -> WorkspaceCtx {
    use_context::<WorkspaceCtx>().expect("WorkspaceCtx provided in App")
}

/// Convenience accessor for the refresh bus.
pub fn use_refresh_bus() -> RefreshBus {
    use_context::<RefreshBus>().expect("RefreshBus provided in App")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_status_healthy_when_all_components_ok() {
        let v = serde_json::json!({"daemon_health": {"healthy_count": 8, "total_count": 8}});
        assert_eq!(WorkspaceCtx::classify_status(&v), DaemonStatus::Healthy);
    }

    #[test]
    fn classify_status_degraded_when_partial() {
        let v = serde_json::json!({"daemon_health": {"healthy_count": 5, "total_count": 8}});
        assert_eq!(WorkspaceCtx::classify_status(&v), DaemonStatus::Degraded);
    }

    #[test]
    fn classify_status_down_on_missing_payload() {
        let v = serde_json::json!({"error": "fetch failed"});
        assert_eq!(WorkspaceCtx::classify_status(&v), DaemonStatus::Down);
    }

    #[test]
    fn extract_root_reads_quality_signal_root() {
        let v = serde_json::json!({"quality_signal": {"root": "/home/g/rust"}});
        assert_eq!(WorkspaceCtx::extract_root(&v), "/home/g/rust");
    }

    #[test]
    fn workspace_label_takes_last_segment() {
        assert_eq!(workspace_label("/home/g/.claude/rust"), "rust");
        assert_eq!(workspace_label("/home/g/.claude/rust/"), "rust");
        assert_eq!(workspace_label(""), "workspace");
    }

    #[test]
    fn daemon_status_labels_and_colors_are_distinct() {
        let all = [
            DaemonStatus::Unknown,
            DaemonStatus::Healthy,
            DaemonStatus::Degraded,
            DaemonStatus::Down,
        ];
        for s in all {
            assert!(!s.label().is_empty());
            assert!(s.color().starts_with("var(--el-"));
        }
    }
}
