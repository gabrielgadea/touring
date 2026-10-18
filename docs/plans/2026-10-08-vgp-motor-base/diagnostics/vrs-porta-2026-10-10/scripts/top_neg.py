import collections
import json
import sys

ids = json.load(open(sys.argv[1]))["ids"]
neg = {i: r for i, r in ids.items() if r["rotulo"] == "negativo" and "erro" not in r}
com = {i: r for i, r in neg.items() if r["findings"]}
print(f"negativos {len(neg)}; com achado {len(com)}; veredito {collections.Counter(r['verdict'] for r in neg.values())}")
por_classe = collections.Counter(f["classe"] for r in com.values() for f in r["findings"])
print("achados por classe:", dict(por_classe))
c = collections.Counter((f["classe"], f["nome"]) for r in com.values() for f in r["findings"])
n_arq = collections.Counter()
for r in com.values():
    for k in {(f["classe"], f["nome"]) for f in r["findings"]}:
        n_arq[k] += 1
print("top (classe, nome) por n de negativos afetados:")
for (cl, n), k in n_arq.most_common(22):
    print(f"  {k:3d} {cl:8s} {n}")
