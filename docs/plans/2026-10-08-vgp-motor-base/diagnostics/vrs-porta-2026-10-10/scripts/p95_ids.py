import json
import math
import sys

for rot_corpus in sys.argv[1:]:
    ids = json.load(open(rot_corpus))["ids"]
    t = sorted(r["elapsed_ms"] for r in ids.values() if r.get("elapsed_ms") is not None)
    p95 = t[min(len(t) - 1, math.ceil(0.95 * len(t)) - 1)]
    print(f"{rot_corpus.split('/')[-1]}: n={len(t)} p50={t[len(t)//2]:.1f} p95={p95:.1f} max={t[-1]:.1f} ms")
