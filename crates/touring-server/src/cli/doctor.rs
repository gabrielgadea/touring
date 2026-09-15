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
    let source_packages = touring_storage::knowledge_wiring::python_source_packages(
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
        if !touring_storage::knowledge_wiring::is_wireable_source(
            &module_file,
            true,
            &source_packages,
        ) {
            // Judged: no read admits this file, in any mode.
            non_wireable += n;
            continue;
        }
        if !touring_storage::knowledge_wiring::is_wireable_source(
            &module_file,
            polyglot,
            &source_packages,
        ) {
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

/// I16 (2026-09-13): the seal on the latest rebuild, read straight from
/// `knowledge.db` (read-only, so it works with the daemon down). `partial` is a
/// FAIL with the remedy in the detail: a rebuild interrupted by a kill left the
/// index a mix of two walks, and until this check nothing said so (measured:
/// `kill -9` at t+3 s, wiring_map 85 rows short, integrity ok, doctor 7/7).
fn check_index_generation() -> Check {
    let cwd = std::env::current_dir().unwrap_or_default();
    index_generation_check_at(&cwd.join(".claude/touring/knowledge.db"))
}

/// [`check_index_generation`] over an explicit database path — the testable half.
fn index_generation_check_at(db_path: &std::path::Path) -> Check {
    if !db_path.exists() {
        return Check {
            name: "index_generation",
            status: "missing",
            detail: format!("{} not found", db_path.display()),
        };
    }
    let conn = match rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(e) => {
            return Check {
                name: "index_generation",
                status: "error",
                detail: format!("open: {e}"),
            };
        }
    };
    match touring_storage::knowledge_index_generation::read_index_generation_state(&conn, None) {
        Ok(state) => {
            let status = match state.state {
                "complete" | "none" => "ok",
                "building" => "building",
                _ => "partial",
            };
            Check {
                name: "index_generation",
                status,
                detail: state.detail,
            }
        }
        Err(e) => Check {
            name: "index_generation",
            status: "error",
            detail: format!("read: {e}"),
        },
    }
}

/// Libraries `libonnxruntime_providers_cuda.so` needs (`readelf -d` on the ORT
/// 1.24.2 cu13 build, 15/09/2026). They follow `ORT_CUDA_VERSION` in
/// `.cargo/config.toml`: a CUDA major bump changes this list.
const CUDA_PROVIDER_SONAMES: &[&str] = &[
    "libcublasLt.so.13",
    "libcublas.so.13",
    "libcurand.so.10",
    "libcufft.so.12",
    "libcudart.so.13",
    "libcudnn.so.9",
];

/// The daemon's own view of its embedder, read from `touring gate-metrics`.
struct DaemonEmbedding {
    device: String,
    cuda_texts: u64,
    fallbacks: u64,
    fallback_reason: String,
}

/// Everything the GPU-embeddings verdict depends on, gathered apart so the
/// verdict itself is a pure function.
struct GpuEmbeddingsFacts {
    /// `TOURING_EMBED_DEVICE` as this process sees it, unparsed.
    policy_raw: Option<String>,
    nvidia_present: bool,
    /// `None` when the loader cache could not be read.
    missing_cuda_libs: Option<Vec<&'static str>>,
    /// `None` when the daemon did not answer.
    daemon: Option<DaemonEmbedding>,
}

/// Sonames from `required` that neither the `ldconfig -p` cache nor any
/// `LD_LIBRARY_PATH` directory provides. Cache lines are `\t<soname> (…) => path`:
/// the leading TAB is part of the format.
fn missing_sonames(
    ldconfig_output: &str,
    extra_dirs: &[PathBuf],
    required: &[&'static str],
) -> Vec<&'static str> {
    required
        .iter()
        .copied()
        .filter(|soname| {
            let cached = ldconfig_output.lines().any(|line| {
                line.trim_start()
                    .strip_prefix(soname)
                    .is_some_and(|rest| rest.starts_with(' '))
            });
            !cached && !extra_dirs.iter().any(|d| d.join(soname).exists())
        })
        .collect()
}

/// The GPU-embeddings verdict. `degraded` whenever a GPU is there but the
/// embedder cannot or did not use it; the detail always names the remedy.
fn gpu_embeddings_verdict(facts: &GpuEmbeddingsFacts) -> Check {
    let check = |status: &'static str, detail: String| Check {
        name: "gpu_embeddings",
        status,
        detail,
    };
    // The executor's own parser decides what the variable means, so the check
    // can never read "cpu" where the embedder reads something else.
    use touring_storage::embeddings::{EMBED_DEVICE_ENV, EmbedDevicePolicy};
    match EmbedDevicePolicy::parse(facts.policy_raw.as_deref()) {
        Some(EmbedDevicePolicy::Cpu) => {
            return check("ok", format!("pinned to CPU by {EMBED_DEVICE_ENV}"));
        }
        None => {
            return check(
                "degraded",
                format!(
                    "{EMBED_DEVICE_ENV}={:?} is not one of auto|cuda|cpu — the embedder reads it as auto",
                    facts.policy_raw.as_deref().unwrap_or_default()
                ),
            );
        }
        Some(_) => {}
    }
    if !facts.nvidia_present {
        return check("ok", "no NVIDIA GPU (/dev/nvidiactl absent) — embeddings run on CPU".into());
    }
    if let Some(missing) = facts.missing_cuda_libs.as_ref().filter(|m| !m.is_empty()) {
        return check(
            "degraded",
            format!(
                "NVIDIA GPU present but the CUDA runtime is incomplete (missing {}) — install CUDA 13 + cuDNN 9 (Omarchy: `omarchy pkg add cuda cudnn`)",
                missing.join(", ")
            ),
        );
    }
    let Some(daemon) = &facts.daemon else {
        return check(
            "ok",
            "CUDA runtime present; daemon not reachable for the live device".into(),
        );
    };
    match (daemon.device.as_str(), daemon.fallbacks) {
        ("cuda", _) => check(
            "ok",
            format!("cuda — {} texts embedded on the GPU by this daemon", daemon.cuda_texts),
        ),
        (_, n) if n > 0 => {
            let remedy = if daemon.fallback_reason.contains("libonnxruntime_providers") {
                "the provider libraries are missing next to the daemon's launch path: run `update-touring` (dev channel) or `touring update` in the pinned project"
            } else {
                "see the daemon log for 'CUDA embedding runtime unavailable'"
            };
            check(
                "degraded",
                format!(
                    "daemon fell back to CPU {n}x: {} — {remedy}",
                    daemon.fallback_reason
                ),
            )
        }
        ("cpu", _) => check(
            "degraded",
            format!("daemon embeds on CPU without a CUDA failure — built without storage-emb-cuda, or {EMBED_DEVICE_ENV}=cpu in the daemon's environment"),
        ),
        _ => check(
            "ok",
            "CUDA runtime present; the daemon has not embedded yet (the device is chosen on the first recall/store)".into(),
        ),
    }
}

/// `touring doctor` GPU-embeddings check (15/09/2026): whether the daemon's
/// embedder can reach the NVIDIA GPU, and whether it actually does.
fn check_gpu_embeddings() -> Check {
    let extra_dirs: Vec<PathBuf> = std::env::var_os("LD_LIBRARY_PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    let missing_cuda_libs = std::process::Command::new("ldconfig")
        .arg("-p")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            missing_sonames(
                &String::from_utf8_lossy(&o.stdout),
                &extra_dirs,
                CUDA_PROVIDER_SONAMES,
            )
        });
    let daemon = super::daemon_query("cli-gate-metrics", serde_json::json!({}))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .map(|v| DaemonEmbedding {
            device: v["embedding_device"].as_str().unwrap_or("none").to_string(),
            cuda_texts: v["embedding_texts_cuda_count"].as_u64().unwrap_or(0),
            fallbacks: v["embedding_cuda_fallback_count"].as_u64().unwrap_or(0),
            fallback_reason: v["embedding_cuda_fallback_reason"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        });
    gpu_embeddings_verdict(&GpuEmbeddingsFacts {
        policy_raw: std::env::var(touring_storage::embeddings::EMBED_DEVICE_ENV).ok(),
        nvidia_present: std::path::Path::new("/dev/nvidiactl").exists(),
        missing_cuda_libs,
        daemon,
    })
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
        check_index_generation(),
        check_gpu_embeddings(),
    ];

    if flags.json {
        let output = serde_json::to_string_pretty(&checks)?;
        json_to_stdout(&output);
    } else {
        let all_ok = checks.iter().all(|c| c.status == "ok");
        for check in &checks {
            let icon = match check.status {
                "ok" => "ok",
                // `building`: a rebuild is running now — worth knowing, not a fault.
                "missing" | "open" | "building" => "WARN",
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

    fn facts(daemon: Option<DaemonEmbedding>) -> GpuEmbeddingsFacts {
        GpuEmbeddingsFacts {
            policy_raw: None,
            nvidia_present: true,
            missing_cuda_libs: Some(Vec::new()),
            daemon,
        }
    }

    fn daemon(device: &str, fallbacks: u64, reason: &str) -> Option<DaemonEmbedding> {
        Some(DaemonEmbedding {
            device: device.into(),
            cuda_texts: 42,
            fallbacks,
            fallback_reason: reason.into(),
        })
    }

    #[test]
    fn missing_sonames_reads_tab_prefixed_ldconfig_lines() {
        let cache = "2 libs found in cache `/etc/ld.so.cache'\n\
                     \tlibcudart.so.13 (libc6,x86-64) => /opt/cuda/lib64/libcudart.so.13\n\
                     \tlibcudnn.so.9 (libc6,x86-64) => /usr/lib/libcudnn.so.9\n";
        let missing = missing_sonames(cache, &[], &["libcudart.so.13", "libcudnn.so.9", "libcublas.so.13"]);
        assert_eq!(missing, vec!["libcublas.so.13"]);
    }

    #[test]
    fn missing_sonames_does_not_take_a_longer_soname_for_a_shorter_one() {
        let cache = "\tlibcublasLt.so.13 (libc6,x86-64) => /opt/cuda/lib64/libcublasLt.so.13\n";
        assert_eq!(missing_sonames(cache, &[], &["libcublas.so.13"]), vec!["libcublas.so.13"]);
    }

    #[test]
    fn missing_sonames_accepts_a_library_path_directory() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(dir.path().join("libcudnn.so.9"), b"").expect("write");
        assert!(missing_sonames("", &[dir.path().to_path_buf()], &["libcudnn.so.9"]).is_empty());
    }

    #[test]
    fn gpu_verdict_is_ok_without_an_nvidia_gpu_or_when_pinned_to_cpu() {
        let mut f = facts(None);
        f.nvidia_present = false;
        assert_eq!(gpu_embeddings_verdict(&f).status, "ok");
        let mut f = facts(daemon("cpu", 0, ""));
        f.policy_raw = Some("cpu".into());
        assert_eq!(gpu_embeddings_verdict(&f).status, "ok");
    }

    #[test]
    fn gpu_verdict_reads_the_policy_with_the_embedder_s_parser() {
        // The embedder accepts `CPU`; a check comparing the raw string to "cpu"
        // would call a correctly pinned daemon degraded.
        let mut f = facts(daemon("cpu", 0, ""));
        f.policy_raw = Some(" CPU ".into());
        assert_eq!(gpu_embeddings_verdict(&f).status, "ok");
    }

    #[test]
    fn gpu_verdict_flags_an_unrecognised_policy_value() {
        let mut f = facts(daemon("cuda", 0, ""));
        f.policy_raw = Some("rocm".into());
        let c = gpu_embeddings_verdict(&f);
        assert_eq!(c.status, "degraded");
        assert!(c.detail.contains("rocm") && c.detail.contains("auto"), "{}", c.detail);
    }

    #[test]
    fn gpu_verdict_names_missing_cuda_libs_and_the_install_command() {
        let mut f = facts(None);
        f.missing_cuda_libs = Some(vec!["libcudnn.so.9"]);
        let c = gpu_embeddings_verdict(&f);
        assert_eq!(c.status, "degraded");
        assert!(c.detail.contains("libcudnn.so.9") && c.detail.contains("omarchy pkg add cuda cudnn"), "{}", c.detail);
    }

    #[test]
    fn gpu_verdict_is_ok_when_the_daemon_embeds_on_cuda() {
        let c = gpu_embeddings_verdict(&facts(daemon("cuda", 0, "")));
        assert_eq!(c.status, "ok");
        assert!(c.detail.contains("42 texts"), "{}", c.detail);
    }

    #[test]
    fn gpu_verdict_turns_a_provider_lib_fallback_into_the_relink_remedy() {
        let reason = "cuda: Failed to load library /p/.touring/bin/libonnxruntime_providers_shared.so";
        let c = gpu_embeddings_verdict(&facts(daemon("cpu", 1, reason)));
        assert_eq!(c.status, "degraded");
        assert!(c.detail.contains(reason), "the reason travels verbatim: {}", c.detail);
        assert!(c.detail.contains("touring update"), "{}", c.detail);
    }

    #[test]
    fn gpu_verdict_flags_cpu_without_any_cuda_failure() {
        assert_eq!(gpu_embeddings_verdict(&facts(daemon("cpu", 0, ""))).status, "degraded");
    }

    #[test]
    fn gpu_verdict_is_ok_before_the_first_embedding() {
        assert_eq!(gpu_embeddings_verdict(&facts(daemon("none", 0, ""))).status, "ok");
        assert_eq!(gpu_embeddings_verdict(&facts(None)).status, "ok");
    }

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

    /// I16: a generation left `building` by a dead process reads `partial` with
    /// the remedy; a complete one reads `ok`; a database without the table reads
    /// `ok` too (an index that predates the seal is not a fault).
    #[test]
    fn index_generation_check_reads_the_seal_from_the_database() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("knowledge.db");
        assert_eq!(index_generation_check_at(&db_path).status, "missing");
        let db = touring_storage::knowledge::FileKnowledgeDB::new(&db_path).expect("open");
        assert_eq!(
            index_generation_check_at(&db_path).status,
            "ok",
            "no seal yet is not a fault"
        );
        // A pid no process carries (Linux pid_max tops out at 2^22).
        let stale = db.begin_index_generation(4_000_000_000).expect("begin");
        let c = index_generation_check_at(&db_path);
        assert_eq!(c.status, "partial", "{}", c.detail);
        assert!(c.detail.contains("touring index rebuild"), "{}", c.detail);
        db.finish_index_generation(stale, 1, 1, true, None)
            .expect("finish");
        assert_eq!(index_generation_check_at(&db_path).status, "ok");
    }

    #[test]
    fn check_project_db_reports_status() {
        let c = check_project_db();
        assert!(!c.detail.is_empty());
        assert!(c.status == "ok" || c.status == "missing");
    }
}
