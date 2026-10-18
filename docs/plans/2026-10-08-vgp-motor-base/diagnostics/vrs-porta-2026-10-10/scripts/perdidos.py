import json
import pathlib
import sys

ids = json.load(open(sys.argv[1]))["ids"]
corpus = pathlib.Path(sys.argv[2])
ents = {}
for l in (corpus / "entradas.jsonl").read_text().splitlines():
    if l.strip():
        e = json.loads(l)
        ents[e["id"]] = e
n = 0
for i, r in sorted(ids.items()):
    e = ents[i]
    if e["rotulo"] != "positivo" or str(e.get("reproduz")).lower() != "true":
        continue
    achou = any(f["nome"] == e["nome_esperado"] for f in r["findings"])
    if achou:
        continue
    n += 1
    snap = (corpus / e["snap"]).read_text().splitlines()
    lin = e.get("linha") or 0
    trecho = snap[lin - 1].strip()[:110] if 0 < lin <= len(snap) else "?"
    outros = [f["nome"] for f in r["findings"]][:3]
    print(
        f"{e['classe']:8s} {e.get('codigo','?'):6s} esperado={e['nome_esperado'][:26]:26s} "
        f"veredito={r['verdict']:7s} outros={outros} | L{lin}: {trecho}"
    )
print("perdidos:", n)
