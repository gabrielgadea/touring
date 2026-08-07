//! D2.2 — SandboxExecutor: isolated subprocess execution for context-mode routing.
//!
//! Part of D2 PreToolUse Output Router (P0, XL). Enables LLM context savings by
//! running large-output commands in a subprocess, capturing output to disk,
//! and returning only a content_hash reference (blake3 hex).
//!
//! When `tool_output_router::classify_tool_routing` returns `RouteToSandbox`,
//! this module owns subprocess lifecycle: spawn, timeout-bounded poll, output
//! capture, blake3 hashing, and on-disk persistence under
//! `~/.claude/touring/sandbox_outputs/<hash>.bin`.
//!
//! Reuses:
//! - `JobRegistry::spawn_worker` pattern (`shared::job_registry`) — execve
//!   without shell interpolation (command-injection safe).
//! - `blake3` (workspace dep) — content-addressable hashing.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::capability::{ResourceLimits, apply_resource_caps_to};
use crate::gateway::exec_pool::ExecPool;
use crate::gateway::summarize::{OutputSummary, summarize_output};

/// Configuration for a sandbox subprocess invocation.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Maximum wall-clock time before the subprocess is killed.
    pub timeout_ms: u64,
    /// Maximum captured stdout bytes; output beyond is truncated.
    pub max_output_bytes: u64,
    /// On timeout, return Err. If false, also returns Err but caller may bypass.
    pub fallback_on_timeout: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            max_output_bytes: 1_000_000,
            fallback_on_timeout: true,
        }
    }
}

/// Result of a sandbox subprocess execution.
#[derive(Debug, Clone)]
pub struct SandboxResult {
    /// Process exit status code returned by the subprocess.
    pub exit_code: i32,
    /// Number of stdout bytes captured (after any truncation).
    pub output_bytes: u64,
    /// `true` if captured stdout hit `max_output_bytes` and was cut short.
    pub was_truncated: bool,
    /// blake3 hex digest (64 chars) of captured stdout.
    pub content_hash: String,
    /// Filesystem path under which raw bytes were persisted.
    pub stored_path: Option<PathBuf>,
    /// C5 — inline, metadata-first digest (exit code, error signatures, `file:line`
    /// refs) so callers get the failure signal without re-reading `stored_path`.
    pub summary: OutputSummary,
}

/// Failure modes of a sandbox subprocess execution.
#[derive(Debug, Error)]
pub enum SandboxError {
    /// An underlying I/O operation (spawn pipe, read, persist) failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The subprocess could not be launched; the string carries the cause.
    #[error("spawn failed: {0}")]
    Spawn(String),
    /// The subprocess exceeded `timeout_ms` (the payload) and was killed.
    #[error("timed out after {0}ms")]
    Timeout(u64),
    /// The requested invocation arguments were malformed or unsupported.
    #[error("invalid args: {0}")]
    InvalidArgs(String),
    /// **ES3 P2 (2026-06-02)** — a [`crate::gateway::supervised::run_supervised_with_locks`]
    /// invocation was denied because its declared access-set conflicts with
    /// a still-held transaction. The carrying `ExecutionId` is the
    /// conflicting holder's id; `resource` is the access path that
    /// triggered the conflict (advisory — exact path may be empty if the
    /// lock manager surfaced only the id).
    #[error(
        "transactional conflict with execution {conflicting_execution_id} on resource {resource:?}"
    )]
    Conflict {
        /// `ExecutionId` of the still-held transaction that blocked this run.
        conflicting_execution_id: u64,
        /// Access path that triggered the conflict (may be empty if the lock
        /// manager surfaced only the id).
        resource: String,
    },
}

/// Resolves the program path for a given tool name.
///
/// Bash → `bash`, Grep → `grep`, Glob → `find`, others → `cat` (no-op default).
///
/// I-11 — Multi-language sandbox: tool_names matching `Sandbox<Lang>` route
/// to the corresponding language runtime via [`resolve_language_runtime`].
pub fn resolve_program(tool_name: &str) -> PathBuf {
    match tool_name {
        "Bash" => PathBuf::from("bash"),
        "Grep" => PathBuf::from("grep"),
        "Glob" => PathBuf::from("find"),
        // I-11 — language runtimes (auto-detected, with fallbacks)
        "SandboxPython" => resolve_language_runtime(SandboxLanguage::Python),
        "SandboxJavaScript" => resolve_language_runtime(SandboxLanguage::JavaScript),
        "SandboxTypeScript" => resolve_language_runtime(SandboxLanguage::TypeScript),
        "SandboxRuby" => resolve_language_runtime(SandboxLanguage::Ruby),
        "SandboxGo" => resolve_language_runtime(SandboxLanguage::Go),
        "SandboxPerl" => resolve_language_runtime(SandboxLanguage::Perl),
        "SandboxR" => resolve_language_runtime(SandboxLanguage::R),
        "SandboxElixir" => resolve_language_runtime(SandboxLanguage::Elixir),
        "SandboxPhp" => resolve_language_runtime(SandboxLanguage::Php),
        "SandboxRust" => resolve_language_runtime(SandboxLanguage::Rust),
        _ => PathBuf::from("cat"),
    }
}

// Fronteira 2 follow-up (2026-06-10): the SandboxLanguage vocabulary enum moved
// to the leaf crate `touring-hooks-shared::sandbox_language` so the leaf-side
// risk matcher (forbidden_patterns) can use it without depending on the CEG.
// Re-exported here so `crate::gateway::sandbox_executor::SandboxLanguage` and
// every `touring_ceg`/`touring_hooks` consumer path keeps resolving unchanged.
pub use touring_hooks_shared::sandbox_language::SandboxLanguage;

/// I-11 — Detect the preferred runtime binary for a language. Order:
/// 1. Modern fast runtime (bun > node, python3 > python).
/// 2. Fallback common alias.
/// 3. Last-resort: language name as binary (PATH may surface alternatives).
///
/// Result cached at module level via OnceLock: re-detection forced via
/// `touring sandbox-runtimes refresh` (future CLI).
pub fn resolve_language_runtime(lang: SandboxLanguage) -> PathBuf {
    fn which(bin: &str) -> Option<PathBuf> {
        // Use `command -v` via shell — portable, no extra deps.
        let out = std::process::Command::new("sh")
            .args(["-c", &format!("command -v {bin} 2>/dev/null")])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(PathBuf::from(s))
        }
    }
    let candidates: &[&str] = match lang {
        SandboxLanguage::JavaScript => &["bun", "node"],
        SandboxLanguage::TypeScript => &["bun", "tsx", "ts-node"],
        SandboxLanguage::Python => &["python3", "python"],
        SandboxLanguage::Ruby => &["ruby"],
        SandboxLanguage::Go => &["go"],
        SandboxLanguage::Rust => &["cargo"],
        SandboxLanguage::Php => &["php"],
        SandboxLanguage::Perl => &["perl"],
        SandboxLanguage::R => &["Rscript", "R"],
        SandboxLanguage::Elixir => &["elixir"],
        SandboxLanguage::Shell => &["bash", "sh"],
    };
    for bin in candidates {
        if let Some(p) = which(bin) {
            return p;
        }
    }
    // Last resort: name itself; spawn will Err if not on PATH.
    PathBuf::from(candidates.first().copied().unwrap_or("cat"))
}

/// I-11 — Build argv for a language runtime executing inline source.
/// Each runtime has its own `-c`/`-e` convention; these are stable across
/// modern versions.
pub fn resolve_language_args(lang: SandboxLanguage, code: &str) -> Vec<String> {
    match lang {
        SandboxLanguage::JavaScript => vec!["-e".into(), code.to_string()],
        SandboxLanguage::TypeScript => vec!["-e".into(), code.to_string()],
        SandboxLanguage::Python => vec!["-c".into(), code.to_string()],
        SandboxLanguage::Ruby => vec!["-e".into(), code.to_string()],
        SandboxLanguage::Php => vec!["-r".into(), code.to_string()],
        SandboxLanguage::Perl => vec!["-e".into(), code.to_string()],
        SandboxLanguage::R => vec!["-e".into(), code.to_string()],
        SandboxLanguage::Shell => vec!["-c".into(), code.to_string()],
        // P4.1 — Go and Rust are compiled: they have no inline-source flag.
        // `execute_in_sandbox` routes them to `compile_and_run_go` /
        // `compile_and_run_rust` (tempfile + compile); this arm is unreachable
        // for them on the sandbox path and yields an empty argv.
        SandboxLanguage::Go | SandboxLanguage::Rust => Vec::new(),
        SandboxLanguage::Elixir => vec!["-e".into(), code.to_string()],
    }
}

/// Resolves argv for a given tool invocation.
///
/// Translates structured args into shell-safe argv (no shell interpolation —
/// passed directly to `execve`).
pub fn resolve_args(tool_name: &str, args: &Value) -> Result<Vec<String>, SandboxError> {
    match tool_name {
        "Bash" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SandboxError::InvalidArgs("Bash requires 'command' field".into()))?;
            Ok(vec!["-c".into(), cmd.to_string()])
        }
        "Grep" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SandboxError::InvalidArgs("Grep requires 'pattern'".into()))?;
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let mut argv = vec!["-rn".to_string(), pattern.to_string(), path.to_string()];
            if args
                .get("recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                argv.insert(0, "-r".into());
            }
            Ok(argv)
        }
        "Glob" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SandboxError::InvalidArgs("Glob requires 'pattern'".into()))?;
            Ok(vec![
                ".".to_string(),
                "-name".to_string(),
                pattern.to_string(),
            ])
        }
        // Language sandbox: SandboxJavaScript, SandboxTypeScript, SandboxPython,
        // SandboxRuby, SandboxGo, SandboxPerl, SandboxR, SandboxElixir, SandboxPhp,
        // SandboxRust — use resolve_language_args for inline script execution.
        s if s.starts_with("Sandbox") || s == "Bash" && args.get("script").is_some() => {
            // Determine SandboxLanguage from tool_name prefix.
            let lang = match tool_name {
                "SandboxJavaScript" => SandboxLanguage::JavaScript,
                "SandboxTypeScript" => SandboxLanguage::TypeScript,
                "SandboxPython" => SandboxLanguage::Python,
                "SandboxRuby" => SandboxLanguage::Ruby,
                "SandboxGo" => SandboxLanguage::Go,
                "SandboxPerl" => SandboxLanguage::Perl,
                "SandboxR" => SandboxLanguage::R,
                "SandboxElixir" => SandboxLanguage::Elixir,
                "SandboxPhp" => SandboxLanguage::Php,
                "SandboxRust" => SandboxLanguage::Rust,
                // Bash with script field (ctx_execute shell fallback).
                "Bash" => SandboxLanguage::Shell,
                _ => return Ok(vec![]),
            };
            let script = args.get("script").and_then(|v| v.as_str()).ok_or_else(|| {
                SandboxError::InvalidArgs("Sandbox requires 'script' field".into())
            })?;
            Ok(resolve_language_args(lang, script))
        }
        _ => Ok(vec![]),
    }
}

/// Executes a tool invocation in a sandbox subprocess.
///
/// Spawns the resolved program with resolved args using `tokio::process::Command`
/// (execve, no shell interpolation). Polls for completion with a timeout. On
/// success, captures stdout, computes blake3 hash, and persists to disk.
pub async fn execute_in_sandbox(
    tool_name: &str,
    original_args: Value,
    config: SandboxConfig,
) -> Result<SandboxResult, SandboxError> {
    // P4.1 — compiled languages (Go, Rust) need tempfile + compile, not the
    // interpreted-runtime inline-arg path; intercept them up front.
    if tool_name == "SandboxGo" || tool_name == "SandboxRust" {
        let script = original_args
            .get("script")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SandboxError::InvalidArgs(format!("{tool_name} requires 'script' field"))
            })?;
        return if tool_name == "SandboxGo" {
            compile_and_run_go(script, &config).await
        } else {
            compile_and_run_rust(script, &config).await
        };
    }

    let program = resolve_program(tool_name);
    let argv = resolve_args(tool_name, &original_args)?;

    let mut cmd = Command::new(&program);
    cmd.args(&argv);
    // I-12 — Credential passthrough: env_clear() then re-inherit only
    // whitelisted credentials. Subprocess vê GH_TOKEN, AWS_*, etc., mas o
    // LLM nunca recebe os valores (stdout passa por redact_secrets).
    apply_credential_whitelist(&mut cmd);
    // P4.3 — every X5 sandbox execution is resource-capped, not only X8.
    apply_resource_caps_to(&mut cmd, &ResourceLimits::sandboxed());

    spawn_and_capture(cmd, &config).await
}

/// P4.1 — spawn a fully-built command and capture its bounded output.
///
/// The shared execution core of [`execute_in_sandbox`] and the compiled-
/// language runners ([`compile_and_run_go`] / [`compile_and_run_rust`]): pipe
/// stdio, spawn, read stdout under the wall-clock timeout, await exit,
/// blake3-hash + persist the output, and tee a failing run's bytes. `cmd`
/// arrives with its program, args and environment already set.
///
/// P4.4 — acquires an [`ExecPool`] slot before the spawn and holds it for the
/// whole capture, so the daemon never has more than the pool's cap of
/// sandboxed subprocesses live at once; under saturation the acquire queues
/// (backpressure) instead of spawning.
pub(crate) async fn spawn_and_capture(
    mut cmd: Command,
    config: &SandboxConfig,
) -> Result<SandboxResult, SandboxError> {
    // P4.4 — bound the daemon's live subprocesses: take an exec-pool slot
    // before the spawn and hold it (`_pool_slot`) for the whole capture; it
    // releases on every return path. Under saturation `acquire` queues for the
    // pool's timeout (backpressure) rather than spawning.
    let _pool_slot = ExecPool::global()
        .acquire()
        .await
        .map_err(|e| SandboxError::Spawn(format!("{e}")))?;

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| SandboxError::Spawn(format!("{e}")))?;

    let stdout_handle = child.stdout.take();
    let timeout_dur = Duration::from_millis(config.timeout_ms);
    let max_bytes = config.max_output_bytes as usize;

    let read_fut = async move {
        let mut buf: Vec<u8> = Vec::with_capacity(8192);
        let mut truncated = false;
        if let Some(mut out) = stdout_handle {
            let mut chunk = vec![0u8; 8192];
            loop {
                if buf.len() >= max_bytes {
                    truncated = true;
                    break;
                }
                match out.read(&mut chunk).await {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }
        }
        (buf, truncated)
    };

    let (output_bytes, was_truncated) = match timeout(timeout_dur, read_fut).await {
        Ok(pair) => pair,
        Err(_) => {
            let _ = child.kill().await;
            return timeout_outcome(config);
        }
    };

    let exit_status = match timeout(Duration::from_millis(500), child.wait()).await {
        Ok(Ok(s)) => s,
        _ => {
            let _ = child.kill().await;
            return timeout_outcome(config);
        }
    };
    let exit_code = exit_status.code().unwrap_or(-1);

    let content_hash = hash_output(&output_bytes);
    let stored_path = store_output(&content_hash, &output_bytes).ok();

    // NEW-2 — Failure tee mode: persist FULL output to tee/ when subprocess
    // returned non-zero. Skips on success path to avoid storage bloat.
    if exit_code != 0 && !output_bytes.is_empty() && store_tee(&content_hash, &output_bytes).is_ok()
    {
        touring_hooks_shared::gate_metrics::record_sandbox_tee_persisted();
    }

    // C5 — build the inline metadata-first digest from the captured buffer
    // BEFORE it is dropped; the full bytes remain on disk via `stored_path`.
    let summary = summarize_output(
        &String::from_utf8_lossy(&output_bytes),
        exit_code,
        was_truncated,
    );

    Ok(SandboxResult {
        exit_code,
        output_bytes: output_bytes.len() as u64,
        was_truncated,
        content_hash,
        stored_path,
        summary,
    })
}

/// Resolves what to return when the sandbox subprocess exceeds `timeout_ms`.
///
/// When `config.fallback_on_timeout = true` we synthesize a sentinel
/// [`SandboxResult`] (exit_code = -2, was_truncated = true, empty hash) so
/// callers can detect the situation without an Err short-circuiting the
/// entire hook chain. When `false`, propagate as [`SandboxError::Timeout`].
fn timeout_outcome(config: &SandboxConfig) -> Result<SandboxResult, SandboxError> {
    if config.fallback_on_timeout {
        Ok(SandboxResult {
            exit_code: -2,
            output_bytes: 0,
            was_truncated: true,
            content_hash: String::new(),
            stored_path: None,
            summary: OutputSummary::empty(-2),
        })
    } else {
        Err(SandboxError::Timeout(config.timeout_ms))
    }
}

/// P4.1 — environment for a compiler-toolchain subprocess.
///
/// [`apply_credential_whitelist`] alone (`env_clear` + credentials) starves a
/// toolchain: `rustc` needs `PATH` to find the linker, `go` needs `HOME` for
/// its build cache. This re-adds exactly `PATH` and `HOME` on top of the
/// credential whitelist — the minimum a compiler cannot run without.
fn apply_toolchain_env(cmd: &mut Command) {
    apply_credential_whitelist(cmd);
    for key in ["PATH", "HOME"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
}

/// P4.1 — compile and run an inline Go program in the sandbox.
///
/// Go is compiled — there is no inline `-e` / `-c` flag. The source is written
/// to a tempfile alongside a minimal `go.mod`, then run via `go run .`, which
/// compiles to a temporary binary and executes it in one step. The tempdir is
/// RAII-removed once the run completes ([`tempfile::TempDir`]).
///
/// This resolves the former `resolve_language_args` placeholder that admitted
/// "Go and Rust require tempfile + compile".
pub async fn compile_and_run_go(
    code: &str,
    config: &SandboxConfig,
) -> Result<SandboxResult, SandboxError> {
    let dir = tempfile::TempDir::new()?;
    std::fs::write(dir.path().join("go.mod"), "module ceg_sandbox\ngo 1.21\n")?;
    std::fs::write(dir.path().join("main.go"), code)?;

    let mut cmd = Command::new(resolve_language_runtime(SandboxLanguage::Go));
    cmd.arg("run").arg(".").current_dir(dir.path());
    apply_toolchain_env(&mut cmd);
    apply_resource_caps_to(&mut cmd, &ResourceLimits::sandboxed());
    spawn_and_capture(cmd, config).await
}

/// P4.1 — compile and run an inline Rust program in the sandbox.
///
/// Rust compilation and execution are two steps: `rustc` builds the source
/// tempfile to a binary, then the binary runs. A compile failure is itself a
/// meaningful outcome — it is returned as the [`SandboxResult`] of the `rustc`
/// invocation (non-zero exit) and the binary is never run.
///
/// This resolves the former `resolve_language_args` placeholder that admitted
/// "Rust requires cargo-script crate".
pub async fn compile_and_run_rust(
    code: &str,
    config: &SandboxConfig,
) -> Result<SandboxResult, SandboxError> {
    let dir = tempfile::TempDir::new()?;
    let src = dir.path().join("main.rs");
    let bin = dir.path().join("main");
    std::fs::write(&src, code)?;

    // Step 1 — compile. `rustc` processes source text; it does not run the
    // user's code, so a compile failure is captured, never executed.
    let mut compile = Command::new("rustc");
    compile
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .arg("--edition=2021")
        .current_dir(dir.path());
    apply_toolchain_env(&mut compile);
    apply_resource_caps_to(&mut compile, &ResourceLimits::sandboxed());
    let compiled = spawn_and_capture(compile, config).await?;
    if compiled.exit_code != 0 {
        // Compilation failed — that IS the outcome; the binary never runs.
        return Ok(compiled);
    }

    // Step 2 — run the compiled binary.
    let mut run = Command::new(&bin);
    run.current_dir(dir.path());
    apply_toolchain_env(&mut run);
    apply_resource_caps_to(&mut run, &ResourceLimits::sandboxed());
    spawn_and_capture(run, config).await
}

/// Synchronous wrapper over [`execute_in_sandbox`] for use inside hook
/// handlers that cannot return a future (e.g. `pre_tool_use`).
///
/// Builds a fresh single-threaded tokio runtime per invocation and drives
/// the async flow to completion.
///
/// **Reentrancy safety (BUG-P0 fix, 2026-05-23)**: when called from a thread
/// that is *already* driving a tokio runtime (for example the `touring exec`
/// CLI's main runtime or the daemon's task scheduler), naive `block_on`
/// panics with `Cannot start a runtime from within a runtime` and the
/// process aborts (SIGABRT). To stay sound in both contexts this wrapper
/// detects the enclosing runtime via [`tokio::runtime::Handle::try_current`]
/// and, when one exists, hands the work off to a fresh OS thread that owns
/// its own runtime. The common (non-nested) case keeps the cheap inline path.
pub fn execute_in_sandbox_blocking(
    tool_name: &str,
    original_args: Value,
    config: SandboxConfig,
) -> Result<SandboxResult, SandboxError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        // Nested runtime: isolate so `block_on` is sound. `Value` and
        // `SandboxConfig` are `Send + 'static`; `tool_name` is borrowed and
        // therefore cloned across the boundary.
        let tool_name = tool_name.to_owned();
        return std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| SandboxError::Spawn(format!("rt build: {e}")))?;
            rt.block_on(execute_in_sandbox(&tool_name, original_args, config))
        })
        .join()
        .map_err(|_| {
            SandboxError::Spawn(
                "sandbox isolation thread panicked before producing a result".to_string(),
            )
        })?;
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| SandboxError::Spawn(format!("rt build: {e}")))?;
    rt.block_on(execute_in_sandbox(tool_name, original_args, config))
}

// S-13 cross-audit (2026-06-06): `sandbox_result_to_doc`, `derive_summary_with_tool`,
// and `execute_and_store` (the sandbox-result → Tantivy `tool_outputs` storage bridge)
// were relocated to the parent module `crate::sandbox_output_store`. They are
// tool-output *storage* — a parent concern consumed by `tool_output_router` — not the
// CEG sandbox runner, and they pulled `crate::{tantivy_index, compression_profiles}`
// parent edges into the gateway. After the move, the gateway's `sandbox_executor` is
// purely the CEG sandbox; the storage bridge depends on the gateway (parent → child),
// the correct direction for extraction.

/// I-12 — Whitelist of environment variable names that are allowed to flow
/// from the parent process into the sandbox subprocess. Tools like `gh`,
/// `aws`, `gcloud`, `kubectl`, `docker` need these to authenticate.
const CREDENTIAL_ENV_WHITELIST: &[&str] = &[
    // GitHub CLI
    "GH_TOKEN",
    "GITHUB_TOKEN",
    // AWS
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_REGION",
    "AWS_DEFAULT_REGION",
    "AWS_PROFILE",
    // Google Cloud
    "GOOGLE_APPLICATION_CREDENTIALS",
    "GCLOUD_PROJECT",
    "CLOUDSDK_CORE_PROJECT",
    // Kubernetes
    "KUBECONFIG",
    "KUBE_CONTEXT",
    // Docker
    "DOCKER_HOST",
    "DOCKER_TLS_VERIFY",
    "DOCKER_CERT_PATH",
    // npm / NodeJS
    "NPM_TOKEN",
    // OpenAI / Anthropic / common LLM API keys (when sandbox calls SDK)
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    // Baseline shell / locale (else many tools misbehave)
    "HOME",
    "PATH",
    "USER",
    "LANG",
    "LC_ALL",
    "TERM",
];

/// Baseline shell/locale vars a subprocess needs even when ALL credentials are
/// withheld — without `PATH`/`HOME` most tools cannot even start.
const BASELINE_ENV: &[&str] = &["HOME", "PATH", "USER", "LANG", "LC_ALL", "TERM"];

/// I-12 — Apply credential whitelist to the subprocess Command.
/// Clears all environment, then re-inherits only whitelisted vars from
/// the parent process. Extra whitelist via env `TOURING_SANDBOX_EXTRA_WHITELIST=A,B,C`.
///
/// Defense-in-depth opt-out (SEC-04, 2026-06-13): set `TOURING_SANDBOX_NO_CREDENTIALS=1`
/// and the sandbox child inherits ONLY [`BASELINE_ENV`] — never cloud/CLI credentials
/// (`GITHUB_TOKEN`, `AWS_*`, `ANTHROPIC_API_KEY`, …) and not even the extra whitelist.
/// The default preserves first-party toolchain behaviour (gh/aws/cargo need their
/// tokens to authenticate); the opt-out is for deployments running untrusted code that
/// must never see a credential, matching the deny-by-default capability model.
pub(crate) fn apply_credential_whitelist(cmd: &mut Command) {
    cmd.env_clear();
    let no_credentials = std::env::var_os("TOURING_SANDBOX_NO_CREDENTIALS").is_some();
    let extra: Vec<String> = std::env::var("TOURING_SANDBOX_EXTRA_WHITELIST")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().to_string())
        .collect();
    for name in CREDENTIAL_ENV_WHITELIST
        .iter()
        .map(|s| s.to_string())
        .chain(extra)
    {
        // In no-credentials mode, withhold everything except the baseline so the
        // child can still launch but carries zero secret material.
        if no_credentials && !BASELINE_ENV.contains(&name.as_str()) {
            continue;
        }
        if let Ok(value) = std::env::var(&name) {
            cmd.env(name, value);
        }
    }
}

/// T-09 (2026-06-15) — token-format secret patterns, matched ANYWHERE in a line.
/// The env-var-name pass in [`redact_secrets`] only catches `KEY=value`; these
/// catch a raw token value with no surrounding key (e.g. a `ghp_…` pasted into an
/// error message or a `Bearer sk-…` header).
static SECRET_TOKEN_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"gh[pousr]_[A-Za-z0-9_]{20,}", // GitHub PAT / OAuth / refresh / server / user
        r"sk-[A-Za-z0-9_-]{20,}",       // OpenAI (incl. sk-proj-…)
        r"AKIA[0-9A-Z]{16}",            // AWS access key id
        r"xox[baprs]-[A-Za-z0-9-]{10,}", // Slack bot/app/user/refresh/legacy tokens
    ]
    .iter()
    .filter_map(|p| Regex::new(p).ok())
    .collect()
});

/// I-12 / T-09 — Redact secret-like material from captured output before it is
/// stored, logged, or surfaced to the LLM. Three passes:
/// 1. **PEM private-key blocks** — `-----BEGIN … PRIVATE KEY-----` … `-----END …`
///    are dropped wholesale (header, base64 body, and footer).
/// 2. **Env-var name pass** — a line containing a known credential var name
///    (`GITHUB_TOKEN`, `AWS_SECRET_ACCESS_KEY`, …) has its `KEY=value` / `KEY: value`
///    value replaced (catches secrets whose value has no recognizable token format).
/// 3. **Token-format pass (T-09)** — `SECRET_TOKEN_PATTERNS` redact a raw token
///    value (GitHub/OpenAI/AWS/Slack) anywhere in the remaining text.
pub fn redact_secrets(s: &str) -> String {
    let needles: &[&str] = &[
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_ACCESS_KEY_ID",
        "AWS_SESSION_TOKEN",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "NPM_TOKEN",
    ];
    let mut out = String::with_capacity(s.len());
    let mut in_pem = false;
    for line in s.lines() {
        // Pass 1: PEM private-key block — drop BEGIN, base64 body, and END.
        if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
            in_pem = true;
            out.push_str("[REDACTED PEM PRIVATE KEY]\n");
            continue;
        }
        if in_pem {
            if line.contains("-----END") && line.contains("PRIVATE KEY") {
                in_pem = false;
            }
            continue;
        }
        // Pass 2: env-var name → redact the value after the first `=`/`:`.
        let mut redacted: Cow<'_, str> = Cow::Borrowed(line);
        if needles.iter().any(|n| line.contains(n))
            && let Some(eq_idx) = line.find(['=', ':'])
        {
            redacted = Cow::Owned(format!("{} [REDACTED]", &line[..=eq_idx]));
        }
        // Pass 3: token-format patterns anywhere in the (possibly redacted) line.
        for re in SECRET_TOKEN_PATTERNS.iter() {
            if re.is_match(&redacted) {
                redacted = Cow::Owned(re.replace_all(&redacted, "[REDACTED]").into_owned());
            }
        }
        out.push_str(&redacted);
        out.push('\n');
    }
    // Preserve final-newline behaviour: if s ended without \n, drop the one
    // we appended at the end of the last iteration.
    if !s.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Computes the blake3 hex digest of the captured output (64 chars).
pub fn hash_output(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Persists captured output under `~/.claude/touring/sandbox_outputs/<hash>.bin`.
pub fn store_output(content_hash: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let mut path = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    path.push(".claude/touring/sandbox_outputs");
    std::fs::create_dir_all(&path)?;
    path.push(format!("{content_hash}.bin"));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// NEW-2 — Resolves the tee directory. Honors `TOURING_TEE_DIR` env var
/// for test determinism (escapes HOME race in parallel tests); falls back
/// to `~/.claude/touring/tee/`.
pub fn tee_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("TOURING_TEE_DIR") {
        return PathBuf::from(custom);
    }
    let mut path = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    path.push(".claude/touring/tee");
    path
}

/// NEW-2 — Persists FULL captured stdout+stderr under
/// `~/.claude/touring/tee/<hash>.log` (or `TOURING_TEE_DIR` override)
/// for failure retrospection.
///
/// Called by `execute_in_sandbox` when `exit_code != 0`. The tee log
/// preserves the unredacted, uncompressed bytes so the LLM can debug
/// without re-running the failed command. Retrieval via the
/// `ctx_tee_retrieve(hash)` MCP tool. Cleanup honors
/// `TOURING_TEE_RETENTION_SECS` (default 7d).
pub fn store_tee(content_hash: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let mut path = tee_dir();
    std::fs::create_dir_all(&path)?;
    path.push(format!("{content_hash}.log"));
    // Apply secret redaction before persisting (I-12 reuse) so creds
    // captured in failed cmd stderr never leak even from tee retrieve.
    let bytes_str = std::str::from_utf8(bytes).unwrap_or_default();
    let redacted = if !bytes_str.is_empty() {
        redact_secrets(bytes_str)
    } else {
        String::new()
    };
    std::fs::write(&path, redacted.as_bytes())?;
    Ok(path)
}

/// NEW-2 — Retrieves a previously-persisted tee log for `content_hash`.
/// Returns None when no tee file exists for the hash.
pub fn read_tee(content_hash: &str) -> Option<String> {
    let mut path = tee_dir();
    path.push(format!("{content_hash}.log"));
    std::fs::read_to_string(&path).ok()
}

/// NEW-2 — Cleanup actor for tee logs older than `retention_secs`.
/// Iterates the tee directory, deletes files with mtime older than
/// `now - retention_secs`. Returns count of removed files.
pub fn cleanup_tee(retention_secs: u64) -> std::io::Result<u64> {
    let dir = tee_dir();
    if !dir.exists() {
        return Ok(0);
    }
    let now = std::time::SystemTime::now();
    let cutoff = now
        .checked_sub(std::time::Duration::from_secs(retention_secs))
        .unwrap_or(std::time::UNIX_EPOCH);
    let mut deleted = 0u64;
    for entry in std::fs::read_dir(&dir)? {
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        // `<=` so retention=0 deletes files whose mtime equals "now" — used
        // by integration tests to prove cleanup actually removes files.
        if mtime <= cutoff && std::fs::remove_file(entry.path()).is_ok() {
            deleted += 1;
        }
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── SEC-04 (2026-06-13) — credential opt-out partition ────────────────
    // `TOURING_SANDBOX_NO_CREDENTIALS` keeps ONLY `BASELINE_ENV`. This proves the
    // partition holds (every secret is outside the baseline, every baseline var
    // remains inheritable), so no-credentials mode provably withholds all secrets
    // while keeping the child launchable. Deterministic — touches no global env.
    #[test]
    fn no_credentials_mode_partitions_secrets_from_baseline() {
        for cred in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "NPM_TOKEN",
            "GOOGLE_APPLICATION_CREDENTIALS",
        ] {
            assert!(
                CREDENTIAL_ENV_WHITELIST.contains(&cred),
                "{cred} should be forwarded by default"
            );
            assert!(
                !BASELINE_ENV.contains(&cred),
                "{cred} MUST be withheld in TOURING_SANDBOX_NO_CREDENTIALS mode"
            );
        }
        for base in BASELINE_ENV {
            assert!(
                CREDENTIAL_ENV_WHITELIST.contains(base),
                "baseline {base} must stay inheritable so the child can launch"
            );
        }
    }

    // ── P4.1 — compiled-language sandbox (Go, Rust) ───────────────────────

    /// `true` when a `go` toolchain is on PATH — the Go tests soft-skip without
    /// one rather than failing on a machine that has no Go installed.
    fn go_present() -> bool {
        std::process::Command::new("go")
            .arg("version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn p41_resolve_language_args_go_rust_are_empty() {
        // Go and Rust are compiled — no inline-source argv.
        assert!(resolve_language_args(SandboxLanguage::Go, "x").is_empty());
        assert!(resolve_language_args(SandboxLanguage::Rust, "x").is_empty());
        // The interpreted languages still produce an inline-source argv.
        assert!(!resolve_language_args(SandboxLanguage::Python, "x").is_empty());
    }

    #[test]
    fn p41_sandbox_rust_missing_script_errors() {
        let err = execute_in_sandbox_blocking("SandboxRust", json!({}), SandboxConfig::default());
        assert!(matches!(err, Err(SandboxError::InvalidArgs(_))));
    }

    #[test]
    fn p41_sandbox_go_missing_script_errors() {
        let err = execute_in_sandbox_blocking("SandboxGo", json!({}), SandboxConfig::default());
        assert!(matches!(err, Err(SandboxError::InvalidArgs(_))));
    }

    #[test]
    fn p41_sandbox_rust_compiles_and_runs() {
        let result = execute_in_sandbox_blocking(
            "SandboxRust",
            json!({"script": "fn main() { println!(\"ceg-p41-rust-ok\"); }"}),
            SandboxConfig::default(),
        )
        .expect("rustc is present in a Rust workspace");
        assert_eq!(
            result.exit_code, 0,
            "a valid Rust program must compile and run"
        );
        assert!(result.output_bytes > 0);
    }

    #[test]
    fn p41_sandbox_rust_output_content_is_captured() {
        let result = execute_in_sandbox_blocking(
            "SandboxRust",
            json!({"script": "fn main() { println!(\"ceg-p41-marker-7281\"); }"}),
            SandboxConfig::default(),
        )
        .expect("compiled + ran");
        let path = result.stored_path.expect("stdout persisted to disk");
        let captured = std::fs::read_to_string(&path).expect("read stored output");
        assert!(
            captured.contains("ceg-p41-marker-7281"),
            "the program's stdout must be captured verbatim: {captured:?}"
        );
    }

    #[test]
    fn p41_sandbox_rust_compile_error_is_a_nonzero_outcome() {
        let result = execute_in_sandbox_blocking(
            "SandboxRust",
            json!({"script": "fn main() { let _x: i32 = \"not an int\"; }"}),
            SandboxConfig::default(),
        )
        .expect("the sandbox path returns Ok even for a compile failure");
        assert_ne!(result.exit_code, 0, "a Rust compile error must be non-zero");
    }

    #[test]
    fn p41_sandbox_rust_respects_the_timeout() {
        // A 200ms budget is shorter than a `rustc` compile — the compiled path
        // must honour the wall-clock timeout (sentinel exit -2).
        let result = execute_in_sandbox_blocking(
            "SandboxRust",
            json!({"script": "fn main() { loop {} }"}),
            SandboxConfig {
                timeout_ms: 200,
                ..SandboxConfig::default()
            },
        )
        .expect("a timeout is a fallback outcome, not an error");
        assert_eq!(result.exit_code, -2, "timeout must yield the -2 sentinel");
    }

    #[test]
    fn p41_sandbox_go_compiles_and_runs() {
        if !go_present() {
            return; // go toolchain absent — soft skip
        }
        let result = execute_in_sandbox_blocking(
            "SandboxGo",
            json!({"script": "package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"ceg-p41-go-ok\") }"}),
            SandboxConfig::default(),
        )
        .expect("go is present");
        assert_eq!(
            result.exit_code, 0,
            "a valid Go program must compile and run"
        );
        assert!(result.output_bytes > 0);
    }

    #[test]
    fn p41_sandbox_go_output_content_is_captured() {
        if !go_present() {
            return;
        }
        let result = execute_in_sandbox_blocking(
            "SandboxGo",
            json!({"script": "package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"ceg-p41-go-9913\") }"}),
            SandboxConfig::default(),
        )
        .expect("compiled + ran");
        let path = result.stored_path.expect("stdout persisted to disk");
        let captured = std::fs::read_to_string(&path).expect("read stored output");
        assert!(
            captured.contains("ceg-p41-go-9913"),
            "captured: {captured:?}"
        );
    }

    #[test]
    fn p41_sandbox_go_compile_error_is_a_nonzero_outcome() {
        if !go_present() {
            return;
        }
        let result = execute_in_sandbox_blocking(
            "SandboxGo",
            json!({"script": "package main\nfunc main() { this is not valid go }"}),
            SandboxConfig::default(),
        )
        .expect("the sandbox path returns Ok even for a compile failure");
        assert_ne!(result.exit_code, 0, "a Go compile error must be non-zero");
    }

    // ── P4.3 — the X5 sandbox path is rlimit-capped ───────────────────────

    #[cfg(target_os = "linux")]
    #[test]
    fn p43_x5_sandbox_path_is_rlimit_capped() {
        // P4.3 — `apply_resource_caps_to` wires rlimit into the X5 sandbox
        // path. `sandboxed_defaults` caps RLIMIT_NOFILE at 256; a Python run
        // inside the sandbox must observe that cap — proving the wiring.
        let result = execute_in_sandbox_blocking(
            "SandboxPython",
            json!({
                "script": "import resource; print(resource.getrlimit(resource.RLIMIT_NOFILE)[0])"
            }),
            SandboxConfig::default(),
        )
        .expect("python sandbox runs");
        assert_eq!(result.exit_code, 0);
        let path = result.stored_path.expect("stdout persisted");
        let captured = std::fs::read_to_string(&path).expect("read stored output");
        assert_eq!(
            captured.trim(),
            "256",
            "the X5 path must apply the rlimit NOFILE cap (256)"
        );
    }

    #[test]
    fn test_resolve_program_known_tools() {
        assert_eq!(resolve_program("Bash"), PathBuf::from("bash"));
        assert_eq!(resolve_program("Grep"), PathBuf::from("grep"));
        assert_eq!(resolve_program("Glob"), PathBuf::from("find"));
    }

    #[test]
    fn test_resolve_args_bash_ok() {
        let args = json!({"command": "echo hello"});
        let argv = resolve_args("Bash", &args).unwrap();
        assert_eq!(argv, vec!["-c".to_string(), "echo hello".into()]);
    }

    #[test]
    fn test_resolve_args_grep_missing_pattern_errors() {
        let args = json!({});
        let err = resolve_args("Grep", &args).unwrap_err();
        assert!(matches!(err, SandboxError::InvalidArgs(_)));
    }

    #[test]
    fn test_hash_output_stable_64chars() {
        let bytes = b"hello sandbox";
        let h1 = hash_output(bytes);
        let h2 = hash_output(bytes);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn test_store_output_writes_file_and_reads_back() {
        let bytes = b"persistent payload";
        let hash = hash_output(bytes);
        let path = store_output(&hash, bytes).expect("store_output");
        assert!(path.exists());
        let read = std::fs::read(&path).unwrap();
        assert_eq!(read, bytes);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_i11_language_args_python_uses_dash_c() {
        let argv = resolve_language_args(SandboxLanguage::Python, "print(2+2)");
        assert_eq!(argv[0], "-c");
        assert_eq!(argv[1], "print(2+2)");
    }

    #[test]
    fn test_i11_language_args_javascript_uses_dash_e() {
        let argv = resolve_language_args(SandboxLanguage::JavaScript, "console.log(1)");
        assert_eq!(argv[0], "-e");
    }

    #[test]
    fn test_i11_resolve_program_routes_to_language_runtime() {
        // SandboxPython tool name → bun/python3/python somewhere on PATH.
        let p = resolve_program("SandboxPython");
        let s = p.to_string_lossy();
        assert!(
            s.contains("python") || s == "python3" || s == "python",
            "resolved Python runtime should be a python binary, got {s}"
        );
    }

    #[test]
    fn test_i11_unknown_sandbox_falls_back_to_cat() {
        let p = resolve_program("UnknownTool");
        assert_eq!(p, PathBuf::from("cat"));
    }

    // ─── NEW-2 — Failure Tee Mode tests ───────────────────────────────
    // Uses TOURING_TEE_DIR override for parallel-test determinism (avoids
    // HOME race conditions across tokio tests).

    /// Process-wide lock guarding the `TOURING_TEE_DIR` env var. `std::env`
    /// mutation is global to the whole process — under `cargo test` two
    /// `test_new2_*` tests on parallel threads would otherwise stomp each
    /// other's value, and one test's `remove_var` could clear the var while
    /// another is still mid-closure. Holding this lock across the entire
    /// `set_var → closure → remove_var` window serializes exactly the
    /// env-var-sensitive tests and nothing else (a per-test `TempDir` keeps
    /// their on-disk state isolated; the *env var key* is the shared slot).
    static TEE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_tee_dir<F: FnOnce(&std::path::Path)>(f: F) -> tempfile::TempDir {
        // A panicking test poisons the mutex; recover the guard so one failing
        // test does not cascade-fail every other env-var test.
        let _env_guard = TEE_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = tempfile::TempDir::new().expect("tempdir");
        // Unique dir inside this test's isolated TempDir. The lock above — not
        // this unique value — is what prevents collision: the env var KEY
        // `TOURING_TEE_DIR` is a single global slot shared by every test.
        let unique = tmp.path().join(format!(
            "tee-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_TEE_DIR", &unique) };
        f(&unique);
        // Clear the env var while still holding the lock, so no parallel test
        // can ever observe a half-updated global.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_TEE_DIR") };
        tmp
    }

    #[test]
    fn test_new2_store_tee_persists_under_tee_dir() {
        let _t = with_tee_dir(|_dir| {
            let hash = "f".repeat(64);
            let path = store_tee(&hash, b"failure stderr trace\n").expect("store_tee");
            assert!(path.exists(), "tee file MUST exist on disk");
        });
    }

    #[test]
    fn test_new2_store_tee_redacts_secrets() {
        let _t = with_tee_dir(|_dir| {
            let hash = "g".repeat(64);
            let path = store_tee(&hash, b"GH_TOKEN=ghp_abc\nfailure trace\n").expect("store_tee");
            let read_back = std::fs::read_to_string(&path).expect("read");
            assert!(read_back.contains("[REDACTED]"));
            assert!(!read_back.contains("ghp_abc"));
        });
    }

    #[test]
    fn test_new2_read_tee_returns_persisted_content() {
        let _t = with_tee_dir(|_dir| {
            let hash = "h".repeat(64);
            let _ = store_tee(&hash, b"some failure output").expect("store_tee");
            let content = read_tee(&hash).expect("read_tee found");
            assert!(content.contains("some failure output"));
        });
    }

    #[test]
    fn test_new2_read_tee_returns_none_for_missing_hash() {
        let _t = with_tee_dir(|_dir| {
            let result = read_tee("nonexistent_hash_xyz");
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_new2_cleanup_tee_removes_old_files() {
        let _t = with_tee_dir(|_dir| {
            let hash = "i".repeat(64);
            let _ = store_tee(&hash, b"stale tee").expect("store_tee");
            // retention=0 forces cleanup of everything (file mtime <= now)
            let deleted = cleanup_tee(0).expect("cleanup_tee");
            assert!(deleted >= 1, "cleanup MUST delete the tee file");
        });
    }

    #[test]
    fn test_i12_redact_secrets_replaces_token_values() {
        let s = "GH_TOKEN=ghp_abc123xyz\nuser=alice\n";
        let red = redact_secrets(s);
        assert!(red.contains("[REDACTED]"));
        assert!(!red.contains("ghp_abc123xyz"));
        assert!(red.contains("user=alice"), "non-secret line preserved");
    }

    #[test]
    fn test_i12_redact_handles_colon_separator() {
        let s = "AWS_SECRET_ACCESS_KEY: deadbeef0123";
        let red = redact_secrets(s);
        assert!(red.contains("[REDACTED]"));
        assert!(!red.contains("deadbeef0123"));
    }

    #[test]
    fn test_i12_redact_passes_through_clean_text() {
        let s = "lorem ipsum dolor\nsit amet";
        let red = redact_secrets(s);
        assert_eq!(red, s);
    }

    #[test]
    fn test_t09_redacts_raw_token_values_with_no_env_var_name() {
        // T-09: a raw token value pasted into output (no surrounding KEY=) must
        // still be redacted by the token-format pass.
        let s = "auth failed: token ghp_0123456789abcdefghijklmnopqrstuvwxyz rejected; \
                 used sk-proj-0123456789abcdefghij and AKIA0123456789ABCDEF and \
                 Bearer xoxb-12345-abcdefghij-token";
        let red = redact_secrets(s);
        assert!(
            !red.contains("ghp_0123456789abcdefghijklmnopqrstuvwxyz"),
            "github token leaked: {red}"
        );
        assert!(
            !red.contains("sk-proj-0123456789abcdefghij"),
            "openai token leaked: {red}"
        );
        assert!(
            !red.contains("AKIA0123456789ABCDEF"),
            "aws key leaked: {red}"
        );
        assert!(
            !red.contains("xoxb-12345-abcdefghij-token"),
            "slack token leaked: {red}"
        );
        assert!(red.contains("[REDACTED]"));
    }

    #[test]
    fn test_t09_redacts_pem_private_key_block() {
        // T-09: the whole PEM block (header, base64 body, footer) is dropped;
        // surrounding non-secret lines are preserved.
        let s = "before\n-----BEGIN OPENSSH PRIVATE KEY-----\n\
                 b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAA\n\
                 AAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5\n\
                 -----END OPENSSH PRIVATE KEY-----\nafter";
        let red = redact_secrets(s);
        assert!(
            !red.contains("b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAA"),
            "pem body leaked: {red}"
        );
        assert!(!red.contains("BEGIN OPENSSH"), "pem header leaked: {red}");
        assert!(red.contains("[REDACTED PEM PRIVATE KEY]"));
        assert!(
            red.contains("before") && red.contains("after"),
            "non-secret lines around the PEM block were dropped: {red}"
        );
    }

    #[test]
    fn test_t09_no_over_redaction_of_token_like_short_words() {
        // Short/non-matching prefixes must pass through untouched (no over-redaction).
        let s = "the sk-helper module and ghp_short var are fine; AKIA alone is too short";
        let red = redact_secrets(s);
        assert_eq!(red, s, "over-redacted clean text: {red}");
    }

    #[test]
    fn test_i13_lifecycle_classifier_handles_extended_taxonomy() {
        use touring_hooks_shared::hook_events::{EventPriority, classify_priority_by_hook_name};
        // P1 CRITICAL
        assert_eq!(
            classify_priority_by_hook_name("user_decision"),
            EventPriority::Critical
        );
        // P2 HIGH
        assert_eq!(
            classify_priority_by_hook_name("blocker"),
            EventPriority::High
        );
        // P3 MEDIUM
        assert_eq!(
            classify_priority_by_hook_name("mcp_call"),
            EventPriority::Medium
        );
        // P4 LOW
        assert_eq!(
            classify_priority_by_hook_name("intent_classification"),
            EventPriority::Low
        );
    }

    #[tokio::test]
    async fn test_execute_in_sandbox_simple_bash() {
        let args = json!({"command": "printf 'sandbox-ok'"});
        let res = execute_in_sandbox("Bash", args, SandboxConfig::default())
            .await
            .expect("execute");
        assert_eq!(res.exit_code, 0);
        assert!(res.output_bytes >= 10);
        assert!(!res.content_hash.is_empty());
        if let Some(p) = &res.stored_path {
            let _ = std::fs::remove_file(p);
        }
    }

    // ── P4.4 — the X5/X8 sandbox path is bounded by the exec pool ─────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn p44_concurrent_sandbox_runs_route_through_the_exec_pool() {
        use crate::gateway::exec_pool::ExecPool;

        // Every X5 spawn takes a pool permit. Run a batch of sandbox
        // executions concurrently and prove the global pool's acquire counter
        // moved by at least the batch size — i.e. the pool IS in the path.
        const BATCH: u64 = 10;
        let before = ExecPool::global().stats().total_acquired;

        let mut handles = Vec::with_capacity(BATCH as usize);
        for i in 0..BATCH {
            handles.push(tokio::spawn(async move {
                execute_in_sandbox(
                    "Bash",
                    json!({ "command": format!("printf p44-run-{i}") }),
                    SandboxConfig::default(),
                )
                .await
            }));
        }
        for h in handles {
            let result = h.await.expect("the run task joins");
            let r = result.expect("each sandboxed run completes");
            assert_eq!(r.exit_code, 0, "the bash run must succeed");
            if let Some(p) = &r.stored_path {
                let _ = std::fs::remove_file(p);
            }
        }

        let moved = ExecPool::global().stats().total_acquired - before;
        assert!(
            moved >= BATCH,
            "all {BATCH} sandbox runs must have taken an exec-pool slot \
             (acquired moved only {moved} — pool not wired into spawn_and_capture?)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn p44_exec_pool_caps_live_subprocesses_at_the_configured_max() {
        use crate::gateway::exec_pool::{ExecPool, PoolConfig};
        use std::sync::Arc;
        use std::time::Duration;

        // A controlled pool: at most MAX subprocesses may be live at once.
        const MAX: usize = 2;
        const TASKS: usize = 8;
        let pool = Arc::new(ExecPool::new(PoolConfig::new(MAX, Duration::from_secs(10))));

        // A sampler watches in_flight while the batch runs; it must never see
        // more than MAX — that IS the "never more than N subprocesses" bound.
        let sampler = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move {
                let mut peak = 0usize;
                for _ in 0..200 {
                    peak = peak.max(pool.in_flight());
                    tokio::time::sleep(Duration::from_millis(3)).await;
                }
                peak
            })
        };

        let mut handles = Vec::with_capacity(TASKS);
        for _ in 0..TASKS {
            let pool = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                let _permit = pool.acquire().await.expect("a pool slot");
                // Hold the slot across a real subprocess, exactly as the
                // sandbox path does: acquire, spawn, wait, release-on-drop.
                let mut child = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg("sleep 0.05")
                    .spawn()
                    .expect("spawn the sh subprocess");
                child.wait().await.expect("the subprocess exits");
            }));
        }
        for h in handles {
            h.await.expect("each execution task completes");
        }
        let peak = sampler.await.expect("the sampler task joins");

        assert!(
            peak <= MAX,
            "in_flight peaked at {peak} — the cap of {MAX} was breached"
        );
        assert_eq!(
            pool.stats().total_acquired,
            TASKS as u64,
            "every task must have acquired — the bound queued the excess, none dropped"
        );
        assert_eq!(pool.in_flight(), 0, "all slots released after the batch");
    }

    // ── BUG-P0 regression (2026-05-23) ─────────────────────────────────────
    // `touring exec --sandbox '<cmd>'` previously aborted with SIGABRT because
    // `execute_in_sandbox_blocking` called `block_on` from inside the CLI's
    // tokio runtime → "Cannot start a runtime from within a runtime". The fix
    // detects the nested runtime via `Handle::try_current()` and isolates the
    // work on a fresh OS thread. This test proves the contract from both
    // contexts: nested (here, via `#[tokio::test]`) and standalone.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn execute_in_sandbox_blocking_does_not_panic_when_nested_in_runtime() {
        // We're already inside a tokio runtime; the wrapper MUST NOT panic
        // with "Cannot start a runtime from within a runtime".
        let cfg = SandboxConfig {
            timeout_ms: 5_000,
            max_output_bytes: 64_000,
            fallback_on_timeout: true,
        };
        let result = std::panic::catch_unwind(|| {
            execute_in_sandbox_blocking("Bash", json!({"command": "echo bug_p0_regression"}), cfg)
        });
        assert!(
            result.is_ok(),
            "execute_in_sandbox_blocking panicked under nested tokio runtime — BUG-P0 regression"
        );
        // The inner Result may surface a SandboxError if the host lacks bash,
        // but it MUST NOT be a panic-driven abort. Both Ok(_) and Err(_) are
        // acceptable here; what fails this test is the panic path.
        let _ = result.unwrap();
    }

    #[test]
    fn execute_in_sandbox_blocking_works_in_standalone_thread() {
        // The common (non-nested) path must keep working — no runtime here.
        let cfg = SandboxConfig {
            timeout_ms: 5_000,
            max_output_bytes: 64_000,
            fallback_on_timeout: true,
        };
        let _ =
            execute_in_sandbox_blocking("Bash", json!({"command": "echo standalone_path"}), cfg);
        // We don't assert on the result content; we only assert the call
        // returned (did not abort the process).
    }
}
