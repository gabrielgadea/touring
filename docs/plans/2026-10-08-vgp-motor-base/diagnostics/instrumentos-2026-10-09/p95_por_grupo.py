"""De onde vem o p95 de ~678 ms no holdout (o vivo deu ~104 ms com o MESMO binario antigo)? Tempo de cada chamada do
motor, agrupado por project_root e balde. Usa o proprio `rodar_motor` do juiz. Espera a V3 sair antes de medir.
"""

import json
import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
sys.path.insert(0, str(SCRIPTS))
import juiz_motor_vgp as jm

SP = Path(__file__).resolve().parent
KIMI = str(SP / "touring_kimi_e935")
CORPUS = SP / "corpus_holdout_ctl"
VIVO = Path.home() / ".claude/touring/vgp/corpus_v0d"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def v3_presente() -> bool:
    return subprocess.run(["pgrep", "-f", "[j]uiz_motor_vgp.py.*camadas"], capture_output=True).returncode == 0


def p95(xs: list[float]) -> float:
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(0.95 * len(xs)))] if xs else 0.0


while v3_presente():
    time.sleep(5)
for nome, corpus in (("HOLDOUT (hibrido)", CORPUS), ("VIVO", VIVO)):
    pos, neg = jm.carregar(corpus)
    grupos: dict[str, list[float]] = defaultdict(list)
    todos = []
    for e in pos + neg:
        if not jm.snap_compila(corpus, e):
            continue
        t0 = time.monotonic()
        jm.rodar_motor([KIMI], e, corpus)
        ms = 1000 * (time.monotonic() - t0)
        todos.append(ms)
        grupos[f"{Path(e['project_root']).name or '?'} · {e['rotulo']} · {jm.balde(e)}"].append(ms)
    out(f"== {nome}: chamadas {len(todos)} | p95 {p95(todos):.0f} ms | mediana {sorted(todos)[len(todos) // 2]:.0f} ms | max {max(todos):.0f} ms | V3 durante: {v3_presente()}")
    for g, xs in sorted(grupos.items()):
        out(f"   {g:42s} n={len(xs):3d} mediana={sorted(xs)[len(xs) // 2]:6.0f} p95={p95(xs):6.0f} max={max(xs):6.0f}")
