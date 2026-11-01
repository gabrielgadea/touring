import json
import sys

vistos = {}
for p in sys.argv[1:]:
    d = json.load(open(p))
    for r in d["resultados"]:
        if r["id"] in ("BASE",):
            continue
        # a 2a rodada substitui a 1a para o mesmo id
        vistos[r["id"]] = r
mortos = 0
for mid, r in vistos.items():
    if mid == "RED-stub":
        print(f"{mid}: {r['resultado']} contagem(passaram,falharam)={r.get('contagem')}")
        continue
    mortos += bool(r.get("morto"))
    ts = ", ".join(t.split("::")[-1] for t in r.get("testes_que_mataram", []))
    print(f"{mid}: {'MORTO' if r.get('morto') else r['resultado']} <- {ts}")
print(f"total de mutantes: {len(vistos) - 1}; mortos: {mortos}")
