#!/usr/bin/env python3
"""Determinismo do motor VGP camada 1: cada id do corpus roda K vezes no MESMO binario; a saida (sem
`elapsed_ms`) tem de ser byte a byte a mesma. Usa o executor do proprio juiz (`rodar_motor`), so leitura.

uso: det_varre.py <binario> <corpus> <saida.json> [K=5] [--ids a,b,c]
"""
import importlib.util
import json
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
spec = importlib.util.spec_from_file_location("juiz", B / "scripts/juiz_motor_vgp.py")
juiz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(juiz)

binario, corpus, saida = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
k = int(sys.argv[4]) if len(sys.argv) > 4 and not sys.argv[4].startswith("--") else 5
so_ids = None
if "--ids" in sys.argv:
    so_ids = set(sys.argv[sys.argv.index("--ids") + 1].split(","))

pos, neg = juiz.carregar(corpus)
entradas = {e["id"]: e for e in pos + neg}
if so_ids:
    entradas = {i: e for i, e in entradas.items() if i in so_ids}


def canon(out: dict) -> str:
    out = dict(out)
    out.pop("elapsed_ms", None)
    return json.dumps(out, sort_keys=True, ensure_ascii=False)


def um(item):
    ident, e = item
    runs = [juiz.rodar_motor([binario], e, corpus) for _ in range(k)]
    distintos = {canon(r) for r in runs}
    info = {"distintos": len(distintos)}
    if len(distintos) > 1:
        ach = [r.get("findings", []) for r in runs if "_erro" not in r]
        ordens = {json.dumps([f.get("reais") for f in a]) for a in ach}
        conjuntos = {json.dumps([sorted(f.get("reais") or []) for f in a]) for a in ach}
        sugest = {json.dumps([f.get("sugestao") for f in a], ensure_ascii=False) for a in ach}
        resto = {
            json.dumps(
                {**{kk: vv for kk, vv in r.items() if kk not in ("elapsed_ms", "findings")},
                 "f": [{kk: vv for kk, vv in f.items() if kk not in ("reais", "sugestao")} for f in r.get("findings", [])]},
                sort_keys=True, ensure_ascii=False)
            for r in runs if "_erro" not in r
        }
        info |= {
            "ordens_distintas_de_reais": len(ordens),
            "conjuntos_distintos_de_reais": len(conjuntos),
            "sugestoes_distintas": len(sugest),
            "resto_identico": len(resto) == 1,
        }
    erros = [r["_erro"] for r in runs if "_erro" in r]
    if erros:
        info["erros"] = erros[:2]
    return ident, info


with ThreadPoolExecutor(max_workers=2) as pool:
    res = dict(pool.map(um, entradas.items()))

div = {i: v for i, v in res.items() if v["distintos"] > 1}
err = {i: v for i, v in res.items() if "erros" in v}
resumo = {
    "binario": binario,
    "corpus": str(corpus),
    "runs_por_id": k,
    "ids": len(res),
    "ids_divergentes": len(div),
    "ids_com_erro": len(err),
    "divergentes": div,
    "erros": err,
}
saida.write_text(json.dumps(resumo, indent=1, ensure_ascii=False))
print(json.dumps({kk: vv for kk, vv in resumo.items() if kk not in ("divergentes", "erros")}, ensure_ascii=False))
print("divergentes:", sorted(div))
