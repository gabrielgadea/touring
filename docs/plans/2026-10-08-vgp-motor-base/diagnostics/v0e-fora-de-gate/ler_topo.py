"""Le o texto COMPLETO das classes pequenas, para groundedar o destino com a evidencia real.

A coluna `primeira_linha` do bruto corta em 200 chars e esconde a causa; aqui cada item aparece com o
texto inteiro ate 400 chars e o comando que o produziu.

Uso: python3 ler_topo.py
"""

import json
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts"
sys.path.insert(0, str(SCRIPTS))
from inventario_erros import (  # noqa: E402
    ROOTS,
    SINCE,
    _text,
    parse_errors,
    residue_class,
    unwrap_run,
)  # noqa: E402

ALVO = {
    "CLI touring/uv/outros (error:)",
    "traceback cortado (sem a linha da exceção)",
    "cargo falhou (sem diagnóstico no recorte)",
    "permissão/sandbox",
    "entrada de ferramenta inválida",
}
LIMITE = 5


def main():
    since_ts = datetime.fromisoformat(SINCE).timestamp()
    acc = defaultdict(list)
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime < since_ts:
                continue
            names, cmds = {}, {}
            for raw in f.open(encoding="utf-8", errors="replace"):
                if '"tool_use"' not in raw and '"tool_result"' not in raw:
                    continue
                try:
                    rec = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                ts = str(rec.get("timestamp", ""))
                if ts < SINCE or ts > "2026-10-08T20:20:00Z":
                    continue
                for c in (rec.get("message") or {}).get("content") or []:
                    if not isinstance(c, dict):
                        continue
                    if c.get("type") == "tool_use":
                        names[c["id"]] = c.get("name")
                        cmds[c["id"]] = str((c.get("input") or {}).get("command", ""))
                        continue
                    if names.get(c.get("tool_use_id")) != "Bash" or not c.get(
                        "is_error"
                    ):
                        continue
                    t = _text(c)
                    if parse_errors(unwrap_run(t)):
                        continue
                    cl = residue_class(t)
                    if cl in ALVO and len(acc[cl]) < LIMITE:
                        acc[cl].append(
                            (
                                f.stem,
                                c.get("tool_use_id"),
                                t,
                                cmds.get(c.get("tool_use_id"), ""),
                            )
                        )
    for cl, rows in acc.items():
        print(f"\n########## {cl}  (lidos {len(rows)})")
        for s, tid, t, cmd in rows:
            print(f"  {s[:8]} / {str(tid)[:12]}")
            print(f"    CMD: {cmd[:135]!r}")
            print(f"    TXT: {t[:400]!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
