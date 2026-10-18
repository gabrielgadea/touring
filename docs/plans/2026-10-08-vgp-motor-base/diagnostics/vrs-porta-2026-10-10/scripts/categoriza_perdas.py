"""Categoriza as perdas de `nome` do corpus natural no binario/ids de uma variante: onde o nome e usado e onde e definido no
PROPRIO snapshot (raiz do arquivo ou dentro de qual `mod` inline), o que o motor respondeu e onde o nome vive hoje.

Uso: python3 -I categoriza_perdas.py SC ROTULO   (le var_<ROTULO>_ids_natural.json)"""

import json
import pathlib
import re
import sys

SC = pathlib.Path(sys.argv[1])
ROT = sys.argv[2]
N = pathlib.Path("/home/gabrielgadea/.claude/touring/vgp/corpus_rust_natural")
ent = {}
for ln in (N / "entradas.jsonl").read_text().splitlines():
    e = json.loads(ln)
    ent[e["id"]] = e
res = json.loads((SC / ("var_%s_ids_natural.json" % ROT)).read_text())["ids"]


def modulo_em(linhas, alvo):
    """Pilha de `mod x {` abertos (coluna 0 ou indentados) na linha `alvo` (1-based), por contagem de chaves dos mod."""
    pilha = []
    prof = 0
    for n, t in enumerate(linhas, 1):
        if n == alvo:
            return [m for m, _ in pilha]
        m = re.match(r"\s*(?:pub(?:\([a-z]+\))?\s+)?mod\s+(\w+)\s*\{", t)
        if m:
            pilha.append((m.group(1), prof))
        prof += t.count("{") - t.count("}")
        while pilha and prof <= pilha[-1][1]:
            pilha.pop()
    return []


for i, e in ent.items():
    if e["rotulo"] != "positivo" or e.get("classe") != "nome":
        continue
    r = res.get(i, {})
    if any(f["nome"] == e["nome_esperado"] for f in r.get("findings", [])):
        continue
    nome = e["nome_esperado"]
    linhas = (N / e["snap"]).read_text().splitlines()
    defs = [n for n, t in enumerate(linhas, 1) if re.search(r"\b(fn|const|static|struct|enum|type|trait)\s+%s\b" % re.escape(nome), t)]
    print(
        json.dumps(
            {
                "id": i[:6],
                "nome": nome,
                "uso_linha": e["linha"],
                "uso_em_mod": modulo_em(linhas, e["linha"]),
                "defs_no_snapshot": [(n, modulo_em(linhas, n)) for n in defs[:3]],
                "verdict": r.get("verdict"),
                "motivo": (r.get("motivo") or "")[:120],
                "achados": [(f["nome"], f["linha"]) for f in r.get("findings", [])][:3],
                "introduzido": e.get("introduzido"),
                "contexto_overlays": len(e.get("contexto") or []),
            },
            ensure_ascii=False,
        )
    )
