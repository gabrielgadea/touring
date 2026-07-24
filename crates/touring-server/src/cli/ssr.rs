//! CLI `touring ssr` — Structural Search & Replace.
//!
//! `touring ssr status`                     — list prebuilt rules
//! `touring ssr apply --pattern X --replacement Y [--lang L] [--stdin]`

use std::io::Read;
use std::str::FromStr;

use serde::Serialize;

use touring_code::ast::ssr::{self, SsrApplyResult, SsrRule};

/// JSON output of `touring ssr status` summarising available SSR capabilities.
#[derive(Debug, Serialize)]
pub struct SsrStatus {
    /// Number of prebuilt structural-search-and-replace rules bundled with `touring`.
    pub prebuilt_rules: usize,
    /// Language identifiers SSR can parse and rewrite (e.g. `rust`, `python`).
    pub supported_languages: Vec<&'static str>,
}

/// JSON output of `touring ssr apply` reporting the result of a rewrite.
#[derive(Debug, Serialize)]
pub struct SsrApplyOutput {
    /// Identifier of the SSR rule that was applied (e.g. `cli-rule` for ad-hoc CLI rules).
    pub rule_id: String,
    /// Path of the rewritten file, or empty for stdin-sourced input.
    pub file_path: String,
    /// Number of pattern matches that were rewritten.
    pub matches: usize,
    /// Whether the rewritten output was re-formatted after substitution.
    pub was_formatted: bool,
    /// Rewritten source text after applying the rule. In `--stdin` mode this is
    /// the whole point of the command — callers need the transformed code, not
    /// just the match count. (A10)
    pub rewritten: String,
}

impl From<SsrApplyResult> for SsrApplyOutput {
    fn from(r: SsrApplyResult) -> Self {
        Self {
            rule_id: r.rule_id,
            file_path: r.file_path,
            matches: r.matches,
            was_formatted: r.was_formatted,
            rewritten: r.output,
        }
    }
}

/// `touring ssr status` — return prebuilt rule count and supported languages.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    // args[0] = "ssr" (subcommand from main dispatch)
    // args[1] = sub-subcommand: "status" | "apply" | ...
    // args[2..] = args for the sub-subcommand
    match args.get(2).map(|s| s.as_str()) {
        Some("status") => ssr_status(),
        Some("apply") => ssr_apply(&args[3..]),
        None => {
            eprintln!("usage: touring ssr status");
            eprintln!(
                "       touring ssr apply --pattern <pat> --replacement <repl> [--lang <l>] [--stdin]"
            );
            std::process::exit(1);
        }
        Some(cmd) => {
            eprintln!("unknown ssr subcommand: {cmd}");
            std::process::exit(1);
        }
    }
}

fn ssr_status() -> anyhow::Result<()> {
    let rules = ssr::prebuilt_rules();
    let status = SsrStatus {
        prebuilt_rules: rules.len(),
        supported_languages: vec![
            "rust",
            "python",
            "javascript",
            "typescript",
            "go",
            "java",
            "bash",
        ],
    };
    let json = serde_json::to_string_pretty(&status).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("{json}");
    Ok(())
}

fn ssr_apply(args: &[String]) -> anyhow::Result<()> {
    let mut pattern = None;
    let mut replacement = None;
    let mut lang = Some("rust".to_string());
    let mut stdin_source: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--pattern" => {
                pattern = args.get(i + 1).cloned();
                i += 2;
            }
            "--replacement" => {
                replacement = args.get(i + 1).cloned();
                i += 2;
            }
            "--lang" => {
                lang = args.get(i + 1).cloned();
                i += 2;
            }
            "--stdin" => {
                // read stdin and use as source
                let mut s = String::new();
                std::io::stdin()
                    .read_to_string(&mut s)
                    .map_err(|e| anyhow::anyhow!("stdin read error: {e}"))?;
                stdin_source = Some(s);
                i += 1;
            }
            _ => {
                eprintln!("unknown flag: {}", args[i]);
                std::process::exit(1);
            }
        }
    }

    let pattern = pattern.ok_or_else(|| anyhow::anyhow!("--pattern is required"))?;
    let replacement = replacement.ok_or_else(|| anyhow::anyhow!("--replacement is required"))?;

    let rule = SsrRule {
        id: "cli-rule".to_string(),
        lang: lang.unwrap_or_else(|| "rust".to_string()),
        pattern,
        replacement,
        file_path: None,
    };

    let lang_str = &rule.lang;
    let ast_lang = touring_code::ast::Lang::from_str(lang_str)
        .map_err(|_| anyhow::anyhow!("unsupported language: {}", lang_str))?;

    let src = stdin_source.ok_or_else(|| anyhow::anyhow!("--stdin required for apply"))?;

    let result = ssr::apply_ssr_rule(&rule, &src, ast_lang)
        .map_err(|e| anyhow::anyhow!("SSR apply failed: {e}"))?;

    let out: SsrApplyOutput = result.into();
    let json = serde_json::to_string_pretty(&out).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("{json}");
    Ok(())
}
