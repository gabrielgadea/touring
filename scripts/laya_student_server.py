#!/usr/bin/env python3
"""laya_student_server — serve o aluno W4.2 no protocolo /v1/systemone do Jev (W4.4).

Stdlib apenas (http.server + ThreadingHTTPServer) — zero dependências novas.
O wire é o da API TypeSafe: POST /v1/systemone {state, model, questions} →
{model, answers, usage}, que é o que scripts/jev_client.py e o hook (W4.4) falam.
O init do Agent (18,7s medido) acontece no boot, nunca no caminho do hook.
Uso: LAYA_STUDENT_PORT=8971 python3 scripts/laya_student_server.py
"""

from __future__ import annotations

import json
import os
import time
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

MODEL_DIR = str(Path("~/.claude/touring/laya-w42-intent/output").expanduser())
MODEL_NAME = "laya-w42-intent"
HOST = os.environ.get("LAYA_STUDENT_HOST", "127.0.0.1")
PORT = int(os.environ.get("LAYA_STUDENT_PORT", "8971"))

_agent = None


def get_agent():
    """Agent único, carregado uma vez (init 18,7s no boot, nunca por request)."""
    global _agent
    if _agent is None:
        import laya

        _agent = laya.Agent(model_id_or_path=MODEL_DIR, device=os.environ.get("LAYA_DEVICE", "cuda"))
    return _agent


def build_answer(ans: Any) -> dict[str, Any]:
    """Normaliza a resposta do Agent para o wire do Jev."""
    get = (lambda k, d=None: getattr(ans, k, d)) if not isinstance(ans, dict) else (lambda k, d=None: ans.get(k, d))
    confidence = get("confidence") if get("confidence") is not None else get("answer_confidence")
    probs = get("probabilities") or {}
    # o lambda `get` aceita default — antes `get("values", {})` explodia em TypeError
    # para TODA resposta sem `choice` top-level (noul e score caíam em 500; cross-audit 01/10)
    choice = get("choice") or (get("values") or {}).get("choice")
    if choice is not None:
        return {"type": "choice", "choice": choice, "probabilities": probs, "confidence": confidence}
    p = get("noul")
    if p is not None:
        return {"type": "noul", "noul": p}
    return {
        "type": "score",
        "score": get("score"),
        "probabilities": probs,
        "legend": get("legend") or {},
        "confidence": confidence,
    }


def answer_questions(state: Any, questions: dict[str, dict[str, Any]]) -> dict[str, Any]:
    agent = get_agent()
    built: dict[str, Any] = {}
    for qid, q in questions.items():
        qtype = q.get("type")
        instructions = q.get("instructions")
        criteria = q.get("criteria")
        if qtype == "choice":
            built[qid] = {"type": "choice", "instructions": instructions, "criteria": criteria or {}}
        elif qtype == "noul":
            built[qid] = {"type": "noul", "instructions": instructions, **({"criteria": criteria} if criteria else {})}
        elif qtype == "score":
            built[qid] = {"type": "score", "instructions": instructions, "criteria": criteria or []}
    state_text = state if isinstance(state, str) else json.dumps(state, ensure_ascii=False)
    result = agent.predict([state_text], built)
    raw = result.answers if hasattr(result, "answers") else result["answers"]
    return {qid: build_answer(raw[qid]) for qid in built}


def parse_systemone_body(raw: bytes) -> tuple[dict[str, Any] | None, str | None]:
    """Parse + validação do body do wire /v1/systemone.

    (body, None) no caminho bom; (None, erro) no ruim — o erro NOMEIA o campo
    (A5: a mensagem ensina a correção), e o handler traduz para 422 em vez do
    500 opaco (cross-audit 2026-10-01: `questions` como lista virava 500 mudo).
    """
    try:
        body = json.loads(raw or b"{}")
    except json.JSONDecodeError as exc:
        return None, f"body não é JSON válido: {exc}"
    if not isinstance(body, dict):
        return None, "body deve ser um objeto JSON"
    questions = body.get("questions")
    if questions is not None and not isinstance(questions, dict):
        return None, "campo 'questions' deve ser um objeto {id: spec}, não lista/string"
    for qid, spec in (questions or {}).items():
        if not isinstance(spec, dict):
            return None, f"questions['{qid}'] deve ser um objeto com type/instructions/criteria"
    return body, None


class Handler(BaseHTTPRequestHandler):
    def _send(self, code: int, payload: dict[str, Any]) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args) -> None:  # silêncio de acesso: quem mede é o journal do cliente
        return

    def do_GET(self) -> None:  # noqa: N802 (nome exigido pela stdlib)
        if self.path == "/health":
            self._send(200, {"status": "ok", "model": MODEL_NAME})
        else:
            self._send(404, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802
        if self.path != "/v1/systemone":
            self._send(404, {"error": "not found"})
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            body, err = parse_systemone_body(self.rfile.read(length))
            if err is not None:
                self._send(422, {"error": err})
                return
            started = time.monotonic()
            answers = answer_questions(body.get("state"), body.get("questions") or {})
            self._send(
                200,
                {
                    "model": MODEL_NAME,
                    "answers": answers,
                    "usage": {"input_tokens": 0, "output_tokens": 0, "latency_ms_server": round((time.monotonic() - started) * 1000, 1)},
                },
            )
        except (KeyError, ValueError) as exc:
            self._send(422, {"error": f"invalid request: {exc}"})
        except Exception as exc:  # o servidor nunca derruba por um request ruim
            traceback.print_exc()  # fail-LOUD: o 500 vai ao log, nunca ao silêncio
            self._send(500, {"error": f"{type(exc).__name__}: {exc}"})


def main() -> int:
    agent = get_agent()  # preload no boot
    # Warm-up REAL: a 1ª inferência após o boot custa ~630ms medido (JIT/pool) —
    # acima do timeout de 200ms do hook, que perdia as primeiras shadow lines
    # (S-2, prova viva 01/10). Um predict de aquecimento antes de servir fecha o buraco.
    started = time.monotonic()
    answer_questions({"prompt": "warmup"}, {"w": {"type": "choice", "instructions": "classify", "criteria": {"general": None}}})
    warm_ms = (time.monotonic() - started) * 1000
    server = ThreadingHTTPServer((HOST, PORT), Handler)
    print(f"laya-student ouvindo em http://{HOST}:{PORT} (modelo {MODEL_NAME}, warmup {warm_ms:.0f}ms)", flush=True)
    server.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
