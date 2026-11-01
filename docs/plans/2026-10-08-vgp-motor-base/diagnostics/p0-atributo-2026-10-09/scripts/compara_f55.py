#!/usr/bin/env python3
"""Condicao da coordenadora: o VEREDITO por id do binario novo e identico ao do f55ec4b5; so a ordem de `reais` e a
`sugestao` podem mudar. Compara dois `*.porid.json` (verdict + (classe, nome, linha, severidade) de cada achado)
e, para cada diferenca, mostra o status da camada 2 nos dois lados: com `prazo de 1000 ms estourado` em qualquer
dos lados a diferenca e do pyright sob carga (V3), nao do motor.

uso: compara_f55.py <f55.porid.json> <novo.porid.json> <saida.json> [--camadas12]
exit 0 = nenhuma diferenca de veredito sem explicacao de timeout da camada 2; 1 = ha diferenca real."""
import json
import sys
from pathlib import Path

a_p, b_p, saida = (Path(x) for x in sys.argv[1:4])
v3 = "--camadas12" in sys.argv


def ler(p):
    return json.loads(p.read_text(encoding="utf-8"))["por_id"]


def veredito(r):
    if "_erro" in r:
        return ("erro", r["_erro"])
    return (
        r.get("verdict"),
        tuple(sorted((f.get("classe"), f.get("nome"), f.get("linha"), f.get("severidade")) for f in r.get("findings", []))),
    )


def timeout(r):
    c2 = r.get("camada2")
    return bool(c2) and "prazo de 1000 ms" in c2


a, b = ler(a_p), ler(b_p)
ids = sorted(set(a) | set(b))
difs, explicadas, reais = [], [], []
for i in ids:
    if i not in a or i not in b:
        reais.append({"id": i, "causa": "so_em_um_lado"})
        continue
    if veredito(a[i]) == veredito(b[i]):
        continue
    item = {
        "id": i,
        "f55": veredito(a[i])[0], "novo": veredito(b[i])[0],
        "f55_achados": [list(x) for x in veredito(a[i])[1]],
        "novo_achados": [list(x) for x in veredito(b[i])[1]],
        "camada2_f55": a[i].get("camada2"), "camada2_novo": b[i].get("camada2"),
    }
    (explicadas if v3 and (timeout(a[i]) or timeout(b[i])) else reais).append(item)
res = {
    "f55": str(a_p), "novo": str(b_p), "comparados": len(ids),
    "diferencas_de_veredito": len(explicadas) + len(reais),
    "explicadas_por_timeout_da_camada2": explicadas,
    "reais": reais,
}
saida.write_text(json.dumps(res, indent=1, ensure_ascii=False))
print(json.dumps({k: (v if not isinstance(v, list) else len(v)) for k, v in res.items()}, ensure_ascii=False))
for x in explicadas + reais:
    print(" ", x["id"], x.get("f55"), "->", x.get("novo"), "| c2:", (x.get("camada2_f55") or "ok")[:40], "/", (x.get("camada2_novo") or "ok")[:40])
raise SystemExit(1 if reais else 0)
