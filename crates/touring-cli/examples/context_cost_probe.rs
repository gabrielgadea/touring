//! B5/B3 probe — run the `context_cost` and `surface_usage` KPI sections over REAL transcripts
//! without the daemon (the live daemon embeds the CLI of its last release).
//!
//! The KPI itself reads the newest transcripts, which are mostly tiny headless runs. To judge the
//! instrument on sessions that matter, this probe draws a seeded sample of main sessions, links
//! them into a throwaway `HOME` and hands that to the same function the KPI calls — so what is
//! printed is exactly what the KPI would print for those sessions.
//!
//! Usage: `cargo run -p touring-cli --example context_cost_probe -- [--sessions 10] [--seed N]
//! [--project-root DIR]`
//!
//! Output is counts, names of skills/agents/tools and numbers — never transcript text.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use touring_cli::cli::context_budget::transcript_dir_for;
use touring_cli::cli::context_cost::context_cost_and_surface_in;
use touring_cli::cli::transcript_scan::ScanCaps;

/// Smallest transcript considered a main session (tiny ones are headless side runs). 512 KiB
/// leaves ten such sessions in this project, the sample the B5 acceptance asks for.
const MIN_SESSION_BYTES: u64 = 512 * 1024;

struct Args {
    sessions: usize,
    seed: u64,
    project_root: PathBuf,
}

fn parse_args() -> Args {
    let mut args = Args {
        sessions: 10,
        seed: 20_261_101,
        project_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().unwrap_or_default();
        match flag.as_str() {
            "--sessions" => args.sessions = value.parse().unwrap_or(args.sessions),
            "--seed" => args.seed = value.parse().unwrap_or(args.seed),
            "--project-root" => args.project_root = PathBuf::from(value),
            other => eprintln!("ignored flag {other}"),
        }
    }
    args
}

/// xorshift64* — a seeded order that needs no dependency and is the same on every run.
fn shuffled(mut files: Vec<PathBuf>, seed: u64) -> Vec<PathBuf> {
    files.sort();
    let mut state = seed | 1;
    for i in (1..files.len()).rev() {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let r = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        files.swap(i, (r % (i as u64 + 1)) as usize);
    }
    files
}

/// Main sessions of the project that ran on a Claude model.
fn candidates(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() >= MIN_SESSION_BYTES))
        .filter(|p| {
            std::fs::read_to_string(p).is_ok_and(|s| s.contains("\"model\":\"claude-"))
        })
        .collect()
}

fn pick<'a>(v: &'a Value, path: &str) -> &'a Value {
    v.pointer(path).unwrap_or(&Value::Null)
}

fn session_row(row: &Value) -> Value {
    json!({
        "session": row["session_id"].as_str().map(|s| s.chars().take(8).collect::<String>()),
        "calls": row["calls"],
        "input_total_tokens": pick(row, "/tokens/input_total"),
        "reconciliation_error": row["reconciliation_error"],
        "unseen_share": pick(row, "/reconciliation/unseen_share"),
        "compactions": row["compactions"],
        "usd": pick(row, "/cost/usd"),
        "unpriced_models": pick(row, "/cost/unpriced_models"),
        "cost_state_total_usd": pick(row, "/cost_state/total_cost_usd"),
        "cost_state_input_ratio": pick(row, "/cost_state/input_total_ratio"),
        "usd_ratio_by_priced_model": priced_ratios(row),
    })
}

/// Per model that is in the price table AND in `cost-state`: transcript dollars over its `costUSD`.
fn priced_ratios(row: &Value) -> Value {
    let by_model = row
        .pointer("/cost_state/by_model")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(_, m)| !m["usd_ratio"].is_null())
        .map(|(model, m)| (model.clone(), m["usd_ratio"].clone()))
        .collect::<serde_json::Map<String, Value>>();
    Value::Object(by_model)
}

fn brief(cost: &Value, surface: &Value) -> Value {
    let kind = |k: &str| {
        let r = &surface[k];
        let top: Vec<&Value> = r["top_unused"]
            .as_array()
            .map(|a| a.iter().take(5).map(|i| &i["name"]).collect())
            .unwrap_or_default();
        json!({
            "announced": r["announced"], "used": r["used"], "unused": r["unused"],
            "used_not_announced": r["used_not_announced"], "unused_share": r["unused_share"],
            "announce_tokens_est_per_session": r["announce_tokens_est_per_session"],
            "uses": r["uses"], "top5_unused_by_bytes": top,
        })
    };
    let a = &cost["attribution"];
    json!({
        "context_cost": {
            "sessions": cost["sessions_scanned"], "calls": cost["calls"],
            "tokens": cost["tokens"],
            "attribution_tokens": {
                "injected_hooks": pick(a, "/injected_hooks/tokens"),
                "announcement": pick(a, "/announcement/tokens"),
                "tool_results": pick(a, "/tool_results/tokens"),
                "rest": pick(a, "/rest/tokens"),
                "rest_parts": pick(a, "/rest/parts"),
            },
            "shares": {
                "injected_hooks": pick(a, "/injected_hooks/share"),
                "announcement": pick(a, "/announcement/share"),
                "tool_results": pick(a, "/tool_results/share"),
                "rest": pick(a, "/rest/share"),
            },
            "announcement_by_kind": pick(a, "/announcement/by_kind"),
            "reconciliation": cost["reconciliation"],
            "cost": cost["cost"],
            "by_session": cost["by_session"].as_array().map(|rows| rows.iter().map(session_row).collect::<Vec<_>>()),
        },
        "surface_usage": {
            "sessions": surface["sessions"],
            "skills": kind("skills"), "agents": kind("agents"), "tools": kind("tools"),
            "mcp_instructions": surface["mcp_instructions"],
        },
    })
}

fn main() {
    let args = parse_args();
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        eprintln!("HOME unset");
        std::process::exit(1);
    };
    let real = transcript_dir_for(&home, &args.project_root);
    let chosen: Vec<PathBuf> = shuffled(candidates(&real), args.seed)
        .into_iter()
        .take(args.sessions)
        .collect();
    let sandbox = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("tempdir: {e}");
            std::process::exit(1);
        }
    };
    let linked = transcript_dir_for(sandbox.path(), &args.project_root);
    if let Err(e) = std::fs::create_dir_all(&linked) {
        eprintln!("mkdir: {e}");
        std::process::exit(1);
    }
    for path in &chosen {
        if let Some(name) = path.file_name()
            && let Err(e) = std::os::unix::fs::symlink(path, linked.join(name))
        {
            eprintln!("symlink: {e}");
            std::process::exit(1);
        }
    }
    let caps = ScanCaps {
        max_transcripts: args.sessions,
        max_bytes: u64::MAX,
    };
    let (cost, surface) = context_cost_and_surface_in(sandbox.path(), &args.project_root, &caps);
    let out = json!({
        "seed": args.seed,
        "requested_sessions": args.sessions,
        "eligible_sessions": candidates(&real).len(),
        "report": brief(&cost, &surface),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
}
