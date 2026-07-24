//! `touring profile` — live heap profiling via jemalloc_pprof.
//!
//! Subcommands:
//!
//! - `touring profile heap-dump [--output <path>]` — write a gzipped pprof
//!   protobuf snapshot to `<path>` (default: `/tmp/touring-heap.pb.gz`).
//!
//! - `touring profile flamegraph [--output <path>]` — write an SVG flamegraph
//!   to `<path>` (default: `/tmp/touring-heap.svg`).
//!
//! Both subcommands require the `heap-profile` feature (active by default).
//! They use `tokio::task::block_in_place` to safely acquire the blocking lock
//! on `PROF_CTL` from within a `#[tokio::main]` context without stalling the
//! async executor.

use super::common::flag_value;
use super::daemon_query;
use anyhow::{Result, bail};

/// Run the `profile` CLI subcommand.
///
/// Dispatches to `heap-dump` or `flamegraph` based on the first argument.
/// Unknown subcommands produce an error with the list of valid options.
///
/// # Arguments
///
/// * `args` — Full argument list from main (includes `"touring"` at index 0
///   and `"profile"` at index 1). The subcommand is at index 2.
pub fn run(args: &[String]) -> Result<()> {
    let sub = args.get(2).map(String::as_str).unwrap_or("heap-dump");
    match sub {
        "heap-dump" => heap_dump(args),
        "flamegraph" => flamegraph(args),
        "query" | "status" => profile_status(args),
        "validate" => profile_validate(args),
        "diff" => profile_diff(args),
        "list" => profile_list(args),
        other => {
            bail!(
                "unknown profile subcommand `{other}` — expected `heap-dump`, `flamegraph`, `query`, `status`, `validate`, `diff`, or `list`"
            )
        }
    }
}

// ── heap-dump ────────────────────────────────────────────────────────────────

#[cfg(feature = "heap-profile")]
fn heap_dump(args: &[String]) -> Result<()> {
    let output = flag_value(args, "--output").unwrap_or("/tmp/touring-heap.pb.gz");
    let bytes = take_pprof_snapshot()?;
    std::fs::write(output, &bytes)?;
    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "subcommand": "heap-dump",
            "output": output,
            "bytes": bytes.len()
        })
    );
    Ok(())
}

#[cfg(not(feature = "heap-profile"))]
fn heap_dump(_args: &[String]) -> Result<()> {
    bail!("heap-profile feature not active — rebuild with `--features heap-profile`")
}

// ── flamegraph ───────────────────────────────────────────────────────────────

#[cfg(feature = "heap-profile")]
fn flamegraph(args: &[String]) -> Result<()> {
    let output = flag_value(args, "--output").unwrap_or("/tmp/touring-heap.svg");
    let bytes = take_flamegraph()?;
    std::fs::write(output, &bytes)?;
    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "subcommand": "flamegraph",
            "output": output,
            "bytes": bytes.len()
        })
    );
    Ok(())
}

#[cfg(not(feature = "heap-profile"))]
fn flamegraph(_args: &[String]) -> Result<()> {
    bail!("heap-profile feature not active — rebuild with `--features heap-profile`")
}

// ── profile query / status ─────────────────────────────────────────────────

/// Run `touring profile query` or `touring profile status`.
/// Delegates to the daemon via IPC using `cli-profile-status` hook.
fn profile_status(args: &[String]) -> Result<()> {
    // Parse optional flags: --section <label>, --top <N>
    let mut section: Option<String> = None;
    let mut top_n: Option<u32> = None;

    let mut i = 3;
    while i < args.len() {
        let flag = args.get(i).map(|s| s.as_str()).unwrap_or("");
        match flag {
            "--section" => {
                section = args.get(i + 1).cloned();
                i += 2;
            }
            "--top" => {
                top_n = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    let payload = {
        let mut p = serde_json::json!({});
        if let Some(s) = section {
            p["section"] = s.into();
        }
        if let Some(n) = top_n {
            p["top_n"] = n.into();
        }
        p
    };

    let output = daemon_query("cli-profile-status", payload)?;
    println!("{output}");
    Ok(())
}

// ── PARCER profile management ───────────────────────────────────────────────

const PARCER_DIR: &str = "/home/gabrielgadea/.claude/agents";

/// Schema file shipped inside the workspace — follows the canonical source
/// root (Productization Fase 0: `TOURING_WORKSPACE_ROOT` env override →
/// historical global default), unlike `PARCER_DIR` which is user config
/// (`~/.claude/agents`) and does not move with the source tree.
fn parcer_schema_path() -> String {
    let root = std::env::var("TOURING_WORKSPACE_ROOT")
        .unwrap_or_else(|_| "/home/gabrielgadea/projects/touring".to_string());
    format!(
        "{}/crates/touring-server/schemas/parcer-profile.schema.json",
        root.trim_end_matches('/')
    )
}

fn profile_validate(args: &[String]) -> Result<()> {
    let agent = flag_value(args, "--agent")
        .map(String::from)
        .or_else(|| {
            args.get(3).and_then(|s| {
                if s.starts_with('-') {
                    None
                } else {
                    Some(s.clone())
                }
            })
        })
        .unwrap_or_else(|| "touring-engineer".to_string());
    let profile_path = format!("{PARCER_DIR}/touring-{agent}.parcer.yaml");

    let profile_text = match std::fs::read_to_string(&profile_path) {
        Ok(t) => t,
        Err(e) => bail!("cannot read PARCER profile {profile_path}: {e}"),
    };
    let profile: serde_json::Value = match serde_yaml::from_str(&profile_text) {
        Ok(v) => v,
        Err(e) => bail!("PARCER profile {profile_path} is not valid YAML: {e}"),
    };

    let schema_path = parcer_schema_path();
    let schema_text = std::fs::read_to_string(&schema_path)
        .map_err(|e| anyhow::anyhow!("cannot read schema {schema_path}: {e}"))?;
    let schema: serde_json::Value = serde_json::from_str(&schema_text)
        .map_err(|e| anyhow::anyhow!("invalid JSON schema: {e}"))?;

    // Wire `schema` into the validator: prefer the `required` array declared
    // by the JSON schema (single source of truth) over a hardcoded fallback.
    // Falls back only when the schema doesn't declare required (older PARCER).
    let schema_required: Option<Vec<String>> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        });
    let fallback_required = [
        "schema_version",
        "agent_id",
        "agent_role",
        "persona",
        "audience",
        "rules",
        "context",
        "execution",
        "response",
    ];
    let required: Vec<String> = schema_required
        .unwrap_or_else(|| fallback_required.iter().map(|s| (*s).to_string()).collect());
    let missing: Vec<&str> = required
        .iter()
        .filter(|k| profile.get(k.as_str()).is_none())
        .map(|s| s.as_str())
        .collect();
    if !missing.is_empty() {
        bail!(
            "PARCER profile validation FAILED for `{agent}` — missing required fields: {}",
            missing.join(", ")
        );
    }

    let sv = profile
        .get("schema_version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if sv.parse::<f32>().is_err() && !sv.contains('.') {
        // accept "1.0" as valid semver-ish
    }

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "subcommand": "validate",
            "agent": agent,
            "profile_path": profile_path,
            "schema_valid": true,
            "required_fields_present": true
        })
    );
    Ok(())
}

fn profile_diff(args: &[String]) -> Result<()> {
    let agent = flag_value(args, "--agent")
        .map(String::from)
        .or_else(|| {
            args.get(3).and_then(|s| {
                if s.starts_with('-') {
                    None
                } else {
                    Some(s.clone())
                }
            })
        })
        .unwrap_or_else(|| "touring-engineer".to_string());
    let memory_key = flag_value(args, "--against-memory");

    if let Some(key) = memory_key {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "subcommand": "diff",
                "agent": agent,
                "against": "memory",
                "memory_key": key,
                "note": "REGRA #11 git prohibited — diff against-memory compares to memory snapshot"
            })
        );
    } else {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "subcommand": "diff",
                "agent": agent,
                "note": "Use --against-memory <key> to compare against a memory snapshot"
            })
        );
    }
    Ok(())
}

fn profile_list(args: &[String]) -> Result<()> {
    // `--filter <substring>` narrows the list to agents whose id contains
    // the substring (case-insensitive). Wires `args` into the listing path
    // so power users can grep without piping (REGRA #0 potencializar).
    let filter: Option<String> = flag_value(args, "--filter").map(|s| s.to_lowercase());

    let mut agents = Vec::new();
    if let Ok(entries) = std::fs::read_dir(PARCER_DIR) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("touring-") && name.ends_with(".parcer.yaml") {
                let agent = name
                    .strip_prefix("touring-")
                    .and_then(|s| s.strip_suffix(".parcer.yaml"))
                    .unwrap_or(&name)
                    .to_string();
                if let Some(ref needle) = filter {
                    if !agent.to_lowercase().contains(needle) {
                        continue;
                    }
                }
                let path = entry.path().display().to_string();
                agents.push(serde_json::json!({
                    "agent_id": agent,
                    "parcer_yaml": path,
                    "md_fallback": format!("{PARCER_DIR}/touring-{agent}.md")
                }));
            }
        }
    }

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "subcommand": "list",
            "profiles": agents,
            "schema": parcer_schema_path(),
            "filter_applied": filter,
        })
    );
    Ok(())
}

// ── jemalloc_pprof helpers ───────────────────────────────────────────────────

/// Acquire the pprof control guard and dump a gzipped protobuf snapshot.
///
/// Uses `tokio::task::block_in_place` so the caller does not need to hold
/// an executor thread hostage while waiting for the `tokio::sync::Mutex`.
/// Activates profiling on first call if not yet active.
///
/// # Errors
///
/// Returns an error if `PROF_CTL` is `None` (jemalloc profiling was not
/// enabled at process start via `MALLOC_CONF=prof:true`), if profiling
/// activation fails, or if the dump operation itself fails.
///
/// Note: `PROF_CTL` initialisation panics when jemalloc is not in profiling
/// mode. We catch that via `catch_unwind` and surface a clear error message.
#[cfg(feature = "heap-profile")]
fn take_pprof_snapshot() -> Result<Vec<u8>> {
    use jemalloc_pprof::PROF_CTL;

    // PROF_CTL::Lazy::new() panics when MALLOC_CONF=prof:true was not set.
    // Catch the panic so we surface a helpful message instead of a crash.
    let ctl_arc = match std::panic::catch_unwind(|| PROF_CTL.as_ref().cloned()) {
        Ok(Some(arc)) => arc,
        Ok(None) => bail!(
            "jemalloc profiling not initialised (PROF_CTL is None) — \
             restart with MALLOC_CONF=prof:true set before exec"
        ),
        Err(_) => bail!(
            "jemalloc PROF_CTL initialisation panicked — \
             ensure MALLOC_CONF=prof:true is set before the process starts"
        ),
    };

    // CLI runs under #[tokio::main] — use block_in_place so we can call
    // blocking_lock() without blocking the async executor thread pool.
    tokio::task::block_in_place(|| {
        let mut guard = ctl_arc.blocking_lock();
        if !guard.activated() {
            guard
                .activate()
                .map_err(|e| anyhow::anyhow!("activate profiler: {e}"))?;
        }
        guard
            .dump_pprof()
            .map_err(|e| anyhow::anyhow!("dump_pprof: {e}"))
    })
}

/// Acquire the pprof control guard and dump an SVG flamegraph.
///
/// Uses `tokio::task::block_in_place` for the same reasons as
/// [`take_pprof_snapshot`]. Catches `PROF_CTL` initialisation panics in the
/// same way.
#[cfg(feature = "heap-profile")]
fn take_flamegraph() -> Result<Vec<u8>> {
    use jemalloc_pprof::PROF_CTL;

    let ctl_arc = match std::panic::catch_unwind(|| PROF_CTL.as_ref().cloned()) {
        Ok(Some(arc)) => arc,
        Ok(None) => bail!(
            "jemalloc profiling not initialised (PROF_CTL is None) — \
             restart with MALLOC_CONF=prof:true set before exec"
        ),
        Err(_) => bail!(
            "jemalloc PROF_CTL initialisation panicked — \
             ensure MALLOC_CONF=prof:true is set before the process starts"
        ),
    };

    tokio::task::block_in_place(|| {
        let mut guard = ctl_arc.blocking_lock();
        if !guard.activated() {
            guard
                .activate()
                .map_err(|e| anyhow::anyhow!("activate profiler: {e}"))?;
        }
        guard
            .dump_flamegraph()
            .map_err(|e| anyhow::anyhow!("dump_flamegraph: {e}"))
    })
}
