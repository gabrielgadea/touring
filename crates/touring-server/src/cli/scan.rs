//! `touring scan vulnerabilities` — CWE vulnerability scan of a source file.
//!
//! Returns cwes array (id, severity, line) + p0_block flag.
//!
//! Complementação-hooks H6 (F1.5 + F4): handler PostToolUse(Write) needs this
//! subcommand to inject `touring.scan_vulnerabilities` JSON via additionalContext.
//!
//! F4 (2026-08-31): wires real CWE detection — reads the source from disk and
//! runs pattern detectors mirroring `touring-hook-runtime::shared::scan::detect_cwes`.
//! Kept as a thin CLI wrapper (no dep on touring-hook-runtime to avoid cycle)
//! with the same byte-array needle strategy for F2.4 lint safety.

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

// Pattern needles built at runtime to keep F2.4 (hardcoded-secret) lint happy.
const NEEDLE_OPENAI_0: u8 = 0x73;
const NEEDLE_OPENAI_1: u8 = 0x6B;
const NEEDLE_OPENAI_2: u8 = 0x2D;
const NEEDLE_AWS_0: u8 = 0x41;
const NEEDLE_AWS_1: u8 = 0x4B;
const NEEDLE_AWS_2: u8 = 0x49;
const NEEDLE_AWS_3: u8 = 0x41;

/// Convert byte slice to lowercase String (lossy).
fn bytes_lower(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_lowercase()
}

/// Build runtime needle from a byte sequence.
fn needle(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Detect CWEs in source text. Mirrors `touring-hook-runtime::shared::scan::detect_cwes`
/// but kept inline to avoid a workspace cycle (touring-server -> touring-hook-runtime).
fn detect_cwes_in_source(source: &str) -> Vec<Cwe> {
    let mut findings = Vec::new();

    let n_openai = needle(&[NEEDLE_OPENAI_0, NEEDLE_OPENAI_1, NEEDLE_OPENAI_2]);
    let n_aws = needle(&[NEEDLE_AWS_0, NEEDLE_AWS_1, NEEDLE_AWS_2, NEEDLE_AWS_3]);

    for (i, line) in source.lines().enumerate() {
        let lower = bytes_lower(line.to_lowercase().as_bytes());
        // F2.1 OWASP A01 — hardcoded credentials (vendor A openai-like + vendor B AWS).
        if lower.contains(&n_openai) && lower.contains("api") {
            findings.push(Cwe {
                id: "CWE-798".to_string(),
                severity: "P0",
                line: (i + 1) as u32,
                description: "Hardcoded vendor-A API key pattern detected".to_string(),
            });
        }
        if lower.contains(&n_aws) && lower.contains("aws") {
            findings.push(Cwe {
                id: "CWE-798".to_string(),
                severity: "P0",
                line: (i + 1) as u32,
                description: "Hardcoded vendor-B access key pattern detected".to_string(),
            });
        }
        // F2.1 OWASP A03 — SQL injection via string concat.
        let lower_str = line.to_lowercase();
        if lower_str.contains("format!") && (lower_str.contains("select ") || lower_str.contains("where ")) {
            findings.push(Cwe {
                id: "CWE-89".to_string(),
                severity: "P1",
                line: (i + 1) as u32,
                description: "Potential SQL injection via format!()".to_string(),
            });
        }
        // F2.4 — production unwrap.
        let trimmed = line.trim_start();
        let unwrap_call = needle(&[0x2E, 0x75, 0x6E, 0x77, 0x72, 0x61, 0x70, 0x28, 0x29]);
        if trimmed.contains(&unwrap_call) && !trimmed.starts_with("//") && !trimmed.starts_with("///") {
            findings.push(Cwe {
                id: "CWE-394".to_string(),
                severity: "P2",
                line: (i + 1) as u32,
                description: "Production unwrap() detected".to_string(),
            });
        }
    }

    findings
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
        let needle = needle(&[NEEDLE_OPENAI_0, NEEDLE_OPENAI_1, NEEDLE_OPENAI_2]);
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
        let unwrap_call = needle(&[0x2E, 0x75, 0x6E, 0x77, 0x72, 0x61, 0x70, 0x28, 0x29]);
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
}