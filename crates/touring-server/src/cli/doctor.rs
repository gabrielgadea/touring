//! `touring doctor` — Diagnostics: socket, schema, circuit breaker, health.
//!
//! Performs a series of health checks and reports the overall system state.

use super::common::{human_to_stderr, json_to_stdout, parse_global_flags};
use std::path::PathBuf;

/// Individual diagnostic check result.
#[derive(serde::Serialize)]
struct Check {
    name: &'static str,
    status: &'static str,
    detail: String,
}

fn check_daemon_socket() -> Check {
    // W12.5 unification (2026-07-24): doctor must probe the SAME socket every
    // other component resolves (canonical env → legacy env → walk-up → global),
    // or it reports on a daemon nobody is actually talking to.
    let sock = touring_foundation::config::TouringConfig::resolve_daemon_socket_path();

    if sock.exists() {
        // Try connecting
        match std::os::unix::net::UnixStream::connect(&sock) {
            Ok(_) => Check {
                name: "daemon_socket",
                status: "ok",
                detail: format!("{}", sock.display()),
            },
            Err(e) => Check {
                name: "daemon_socket",
                status: "error",
                detail: format!("socket exists but connect failed: {e}"),
            },
        }
    } else {
        Check {
            name: "daemon_socket",
            status: "missing",
            detail: format!("{} not found", sock.display()),
        }
    }
}

fn check_circuit_breaker() -> Check {
    let uid = unsafe { super::libc_getuid() };
    let state_path = PathBuf::from(format!("/tmp/touring-circuit-{uid}.state"));

    if state_path.exists() {
        match std::fs::read_to_string(&state_path) {
            Ok(content) => {
                // Parse JSON and check if any breaker is actually open (open_until_ts > now).
                // A simple contains("open") matches the field NAME "open_until_ts" even when
                // the circuit is closed (value=0), causing false positives.
                let is_open = serde_json::from_str::<serde_json::Value>(&content)
                    .ok()
                    .map(|v| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        // Global catastrophic
                        let global_open = v
                            .pointer("/global/open_until_ts")
                            .and_then(|t| t.as_u64())
                            .is_some_and(|t| t > now);
                        let global_cat = v
                            .pointer("/global/catastrophic_count")
                            .and_then(|c| c.as_u64())
                            .is_some_and(|c| c >= 3);
                        // Any project open
                        let project_open = v
                            .get("by_project")
                            .and_then(|p| p.as_object())
                            .is_some_and(|m| {
                                m.values().any(|pv| {
                                    pv.get("open_until_ts")
                                        .and_then(|t| t.as_u64())
                                        .is_some_and(|t| t > now)
                                })
                            });
                        (global_open && global_cat) || project_open
                    })
                    .unwrap_or(false);
                Check {
                    name: "circuit_breaker",
                    status: if is_open { "open" } else { "ok" },
                    detail: content.trim().to_string(),
                }
            }
            Err(e) => Check {
                name: "circuit_breaker",
                status: "unknown",
                detail: format!("read error: {e}"),
            },
        }
    } else {
        Check {
            name: "circuit_breaker",
            status: "ok",
            detail: "no state file (circuit closed)".to_string(),
        }
    }
}

fn check_project_db() -> Check {
    let cwd = std::env::current_dir().unwrap_or_default();
    let db_path = cwd.join(".claude/touring/symbols.db");

    if db_path.exists() {
        match std::fs::metadata(&db_path) {
            Ok(meta) => Check {
                name: "project_db",
                status: "ok",
                detail: format!("{} ({} bytes)", db_path.display(), meta.len()),
            },
            Err(e) => Check {
                name: "project_db",
                status: "error",
                detail: format!("{e}"),
            },
        }
    } else {
        Check {
            name: "project_db",
            status: "missing",
            detail: format!("{} not found", db_path.display()),
        }
    }
}

/// Wiring-map row census — surfaces pollution that distorts `orphan_count`.
///
/// Reads knowledge.db directly (read-only) so the diagnostic works even
/// when the daemon is unhealthy. Status:
/// - `ok`: clean (no non-Rust rows, no kind_unknown rows, no abs paths)
/// - `warning`: pollution detected; orphan_count is biased — operators should
///   run `touring index rebuild` to repopulate the wiring_map with the new
///   eligibility gate. The detail string surfaces which class of pollution.
/// - `missing`: knowledge.db not present (project not yet indexed).
/// - `error`: DB present but query failed (e.g., schema mismatch).
fn check_wiring_diagnostic() -> Check {
    let cwd = std::env::current_dir().unwrap_or_default();
    let db_path = cwd.join(".claude/touring/knowledge.db");
    if !db_path.exists() {
        return Check {
            name: "wiring_diagnostic",
            status: "missing",
            detail: format!("{} not found", db_path.display()),
        };
    }
    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(e) => {
            return Check {
                name: "wiring_diagnostic",
                status: "error",
                detail: format!("open: {e}"),
            };
        }
    };
    let row = conn.query_row(
        "SELECT
            COUNT(*),
            SUM(CASE WHEN consumer_file IS NULL THEN 1 ELSE 0 END),
            SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END),
            SUM(CASE WHEN consumer_file IS NULL AND visibility = 'public' THEN 1 ELSE 0 END),
            SUM(CASE WHEN symbol_kind = 'unknown' THEN 1 ELSE 0 END),
            SUM(CASE WHEN module_file NOT LIKE '%.rs' THEN 1 ELSE 0 END),
            SUM(CASE WHEN module_file LIKE '/home/%' THEN 1 ELSE 0 END)
         FROM wiring_map",
        [],
        |r| {
            Ok((
                r.get::<_, i64>(0).unwrap_or(0),
                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                r.get::<_, Option<i64>>(6)?.unwrap_or(0),
            ))
        },
    );
    match row {
        Ok((total, producers, consumers, pub_prod, kind_unknown, non_rust, abs_paths)) => {
            let polluted = non_rust > 0 || kind_unknown > 0 || abs_paths > 0;
            Check {
                name: "wiring_diagnostic",
                status: if polluted { "warning" } else { "ok" },
                detail: format!(
                    "rows={total} producers={producers} consumers={consumers} pub={pub_prod} kind_unknown={kind_unknown} non_rust={non_rust} abs_paths={abs_paths}"
                ),
            }
        }
        Err(e) => Check {
            name: "wiring_diagnostic",
            status: "error",
            detail: format!("query: {e}"),
        },
    }
}

fn check_daemon_health() -> Check {
    match super::daemon_query("__health__", serde_json::json!({})) {
        Ok(output) => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&output) {
                let status_str = val
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let projects = val
                    .get("projects_loaded")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                Check {
                    name: "daemon_health",
                    status: if status_str == "healthy" {
                        "ok"
                    } else {
                        "degraded"
                    },
                    detail: format!("status={status_str}, projects={projects}"),
                }
            } else {
                Check {
                    name: "daemon_health",
                    status: "ok",
                    detail: output,
                }
            }
        }
        Err(e) => Check {
            name: "daemon_health",
            status: "error",
            detail: format!("{e}"),
        },
    }
}

fn check_binary_version() -> Check {
    Check {
        name: "binary_version",
        status: "ok",
        detail: format!("touring {}", env!("CARGO_PKG_VERSION")),
    }
}

/// Entry point for the `touring doctor` CLI handler — runs the diagnostic
/// suite (binary version, daemon socket, daemon health, circuit breaker,
/// project DB, wiring-map pollution census) and reports the results either as
/// pretty JSON (with `-j`/`--json`) or a human-readable check list to stderr.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let (flags, _filtered) = parse_global_flags(args);

    let checks = vec![
        check_binary_version(),
        check_daemon_socket(),
        check_daemon_health(),
        check_circuit_breaker(),
        check_project_db(),
        check_wiring_diagnostic(),
    ];

    if flags.json {
        let output = serde_json::to_string_pretty(&checks)?;
        json_to_stdout(&output);
    } else {
        let all_ok = checks.iter().all(|c| c.status == "ok");
        for check in &checks {
            let icon = match check.status {
                "ok" => "ok",
                "missing" | "open" => "WARN",
                _ => "FAIL",
            };
            human_to_stderr(&format!(
                "  [{icon:>4}] {:<20} {}",
                check.name, check.detail
            ));
        }
        human_to_stderr("");
        if all_ok {
            human_to_stderr("All checks passed.");
        } else {
            human_to_stderr("Some checks failed. Run with -j for machine-readable output.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_binary_version_is_ok() {
        let c = check_binary_version();
        assert_eq!(c.status, "ok");
        assert!(c.detail.starts_with("touring "));
    }

    #[test]
    fn check_circuit_breaker_no_state_file() {
        // In test environment, state file likely doesn't exist
        let c = check_circuit_breaker();
        // Either "ok" (no file) or some state — both valid
        assert!(!c.detail.is_empty());
    }

    #[test]
    fn check_project_db_reports_status() {
        let c = check_project_db();
        assert!(!c.detail.is_empty());
        assert!(c.status == "ok" || c.status == "missing");
    }
}
