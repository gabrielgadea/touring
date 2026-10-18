"""p95 do motor A/B INTERCALADO (mesma carga): para cada id roda A e B em seguida; imprime p50/p95/max e as maiores regressoes.

Uso: touring run --file p95_ab.py --args '[CORPUS_DIR, BIN_A, BIN_B]'"""

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

corpus, ba, bb = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
pos, neg = juiz.carregar(corpus)
ea, eb, par = [], [], []
for e in pos + neg:
    a = juiz.rodar_motor([ba], e, corpus).get("elapsed_ms", -1.0)
    b = juiz.rodar_motor([bb], e, corpus).get("elapsed_ms", -1.0)
    ea.append(a)
    eb.append(b)
    par.append((b - a, e["id"], str(e.get("path", "")).split("/")[-1], a, b))


def p(v, q):
    v = sorted(v)
    return v[min(len(v) - 1, int(q * len(v)))]


print("n", len(ea), "| A p50 %.0f p95 %.0f max %.0f | B p50 %.0f p95 %.0f max %.0f" % (p(ea, 0.5), p(ea, 0.95), max(ea), p(eb, 0.5), p(eb, 0.95), max(eb)))
print("acima de 150 ms: A %d  B %d" % (sum(x > 150 for x in ea), sum(x > 150 for x in eb)))
for d, i, f, a, b in sorted(par, reverse=True)[:10]:
    print(i, f, "%.0f -> %.0f" % (a, b))
