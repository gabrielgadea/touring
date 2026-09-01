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
//! `--orchestrate` (R4) prepends the read-only `touring` Python SDK (`py_sdk()`) so a
//! single sandboxed script queries the daemon over its socket — orchestration-in-code WITHOUT
//! MCP. See `docs/2026-06-27-coupling-codemode-cli-and-master-commands.md` §3 (Camada 2 / R4).

use anyhow::{Context, Result};
use clap::Parser;

use crate::tools::ctx_execute_tools::{CtxExecuteOutput, ctx_execute_impl};

/// S4 SURFACE — a allowlist de leitura, reexportada da FONTE ÚNICA.
///
/// Ela mora em [`touring_foundation::orchestrate_allowlist`] desde 27/08/2026
/// porque DOIS lados precisam da mesma lista: este, que RENDERIZA o guard no
/// SDK, e o daemon, que o IMPÕE. Enquanto só existia aqui (e, pior, só dentro
/// da string Python), o guard era um atributo mutável do cliente — um programa
/// no sandbox reescrevia `touring.READONLY_HOOKS` e chamava o que quisesse.
use touring_foundation::orchestrate_allowlist::READONLY_HOOKS;

/// S4 — one typed shortcut of the orchestrate SDK.
///
/// `hook: None` marks `query` itself: it is declared in the stub (so the escape
/// hatch is discoverable) but hand-written in the client, since it IS the
/// transport the other methods are built on.
struct SdkMethod {
    /// Python method name — also the stub's sort key.
    name: &'static str,
    /// Daemon hook it calls; `None` only for `query`.
    hook: Option<&'static str>,
    /// Implementation signature, e.g. `"self, symbol"`.
    args: &'static str,
    /// Stub signature with type hints, e.g. `"self, symbol: str"`.
    stub_args: &'static str,
    /// Payload dict literal passed to `query`.
    payload: &'static str,
    /// One-line docstring, shared by implementation and stub.
    doc: &'static str,
}

const SDK_METHODS: &[SdkMethod] = &[
    SdkMethod {
        name: "ast_blast",
        hook: Some("cli-ast-blast"),
        args: "self, file_path",
        stub_args: "self, file_path: str",
        payload: r#"{"file_path": file_path}"#,
        doc: "Full dependency tree (blast radius) for a file.",
    },
    SdkMethod {
        name: "ast_meta",
        hook: Some("cli-ast-meta"),
        args: "self, file_path",
        stub_args: "self, file_path: str",
        payload: r#"{"file_path": file_path}"#,
        doc: "File metadata first: blast_radius, quality/cognitive score, fan-in/fan-out.",
    },
    SdkMethod {
        name: "ast_overview",
        hook: Some("cli-ast-overview"),
        args: "self, file_path",
        stub_args: "self, file_path: str",
        payload: r#"{"file_path": file_path}"#,
        doc: "Structure + symbol map for a file.",
    },
    SdkMethod {
        name: "ast_tdg",
        hook: Some("cli-ast-tdg"),
        args: "self, file_path",
        stub_args: "self, file_path: str",
        payload: r#"{"file_path": file_path}"#,
        doc: "Technical-debt grade A+..F over 6 dimensions.",
    },
    SdkMethod {
        name: "doctor",
        hook: Some("cli-doctor"),
        args: "self",
        stub_args: "self",
        payload: r#"{}"#,
        doc: "Daemon/index health check (the FASE 0 gate).",
    },
    SdkMethod {
        name: "find_references",
        hook: Some("cli-find-references"),
        args: "self, symbol",
        stub_args: "self, symbol: str",
        payload: r#"{"symbol": symbol}"#,
        doc: "Every reference to a symbol across the project.",
    },
    SdkMethod {
        name: "gotcha_match",
        hook: Some("cli-gotcha-match"),
        args: "self, file_path",
        stub_args: "self, file_path: str",
        payload: r#"{"file_path": file_path}"#,
        doc: "Known pitfalls recorded for this file.",
    },
    SdkMethod {
        name: "index_find",
        hook: Some("cli-index-find"),
        args: "self, symbol",
        stub_args: "self, symbol: str",
        payload: r#"{"symbol_name": symbol}"#,
        doc: "Exact symbol lookup in the project index (VGP).",
    },
    SdkMethod {
        name: "memory_recall",
        hook: Some("cli-memory-recall"),
        args: "self, query",
        stub_args: "self, query: str",
        payload: r#"{"query": query}"#,
        doc: "Semantic memory recall; '#facet:value' tokens filter the tagged corpus.",
    },
    SdkMethod {
        name: "parallel",
        hook: None,
        args: "",
        stub_args: "self, calls: list, max_workers: int = 10",
        payload: "",
        doc: "Fan-out: run independent read-only queries concurrently — calls is a list of (hook, payload) pairs, results return in the same order, a failed slot becomes {'parallel_error': ...}; pool hard-capped at 10. Typed method names (e.g. 'memory_recall') are accepted as hooks.",
    },
    SdkMethod {
        name: "query",
        hook: None,
        args: "",
        stub_args: "self, hook: str, payload: dict",
        payload: "",        doc: "Escape hatch: any hook in the read-only allowlist (see READONLY_HOOKS); typed method names (e.g. 'memory_recall') are accepted as aliases.",
    },
    SdkMethod {
        name: "search",
        hook: Some("cli-search-docs"),
        args: "self, query",
        stub_args: "self, query: str",
        payload: r#"{"query": query}"#,
        doc: "Search over the docs index.",
    },
    SdkMethod {
        name: "tantivy_search",
        hook: Some("cli-tantivy-search"),
        args: "self, query, top=10",
        stub_args: "self, query: str, top: int = 10",
        payload: r#"{"query": query, "top": top}"#,
        doc: "BM25 ranked search over the symbol/docs index.",
    },
    SdkMethod {
        name: "wiring_impact",
        hook: Some("cli-wiring-impact"),
        args: "self, symbol, depth=2",
        stub_args: "self, symbol: str, depth: int = 2",
        payload: r#"{"symbol": symbol, "depth": depth, "format": "json"}"#,
        doc: "Transitive consumers of a symbol (blast radius, BFS to `depth`).",
    },
    SdkMethod {
        name: "wiring_orphans",
        hook: Some("cli-wiring-orphans"),
        args: "self",
        stub_args: "self",
        payload: r#"{}"#,
        doc: "Public symbols with no consumer (REGRA #0).",
    },
    SdkMethod {
        name: "wiring_status",
        hook: Some("cli-wiring-status"),
        args: "self",
        stub_args: "self",
        payload: r#"{}"#,
        doc: "Workspace wiring summary (orphans, module scores).",
    },
];

/// S4 — the client body, with `{allowlist}` and `{methods}` filled in by
/// [`py_sdk`]. Everything the model reads about the surface comes from
/// [`READONLY_HOOKS`] and [`SDK_METHODS`]; nothing here restates it.
const TOURING_PY_SDK_TEMPLATE: &str = r#"# --- touring orchestration SDK (injected by `touring run --orchestrate`) ---
# Why this pays: each call below is a daemon read that would otherwise cost a
# full model round-trip. Inside one `touring run` they are ordinary function
# calls, so a program that consults N facts spends ONE turn instead of N, and
# only what it PRINTS enters the context — intermediates stay in the sandbox
# (Anthropic CodeAct / programmatic tool calling; Cloudflare Code Mode).
#   * 3 lookups (find + blast + impact) = 1 turn, not 3.
#   * A loop over 40 files = 1 turn; the same sweep as tool calls is 40.
#   * `--brief` returns the digest and DECLARES what it elided (`elided_lines`).
#   * {n_hooks} read hooks are reachable — the 9 typed shortcuts below are
#     conveniences; `touring.query(hook, payload)` reaches all of them.
import socket as _tr_socket, os as _tr_os, json as _tr_json, threading as _tr_threading


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
        # `parallel` calls `query` from worker threads; the origin counter must
        # never mint the same sequence twice or d4 under-counts sub-calls.
        self._mutex = _tr_threading.Lock()

    # W2 d1/S-2.1 — the read-only hook allowlist this SDK speaks, GENERATED from
    # `READONLY_HOOKS` (S4). Containment, not a security boundary (the sandbox
    # reaches the socket regardless; the server-side grant/proxy is the tracked
    # follow-up): the guard teaches the contract up front instead of failing
    # opaquely at the daemon.
    READONLY_HOOKS = (
{allowlist}
    )

    # M0 (29/08/2026) — typed method names accepted as hook aliases in
    # query/parallel, GENERATED from `SDK_HOOK_ALIASES` — the same table the
    # daemon resolves, so client convenience and enforcement cannot drift.
    HOOK_ALIASES = {
{aliases_py}
    }

    def query(self, hook, payload=None, _par=False):
        """{query_doc}"""
        hook = self.HOOK_ALIASES.get(hook, hook)
        if hook not in self.READONLY_HOOKS:
            raise RuntimeError(
                "hook " + repr(hook) + " is not in the orchestrate read-only "
                "allowlist; available: " + ", ".join(self.READONLY_HOOKS))
        s = _tr_socket.socket(_tr_socket.AF_UNIX, _tr_socket.SOCK_STREAM)
        s.settimeout(30)
        try:
            s.connect(self._sock)
            with self._mutex:
                self._n += 1
                seq = self._n
            body = {"hook": hook, "payload": payload or {}, "project_root": self._root}
            if self._run_id:
                # M0 — a `:par` suffix marks sub-calls issued through
                # `parallel`, so run_subcalls.jsonl can measure fan-out
                # adoption; the run_id prefix (the correlation key) is intact.
                body["origin"] = self._run_id + ":code:" + str(seq) + (":par" if _par else "")
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
            # A razão do daemon VIAJA. Sem isto o programa via só
            # "success=false" e o remédio — que o servidor escreveu — morria no
            # caminho; um deny que não ensina é obstáculo, não gate.
            motivo = ""
            try:
                motivo = (_tr_json.loads(resp.get("output") or "{}") or {}).get("error") or ""
            except _tr_json.JSONDecodeError:
                motivo = resp.get("output") or ""
            raise RuntimeError(
                "touring daemon refused hook " + repr(hook)
                + (": " + motivo if motivo else ""))
        out = resp.get("output", "")
        if not out:
            return None
        try:
            return _tr_json.loads(out)
        except _tr_json.JSONDecodeError:
            return out

    def parallel(self, calls, max_workers=10):
        """{parallel_doc}"""
        from concurrent.futures import ThreadPoolExecutor
        def one(call):
            hook, payload = call[0], (call[1] if len(call) > 1 else None)
            try:
                return self.query(hook, payload, _par=True)
            except Exception as e:
                # keep the batch: one failed slot must not void the other N-1
                return {"parallel_error": str(e), "hook": hook}
        with ThreadPoolExecutor(max_workers=max(1, min(int(max_workers), 10))) as pool:
            return list(pool.map(one, calls))

    # F4 S4 (2026-09-01) — append-only mirror sink for signal_use counts.
    # The daemon reads `~/.claude/touring/sdk_signal_mirror.jsonl` and
    # materializes per-hook call_count / failure_count / p50 / p99 into
    # the SignalReport that drives `touring.kpi.code_mode.signal_use.composite`
    # (BestPracticesGate, F5). Failures here NEVER abort the program — the
    # mirror is observability, not correctness.
    def record_hook_call(self, hook, duration_ms, success=True):
        """Append one hook-call entry to the post-tool-use mirror.

        `hook` is the canonical snake_case name (e.g. "ast_meta"); `duration_ms`
        is the wall-clock time the hook took; `success` defaults to True.
        No-ops silently if `TOURING_SDK_SIGNAL_MIRROR` is unset.
        """
        import os as _tr_os2, json as _tr_json2, pathlib as _tr_pathlib
        mirror = _tr_os2.environ.get("TOURING_SDK_SIGNAL_MIRROR")
        if not mirror:
            return None
        line = _tr_json2.dumps({
            "ts": int(_tr_os2.environ.get("TOURING_RUN_TS") or _tr_json2.dumps(_tr_json2.loads("null")) and __import__("time").time()),
            "hook_name": hook,
            "duration_ms": int(duration_ms),
            "success": bool(success),
        })
        try = None
        try:
            _tr_pathlib.Path(mirror).parent.mkdir(parents=True, exist_ok=True)
            with open(mirror, "a", encoding="utf-8") as _tr_f:
                _tr_f.write(line + "\n")
        except Exception as _tr_e:
            # Fail-soft: never abort the program over a mirror write.
            return None
        return None

    # F4 S4 — wrap every query in a timed mirror-write so the daemon sees
    # REAL signal_use counts (not the zeros that the strategy-doc listed
    # before F3/F4). Cheap: 1 append per call, no contention.
    _orig_query = query
    def query(self, hook, payload=None, _par=False):
        """{query_doc} — wrapped for F4 signal_use telemetry."""
        _tr_t0 = __import__("time").time()
        _tr_ok = True
        try:
            return self._orig_query(hook, payload, _par=_par)
        except Exception:
            _tr_ok = False
            raise
        finally:
            try:
                self.record_hook_call(
                    self.HOOK_ALIASES.get(hook, hook),
                    int((__import__("time").time() - _tr_t0) * 1000),
                    _tr_ok,
                )
            except Exception:
                pass

{methods}
touring = _TouringClient()
# --- end touring SDK ---
"#;

/// S4 — the stub header; the Protocol methods are appended by [`py_sdk_stub`].
const TOURING_PY_SDK_STUB_HEADER: &str = r#"# touring run --orchestrate SDK — STATIC STUB (auto-generated, byte-stable)
# Exactly two of the names declared below are bound at runtime: `touring` and
# the RuntimeError raised on a non-allowlisted hook. Everything else is a
# STATIC STUB for reading: build arguments as plain dict/list — never
# `IndexFindArgs(symbol_name=...)`, which raises NameError at run time.
# `query(hook, payload)` reaches all {n_hooks} allowlisted read hooks; the typed
# methods below are the shortcuts for the ones used most.
from typing import Any, Protocol


class _Touring(Protocol):
"#;

/// The allowlist rendered 4-per-line, 8-space indent, trailing comma — a body
/// valid inside BOTH a Python tuple and a JS array, so the two SDKs share it.
fn allowlist_body() -> String {
    READONLY_HOOKS
        .chunks(4)
        .map(|linha| {
            let itens: Vec<String> = linha.iter().map(|h| format!("\"{h}\"")).collect();
            format!("        {},", itens.join(", "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// M0 — the alias map rendered for the Python SDK (`"name": "hook",` lines),
/// from the SAME table the daemon resolves (`SDK_HOOK_ALIASES`).
fn aliases_body_py() -> String {
    touring_foundation::orchestrate_allowlist::SDK_HOOK_ALIASES
        .iter()
        .map(|(n, h)| format!("        \"{n}\": \"{h}\","))
        .collect::<Vec<_>>()
        .join("\n")
}

/// M0 — the same alias map as a JS object body (names are identifiers).
fn aliases_body_js() -> String {
    touring_foundation::orchestrate_allowlist::SDK_HOOK_ALIASES
        .iter()
        .map(|(n, h)| format!("            {n}: \"{h}\","))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The typed shortcuts rendered as Python methods (skipping `query`, which the
/// template hand-writes because it is the transport itself).
fn methods_py() -> String {
    SDK_METHODS
        .iter()
        .filter_map(|m| {
            let hook = m.hook?;
            Some(format!(
                "    def {}({}):\n        \"\"\"{}\"\"\"\n        return self.query(\"{}\", {})\n",
                m.name, m.args, m.doc, hook, m.payload
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The doc of one hand-written method (`query`, `parallel`), read from the
/// SAME table the stub renders — duas descrições do mesmo método é como a
/// divergência começa.
fn doc_of(name: &str) -> &'static str {
    SDK_METHODS
        .iter()
        .find(|m| m.name == name)
        .map_or("Send a daemon RPC.", |m| m.doc)
}

/// R4 — the orchestrate SDK, generated once from [`READONLY_HOOKS`] and
/// [`SDK_METHODS`]. `OnceLock` so the cost is paid once and the bytes are
/// identical for the rest of the process (provider KV-cache stability, P21).
fn py_sdk() -> &'static str {
    static S: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        TOURING_PY_SDK_TEMPLATE
            .replace("{allowlist}", &allowlist_body())
            .replace("{aliases_py}", &aliases_body_py())
            .replace("{methods}", &methods_py())
            .replace("{n_hooks}", &READONLY_HOOKS.len().to_string())
            .replace("{query_doc}", doc_of("query"))
            .replace("{parallel_doc}", doc_of("parallel"))
    })
}

/// SDK-1 — the same orchestrate SDK for the JS runtimes (node/bun/deno), with
/// `{allowlist}`, `{methods}`, `{n_hooks}` and the two hand-written docs filled
/// by [`js_sdk`]. Promise-based because `net` is async; `node:net` via dynamic
/// import works under node -e (CJS), bun -e, and deno eval (ESM) alike.
const TOURING_JS_SDK_TEMPLATE: &str = r#"// --- touring orchestration SDK (injected by `touring run --orchestrate`) ---
// Same contract as the Python SDK: {n_hooks} read hooks are reachable through
// `touring.query(hook, payload)`; the typed shortcuts below are conveniences.
// Every method returns a Promise — await it inside `(async () => { ... })()`.
class _TouringClient {
    constructor() {
        this._sock = process.env.TOURING_DAEMON_SOCKET || process.env.TOURING_DAEMON_SOCK
            || "/tmp/touring-daemon-" + (process.getuid ? process.getuid() : 1000) + ".sock";
        this._root = process.env.TOURING_PROJECT_ROOT || process.cwd();
        this._runId = process.env.TOURING_RUN_ID || "";
        this._n = 0;
        this.READONLY_HOOKS = [
{allowlist}
        ];
        // M0 — typed method names accepted as hook aliases (same table the
        // daemon resolves; generated, so convenience and enforcement agree).
        this.HOOK_ALIASES = {
{aliases_js}
        };
    }

    /** {query_doc} */
    async query(hook, payload, _par) {
        hook = this.HOOK_ALIASES[hook] || hook;
        if (!this.READONLY_HOOKS.includes(hook)) {
            throw new Error("hook " + JSON.stringify(hook) + " is not in the orchestrate "
                + "read-only allowlist; available: " + this.READONLY_HOOKS.join(", "));
        }
        this._n += 1;
        const body = { hook: hook, payload: payload || {}, project_root: this._root };
        // M0 — `:par` marks parallel-issued sub-calls (adoption telemetry).
        if (this._runId) { body.origin = this._runId + ":code:" + this._n + (_par ? ":par" : ""); }
        const net = await import("node:net");
        return await new Promise((resolve, reject) => {
            const s = net.createConnection(this._sock);
            let buf = "";
            s.setTimeout(30000, () => { s.destroy(new Error("touring daemon timeout (30s)")); });
            s.on("error", reject);
            s.on("data", (chunk) => { buf += chunk; if (buf.endsWith("\n")) { s.end(); } });
            s.on("close", () => {
                let resp;
                try { resp = JSON.parse(buf); } catch (e) { reject(e); return; }
                if (!resp.success) {
                    // The daemon's reason TRAVELS — a deny that does not teach
                    // is an obstacle, not a gate (same rule as the Python SDK).
                    let motivo = "";
                    try { motivo = (JSON.parse(resp.output || "{}") || {}).error || ""; }
                    catch (e) { motivo = resp.output || ""; }
                    reject(new Error("touring daemon refused hook " + JSON.stringify(hook)
                        + (motivo ? ": " + motivo : "")));
                    return;
                }
                const out = resp.output || "";
                if (!out) { resolve(null); return; }
                try { resolve(JSON.parse(out)); } catch (e) { resolve(out); }
            });
            s.write(JSON.stringify(body) + "\n");
        });
    }

    /** {parallel_doc} */
    async parallel(calls, max_workers = 10) {
        const bounded = Math.max(1, Math.min(max_workers | 0, 10));
        const results = new Array(calls.length);
        let next = 0;
        const worker = async () => {
            while (next < calls.length) {
                const i = next++;
                const hook = calls[i][0], payload = calls[i][1];
                try { results[i] = await this.query(hook, payload, true); }
                catch (e) {
                    // keep the batch: one failed slot must not void the other N-1
                    results[i] = { parallel_error: String((e && e.message) || e), hook: hook };
                }
            }
        };
        await Promise.all(Array.from({ length: Math.min(bounded, calls.length || 1) }, worker));
        return results;
    }

{methods}
}
const touring = new _TouringClient();
// --- end touring SDK ---
"#;

/// The typed shortcuts as JS methods, from the SAME table as Python's: `self`
/// is stripped from the signature and the payload dict literal is already
/// valid JS (quoted keys, `=` defaults are shared syntax).
fn methods_js() -> String {
    SDK_METHODS
        .iter()
        .filter_map(|m| {
            let hook = m.hook?;
            let args = m
                .args
                .trim_start_matches("self")
                .trim_start_matches(',')
                .trim();
            Some(format!(
                "    /** {} */\n    {}({}) {{\n        return this.query(\"{}\", {});\n    }}\n",
                m.doc, m.name, args, hook, m.payload
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// SDK-1 — the JS orchestrate SDK, generated from the SAME two tables as the
/// Python one so the multi-language surface cannot drift from the enforced
/// one (D8: one source, two renderings).
fn js_sdk() -> &'static str {
    static S: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        TOURING_JS_SDK_TEMPLATE
            .replace("{allowlist}", &allowlist_body())
            .replace("{aliases_js}", &aliases_body_js())
            .replace("{methods}", &methods_js())
            .replace("{n_hooks}", &READONLY_HOOKS.len().to_string())
            .replace("{query_doc}", doc_of("query"))
            .replace("{parallel_doc}", doc_of("parallel"))
    })
}

/// W2 d1/S-2.3 — the STATIC STUB (dsh `py-types` blueprint): a Protocol with
/// the docstring INSIDE each method plus the mandatory static-stub warning,
/// lexicographically ordered so the text is byte-identical across runs.
/// Printed by `touring run --sdk-stub`; decision record: P23 (2026-08-23) —
/// the full stub is on-demand; nudges carry the 1-line form.
///
/// S4 made it GENERATED: it and the SDK now read the same two tables, so the
/// advertised surface cannot drift from the enforced one.
fn py_sdk_stub() -> &'static str {
    static S: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        let mut ordenados: Vec<&SdkMethod> = SDK_METHODS.iter().collect();
        ordenados.sort_by_key(|m| m.name);
        let corpo = ordenados
            .iter()
            .map(|m| {
                format!(
                    "    def {}({}) -> Any:\n        \"\"\"{}\"\"\"\n        ...\n",
                    m.name, m.stub_args, m.doc
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "{}{corpo}\n\ntouring: _Touring\n",
            TOURING_PY_SDK_STUB_HEADER.replace("{n_hooks}", &READONLY_HOOKS.len().to_string())
        )
    })
}


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

    /// Wall-clock timeout in milliseconds (engine default 30000, max 600000)
    #[arg(long)]
    timeout_ms: Option<u64>,

    /// Permit forbidden primitives (fs.write*, subprocess, eval); off by default
    #[arg(long)]
    allow_forbidden: bool,

    /// Compact output: C5 summary digest (< ~200 tokens; preserves exit_code + error lines)
    #[arg(long)]
    brief: bool,

    /// Inject the read-only `touring` orchestration SDK (Python sync, or the Promise mirror
    /// for js/node/bun/ts) so the script can query the daemon over its socket in one
    /// execution — code-mode orchestration WITHOUT MCP (R4/SDK-1)
    #[arg(long)]
    orchestrate: bool,

    /// Busy-time (CPU) budget of the child in ms (engine default 60000) —
    /// QW-3, cross-audit 27/08
    #[arg(long)]
    compute_ms: Option<u64>,

    /// Inline stdout cap in bytes (default 8192; the full output always lands
    /// in the spill regardless) — QW-2
    #[arg(long)]
    max_stdout_bytes: Option<usize>,

    /// Inline stderr cap in bytes (default 4096) — QW-2
    #[arg(long)]
    max_stderr_bytes: Option<usize>,

    /// File whose bytes become the program's stdin — QW-4
    #[arg(long)]
    input: Option<String>,

    /// Grant outbound TCP to this port (repeatable; Landlock NetPort — the
    /// kernel enforces). Residual, accepted and documented: the filter is by
    /// PORT, not host — 443 talks to any host on 443. — NET-1
    #[arg(long = "allow-net-port", action = clap::ArgAction::Append)]
    allow_net_port: Vec<u16>,

    /// Mirror the program's output to stderr AS IT ARRIVES (the JSON envelope
    /// keeps stdout) — a long run stops being a black box until the end. The
    /// captured buffer and its caps do not change. — OUT-1
    #[arg(long)]
    stream: bool,

    /// Print the byte-stable typed stub (.pyi) of the orchestrate SDK and exit —
    /// the full contract for reading; nudges carry the 1-line form (P23)
    #[arg(long, conflicts_with_all = ["code", "file", "stdin"])]
    sdk_stub: bool,

    /// W3b — harvest THIS program as a reusable `#kind:snippet` memory under
    /// `<slug>`, and enrol it in the measured trust ladder. The executor does
    /// the persisting, so the library populates from use instead of from the
    /// caller remembering to run a second command.
    #[arg(long, value_name = "SLUG")]
    harvest: Option<String>,
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
        print!("{}", py_sdk_stub());
        return Ok(());
    }

    let user_code = resolve_code(&cli)?;
    let code = maybe_inject_sdk(user_code.clone(), &cli.lang, cli.orchestrate)?;
    // W3b/S-3.4 — a biblioteca de snippets vira bindings `snippet_*` DEPOIS do
    // SDK (um snippet pode usar `touring.*`) e ANTES do corpo do usuário.
    let (snippet_pre, snippet_report) = snippet_preamble(cli.orchestrate, &cli.lang);
    let code = if snippet_pre.is_empty() {
        code
    } else {
        format!("{snippet_pre}\n{code}")
    };

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
    // W8 S-8.4 (plano code-mode-total) — a lacuna que a fonte TanStack nomeia:
    // a tool que executa código arbitrário era a ÚNICA sem gate de aprovação.
    // `--allow-forbidden` eleva a Trusted; a decisão é POR-COMANDO (o padrão
    // GIT_DESTRUCTIVE_OK — cada uso é uma escolha, nunca um estado exportado).
    if cli.allow_forbidden
        && std::env::var("TOURING_TRUSTED_OK").map(|v| v == "1") != Ok(true)
    {
        anyhow::bail!(
            "--allow-forbidden eleva a execução a Trusted (capacidades sem sandbox) e \
             exige decisão por-comando: prefixe TOURING_TRUSTED_OK=1 no PRÓPRIO comando \
             (nunca exporte na sessão). Sem elevação, rode sem --allow-forbidden — o \
             perfil Sandboxed cobre leitura + orquestração."
        );
    }
    // QW-2/3/4 + NET-1 (cross-audit 27-28/08) — tunables por chamada: só valem
    // Some quando o operador passou a flag; `None` preserva o engine default.
    let stdin_bytes = match &cli.input {
        Some(path) => Some(std::fs::read(path).map_err(|e| {
            anyhow::anyhow!("--input {path}: {e}")
        })?),
        None => None,
    };
    let tunables = if cli.compute_ms.is_some()
        || cli.max_stdout_bytes.is_some()
        || cli.max_stderr_bytes.is_some()
        || stdin_bytes.is_some()
        || !cli.allow_net_port.is_empty()
        || cli.stream
    {
        Some(crate::tools::ctx_execute_tools::RunTunables {
            compute_ms: cli.compute_ms,
            max_stdout_bytes: cli.max_stdout_bytes,
            max_stderr_bytes: cli.max_stderr_bytes,
            stdin_bytes,
            allow_net_ports: if cli.allow_net_port.is_empty() {
                None
            } else {
                Some(cli.allow_net_port.clone())
            },
            stream: cli.stream,
        })
    } else {
        None
    };
    let ceg_advisory = gate_run(&cli.lang, &user_code, cli.allow_forbidden, &cli.allow_net_port)?;

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
        tunables,
    ))
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // O executor já chamou `record_code_mode_run`, mas esse counter vive na
    // memória do PROCESSO — e este é o CLI, que morre em seguida. O
    // `gate-metrics` lê do daemon, então sem este relay o canal principal do
    // code mode (`touring run`, o que dispensa MCP) contava ZERO enquanto a
    // rota MCP contava tudo: o KPI de economia media exatamente a via que o
    // programa existe para substituir. Medido em 24/08/2026 — 163 execuções no
    // journal, `code_mode_runs_count = 0`.
    //
    // Espelha o que a C2-W0 faz com as sub-chamadas do `--orchestrate`, que o
    // daemon contabiliza quando chegam pelo socket. Fail-open por construção:
    // o resultado é ignorado, porque perder um counter jamais pode custar a
    // execução do usuário (invariante do CEG).
    let _ = crate::daemon_client::daemon_query(
        "cli-code-mode-run",
        serde_json::json!({ "bytes_elided": out.bytes_elided }),
    );

    // W3 d2/S-3.3 — offer harvest on the USER's code (never the injected SDK).
    let harvest = harvest_hint(
        &user_code,
        &cli.lang,
        out.exit_code,
        cli.harvest.is_some(),
    );
    // W3b — the executor settles the ladder itself: harvesting when asked, and
    // otherwise recognising a re-run of an already-harvested body. Both operate
    // on the USER's code, never the injected SDK (same rule as the hint).
    let trust = settle_snippet_ladder(&user_code, &cli.lang, out.exit_code, cli.harvest.as_deref());
    emit_output(
        &out,
        cli.brief,
        harvest.as_deref(),
        trust.as_deref(),
        &snippet_report,
        ceg_advisory.as_ref(),
    )?;

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
/// S5a (26/08/2026) — o advisory do exec-gate viaja no RESULTADO (campo
/// estruturado), não no stderr: lá ele se misturava ao stderr real do
/// programa sandboxed e corrompia quem parseava o stream (o consumidor CC lê
/// stdout+stderr fundidos). O conteúdo é o mesmo; o canal é o JSON.
#[derive(Debug)]
struct CegRunAdvisory {
    composite: f64,
    reason: String,
}

/// S5a — o JSON do advisory, uma forma só nos dois modos (brief/full).
fn ceg_advisory_json(a: &CegRunAdvisory) -> serde_json::Value {
    serde_json::json!({
        "composite": a.composite,
        "reason": a.reason,
        "note": "subprocess-only X6 deny waived — the sandbox DOES contain the \
                 filesystem (a write outside the workspace fails), which is what \
                 makes this class waivable in every language (extended from \
                 shell-only by Gabriel's order, 30/08/2026). Network and \
                 destructive-pattern denials are NOT waived: they hard-deny.",
    })
}

fn gate_run(lang: &str, code: &str, allow_forbidden: bool, allow_net_ports: &[u16]) -> Result<Option<CegRunAdvisory>> {
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
    match run_gateway(tool, code, None, &deps) {
        Ok(outcome) => match outcome.decision.verdict {
            // SHELL denials used to downgrade WHOLESALE, on the grounds that
            // the sandbox is the real containment. Measured 2026-08-27, that
            // grounding was false for one class: Landlock is filesystem-only,
            // so `curl https://example.com` — X6-denied as `network`, then
            // downgraded — executed and returned HTTP 200, while the identical
            // Python `socket` call was correctly refused. Same capability, same
            // verdict, opposite outcomes, decided by the language.
            //
            // So the downgrade is now SELECTIVE: only `subprocess` noise is
            // waived (and after the builtin fix in `gate.rs` there is far less
            // of it). Any other denied class — network above all — keeps the
            // hard deny that code languages always had.
            // NET-1 (28/08): com `--allow-net-port` o operador concedeu portas
            // TCP explicitamente — um deny cuja ÚNICA classe é `network` vira
            // advisory e o kernel (Landlock NetPort) restringe a essas portas.
            // Residual aceito e documentado: a concessão é por PORTA; 443 fala
            // com qualquer host (Landlock não filtra host).
            Verdict::Deny if !allow_net_ports.is_empty() && only_network_denials(&outcome.decision) => {
                let advisory = CegRunAdvisory {
                    composite: outcome.decision.composite_score,
                    reason: format!(
                        "network deny dispensado: operador concedeu a(s) porta(s) {:?} por \
                         --allow-net-port; o kernel (Landlock NetPort) restringe a elas. \
                         Residual: por PORTA, não por host.",
                        allow_net_ports
                    ),
                };
                tracing::debug!(
                    composite = advisory.composite,
                    ports = ?allow_net_ports,
                    "CEG network deny waived per --allow-net-port (kernel NetPort enforces)"
                );
                Ok(Some(advisory))
            }
            // 30/08/2026, ordem de Gabriel ("estenda o waiver às linguagens de
            // código"): o `is_shell &&` saiu da guarda. A assimetria era
            // puramente lexical — mesmo perfil Sandboxed, mesmo Landlock,
            // mesmo rlimit, e `echo` negado em python enquanto `sed` passava
            // em bash (medido pela analise-a2, 4 sondas). O predicado
            // `only_subprocess_denials` é quem carrega a segurança: rede e
            // X2 destrutivo seguem negando duro em toda linguagem.
            Verdict::Deny if only_subprocess_denials(&outcome.decision) => {
                // S5a — o advisory virou campo do resultado; no stderr ele se
                // misturava ao erro real do programa. debug! guarda o rastro
                // forense sem poluir o canal (default-off).
                let advisory = CegRunAdvisory {
                    composite: outcome.decision.composite_score,
                    reason: outcome
                        .decision
                        .reasons
                        .first()
                        .map(String::as_str)
                        .unwrap_or("hard block fired")
                        .to_string(),
                };
                tracing::debug!(
                    composite = advisory.composite,
                    reason = advisory.reason.as_str(),
                    "CEG advisory deny on shell run; proceeding under the real sandbox"
                );
                Ok(Some(advisory))
            }
            Verdict::Deny => anyhow::bail!(
                // G-B (30/08, campo analise-a2): a razão citada é a da CLASSE
                // que manteve o deny de pé, nunca `reasons.first()` cru — um
                // programa com `sed` + um redirect de escrita imprimia
                // "denied the subprocess capability 'sed'" quando a classe
                // determinante era file-write, e o operador bisecou um `sed`
                // inocente.
                "the CEG gateway denied this run (composite {:.2}): {}.{} Adjust the code, \
                 or rerun with --allow-forbidden for the trusted profile if the operation \
                 is intentionally privileged.",
                outcome.decision.composite_score,
                blocking_reason(&outcome.decision),
                deny_class_hint(&outcome.decision)
            ),
            _ => Ok(None),
        },
        Err(e) => {
            tracing::warn!(error = %e, "CEG gateway errored on the run path; proceeding (fail-open)");
            Ok(None)
        }
    }
}

/// The reason worth showing when a deny STANDS: the first one whose class is
/// not `subprocess`, because that is the class that kept the waiver from
/// firing — `reasons.first()` named an innocent `sed` while the determinant
/// class was `file-write` (G-B, campo analise-a2 30/08). Falls back to the
/// first reason (or the generic line) when nothing more specific exists.
fn blocking_reason(decision: &touring_ceg::gateway::GateDecision) -> &str {
    let has_non_subprocess = decision.denied_classes.iter().any(|c| c != "subprocess");
    if has_non_subprocess
        && let Some(r) = decision
            .reasons
            .iter()
            .find(|r| !r.contains("the subprocess capability"))
    {
        return r;
    }
    decision
        .reasons
        .first()
        .map(String::as_str)
        .unwrap_or("hard block fired")
}

/// G-D (30/08, campo analise-a2): env reads are denied ON PURPOSE under
/// `Sandboxed` — the child carries whitelisted credentials (I-12) and a
/// workspace file write would bypass the stdout redaction — but denying
/// WITHOUT a route left the program unable to self-verify (the probe that
/// confirms the sandbox was the one the sandbox refused). The hint names the
/// probes that answer the same questions without touching env.
fn deny_class_hint(decision: &touring_ceg::gateway::GateDecision) -> &'static str {
    if decision.denied_classes.iter().any(|c| c == "environment") {
        " Env reads are denied under Sandboxed on purpose: the child carries \
         whitelisted credentials, and a workspace file write would bypass \
         stdout redaction. To self-verify the Python environment use \
         sys.executable / sys.path / sys.prefix (they reflect PYTHONPATH \
         without touching env); the sandbox env itself is env_clear plus a \
         fixed allowlist — see docs/code-mode.md, section CEG."
    } else {
        ""
    }
}

/// Whether every capability X6 denied is `subprocess` — the only class the
/// run path waives (every language since 30/08/2026; shell-only before).
///
/// An EMPTY class list means the deny came from somewhere other than X6 (an X2
/// destructive pattern, a composite below threshold), and those were never
/// shell noise: it returns `false`, so the deny stands. Failing to the hard
/// verdict is the right default — a waiver granted by accident is a hole.
fn only_subprocess_denials(decision: &touring_ceg::gateway::GateDecision) -> bool {
    // An X2 destructive pattern is never shell noise, even when a `subprocess`
    // denial rides along with it — `rm -rf` denies for BOTH reasons, and keying
    // the waiver on the capability class alone let the destructive half through
    // on the coattails of the harmless one (observed 2026-08-27).
    !decision.static_blocked
        && !decision.denied_classes.is_empty()
        && decision.denied_classes.iter().all(|c| c == "subprocess")
}

/// NET-1 (28/08) — a rede é a razão do deny, com no máximo o `subprocess` do
/// próprio cliente ao lado (`curl` é X6-negado como `network` E `subprocess` —
/// Run("curl") — porque não é read-only binary). Um X2 static block ou uma
/// classe fora desse par → o deny fica de pé.
fn only_network_denials(decision: &touring_ceg::gateway::GateDecision) -> bool {
    !decision.static_blocked
        && decision.denied_classes.iter().any(|c| c == "network")
        && decision
            .denied_classes
            .iter()
            .all(|c| c == "network" || c == "subprocess")
}

/// S7 (2026-08-27) — abaixo deste número de linhas, `--brief` NÃO resume.
///
/// Medido no próprio comando: uma saída de 3 linhas (6 bytes) virava o digest
/// `{"counts":{},"elided_lines":0,"error_lines":[],"head_tail":[...],...}` —
/// ~140 bytes. A "compressão" custava 23× o original e ainda entregava a
/// informação numa forma que o leitor precisa desembrulhar. Um resumidor que
/// aumenta o texto não está resumindo; está taxando o caso comum, a mesma
/// recusa que orienta o modo `code` (S3).
///
/// 200 linhas é o ponto em que o digest passa a ganhar de forma inequívoca: o
/// `head_tail` do sumarizador guarda ~20 linhas, então abaixo disso ele estaria
/// devolvendo quase tudo de qualquer jeito, empacotado.
const BRIEF_FLOOR_LINES: usize = 200;

/// Whether summarising this output actually pays.
///
/// Uma saída TRUNCADA sempre paga — ali o digest é a única forma de dizer o que
/// ficou de fora, e o `stored_path` do spill é o caminho para a íntegra.
fn brief_pays_off(stdout: &str, truncated: bool) -> bool {
    truncated || stdout.lines().count() >= BRIEF_FLOOR_LINES
}

/// Render the sandbox result to stdout: a C5 summary digest under `--brief`, otherwise
/// the full JSON payload (mirrors the MCP adapter's field set in `tools_ctx_execute.rs`).
fn emit_output(
    out: &CtxExecuteOutput,
    brief: bool,
    harvest: Option<&str>,
    snippet_trust: Option<&str>,
    snippet_report: &serde_json::Value,
    ceg_advisory: Option<&CegRunAdvisory>,
) -> Result<()> {
    if brief && brief_pays_off(&out.stdout, out.stdout_truncated) {
        let summary = touring_ceg::gateway::summarize_output(
            &out.stdout,
            out.exit_code,
            out.stdout_truncated,
        );
        let mut v = serde_json::to_value(&summary)?;
        // S5a — campo aditivo: o shape do summary não muda
        if let Some(a) = ceg_advisory {
            v["ceg_advisory"] = ceg_advisory_json(a);
        }
        println!("{}", serde_json::to_string(&v)?);
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
        // S5a — o advisory do exec-gate como campo estruturado (canal próprio,
        // jamais o stderr que o programa sandboxed também usa)
        if let Some(a) = ceg_advisory {
            payload["ceg_advisory"] = ceg_advisory_json(a);
        }
        // S7 — pediram `--brief` e a saída veio inteira: DIZER isso é o mesmo
        // contrato do `elided_lines` (o digest declara o que omitiu; aqui o
        // payload declara que não omitiu nada e por quê). Silenciar deixaria o
        // chamador achando que 12 linhas é o resumo de um volume maior.
        if brief {
            payload["brief_skipped"] = serde_json::json!({
                "reason": "output below the summariser floor",
                "lines": out.stdout.lines().count(),
                "floor_lines": BRIEF_FLOOR_LINES,
            });
        }
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
        // W3b — the MEASURED standing of a snippet that was just harvested or
        // re-run. The badge is shown TO the model (the TanStack detail worth
        // copying): a snippet that keeps failing visibly loses its ✓.
        if let Some(t) = snippet_trust {
            payload["snippet_trust"] = serde_json::json!(t);
        }
        // W3b/S-3.4 — quais bindings `snippet_*` o programa tinha à disposição,
        // o que o teto cortou, e o erro quando a biblioteca está inconsistente.
        if !snippet_report.is_null() {
            payload["snippet_bindings"] = snippet_report.clone();
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
    }
    Ok(())
}

/// W3b — close the loop the harvest hint could only *ask* for.
///
/// The W3 ladder had two breaks, both measured on 2026-08-24: nothing ever
/// called `record_execution` outside a hand-typed `learning reward`, and the
/// hint told the model to store the snippet under a FREE slug while the
/// bridge only recognised `snippet:`-prefixed keys — so the code-mode path
/// could never reach the ladder even if someone did emit the reward.
///
/// This is the affordance version: the executor persists (`--harvest`) and
/// recognises (by body digest) on its own, so the library populates and
/// grades itself from ordinary use. Returns the trust badge to show the model.
///
/// Fail-open throughout — a snippet-bookkeeping failure must never change the
/// outcome of the program the user actually ran.
fn settle_snippet_ladder(
    user_code: &str,
    lang: &str,
    exit_code: i32,
    harvest_slug: Option<&str>,
) -> Option<String> {
    use touring_intelligence::rl::memory::snippet_stats;

    let sig = snippet_stats::code_sig(user_code);
    let root = std::env::current_dir().ok()?;
    let db_path = touring_foundation::TouringConfig::memory_db_canonical(&root);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = rusqlite::Connection::open(&db_path).ok()?;

    let entry_key = match harvest_slug {
        // Explicit harvest: mint the canonical key and persist the body as a
        // `#kind:snippet` memory through the same daemon hook `touring memory
        // store` uses — one command, no second step to forget.
        Some(slug) => {
            let key = snippet_stats::harvest_key(slug);
            let stored = crate::daemon_client::daemon_query(
                "cli-memory-store",
                serde_json::json!({
                    "key": key,
                    "value": user_code,
                    "tier": "semantic",
                    "entry_type": "snippet",
                    "reward": null,
                    "tags": [
                        "#kind:snippet",
                        format!("#lang:{lang}"),
                        "#process:code-mode",
                    ],
                }),
            );
            if stored.is_err() {
                // The memory did not persist; enrolling it in the ladder would
                // leave a graded key pointing at nothing.
                return None;
            }
            key
        }
        // No explicit harvest: is this body one we already know?
        None => snippet_stats::by_sig(&conn, &sig).ok().flatten()?,
    };

    let trust = snippet_stats::record_execution(&conn, &entry_key, exit_code == 0, &sig).ok()?;
    Some(format!("{} {}", trust.badge(), trust.as_str()))
}

/// W3 d2/S-3.3 — offer to harvest a successful, GENERALIZABLE program as a
/// `#kind:snippet` memory. Threshold calibrated on the real corpus of this
/// session's 23 executed programs (decision P33, 2026-08-23): the naive
/// "≥5 lines OR has def" offered for 77% of one-off programs; requiring
/// parametrization AND no ephemeral identifiers lands at ~26% with the
/// genuine candidates captured. NEVER automatic — an offer the caller runs.
fn harvest_hint(code: &str, lang: &str, exit_code: i32, already_harvested: bool) -> Option<String> {
    if exit_code != 0 || already_harvested {
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
    // W3b — the offer teaches the form that ENTERS the measured ladder. The
    // earlier wording ("touring memory store <slug> …") minted a free slug the
    // trust bridge did not recognise, so a snippet harvested this way could
    // never be graded — the hint taught a dead end. Re-running `--harvest`
    // both persists the memory and enrols it, in one command.
    Some(format!(
        "reusable pattern detected ({lines} lines, parametrized): re-run it with \
         --harvest <slug> to persist it as a #kind:snippet memory AND enrol it in \
         the measured trust ladder (the executor tags #lang:{lang} itself, and every \
         later run of the same body updates its badge) — recall via \
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

/// R4/SDK-1 — prepend the `touring` orchestration SDK when `--orchestrate` is
/// set, so the script can call `touring.search(...)`, `touring.index_find(...)`, …
/// against the daemon in a single execution. Python gets the sync client
/// ([`py_sdk`]); the JS runtimes get the Promise-based mirror ([`js_sdk`]),
/// both rendered from the same tables. Other languages get a clear error
/// rather than a silent no-op.
fn maybe_inject_sdk(code: String, lang: &str, orchestrate: bool) -> Result<String> {
    if !orchestrate {
        return Ok(code);
    }
    match lang.trim().to_ascii_lowercase().as_str() {
        "python" | "py" => Ok(format!("{}\n{code}", py_sdk())),
        "js" | "node" | "bun" | "ts" => Ok(format!("{}\n{code}", js_sdk())),
        _ => anyhow::bail!(
            "--orchestrate supports --lang python (sync SDK) and js/node/bun/ts \
             (Promise SDK); got {lang:?}"
        ),
    }
}

/// W3b/S-3.4 — o preâmbulo de bindings `snippet_*` para este run.
///
/// Devolve `(preâmbulo, relatório)`. O relatório é o que aparece no payload:
/// quais bindings ficaram disponíveis, o que o teto cortou e — quando a
/// biblioteca está inconsistente (ciclo, colisão, credencial embutida) — o erro
/// que ensina a consertá-la.
///
/// **Fail-open por desenho**: uma biblioteca inconsistente NÃO aborta o run.
/// O programa do usuário pode nem usar snippets, e derrubá-lo por causa de um
/// snippet alheio seria o gate bricando a sessão (a invariante do CEG). Sem
/// bindings, um programa que dependia deles falha com `NameError` — e o
/// `snippet_bindings_error` no payload diz exatamente por quê.
fn snippet_preamble(orchestrate: bool, lang: &str) -> (String, serde_json::Value) {
    use touring_intelligence::rl::memory::{snippet_bindings, snippet_stats::TrustLevel};

    let canon = lang.trim().to_ascii_lowercase();
    if !orchestrate || (canon != "python" && canon != "py") {
        return (String::new(), serde_json::Value::Null);
    }
    let Ok(root) = std::env::current_dir() else {
        return (String::new(), serde_json::Value::Null);
    };
    let db_path = touring_foundation::TouringConfig::memory_db_canonical(&root);
    let Ok(conn) = rusqlite::Connection::open(&db_path) else {
        return (String::new(), serde_json::Value::Null);
    };
    // Doutrina do módulo de trust: só `>= Provisional` é oferecido sozinho.
    let eligible = match snippet_bindings::load_eligible(&conn, TrustLevel::Provisional) {
        Ok(v) if !v.is_empty() => v,
        _ => return (String::new(), serde_json::Value::Null),
    };
    match snippet_bindings::render_preamble(&eligible) {
        Ok(r) => {
            let mut report = serde_json::json!({ "available": r.exposed });
            if !r.omitted.is_empty() {
                // Corte declarado — nunca silencioso.
                report["omitted_over_cap"] = serde_json::json!(r.omitted);
            }
            (r.preamble, report)
        }
        Err(e) => (
            String::new(),
            serde_json::json!({ "available": [], "error": e.to_string() }),
        ),
    }
}

#[cfg(test)]
mod tests {
    // ── W2 d1 — SDK stub + allowlist contract ─────────────────────────────

    // ── W3 d2/S-3.3 — harvest threshold (calibrated in P33) ───────────────

    #[test]
    fn successful_run_emits_harvest_hint_with_real_values() {
        let code = "import sys\ndef clean(path):\n    return path.strip()\nfor a in sys.argv[1:]:\n    print(clean(a))";
        let hint = super::harvest_hint(code, "python", 0, false)
            .expect("parametrized multi-line code is offerable");
        assert!(hint.contains("#kind:snippet"), "offer carries the facet tag");
        assert!(hint.contains("#lang:python"), "offer carries the real lang");
        // W3b — the offer must teach the form that ENTERS the ladder. Teaching
        // a bare `memory store <slug>` minted a key the trust bridge could not
        // recognise: a hint that leads nowhere is worse than no hint.
        assert!(
            hint.contains("--harvest"),
            "offer must teach the executor-side harvest, not a dead-end slug"
        );
        assert!(
            !hint.contains("memory store"),
            "the superseded wording must not survive alongside the new one"
        );
    }

    #[test]
    fn snippet_bindings_are_scoped_to_orchestrated_python() {
        // O preâmbulo É Python e injeta funções no programa: um run comum não
        // deve ganhar bindings que não pediu, e uma linguagem não-Python não
        // pode receber corpo Python (SyntaxError na primeira linha).
        let (pre, report) = super::snippet_preamble(false, "python");
        assert!(pre.is_empty() && report.is_null(), "run sem --orchestrate");

        let (pre, report) = super::snippet_preamble(true, "bash");
        assert!(pre.is_empty() && report.is_null(), "--orchestrate + bash");
    }

    #[test]
    fn an_already_harvested_run_does_not_re_offer_itself() {
        // The executor just persisted and enrolled this body; repeating the
        // offer would ask the caller to do what already happened.
        let code = "import sys\ndef clean(path):\n    return path.strip()\nfor a in sys.argv[1:]:\n    print(clean(a))";
        assert!(super::harvest_hint(code, "python", 0, true).is_none());
        assert!(super::harvest_hint(code, "python", 0, false).is_some());
    }

    #[test]
    fn harvest_flag_carries_the_slug_and_is_off_by_default() {
        // W3b — harvesting mutates the library, so it stays an explicit act;
        // what the executor does WITHOUT the flag is only recognise a body it
        // already knows (no new state from a plain run).
        let cli = RunCli::try_parse_from([
            "run",
            "--lang",
            "python",
            "--code",
            "print(1)",
            "--harvest",
            "scan-crates",
        ])
        .expect("parse");
        assert_eq!(cli.harvest.as_deref(), Some("scan-crates"));

        let plain = RunCli::try_parse_from(["run", "--lang", "python", "--code", "print(1)"])
            .expect("parse");
        assert_eq!(plain.harvest, None);
    }

    #[test]
    fn harvest_hint_skips_one_offs_and_failures() {
        // too short
        assert!(super::harvest_hint("print(1)", "python", 0, false).is_none());
        // not parametrized
        assert!(super::harvest_hint("a=1\nb=2\nc=3\nd=4\nprint(a+b+c+d)", "python", 0, false).is_none());
        // ephemeral identifiers (session path / absolute date)
        assert!(super::harvest_hint(
            "def f(x):\n    return x\nopen('/tmp/claude-1000/x')\nf(1)\nf(2)", "python", 0, false).is_none());
        // failed execution
        assert!(super::harvest_hint("def f(x):\n    return x\nf(1)\nf(2)\nf(3)", "python", 3, false).is_none());
    }

    /// 2026-08-27 — o waiver do shell é SELETIVO.
    ///
    /// Medido antes deste conserto: `curl https://example.com` foi X6-negado
    /// como `network`, rebaixado a advisory pelo waiver cego, e executou
    /// devolvendo HTTP 200 — enquanto o `socket` equivalente em Python era
    /// corretamente recusado. Mesma capability, mesmo veredito, destinos
    /// opostos decididos pela linguagem. O waiver agora cobre só `subprocess`,
    /// a classe cuja contenção o sandbox REALMENTE tem (provado no mesmo dia:
    /// `touch ~/.ssh/x` falha, `touch <workspace>/x` funciona; Landlock é
    /// filesystem-only, e é por isso que rede não podia ser rebaixada).
    #[test]
    fn the_shell_waiver_covers_subprocess_noise_only() {
        let so_subprocess = decisao(vec!["subprocess"], false);
        assert!(
            super::only_subprocess_denials(&so_subprocess),
            "ruído de subprocess é o que o waiver existe para calar"
        );
        for classe in ["network", "fs-write", "env-read"] {
            let d = decisao(vec![classe], false);
            assert!(
                !super::only_subprocess_denials(&d),
                "`{classe}` não é ruído de shell — o deny tem de valer"
            );
            let misto = decisao(vec!["subprocess", classe], false);
            assert!(
                !super::only_subprocess_denials(&misto),
                "`{classe}` junto de subprocess ainda nega — o waiver não pode \
                 pegar carona no ruído"
            );
        }
    }

    /// Um bloqueio do X2 (padrão destrutivo) nunca é ruído de shell, mesmo com
    /// um deny de subprocess ao lado.
    ///
    /// Observado ao vivo em 2026-08-27, antes do campo `static_blocked`:
    /// `rm -rf /tmp/zz` EXECUTOU sob um advisory que dizia "X2 STATIC blocked
    /// the code". O `rm` nega por dois motivos ao mesmo tempo, e um waiver
    /// chaveado só na classe da capability deixava a metade destrutiva passar
    /// na carona da metade inofensiva.
    #[test]
    fn a_destructive_pattern_is_never_waived() {
        assert!(
            !super::only_subprocess_denials(&decisao(vec!["subprocess"], true)),
            "X2 block + subprocess: o deny vale"
        );
        assert!(
            !super::only_subprocess_denials(&decisao(vec![], true)),
            "X2 block sozinho: o deny vale"
        );
    }

    /// Lista vazia = o deny não veio do X6 (composite abaixo do limiar, por
    /// exemplo). Isso nunca foi ruído de shell: falhar para o veredito duro é
    /// o default correto — um waiver concedido por acidente é um buraco.
    #[test]
    fn an_empty_class_list_does_not_grant_a_waiver() {
        assert!(!super::only_subprocess_denials(&decisao(vec![], false)));
    }

    /// G-B (30/08, campo analise-a2): quando o deny FICA de pé, a razão
    /// citada é a da classe que o manteve — `reasons.first()` nomeava um
    /// `sed` inocente ("subprocess capability 'sed'") quando a classe
    /// determinante era `file-write`, e o operador bisecou o comando errado.
    #[test]
    fn the_blocking_reason_names_the_class_that_kept_the_deny_standing() {
        let mut d = decisao(vec!["subprocess", "fs-write"], false);
        d.reasons = vec![
            "X6 denied the subprocess capability 'sed' under profile 'Sandboxed'".into(),
            "X6 denied the file-write capability 'redirection' under profile 'Sandboxed'"
                .into(),
        ];
        assert!(
            super::blocking_reason(&d).contains("file-write"),
            "a classe que quebrou o waiver é a que se nomeia: {}",
            super::blocking_reason(&d)
        );
        // Controle: deny só-subprocess (ex.: sob X2 static) cita a primeira.
        let mut so_sub = decisao(vec!["subprocess"], true);
        so_sub.reasons =
            vec!["X6 denied the subprocess capability 'rm' under profile 'Sandboxed'".into()];
        assert!(super::blocking_reason(&so_sub).contains("'rm'"));
    }

    /// G-D (30/08, campo analise-a2): o deny de env-read fica de pé (é
    /// intencional), mas a mensagem ENSINA a rota de auto-verificação — a
    /// sonda que confirma o sandbox não pode ser a que o sandbox recusa sem
    /// caminho (A5).
    #[test]
    fn the_env_read_deny_teaches_the_selfcheck_route() {
        let d = decisao(vec!["environment"], false);
        assert!(
            super::deny_class_hint(&d).contains("sys.executable"),
            "o hint nomeia a sonda que não toca env"
        );
        // Controle: classes sem hint dedicado não ganham texto espúrio.
        assert_eq!(super::deny_class_hint(&decisao(vec!["network"], false)), "");
        // O hint compõe com classes mistas: environment presente basta.
        assert!(
            super::deny_class_hint(&decisao(vec!["subprocess", "environment"], false))
                .contains("sys.executable")
        );
        // O ARM não é unit-testável para esta classe — o veredito de
        // `environment` depende do composite (histórico neutro do stub →
        // Allow; produção com histórico vivo → Deny, medido pela analise-a2
        // na 30.4.26). A prova do arm é comportamental, pós-propagação.
        // Mas a DISCREPÂNCIA em si é contrato (método da peer, 30/08): o
        // stub devolvendo Allow é ASSERIDO — no dia em que ele passar a
        // negar, este teste falha como SINAL (o arm talvez tenha virado
        // determinístico e o teste do arm pode voltar), em vez de o
        // comportamento mudar em silêncio.
        let stub_verdict = super::gate_run(
            "python",
            "import os\nprint(os.environ.get(\"PYTHONPATH\"))\n",
            false,
            &[],
        );
        assert!(
            matches!(stub_verdict, Ok(None)),
            "o stub com histórico neutro permite env-read (a produção nega); \
             se isto falhou, o arm virou determinístico — reavalie o teste \
             do arm: {stub_verdict:?}"
        );
    }

    /// 30/08/2026, ordem de Gabriel: o waiver subprocess-only vale em TODA
    /// linguagem — a assimetria anterior era puramente lexical
    /// (`is_shell = matches!(tool, "Bash")`): mesmo perfil, mesmo Landlock,
    /// `echo` negado em python enquanto `sed` passava em bash. O ARM inteiro
    /// é exercitado (não só a função pura): python com subprocess vira
    /// advisory; python com rede segue Err (o controle negativo que impede
    /// esta extensão de reabrir o furo do waiver cego de 27/08).
    #[test]
    fn the_waiver_covers_code_languages_and_network_still_hard_denies() {
        let advisory = super::gate_run(
            "python",
            "import subprocess\nsubprocess.run([\"echo\", \"oi\"])\n",
            false,
            &[],
        )
        .expect("subprocess-only em python é advisory, nunca erro");
        assert!(
            advisory.is_some(),
            "o deny rebaixado viaja como advisory no resultado"
        );
        let rede = super::gate_run(
            "python",
            "import socket\nsocket.create_connection((\"1.2.3.4\", 443))\n",
            false,
            &[],
        );
        assert!(rede.is_err(), "rede segue negando duro em toda linguagem");
    }

    /// Monta uma `GateDecision` mínima para exercitar o predicado do waiver.
    fn decisao(classes: Vec<&str>, static_blocked: bool) -> touring_ceg::gateway::GateDecision {
        let mut d = touring_ceg::gateway::GateDecision::from_evidence(
            &touring_ceg::gateway::Evidence::default(),
        );
        d.denied_classes = classes.into_iter().map(str::to_owned).collect();
        d.static_blocked = static_blocked;
        d
    }

    /// S7 — o piso do `--brief`.
    #[test]
    fn brief_does_not_summarise_below_the_floor() {
        let curta = "a\nb\nc\n";
        assert!(
            !super::brief_pays_off(curta, false),
            "3 linhas viram um digest MAIOR que a saída — resumir aqui é taxar"
        );
        let longa = "linha\n".repeat(super::BRIEF_FLOOR_LINES);
        assert!(
            super::brief_pays_off(&longa, false),
            "no piso o digest passa a ganhar"
        );
    }

    /// Truncada sempre paga: ali o digest é a ÚNICA forma de dizer o que ficou
    /// de fora, e o `stored_path` do spill é o caminho para a íntegra.
    #[test]
    fn a_truncated_output_always_pays_off() {
        assert!(
            super::brief_pays_off("uma linha só\n", true),
            "truncada resume mesmo curta — senão o corte fica invisível"
        );
    }

    /// A borda é inclusiva, e um a menos não resume — o piso é um limiar, não
    /// uma faixa cinzenta.
    #[test]
    fn the_floor_is_an_inclusive_threshold() {
        let no_piso = "x\n".repeat(super::BRIEF_FLOOR_LINES);
        let um_a_menos = "x\n".repeat(super::BRIEF_FLOOR_LINES - 1);
        assert!(super::brief_pays_off(&no_piso, false));
        assert!(!super::brief_pays_off(&um_a_menos, false));
    }

    #[test]
    fn sdk_stub_is_byte_identical_across_runs() {
        // Agora o stub é GERADO, então identidade entre duas chamadas é uma
        // afirmação real (antes a const era trivialmente igual a si mesma).
        assert_eq!(super::py_sdk_stub(), super::py_sdk_stub());
        let methods: Vec<&str> = super::py_sdk_stub()
            .lines()
            .filter_map(|l| l.trim().strip_prefix("def "))
            .collect();
        let mut sorted = methods.clone();
        sorted.sort_unstable();
        assert_eq!(methods, sorted, "stub methods must be lexicographically ordered");
        assert!(!methods.is_empty(), "um stub vazio anunciaria superfície nenhuma");
    }

    /// S4 — o invariante central: a superfície ANUNCIADA e a IMPOSTA saem da
    /// mesma tabela. Antes eram duas consts escritas à mão, o D8 em duas vozes.
    #[test]
    fn stub_and_sdk_declare_exactly_the_same_methods() {
        let stub = super::py_sdk_stub();
        let sdk = super::py_sdk();
        for m in super::SDK_METHODS {
            assert!(
                stub.contains(&format!("def {}(", m.name)),
                "stub deve declarar {}",
                m.name
            );
            assert!(
                sdk.contains(&format!("def {}(", m.name)),
                "SDK deve implementar {}",
                m.name
            );
            assert!(
                stub.contains(m.doc) && sdk.contains(m.doc),
                "a docstring de {} é a MESMA nos dois — texto duplicado diverge",
                m.name
            );
        }
        // e nada além: um método no stub sem entrada na tabela seria superfície
        // anunciada que ninguém implementa.
        let no_stub: Vec<&str> = stub
            .lines()
            .filter_map(|l| l.trim().strip_prefix("def "))
            .filter_map(|l| l.split('(').next())
            .collect();
        for nome in no_stub {
            assert!(
                super::SDK_METHODS.iter().any(|m| m.name == nome),
                "stub declara `{nome}`, que não está em SDK_METHODS"
            );
        }
    }

    /// A7 — `parallel` é hand-written (como `query`): o pool tem teto 10 no
    /// PRÓPRIO corpo executável, não só na docstring (dsh maxParallelSubCalls).
    #[test]
    fn parallel_pool_cap_lives_in_the_client_body() {
        let sdk = super::py_sdk();
        assert!(sdk.contains("def parallel(self, calls, max_workers=10):"));
        assert!(
            sdk.contains("min(int(max_workers), 10)"),
            "o teto 10 deve estar no código que executa, não apenas anunciado"
        );
        let js = super::js_sdk();
        assert!(js.contains("async parallel(calls, max_workers = 10)"));
        assert!(
            js.contains("Math.min(max_workers | 0, 10)"),
            "o mesmo teto vale no SDK JS"
        );
    }

    /// M0 (29/08/2026) — cross-guard: todo método tipado com hook tem alias
    /// fiel em `SDK_HOOK_ALIASES` (foundation), e nenhum alias é órfão de
    /// método. É o que impede o nome que o stub ENSINA de divergir do nome
    /// que o daemon RESOLVE (D8: uma tabela, dois executores).
    #[test]
    fn every_typed_method_is_a_faithful_alias_in_foundation() {
        use touring_foundation::orchestrate_allowlist::SDK_HOOK_ALIASES;
        for m in super::SDK_METHODS {
            let Some(hook) = m.hook else { continue };
            let alvo = SDK_HOOK_ALIASES
                .iter()
                .find(|(n, _)| *n == m.name)
                .map(|(_, h)| *h);
            assert_eq!(
                alvo,
                Some(hook),
                "método tipado `{}` sem alias fiel na foundation",
                m.name
            );
        }
        for (n, _) in SDK_HOOK_ALIASES {
            assert!(
                super::SDK_METHODS.iter().any(|m| m.name == *n),
                "alias `{n}` não corresponde a nenhum método tipado"
            );
        }
    }

    /// M0 — os dois SDKs resolvem aliases no `query` executável e carimbam
    /// `:par` no origin das sub-chamadas emitidas via `parallel` (o sinal de
    /// adoção que o run_subcalls.jsonl passa a registrar).
    #[test]
    fn both_sdks_resolve_aliases_and_stamp_parallel_origin() {
        let py = super::py_sdk();
        assert!(py.contains("HOOK_ALIASES"));
        assert!(py.contains("\"memory_recall\": \"cli-memory-recall\","));
        assert!(py.contains("hook = self.HOOK_ALIASES.get(hook, hook)"));
        assert!(py.contains("(\":par\" if _par else \"\")"));
        assert!(py.contains("self.query(hook, payload, _par=True)"));
        let js = super::js_sdk();
        assert!(js.contains("HOOK_ALIASES"));
        assert!(js.contains("memory_recall: \"cli-memory-recall\","));
        assert!(js.contains("hook = this.HOOK_ALIASES[hook] || hook;"));
        assert!(js.contains("(_par ? \":par\" : \"\")"));
        assert!(js.contains("await this.query(hook, payload, true)"));
    }

    /// SDK-1 — o SDK JS é renderizado das MESMAS tabelas que o Python: cada
    /// atalho tipado e cada docstring aparecem nos dois, ou a superfície
    /// multi-linguagem divergiu da imposta (D8).
    #[test]
    fn js_sdk_mirrors_the_same_method_table() {
        let js = super::js_sdk();
        for m in super::SDK_METHODS {
            assert!(
                js.contains(&format!(" {}(", m.name)),
                "SDK JS deve declarar {}",
                m.name
            );
            assert!(js.contains(m.doc), "a doc de {} é a MESMA nos dois SDKs", m.name);
        }
        for hook in super::READONLY_HOOKS {
            assert!(js.contains(&format!("\"{hook}\"")), "allowlist JS sem {hook}");
        }
        // byte-stável entre chamadas (P21, KV-cache do provider)
        assert_eq!(super::js_sdk(), super::js_sdk());
    }

    /// Todo atalho tipado chama um hook que a allowlist aceita — senão o método
    /// levantaria o RuntimeError do próprio guard ao ser usado.
    #[test]
    fn every_typed_method_targets_an_allowlisted_hook() {
        for m in super::SDK_METHODS {
            let Some(hook) = m.hook else { continue };
            assert!(
                super::READONLY_HOOKS.contains(&hook),
                "`{}` chama `{hook}`, fora da allowlist",
                m.name
            );
        }
    }

    /// S4 — cada hook da allowlist EXISTE no registry do daemon.
    ///
    /// Sem isto o SDK ensinaria uma rota morta: o programa no sandbox chamaria
    /// um hook que o guard aceita e o daemon não atende, e a falha chegaria
    /// como `success=false` opaco. É o modo de falha
    /// `teste-do-componente-nao-e-teste-do-caminho` aplicado à superfície.
    #[test]
    fn every_allowlisted_hook_exists_in_the_daemon_registry() {
        let registry = touring_hooks::hook_registry::all_daemon_hook_names();
        let ausentes: Vec<&&str> = super::READONLY_HOOKS
            .iter()
            .filter(|h| !registry.contains(h))
            .collect();
        assert!(
            ausentes.is_empty(),
            "allowlist cita hooks que o daemon não registra: {ausentes:?}"
        );
    }

    /// Defesa em profundidade sobre a curadoria: nenhum verbo de MUTAÇÃO entra
    /// na allowlist de leitura.
    ///
    /// Não é o critério de seleção — a curadoria é por propósito, porque um
    /// filtro de verbos deixava passar `cli-gotcha-add`, `cli-jobs-spawn` e
    /// `cli-saga-begin` (ausência de palavra perigosa não é prova de
    /// segurança). É a rede embaixo: se alguém acrescentar um hook mutante à
    /// lista sem pensar, este teste reprova.
    #[test]
    fn no_mutating_verb_in_the_readonly_allowlist() {
        const MUTANTES: &[&str] = &[
            "-add", "-create", "-update", "-delete", "-reset", "-store", "-write",
            "-rebuild", "-ingest", "-spawn", "-drop", "-begin", "-abort", "-commit",
            "-apply", "-purge", "-flush", "-install", "-init", "-sync", "-import",
            "-reindex", "-populate", "-repair", "-rename", "-run", "-claim",
            "-release", "-finalize", "-reload", "-unregister", "-gc", "-warmstart",
            "-drain", "-consumed", "-checkpoint", "-start", "-edit", "-backfill",
        ];
        for h in super::READONLY_HOOKS {
            for m in MUTANTES {
                assert!(!h.ends_with(m), "`{h}` termina em `{m}` — não é leitura");
            }
        }
    }

    /// A allowlist não tem duplicatas e está ordenada — o texto gerado precisa
    /// ser byte-estável, e uma duplicata a faria repetir uma linha.
    #[test]
    fn allowlist_is_sorted_and_unique() {
        let mut ordenada = super::READONLY_HOOKS.to_vec();
        ordenada.sort_unstable();
        assert_eq!(
            super::READONLY_HOOKS.to_vec(),
            ordenada,
            "READONLY_HOOKS deve estar ordenada"
        );
        let n = ordenada.len();
        ordenada.dedup();
        assert_eq!(n, ordenada.len(), "READONLY_HOOKS tem duplicatas");
    }

    /// A superfície de leitura cresceu de verdade — o S4 existe porque 8 hooks
    /// obrigavam o programa a voltar ao modelo para tudo o mais.
    #[test]
    fn allowlist_covers_far_more_than_the_original_eight() {
        assert!(
            super::READONLY_HOOKS.len() >= 60,
            "a allowlist encolheu para {} — o ganho do S4 era a superfície",
            super::READONLY_HOOKS.len()
        );
        for hook in [
            "cli-index-find", "cli-ast-blast", "cli-ast-overview", "cli-wiring-status",
            "cli-search-docs", "cli-memory-recall", "cli-tantivy-search", "cli-wiring-impact",
        ] {
            assert!(
                super::READONLY_HOOKS.contains(&hook),
                "os 8 originais continuam valendo: {hook}"
            );
        }
    }

    /// Os bullets de economia: o SDK injetado precisa DIZER por que compensa —
    /// um contrato que só lista métodos ensina a API e não a decisão.
    #[test]
    fn sdk_states_what_it_saves() {
        let sdk = super::py_sdk();
        assert!(sdk.contains("READONLY_HOOKS"), "o guard client-side deve existir");
        assert!(
            sdk.contains("ONE turn instead of N"),
            "o SDK deve declarar a economia, não só a API"
        );
        assert!(
            sdk.contains(&super::READONLY_HOOKS.len().to_string()),
            "o número de hooks alcançáveis é derivado, nunca escrito à mão"
        );
        // e a allowlist inteira viaja no texto injetado
        for h in super::READONLY_HOOKS {
            assert!(sdk.contains(h), "allowlist gerada deve conter {h}");
        }
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
        assert!(py_sdk().contains("\"cli-index-find\", {\"symbol_name\":"));
        assert!(py_sdk().contains("\"cli-ast-blast\", {\"file_path\":"));
        assert!(py_sdk().contains("\"cli-search-docs\", {\"query\":"));
        assert!(py_sdk().contains("\"cli-wiring-status\", {}"));
    }

    // ── C2-W0 — the gate targets the USER's code, never the injected SDK ────

    /// Pins the reason `run()` passes `user_code` to `gate_run`: the SDK
    /// opens the daemon socket BY DESIGN, so gating the composed program
    /// would X6-deny every `--orchestrate`. A user program that reaches for
    /// sockets itself is still denied.
    #[test]
    fn gate_targets_user_code_because_the_sdk_itself_would_deny() {
        assert!(
            gate_run("python", py_sdk(), false, &[]).is_err(),
            "the SDK's socket use must trip the sandboxed profile — that is \
             why it is exempt from the gate"
        );
        assert!(
            gate_run("python", "print(1)", false, &[]).is_ok(),
            "plain user code passes"
        );
    }

    // ── S5a — o advisory do exec-gate viaja no resultado, não no stderr ────

    /// Um deny de shell POR SUBPROCESSO volta como advisory estruturado —
    /// composite + reason no campo, jamais no stderr que o programa também usa.
    ///
    /// Este teste usava `echo hi` como o caso do advisory. Ele parou de servir
    /// em 27/08: `echo` é builtin e não exige mais grant de subprocesso, então
    /// o run sai LIMPO — que é justamente o conserto (de 10 comandos benignos
    /// medidos, 0 emitem advisory hoje; antes eram 10). O advisory continua
    /// existindo para o spawn REAL, e é ele que o teste passou a exercitar.
    #[test]
    fn shell_subprocess_deny_vira_advisory_estruturado() {
        assert!(
            gate_run("bash", "echo hi", false, &[]).expect("builtin passa").is_none(),
            "`echo` é builtin — advisory aqui seria o ruído que o S6 eliminou"
        );
        let adv = gate_run("bash", "python3 -c 1", false, &[])
            .expect("spawn real segue adiante sob advisory (o FS é contido)")
            .expect("o deny de subprocesso produz advisory estruturado");
        assert!(adv.composite > 0.0, "composite viaja: {}", adv.composite);
        assert!(!adv.reason.is_empty(), "reason viaja");
        let v = ceg_advisory_json(&adv);
        assert_eq!(v["composite"], serde_json::json!(adv.composite));
        assert!(
            v["note"].as_str().expect("note").contains("subprocess-only"),
            "a nota declara que o waiver é seletivo: {}",
            v["note"]
        );
    }

    /// O outro lado do mesmo contrato: uma classe que o sandbox NÃO contém
    /// nega de verdade, mesmo em shell.
    ///
    /// Medido em 27/08 antes do conserto: `curl https://example.com` era
    /// X6-negado como `network`, rebaixado pelo waiver cego, e devolvia
    /// HTTP 200 — enquanto o `socket` equivalente em Python era recusado.
    #[test]
    fn shell_network_deny_is_not_waived() {
        let r = gate_run("bash", "curl https://example.com", false, &[]);        let msg = match r {
            Err(e) => e.to_string(),
            Ok(_) => panic!("rede em shell tem de negar DURO, como no Python"),
        };
        assert!(msg.contains("network"), "o deny nomeia a classe: {msg}");
    }

    /// NET-1 (28/08): com `--allow-net-port` o operador concedeu a porta — o
    /// deny de rede vira advisory e o kernel (Landlock NetPort) restringe a
    /// ela. Sem a flag o deny segue duro (o teste acima).
    #[test]
    fn net1_allow_net_port_dispensa_o_deny_de_rede() {
        let adv = gate_run("bash", "curl https://example.com", false, &[443])
            .expect("com a porta concedida o run segue")
            .expect("e carrega o advisory NET-1");
        assert!(
            adv.reason.contains("--allow-net-port"),
            "o advisory nomeia a concessão: {}",
            adv.reason
        );
        // e um deny MISTO (network + outra classe) não se dispersa
        assert!(
            gate_run("bash", "curl https://x && rm -rf /tmp/zz", false, &[443]).is_err(),
            "network + destrutivo junto continua deny-duro"
        );
    }

    /// E um padrão destrutivo nunca é waived, nem com um deny de subprocesso
    /// ao lado (`rm` gera os dois ao mesmo tempo).
    #[test]
    fn shell_destructive_pattern_is_not_waived() {
        assert!(
            gate_run("bash", "rm -rf /tmp/zz-probe", false, &[]).is_err(),
            "X2 block não pega carona no waiver de subprocesso"
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
            py_sdk().contains("TOURING_RUN_ID"),
            "SDK must read the exported run identity"
        );
        assert!(
            py_sdk().contains("\":code:\" + str(seq)"),
            "each sub-call is numbered <run_id>:code:<n>"
        );
        assert!(
            py_sdk().contains("with self._mutex:"),
            "the sequence is minted under a lock — `parallel` calls `query` \
             from worker threads and a duplicated origin under-counts d4"
        );
        assert!(
            py_sdk().contains("if self._run_id:"),
            "origin is only attached when an identity exists"
        );
    }

    #[test]
    fn sdk_marker_matches_the_daemon_const() {
        // H6 (cross-audit 27/08): o SDK cunha o marker como literal Python e o
        // daemon impõe a const Rust em `is_sandbox_origin` — este teste é o
        // guard de paridade cross-linguagem entre os dois lados do contrato.
        let marker = touring_foundation::orchestrate_allowlist::SANDBOX_ORIGIN_MARKER;
        assert!(
            py_sdk().contains(&format!("\"{marker}\"")),
            "o SDK Python deve cunhar exatamente SANDBOX_ORIGIN_MARKER ({marker})"
        );
    }
}
