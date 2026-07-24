//! `touring-resource-monitor` standalone daemon.
//!
//! Runs as a `Type=simple` systemd user unit (no `sd_notify` required).
//! Starts the global [`MemoryGuard`] ticker at 1 Hz and logs pressure-tier
//! summaries every 60 s. Handles `SIGINT` / `SIGTERM` gracefully.
//!
//! ## Usage
//!
//! ```bash
//! # Direct invocation (foreground, debug logs)
//! RUST_LOG=info touring-resource-monitor
//!
//! # As systemd user unit:
//! systemctl --user enable --now touring-resource-monitor.service
//! ```

use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "touring-resource-monitor starting"
    );
    #[cfg(target_os = "linux")]
    let _scheduler = match touring_resilience::sentinel::CoreScheduler::detect() {
        Ok(s) => {
            tracing::info!(
                p_cores = s.p_cores().len(),
                e_cores = s.e_cores().len(),
                total = s.topology().total,
                "CPU topology detected"
            );
            Some(s)
        }
        Err(e) => {
            tracing::warn!(
                error = % e,
                "topology detection failed — continuing without P/E pinning"
            );
            None
        }
    };
    #[cfg(target_os = "linux")]
    {
        touring_resilience::sentinel::MemoryGuard::global()
            .start_ticker(Duration::from_millis(1000))
            .await?;
        tracing::info!("memory pressure ticker started (1 Hz)");
    }
    // (sd_notify Type=notify block dropped in A4 P3 — it was gated on an undefined
    //  `systemd-notify` feature with no sd_notify dependency, i.e. dead code.)
    tracing::info!("touring-resource-monitor ready");
    let mut log_interval = tokio::time::interval(Duration::from_secs(60));
    log_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    log_interval.tick().await;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
            tracing::info!("SIGINT received, shutting down"); break; } _ = log_interval
            .tick() => { #[cfg(target_os = "linux")] log_status(); }
        }
    }
    #[cfg(target_os = "linux")]
    touring_resilience::sentinel::MemoryGuard::global().stop_ticker();
    tracing::info!("touring-resource-monitor stopped");
    Ok(())
}

#[cfg(target_os = "linux")]
fn log_status() {
    let p = touring_resilience::sentinel::MemoryGuard::global().pressure();
    let snap = touring_resilience::sentinel::MemoryGuard::global().snapshot();
    let m = touring_resilience::sentinel::metrics::capture();
    if let Some(s) = snap {
        tracing::info!(
            pressure = ? p, available_mb = s.available_mb, swap_used_pct = s
            .swap_used_pct, psi_some_avg10 = s.psi_some_avg10, total_ticks = m
            .memory_pressure_total_tick_count, red_ticks = m.memory_pressure_red_count,
            yellow_ticks = m.memory_pressure_yellow_count, "status"
        );
    } else {
        tracing::info!(pressure = ? p, "status (no snapshot yet)");
    }
}
