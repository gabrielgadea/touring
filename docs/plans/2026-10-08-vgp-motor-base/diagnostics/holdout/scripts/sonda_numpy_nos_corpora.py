"""Por que o vivo teve 0 falsos deny com `np.array` em 18 snaps e o disjunto teve 1 em 5? Roda o e935 (via a MESMA
`rodar_motor` do juiz) em todo snap que cita np.array/np.where e mostra veredito, achados de `array`/`where`,
project_root e a forma do import. Leitura apenas.
"""

import json
import re
import sys
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
sys.path.insert(0, str(SCRIPTS))
import juiz_motor_vgp as j  # noqa: E402

SP = Path(__file__).resolve().parent
BIN = str(SP / "touring_kimi_e935")
VGP = Path.home() / ".claude/touring/vgp"
PADRAO = re.compile(r"np\.array|np\.where")


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


for nome, base in (("VIVO", VGP / "corpus_v0d"), ("DISJUNTO", VGP / "corpus_holdout_disjunto")):
    entradas = []
    for arq in ("entradas.jsonl", "entradas_inline_neg.jsonl"):
        p = base / arq
        if p.is_file():
            entradas += [json.loads(ln) for ln in p.read_text(encoding="utf-8").splitlines() if ln.strip()]
    for e in entradas:
        snap = base / e["snap"]
        if not snap.is_file() or not PADRAO.search(snap.read_text(encoding="utf-8", errors="replace")):
            continue
        texto = snap.read_text(encoding="utf-8", errors="replace")
        saida = j.rodar_motor([BIN], e, base)
        achados = [(f.get("severidade"), f.get("nome")) for f in saida.get("findings", saida.get("achados", [])) if f.get("nome") in ("array", "where")]
        imp = re.findall(r"^\s*(import numpy[^\n]*|from numpy[^\n]*)", texto, re.M)[:2]
        topo = bool(re.search(r"^import numpy as np", texto, re.M))
        out(f"{nome} {e['id']} {e['rotulo']:8} {e['origem']:9} root={Path(e['project_root']).name:9} topo={topo!s:5} import={imp} -> {saida.get('verdict', saida.get('_erro'))} {achados}")
