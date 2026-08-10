//! D2.1 — ToolOutputRouter: classifies tools for sandbox routing.
//!
//! Part of D2 PreToolUse Output Router (P0, XL).
//! Decision: PassThrough vs RouteToSandbox based on estimated output size.
//!
//! Reuses: OutputCapture::CAPTURE_THRESHOLD (output size concept).

use serde_json::Value;

/// Routing decision for a tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingDecision {
    /// Execute normally — output stays in-band.
    PassThrough,
    /// Execute in sandbox subprocess; return content_hash to LLM.
    RouteToSandbox,
}

/// Classifies a tool invocation routing path based on tool name + arguments.
///
/// Uses heuristics to estimate whether the tool will produce large output
/// that should be routed to sandbox execution + Tantivy storage.
///
/// # Arguments
/// * `tool_name` — e.g. "Bash", "Read", "Write"
/// * `tool_args` — parsed JSON arguments (from pre_tool_use input)
pub fn classify_tool_routing(tool_name: &str, tool_args: &Value) -> Option<RoutingDecision> {
    let threshold = crate::shared::feature_flags::routing_threshold_bytes();
    let estimated = estimate_output_size(tool_name, tool_args);
    if estimated > threshold {
        Some(RoutingDecision::RouteToSandbox)
    } else {
        Some(RoutingDecision::PassThrough)
    }
}

/// Estimates expected output bytes for a tool invocation.
///
/// Heuristic based on tool type and argument patterns:
/// - Bash with output-redirection args (grep -r, find, gh api, etc.) → large
/// - Read/Edit/Write → small (file content already in context)
/// - Grep/Glob with recursive flags → medium-large
///
/// # Arguments
/// * `tool_name` — e.g. "Bash", "Read", "Write"
/// * `tool_args` — parsed JSON arguments
pub fn estimate_output_size(tool_name: &str, tool_args: &Value) -> u64 {
    match tool_name {
        "Bash" => estimate_bash_output_size(tool_args),
        "Grep" => estimate_grep_output_size(tool_args),
        "Glob" => estimate_glob_output_size(tool_args),
        // Read/Edit/Write: small (file content in context, not large output)
        "Read" | "Edit" | "Write" | "WebFetch" => 512,
        _ => 1024, // default conservative small
    }
}

/// Heuristic for Bash tools with large-output indicators.
fn estimate_bash_output_size(args: &Value) -> u64 {
    let args_str = args.to_string();
    // Large-output patterns: recursive search, API calls, long listings
    let large_patterns = [
        "gh api ",
        "gh issue",
        "gh pr ",
        "git log",
        "git diff",
        "grep -r",
        "grep -l",
        "find .",
        "find /",
        "rg ",
        "ag ",
        "curl ",
        "wget ",
        "--json",
        "-l ",
        "-r ",
        "--recursive",
    ];
    let has_large = large_patterns.iter().any(|p| args_str.contains(p));
    if has_large {
        50_000 // heuristic: 50KB+ for large Bash commands
    } else {
        2048 // default: small
    }
}

/// Heuristic for Grep tool output size.
fn estimate_grep_output_size(args: &Value) -> u64 {
    let args_str = args.to_string();
    // Check for recursive flag in args string (-r, --recursive)
    let has_recursive_flag = args_str.contains("-r") || args_str.contains("--recursive");
    // Also check for `recursive: true` JSON field (field name + true value)
    let has_recursive_field =
        args_str.contains("\"recursive\":true") || args_str.contains("\"recursive\": true");
    let has_recursive = has_recursive_flag || has_recursive_field;
    let has_many = args_str.contains("-l") || args_str.contains("--files-with-matches");
    if has_recursive {
        30_000
    } else if has_many {
        10_000
    } else {
        2048
    }
}

/// Heuristic for Glob tool output size.
fn estimate_glob_output_size(args: &Value) -> u64 {
    let args_str = args.to_string();
    if args_str.contains("**") {
        20_000 // glob with recursion can hit many files
    } else {
        2048
    }
}

/// Builds sandbox-wrapped arguments for a tool invocation by **actually
/// running the subprocess** via the D2.2 executor and persisting the
/// captured output to the D2.3 Tantivy `tool_outputs` index.
///
/// On success the returned JSON contains the content_hash, summary and
/// exit_code so the LLM can address the cached output via
/// `touring_hooks::cli_handlers_mcp::ctx_retrieve` without re-running the tool.
///
/// On failure (subprocess error, missing index, feature disabled) the
/// function returns a JSON envelope with `ok: false` and the original
/// args echoed back — callers may then decide to fall back to direct
/// execution.
#[cfg(feature = "tantivy-fts")]
pub fn build_sandbox_wrapper_args(
    project_root: Option<&std::path::Path>,
    tool_name: &str,
    original_args: Value,
) -> Value {
    // S-13 cross-audit (2026-06-06): execute_and_store moved to the parent module
    // sandbox_output_store (tool-output storage); SandboxConfig stays in the gateway.
    use crate::sandbox_executor::SandboxConfig;
    use crate::sandbox_output_store::execute_and_store;
    let cfg = SandboxConfig {
        timeout_ms: crate::shared::feature_flags::sandbox_timeout_ms(),
        max_output_bytes: crate::shared::feature_flags::sandbox_max_output_bytes(),
        fallback_on_timeout: crate::shared::feature_flags::sandbox_fallback_on_timeout(),
    };
    match execute_and_store(project_root, tool_name, original_args.clone(), cfg) {
        Ok(res) => {
            let envelope = serde_json::json!({
                "_sandbox_routed": true,
                "ok": true,
                "tool_name": tool_name,
                "content_hash": res.content_hash,
                "exit_code": res.exit_code,
                "output_bytes": res.output_bytes,
                "was_truncated": res.was_truncated,
                "stored_path": res.stored_path
                    .as_ref()
                    .map(|p| p.display().to_string()),
            });
            // A2 (2026-08-08): the saving of a routed call is exactly
            // `captured output − the envelope the model gets instead`, and both
            // halves are known right here. `res.output_bytes` is what the
            // sandbox actually captured — the number `ctx_roi` used to
            // approximate with a flat 30_000 per event.
            crate::shared::gate_metrics::record_routing_savings(
                res.output_bytes,
                envelope.to_string().len() as u64,
            );
            envelope
        }
        Err(e) => serde_json::json!({
            "_sandbox_routed": false,
            "ok": false,
            "error": e.to_string(),
            "original_args": original_args,
        }),
    }
}

/// Turns a routing envelope into an input the ORIGINAL tool can actually
/// accept, or `None` when no safe conversion exists.
///
/// **2026-08-08 — the landmine this defuses.** `build_sandbox_wrapper_args`
/// returns a descriptive envelope (`content_hash`, `exit_code`, `stored_path`,
/// …) and the PreToolUse hook hands it back as `updatedInput`, which the
/// harness documents as *"Modified tool input to use"* — the tool then RUNS
/// with it. The envelope has no `command`, so registering this hook for `Bash`
/// would have broken every command matching the routing heuristics (`-r `,
/// `-l `, `--json`, `git log`, `grep -r`, `rg `, `curl `, `find .` — most of an
/// ordinary session). The subsystem was never registered, so the defect never
/// fired; it was a trap waiting for whoever enabled it.
///
/// The sandbox has ALREADY executed the command, so the replacement must not
/// re-run it (that would double any side effect). Printing the envelope is
/// side-effect-free, valid, and hands the model the hash it needs.
///
/// Returns `None` for tools whose input schema this cannot satisfy (Grep needs
/// `pattern`, Glob needs `pattern`, …) — the caller must then fall back to an
/// advisory-only response rather than substituting an input that cannot run.
#[must_use]
pub fn envelope_as_tool_input(tool_name: &str, envelope: &Value) -> Option<Value> {
    if tool_name != "Bash" {
        return None;
    }
    // Single-quote for POSIX sh: close, escape, reopen around each quote.
    let json = envelope.to_string();
    let quoted = json.replace('\'', r"'\''");
    Some(serde_json::json!({
        "command": format!("printf '%s\\n' '{quoted}'"),
        "description": "touring: cached sandbox result (command already executed)",
    }))
}

/// Fallback when the `tantivy-fts` feature is disabled — preserves the
/// original API shape so call-sites stay feature-agnostic.
///
/// 2026-08-07: essa promessa estava quebrada. Quando `project_root` entrou na
/// variante com a feature ligada, esta ficou com dois parâmetros, então todo
/// call site (3 argumentos) só compilava COM `tantivy-fts`. O workspace inteiro
/// mascarava a falha por unificação de features — `cargo check --workspace`
/// passa, e um `cargo nextest -p touring-intelligence -p touring-storage -p
/// touring-hooks-core` quebra com E0061. Manter as duas assinaturas idênticas é
/// a razão de existir deste par gated.
#[cfg(not(feature = "tantivy-fts"))]
pub fn build_sandbox_wrapper_args(
    _project_root: Option<&std::path::Path>,
    _tool_name: &str,
    original_args: Value,
) -> Value {
    serde_json::json!({
        "_sandbox_routed": false,
        "ok": false,
        "error": "tantivy-fts feature disabled — sandbox storage unavailable",
        "original_args": original_args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── envelope_as_tool_input: a mina desarmada (2026-08-08) ───────────────

    /// O envelope NUNCA pode ser devolvido como input da Bash: ele não tem
    /// `command`, e o harness roda a tool com o que o hook devolver.
    #[test]
    fn the_raw_envelope_is_never_a_valid_bash_input() {
        let envelope = json!({
            "_sandbox_routed": true, "ok": true, "tool_name": "Bash",
            "content_hash": "abc", "exit_code": 0, "output_bytes": 8172,
            "was_truncated": false, "stored_path": "/tmp/abc.bin",
        });
        assert!(
            envelope.get("command").is_none(),
            "premissa do teste: o envelope não é um input de Bash"
        );
        let input = envelope_as_tool_input("Bash", &envelope).expect("Bash tem conversão");
        let cmd = input["command"].as_str().expect("command obrigatório");
        assert!(!cmd.is_empty());
        // Não pode RE-executar nada: o sandbox já rodou o comando original.
        assert!(cmd.starts_with("printf "), "got: {cmd}");
        assert!(cmd.contains("abc"), "o hash tem de chegar ao modelo: {cmd}");
    }

    /// Aspas simples no envelope não podem quebrar o quoting POSIX.
    ///
    /// Verificado EXECUTANDO o comando gerado: contar barras invertidas prova
    /// nada sobre quoting (a primeira versão deste teste fazia isso e reprovou
    /// por aritmética, não por defeito). O contrato é observável — o comando
    /// tem de imprimir o envelope de volta, byte a byte.
    #[test]
    fn single_quotes_in_the_envelope_survive_a_real_shell() {
        let envelope = json!({"stored_path": "/tmp/it's here.bin", "content_hash": "x'y"});
        let cmd = envelope_as_tool_input("Bash", &envelope).expect("conversão")["command"]
            .as_str()
            .expect("command")
            .to_string();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .output()
            .expect("sh");
        assert!(out.status.success(), "comando gerado não roda: {cmd}");
        let printed = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            printed.trim_end(),
            envelope.to_string(),
            "o shell tem de devolver o envelope intacto"
        );
    }

    /// Ferramentas cujo schema não sabemos satisfazer ficam SEM substituição —
    /// advisory é seguro, um input inválido não é.
    #[test]
    fn tools_without_a_known_input_schema_get_no_substitution() {
        let envelope = json!({"content_hash": "abc"});
        for tool in ["Grep", "Glob", "WebFetch", "Read"] {
            assert!(
                envelope_as_tool_input(tool, &envelope).is_none(),
                "{tool} não tem conversão conhecida — tem de ficar advisory"
            );
        }
    }

    #[test]
    fn test_pass_through_small_bash() {
        let args = json!({"command": "echo hello"});
        let decision = classify_tool_routing("Bash", &args).unwrap();
        assert_eq!(decision, RoutingDecision::PassThrough);
    }

    #[test]
    fn test_route_large_bash_gh_api() {
        let args = json!({"command": "gh api repos"});
        let decision = classify_tool_routing("Bash", &args).unwrap();
        assert_eq!(decision, RoutingDecision::RouteToSandbox);
    }

    #[test]
    fn test_route_large_bash_grep_recursive() {
        let args = json!({"pattern": "TODO", "path": ".", "recursive": true});
        let decision = classify_tool_routing("Grep", &args).unwrap();
        assert_eq!(decision, RoutingDecision::RouteToSandbox);
    }

    #[test]
    fn test_pass_through_read() {
        let args = json!({"file_path": "src/main.rs"});
        let decision = classify_tool_routing("Read", &args).unwrap();
        assert_eq!(decision, RoutingDecision::PassThrough);
    }

    #[test]
    fn test_estimate_grep_recursive() {
        let args = json!({"pattern": "TODO", "path": ".", "recursive": true});
        let size = estimate_output_size("Grep", &args);
        assert!(size > 10_000);
    }

    #[test]
    fn test_estimate_grep_simple() {
        let args = json!({"pattern": "TODO", "path": "src"});
        let size = estimate_output_size("Grep", &args);
        assert!(size < 5000);
    }

    #[test]
    fn test_estimate_glob_recursive() {
        let args = json!({"pattern": "**/*.rs"});
        let size = estimate_output_size("Glob", &args);
        assert!(size > 10_000);
    }

    #[test]
    fn test_sandbox_wrapper_args_returns_envelope() {
        let original = json!({"command": "echo wrapper-shape-only"});
        let wrapped = build_sandbox_wrapper_args(None, "Bash", original.clone());
        // Envelope must always carry these structural fields so callers
        // (HookResponse::ContextWithUpdatedInput) get a stable shape.
        assert!(wrapped.get("_sandbox_routed").is_some());
        assert!(wrapped.get("ok").is_some());
    }
}
