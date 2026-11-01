#!/usr/bin/env python3
"""Agrega os tempos pareados de `tempos_det.sh`: por (modo, binario) o p95/mediana do MINIMO por id entre as repeticoes,
o p95 de cada repeticao e, no V3, quantos ids tiveram a camada 2 estourada (prazo de 1000 ms) em cada repeticao.

uso: agrega_tempos.py <dir tempos_det> <saida.json>"""
import json
import math
import re
import statistics
import sys
from collections import defaultdict
from pathlib import Path

D, SAIDA = Path(sys.argv[1]), Path(sys.argv[2])


def p95(v):
    v = sorted(v)
    return v[min(len(v) - 1, math.ceil(0.95 * len(v)) - 1)] if v else None


reps = defaultdict(dict)  # (modo, bin) -> {rep: {id: ms}}
c2 = defaultdict(dict)    # (modo, bin) -> {rep: {id: camada2}}
for f in sorted(D.glob("*_*_*.json")):
    m = re.fullmatch(r"(l1|v3)_([0-9a-f]{8})_(\d)\.json", f.name)
    if not m:
        continue
    modo, binario, rep = m.group(1), m.group(2), int(m.group(3))
    reps[(modo, binario)][rep] = json.loads(f.read_text())
    cf = f.with_suffix(".camada2.json")
    if cf.exists():
        c2[(modo, binario)][rep] = json.loads(cf.read_text())

res = {}
for (modo, binario), por_rep in sorted(reps.items()):
    ids = sorted(set.intersection(*(set(r) for r in por_rep.values())))
    minimos = [min(r[i] for r in por_rep.values() if r[i] is not None) for i in ids]
    item = {
        "reps": sorted(por_rep),
        "ids": len(ids),
        "p95_minimo_por_id_ms": round(p95(minimos), 1),
        "mediana_minimo_por_id_ms": round(statistics.median(minimos), 2),
        "p95_por_repeticao_ms": {r: round(p95([x for x in v.values() if x is not None]), 1) for r, v in sorted(por_rep.items())},
    }
    if c2.get((modo, binario)):
        item["camada2_estourada_por_repeticao"] = {
            r: sum(1 for v in d.values() if v and "prazo de 1000 ms" in v) for r, d in sorted(c2[(modo, binario)].items())
        }
    res[f"{modo} {binario}"] = item

# pareado: diferenca do minimo por id contra a base e935aaf1, no mesmo modo
for (modo, binario), por_rep in sorted(reps.items()):
    if binario == "e935aaf1" or (modo, "e935aaf1") not in reps:
        continue
    base = reps[(modo, "e935aaf1")]
    ids = sorted(set.intersection(*(set(r) for r in list(por_rep.values()) + list(base.values()))))
    dif = [min(r[i] for r in por_rep.values()) - min(r[i] for r in base.values()) for i in ids]
    res[f"{modo} {binario} - e935aaf1 (min por id)"] = {
        "ids": len(ids),
        "mediana_ms": round(statistics.median(dif), 2),
        "p95_da_diferenca_ms": round(p95(dif), 1),
        "ids_mais_lentos_em_mais_de_5ms": sum(1 for x in dif if x > 5),
        "ids_mais_rapidos_em_mais_de_5ms": sum(1 for x in dif if x < -5),
    }
SAIDA.write_text(json.dumps(res, indent=1, ensure_ascii=False))
print(json.dumps(res, indent=1, ensure_ascii=False))
