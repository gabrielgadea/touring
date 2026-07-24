//! R3 — `touring audit <file>`: CLI adapter over the `run_audit` engine
//! (code-mode without MCP).
//!
//! Mirrors the `touring_audit` MCP tool over the SAME engine
//! (`server::tools_workflow::run_audit`) — engine once, two adapters (the MT-1
//! pattern, identical to `route` / `search`). One ranked report: the offensive
//! CWE/OWASP vulnerability layer (10 detectors) + the 6 P0 BLOCK quality gates,
//! plus a `Block|Warn|Info` verdict. See
//! `docs/2026-06-27-coupling-codemode-cli-and-master-commands.md` §4 (R3, audit row).
//!
//! The verdict is propagated as the **exit code** so `touring audit` is a usable
//! gate: `Info`/clean = 0, `Warn` = 1, `Block` = 2.

use anyhow::{Context, Result};
use clap::Parser;
use std::path::Path;

use crate::server::tools_workflow::{AuditLayers, AuditReport, AuditSeverity, run_audit};

/// `touring audit` — offensive CWE/OWASP + 6 P0 quality gates over one file.
#[derive(Parser, Debug)]
#[command(
    name = "audit",
    about = "Audit a source file: offensive CWE/OWASP + 6 P0 quality gates (code-mode, no MCP)",
    long_about = "Run the same engine as the touring_audit MCP tool over a single file: \
                  the offensive vulnerability layer (SQL/command/LDAP/XML injection, XSS, path \
                  traversal, SSRF, deserialization, integer/buffer overflow) and the 6 P0 BLOCK \
                  quality gates (secrets, dependency CVEs, config, deprecated APIs, package mgmt). \
                  Exit code is the verdict: Info=0, Warn=1, Block=2."
)]
struct AuditCli {
    /// Source file to audit
    file: String,
    /// Layers to run (comma-separated): vuln, quality, or all (default: all)
    #[arg(long, value_delimiter = ',')]
    layers: Option<Vec<String>>,
    /// Compact JSON output (default: pretty-printed)
    #[arg(short = 'j', long)]
    json: bool,
}

/// Map an audit verdict to a process exit code so the command gates by severity.
fn verdict_exit_code(verdict: AuditSeverity) -> i32 {
    match verdict {
        AuditSeverity::Block => 2,
        AuditSeverity::Warn => 1,
        AuditSeverity::Info => 0,
    }
}

/// Serialize a report as compact or pretty JSON per the `--json` flag.
fn render(report: &AuditReport, compact: bool) -> Result<String> {
    let text = if compact {
        serde_json::to_string(report)?
    } else {
        serde_json::to_string_pretty(report)?
    };
    Ok(text)
}

/// CLI entry point for `touring audit`. `args[0]` = binary, `args[1]` = "audit";
/// clap parses `args[1..]` with "audit" as the program name (same convention as
/// `run`). The verdict severity becomes the process exit code.
pub fn run(args: &[String]) -> Result<()> {
    let cli = match AuditCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    let path = Path::new(&cli.file);
    let layers = AuditLayers::from_param(&cli.layers);
    let report =
        run_audit(path, &layers).with_context(|| format!("audit could not read {}", cli.file))?;
    println!("{}", render(&report, cli.json)?);
    let code = verdict_exit_code(report.verdict);
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn temp_file(name: &str, content: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("touring_cli_audit_{}_{name}", std::process::id()));
        let mut f = std::fs::File::create(&dir).expect("create temp");
        f.write_all(content.as_bytes()).expect("write temp");
        dir
    }
    #[test]
    fn parses_file_and_layers() {
        let cli = AuditCli::try_parse_from(["audit", "src/x.rs", "--layers", "vuln,quality"])
            .expect("parse");
        assert_eq!(cli.file, "src/x.rs");
        assert_eq!(
            cli.layers.as_deref(),
            Some(["vuln".to_string(), "quality".to_string()].as_slice())
        );
        assert!(!cli.json);
    }
    #[test]
    fn file_is_required() {
        assert!(AuditCli::try_parse_from(["audit"]).is_err());
    }
    #[test]
    fn json_flag_parsed() {
        let cli = AuditCli::try_parse_from(["audit", "x.rs", "-j"]).expect("parse");
        assert!(cli.json);
    }
    #[test]
    fn verdict_exit_code_maps_severity() {
        assert_eq!(verdict_exit_code(AuditSeverity::Block), 2);
        assert_eq!(verdict_exit_code(AuditSeverity::Warn), 1);
        assert_eq!(verdict_exit_code(AuditSeverity::Info), 0);
    }
    #[test]
    fn run_audit_blocks_on_xss() {
        // CWE-79 XSS is a known Block via the vuln layer (mirrors tools_workflow's own
        // test); proves run_audit -> Block -> exit 2 without embedding a secret pattern
        // in this file (which would itself trip the F2.4 secret gate).
        let path = temp_file("xss", "render(\"<script>alert(1)</script>\")\n");
        let report = run_audit(
            &path,
            &AuditLayers {
                vuln: true,
                quality: false,
            },
        )
        .expect("audit runs");
        assert_eq!(
            verdict_exit_code(report.verdict),
            2,
            "XSS must gate to Block/exit 2"
        );
        let _ = std::fs::remove_file(&path);
    }
    #[test]
    fn run_audit_clean_file_is_info() {
        let path = temp_file("clean", "fn add(a: i32, b: i32) -> i32 { a + b }\n");
        let report = run_audit(
            &path,
            &AuditLayers {
                vuln: true,
                quality: false,
            },
        )
        .expect("audit runs");
        assert_eq!(
            verdict_exit_code(report.verdict),
            0,
            "clean file must be Info/exit 0"
        );
        let _ = std::fs::remove_file(&path);
    }
    #[test]
    fn render_compact_and_pretty_differ() {
        let path = temp_file("render", "fn ok() {}\n");
        let report = run_audit(
            &path,
            &AuditLayers {
                vuln: true,
                quality: true,
            },
        )
        .expect("audit runs");
        let compact = render(&report, true).expect("compact");
        let pretty = render(&report, false).expect("pretty");
        assert!(!compact.contains('\n'), "compact is single-line");
        assert!(pretty.contains('\n'), "pretty is multi-line");
        let _ = std::fs::remove_file(&path);
    }
}
