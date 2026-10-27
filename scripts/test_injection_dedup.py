#!/usr/bin/env python3
"""Testes do client/hooks/injection_dedup.py (S-4p, 26/10/2026).

O contrato é FAIL-OPEN EM TODA FALHA — binário ausente, subcomando
desconhecido, saída vazia, exceção — mais o roundtrip real quando há um
binário touring-hook com `dedup-stdin` (o target/debug ou release do
workspace; sem ele, o roundtrip é skip declarado, nunca verde falso).
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "client", "hooks"))

import injection_dedup  # noqa: E402

PASS = 0
FAIL = 0
SKIP = 0


def check(name: str, cond: bool, detail: str = "") -> None:
    global PASS, FAIL
    if cond:
        PASS += 1
        print(f"ok   {name}")
    else:
        FAIL += 1
        print(f"FALHA {name} {detail}")


def skip(name: str, why: str) -> None:
    global SKIP
    SKIP += 1
    print(f"skip {name} ({why})")


def find_workspace_binary() -> str | None:
    root = os.path.abspath(os.path.join(HERE, ".."))
    for profile in ("debug", "release"):
        cand = os.path.join(root, "target", profile, "touring-hook")
        if os.path.isfile(cand) and os.access(cand, os.X_OK):
            # O binário tem o subcomando? (binário velho sai 0 com stdout vazio)
            proc = subprocess.run(
                [cand, "dedup-stdin", "--session", "probe", "--cwd", root],
                input='{"probe": true}', capture_output=True, text=True, timeout=5, check=False,
            )
            if proc.returncode == 0 and proc.stdout.strip():
                return cand
    return None


def main() -> int:
    # 1. Sem sessão → input intacto.
    out = '{"hookSpecificOutput": {"additionalContext": "x"}}'
    check(
        "sem session_id devolve o input intacto",
        injection_dedup.dedup_output(out, session_id="") == out,
    )

    # 2. Binário inexistente → input intacto (fail-open).
    os.environ["TOURING_HOOK_BIN"] = "/caminho/que/nao/existe/touring-hook"
    # _binary cai para o próximo candidato; forçar o fracasso total exige PATH
    # vazio também — o contrato testado aqui é o do candidato inválido + PATH
    # real: se houver touring-hook no PATH o resultado é o do roundtrip, então
    # o assert é sobre NUNCA levantar e SEMPRE devolver texto.
    try:
        res = injection_dedup.dedup_output(out, session_id="s")
        check("binário inválido nunca levanta", isinstance(res, str) and len(res) > 0)
    except Exception as exc:  # noqa: BLE001
        check("binário inválido nunca levanta", False, f"{type(exc).__name__}: {exc}")
    finally:
        del os.environ["TOURING_HOOK_BIN"]

    # 3. Kill switch humano → input intacto sem tocar o binário.
    os.environ["TOURING_DEDUP_DISABLED"] = "1"
    check(
        "TOURING_DEDUP_DISABLED=1 devolve intacto",
        injection_dedup.dedup_output(out, session_id="s") == out,
    )
    del os.environ["TOURING_DEDUP_DISABLED"]

    # 4. Roundtrip real com o binário do workspace (quando presente).
    binary = find_workspace_binary()
    if binary is None:
        skip(
            "roundtrip no binário real",
            "nenhum touring-hook com dedup-stdin em target/{debug,release}",
        )
    else:
        os.environ["TOURING_HOOK_BIN"] = binary
        # Sessão ÚNICA por run: um cwd sem marcador de projeto (tempdir) resolve
        # para o store global ~/.claude/touring/ pelo mesmo normalize_project_root
        # do daemon — uma sessão fixa colide entre runs (medido 26/10).
        sess = f"sess-teste-dedup-{os.getpid()}-{time.time_ns()}"
        try:
            with tempfile.TemporaryDirectory() as tmp:
                # Marcador de projeto: sem ele o normalize_project_root cai no
                # store global ~/.claude/touring/ e cada run deixaria estado lá.
                os.mkdir(os.path.join(tmp, ".git"))
                ctx = ("  (cached 300s — set TOURING_SUGGESTER_DISABLED=1 to silence)"
                       " prosa longa de verdade para o roundtrip")
                payload = json.dumps({
                    "hookSpecificOutput": {
                        "hookEventName": "PostToolUse",
                        "additionalContext": ctx,
                    }
                }, ensure_ascii=False)
                kw = {"session_id": sess, "cwd": tmp}
                o1 = injection_dedup.dedup_output(payload, **kw)
                check("1ª emissão passa intacta", ctx in o1, o1[:120])
                o2 = injection_dedup.dedup_output(payload, **kw)
                check("2ª emissão idêntica é omitida", ctx not in o2, o2[:120])
                o3 = injection_dedup.dedup_output(
                    payload, session_id=f"{sess}-outra", cwd=tmp,
                )
                check("outra sessão nunca herda o estado", ctx in o3, o3[:120])
        finally:
            del os.environ["TOURING_HOOK_BIN"]

    print(f"\n{PASS} ok · {FAIL} falhas · {SKIP} skips")
    return 1 if FAIL else 0


if __name__ == "__main__":
    sys.exit(main())
