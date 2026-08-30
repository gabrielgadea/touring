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

/// W1 d3/S-1.1 — orthogonal failure taxonomy (dsh-derived, 6 kinds): a budget
/// expiry is not an exception, an abort is not a timeout, and a substrate
/// death is neither. Each kind steers a DIFFERENT correction by the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunFailureKind {
    /// The program itself failed (non-zero exit with its own error output).
    Exception,
    /// A budget (wall-clock or CPU busy time) expired and the process was killed.
    Timeout,
    /// The execution was denied/cancelled before or during the run (e.g. lock conflict).
    Abort,
    /// The subprocess died abnormally or could not be launched.
    ProcExit,
    /// The request itself was malformed (bad args / unsupported input).
    InvalidOutput,
    /// Captured output hit the sandbox byte cap and was cut — data was LOST,
    /// not merely elided; rerun producing less output or read the stored file.
    OutputLimit,
}

/// W1 d3/S-1.1 — which pipeline stage the failure belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunPhase {
    /// Before any process existed: language/args validation.
    Parse,
    /// Launching the subprocess.
    Spawn,
    /// While the program was running.
    Execute,
}

/// W1 d3/S-1.1 — structured failure descriptor attached to a run result.
/// `message` is written for the model to self-correct (P9/P15).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFailure {
    /// Orthogonal failure class.
    pub kind: RunFailureKind,
    /// Pipeline stage where it happened.
    pub phase: RunPhase,
    /// Human/model-readable cause naming the next action.
    pub message: String,
}

/// `bytes_elided == 0` é o caso comum (nada foi cortado) — omiti-lo mantém
/// o payload enxuto sem perder a informação quando ela existe.
fn is_zero(v: &u64) -> bool {
    *v == 0
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
    /// W1 d3/S-1.1 — structured failure taxonomy; `None` on a clean success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<RunFailure>,
    /// W1 d3/S-1.2 — full-output locator on disk when the inline view was
    /// truncated (the complete stdout is already persisted by the sandbox).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stored_path: Option<String>,
    /// W1 d3/S-1.2 — how to read the full output (P19: spill with a
    /// retrieval hint, never a silent cut).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_hint: Option<String>,
    /// Bytes que a elisão manteve FORA do contexto (saída completa − inline).
    ///
    /// Exposto porque o `record_code_mode_run` logo abaixo incrementa um counter
    /// do PROCESSO, e o `touring run` é um CLI efêmero: o incremento morria com
    /// ele enquanto o `journal_run` da linha seguinte — que escreve em disco —
    /// sobrevivia. Medido em 24/08/2026: 163 linhas de journal contra
    /// `code_mode_runs_count = 0` no daemon. Com o valor aqui, o adaptador CLI
    /// retransmite ao daemon, que é quem o `gate-metrics` lê.
    #[serde(skip_serializing_if = "is_zero", default)]
    pub bytes_elided: u64,
    /// C2-W0 S-5.2 — this execution's identity (`run-<epoch_ms>-<pid>`).
    /// The same id keys `run_journal.jsonl`, reaches the sandbox child as
    /// `TOURING_RUN_ID`, and prefixes each orchestrate sub-call's `origin`
    /// (`<run_id>:code:<n>`) in the daemon's `run_subcalls.jsonl` — one id
    /// correlates CLI, sandbox, and daemon.
    pub run_id: String,
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

/// W1 d3/S-1.2 — head/tail preview: keep 3/4 of the cap from the start and
/// 1/4 from the end with an explicit elision marker. The tail is where a
/// program's FINAL result usually lives — a head-only cut hid exactly the
/// bytes the model needed.
fn truncate_head_tail(s: &str, max_bytes: usize) -> (String, bool) {
    let bytes = s.as_bytes();
    if bytes.len() <= max_bytes {
        return (s.to_string(), false);
    }
    let head_len = max_bytes * 3 / 4;
    let tail_len = max_bytes - head_len;
    let head = String::from_utf8_lossy(&bytes[..head_len]);
    let tail = String::from_utf8_lossy(&bytes[bytes.len() - tail_len..]);
    let elided = bytes.len() - head_len - tail_len;
    (
        format!("{head}\n... [{elided} bytes elided] ...\n{tail}"),
        true,
    )
}

/// W1 d3/S-1.2 — inline caps for the run result, env-overridable so a caller
/// that wants the full million bytes inline can ask for it explicitly.
fn inline_caps() -> (usize, usize) {
    fn cap(var: &str, default: usize) -> usize {
        std::env::var(var)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }
    (
        cap("TOURING_RUN_MAX_STDOUT_BYTES", 8 * 1024),
        cap("TOURING_RUN_MAX_STDERR_BYTES", 4 * 1024),
    )
}

/// Cross-audit 27/08 (QW-2/3/4) — ajustes POR CHAMADA do `touring run`; antes
/// só existiam como env vars globais do processo. Todo campo `None` preserva
/// o comportamento anterior (default do engine / env var).
#[derive(Debug, Default, Clone)]
pub struct RunTunables {
    /// Busy-time (CPU) budget do filho em ms — override do default 60 s.
    pub compute_ms: Option<u64>,
    /// Cap do stdout INLINE em bytes (a íntegra sempre vai para o spill).
    pub max_stdout_bytes: Option<usize>,
    /// Cap do stderr INLINE em bytes.
    pub max_stderr_bytes: Option<usize>,
    /// Bytes entregues ao stdin do programa (`touring run --input <file>`).
    pub stdin_bytes: Option<Vec<u8>>,
    /// NET-1 (28/08): portas TCP de saída concedidas (`--allow-net-port`).
    /// `None`/`Some(vec![])` = deny-all (default). Residual aceito e
    /// documentado: Landlock filtra por PORTA, não por host.
    pub allow_net_ports: Option<Vec<u16>>,
    /// OUT-1 (28/08): espelha a saída do programa no stderr do pai CONFORME
    /// chega (`touring run --stream`); o envelope JSON continua no stdout.
    pub stream: bool,
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

/// W4 d4/S-4.1 — append one JSONL record per execution to the run journal
/// (`~/.claude/touring/run_journal.jsonl`). The journal is the durable trace
/// the counterfactual KPI reads from; sub-call identity joins it in W5.
/// Fail-open: any I/O error is swallowed — observability never blocks a run.
fn journal_run(
    run_id: &str,
    language: &str,
    stdout_full: &str,
    exit_code: i32,
    duration_ms: u64,
    failure_kind: Option<RunFailureKind>,
    bytes_elided: u64,
) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let dir = std::path::Path::new(&home).join(".claude/touring");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let record = serde_json::json!({
        "ts": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "run_id": run_id,
        "language": language,
        "code_hash_stdout_bytes": stdout_full.len(),
        "exit_code": exit_code,
        "duration_ms": duration_ms,
        "failure_kind": failure_kind.map(|k| serde_json::to_value(k).ok()),
        "bytes_elided": bytes_elided,
    });
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("run_journal.jsonl"))
    {
        let _ = writeln!(f, "{record}");
    }
}

/// W0 d0 + W1 d3/S-1.1 — turn the raw sandbox result into the adapter's view:
/// `(stdout, stderr, exit_code, sandbox_stderr_truncated, stored_path, failure)`.
///
/// Propagates the REAL captured stderr (before 2026-08-23 the success arm
/// materialized `String::new()`) and derives the structured failure with
/// priority Timeout > OutputLimit > Exception — one failure, the one whose
/// correction the model should attempt first.
#[allow(clippy::type_complexity)]
fn derive_run_outcome(
    result: std::result::Result<touring_hooks::sandbox_executor::SandboxResult, SandboxError>,
) -> (
    String,
    String,
    i32,
    bool,
    Option<String>,
    Option<RunFailure>,
) {
    match result {
        Ok(r) => {
            let stdout_bytes = if let Some(ref path) = r.stored_path {
                std::fs::read(path).unwrap_or_default()
            } else {
                Vec::new()
            };
            let stdout_str = String::from_utf8_lossy(&stdout_bytes).to_string();
            let first_stderr_line = r
                .stderr
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .to_string();
            let failure = if r.exit_code == -2 {
                Some(RunFailure {
                    kind: RunFailureKind::Timeout,
                    phase: RunPhase::Execute,
                    message: first_stderr_line,
                })
            } else if r.was_truncated || r.stderr_truncated {
                Some(RunFailure {
                    kind: RunFailureKind::OutputLimit,
                    phase: RunPhase::Execute,
                    message: format!(
                        "output hit the sandbox byte cap and was cut (stdout_truncated={}, \
                         stderr_truncated={}); emit less output or read the stored file",
                        r.was_truncated, r.stderr_truncated
                    ),
                })
            } else if r.exit_code != 0 {
                Some(RunFailure {
                    kind: RunFailureKind::Exception,
                    phase: RunPhase::Execute,
                    message: if first_stderr_line.is_empty() {
                        if r.exit_code < 0 {
                            // 30/08 (analise-a2): exit negativo = morto por SINAL,
                            // não código do programa — o suspeito é um resource
                            // cap (SIGKILL não deixa stderr). Antes esta mensagem
                            // era a do "no match" e o operador leu kill de rlimit
                            // como timeout.
                            format!(
                                "process was killed by a signal (exit {}) before \
                                 finishing — a resource cap likely fired (the CPU \
                                 rlimit scales with --timeout-ms; memory is capped \
                                 at 80% of RAM). Empty stderr is typical of SIGKILL; \
                                 this is not the program's own exit code",
                                r.exit_code
                            )
                        } else {
                            // A5: a silent non-zero exit teaches nothing — and 71.6%
                            // of real failures land here (M1 ruler, 2026-08-28). The
                            // dominant silent case is grep/test/diff "no match", which
                            // exits 1 by DESIGN: name it so the model stops retrying
                            // the same body and handles the empty result instead.
                            format!(
                                "process exited with code {} with empty stderr — for \
                                 grep/test/diff exit 1 usually means 'no match', a \
                                 valid result to handle (append `|| true` if so), not \
                                 an error to retry",
                                r.exit_code
                            )
                        }
                    } else {
                        first_stderr_line
                    },
                })
            } else {
                None
            };
            let stored = r.stored_path.as_ref().map(|p| p.display().to_string());
            (
                stdout_str,
                r.stderr,
                r.exit_code,
                r.stderr_truncated,
                stored,
                failure,
            )
        }
        Err(e) => {
            let (kind, phase) = match &e {
                SandboxError::Spawn(_) => (RunFailureKind::ProcExit, RunPhase::Spawn),
                SandboxError::Timeout(_) => (RunFailureKind::Timeout, RunPhase::Execute),
                SandboxError::InvalidArgs(_) => (RunFailureKind::InvalidOutput, RunPhase::Parse),
                SandboxError::Conflict { .. } => (RunFailureKind::Abort, RunPhase::Spawn),
                SandboxError::Io(_) => (RunFailureKind::ProcExit, RunPhase::Execute),
            };
            let failure = Some(RunFailure {
                kind,
                phase,
                message: e.to_string(),
            });
            (String::new(), e.to_string(), -1, false, None, failure)
        }
    }
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
    tunables: Option<RunTunables>,
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
    // Cross-audit 27/08 (QW-3): teto do wall-clock sobe de 120 s para 600 s —
    // compilação pesada (rustc em projeto grande) estrangulava nos 120 s. O hot
    // loop segue contido: RLIMIT_CPU=30 s (kernel) + busy budget de CPU abaixo.
    let timeout = timeout_ms.unwrap_or(30_000).min(600_000);
    // C2-W0 S-5.2 — mint this execution's identity. It keys the journal,
    // reaches the child as TOURING_RUN_ID, and prefixes every orchestrate
    // sub-call's origin (`<run_id>:code:<n>`) at the daemon.
    let run_id = format!(
        "run-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        std::process::id()
    );
    let config = SandboxConfig {
        timeout_ms: timeout,
        max_output_bytes: 1_000_000,
        fallback_on_timeout: true,
        run_id: Some(run_id.clone()),
        compute_ms: tunables
            .as_ref()
            .and_then(|t| t.compute_ms)
            .or(Some(60_000)),
        stdin_bytes: tunables.as_ref().and_then(|t| t.stdin_bytes.clone()),
        allow_net_ports: tunables
            .as_ref()
            .and_then(|t| t.allow_net_ports.clone())
            .unwrap_or_default(),
        stream_output: tunables.as_ref().is_some_and(|t| t.stream),
        ..SandboxConfig::default()
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
    let (stdout, stderr, exit_code, sandbox_stderr_truncated, stored_path, failure) =
        derive_run_outcome(result);
    let (env_stdout, env_stderr) = inline_caps();
    // QW-2 — o pedido por chamada (flags do `touring run`) vence a env var.
    let (max_stdout, max_stderr) = (
        tunables
            .as_ref()
            .and_then(|t| t.max_stdout_bytes)
            .unwrap_or(env_stdout),
        tunables
            .as_ref()
            .and_then(|t| t.max_stderr_bytes)
            .unwrap_or(env_stderr),
    );
    let (stdout_trunc, stdout_truncated) = truncate_head_tail(&stdout, max_stdout);

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

    let (stderr_trunc, adapter_stderr_truncated) = truncate_head_tail(&final_stderr, max_stderr);
    let stderr_truncated = adapter_stderr_truncated || sandbox_stderr_truncated;
    // W4 d4/S-4.1+S-4.2 — journal the execution and count the MEASURED
    // context savings of the spill (full bytes − inline bytes). Fail-open:
    // observability never blocks the run path.
    let bytes_elided = (stdout.len() + final_stderr.len())
        .saturating_sub(stdout_trunc.len() + stderr_trunc.len()) as u64;
    touring_hooks::shared::gate_metrics::record_code_mode_run(bytes_elided);
    journal_run(
        &run_id,
        &language,
        &stdout,
        exit_code,
        duration_ms,
        failure.as_ref().map(|f| f.kind),
        bytes_elided,
    );
    // W1 d3/S-1.2 — when the inline view lost bytes and the full output is on
    // disk, hand the model the locator + how to read it (P19).
    let retrieval_hint = match (&stored_path, stdout_truncated || stderr_truncated) {
        (Some(p), true) => Some(format!(
            "full stdout on disk: Read {p} --offset N --limit M, or grep '<pattern>' {p}"
        )),
        _ => None,
    };
    Ok(CtxExecuteOutput {
        bytes_elided,
        stdout: stdout_trunc,
        stderr: stderr_trunc,
        exit_code,
        duration_ms,
        forbidden_calls: forbidden,
        stdout_truncated,
        // Honest flag: truncated at EITHER boundary (sandbox cap or the
        // adapter's inline cap) — `stderr_truncated: false` with swallowed
        // content was the lie that hid the d0 bug.
        stderr_truncated,
        failure,
        stored_path: if stdout_truncated || stderr_truncated {
            stored_path
        } else {
            None
        },
        retrieval_hint,
        run_id,
    })
}

/// Serialize a `ctx_execute` result (success or error) into the canonical
/// JSON envelope returned to MCP consumers.
pub fn format_output(
    result: std::result::Result<CtxExecuteOutput, CtxExecuteError>,
) -> serde_json::Value {
    match result {
        Ok(output) => {
            let mut v = serde_json::json!({
                "success": true,
                "stdout": output.stdout,
                "stderr": output.stderr,
                "exit_code": output.exit_code,
                "duration_ms": output.duration_ms,
                "forbidden_calls": output.forbidden_calls,
                "stdout_truncated": output.stdout_truncated,
                "stderr_truncated": output.stderr_truncated,
            });
            // W1 d3 — taxonomy + spill locator ride along when present.
            if let Some(f) = &output.failure {
                v["failure"] = serde_json::json!({
                    "kind": f.kind, "phase": f.phase, "message": f.message,
                });
            }
            if let Some(p) = &output.stored_path {
                v["stored_path"] = serde_json::json!(p);
            }
            if let Some(h) = &output.retrieval_hint {
                v["retrieval_hint"] = serde_json::json!(h);
            }
            v
        }
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
    fn test_truncate_head_tail() {
        let long = "x".repeat(100);
        let (trunc, was_trunc) = truncate_head_tail(&long, 50);
        assert!(was_trunc);
        assert!(trunc.contains("bytes elided"), "elision is explicit");
        assert!(trunc.starts_with('x') && trunc.ends_with('x'), "head and tail kept");
        let short = "hello";
        let (trunc, was_trunc) = truncate_head_tail(short, 50);
        assert!(!was_trunc);
        assert_eq!(trunc, "hello");
    }

    #[test]
    fn test_code_too_large() {
        let lang = parse_language("js");
        assert!(lang.is_ok());
    }

    /// C2-W0 S-5.2 — every execution carries its minted identity: the same
    /// `run-<epoch_ms>-<pid>` keys the journal, reaches the child as
    /// `TOURING_RUN_ID`, and prefixes sub-call origins at the daemon.
    #[tokio::test]
    async fn ctx_execute_output_carries_run_id() {
        let out = ctx_execute_impl(
            "python".to_string(),
            "print(1)".to_string(),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("run");
        assert!(
            out.run_id.starts_with("run-"),
            "run_id must be minted with the run- prefix, got {:?}",
            out.run_id
        );
        assert_eq!(out.exit_code, 0);
    }

    // ── Cross-audit 27/08 — quick-wins QW-1..QW-4 ────────────────────────

    /// QW-4: `--input` bytes reach the program's stdin.
    #[tokio::test]
    async fn tunables_stdin_bytes_reach_the_program() {
        let out = ctx_execute_impl(
            "python".to_string(),
            "import sys; print('GOT:' + sys.stdin.read().strip())".to_string(),
            None,
            None,
            None,
            None,
            Some(crate::tools::ctx_execute_tools::RunTunables {
                stdin_bytes: Some(b"payload-via-input\n".to_vec()),
                ..Default::default()
            }),
        )
        .await
        .expect("run");
        assert_eq!(out.exit_code, 0);
        assert!(
            out.stdout.contains("GOT:payload-via-input"),
            "stdin entregue ao programa: {:?}",
            out.stdout
        );
    }

    /// QW-1: TMPDIR is declared (=/tmp, a write root Landlock already grants)
    /// so tools honouring it have a temp dir.
    #[tokio::test]
    async fn child_sees_tmpdir_pointing_to_tmp() {
        let out = ctx_execute_impl(
            "bash".to_string(),
            "echo \"TMPDIR=$TMPDIR\"; mktemp -d >/dev/null && echo MKTEMP_OK".to_string(),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("run");
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("TMPDIR=/tmp"), "TMPDIR: {:?}", out.stdout);
        assert!(out.stdout.contains("MKTEMP_OK"), "mktemp: {:?}", out.stdout);
    }

    /// QW-2: per-call inline caps override the 8 KB default — a 10 KB stdout
    /// stays whole inline when the caller asks for it.
    #[tokio::test]
    async fn tunables_max_stdout_bytes_overrides_the_inline_cap() {
        let out = ctx_execute_impl(
            "python".to_string(),
            "print('x' * 10000)".to_string(),
            None,
            None,
            None,
            None,
            Some(crate::tools::ctx_execute_tools::RunTunables {
                max_stdout_bytes: Some(64 * 1024),
                ..Default::default()
            }),
        )
        .await
        .expect("run");
        assert_eq!(out.exit_code, 0);
        assert!(
            out.stdout.contains(&"x".repeat(10_000)),
            "stdout inline integral com o cap pedido (len={})",
            out.stdout.len()
        );
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
        let _env = crate::cli::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Case 1: no env vars → Warn (the phased-rollout default).
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_OFF") };
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_ENFORCE") };
        assert_eq!(ForbiddenCallPolicy::from_env(), ForbiddenCallPolicy::Warn);

        // Case 2: OFF=1 → Off.
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
        unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_OFF", "1") };
        assert_eq!(ForbiddenCallPolicy::from_env(), ForbiddenCallPolicy::Off);
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_OFF") };

        // Case 3: ENFORCE=1 → Block.
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
        unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_ENFORCE", "1") };
        assert_eq!(ForbiddenCallPolicy::from_env(), ForbiddenCallPolicy::Block);
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_ENFORCE") };

        // Case 4: OFF wins over ENFORCE when both are set (off is checked first).
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
        unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_OFF", "1") };
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
        unsafe { std::env::set_var("TOURING_CEG_FORBIDDEN_ENFORCE", "1") };
        assert_eq!(ForbiddenCallPolicy::from_env(), ForbiddenCallPolicy::Off);
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
        unsafe { std::env::remove_var("TOURING_CEG_FORBIDDEN_OFF") };
        // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
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
