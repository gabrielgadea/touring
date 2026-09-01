//! Smoke test for the Rust SignalReport — emits the same JSON `gen_sdk.py`
//! produces, so the two cross-process emitters can be diffed.
//!
//! Usage:
//! ```text
//! cargo run --example sdk_smoke -- <journal_path> <out_path>
//! ```
//!
//! Run with `gen_sdk.py --check` and diff the two outputs to prove the
//! schemas agree. The diff is the cross-process invariant.

use std::env;
use std::path::PathBuf;

use touring_code::sdk::signal_report_from_journal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: sdk_smoke <journal> <out>");
        std::process::exit(2);
    }
    let journal = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[2]);

    let report = signal_report_from_journal(&journal)?;
    let body = serde_json::to_string_pretty(&report)?;
    let len = body.len();
    std::fs::write(&out, body)?;
    println!("Wrote {} bytes to {}", len, out.display());
    Ok(())
}