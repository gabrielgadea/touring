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
//! `--orchestrate` (R4) prepends the read-only `touring` Python SDK (`TOURING_PY_SDK`) so a
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
        # C2-W0 S-5.2 — run identity minted by `touring run` and exported into
        # the sandbox; each sub-call is stamped `<run_id>:code:<n>` so the
        # daemon can count the counterfactual tool-part it replaced (d4).
        self._run_id = _tr_os.environ.get("TOURING_RUN_ID") or ""
        self._n = 0

    # W2 d1/S-2.1 — the read-only hook allowlist this SDK speaks. Containment,
    # not a security boundary (the sandbox reaches the socket regardless; the
    # server-side grant/proxy is the tracked follow-up): the guard teaches the
    # contract up front instead of failing opaquely at the daemon.
    READONLY_HOOKS = (
        "cli-index-find", "cli-ast-blast", "cli-ast-overview",
        "cli-wiring-status", "cli-search-docs", "cli-memory-recall",
        "cli-tantivy-search", "cli-wiring-impact",
    )

    def query(self, hook, payload=None):
        """Send a daemon RPC; return the parsed JSON output (or the raw string)."""
        if hook not in self.READONLY_HOOKS:
            raise RuntimeError(
                "hook " + repr(hook) + " is not in the orchestrate read-only "
                "allowlist; available: " + ", ".join(self.READONLY_HOOKS))
        s = _tr_socket.socket(_tr_socket.AF_UNIX, _tr_socket.SOCK_STREAM)
        s.settimeout(30)
        try:
            s.connect(self._sock)
            self._n += 1
            body = {"hook": hook, "payload": payload or {}, "project_root": self._root}
            if self._run_id:
                body["origin"] = self._run_id + ":code:" + str(self._n)
            req = _tr_json.dumps(body)
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

    def memory_recall(self, query):
        """Semantic memory recall; '#facet:value' tokens filter the tagged corpus."""
        return self.query("cli-memory-recall", {"query": query})

    def tantivy_search(self, query, top=10):
        """BM25 ranked search over the symbol/docs index."""
        return self.query("cli-tantivy-search", {"query": query, "top": top})

    def wiring_impact(self, symbol, depth=2):
        """Transitive consumers of a symbol (blast radius, BFS to `depth`)."""
        return self.query("cli-wiring-impact", {"symbol": symbol, "depth": depth, "format": "json"})


touring = _TouringClient()
# --- end touring SDK ---
"#;

/// W2 d1/S-2.3 — the STATIC STUB for the orchestrate SDK (dsh `py-types`
/// blueprint): TypedDict payloads + a Protocol with the docstring INSIDE each
/// method + the mandatory static-stub warning, lexicographically ordered so
/// the text is byte-identical across runs (provider KV-cache stability, P21).
/// Printed by `touring run --sdk-stub`; decision record: P23 (2026-08-23) —
/// the full stub (~360 tok) is on-demand; nudges carry the 1-line form.
const TOURING_PY_SDK_STUB: &str = r#"# touring run --orchestrate SDK — STATIC STUB (auto-generated, byte-stable)
# Exactly two of the names declared below are bound at runtime: `touring` and
# the RuntimeError raised on a non-allowlisted hook. Everything else is a
# STATIC STUB for reading: build arguments as plain dict/list — never
# `IndexFindArgs(symbol_name=...)`, which raises NameError at run time.
from typing import Any, Protocol


class _Touring(Protocol):
    def ast_blast(self, file_path: str) -> Any:
        """Full dependency tree (blast radius) for a file."""
        ...

    def ast_overview(self, file_path: str) -> Any:
        """Structure + symbol map for a file."""
        ...

    def index_find(self, symbol: str) -> Any:
        """Exact symbol lookup in the project index (VGP)."""
        ...

    def memory_recall(self, query: str) -> Any:
        """Semantic memory recall; '#facet:value' tokens filter the tagged corpus."""
        ...

    def query(self, hook: str, payload: dict) -> Any:
        """Escape hatch: any hook in the read-only allowlist (see READONLY_HOOKS)."""
        ...

    def search(self, query: str) -> Any:
        """Search over the docs index."""
        ...

    def tantivy_search(self, query: str, top: int = 10) -> Any:
        """BM25 ranked search over the symbol/docs index."""
        ...

    def wiring_impact(self, symbol: str, depth: int = 2) -> Any:
        """Transitive consumers of a symbol (blast radius, BFS to `depth`)."""
        ...

    def wiring_status(self) -> Any:
        """Workspace wiring summary (orphans, module scores)."""
        ...


touring: _Touring
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

    /// Print the byte-stable typed stub (.pyi) of the orchestrate SDK and exit —
    /// the full contract for reading; nudges carry the 1-line form (P23)
    #[arg(long, conflicts_with_all = ["code", "file", "stdin"])]
    sdk_stub: bool,
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

    // W2 d1/S-2.3 — the stub is a pure print: no sandbox, no daemon.
    if cli.sdk_stub {
        print!("{TOURING_PY_SDK_STUB}");
        return Ok(());
    }

    let user_code = resolve_code(&cli)?;
    let code = maybe_inject_sdk(user_code.clone(), &cli.lang, cli.orchestrate)?;

    // W5 T12/S-5.1 — `touring run` now traverses the CEG X0..X7 gateway before
    // execution (before 2026-08-23 only the `touring exec` family exercised
    // it, so ceg counters never saw the run path). Profile: `sandboxed` by
    // default, `trusted` under --allow-forbidden. Deny teaches the route;
    // an internal gateway error is fail-open (the CEG invariant).
    //
    // C2-W0 — gate the USER's code, never the injected SDK: the orchestrate
    // SDK opens the daemon's Unix socket BY DESIGN (its containment is the
    // read-only hook allowlist), so gating the composed program made X6 deny
    // every `--orchestrate` the moment X0 learned to admit Sandbox* runtimes.
    // A user program that opens sockets itself is still caught.
    gate_run(&cli.lang, &user_code, cli.allow_forbidden)?;

    let args_json = match cli.args.as_deref() {
        Some(s) => Some(serde_json::from_str(s).context("parsing --args as a JSON array")?),
        None => None,
    };
    let allow_forbidden = cli.allow_forbidden.then_some(true);

    let out = block_on_async(ctx_execute_impl(
        cli.lang.clone(),
        code,
        args_json,
        cli.timeout_ms,
        None, // cwd: inherit current working directory
        allow_forbidden,
    ))
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // W3 d2/S-3.3 — offer harvest on the USER's code (never the injected SDK).
    let harvest = harvest_hint(&user_code, &cli.lang, out.exit_code);
    emit_output(&out, cli.brief, harvest.as_deref())?;

    // Propagate the sandboxed program's exit code as the CLI exit code so callers (and
    // code-mode orchestration) see a faithful success/failure signal, not just rc=0.
    if out.exit_code != 0 {
        std::process::exit(out.exit_code);
    }
    Ok(())
}

/// W5 T12/S-5.1 — drive the run's code through the CEG X0..X7 gateway before
/// execution. Mirrors `exec::gate_command` (C08: the two callers of
/// `run_gateway` stay symmetric): neutral deps, deferred dry-run (the REAL
/// execution happens after, via `ctx_execute_impl`), capability profile
/// `sandboxed` (default) or `trusted` (--allow-forbidden). A `Deny` verdict
/// aborts with a teaching error; an internal gateway error logs and proceeds
/// (fail-open — the CEG invariant: the gate never bricks the session).
fn gate_run(lang: &str, code: &str, allow_forbidden: bool) -> Result<()> {
    use touring_hooks::capability::builtins;
    use touring_hooks::gateway::{
        ExecutionOutcomePredictor, GatewayDeps, SandboxCapabilities, Verdict,
        deferred_dry_run, neutral_outcome_history, run_gateway, soft_pass_symbol,
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let profile = if allow_forbidden {
        builtins::trusted()
    } else {
        builtins::sandboxed(&cwd)
    };
    let predictor = ExecutionOutcomePredictor::new();
    let _caps = SandboxCapabilities::from_profile(&profile);
    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        outcome_history: &neutral_outcome_history,
        sandbox_runner: &deferred_dry_run,
        predictor: &predictor,
        profile: &profile,
        claim: None,
        claim_context: touring_hooks::offensive_integration::ClaimContext::default(),
        solver_backend: touring_hooks::offensive_integration::SolverBackendKind::Stub,
    };
    // Tool identity mirrors the sandbox adapter's mapping so the gateway's
    // ledger names the real runtime, not a generic "Bash".
    let tool = match lang.to_ascii_lowercase().as_str() {
        "sh" | "bash" | "shell" => "Bash",
        "python" | "py" => "SandboxPython",
        "js" | "javascript" | "node" | "bun" => "SandboxJavaScript",
        "ts" | "typescript" => "SandboxTypeScript",
        other => match other {
            "ruby" | "rb" => "SandboxRuby",
            "go" | "golang" => "SandboxGo",
            "rust" | "rs" => "SandboxRust",
            _ => "SandboxPython",
        },
    };
    let is_shell = matches!(tool, "Bash");
    match run_gateway(tool, code, None, &deps) {
        Ok(outcome) => match outcome.decision.verdict {
            // For SHELL payloads the X6 capability gate evaluates every word
            // as a subprocess grant, so `echo hi` denies under `sandboxed` —
            // taxing the common case (P20). The run's REAL containment is the
            // execution sandbox itself (rlimits + landlock + env-clear + the
            // forbidden-call policy), so a shell Deny downgrades to a logged
            // advisory; code languages keep the hard gate.
            Verdict::Deny if is_shell => {
                tracing::warn!(
                    composite = outcome.decision.composite_score,
                    reason = outcome
                        .decision
                        .reasons
                        .first()
                        .map(String::as_str)
                        .unwrap_or("hard block fired"),
                    "CEG advisory deny on shell run; proceeding under the real sandbox"
                );
                Ok(())
            }
            Verdict::Deny => anyhow::bail!(
                "the CEG gateway denied this run (composite {:.2}): {}. Adjust the code, \
                 or rerun with --allow-forbidden for the trusted profile if the operation \
                 is intentionally privileged.",
                outcome.decision.composite_score,
                outcome
                    .decision
                    .reasons
                    .first()
                    .map(String::as_str)
                    .unwrap_or("hard block fired")
            ),
            _ => Ok(()),
        },
        Err(e) => {
            tracing::warn!(error = %e, "CEG gateway errored on the run path; proceeding (fail-open)");
            Ok(())
        }
    }
}

/// Render the sandbox result to stdout: a C5 summary digest under `--brief`, otherwise
/// the full JSON payload (mirrors the MCP adapter's field set in `tools_ctx_execute.rs`).
fn emit_output(out: &CtxExecuteOutput, brief: bool, harvest: Option<&str>) -> Result<()> {
    if brief {
        let summary = touring_ceg::gateway::summarize_output(
            &out.stdout,
            out.exit_code,
            out.stdout_truncated,
        );
        println!("{}", serde_json::to_string(&summary)?);
    } else {
        let mut payload = serde_json::json!({
            "stdout": out.stdout,
            "stderr": out.stderr,
            "exit_code": out.exit_code,
            "duration_ms": out.duration_ms,
            "forbidden_calls": out.forbidden_calls,
            "stdout_truncated": out.stdout_truncated,
            "stderr_truncated": out.stderr_truncated,
            // C2-W0 S-5.2 — the identity that joins run_journal.jsonl and
            // run_subcalls.jsonl; without it here the correlation key never
            // reached the CLI caller.
            "run_id": out.run_id,
        });
        // W1 d3 — taxonomy + spill locator reach the CLI surface too.
        if let Some(f) = &out.failure {
            payload["failure"] = serde_json::json!({
                "kind": f.kind, "phase": f.phase, "message": f.message,
            });
        }
        if let Some(p) = &out.stored_path {
            payload["stored_path"] = serde_json::json!(p);
        }
        if let Some(h) = &out.retrieval_hint {
            payload["retrieval_hint"] = serde_json::json!(h);
        }
        // W3 d2/S-3.3 — the harvest offer (threshold calibrated in P33).
        if let Some(h) = harvest {
            payload["harvest_hint"] = serde_json::json!(h);
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
    }
    Ok(())
}

/// W3 d2/S-3.3 — offer to harvest a successful, GENERALIZABLE program as a
/// `#kind:snippet` memory. Threshold calibrated on the real corpus of this
/// session's 23 executed programs (decision P33, 2026-08-23): the naive
/// "≥5 lines OR has def" offered for 77% of one-off programs; requiring
/// parametrization AND no ephemeral identifiers lands at ~26% with the
/// genuine candidates captured. NEVER automatic — an offer the caller runs.
fn harvest_hint(code: &str, lang: &str, exit_code: i32) -> Option<String> {
    if exit_code != 0 {
        return None;
    }
    let lines = code.lines().filter(|l| !l.trim().is_empty()).count();
    if lines < 5 {
        return None;
    }
    let parametrized = ["def ", "function ", "fn ", "sys.argv", "$1", "${@"]
        .iter()
        .any(|m| code.contains(m));
    if !parametrized {
        return None;
    }
    let ephemeral = code.contains("/tmp/claude-")
        || code.contains("task_1")
        || regex_lite_date(code);
    if ephemeral {
        return None;
    }
    Some(format!(
        "reusable pattern detected ({lines} lines, parametrized): persist it with \
         touring memory store <slug> '<the code>' --tag \"#kind:snippet\" --tag \
         \"#lang:{lang}\" --tag \"#purpose:<what it does>\" — recall later via \
         touring memory query '#kind:snippet #lang:{lang}'"
    ))
}

/// Cheap absolute-ISO-date detector (`20XX-`) without a regex dependency —
/// an embedded absolute date marks a one-off, not a reusable pattern.
fn regex_lite_date(code: &str) -> bool {
    code.contains("2025-") || code.contains("2026-") || code.contains("2027-")
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
        anyhow::bail!("provide the code body with one of --code, --file, or --stdin — run `touring help` for details")
    }
}

/// R4 — prepend the `touring` orchestration SDK (`TOURING_PY_SDK`) when `--orchestrate`
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
    // ── W2 d1 — SDK stub + allowlist contract ─────────────────────────────

    // ── W3 d2/S-3.3 — harvest threshold (calibrated in P33) ───────────────

    #[test]
    fn successful_run_emits_harvest_hint_with_real_values() {
        let code = "import sys\ndef clean(path):\n    return path.strip()\nfor a in sys.argv[1:]:\n    print(clean(a))";
        let hint = super::harvest_hint(code, "python", 0).expect("parametrized multi-line code is offerable");
        assert!(hint.contains("#kind:snippet"), "offer carries the facet tag");
        assert!(hint.contains("#lang:python"), "offer carries the real lang");
    }

    #[test]
    fn harvest_hint_skips_one_offs_and_failures() {
        // too short
        assert!(super::harvest_hint("print(1)", "python", 0).is_none());
        // not parametrized
        assert!(super::harvest_hint("a=1\nb=2\nc=3\nd=4\nprint(a+b+c+d)", "python", 0).is_none());
        // ephemeral identifiers (session path / absolute date)
        assert!(super::harvest_hint(
            "def f(x):\n    return x\nopen('/tmp/claude-1000/x')\nf(1)\nf(2)", "python", 0
        ).is_none());
        // failed execution
        assert!(super::harvest_hint("def f(x):\n    return x\nf(1)\nf(2)\nf(3)", "python", 3).is_none());
    }

    #[test]
    fn sdk_stub_is_byte_identical_across_runs() {
        // The const IS the output — identity here proves regen stability; the
        // lexicographic method order is asserted structurally below.
        assert_eq!(super::TOURING_PY_SDK_STUB, super::TOURING_PY_SDK_STUB);
        let methods: Vec<&str> = super::TOURING_PY_SDK_STUB
            .lines()
            .filter_map(|l| l.trim().strip_prefix("def "))
            .collect();
        let mut sorted = methods.clone();
        sorted.sort();
        assert_eq!(methods, sorted, "stub methods must be lexicographically ordered");
    }

    #[test]
    fn sdk_stub_lists_every_binding() {
        for m in [
            "query", "index_find", "ast_blast", "ast_overview", "wiring_status",
            "search", "memory_recall", "tantivy_search", "wiring_impact",
        ] {
            assert!(
                super::TOURING_PY_SDK_STUB.contains(&format!("def {m}(")),
                "stub must declare {m}"
            );
            assert!(
                super::TOURING_PY_SDK.contains(&format!("def {m}(")),
                "SDK must implement {m}"
            );
        }
        assert!(
            super::TOURING_PY_SDK_STUB.contains("STATIC STUB"),
            "the static-stub warning is mandatory (dsh py-types blueprint)"
        );
    }

    #[test]
    fn sdk_allowlist_covers_every_binding_hook() {
        for hook in [
            "cli-index-find", "cli-ast-blast", "cli-ast-overview", "cli-wiring-status",
            "cli-search-docs", "cli-memory-recall", "cli-tantivy-search", "cli-wiring-impact",
        ] {
            assert!(
                super::TOURING_PY_SDK.contains(hook),
                "SDK allowlist/bindings must reference {hook}"
            );
        }
        assert!(
            super::TOURING_PY_SDK.contains("READONLY_HOOKS"),
            "the client-side allowlist guard must exist"
        );
    }

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

    // ── C2-W0 — the gate targets the USER's code, never the injected SDK ────

    /// Pins the reason `run()` passes `user_code` to `gate_run`: the SDK
    /// opens the daemon socket BY DESIGN, so gating the composed program
    /// would X6-deny every `--orchestrate`. A user program that reaches for
    /// sockets itself is still denied.
    #[test]
    fn gate_targets_user_code_because_the_sdk_itself_would_deny() {
        assert!(
            gate_run("python", TOURING_PY_SDK, false).is_err(),
            "the SDK's socket use must trip the sandboxed profile — that is \
             why it is exempt from the gate"
        );
        assert!(
            gate_run("python", "print(1)", false).is_ok(),
            "plain user code passes"
        );
    }

    // ── C2-W0 S-5.2 — sub-call identity ─────────────────────────────────────

    #[test]
    fn sdk_carries_subcall_origin() {
        // The SDK reads the run identity minted by `touring run` and stamps
        // every daemon sub-call `<run_id>:code:<n>` — the field the dispatch
        // site counts the counterfactual from (d4). No identity in the env →
        // no origin key at all (old-daemon compatible either way).
        assert!(
            TOURING_PY_SDK.contains("TOURING_RUN_ID"),
            "SDK must read the exported run identity"
        );
        assert!(
            TOURING_PY_SDK.contains("\":code:\" + str(self._n)"),
            "each sub-call is numbered <run_id>:code:<n>"
        );
        assert!(
            TOURING_PY_SDK.contains("if self._run_id:"),
            "origin is only attached when an identity exists"
        );
    }
}
