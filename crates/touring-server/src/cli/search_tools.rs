//! `touring search-tools <intent>` — intent-ranked tool discovery (C3).
//!
//! Progressive disclosure for the CLI surface: describe what you want to do in
//! natural language and get the ranked Touring commands that fit, instead of
//! scanning the full `--help`. Backed by [`crate::tool_catalog`].

use crate::tool_catalog::{search_as_json, search_catalog};

/// Default number of ranked tools returned.
const DEFAULT_TOP_K: usize = 8;

/// Entry point for the `search-tools` subcommand. `args` is the full process
/// argv (`[binary, "search-tools", ...intent]` from `std::env::args`), so the
/// intent is `args[2..]`; a `-j`/`--json` flag anywhere selects JSON output.
/// Always exits `Ok` — discovery is advisory.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let json = args.iter().any(|a| a == "-j" || a == "--json");
    let intent = args
        .iter()
        .skip(2) // skip the binary name and the "search-tools" subcommand
        .filter(|a| *a != "-j" && *a != "--json")
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let intent = intent.trim();

    if intent.is_empty() {
        eprintln!("usage: touring search-tools [-j] <natural-language intent>");
        eprintln!("  e.g. touring search-tools \"find who calls a function\"");
        return Ok(());
    }

    if json {
        println!("{}", search_as_json(intent, DEFAULT_TOP_K));
        return Ok(());
    }

    let hits = search_catalog(intent, DEFAULT_TOP_K);
    if hits.is_empty() {
        println!("No matching tool for intent: {intent}");
        return Ok(());
    }
    println!("intent: {intent}");
    for (i, h) in hits.iter().enumerate() {
        println!(
            "  {}. {}  [{}]  (score {:.2})",
            i + 1,
            h.entry.name,
            h.entry.kind,
            h.score
        );
        println!("     {} — {}", h.entry.summary, h.entry.when_to_use);
    }
    Ok(())
}
