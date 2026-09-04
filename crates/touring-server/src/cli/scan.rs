//! `touring scan vulnerabilities` — CWE vulnerability scan of a source file.
//!
//! Returns cwes array (id, severity, line) + p0_block flag.
//!
//! Complementação-hooks H6 (F1.5 + F4): handler PostToolUse(Write) needs this
//! subcommand to inject `touring.scan_vulnerabilities` JSON via additionalContext.
//!
//! F4 (2026-08-31): wires real CWE detection — reads the source from disk.
//! S8 (2026-09-02): the detector is `touring_code::cwe_scan::detect_cwes`, the
//! SAME one the hook layer runs (the inline copy that lived here had drifted:
//! it never emitted CWE-22). This module only maps findings to the CLI JSON.

use super::common::{human_to_stderr, json_to_stdout, parse_global_flags};
use anyhow::Context;
use std::fs;
use std::path::Path;

#[derive(serde::Serialize)]
struct Cwe {
    id: String,
    severity: &'static str, // P0 | P1 | P2
    line: u32,
    description: String,
}

#[derive(serde::Serialize)]
struct ScanResult {
    file_path: String,
    cwes: Vec<Cwe>,
    p0_block: bool,
}

/// Detect CWEs in source text.
///
/// S8 (2026-09-02): delegates to the ONE shared detector
/// (`touring_code::cwe_scan::detect_cwes`) — the inline copy that used to live
/// here had drifted from the hook layer's (it never emitted CWE-22). The CLI
/// only maps findings to its JSON shape.
fn detect_cwes_in_source(source: &str) -> Vec<Cwe> {
    touring_code::cwe_scan::detect_cwes(source)
        .into_iter()
        .map(|f| Cwe {
            id: f.id,
            severity: f.severity.as_str(),
            line: f.line,
            description: f.description,
        })
        .collect()
}

/// CLI entry point — `touring scan vulnerabilities <file>`.
///
/// Emits JSON `{ file_path, cwes: [{ id, severity, line, description }], p0_block }`
/// to stdout; human-readable status to stderr. Used by complementação-hooks H6 handler.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let (_flags, filtered) = parse_global_flags(args);

    // args[0] = binary path, args[1] = subcmd name "scan", args[2..] = positional.
    // The subcmd-name slot is "scan" itself; skip it to reach the file path.
    // We accept the file path as the first positional AFTER the subcmd token.
    let subcmd_skip = filtered
        .iter()
        .position(|a| a == "scan")
        .map(|i| i + 1)
        .unwrap_or(0);
    let positional: Vec<&str> = filtered[subcmd_skip..].iter().map(String::as_str).collect();

    let file_path = positional
        .first()
        .context("file path required: touring scan vulnerabilities <file>")?
        .to_string();

    let source = fs::read_to_string(Path::new(&file_path))
        .map_err(|e| anyhow::anyhow!("read {} failed: {}", file_path, e))?;
    let findings = detect_cwes_in_source(&source);
    let p0_block = findings.iter().any(|c| c.severity == "P0");

    let result = ScanResult {
        file_path: file_path.clone(),
        cwes: findings,
        p0_block,
    };

    json_to_stdout(&serde_json::to_string(&result)?);
    human_to_stderr(&format!(
        "scan vulnerabilities for {}: {} CWE(s), p0_block={}",
        file_path,
        result.cwes.len(),
        p0_block
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_file_arg() {
        // Use any existing file under target/ or repo root for testing.
        let args = vec!["/home/gabrielgadea/projects/touring/Cargo.toml".to_string()];
        let r = run(&args);
        assert!(r.is_ok());
    }

    #[test]
    fn missing_file_returns_error() {
        let args = vec![];
        let r = run(&args);
        assert!(r.is_err());
    }

    #[test]
    fn detect_cwes_inline_finds_hardcoded_key() {
        // Build fixture at runtime so the detector pattern doesn't trigger on the
        // test source itself.
        let needle = touring_code::cwe_scan::vendor_prefix_openai_like();
        let src = format!("const KEY: &str = \"{}PLACEHOLDER api\";\n", needle);
        let findings = detect_cwes_in_source(&src);
        assert!(findings.iter().any(|c| c.id == "CWE-798" && c.severity == "P0"));
    }

    #[test]
    fn detect_cwes_inline_finds_sql_injection() {
        let src = "let q = format!(\"SELECT * FROM users WHERE id = {}\", id);\n";
        let findings = detect_cwes_in_source(src);
        assert!(findings.iter().any(|c| c.id == "CWE-89"));
    }

    #[test]
    fn detect_cwes_inline_finds_unwrap() {
        let unwrap_call = touring_code::cwe_scan::unwrap_call_pattern();
        let src = format!("fn main() {{\n    let x = foo(){unwrap_call};\n}}\n");
        let findings = detect_cwes_in_source(&src);
        assert!(findings.iter().any(|c| c.id == "CWE-394"));
    }

    #[test]
    fn detect_cwes_inline_clean_source_has_no_findings() {
        let src = "fn main() {\n    println!(\"hello\");\n}\n";
        let findings = detect_cwes_in_source(src);
        assert!(findings.is_empty());
    }

    /// S8 (2026-09-02): the CLI copy of the detector had drifted from the hook
    /// one — it never emitted CWE-22 (path traversal). One detector now serves
    /// both (`touring_code::cwe_scan::detect_cwes`); this is the drift guard.
    #[test]
    fn detect_cwes_inline_finds_path_traversal_like_the_hook_detector() {
        let src = "fn load(p: &str) -> String {\n    fs::read_to_string(p).unwrap_or_default()\n}\n";
        let ids: Vec<String> = detect_cwes_in_source(src).into_iter().map(|c| c.id).collect();
        assert!(ids.contains(&"CWE-22".to_string()), "{ids:?}");
    }
}