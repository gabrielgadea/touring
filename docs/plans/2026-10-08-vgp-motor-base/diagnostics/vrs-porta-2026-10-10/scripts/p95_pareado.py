"""Razao pareada de elapsed_ms por id entre duas medicoes (ids_*.json) e os mais lentos de cada uma.
Uso: touring run --file p95_pareado.py --args '[SC, ids_A.json, ids_B.json]'"""

import json
import statistics
import sys

SC, fa, fb = sys.argv[1:4]
A = json.load(open(SC + "/" + fa))["ids"]
B = json.load(open(SC + "/" + fb))["ids"]


def p(v, q):
    v = sorted(v)
    return v[min(len(v) - 1, int(q * len(v)))]


comuns = [k for k in A if k in B and "elapsed_ms" in A[k] and "elapsed_ms" in B[k]]
ea = [A[k]["elapsed_ms"] for k in comuns]
eb = [B[k]["elapsed_ms"] for k in comuns]
print("n", len(comuns), "p50", round(p(ea, 0.5), 1), round(p(eb, 0.5), 1), "p95", round(p(ea, 0.95), 1), round(p(eb, 0.95), 1), "max", round(max(ea), 1), round(max(eb), 1))
rz = [b / a for a, b in zip(ea, eb) if a > 0.5]
print("razao B/A: mediana", round(statistics.median(rz), 2), "p90", round(p(rz, 0.9), 2))
top = sorted(comuns, key=lambda k: -B[k]["elapsed_ms"])[:8]
for k in top:
    print(k[:60], round(A[k]["elapsed_ms"], 1), round(B[k]["elapsed_ms"], 1), B[k].get("rotulo"), B[k].get("verdict"))
