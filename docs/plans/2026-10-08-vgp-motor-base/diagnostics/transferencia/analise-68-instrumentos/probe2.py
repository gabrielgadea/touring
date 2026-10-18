"""Sonda 2 — a linha de base do frontend e a estratificação do HTML (o que é gerado vs. escrito a mão)."""

import json
import re
import subprocess
import time
from collections import Counter
from pathlib import Path

ANALISE = Path.home() / "projects/analise"
FRONTEND = ANALISE / "frontend"
IGNORAR = {"node_modules", ".git", "dist", ".next", "build", "__pycache__", ".venv"}

CDN = re.compile(r"""(?:src|href)\s*=\s*["'](https?:)?//[^"']+""", re.I)
CLASSE = re.compile(r"""class\s*=\s*["'][^"']+["']""", re.I)
LOCAL = re.compile(r"""(?:src|href)\s*=\s*["'](?!https?:|//|#|mailto:|data:|tel:|javascript:)[^"']+""", re.I)
GERADOR = re.compile(r"gerad[oa]|generated|doctype html>\s*$", re.I | re.M)


def caminha(raiz: Path, exts: tuple[str, ...]):
    for e in exts:
        for p in raiz.rglob(f"*{e}"):
            if any(x in IGNORAR for x in p.parts) or not p.is_file():
                continue
            yield p


# 1) linha de base do frontend
r = subprocess.run(
    [str(FRONTEND / "node_modules/.bin/tsc"), "--noEmit", "-p", "."],
    cwd=FRONTEND,
    capture_output=True,
    text=True,
    timeout=900,
)
erros = [x for x in r.stdout.splitlines() if "error TS" in x]
base = {"rc": r.returncode, "erros": len(erros), "amostra": erros[:3]}

# 2) estratificação do HTML
camadas = Counter()
tamanhos = []
candidatos = []
for p in caminha(ANALISE, (".html",)):
    try:
        t = p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        camadas["ilegivel"] += 1
        continue
    kb = p.stat().st_size / 1024
    tamanhos.append(kb)
    cdn, cl, lo = bool(CDN.search(t)), bool(CLASSE.search(t)), bool(LOCAL.search(t))
    chave = (
        ("cdn" if cdn else "") + ("|classe" if cl else "") + ("|local" if lo else "")
    ) or "nenhum"
    camadas[chave] += 1
    if cdn or cl or lo:
        candidatos.append((str(p), round(kb, 1), chave))

tamanhos.sort()
print(
    json.dumps(
        {
            "frontend_tsc": base,
            "html_estratos": dict(camadas.most_common()),
            "html_tamanho_kb": {
                "p50": tamanhos[len(tamanhos) // 2] if tamanhos else 0,
                "p90": tamanhos[int(len(tamanhos) * 0.9)] if tamanhos else 0,
                "max": tamanhos[-1] if tamanhos else 0,
                "ate_400kb": sum(1 for x in tamanhos if x <= 400),
            },
            "html_com_sinal": len(candidatos),
            "amostra_candidatos": candidatos[:12],
        },
        ensure_ascii=False,
        indent=1,
    )
)