import collections
import json
import pathlib
import sys

d = [x for x in json.load(open(sys.argv[1])) if "classe" in x]


def zona(a):
    p = pathlib.PurePosixPath(a).parts
    for z in ("tests", "benches", "examples"):
        if z in p:
            return z
    return "src"


arqs = collections.defaultdict(set)
cls = collections.defaultdict(collections.Counter)
for x in d:
    z = zona(x["arquivo"])
    arqs[z].add(x["arquivo"])
    cls[z][x["classe"]] += 1
tot = collections.Counter()
raiz = pathlib.Path("/home/gabrielgadea/projects/touring-wt-semeado")
for p in (raiz / "crates").rglob("*.rs"):
    if any(c in p.parts for c in ("touring-code", "touring-identity", "touring-pyright")) and "target" not in p.parts:
        tot[zona(str(p.relative_to(raiz)))] += 1
for z in ("src", "tests", "benches", "examples"):
    print(z, "arquivos", tot[z], "com achado", len(arqs[z]), dict(cls[z]))
srcs = [x for x in d if zona(x["arquivo"]) == "src"]
c = collections.Counter((x["classe"], x["nome"]) for x in srcs)
print("--- src (nao-teste), top:")
for (cl, n), k in c.most_common(14):
    ex = next(x for x in srcs if x["classe"] == cl and x["nome"] == n)
    print(f"{k:3d} {cl:8s} {n:22s} {ex['arquivo']}:{ex['linha']} | {ex['sugestao'][:60]}")
