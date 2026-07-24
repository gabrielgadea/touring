//! `touring context compile` — Context compiler CLI.
//!
//! Reads optional `--intent`, `--files`, and `--max-tokens` from args.
//! Also reads JSON from stdin for gotchas/relations/errors enrichment.

use super::read_stdin_safe;

/// Parsed CLI arguments for the context compile command.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContextArgs {
    pub intent: String,
    pub files: Vec<String>,
    pub max_tokens: usize,
}

impl Default for ContextArgs {
    fn default() -> Self {
        Self {
            intent: String::from("subagent task"),
            files: Vec::new(),
            max_tokens: 2000,
        }
    }
}

/// Parse CLI flags from args slice. Starts scanning at index 2 (skipping "touring" "context").
pub(crate) fn parse_args(args: &[String]) -> ContextArgs {
    let mut result = ContextArgs::default();

    let mut i = 2; // skip "touring" "context"
    // Skip optional "compile" sub-subcommand
    if args.get(i).map(|s| s.as_str()) == Some("compile") {
        i += 1;
    }
    while i < args.len() {
        let Some(arg) = args.get(i) else { break };
        match arg.as_str() {
            "--intent" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    result.intent = v.clone();
                }
            }
            "--files" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    result.files = v
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
            "--max-tokens" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    result.max_tokens = v.parse().unwrap_or(2000);
                }
            }
            _ => {} // ignore unknown flags
        }
        i += 1;
    }
    result
}

/// Parse enrichment data (gotchas, relations, recent_errors) from a JSON value.
pub(crate) fn parse_enrichment(
    data: &serde_json::Value,
) -> (Vec<(String, String)>, Vec<String>, Vec<String>) {
    let gotchas: Vec<(String, String)> = data
        .get("gotchas")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let sev = item.get("severity")?.as_str()?.to_string();
                    let desc = item.get("description")?.as_str()?.to_string();
                    Some((sev, desc))
                })
                .collect()
        })
        .unwrap_or_default();

    let relations: Vec<String> = data
        .get("relations")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let recent_errors: Vec<String> = data
        .get("recent_errors")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    (gotchas, relations, recent_errors)
}

/// Entry point for the `touring context compile` CLI handler — parses
/// `--intent`/`--files`/`--max-tokens` flags, reads optional gotchas/relations/errors
/// enrichment JSON from stdin, runs the context compiler under the token budget, and
/// prints the compiled context as JSON.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let parsed = parse_args(args);

    // Read enrichment data from stdin (optional JSON)
    let data = read_stdin_safe();
    let (gotchas, relations, recent_errors) = parse_enrichment(&data);

    let compiled = crate::context_compiler::compile_context(
        &parsed.intent,
        &parsed.files,
        &gotchas,
        &relations,
        &recent_errors,
        parsed.max_tokens,
    );

    println!("{}", serde_json::to_string(&compiled)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_args_defaults_when_no_flags() {
        let parsed = parse_args(&args(&["touring", "context"]));
        assert_eq!(parsed, ContextArgs::default());
        assert_eq!(parsed.intent, "subagent task");
        assert!(parsed.files.is_empty());
        assert_eq!(parsed.max_tokens, 2000);
    }

    #[test]
    fn parse_args_intent_flag() {
        let parsed = parse_args(&args(&["touring", "context", "--intent", "debug session"]));
        assert_eq!(parsed.intent, "debug session");
    }

    #[test]
    fn parse_args_files_flag_comma_separated() {
        let parsed = parse_args(&args(&[
            "touring",
            "context",
            "--files",
            "a.rs, b.rs, c.rs",
        ]));
        assert_eq!(parsed.files, vec!["a.rs", "b.rs", "c.rs"]);
    }

    #[test]
    fn parse_args_files_flag_filters_empty_entries() {
        let parsed = parse_args(&args(&["touring", "context", "--files", "a.rs,,b.rs,"]));
        assert_eq!(parsed.files, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn parse_args_max_tokens_flag() {
        let parsed = parse_args(&args(&["touring", "context", "--max-tokens", "5000"]));
        assert_eq!(parsed.max_tokens, 5000);
    }

    #[test]
    fn parse_args_max_tokens_invalid_falls_back_to_default() {
        let parsed = parse_args(&args(&[
            "touring",
            "context",
            "--max-tokens",
            "not_a_number",
        ]));
        assert_eq!(parsed.max_tokens, 2000);
    }

    #[test]
    fn parse_args_all_flags_combined() {
        let parsed = parse_args(&args(&[
            "touring",
            "context",
            "compile",
            "--intent",
            "refactor",
            "--files",
            "main.rs,lib.rs",
            "--max-tokens",
            "1000",
        ]));
        assert_eq!(parsed.intent, "refactor");
        assert_eq!(parsed.files, vec!["main.rs", "lib.rs"]);
        assert_eq!(parsed.max_tokens, 1000);
    }

    #[test]
    fn parse_args_skips_compile_subsubcommand() {
        let with_compile = parse_args(&args(&[
            "touring", "context", "compile", "--intent", "test",
        ]));
        let without_compile = parse_args(&args(&["touring", "context", "--intent", "test"]));
        assert_eq!(with_compile.intent, without_compile.intent);
    }

    #[test]
    fn parse_args_ignores_unknown_flags() {
        let parsed = parse_args(&args(&[
            "touring",
            "context",
            "--unknown",
            "value",
            "--intent",
            "ok",
        ]));
        assert_eq!(parsed.intent, "ok");
    }

    #[test]
    fn parse_enrichment_empty_json() {
        let data = serde_json::json!({});
        let (gotchas, relations, errors) = parse_enrichment(&data);
        assert!(gotchas.is_empty());
        assert!(relations.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn parse_enrichment_gotchas() {
        let data = serde_json::json!({
            "gotchas": [
                {"severity": "high", "description": "null pointer risk"},
                {"severity": "low", "description": "unused import"}
            ]
        });
        let (gotchas, _, _) = parse_enrichment(&data);
        assert_eq!(gotchas.len(), 2);
        assert_eq!(
            gotchas[0],
            ("high".to_string(), "null pointer risk".to_string())
        );
        assert_eq!(gotchas[1], ("low".to_string(), "unused import".to_string()));
    }

    #[test]
    fn parse_enrichment_relations_and_errors() {
        let data = serde_json::json!({
            "relations": ["mod_a -> mod_b", "mod_c -> mod_d"],
            "recent_errors": ["E0308: type mismatch"]
        });
        let (_, relations, errors) = parse_enrichment(&data);
        assert_eq!(relations.len(), 2);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0], "E0308: type mismatch");
    }

    #[test]
    fn parse_enrichment_skips_malformed_gotchas() {
        let data = serde_json::json!({
            "gotchas": [
                {"severity": "high"},
                {"description": "missing severity"},
                {"severity": "low", "description": "valid entry"}
            ]
        });
        let (gotchas, _, _) = parse_enrichment(&data);
        // Only the third entry has both fields
        assert_eq!(gotchas.len(), 1);
        assert_eq!(gotchas[0].0, "low");
    }
}
