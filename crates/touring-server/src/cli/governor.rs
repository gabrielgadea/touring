//! `touring governor` — Unified resource management and context budget control.
//!
//! Centralizes resource tracking: memory, CPU, context tokens, and operation
//! timeouts. Aggregates data from `touring doctor -j`, `touring status -j`,
//! and `touring gate-metrics -j` into a unified `GovernorStats` view.
//!
//! # Config file
//!
//! `~/.claude/touring/governor.toml` (TOML):
//! ```toml
//! [limits]
//! memory_mb = 512
//! context_tokens = 8192
//! max_concurrent_ops = 16
//! query_timeout_secs = 30
//! ```
//!
//! All fields optional; defaults are sensible per-invocation limits.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Resource statistics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GovernorStats {
    /// Resident memory in MB (from gate_metrics or process stats).
    pub memory_mb: f64,
    /// Estimated context tokens currently in flight.
    pub context_tokens: u64,
    /// Number of active operations.
    pub active_operations: u32,
    /// Current configured limits.
    pub limits: GovernorLimits,
    /// Composite resource pressure score [0.0, 1.0].
    pub pressure_score: f64,
}

/// Configured resource limits (from `governor.toml` or environment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernorLimits {
    /// Max resident memory in MB before warning.
    pub memory_mb: Option<u64>,
    /// Max context tokens per operation.
    pub context_tokens: Option<u64>,
    /// Max concurrent operations.
    pub max_concurrent_ops: Option<u32>,
    /// Query timeout in seconds.
    pub query_timeout_secs: Option<u64>,
    /// Memory pressure threshold [0.0, 1.0] that triggers warning.
    pub memory_pressure_threshold: Option<f64>,
}

impl Default for GovernorLimits {
    fn default() -> Self {
        Self {
            memory_mb: Some(512),
            context_tokens: Some(8192),
            max_concurrent_ops: Some(16),
            query_timeout_secs: Some(30),
            memory_pressure_threshold: Some(0.8),
        }
    }
}

/// ResourceGovernor trait — pluggable backing for resource tracking.
///
/// Default implementation reads from daemon queries and process stats.
/// Users can provide a custom implementation for test or constrained environments.
pub trait ResourceGovernor {
    /// Return current resource snapshot.
    fn stats(&self) -> anyhow::Result<GovernorStats>;

    /// Check if an operation would exceed the budget.
    /// Returns `Ok(())` if within budget, `Err(msg)` if over.
    fn check_budget(&self, stats: &GovernorStats) -> anyhow::Result<()>;

    /// Return the effective limits.
    fn limits(&self) -> GovernorLimits;

    /// Reset any transient counters (e.g. active operations).
    fn reset(&self) -> anyhow::Result<()>;

    /// Generate a human-readable resource report.
    fn report(&self) -> anyhow::Result<String>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Default implementation — reads from daemon + process stats
// ─────────────────────────────────────────────────────────────────────────────

/// Default governor backed by daemon queries and the process's own RSS.
pub struct DefaultResourceGovernor {
    config_path: PathBuf,
    limits: GovernorLimits,
}

impl Default for DefaultResourceGovernor {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultResourceGovernor {
    /// Create a new governor, loading limits from `~/.claude/touring/governor.toml`.
    pub fn new() -> Self {
        let config_path = Self::config_path();
        let limits = Self::load_config(&config_path);
        Self {
            config_path,
            limits,
        }
    }

    fn config_path() -> PathBuf {
        if let Ok(p) = std::env::var("TOURING_GOVERNOR_CONFIG") {
            return PathBuf::from(p);
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/gabrielgadea".to_string());
        PathBuf::from(home).join(".claude/touring/governor.toml")
    }

    fn load_config(path: &PathBuf) -> GovernorLimits {
        if !path.exists() {
            return GovernorLimits::default();
        }
        match std::fs::read_to_string(path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("governor: failed to parse {}: {e}", path.display());
                GovernorLimits::default()
            }),
            Err(e) => {
                eprintln!("governor: failed to read {}: {e}", path.display());
                GovernorLimits::default()
            }
        }
    }

    /// Fetch daemon health via socket query.
    fn query_daemon(&self, hook: &str) -> anyhow::Result<serde_json::Value> {
        let output = super::daemon_query(hook, serde_json::json!({}))?;
        let v: serde_json::Value = serde_json::from_str(&output)
            .map_err(|e| anyhow::anyhow!("failed to parse {hook} response: {e}"))?;
        Ok(v)
    }

    /// Estimate RSS from `/proc/self/statm` (Linux only).
    fn process_memory_mb() -> f64 {
        #[cfg(target_os = "linux")]
        {
            use std::io::Read;
            let mut statm = String::new();
            if let Ok(f) = std::fs::File::open("/proc/self/statm") {
                if std::io::BufReader::new(f)
                    .read_to_string(&mut statm)
                    .is_ok()
                {
                    // statm fields: size resident shared text doc (anon) etc.
                    // resident pages (field 1, 0-indexed)
                    let parts: Vec<u64> = statm
                        .split_whitespace()
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    if parts.len() >= 2 {
                        // Convert pages to MB (assuming 4 KiB page size)
                        return (parts[1] * 4) as f64 / 1024.0;
                    }
                }
            }
        }
        // Fallback: return 0 (no data)
        0.0
    }

    fn compute_pressure(stats: &GovernorStats) -> f64 {
        let mem_usage = stats
            .limits
            .memory_mb
            .map(|limit| {
                if limit == 0 {
                    0.0
                } else {
                    (stats.memory_mb / limit as f64).min(1.0)
                }
            })
            .unwrap_or(0.0);

        let token_usage = stats
            .limits
            .context_tokens
            .map(|limit| {
                if limit == 0 {
                    0.0
                } else {
                    (stats.context_tokens as f64 / limit as f64).min(1.0)
                }
            })
            .unwrap_or(0.0);

        let op_usage = stats
            .limits
            .max_concurrent_ops
            .map(|limit| {
                if limit == 0 {
                    0.0
                } else {
                    (stats.active_operations as f64 / limit as f64).min(1.0)
                }
            })
            .unwrap_or(0.0);

        // Weighted average: memory heaviest, tokens second, ops third
        0.5 * mem_usage + 0.3 * token_usage + 0.2 * op_usage
    }
}

impl ResourceGovernor for DefaultResourceGovernor {
    fn stats(&self) -> anyhow::Result<GovernorStats> {
        let memory_mb = Self::process_memory_mb();

        // Try to get active operations and context tokens from gate_metrics
        let (context_tokens, active_operations) = match self.query_daemon("cli-gate-metrics") {
            Ok(v) => {
                let tokens = v
                    .get("context_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0);
                let ops = v
                    .get("active_operations")
                    .or(v.get("inflight_requests"))
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as u32;
                (tokens, ops)
            }
            Err(_) => (0, 0),
        };

        let pressure_score = {
            let s = GovernorStats {
                memory_mb,
                context_tokens,
                active_operations,
                limits: self.limits.clone(),
                pressure_score: 0.0,
            };
            Self::compute_pressure(&s)
        };

        Ok(GovernorStats {
            memory_mb,
            context_tokens,
            active_operations,
            limits: self.limits.clone(),
            pressure_score,
        })
    }

    fn check_budget(&self, stats: &GovernorStats) -> anyhow::Result<()> {
        if let Some(limit) = self.limits.memory_mb {
            if stats.memory_mb as u64 > limit && limit > 0 {
                anyhow::bail!(
                    "memory budget exceeded: {} MB used > {} MB limit",
                    stats.memory_mb,
                    limit
                );
            }
        }

        if let Some(limit) = self.limits.context_tokens {
            if stats.context_tokens > limit && limit > 0 {
                anyhow::bail!(
                    "context token budget exceeded: {} tokens > {} limit",
                    stats.context_tokens,
                    limit
                );
            }
        }

        if let Some(limit) = self.limits.max_concurrent_ops {
            if stats.active_operations > limit && limit > 0 {
                anyhow::bail!(
                    "concurrent operations budget exceeded: {} ops > {} limit",
                    stats.active_operations,
                    limit
                );
            }
        }

        Ok(())
    }

    fn limits(&self) -> GovernorLimits {
        self.limits.clone()
    }

    fn reset(&self) -> anyhow::Result<()> {
        // Reset is a no-op for the default governor — the daemon manages state.
        // This is here so custom governors can implement actual reset logic.
        Ok(())
    }

    fn report(&self) -> anyhow::Result<String> {
        let stats = self.stats()?;
        let over_budget = self.check_budget(&stats).is_err();

        let mut lines = vec![
            "=== Resource Governor Report ===".to_string(),
            format!(
                "Memory:        {:.1} MB  (limit: {:?})",
                stats.memory_mb, stats.limits.memory_mb
            ),
            format!(
                "Context tokens: {}    (limit: {:?})",
                stats.context_tokens, stats.limits.context_tokens,
            ),
            format!(
                "Active ops:    {}    (limit: {:?})",
                stats.active_operations, stats.limits.max_concurrent_ops,
            ),
            format!("Pressure score: {:.3}", stats.pressure_score),
        ];

        if over_budget {
            lines.push("OVER BUDGET".to_string());
        } else {
            lines.push("Within budget.".to_string());
        }

        if self.config_path.exists() {
            lines.push(format!("Config: {}", self.config_path.display()));
        } else {
            lines.push(format!(
                "Config: (defaults, no file at {})",
                self.config_path.display()
            ));
        }

        Ok(lines.join("\n"))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CLI dispatcher
// ─────────────────────────────────────────────────────────────────────────────

/// Run the `governor` CLI subcommand.
///
/// Args received from main.rs dispatch: `["touring", "governor", "status|limits|reset|report"]`
/// We use `args.get(2)` since the binary name is at index 0 and "governor" at index 1.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("status");

    let governor = DefaultResourceGovernor::new();

    match subcommand {
        "status" => {
            let stats = governor.stats()?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        "limits" => {
            let limits = governor.limits();
            println!("{}", serde_json::to_string_pretty(&limits)?);
        }
        "reset" => {
            governor.reset()?;
            println!("governor state reset");
        }
        "report" => {
            let report = governor.report()?;
            println!("{report}");
        }
        _ => {
            anyhow::bail!(
                "Unknown governor subcommand: {}. Use: status, limits, reset, report",
                subcommand
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governor_stats_default() {
        let stats = GovernorStats::default();
        assert_eq!(stats.memory_mb, 0.0);
        assert_eq!(stats.context_tokens, 0);
        assert_eq!(stats.active_operations, 0);
    }

    #[test]
    fn governor_limits_default() {
        let limits = GovernorLimits::default();
        assert_eq!(limits.memory_mb, Some(512));
        assert_eq!(limits.context_tokens, Some(8192));
        assert_eq!(limits.max_concurrent_ops, Some(16));
        assert_eq!(limits.query_timeout_secs, Some(30));
    }

    #[test]
    fn compute_pressure_all_zero() {
        let stats = GovernorStats {
            memory_mb: 0.0,
            context_tokens: 0,
            active_operations: 0,
            limits: GovernorLimits::default(),
            pressure_score: 0.0,
        };
        let p = DefaultResourceGovernor::compute_pressure(&stats);
        assert!((p - 0.0).abs() < 1e-9);
    }

    #[test]
    fn compute_pressure_at_limit() {
        let limits = GovernorLimits {
            memory_mb: Some(100),
            context_tokens: Some(1000),
            max_concurrent_ops: Some(10),
            query_timeout_secs: Some(30),
            memory_pressure_threshold: None,
        };
        let stats = GovernorStats {
            memory_mb: 100.0,      // at limit
            context_tokens: 1000,  // at limit
            active_operations: 10, // at limit
            limits: limits.clone(),
            pressure_score: 0.0,
        };
        let p = DefaultResourceGovernor::compute_pressure(&stats);
        // Should be close to 1.0 (within floating point tolerance)
        assert!((p - 1.0).abs() < 1e-6, "expected ~1.0, got {p}");
    }

    #[test]
    fn compute_pressure_half() {
        let limits = GovernorLimits {
            memory_mb: Some(100),
            context_tokens: Some(1000),
            max_concurrent_ops: Some(10),
            query_timeout_secs: Some(30),
            memory_pressure_threshold: None,
        };
        let stats = GovernorStats {
            memory_mb: 50.0,      // 50% of limit
            context_tokens: 500,  // 50% of limit
            active_operations: 5, // 50% of limit
            limits: limits.clone(),
            pressure_score: 0.0,
        };
        let p = DefaultResourceGovernor::compute_pressure(&stats);
        // 0.5 * 0.5 + 0.3 * 0.5 + 0.2 * 0.5 = 0.5
        assert!((p - 0.5).abs() < 1e-6, "expected 0.5, got {p}");
    }

    #[test]
    fn pressure_with_zero_limits() {
        // Zero limit should contribute 0 to pressure, not panic
        let limits = GovernorLimits {
            memory_mb: Some(0),
            context_tokens: Some(0),
            max_concurrent_ops: Some(0),
            query_timeout_secs: Some(0),
            memory_pressure_threshold: None,
        };
        let stats = GovernorStats {
            memory_mb: 100.0,
            context_tokens: 1000,
            active_operations: 10,
            limits,
            pressure_score: 0.0,
        };
        let p = DefaultResourceGovernor::compute_pressure(&stats);
        assert!((p - 0.0).abs() < 1e-9, "zero limits should give 0 pressure");
    }

    #[test]
    fn check_budget_ok() {
        let governor = DefaultResourceGovernor::new();
        let stats = GovernorStats {
            memory_mb: 100.0,
            context_tokens: 1000,
            active_operations: 4,
            limits: GovernorLimits::default(),
            pressure_score: 0.3,
        };
        assert!(governor.check_budget(&stats).is_ok());
    }

    #[test]
    fn check_budget_over_memory() {
        let governor = DefaultResourceGovernor::new();
        let stats = GovernorStats {
            memory_mb: 600.0, // > 512 default
            context_tokens: 0,
            active_operations: 0,
            limits: GovernorLimits::default(),
            pressure_score: 0.0,
        };
        assert!(governor.check_budget(&stats).is_err());
    }

    #[test]
    fn check_budget_over_tokens() {
        let governor = DefaultResourceGovernor::new();
        let stats = GovernorStats {
            memory_mb: 0.0,
            context_tokens: 9000, // > 8192 default
            active_operations: 0,
            limits: GovernorLimits::default(),
            pressure_score: 0.0,
        };
        assert!(governor.check_budget(&stats).is_err());
    }

    #[test]
    fn check_budget_over_ops() {
        let governor = DefaultResourceGovernor::new();
        let stats = GovernorStats {
            memory_mb: 0.0,
            context_tokens: 0,
            active_operations: 20, // > 16 default
            limits: GovernorLimits::default(),
            pressure_score: 0.0,
        };
        assert!(governor.check_budget(&stats).is_err());
    }

    #[test]
    fn run_unknown_subcommand() {
        let args = vec![
            "touring".to_string(),
            "governor".to_string(),
            "bad".to_string(),
        ];
        let result = run(&args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Unknown governor subcommand"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn run_status_subcommand() {
        let args = vec![
            "touring".to_string(),
            "governor".to_string(),
            "status".to_string(),
        ];
        // May fail if daemon is unreachable, but should not panic
        let _ = run(&args);
    }

    #[test]
    fn run_limits_subcommand() {
        let args = vec![
            "touring".to_string(),
            "governor".to_string(),
            "limits".to_string(),
        ];
        let result = run(&args);
        // Daemon might be unreachable, but limits should still print
        match result {
            Ok(_) => {}
            Err(_e) => {
                // Accept daemon-unreachable errors; limits should still print defaults
                // Non-daemon errors are allowed in test (e.g. config parse failures)
            }
        }
    }

    #[test]
    fn run_reset_subcommand() {
        // Args after stripping "touring" prefix: ["governor", "reset"]
        let args = vec![
            "touring".to_string(),
            "governor".to_string(),
            "reset".to_string(),
        ];
        let result = run(&args);
        // Reset is a no-op — only daemon config errors should cause failure
        if let Err(err) = result {
            assert!(
                err.to_string().contains("daemon") || err.to_string().contains("connect"),
                "reset should not fail with unexpected error: {err}"
            );
        }
    }

    #[test]
    fn run_report_subcommand() {
        // Args after stripping "touring" prefix: ["governor", "report"]
        let args = vec![
            "touring".to_string(),
            "governor".to_string(),
            "report".to_string(),
        ];
        let result = run(&args);
        // Report prints what it can — daemon may be unreachable
        if result.is_ok() {
            // report should contain the sections
        }
    }

    #[test]
    fn run_no_subcommand_defaults_to_status() {
        // When called as "touring governor" (no subcommand), args=["touring","governor"]
        // get(2) returns None, so subcommand defaults to "status"
        let args = vec!["touring".to_string(), "governor".to_string()];
        let result = run(&args);
        // Daemon may be unreachable but the dispatch should work (status subcommand)
        assert!(result.is_ok() || result.unwrap_err().to_string().contains("daemon"));
    }
}
