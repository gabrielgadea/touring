#!/usr/bin/env python3
"""Dedupe de injeção por época — ponte Python → Rust canônico (S-4p, 26/10/2026).

Os hooks Python são processos separados e nunca passam pelo `run_hook_command`
do daemon, onde a dedupe da S8 vive — por isso as famílias deles lideravam a
repetição medida no corpus de 400 sessões (evidence-guard 96,2% de excedente,
2.356 blocos → 134 distintos). Este módulo aplica a MESMA dedupe
(`touring_hooks_shared::injection_dedup::dedup_hook_output`) à saída JSON do
hook, via o subcomando stateless `touring-hook dedup-stdin`.

O estado é o arquivo por sessão (`<root>/.claude/touring/dedup-<sessao>.txt`),
compartilhado com o caminho do daemon: a 1ª cópia emitida por QUALQUER caminho
protege a repetição em ambos, e a época avança no post-compact como sempre.

FAIL-OPEN EM TODA FALHA — binário ausente, subcomando desconhecido (binário
antigo), timeout, exceção: o texto original volta intacto. Um hook nunca perde
a saída por causa da dedupe.

Kill switch HUMANO: `TOURING_DEDUP_DISABLED=1` (o mesmo do lado Rust).

Uso:

    import injection_dedup
    print(injection_dedup.dedup_output(json.dumps(payload), session_id=sid, cwd=cwd))
"""
from __future__ import annotations

import os
import shutil
import subprocess

_TIMEOUT_S = 2.0


def _binary() -> str | None:
    """O binário touring-hook: env explícita, canal dev do usuário, PATH."""
    for cand in (
        os.environ.get("TOURING_HOOK_BIN"),
        os.path.expanduser("~/.local/bin/touring-hook"),
    ):
        if cand and os.path.isfile(cand) and os.access(cand, os.X_OK):
            return cand
    return shutil.which("touring-hook")


def dedup_output(output: str, session_id: str = "", cwd: str = "") -> str:
    """Devolve `output` com as repetições da época omitidas. Nunca levanta.

    `session_id` vazio devolve o input intacto: sem sessão não há arquivo de
    estado e a dedupe não tem como ser por época.
    """
    if not output or not session_id:
        return output
    if os.environ.get("TOURING_DEDUP_DISABLED", "0") not in ("", "0"):
        return output
    binary = _binary()
    if binary is None:
        return output
    argv = [binary, "dedup-stdin", "--session", session_id, "--cwd", cwd or os.getcwd()]
    try:
        proc = subprocess.run(
            argv, input=output, capture_output=True, text=True,
            timeout=_TIMEOUT_S, check=False,
        )
    except Exception:
        return output
    # Binário antigo sem o subcomando sai 0 com stdout vazio — também fail-open.
    if proc.returncode != 0 or not proc.stdout:
        return output
    return proc.stdout
