//! `touring diagnostics <file> [--with-fixes] [-j]` — List diagnostics for a file.
//!
//! D.3 (RFC-100): Lists RFC-100 diagnostics for a file, showing fix assist
//! candidates when `--with-fixes` is passed.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`. No daemon_query — uses
//! touring_foundation::diagnostic directly.

use std::path::Path;

use clap::Parser;
use touring_foundation::diagnostic::{Diagnostic, Severity, codes};

use super::common::json_to_stdout;

#[derive(Parser, Debug)]
#[command(
    name = "touring diagnostics",
    bin_name = "touring diagnostics",
    about = "List RFC-100 diagnostics for a file with optional fix candidates",
    disable_help_subcommand = true
)]
struct DiagnosticsCli {
    /// Source file to analyse.
    file: String,
    /// Also report applicable fix assist candidates.
    #[arg(long)]
    with_fixes: bool,
    /// Output as JSON.
    #[arg(short = 'j', long = "json")]
    json: bool,
}
/// D.3.4 CLI — list diagnostics for a file with optional fix candidates.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match DiagnosticsCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    let file_path = cli.file.as_str();
    if !Path::new(file_path).exists() {
        return Err(anyhow::anyhow!("file not found: {file_path}"));
    }
    let source = std::fs::read_to_string(file_path)?;
    let diagnostics = collect_diagnostics(&source, file_path, cli.with_fixes);
    if cli.json {
        let out: Vec<_> = diagnostics
            .into_iter()
            .map(|d| {
                let fixes = if cli.with_fixes {
                    d.fixes.clone()
                } else {
                    Vec::new()
                };
                serde_json::json!(
                    { "code" : d.code, "severity" : d.severity.as_str(), "message" : d
                    .message, "file" : d.file, "line" : d.line, "help" : d.help, "fixes"
                    : fixes, }
                )
            })
            .collect();
        json_to_stdout(&serde_json::to_string_pretty(&out)?);
    } else {
        for d in &diagnostics {
            let sev = d.severity.as_str().to_uppercase();
            let line = d.line.map_or(String::new(), |l| format!(":{l}"));
            println!("[{sev}] {}{} — {}", d.code, line, d.message);
            if let Some(help) = &d.help {
                println!("  help: {help}");
            }
            if cli.with_fixes && !d.fixes.is_empty() {
                println!("  fixes: {}", d.fixes.join(", "));
            }
        }
        if diagnostics.is_empty() {
            println!("No diagnostics found for {file_path}");
        }
    }
    Ok(())
}
/// Fix table: RFC-100 code → applicable AssistIds.
fn get_fixes_for(code: &str) -> Vec<String> {
    match code {
        "Q-201" | "W-100" | "W-101" | "W-102" | "W-103" => vec!["auto_wire".to_string()],
        "Q-220" => vec!["format_rust_preserve".to_string()],
        "Q-240" => vec!["extract_function".to_string()],
        "M-500" | "M-510" | "M-520" | "M-530" => vec!["auto_import".to_string()],
        "B-301" => vec!["extract_function".to_string()],
        _ => vec![],
    }
}
fn check_pub_symbols(source: &str, file_path: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        if line.contains("pub fn") || line.contains("pub struct") {
            if let Some(name) = extract_symbol_name(line) {
                out.push(
                    Diagnostic::new(
                        codes::Q_201_TDG_GRADE_F,
                        Severity::Warning,
                        format!("pub symbol `{name}` may be orphan — verify wiring"),
                    )
                    .with_file(file_path)
                    .with_line((idx + 1) as u32)
                    .with_help("Run `touring wiring orphans` to check; use `auto_wire` assist")
                    .with_fixes(get_fixes_for("Q-201")),
                );
            }
        }
    }
    out
}
fn check_format_idempotency(source: &str, file_path: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        if line.contains("format!(") && line.contains("format!(\"") {
            out.push(
                Diagnostic::new(
                    codes::Q_220_IMPROVEMENT_STREAK,
                    Severity::Info,
                    "Nested format! call may not be idempotent".to_string(),
                )
                .with_file(file_path)
                .with_line((idx + 1) as u32)
                .with_help("Verify format(format(x)) == format(x)")
                .with_fixes(get_fixes_for("Q-220")),
            );
        }
    }
    out
}
fn check_cyclomatic_complexity(source: &str, file_path: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        if line.contains("match") || line.contains("if let") {
            let following: Vec<_> = source.lines().skip(idx).take(10).collect();
            let branches = following
                .iter()
                .take(8)
                .filter(|l| l.trim_start().starts_with("=>"))
                .count();
            if branches >= 6 {
                out.push(
                    Diagnostic::new(
                        codes::Q_240_HIGH_CYCLOMATIC,
                        Severity::Warning,
                        format!("High cyclomatic complexity detected (~{branches} branches)"),
                    )
                    .with_file(file_path)
                    .with_line((idx + 1) as u32)
                    .with_help("Consider extracting functions to reduce branches")
                    .with_fixes(get_fixes_for("Q-240")),
                );
            }
        }
    }
    out
}
fn check_module_decls(source: &str, file_path: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        if line.contains("mod ") {
            out.push(
                Diagnostic::new(
                    codes::M_500_RECALL_EMPTY,
                    Severity::Hint,
                    "Module declaration detected".to_string(),
                )
                .with_file(file_path)
                .with_line((idx + 1) as u32)
                .with_fixes(get_fixes_for("M-500")),
            );
        }
    }
    out
}
fn collect_diagnostics(source: &str, file_path: &str, with_fixes: bool) -> Vec<Diagnostic> {
    if !with_fixes {
        return vec![];
    }
    let mut diags = Vec::new();
    diags.extend(check_pub_symbols(source, file_path));
    diags.extend(check_format_idempotency(source, file_path));
    diags.extend(check_cyclomatic_complexity(source, file_path));
    diags.extend(check_module_decls(source, file_path));
    diags
}
fn extract_symbol_name(line: &str) -> Option<String> {
    if let Some(pos) = line.find("pub fn") {
        let after = line[pos + 6..].trim();
        let end = after
            .find(|c: char| c.is_whitespace() || c == '(')
            .unwrap_or(after.len());
        return Some(after[..end].to_string());
    }
    if let Some(pos) = line.find("pub struct") {
        let after = line[pos + 10..].trim();
        let end = after
            .find(|c: char| c.is_whitespace() || c == ';')
            .unwrap_or(after.len());
        return Some(after[..end].to_string());
    }
    None
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    DiagnosticsCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }
    #[test]
    fn extract_symbol_name_pub_fn() {
        assert_eq!(
            extract_symbol_name("pub fn foo_bar()"),
            Some("foo_bar".to_string())
        );
        assert_eq!(
            extract_symbol_name("  pub fn main() -> Result<()>"),
            Some("main".to_string())
        );
    }
    #[test]
    fn extract_symbol_name_pub_struct() {
        assert_eq!(
            extract_symbol_name("pub struct Foo<T>"),
            Some("Foo<T>".to_string())
        );
    }
    #[test]
    fn get_fixes_for_auto_wire_codes() {
        for code in &["Q-201", "W-100", "W-101", "W-102", "W-103"] {
            let fixes = get_fixes_for(code);
            assert_eq!(
                fixes,
                vec!["auto_wire"],
                "code {code} should map to auto_wire"
            );
        }
    }
    #[test]
    fn get_fixes_for_q220() {
        assert_eq!(get_fixes_for("Q-220"), vec!["format_rust_preserve"]);
    }
    #[test]
    fn get_fixes_for_m5xx() {
        for code in &["M-500", "M-510", "M-520", "M-530"] {
            assert_eq!(
                get_fixes_for(code),
                vec!["auto_import"],
                "code {code} should map to auto_import"
            );
        }
    }
    #[test]
    fn get_fixes_for_unknown() {
        assert!(get_fixes_for("X-999").is_empty());
    }
    #[test]
    fn collect_diagnostics_empty_without_fixes() {
        let src = "pub fn main() {}\nlet x = 1;";
        let diags = collect_diagnostics(src, "test.rs", false);
        assert!(diags.is_empty());
    }
    #[test]
    fn collect_diagnostics_finds_pub_fn_with_fixes() {
        let src = "pub fn helper() {}\npub fn main() {}";
        let diags = collect_diagnostics(src, "test.rs", true);
        assert!(!diags.is_empty());
        assert_eq!(diags[0].code, "Q-201");
        assert!(!diags[0].fixes.is_empty());
    }
    #[test]
    fn check_format_idempotency_detects_nested() {
        let src = "let s = format!(\"a {} format!(\\\"b\\\")\");\n";
        let diags = check_format_idempotency(src, "test.rs");
        assert!(!diags.is_empty());
    }
    #[test]
    fn check_cyclomatic_complexity_detects_match() {
        let src = "match x {\n=> 1,\n=> 2,\n=> 3,\n=> 4,\n=> 5,\n=> 6,\n}";
        let diags = check_cyclomatic_complexity(src, "test.rs");
        assert!(!diags.is_empty());
    }
    #[test]
    fn check_module_decls_detects_mod() {
        let src = "mod inner;\npub fn foo() {}".to_string();
        let diags = check_module_decls(&src, "test.rs");
        assert!(!diags.is_empty());
    }
    #[test]
    fn cli_requires_file_arg() {
        let result = DiagnosticsCli::try_parse_from(s(&["diagnostics"]).iter());
        assert!(result.is_err());
    }
    #[test]
    fn cli_parses_file_and_flags() {
        let cli = DiagnosticsCli::try_parse_from(
            s(&["diagnostics", "src/lib.rs", "--with-fixes", "-j"]).iter(),
        )
        .unwrap();
        assert_eq!(cli.file, "src/lib.rs");
        assert!(cli.with_fixes);
        assert!(cli.json);
    }
    #[test]
    fn cli_defaults_no_flags() {
        let cli = DiagnosticsCli::try_parse_from(s(&["diagnostics", "src/lib.rs"]).iter()).unwrap();
        assert!(!cli.with_fixes);
        assert!(!cli.json);
    }
}
