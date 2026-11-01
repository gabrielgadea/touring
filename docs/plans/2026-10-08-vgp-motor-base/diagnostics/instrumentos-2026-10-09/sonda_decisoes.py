"""Sonda da DECISÃO dos gates no touring-hook instalado (passo 4 do release agrupado, 09/10/2026).

Projeto temporário em modo `code`; cada caso afirma o campo-veredito do JSON, nunca só o texto:
S3 avisa sem decidir; G8 e G2 reescrevem preservando os campos da chamada; G8 declara o orçamento.
Saída: JSON com cada caso e `ok`; exit 1 se algum falhar.
"""

import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HOOK = Path.home() / ".local/bin/touring-hook"
ENV = {k: v for k, v in os.environ.items() if k not in ("TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK")}


def pre(root: Path, sessao: str, tool_input: dict) -> dict:
    payload = {"tool_name": "Bash", "session_id": sessao, "cwd": str(root), "tool_input": tool_input}
    p = subprocess.run(
        [str(HOOK), "cli-suggest"], input=json.dumps(payload), capture_output=True,
        text=True, cwd=str(root), env=ENV, timeout=60,
    )
    try:
        return json.loads(p.stdout or "{}").get("hookSpecificOutput", {})
    except json.JSONDecodeError:
        return {"_stdout_nao_json": p.stdout[:200], "_rc": p.returncode, "_stderr": p.stderr[:200]}


def main() -> int:
    nonce = f"{os.getpid()}-{time.monotonic_ns()}"
    casos = []
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / ".touring").mkdir()
        (root / ".touring/touring.toml").write_text('[code_mode]\nmode = "code"\n')
        s = f"sonda-s3-{nonce}"
        o1 = pre(root, s, {"command": "grep -rn alfa src/"})
        o2 = pre(root, s, {"command": "grep -rn beta src/"})
        o3 = pre(root, s, {"command": "grep -rn gama src/"})
        ctx2 = o2.get("additionalContext", "")
        casos.append({"caso": "S3 1ª passa", "ok": "CODE MODE · rajada" not in json.dumps(o1, ensure_ascii=False)})
        casos.append({"caso": "S3 2ª avisa SEM decisão", "ok": "[CODE MODE · rajada] aviso" in ctx2
                      and "permissionDecision" not in o2 and "updatedInput" not in o2, "resp": o2})
        casos.append({"caso": "S3 3ª sem repetir o aviso", "ok": "CODE MODE · rajada" not in json.dumps(o3, ensure_ascii=False)})

        laco = 'for f in a.rs b.rs; do grep -n "fn main" $f; done'
        o8 = pre(root, f"sonda-g8-{nonce}", {"command": laco, "timeout": 300000, "description": "laço"})
        u8 = o8.get("updatedInput", {})
        casos.append({"caso": "G8 reescreve com orçamento e campos", "ok": o8.get("permissionDecision") == "allow"
                      and "--timeout-ms 295000" in u8.get("command", "") and u8.get("timeout") == 300000
                      and u8.get("description") == "laço", "resp": o8})

        cmd2 = "cargo test 2>&1 | tail -3; echo EXIT=$?"
        o2b = pre(root, f"sonda-g2-{nonce}", {"command": cmd2, "timeout": 600000, "run_in_background": True})
        u2 = o2b.get("updatedInput", {})
        casos.append({"caso": "G2 reescreve preservando background e timeout", "ok": o2b.get("permissionDecision") == "allow"
                      and u2.get("command") == f"set -o pipefail; {cmd2}" and u2.get("timeout") == 600000
                      and u2.get("run_in_background") is True, "resp": o2b})
    print(json.dumps({"hook": str(HOOK), "casos": casos}, ensure_ascii=False, indent=1))
    return 0 if all(c["ok"] for c in casos) else 1


if __name__ == "__main__":
    sys.exit(main())
