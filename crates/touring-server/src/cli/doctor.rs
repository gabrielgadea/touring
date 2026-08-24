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
/// when the daemon is unhealthy.
///
/// # What counts as pollution
///
/// The question is NOT "is this Rust?" but "is this a source file the wiring
/// graph can resolve?". Measured across four projects on 2026-08-19, asking
/// the first errs in BOTH directions at once:
///
/// - **false positive** — `analise` is a Python project; 192.997 rows earned a
///   permanent warning for the offence of not ending in `.rs`.
/// - **false negative** — `touring` itself carries 102 rows of
///   `benches/src/*.rs`. They end in `.rs`, so the flagship project read
///   `ok` — while the write gate rejects `benches/` and the read filter,
///   being extension-only, lets them inflate the orphan count. Exactly the
///   unreliability the warning exists to signal, invisible to it.
///
/// So three counters, and only ONE of them judges:
///
/// | counter | meaning | effect |
/// |---|---|---|
/// | `non_wireable` | inadmissible under the MAXIMUM vocabulary (`polyglot=true`): vendored trees, `docs/`, `scripts/`, `tests/`, `benches/`, test files, extensions outside the vocabulary | **warning** |
/// | `unread` | a supported-language source the CURRENT mode does not read | informative — the answer to "why is my Python project's wiring thin?" |
/// | census | `rows`/`producers`/`consumers`/`pub` over the set the answers actually use | describes what is read |
///
/// `non_wireable` is mode-INDEPENDENT by design: a `.json` or a virtualenv is
/// not wiring under any read. `unread` depends on the mode, and is information,
/// not a defect. The classification calls [`is_wireable_source`] — the writer's
/// own vocabulary — so the diagnostic cannot disagree with the thing it
/// diagnoses, the same principle that put `root=` here in 30.4.6.
///
/// Status:
/// - `ok`: clean (nothing non-wireable, no kind_unknown rows, no abs paths)
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
    // GROUP BY module_file, not a flat SUM: the classification below is Rust
    // logic (the writer's own predicate), which SQL cannot express. Grouping
    // makes the cost O(distinct files) rather than O(rows) — 1.832 vs 24.026 in
    // `analise` — and each group carries its own census contribution.
    let mut stmt = match conn.prepare(
        "SELECT
            module_file,
            COUNT(*),
            SUM(CASE WHEN consumer_file IS NULL THEN 1 ELSE 0 END),
            SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END),
            SUM(CASE WHEN consumer_file IS NULL AND visibility = 'public' THEN 1 ELSE 0 END),
            SUM(CASE WHEN symbol_kind = 'unknown' THEN 1 ELSE 0 END)
         FROM wiring_map
         GROUP BY module_file",
    ) {
        Ok(s) => s,
        Err(e) => {
            return Check {
                name: "wiring_diagnostic",
                status: "error",
                detail: format!("prepare: {e}"),
            };
        }
    };
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?.unwrap_or_default(),
            r.get::<_, i64>(1).unwrap_or(0),
            r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            r.get::<_, Option<i64>>(4)?.unwrap_or(0),
            r.get::<_, Option<i64>>(5)?.unwrap_or(0),
        ))
    });
    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            return Check {
                name: "wiring_diagnostic",
                status: "error",
                detail: format!("query: {e}"),
            };
        }
    };

    let root = touring_foundation::config::TouringConfig::project_root_for_db(&db_path);
    // The mode decides WHICH rows any wiring query can see, so the census is
    // unreadable without it: the same 200k rows mean "a wired Python project"
    // under `on` and "dead weight" under `off`.
    let polyglot = touring_foundation::config::TouringConfig::polyglot_wiring_for_root(
        root.as_deref().map(std::path::Path::new),
    );

    let (mut total, mut producers, mut consumers, mut pub_prod) = (0i64, 0i64, 0i64, 0i64);
    let (mut kind_unknown, mut non_wireable, mut unread, mut abs_paths) = (0i64, 0i64, 0i64, 0i64);
    for row in rows {
        let Ok((module_file, n, prod, cons, pubp, unknown)) = row else {
            continue;
        };
        // Absolute means absolute: this counted `/home/%` until 2026-08-19, so
        // a polluted row under `/tmp`, `/opt` or `/Users` read as clean and the
        // diagnostic under-reported exactly where the developer's own layout
        // differed from the machine being diagnosed.
        if module_file.starts_with('/') {
            abs_paths += n;
        }
        if !touring_storage::knowledge_wiring::is_wireable_source(&module_file, true) {
            // Judged: no read admits this file, in any mode.
            non_wireable += n;
            continue;
        }
        if !touring_storage::knowledge_wiring::is_wireable_source(&module_file, polyglot) {
            // Merely unread: a supported-language source this mode filters out.
            unread += n;
            continue;
        }
        // The census covers ONLY what the answers use.
        total += n;
        producers += prod;
        consumers += cons;
        pub_prod += pubp;
        kind_unknown += unknown;
    }

    let polluted = non_wireable > 0 || kind_unknown > 0 || abs_paths > 0;
    Check {
        name: "wiring_diagnostic",
        status: if polluted { "warning" } else { "ok" },
        // `root` first: it is the field whose ABSENCE hid a defect for two
        // months. The wiring paths of one project were being canonicalized
        // against ANOTHER project's root (inherited env), and every number
        // below is relative to that root — a census of rows means nothing until
        // you know what the rows are relative to. Same function that derives it
        // for the writer, so the diagnostic cannot disagree with the thing it
        // diagnoses.
        detail: format!(
            "root={} polyglot={} rows={total} producers={producers} consumers={consumers} pub={pub_prod} kind_unknown={kind_unknown} non_wireable={non_wireable} unread={unread} abs_paths={abs_paths}",
            root.unwrap_or_else(|| "<não derivável>".to_string()),
            if polyglot { "on" } else { "off" },
        ),
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
/// Probe the PROJECT ACTOR, not just the daemon process.
///
/// `check_daemon_health` asks the daemon's `__health__` handler, which does not
/// pass through the per-project actor queue. On 2026-08-20 a single
/// `touring mutation-test` (a 19-minute job dispatched to a handler with a 15s
/// budget) serialized that queue: `status`, `index status`, `memory recall` and
/// `e2e` all timed out for the whole run while `doctor` reported 6/6 ok. The
/// handlers that died are exactly the ones `loop_diagnose.py` and
/// `loop_converged.py` consume — a loop would have read "healthy" and then
/// failed every clause on timeout, with nothing explaining why.
///
/// A health gate that cannot see the queue it depends on is not a health gate.
fn check_project_actor() -> Check {
    const SLOW_MS: u128 = 5_000;

    let started = std::time::Instant::now();
    let outcome = super::daemon_query("cli-index-status", serde_json::json!({}));
    let elapsed_ms = started.elapsed().as_millis();

    match outcome {
        Ok(_) if elapsed_ms > SLOW_MS => Check {
            name: "project_actor",
            status: "degraded",
            detail: format!(
                "responsive but slow ({elapsed_ms} ms) — a long-running handler \
                 may be serializing the queue"
            ),
        },
        Ok(_) => Check {
            name: "project_actor",
            status: "ok",
            detail: format!("responsive ({elapsed_ms} ms)"),
        },
        Err(e) => Check {
            name: "project_actor",
            status: "error",
            detail: format!(
                "no answer after {elapsed_ms} ms: {e} — the actor is likely busy \
                 with an earlier heavy op (index rebuild, mutation-test); \
                 project-scoped commands will time out until it drains"
            ),
        },
    }
}

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
        check_project_actor(),
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
