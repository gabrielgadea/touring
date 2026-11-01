"""Tempo do motor (elapsed_ms) por id, N repeticoes: minimo e mediana, para comparar binarios sob a mesma carga.

Uso: touring run --file tempo_ids.py --args '[CORPUS_DIR, N, BIN1,BIN2, id, id, ...]'  (BINs separados por virgula)"""

import importlib.util
import statistics
import sys
from pathlib import Path

JUIZ = Path(
    "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/juiz_motor_vgp.py"
)
spec = importlib.util.spec_from_file_location("juiz_motor_vgp_ro", JUIZ)
juiz = importlib.util.module_from_spec(spec)
sys.modules["juiz_motor_vgp_ro"] = juiz
spec.loader.exec_module(juiz)

corpus, n, bins, *ids = sys.argv[1:]
corpus = Path(corpus)
n = int(n)
bins = bins.split(",")
pos, neg = juiz.carregar(corpus)
por_id = {e["id"]: e for e in pos + neg}
for i in ids:
    e = por_id.get(i)
    if e is None:
        print(i, "ausente")
        continue
    linha = [i, str(e.get("path", "")).split("/")[-1][:28]]
    for b in bins:
        ts = []
        for _ in range(n):
            r = juiz.rodar_motor([b], e, corpus)
            ts.append(r.get("elapsed_ms", -1.0))
        linha.append("%s: min %.0f med %.0f" % (b.split("/")[-2][:8], min(ts), statistics.median(ts)))
    print(" | ".join(linha))
