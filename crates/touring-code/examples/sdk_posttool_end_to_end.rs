//! End-to-end demo of the F3 PostToolUse-sync path:
//! 1. Record a handful of hook calls into the mirror.
//! 2. Build the SignalReport from BOTH the journal AND the mirror.
//! 3. Print the report so the operator can verify counts are no longer zero.
//!
//! Usage:
//! ```text
//! cargo run --example sdk_posttool_end_to_end -- <journal> <mirror> <out>
//! ```
//!
//! Three arguments: source journal path, mirror path (will be created),
//! output report path. The mirror is seeded with a few synthetic calls so
//! the operator sees counts move off zero without needing a live PostToolUse.

use std::env;
use std::path::PathBuf;

use touring_code::sdk::HookName;
use touring_code::sdk_signal_mirror::{
    MirrorOrigin, default_mirror_path, read, record, signal_report_from_journal_and_mirror,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let (journal, out) = match args.len() {
        3 => (PathBuf::from(&args[1]), PathBuf::from(&args[2])),
        _ => {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            let journal = PathBuf::from(&home).join(".claude/touring/run_journal.jsonl");
            let out = PathBuf::from("/tmp/sdk_end_to_end_report.json");
            run_default(&journal, &out)?;
            return Ok(());
        }
    };

    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let mirror = default_mirror_path(&PathBuf::from(&home));
    run(&journal, &mirror, &out)?;
    Ok(())
}

fn run_default(
    journal: &std::path::Path,
    out: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let mirror = default_mirror_path(&PathBuf::from(&home));
    run(journal, &mirror, out)
}

fn run(
    journal: &std::path::Path,
    mirror: &std::path::Path,
    out: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1 — seed the mirror with synthetic calls. A real PostToolUse
    // handler invokes `record()` exactly this way.
    for _ in 0..5 {
        record(mirror, HookName::AstMeta, 8, true, MirrorOrigin::Sdk)?;
    }
    for _ in 0..3 {
        record(mirror, HookName::MemoryRecall, 42, true, MirrorOrigin::Sdk)?;
    }
    record(mirror, HookName::Parallel, 100, false, MirrorOrigin::Sdk)?;

    // Step 2 — read what we just wrote.
    let agg = read(mirror)?;
    println!(
        "mirror seeded: {} calls, {} failures",
        agg.total_calls, agg.total_failures
    );

    // Step 3 — build the combined report.
    let mut report = signal_report_from_journal_and_mirror(journal, mirror)?;
    // Stamp a deterministic timestamp so two runs are byte-comparable.
    report.generated_at_unix = 0; // 1970-01-01 — "do not trust the wall clock here"
    let body = serde_json::to_string_pretty(&report)?;
    let len = body.len();
    std::fs::write(out, body)?;
    println!("Wrote {} bytes to {}", len, out.display());
    Ok(())
}