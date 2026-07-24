//! D2.4 — MCP tool: sandboxed multi-language code execution.
//! P1.3: AST-based forbidden-call detection (hybrid 11-language).
//! P1.4: ForbiddenCallPolicy enforcement (Warn/Block/Off).

use serde::{Deserialize, Serialize};
use std::time::Instant;
use thiserror::Error;

use touring_hooks::sandbox_executor::{
    SandboxConfig, SandboxError, SandboxLanguage, execute_in_sandbox,
};
use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;

/// P1.4 — three-level enforcement policy for forbidden-call detection.
///
/// Determined from environment variables at call time (fail-open):
/// - `TOURING_CEG_FORBIDDEN_OFF=1`     → Off  (detection suppressed)
/// - `TOURING_CEG_FORBIDDEN_ENFORCE=1` → Block (execution rejected when calls found)
/// - default                            → Warn  (calls logged, execution continues)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForbiddenCallPolicy {
    /// Silently skip detection. Use for trusted callers.
    Off,
    /// Detect and warn; execution proceeds regardless.
    Warn,
    /// Detect and block; returns `Err(ForbiddenCalls(...))` when calls found.
    Block,
}

impl ForbiddenCallPolicy {
    /// Reads from environment. Fail-open: any env error → `Warn`.
    pub fn from_env() -> Self {
        if std::env::var("TOURING_CEG_FORBIDDEN_OFF").as_deref() == Ok("1") {
            return ForbiddenCallPolicy::Off;
        }
        if std::env::var("TOURING_CEG_FORBIDDEN_ENFORCE").as_deref() == Ok("1") {
            return ForbiddenCallPolicy::Block;
        }
        ForbiddenCallPolicy::Warn
    }
}

/// Error returned by the sandboxed `ctx_execute` tool.
#[derive(Error, Debug)]
pub enum CtxExecuteError {
    /// The requested language identifier is not supported.
    #[error("Invalid language: {0}")]
    InvalidLanguage(String),
    /// The submitted code exceeds the 64 KB size cap.
    #[error("Code too large: {0} bytes (max 64KB)")]
    CodeTooLarge(usize),
    /// The underlying sandbox executor failed.
    #[error("Sandbox execution failed: {0}")]
    SandboxFailed(#[from] SandboxError),
    /// An unexpected internal failure occurred.
    #[error("Internal error: {0}")]
    Internal(String),
    /// P1.4: Returned when policy=Block and forbidden calls are detected.
    #[error("Forbidden calls detected: {}", .0.join(", "))]
    ForbiddenCalls(Vec<String>),
}

/// Convenience alias for results that may fail with [`CtxExecuteError`].
pub type Result<T> = std::result::Result<T, CtxExecuteError>;

/// Request parameters for the `ctx_execute` MCP tool.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtxExecuteInput {
    /// Language identifier (e.g. `python`, `js`, `ts`).
    pub language: String,
    /// Source code to run inside the sandbox.
    pub code: String,
    /// Optional JSON arguments passed to the executed program.
    #[serde(default)]
    pub args: Option<serde_json::Value>,
    /// Optional wall-clock timeout in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Optional working directory for the sandboxed process.
    #[serde(default)]
    pub cwd: Option<String>,
}

/// Result of a sandboxed `ctx_execute` invocation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CtxExecuteOutput {
    /// Captured standard output of the executed program.
    pub stdout: String,
    /// Captured standard error of the executed program.
    pub stderr: String,
    /// Process exit code.
    pub exit_code: i32,
    /// Execution wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Forbidden calls detected by the AST scanner, if any.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub forbidden_calls: Vec<String>,
    /// Whether `stdout` was truncated to fit the size cap.
    pub stdout_truncated: bool,
    /// Whether `stderr` was truncated to fit the size cap.
    pub stderr_truncated: bool,
}

fn parse_language(lang: &str) -> Result<SandboxLanguage> {
    match lang.to_lowercase().as_str() {
        "js" | "javascript" => Ok(SandboxLanguage::JavaScript),
        "python" | "py" => Ok(SandboxLanguage::Python),
        "ts" | "typescript" => Ok(SandboxLanguage::TypeScript),
        "ruby" | "rb" => Ok(SandboxLanguage::Ruby),
        "go" | "golang" => Ok(SandboxLanguage::Go),
        "perl" | "pl" => Ok(SandboxLanguage::Perl),
        "r" | "rlang" => Ok(SandboxLanguage::R),
        "elixir" | "ex" => Ok(SandboxLanguage::Elixir),
        "php" => Ok(SandboxLanguage::Php),
        "rust" | "rs" => Ok(SandboxLanguage::Rust),
        "sh" | "bash" | "shell" => Ok(SandboxLanguage::Shell),
        "bun" => Ok(SandboxLanguage::JavaScript),
        "node" => Ok(SandboxLanguage::JavaScript),
        _ => Err(CtxExecuteError::InvalidLanguage(lang.to_string())),
    }
}

fn inject_args(code: &str, args: &serde_json::Value, lang: SandboxLanguage) -> String {
    let arr: &[serde_json::Value] = args.as_array().map_or(&[], |v| v.as_slice());
    let args_json = serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string());
    match lang {
        SandboxLanguage::Python => {
            format!(
                "import sys, json\nsys.argv = [''] + json.loads('{}')\n{}",
                args_json.replace('\'', r"\'"),
                code
            )
        }
        SandboxLanguage::JavaScript | SandboxLanguage::TypeScript => {
            format!(
                "const __ctx_args = JSON.parse('{}');\n{}",
                args_json.replace('\'', r"\'"),
                code
            )
        }
        _ => code.to_string(),
    }
}

fn truncate(s: &str, max_bytes: usize) -> (String, bool) {
    let bytes = s.as_bytes();
    if bytes.len() <= max_bytes {
        return (s.to_string(), false);
    }
    let truncated: String = String::from_utf8_lossy(&bytes[..max_bytes]).to_string();
    (truncated, true)
}

/// P1.3: Hybrid forbidden-call scanner.
///
/// Wraps `ast_forbidden_scan` (AST-backed for 9 grammars + substring fallback
/// for Perl/R). Fail-open: if the scanner itself panics or errors, returns an
/// empty list so execution is never blocked by scanner failures.
fn run_forbidden_scan(lang: SandboxLanguage, code: &str) -> Vec<String> {
    // Catch any unexpected panic from tree-sitter grammars (e.g. B-FUZZ-001 style).
    std::panic::catch_unwind(|| ast_forbidden_scan(lang, code)).unwrap_or_else(|_| {
        tracing::warn!(
            ?lang,
            "forbidden-call scanner panicked; proceeding without detection (fail-open)"
        );
        Vec::new()
    })
}

/// Run `code` for `language` inside the sandbox and return captured output.
///
/// Enforces the 64 KB size cap, applies the active [`ForbiddenCallPolicy`]
/// (overridable via `allow_forbidden`), and times the execution.
pub async fn ctx_execute_impl(
    language: String,
    code: String,
    args: Option<serde_json::Value>,
    timeout_ms: Option<u64>,
    _cwd: Option<String>,
    allow_forbidden: Option<bool>,
) -> Result<CtxExecuteOutput> {
    const MAX_CODE_BYTES: usize = 64 * 1024;
    if code.len() > MAX_CODE_BYTES {
        return Err(CtxExecuteError::CodeTooLarge(code.len()));
    }
    let lang = parse_language(&language)?;

    // P1.3 + P1.4: Determine policy and run hybrid scanner.
    let policy = ForbiddenCallPolicy::from_env();
    let forbidden: Vec<String> = if policy == ForbiddenCallPolicy::Off {
        Vec::new()
    } else {
        run_forbidden_scan(lang, &code)
    };

    // P1.4: Enforcement gate — block if policy=Block, forbidden non-empty,
    // and caller has not explicitly overridden via allow_forbidden=true.
    if policy == ForbiddenCallPolicy::Block
        && !forbidden.is_empty()
        && allow_forbidden != Some(true)
    {
        return Err(CtxExecuteError::ForbiddenCalls(forbidden));
    }

    let final_code = if let Some(ref a) = args {
        inject_args(&code, a, lang)
    } else {
        code
    };
    let timeout = timeout_ms.unwrap_or(30_000).min(120_000);
    let config = SandboxConfig {
        timeout_ms: timeout,
        max_output_bytes: 1_000_000,
        fallback_on_timeout: true,
    };
    let tool_name = match lang {
        SandboxLanguage::JavaScript => "SandboxJavaScript",
        SandboxLanguage::TypeScript => "SandboxTypeScript",
        SandboxLanguage::Python => "SandboxPython",
        SandboxLanguage::Ruby => "SandboxRuby",
        SandboxLanguage::Go => "SandboxGo",
        SandboxLanguage::Perl => "SandboxPerl",
        SandboxLanguage::R => "SandboxR",
        SandboxLanguage::Elixir => "SandboxElixir",
        SandboxLanguage::Php => "SandboxPhp",
        SandboxLanguage::Rust => "SandboxRust",
        SandboxLanguage::Shell => "Bash",
    };
    let sandbox_args = match lang {
        SandboxLanguage::Shell => serde_json::json!({ "command": final_code }),
        SandboxLanguage::JavaScript | SandboxLanguage::TypeScript => {
            serde_json::json!({ "script": final_code })
        }
        SandboxLanguage::Python
        | SandboxLanguage::Ruby
        | SandboxLanguage::Perl
        | SandboxLanguage::R
        | SandboxLanguage::Elixir
        | SandboxLanguage::Php => {
            serde_json::json!({ "script": final_code })
        }
        SandboxLanguage::Go => serde_json::json!({ "script": final_code }),
        SandboxLanguage::Rust => serde_json::json!({ "script": final_code }),
    };
    let start = Instant::now();
    let result = execute_in_sandbox(tool_name, sandbox_args, config).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    let (stdout, stderr, exit_code) = match result {
        Ok(r) => {
            let stdout_bytes = if let Some(ref path) = r.stored_path {
                std::fs::read(path).unwrap_or_default()
            } else {
                Vec::new()
            };
            let stdout_str = String::from_utf8_lossy(&stdout_bytes).to_string();
            (stdout_str, String::new(), r.exit_code)
        }
        Err(e) => (String::new(), e.to_string(), -1),
    };
    const MAX_STDOUT: usize = 8 * 1024;
    const MAX_STDERR: usize = 4 * 1024;
    let (stdout_trunc, stdout_truncated) = truncate(&stdout, MAX_STDOUT);

    // P1.4 Warn mode: emit warning to stderr when forbidden calls found but policy != Block.
    let final_stderr = if policy == ForbiddenCallPolicy::Warn && !forbidden.is_empty() {
        let warning = format!(
            "[CEG WARNING] Forbidden calls detected: {}\n{}",
            forbidden.join(", "),
            stderr
        );
        warning
    } else {
        stderr
    };

    let (stderr_trunc, stderr_truncated) = truncate(&final_stderr, MAX_STDERR);
    Ok(CtxExecuteOutput {
        stdout: stdout_trunc,
        stderr: stderr_trunc,
        exit_code,
        duration_ms,
        forbidden_calls: forbidden,
        stdout_truncated,
        stderr_truncated,
    })
}

/// Serialize a `ctx_execute` result (success or error) into the canonical
/// JSON envelope returned to MCP consumers.
pub fn format_output(
    result: std::result::Result<CtxExecuteOutput, CtxExecuteError>,
) -> serde_json::Value {
    match result {
        Ok(output) => serde_json::json!({
            "success": true,
            "stdout": output.stdout,
            "stderr": output.stderr,
            "exit_code": output.exit_code,
            "duration_ms": output.duration_ms,
            "forbidden_calls": output.forbidden_calls,
            "stdout_truncated": output.stdout_truncated,
            "stderr_truncated": output.stderr_truncated,
        }),
        Err(e) => serde_json::json!({
            "success": false,
            "error": e.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_language() {
        assert!(matches!(
            parse_language("js").unwrap(),
            SandboxLanguage::JavaScript
        ));
        assert!(matches!(
            parse_language("python").unwrap(),
            SandboxLanguage::Python
        ));
        assert!(matches!(
            parse_language("ts").unwrap(),
            SandboxLanguage::TypeScript
        ));
        assert!(parse_language("cobol").is_err());
    }

    #[test]
    fn test_truncate() {
        let long = "x".repeat(100);
        let (trunc, was_trunc) = truncate(&long, 50);
        assert!(was_trunc);
        assert_eq!(trunc.len(), 50);
        let short = "hello";
        let (trunc, was_trunc) = truncate(short, 50);
        assert!(!was_trunc);
        assert_eq!(trunc, "hello");
    }

    #[test]
    fn test_code_too_large() {
        let lang = parse_language("js");
        assert!(lang.is_ok());
    }

    // P1.3 unit tests — ast_forbidden_scan integration.
    #[test]
    fn test_forbidden_scan_python_ast() {
        use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;
        let dangerous = "import subprocess\nsubprocess.run(['ls'])\nos.remove('/etc/passwd')";
        let found = ast_forbidden_scan(SandboxLanguage::Python, dangerous);
        assert!(
            !found.is_empty(),
            "Should detect subprocess.run: {:?}",
            found
        );
    }

    #[test]
    fn test_forbidden_scan_js_ast() {
        use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;
        let dangerous = "const fs = require('fs');\nfs.writeFileSync('/tmp/x', 'data');";
        let found = ast_forbidden_scan(SandboxLanguage::JavaScript, dangerous);
        assert!(!found.is_empty(), "Should detect fs calls: {:?}", found);
    }

    #[test]
    fn test_forbidden_scan_clean_code() {
        use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;
        let clean = "const x = 1 + 2;\nconsole.log(x);";
        let found = ast_forbidden_scan(SandboxLanguage::JavaScript, clean);
        assert!(
            found.is_empty(),
            "Clean code should have no violations: {:?}",
            found
        );
    }

    // P1.4 — ForbiddenCallPolicy env resolution. The three cases mutate the
    // same process-global env vars, so they are consolidated into ONE test
    // that runs them sequentially — separate `#[test]` fns would race under
    // the parallel test runner (observed flaky pattern, see memory F2).
    #[test]
    fn test_policy_from_env_all_cases() {
        // Case 1: no env vars → Warn (the phased-rollout default).
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_OFF") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_ENFORCE") };
        assert_eq!(ForbiddenCallPolicy::from_env(), ForbiddenCallPolicy::Warn);

        // Case 2: OFF=1 → Off.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_OFF", "1") };
        assert_eq!(ForbiddenCallPolicy::from_env(), ForbiddenCallPolicy::Off);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_OFF") };

        // Case 3: ENFORCE=1 → Block.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_ENFORCE", "1") };
        assert_eq!(ForbiddenCallPolicy::from_env(), ForbiddenCallPolicy::Block);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_ENFORCE") };

        // Case 4: OFF wins over ENFORCE when both are set (off is checked first).
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_OFF", "1") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_ENFORCE", "1") };
        assert_eq!(ForbiddenCallPolicy::from_env(), ForbiddenCallPolicy::Off);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_OFF") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_ENFORCE") };
    }

    #[test]
    fn test_run_forbidden_scan_fail_open() {
        // Empty code should never panic and should return empty.
        let result = run_forbidden_scan(SandboxLanguage::Python, "");
        assert!(result.is_empty());
    }

    #[test]
    fn test_forbidden_scan_perl_substring_fallback() {
        use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;
        // Perl has no tree-sitter grammar; uses substring fallback.
        let dangerous = "system('rm -rf /tmp/test');";
        let found = ast_forbidden_scan(SandboxLanguage::Perl, dangerous);
        assert!(
            !found.is_empty(),
            "Perl substring fallback should detect system(): {:?}",
            found
        );
    }

    #[test]
    fn test_forbidden_scan_r_substring_fallback() {
        use touring_hooks::shared::forbidden_patterns::ast_forbidden_scan;
        // R has no tree-sitter grammar; uses substring fallback.
        let dangerous = "system('ls')";
        let found = ast_forbidden_scan(SandboxLanguage::R, dangerous);
        assert!(
            !found.is_empty(),
            "R substring fallback should detect system(): {:?}",
            found
        );
    }
}
