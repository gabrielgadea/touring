#!/usr/bin/env python3
"""Compara DOIS `medir_ids.py` (antes x depois), por id e por classe. So agregados no stdout.

Uso: python3 -I comparar_ids.py <antes.json> <depois.json> [--lista-diferencas N]
Para cada id: veredito e achados {classe,nome,linha,severidade}. Reporta idênticos, diferentes (com o
tipo da diferença) e a tabela de recall de DETECÇÃO por classe (deny OU warn com o `nome_esperado`) e de
falso deny (qualquer deny em negativo), antes e depois, com denominadores.
"""

import collections
import json
import sys
from pathlib import Path


def carregar(p: str) -> dict:
    return json.loads(Path(p).read_text())["ids"]


def chave(r: dict):
    return (
        r.get("verdict"),
        tuple(
            sorted(
                (f["classe"], f["nome"], f["linha"], f["severidade"]) for f in r["findings"]
            )
        ),
    )


def detectou(r: dict, nome: str) -> bool:
    return any(f["nome"] == nome for f in r.get("findings", []))


def tabela(ids: dict) -> dict:
    t = collections.defaultdict(lambda: [0, 0])
    falsos = [0, 0]
    for r in ids.values():
        if "erro" in r:
            continue
        if r["rotulo"] == "positivo":
            c = r.get("classe") or "?"
            t[c][1] += 1
            t[c][0] += detectou(r, r["nome_esperado"])
        else:
            falsos[1] += 1
            falsos[0] += any(f["severidade"] == "deny" for f in r["findings"])
    return {"recall": dict(t), "falso_deny": falsos}


def main() -> int:
    antes, depois = carregar(sys.argv[1]), carregar(sys.argv[2])
    n_dif = int(sys.argv[sys.argv.index("--lista-diferencas") + 1]) if "--lista-diferencas" in sys.argv else 10
    comuns = sorted(set(antes) & set(depois))
    so_um = sorted(set(antes) ^ set(depois))
    iguais, diferentes = [], []
    for i in comuns:
        (iguais if chave(antes[i]) == chave(depois[i]) else diferentes).append(i)
    # as diferencas, classificadas
    tipos = collections.Counter()
    exemplos = collections.defaultdict(list)
    for i in diferentes:
        a, d = chave(antes[i]), chave(depois[i])
        if a[1] == d[1]:
            tipo = f"so veredito: {a[0]} -> {d[0]}"
        else:
            tipo = f"achados diferem ({a[0]} -> {d[0]})"
        tipos[tipo] += 1
        if len(exemplos[tipo]) < n_dif:
            exemplos[tipo].append(i)
    erros = [i for i in comuns if "erro" in antes[i] or "erro" in depois[i]]
    print(
        json.dumps(
            {
                "ids_antes": len(antes),
                "ids_depois": len(depois),
                "so_em_um_dos_dois": len(so_um),
                "identicos": len(iguais),
                "diferentes": len(diferentes),
                "com_erro_do_motor": len(erros),
                "tipos_de_diferenca": dict(tipos),
                "exemplos": dict(exemplos),
                "antes": tabela(antes),
                "depois": tabela(depois),
            },
            ensure_ascii=False,
            indent=1,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
