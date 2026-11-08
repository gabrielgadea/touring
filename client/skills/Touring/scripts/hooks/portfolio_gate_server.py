#!/usr/bin/env python3
# #tags: kind:script purpose:gate domain:code-mode process:measure lang:python
"""portfolio_gate_server.py — the portfolio gate as a RESIDENT process, served over HTTP (D09, 05/11/2026).

Why: ``script_portfolio_gate.py`` as a ``"type": "command"`` hook is a fresh Python process on every Write/Bash
(~20 ms of interpreter + ~25 ms of imports before any work; measured p99 166-194 ms against the 150 ms contract).
A resident process pays those once. Claude Code POSTs the same JSON it would pipe to the command hook's stdin;
this server answers with the same JSON the command hook would print on stdout.

Contract (one decision, never two copies of the rule):
* ``POST /hook``  body = the hook event JSON; the answer is ``script_portfolio_gate.responder(payload, "http")`` —
  the SAME function ``main()`` of the command transport runs. ``None`` (the hook's exit-0-with-empty-stdout) is
  answered ``{}``. The gate has no deny path (it never exits non-zero and never emits a ``permissionDecision``), so
  there is nothing to translate into ``deny``: the shadow gate stays a shadow over HTTP.
* Invalid body, non-object body, any exception while deciding: 200 ``{}`` (fail-open, like the command hook) — the
  server never dies for a request, and ``/health`` counts them (``invalidos`` / ``erros``).
* ``GET /health`` -> ``{"ok": true, "pid", "porta", "indices": [{indice, mtime_ns, entradas}], ...}``.
* Events go to the same ``events.jsonl`` as the command transport, tagged ``"transporte": "http"``.

What stays in memory, and what keeps it honest:
* the code (hook + ``portfolio/*.py``) is loaded ONCE; the mtime/size of those files is checked on every request and
  a change reloads the whole set (a failed reload keeps the old code and says so on stderr) — a resident process must
  never serve a rule that was already edited;
* the project index rows (``federacao.fonte_projeto``) are memoized per index file by (mtime_ns, size) — the same key
  as the on-disk marshal cache, so an exporter rewriting ``indice.json`` is picked up on the next request;
* the decisions themselves (injection policy, active plan, kill switch, tantivy) are NEVER cached: they run per call.

Evaluations are serialized by one lock: ``recuperar.ULTIMA_ROTA_TANTIVY`` is a module variable that names the route of
"this call", and a concurrent call would read another's. The work is milliseconds; reading the body and writing the
answer stay concurrent.

Security: binds ``127.0.0.1`` only; refuses a ``Host`` that is not loopback (DNS rebinding) and a body that is not
``application/json`` (a browser's no-preflight cross-origin POST); body capped at ``MAX_CORPO``.

Kill switches (a resident process does not inherit the session's environment): ``TOURING_PORTFOLIO_GATE_DISABLED`` in
the SERVER's environment, or — without a restart — the file ``~/.claude/touring/portfolio_gate/DISABLED``.

    python3 portfolio_gate_server.py [--port N]     # N or $PORTFOLIO_GATE_PORT, default 47821; 0 = ephemeral
"""

from __future__ import annotations

import argparse
import http.server
import importlib
import importlib.util
import json
import os
import signal
import sys
import threading
import time
from collections import deque
from email.message import Message
from pathlib import Path
from urllib.parse import urlsplit

VERSAO = "1"
PORTA_PADRAO = 47821
HOST = "127.0.0.1"
MAX_CORPO = 16 * 1024 * 1024
MAX_INDICES = 8  # memoized project indexes (each ~10 MB of rows for ~8k entries)
LOCAIS = {"127.0.0.1", "localhost", "::1"}

AQUI = Path(__file__).resolve().parent
PORTFOLIO_DIR = AQUI.parent / "portfolio"
HOOK = AQUI / "script_portfolio_gate.py"
#: The code a resident process holds: when any of these changes on disk, the loaded rule is stale.
FONTES = (
    HOOK,
    PORTFOLIO_DIR / "recuperar.py",
    PORTFOLIO_DIR / "federacao.py",
    PORTFOLIO_DIR / "simhash.py",
    PORTFOLIO_DIR / "heredoc.py",
)
#: Every ``sys.modules`` name the hook and the portfolio code register; dropped before a (re)load so nothing is stale.
MODULOS = (
    "script_portfolio_gate_http",
    "portfolio_recuperar",
    "portfolio_heredoc",
    "recuperar",
    "federacao",
    "simhash",
)
DESLIGADO = Path.home() / ".claude" / "touring" / "portfolio_gate" / "DISABLED"


def _assinatura() -> tuple:
    """(mtime_ns, size) of each source file; ``None`` for a missing one (a half-written edit is a signature too)."""
    out = []
    for fonte in FONTES:
        try:
            st = fonte.stat()
            out.append((st.st_mtime_ns, st.st_size))
        except OSError:
            out.append(None)
    return tuple(out)


class Motor:
    """One loaded generation of the code: the hook module, with the per-call loaders and the index read memoized."""

    def __init__(self) -> None:
        self.assinatura = _assinatura()
        for nome in MODULOS:
            sys.modules.pop(nome, None)
        importlib.invalidate_caches()
        if str(PORTFOLIO_DIR) not in sys.path:
            sys.path.insert(0, str(PORTFOLIO_DIR))
        spec = importlib.util.spec_from_file_location("script_portfolio_gate_http", HOOK)
        if spec is None or spec.loader is None:
            raise ImportError(f"nao consegui carregar {HOOK}")
        self.hook = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = self.hook
        spec.loader.exec_module(self.hook)
        # The hook re-executes these modules on EVERY call (right for a fresh process, a waste — and a reset of
        # module state — for a resident one): load each once and hand the same module back.
        self.recuperar = self.hook._recuperar()
        heredoc = self.hook._heredoc()
        self.hook._recuperar = lambda: self.recuperar
        self.hook._heredoc = lambda: heredoc
        self.indices: dict[str, tuple[tuple[int, int], list]] = {}
        self._memoizar_indice()

    def _memoizar_indice(self) -> None:
        federacao = importlib.import_module("federacao")
        original = federacao.fonte_projeto

        def fonte_projeto(indice: Path) -> list:
            try:
                st = indice.stat()
            except OSError:
                return original(indice)  # raises IndiceInvalido: the caller names the failed source
            chave = (st.st_mtime_ns, st.st_size)
            achado = self.indices.get(str(indice))
            if achado is not None and achado[0] == chave:
                return achado[1]
            registros = original(indice)  # also (re)writes the marshal cache the command transport uses
            self.indices.pop(str(indice), None)
            while len(self.indices) >= MAX_INDICES:
                self.indices.pop(next(iter(self.indices)))
            self.indices[str(indice)] = (chave, registros)
            return registros

        federacao.fonte_projeto = fonte_projeto

    def responder(self, payload: object) -> dict | None:
        """The hook's answer for one event. The caller holds the evaluation lock (see the module docstring)."""
        self.recuperar.ULTIMA_ROTA_TANTIVY = ""  # a fresh process starts here; a resident one must reset it
        return self.hook.responder(payload, "http")


class Estado:
    """Everything the handlers share: the current ``Motor``, the evaluation lock and the counters."""

    def __init__(self, porta: int) -> None:
        self.porta = porta
        self.inicio = time.time()
        self.lock = threading.Lock()
        self.motor = Motor()
        self.recargas = 0
        self.falhou_em: tuple | None = None
        self.requisicoes = 0
        self.invalidos = 0
        self.erros = 0
        self.atendimento_ms: deque[float] = deque(maxlen=1000)

    def motor_atual(self) -> Motor:
        """The ``Motor`` for THIS request; reloads when the code changed. Caller holds ``self.lock``."""
        atual = _assinatura()
        if atual == self.motor.assinatura or atual == self.falhou_em:
            return self.motor
        antes = {nome: sys.modules.get(nome) for nome in MODULOS}
        try:
            self.motor = Motor()
            self.recargas += 1
            self.falhou_em = None
        except Exception as exc:  # a half-written edit must not take the gate down: keep the old code, say so once
            self.falhou_em = atual
            print(
                f"[portfolio-gate] recarga falhou ({type(exc).__name__}: {exc}); mantendo o codigo anterior",
                file=sys.stderr,
                flush=True,
            )
            for nome, modulo in antes.items():  # the failed load dropped modules the old generation resolves lazily
                if modulo is None:
                    sys.modules.pop(nome, None)
                else:
                    sys.modules[nome] = modulo
        return self.motor

    def responder(self, payload: object) -> dict | None:
        t0 = time.perf_counter()
        with self.lock:
            try:
                return self.motor_atual().responder(payload)
            except Exception as exc:  # fail-open: the command hook's contract is "never blocks, never dies"
                self.erros += 1
                print(f"[portfolio-gate] erro ao decidir ({type(exc).__name__}: {exc})", file=sys.stderr, flush=True)
                return None
            finally:
                self.requisicoes += 1
                self.atendimento_ms.append((time.perf_counter() - t0) * 1000)

    def saude(self) -> dict:
        medidas = sorted(self.atendimento_ms)
        n = len(medidas)
        motor = self.motor
        return {
            "ok": True,
            "pid": os.getpid(),
            "porta": self.porta,
            "versao": VERSAO,
            "uptime_s": round(time.time() - self.inicio, 1),
            "requisicoes": self.requisicoes,
            "invalidos": self.invalidos,
            "erros": self.erros,
            "recargas": self.recargas,
            "desligado": _desligado(),
            "codigo": [None if a is None else a[0] for a in motor.assinatura],
            "indices": [
                {"indice": caminho, "mtime_ns": chave[0], "entradas": len(regs)}
                for caminho, (chave, regs) in list(motor.indices.items())
            ],
            "atendimento_ms": (
                {
                    "n": n,
                    "p50": round(medidas[n // 2], 2),
                    "p99": round(medidas[min(n - 1, int(0.99 * n))], 2),
                    "max": round(medidas[-1], 2),
                }
                if n
                else {"n": 0}
            ),
        }


def _desligado() -> bool:
    return DESLIGADO.exists()


def _host_local(host: str | None) -> bool:
    """A ``Host`` header naming loopback (port ignored). An absent header is refused: every HTTP/1.1 client sends it."""
    if not host:
        return False
    try:
        return urlsplit(f"//{host}").hostname in LOCAIS
    except ValueError:
        return False


def _tamanho(headers: Message) -> int | None:
    """The declared body length, or ``None`` when absent, malformed, negative or above ``MAX_CORPO``."""
    try:
        tamanho = int(headers.get("Content-Length") or "")
    except ValueError:
        return None
    return tamanho if 0 <= tamanho <= MAX_CORPO else None


def _recusa(headers: Message) -> tuple[int, str] | None:
    """``(status, reason)`` for a POST that must not reach the decision; ``None`` when it may."""
    if not _host_local(headers.get("Host")):
        return 403, "Host nao e loopback"
    if not (headers.get("Content-Type") or "").lower().startswith("application/json"):
        return 415, "esperava application/json"
    if _tamanho(headers) is None:
        return 413, "Content-Length ausente, invalido ou grande demais"
    return None


def _corpo_json(texto: str) -> object | None:
    try:
        return json.loads(texto)
    except ValueError:
        return None


def fabricar_handler(estado: Estado) -> type[http.server.BaseHTTPRequestHandler]:
    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"  # keep-alive: Claude Code reuses the connection
        timeout = 10  # an idle or stalled client frees its thread
        disable_nagle_algorithm = True
        wbufsize = 65536  # headers + body leave in ONE segment (two small writes + Nagle = a 40 ms stall)

        def log_message(self, *_args: object) -> None:  # one access-log line per Write would flood the journal
            return

        def _responder(self, codigo: int, corpo: dict, fechar: bool = False) -> None:
            dados = json.dumps(corpo).encode()
            self.send_response(codigo)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(dados)))
            if fechar:
                self.send_header("Connection", "close")  # also sets ``close_connection``
            self.end_headers()
            self.wfile.write(dados)

        def _recusar(self, codigo: int, motivo: str) -> None:
            # An unread body would poison the next request on this socket: a refusal closes it.
            self._responder(codigo, {"erro": motivo}, fechar=True)

        def do_GET(self) -> None:
            if self.path != "/health":
                self._recusar(404, "rota inexistente")
                return
            self._responder(200, estado.saude())

        def do_POST(self) -> None:
            if self.path != "/hook":
                self._recusar(404, "rota inexistente")
                return
            if (recusa := _recusa(self.headers)) is not None:
                self._recusar(*recusa)
                return
            bruto = self.rfile.read(_tamanho(self.headers) or 0)  # always drained: keep-alive needs the socket clean
            desligado = _desligado()
            payload = None if desligado else _corpo_json(bruto.decode("utf-8", errors="replace"))
            if desligado:
                self._responder(200, {})
            elif not isinstance(payload, dict):
                estado.invalidos += 1
                self._responder(200, {})  # fail-open: an unreadable event never blocks
            else:
                self._responder(200, estado.responder(payload) or {})

    return Handler


def servir(porta: int) -> http.server.ThreadingHTTPServer:
    """Bind loopback and return the server (not yet serving). Port 0 picks an ephemeral port: read ``server_port``."""
    estado = Estado(porta)
    servidor = http.server.ThreadingHTTPServer((HOST, porta), fabricar_handler(estado))
    servidor.daemon_threads = True
    estado.porta = servidor.server_port
    servidor.estado = estado  # type: ignore[attr-defined]
    return servidor


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=int(os.environ.get("PORTFOLIO_GATE_PORT", PORTA_PADRAO)))
    args = ap.parse_args(argv)
    try:
        servidor = servir(args.port)
    except OSError as exc:
        print(f"[portfolio-gate] nao consegui abrir {HOST}:{args.port} ({exc})", file=sys.stderr, flush=True)
        return 1
    signal.signal(signal.SIGTERM, lambda *_: threading.Thread(target=servidor.shutdown, daemon=True).start())
    print(json.dumps({"servindo": {"host": HOST, "porta": servidor.server_port, "pid": os.getpid()}}), flush=True)
    try:
        servidor.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        servidor.server_close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
