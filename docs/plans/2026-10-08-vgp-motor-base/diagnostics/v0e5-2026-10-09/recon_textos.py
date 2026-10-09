"""Recon V0e5: os TEXTOS REAIS que caem no balde G1 do v0e_negacoes.

O padrao `rajada-de-inspe|inspection burst|RAJADA` e compilado com re.I e casa por `search` em
QUALQUER ponto do texto. Este script imprime as 30 primeiras linhas LITERAIS que casam, para o
detector ser escrito contra o que existe e nao contra o que se supoe.

Uso: python3 recon_textos.py [--since 2026-09-17]
"""

import json
import re
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
sys.path.insert(0, str(SCRIPTS))
from inventario_erros import ROOTS, SINCE, _text  # noqa: E402
from v0e_negacoes import HOOK_DENIAL  # noqa: E402

G1_PAT = re.compile(r"rajada-de-inspe|inspection burst|RAJADA", re.I)


def main():
    since = sys.argv[sys.argv.index("--since") + 1] if "--since" in sys.argv else SINCE
    ts_cut = datetime.fromisoformat(since).timestamp()
    linhas, casaram, total = Counter(), Counter(), 0
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime < ts_cut:
                continue
            for line in f.open(encoding="utf-8", errors="replace"):
                if '"tool_result"' not in line:
                    continue
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if str(rec.get("timestamp", "")) < since:
                    continue
                for c in (rec.get("message") or {}).get("content") or []:
                    if not (isinstance(c, dict) and c.get("type") == "tool_result" and c.get("is_error")):
                        continue
                    t = _text(c)
                    if not HOOK_DENIAL.search(t):
                        continue
                    m = G1_PAT.search(t)
                    if not m:
                        continue
                    total += 1
                    casaram[m.group(0).lower()] += 1
                    primeira = next((ln.strip() for ln in t.splitlines() if G1_PAT.search(ln)), "")
                    linhas[primeira[:150]] += 1
    print(f"--since {since} · {total} negacoes casam o padrao G1\n")
    print(f"-- o que casou (m.group): {dict(casaram)}\n")
    print(f"-- {len(linhas)} primeiras linhas distintas; top 30 --")
    for ln, n in linhas.most_common(30):
        print(f"  {n:>5}  {ln!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())