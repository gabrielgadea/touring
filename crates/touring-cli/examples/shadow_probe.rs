//! B2/B1 probe — run the `family_gate` and `lesson_lifecycle` shadow sections over EVERY real
//! transcript of the project, without the daemon (the live daemon embeds the CLI of its last
//! release) and with the scan caps lifted.
//!
//! The KPI reads the newest `SHADOW_CAPS` transcripts; the calibration of `family_gate` and the
//! corpus numbers of `lesson_lifecycle` need the whole history. This probe links all transcripts
//! (optionally above a size floor) into a throwaway `HOME` and hands that to the same functions
//! the KPI calls, so the numbers printed are exactly what the KPI would print with the caps off.
//! It also re-derives the global MUST counts with `context_budget`'s own aggregator, session by
//! session, to prove the per-family counts add up to the global ruler.
//!
//! Usage: `cargo run -p touring-cli --example shadow_probe -- [--project-root DIR]
//! [--min-bytes N]`
//!
//! Output is counts, family names and numbers — never transcript text.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use touring_cli::cli::context_budget::{
    ContextBudgetAggregate, aggregate_session_lines, transcript_dir_for,
};
use touring_cli::cli::family_gate::family_gate_in;
use touring_cli::cli::lesson_lifecycle::lesson_lifecycle_in;
use touring_cli::cli::transcript_scan::ScanCaps;

struct Args {
    project_root: PathBuf,
    min_bytes: u64,
}

fn parse_args() -> Args {
    let mut args = Args {
        project_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        min_bytes: 0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().unwrap_or_default();
        match flag.as_str() {
            "--project-root" => args.project_root = PathBuf::from(value),
            "--min-bytes" => args.min_bytes = value.parse().unwrap_or(args.min_bytes),
            other => eprintln!("ignored flag {other}"),
        }
    }
    args
}

fn die(what: &str, e: impl std::fmt::Display) -> ! {
    eprintln!("{what}: {e}");
    std::process::exit(1)
}

/// Every transcript of the project at or above `min_bytes`.
fn transcripts(dir: &Path, min_bytes: u64) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() >= min_bytes))
        .collect();
    files.sort();
    files
}

/// The global MUST counts of the `context_budget` ruler, one fresh aggregate per session.
fn global_counts(files: &[PathBuf]) -> (u64, u64, u64) {
    let mut total = ContextBudgetAggregate::default();
    for path in files {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut session = ContextBudgetAggregate::default();
        let mut seen: BTreeMap<u64, u64> = BTreeMap::new();
        aggregate_session_lines(content.lines(), &mut session, &mut seen, None);
        total.must_emitted += session.must_emitted;
        total.must_denied += session.must_denied;
        total.must_followed += session.must_followed;
    }
    (total.must_emitted, total.must_denied, total.must_followed)
}

/// Sum of one counter over the families of the given windows of the `family_gate` payload.
fn family_sum(gate: &Value, field: &str) -> u64 {
    ["calibration", "application"]
        .iter()
        .filter_map(|w| gate.pointer(&format!("/windows/{w}/families")))
        .filter_map(Value::as_object)
        .flat_map(|m| m.values())
        .filter_map(|f| f[field].as_u64())
        .sum()
}

/// A window as a compact table: per family, `n`, ratio, tokens and the recommended level.
fn window_table(gate: &Value, window: &str) -> Value {
    let w = &gate["windows"][window];
    let rows: Vec<Value> = w["families"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, f)| {
            json!({
                "family": name, "blocks": f["blocks"], "n": f["n"],
                "follow_ratio": f["follow_ratio"], "injected_tokens_est": f["injected_tokens_est"],
                "level": f["recommended_level"], "status": f["status"], "reason": f["reason"],
            })
        })
        .collect();
    json!({"sessions": w["sessions"], "families": rows})
}

fn main() {
    let args = parse_args();
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        die("HOME", "unset");
    };
    let real = transcript_dir_for(&home, &args.project_root);
    let files = transcripts(&real, args.min_bytes);
    let sandbox = tempfile::tempdir().unwrap_or_else(|e| die("tempdir", e));
    let linked = transcript_dir_for(sandbox.path(), &args.project_root);
    std::fs::create_dir_all(&linked).unwrap_or_else(|e| die("mkdir", e));
    for path in &files {
        if let Some(name) = path.file_name() {
            std::os::unix::fs::symlink(path, linked.join(name))
                .unwrap_or_else(|e| die("symlink", e));
        }
    }
    let caps = ScanCaps {
        max_transcripts: usize::MAX,
        max_bytes: u64::MAX,
    };
    let gate = family_gate_in(sandbox.path(), &args.project_root, &caps);
    let lessons = lesson_lifecycle_in(sandbox.path(), &args.project_root, &caps);
    let (emitted, denied, followed) = global_counts(&files);
    let out = json!({
        "transcripts": files.len(),
        "min_bytes": args.min_bytes,
        "family_gate": {
            "rule": gate["rule"],
            "undated_sessions": gate["undated_sessions"],
            "calibration_window": window_table(&gate, "calibration"),
            "application_window": window_table(&gate, "application"),
            "calibration": gate["calibration"],
            "parity_with_context_budget": {
                "global": {"must_emitted": emitted, "must_denied": denied, "must_followed": followed},
                "families": {
                    "must_emitted": family_sum(&gate, "must_emitted"),
                    "must_denied": family_sum(&gate, "must_denied"),
                    "followed": family_sum(&gate, "followed"),
                },
            },
        },
        "lesson_lifecycle": lessons,
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
}
