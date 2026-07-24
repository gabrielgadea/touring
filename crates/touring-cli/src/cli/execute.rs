//! `cli-ctx-execute` handler (Master Plan A.W2.P5 extraction).
//!
//! Mechanical extraction from `cli_handlers.rs`. Dispatches a source
//! snippet to the sandboxed-subprocess substrate
//! (`execute_in_sandbox_blocking`) and increments the
//! `record_ctx_execute_file_count` observability counter on success.

use crate::runtime::HookRuntime;

/// `cli-ctx-execute` — production entry point for §2.1.1 PoT
/// (Program-of-Thought, arXiv 2605.18747). Closes the 5th orphan counter
/// (P3-NO-OP pattern) by adding a real production callsite for `ctx_execute`:
/// increments `record_ctx_execute_file_count` (gate_metrics.rs:2504) on every
/// successful invocation.
///
/// The handler dispatches through `execute_in_sandbox_blocking` (the canonical
/// sandboxed-subprocess path), passing the language-specific `Sandbox<Lang>`
/// tool name + the script body in the `script` field (the substrate's
/// contract — see `resolve_args` in sandbox_executor.rs:230-254, which maps
/// `Sandbox<Lang>` + `script` field to the language runtime argv).
///
/// Payload: `{ "language": "python"|"rust"|"go"|"javascript"|...,
///             "script": "<source>" }`.
/// Returns JSON: `{ok, language, exit_code, output_bytes, was_truncated,
/// content_hash, stored_path, error?}`.
///
/// Constitutional CODE constraints:
/// - type hints on every signature
/// - specific `SandboxError` / typed error, never bare except
/// - SRP: `cli_ctx_execute` orchestrates, `parse_ctx_execute_payload` only
///   parses, `format_ctx_execute_error` only formats
pub fn cli_ctx_execute(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use crate::sandbox_executor::{SandboxConfig, execute_in_sandbox_blocking};

    // SRP step 1: strict parse (typed error, no bare except). Note: language
    // label is owned String to avoid borrowing from the input payload (lifetime
    // would otherwise outlive `'static` since we return a tuple of refs).
    let (tool_name, language_label): (&'static str, String) =
        match parse_ctx_execute_payload(payload) {
            Ok(v) => v,
            Err(e) => return format_ctx_execute_error(&e),
        };

    // SRP step 2: build substrate args. The substrate expects a `script`
    // field (resolve_args at sandbox_executor.rs:230-254 maps Sandbox<Lang>
    // tool_name + script → language runtime argv via resolve_language_args).
    let script: String = payload
        .get("script")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let args: serde_json::Value = serde_json::json!({ "script": script });

    // SRP step 3: execute via the substrate (default SandboxConfig: 30s timeout,
    // 1 MiB max output, fallback_on_timeout=true).
    let config: SandboxConfig = SandboxConfig::default();
    match execute_in_sandbox_blocking(tool_name, args, config) {
        Ok(result) => {
            // SRP step 4: increment the 5th orphan counter (P3-NO-OP close).
            // From 0 → >0 means a real production callsite now exercises the
            // substrate; `touring gate-metrics -j` will show the increment.
            crate::shared::gate_metrics::record_ctx_execute_file_count();
            serde_json::json!({
                "ok": true,
                "language": language_label,
                "exit_code": result.exit_code,
                "output_bytes": result.output_bytes,
                "was_truncated": result.was_truncated,
                "content_hash": result.content_hash,
                "stored_path": result.stored_path.map(|p| p.display().to_string()),
            })
            .to_string()
        }
        Err(e) => format_ctx_execute_error(&format!("{e}")),
    }
}

/// Strict payload validation for `cli_ctx_execute`. SRP: only parsing, no
/// execution. Returns the substrate's `Sandbox<Lang>` tool name (always
/// `'static` — it is a string literal) + the human-readable language label
/// (owned `String` to avoid the lifetime constraint of borrowing from
/// `payload`).
///
/// Language mapping (canonical from `resolve_args` at sandbox_executor.rs:230):
/// - `python|py → SandboxPython`  (runtime: python3, argv: -c script)
/// - `rust|rs → SandboxRust`       (runtime: rustc, compile-and-run path)
/// - `javascript|js → SandboxJavaScript` (runtime: bun/node, argv: -e script)
/// - `typescript|ts → SandboxTypeScript` (runtime: bun, argv: -e script)
/// - `ruby|rb → SandboxRuby`       (runtime: ruby, argv: -e script)
/// - `go → SandboxGo`             (runtime: go, compile-and-run path)
/// - `php → SandboxPhp`           (runtime: php, argv: -r script)
/// - `perl|pl → SandboxPerl`      (runtime: perl, argv: -e script)
/// - `r → SandboxR`               (runtime: Rscript, argv: -e script)
/// - `elixir|ex → SandboxElixir`  (runtime: elixir, argv: -e script)
/// - `shell|sh|bash → Bash`        (special: Bash + script → Shell, argv: -c script)
fn parse_ctx_execute_payload(
    payload: &serde_json::Value,
) -> Result<(&'static str, String), String> {
    let language: &str = payload
        .get("language")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "payload must carry a 'language' string".to_string())?;
    if payload.get("script").and_then(|v| v.as_str()).is_none() {
        return Err("payload must carry a 'script' string".to_string());
    }
    let normalized: String = language.to_ascii_lowercase();
    let tool_name: &'static str = match normalized.as_str() {
        "python" | "py" => "SandboxPython",
        "rust" | "rs" => "SandboxRust",
        "javascript" | "js" => "SandboxJavaScript",
        "typescript" | "ts" => "SandboxTypeScript",
        "ruby" | "rb" => "SandboxRuby",
        "go" => "SandboxGo",
        "php" => "SandboxPhp",
        "perl" | "pl" => "SandboxPerl",
        "r" => "SandboxR",
        "elixir" | "ex" => "SandboxElixir",
        // Shell: special — Bash + script field is the ctx_execute shell path
        "shell" | "sh" | "bash" => "Bash",
        _ => return Err(format!("unsupported language: {language}")),
    };
    Ok((tool_name, language.to_string()))
}

/// Format a typed-error response. SRP: only formatting, no logic.
fn format_ctx_execute_error(error: &str) -> String {
    serde_json::json!({
        "ok": false,
        "error": error,
    })
    .to_string()
}

#[cfg(test)]
mod tests_ctx_execute {
    use super::*;

    /// P5-EXTENSION test: all 11 supported languages map to the right
    /// substrate tool name (canonical mapping from `resolve_args`).
    #[test]
    fn parse_ctx_execute_payload_language_mapping_canonical() {
        let cases: &[(&str, &str, &str)] = &[
            ("python", "print(1)", "SandboxPython"),
            ("py", "print(1)", "SandboxPython"),
            ("rust", "fn main(){}", "SandboxRust"),
            ("rs", "fn main(){}", "SandboxRust"),
            ("javascript", "console.log(1)", "SandboxJavaScript"),
            ("js", "console.log(1)", "SandboxJavaScript"),
            ("typescript", "console.log(1)", "SandboxTypeScript"),
            ("ts", "console.log(1)", "SandboxTypeScript"),
            ("ruby", "puts 1", "SandboxRuby"),
            ("rb", "puts 1", "SandboxRuby"),
            ("go", "package main", "SandboxGo"),
            ("php", "echo 1;", "SandboxPhp"),
            ("perl", "print 1;", "SandboxPerl"),
            ("pl", "print 1;", "SandboxPerl"),
            ("r", "print(1)", "SandboxR"),
            ("elixir", "IO.puts 1", "SandboxElixir"),
            ("ex", "IO.puts 1", "SandboxElixir"),
            // Shell: special "Bash" routing
            ("shell", "echo 1", "Bash"),
            ("sh", "echo 1", "Bash"),
            ("bash", "echo 1", "Bash"),
        ];
        for (lang, script, expected_tool) in cases {
            let payload = serde_json::json!({ "language": lang, "script": script });
            let (tool, _label) = parse_ctx_execute_payload(&payload)
                .unwrap_or_else(|e| panic!("language={lang} should parse: {e}"));
            assert_eq!(tool, *expected_tool, "language={lang}");
        }
    }

    /// P5-EXTENSION test: typed errors (never bare except) for malformed input.
    #[test]
    fn parse_ctx_execute_payload_rejects_malformed() {
        // missing language
        let p1 = serde_json::json!({ "script": "x" });
        let e1 = parse_ctx_execute_payload(&p1).unwrap_err();
        assert!(
            e1.contains("language"),
            "expected 'language' in error: {e1}"
        );

        // missing script
        let p2 = serde_json::json!({ "language": "python" });
        let e2 = parse_ctx_execute_payload(&p2).unwrap_err();
        assert!(e2.contains("script"), "expected 'script' in error: {e2}");

        // unsupported language
        let p3 = serde_json::json!({ "language": "kotlin", "script": "x" });
        let e3 = parse_ctx_execute_payload(&p3).unwrap_err();
        assert!(
            e3.contains("unsupported"),
            "expected 'unsupported' in error: {e3}"
        );
        assert!(
            e3.contains("kotlin"),
            "expected language name in error: {e3}"
        );
    }
}
