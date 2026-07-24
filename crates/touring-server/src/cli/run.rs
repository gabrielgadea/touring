//! R1 — `touring run`: CLI adapter over the sandbox execution engine (code-mode without MCP).
//!
//! Mirrors the MCP adapter `server/tools_ctx_execute.rs` over the **same** engine
//! (`tools::ctx_execute_tools::ctx_execute_impl`). This is the CLI channel the adoption
//! diagnosis identified as missing: the engine existed but its only surface was the MCP
//! server, which was not always connected. See
//! `docs/2026-06-27-coupling-codemode-cli-and-master-commands.md` §3 (R1).
//!
//! `--brief` applies the C5 Active Summarizer (`touring_ceg::gateway::summarize_output`)
//! so the model receives a < ~200-token digest that NEVER masks a failure (exit_code +
//! error lines are preserved verbatim).
//!
//! `--orchestrate` (R4) prepends the read-only `touring` Python SDK ([`TOURING_PY_SDK`]) so a
//! single sandboxed script queries the daemon over its socket — orchestration-in-code WITHOUT
//! MCP. See `docs/2026-06-27-coupling-codemode-cli-and-master-commands.md` §3 (Camada 2 / R4).

use anyhow::{Context, Result};
use clap::Parser;

use crate::tools::ctx_execute_tools::{CtxExecuteOutput, ctx_execute_impl};

/// R4 — the `touring` orchestration SDK, injected ahead of the user's Python body by
/// `--orchestrate`. It speaks the daemon's newline-delimited JSON RPC over the Unix
/// socket (`daemon_client` wire format), so one sandboxed script can query the whole
/// stack in a single execution — orchestration-in-code WITHOUT MCP (the −60-85% token
/// win of CodeAct). Read-only query helpers only; `query()` is the generic escape hatch.
///
/// Security note: the sandbox already reaches the daemon socket (R1 — `socket` is not a
/// forbidden primitive and landlock permits `/tmp`), so `--orchestrate` adds *ergonomics*
/// (the SDK), not a new capability. Hardening the socket behind an explicit grant + a
/// server-side read-only hook allowlist is tracked as a follow-up (MAESTRO mitigation).
const TOURING_PY_SDK: &str = r#"# --- touring orchestration SDK (injected by `touring run --orchestrate`) ---
import socket as _tr_socket, os as _tr_os, json as _tr_json


class _TouringClient:
    """In-sandbox, read-only client to the touring daemon (the shared context store).

    Talks the daemon's newline-delimited JSON RPC over its Unix socket so a sandboxed
    script orchestrates the stack in ONE execution — code-mode WITHOUT MCP.
    """

    def __init__(self):
        self._sock = _tr_os.environ.get("TOURING_DAEMON_SOCKET") or _tr_os.environ.get("TOURING_DAEMON_SOCK") or f"/tmp/touring-daemon-{_tr_os.getuid()}.sock"
        self._root = _tr_os.environ.get("TOURING_PROJECT_ROOT") or _tr_os.getcwd()

    def query(self, hook, payload=None):
        """Send a daemon RPC; return the parsed JSON output (or the raw string)."""
        s = _tr_socket.socket(_tr_socket.AF_UNIX, _tr_socket.SOCK_STREAM)
        s.settimeout(30)
        try:
            s.connect(self._sock)
            req = _tr_json.dumps({"hook": hook, "payload": payload or {}, "project_root": self._root})
            s.sendall(req.encode() + b"\n")
            buf = b""
            while not buf.endswith(b"\n"):
                chunk = s.recv(65536)
                if not chunk:
                    break
                buf += chunk
        finally:
            s.close()
        resp = _tr_json.loads(buf.decode())
        if not resp.get("success"):
            raise RuntimeError("touring daemon returned success=false for hook " + repr(hook))
        out = resp.get("output", "")
        if not out:
            return None
        try:
            return _tr_json.loads(out)
        except _tr_json.JSONDecodeError:
            return out

    def index_find(self, symbol):
        return self.query("cli-index-find", {"symbol_name": symbol})

    def ast_blast(self, file_path):
        return self.query("cli-ast-blast", {"file_path": file_path})

    def ast_overview(self, file_path):
        return self.query("cli-ast-overview", {"file_path": file_path})

    def wiring_status(self):
        return self.query("cli-wiring-status", {})

    def search(self, query):
        return self.query("cli-search-docs", {"query": query})


touring = _TouringClient()
# --- end touring SDK ---
"#;

/// `touring run` — execute code in the deny-by-default sandbox (11 languages,
/// forbidden-call detection, 1 MB output cap). The code-mode channel without MCP.
#[derive(Parser, Debug)]
#[command(
    name = "run",
    about = "Execute code in the touring sandbox (code-mode without MCP)",
    long_about = "Run code in the deny-by-default sandbox via the same engine as the \
                  touring_ctx_execute MCP tool. Supply the body with one of --code / \
                  --file / --stdin. Languages: python, js/node, ts/bun, ruby, go, rust, \
                  perl, r, elixir, php, bash/sh."
)]
struct RunCli {
    /// Language: python | js | node | ts | bun | ruby | go | rust | perl | r | elixir | php | bash | sh
    #[arg(long)]
    lang: String,

    /// Inline code body (mutually exclusive with --file / --stdin)
    #[arg(long, conflicts_with_all = ["file", "stdin"])]
    code: Option<String>,

    /// Read the code body from a file
    #[arg(long, conflicts_with_all = ["code", "stdin"])]
    file: Option<String>,

    /// Read the code body from stdin
    #[arg(long, conflicts_with_all = ["code", "file"])]
    stdin: bool,

    /// JSON array of argv passed to the program, e.g. '["a","b"]'
    #[arg(long)]
    args: Option<String>,

    /// Wall-clock timeout in milliseconds (engine default 30000, max 120000)
    #[arg(long)]
    timeout_ms: Option<u64>,

    /// Permit forbidden primitives (fs.write*, subprocess, eval); off by default
    #[arg(long)]
    allow_forbidden: bool,

    /// Compact output: C5 summary digest (< ~200 tokens; preserves exit_code + error lines)
    #[arg(long)]
    brief: bool,

    /// Inject the read-only `touring` orchestration SDK (Python) so the script can query the
    /// daemon over its socket in one execution — code-mode orchestration WITHOUT MCP (R4)
    #[arg(long)]
    orchestrate: bool,
}

/// Bridge the synchronous CLI dispatch to the async engine. `main.rs` builds a
/// multi-thread runtime, so handlers run *inside* it; a fresh `Runtime::new()` would
/// panic with "Cannot start a runtime from within a runtime". Reuse the current handle
/// via `block_in_place`, falling back to a fresh runtime only outside one (unit tests).
/// Mirrors `generate::block_on_async`.
fn block_on_async<F: std::future::Future>(future: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
        _ => tokio::runtime::Runtime::new()
            .expect("create tokio runtime for non-runtime context")
            .block_on(future),
    }
}

/// CLI entry point for `touring run`. `args[0]` = binary, `args[1]` = "run"; clap parses
/// `args[1..]` with "run" acting as the program name (same convention as `generate::run`).
pub fn run(args: &[String]) -> Result<()> {
    let cli = match RunCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    let code = resolve_code(&cli)?;
    let code = maybe_inject_sdk(code, &cli.lang, cli.orchestrate)?;

    let args_json = match cli.args.as_deref() {
        Some(s) => Some(serde_json::from_str(s).context("parsing --args as a JSON array")?),
        None => None,
    };
    let allow_forbidden = cli.allow_forbidden.then_some(true);

    let out = block_on_async(ctx_execute_impl(
        cli.lang,
        code,
        args_json,
        cli.timeout_ms,
        None, // cwd: inherit current working directory
        allow_forbidden,
    ))
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    emit_output(&out, cli.brief)?;

    // Propagate the sandboxed program's exit code as the CLI exit code so callers (and
    // code-mode orchestration) see a faithful success/failure signal, not just rc=0.
    if out.exit_code != 0 {
        std::process::exit(out.exit_code);
    }
    Ok(())
}

/// Render the sandbox result to stdout: a C5 summary digest under `--brief`, otherwise
/// the full JSON payload (mirrors the MCP adapter's field set in `tools_ctx_execute.rs`).
fn emit_output(out: &CtxExecuteOutput, brief: bool) -> Result<()> {
    if brief {
        let summary = touring_ceg::gateway::summarize_output(
            &out.stdout,
            out.exit_code,
            out.stdout_truncated,
        );
        println!("{}", serde_json::to_string(&summary)?);
    } else {
        let payload = serde_json::json!({
            "stdout": out.stdout,
            "stderr": out.stderr,
            "exit_code": out.exit_code,
            "duration_ms": out.duration_ms,
            "forbidden_calls": out.forbidden_calls,
            "stdout_truncated": out.stdout_truncated,
            "stderr_truncated": out.stderr_truncated,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    }
    Ok(())
}

/// Resolve the code body from exactly one of `--code` / `--file` / `--stdin`.
fn resolve_code(cli: &RunCli) -> Result<String> {
    if let Some(code) = &cli.code {
        Ok(code.clone())
    } else if let Some(file) = &cli.file {
        std::fs::read_to_string(file).with_context(|| format!("reading --file {file}"))
    } else if cli.stdin {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading code from stdin")?;
        Ok(buf)
    } else {
        anyhow::bail!("provide the code body with one of --code, --file, or --stdin")
    }
}

/// R4 — prepend the `touring` orchestration SDK ([`TOURING_PY_SDK`]) when `--orchestrate`
/// is set, so the script can call `touring.search(...)`, `touring.index_find(...)`, … against
/// the daemon in a single execution. The SDK is Python, so `--orchestrate` requires
/// `--lang python` (a clear error rather than a silent no-op for other languages).
fn maybe_inject_sdk(code: String, lang: &str, orchestrate: bool) -> Result<String> {
    if !orchestrate {
        return Ok(code);
    }
    let canon = lang.trim().to_ascii_lowercase();
    if canon != "python" && canon != "py" {
        anyhow::bail!(
            "--orchestrate currently supports --lang python only (the touring SDK is Python); got {lang:?}"
        );
    }
    Ok(format!("{TOURING_PY_SDK}\n{code}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lang_and_inline_code() {
        let cli = RunCli::try_parse_from(["run", "--lang", "python", "--code", "print(1)"])
            .expect("parse");
        assert_eq!(cli.lang, "python");
        assert_eq!(cli.code.as_deref(), Some("print(1)"));
        assert!(!cli.brief);
    }

    #[test]
    fn code_and_file_are_mutually_exclusive() {
        assert!(
            RunCli::try_parse_from(["run", "--lang", "python", "--code", "x", "--file", "y"])
                .is_err()
        );
    }

    #[test]
    fn brief_flag_parsed() {
        let cli = RunCli::try_parse_from(["run", "--lang", "bash", "--code", "echo hi", "--brief"])
            .expect("parse");
        assert!(cli.brief);
    }

    #[test]
    fn resolve_code_requires_a_source() {
        let cli = RunCli::try_parse_from(["run", "--lang", "python"]).expect("parse");
        assert!(resolve_code(&cli).is_err());
    }

    #[test]
    fn resolve_code_returns_inline_body() {
        let cli = RunCli::try_parse_from(["run", "--lang", "python", "--code", "print(42)"])
            .expect("parse");
        assert_eq!(resolve_code(&cli).expect("inline body"), "print(42)");
    }

    // ── R4 — `--orchestrate`: inject the read-only `touring` Python SDK ──────────

    #[test]
    fn orchestrate_flag_parsed() {
        let cli =
            RunCli::try_parse_from(["run", "--lang", "python", "--code", "x", "--orchestrate"])
                .expect("parse");
        assert!(cli.orchestrate);
        let off =
            RunCli::try_parse_from(["run", "--lang", "python", "--code", "x"]).expect("parse");
        assert!(!off.orchestrate);
    }

    #[test]
    fn sdk_injected_only_with_orchestrate() {
        let plain = maybe_inject_sdk("print(1)".to_string(), "python", false).expect("no-op");
        assert_eq!(
            plain, "print(1)",
            "without --orchestrate the body is untouched"
        );
        let injected = maybe_inject_sdk("print(1)".to_string(), "python", true).expect("inject");
        assert!(injected.contains("_TouringClient"), "SDK prepended");
        assert!(
            injected.contains("touring = _TouringClient()"),
            "global `touring` is defined"
        );
        assert!(
            injected.ends_with("print(1)"),
            "user body kept verbatim after the SDK"
        );
    }

    #[test]
    fn orchestrate_requires_python() {
        assert!(
            maybe_inject_sdk("echo hi".to_string(), "bash", true).is_err(),
            "--orchestrate must reject non-Python languages"
        );
        // the `py` alias is accepted
        assert!(maybe_inject_sdk("x".to_string(), "py", true).is_ok());
    }

    #[test]
    fn sdk_uses_correct_daemon_hook_names() {
        // VGP: the SDK's payload keys must match the daemon's CLI hooks (verified via
        // the handlers: index.rs / snapshot.rs / clones.rs / eval.rs).
        assert!(TOURING_PY_SDK.contains("\"cli-index-find\", {\"symbol_name\":"));
        assert!(TOURING_PY_SDK.contains("\"cli-ast-blast\", {\"file_path\":"));
        assert!(TOURING_PY_SDK.contains("\"cli-search-docs\", {\"query\":"));
        assert!(TOURING_PY_SDK.contains("\"cli-wiring-status\", {}"));
    }
}
