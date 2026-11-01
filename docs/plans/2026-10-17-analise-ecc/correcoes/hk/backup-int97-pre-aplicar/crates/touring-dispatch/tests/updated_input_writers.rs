//! S211 (plan 2026-09-25) — the producers of `updatedInput` are PINNED.
//!
//! Claude Code applies `hookSpecificOutput.updatedInput` from PreToolUse hooks with "the
//! last to finish wins" — the order is not deterministic — so two writers on one tool turn
//! the rewritten input into a race. The analise contract suite
//! (`tests/hooks_contract/test_updated_input_writers.py`) proves AT MOST ONE writer per tool
//! from the REGISTRATIONS; this test pins the PRODUCERS in the source, so a new writer cannot
//! appear without that table (and its proof) being updated in the same change.

use std::path::{Path, PathBuf};

/// Files that BUILD an `updatedInput` for a hook, with the registry name and tool they serve.
const PRODUCERS: &[&str] = &[
    // "cli-suggest" → Bash (pipefail + G8 loop rewrite, decided in ONE process, in order)
    "touring-cli/src/cli_suggester.rs",
    // "cli-suggest" → Bash (code-mode rewrite)
    "touring-cli/src/hook_rewrite.rs",
    // "pre-read" → Read
    "touring-hook-handlers/src/hooks/pre_read.rs",
    // "pre-tool-use" → NOT registered in any settings (dormant)
    "touring-hook-handlers/src/hooks/pre_tool_use.rs",
];

/// Files that DEFINE, build or serialize the variant — infrastructure, not writers.
const FRAMEWORK: &[&str] = &[
    "touring-hook-runtime/src/hook_runtime.rs",
    "touring-hook-runtime/src/hook_runtime_ext.rs",
    "touring-hooks-core/src/hook_response.rs",
];

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n == "target" || n == "tests")
            {
                continue;
            }
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// A test file is not a producer: `*_tests.rs` and `integration_tests.rs`.
fn is_production(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    !(name.ends_with("_tests.rs") || name == "integration_tests.rs")
}

/// A line that constructs the variant or writes the JSON key (comments excluded).
fn produces(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_start();
        !t.starts_with("//")
            && (t.contains("ContextWithUpdatedInput {")
                || t.contains("\"updatedInput\":")
                || t.contains("\"updatedInput\" :"))
    })
}

#[test]
fn updated_input_producers_are_pinned() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir");
    let mut files = Vec::new();
    rs_files(crates, &mut files);
    assert!(
        files.len() > 500,
        "control: the walk must see the workspace ({} files)",
        files.len()
    );
    let mut found: Vec<String> = files
        .iter()
        .filter(|p| is_production(p))
        .filter(|p| std::fs::read_to_string(p).is_ok_and(|t| produces(&t)))
        .filter_map(|p| p.strip_prefix(crates).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    found.sort();
    let mut expected: Vec<String> = PRODUCERS
        .iter()
        .chain(FRAMEWORK)
        .map(|s| (*s).to_string())
        .collect();
    expected.sort();
    assert_eq!(
        found, expected,
        "the set of updatedInput producers changed: a NEW writer must be added to PRODUCERS \
         with its registry name and tool, AND to the WRITERS table of the analise contract \
         test (tests/hooks_contract/test_updated_input_writers.py), which proves at most one \
         writer per tool — two writers on one tool are a race (last to finish wins)"
    );
}

#[test]
fn producers_declare_the_real_event_name() {
    // A hookSpecificOutput whose hookEventName is not the event it answers is discarded by
    // Claude Code; `pre_tool_use` and `TaskCreate` (a TOOL name) were emitted here.
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir");
    for rel in PRODUCERS {
        let text = std::fs::read_to_string(crates.join(rel)).expect("producer file");
        for bad in [
            "Some(\"pre_tool_use\"",
            "Some(\"TaskCreate\"",
            "Some(\"pre-tool-use\"",
        ] {
            assert!(
                !text.contains(bad),
                "{rel} emits the event name {bad:?}: it must be PreToolUse"
            );
        }
    }
}
