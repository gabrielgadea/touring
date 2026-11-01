"""Falsos deny de `atributo` do juiz VGP: o receptor e como ele está ligado no snap (predicado positivo)."""

from __future__ import annotations

import ast
import collections
import json
import sys
from pathlib import Path

from classificar_falsos_vgp import CORPUS, entradas, ligacao

BUILTIN_TIPOS = (dict, list, str, set, bytes, tuple, int, float)


def metodo_builtin(nome: str) -> bool:
    return any(hasattr(t, nome) for t in BUILTIN_TIPOS) or hasattr(Path, nome)


def receptores(arvore: ast.AST, attr: str, linha: int) -> list[ast.expr]:
    return [n.value for n in ast.walk(arvore) if isinstance(n, ast.Attribute) and n.attr == attr and n.lineno == linha]


def classe_receptor(arvore: ast.AST, rec: ast.expr) -> str:
    if isinstance(rec, ast.Name):
        return f"nome:{ligacao(arvore, rec.id)}"
    return f"expr:{type(rec).__name__}"


def main() -> int:
    juiz = json.loads(Path(sys.argv[1]).read_text())
    ent = entradas()
    tab: collections.Counter = collections.Counter()
    ex: dict = {}
    for f in juiz["falsos"]:
        e = ent[f["id"]]
        texto = (CORPUS / e["snap"]).read_text(errors="replace")
        arvore = ast.parse(texto)
        for a in f["achados"]:
            if a["severidade"] != "deny" or a["classe"] != "atributo":
                continue
            recs = receptores(arvore, a["nome"], a["linha"])
            rc = classe_receptor(arvore, recs[0]) if recs else "receptor-nao-achado"
            chave = (f["balde"], rc, "metodo-de-builtin" if metodo_builtin(a["nome"]) else "outro")
            tab[chave] += 1
            r = ast.unparse(recs[0])[:40] if recs else "?"
            ex.setdefault(chave, []).append(f"{f['id']}:{a['linha']} {r}.{a['nome']}  sug={a['sugestao'][:70]!r}")
    for k, n in tab.most_common():
        print(f"{n:4}  {k}")
        for x in ex[k][:2]:
            print(f"        {x}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
