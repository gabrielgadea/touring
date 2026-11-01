"""Consolida os DOIS held-outs x os DOIS binarios: n, falsos deny, limite superior 95% (Clopper-Pearson, exato,
unilateral), concentracao (arquivos/sessoes distintos, maior fatia) e os construtos dos falsos.

Le: k_v2_holdout.json / c8b5_v2_holdout.json (temporal) e k_v2_disjunto.json / c8b5_v2_disjunto.json (disjunto) e os
classificar_*.json de cada par. Grava consolidado_holdouts.json e tabela_holdouts.md ao lado. Leitura apenas.
"""

import json
import sys
from collections import Counter
from pathlib import Path

SP = Path(__file__).resolve().parent
sys.path.insert(0, str(SP))
import exclusoes_disjunto as ex
from limites import limite_superior

VGP = ex.VGP
CASOS = [
    ("TEMPORAL", VGP / "corpus_holdout", {"e935": "k_v2_holdout", "c8b5": "c8b5_v2_holdout"}, {"e935": "classificar_holdout_e935", "c8b5": "classificar_holdout_c8b5"}),
    ("DISJUNTO", VGP / "corpus_holdout_disjunto", {"e935": "k_v2_disjunto", "c8b5": "c8b5_v2_disjunto"}, {"e935": "classificar_disjunto_e935", "c8b5": "classificar_disjunto_c8b5"}),
]


def balde(e: dict) -> str:
    return "inline" if e["origem"] == "inline" else "arquivo"  # a regra do juiz (tmp conta como arquivo)


def pct(x: float) -> str:
    return f"{100 * x:.2f}%"


def concentracao(entradas: list[dict]) -> dict:
    neg = [e for e in entradas if e["rotulo"] == "negativo"]
    arq = [e for e in neg if balde(e) == "arquivo"]
    inl = [e for e in neg if balde(e) == "inline"]
    f, s = Counter(e["path"] for e in arq), Counter(e["session"] for e in inl)
    return {"arquivo": {"n": len(arq), "arquivos_distintos": len(f), "maior_arquivo": max(f.values(), default=0),
                        "maior_fatia_pct": round(100 * max(f.values(), default=0) / max(len(arq), 1), 2)},
            "inline": {"n": len(inl), "sessoes_distintas": len(s), "maior_sessao": max(s.values(), default=0),
                       "maior_fatia_pct": round(100 * max(s.values(), default=0) / max(len(inl), 1), 2)}}


resultado, linhas_md = {}, ["| held-out | binario | balde | n | falsos deny | taxa | limite superior 95% (CP) |", "|---|---|---|---|---|---|---|"]
for nome, hold, jsons, classifs in CASOS:
    entradas = ex.entradas(hold)
    por_id = {e["id"]: e for e in entradas}
    resultado[nome] = {"concentracao": concentracao(entradas), "binarios": {}}
    for binario, stem in jsons.items():
        arq_json = SP / f"{stem}.json"
        if not arq_json.is_file():
            continue
        d = json.loads(arq_json.read_text(encoding="utf-8"))
        cl = json.loads((SP / f"{classifs[binario]}.json").read_text(encoding="utf-8")) if (SP / f"{classifs[binario]}.json").is_file() else {"achados": []}
        achados_por_id: dict[str, list[dict]] = {}
        for a in cl["achados"]:
            achados_por_id.setdefault(a["id"], []).append(a)
        baldes = {}
        for b in ("arquivo", "inline"):
            n = d["negativos"][b]
            falsos = [f for f in d["falsos"] if f["balde"] == b]
            k = len(falsos)
            grupo = Counter((por_id[f["id"]]["path"] if b == "arquivo" else por_id[f["id"]]["session"]) for f in falsos)
            baldes[b] = {"n": n, "falsos": k, "taxa": k / n if n else None, "limite_sup_95": limite_superior(k, n) if n else None,
                         "clusters_dos_falsos": len(grupo), "falsos_ids": [f["id"] for f in falsos],
                         "construtos": dict(Counter(a["construto"] for f in falsos for a in achados_por_id.get(f["id"], []))),
                         "classes_dos_achados": dict(Counter(a["classe"] for f in falsos for a in achados_por_id.get(f["id"], [])))}
            linhas_md.append(f"| {nome} | {binario} | {b} | {n} | {k} | {pct(k / n) if n else '-'} | {pct(limite_superior(k, n)) if n else '-'} |")
        resultado[nome]["binarios"][binario] = {
            "baldes": baldes, "recall_controle": d["positivos_simbolo"], "avisos_em_negativos": d["avisos_em_negativos"],
            "p95_ms": round(d["p95_ms"], 1), "chamadas": d["chamadas"], "erros": len(d["erros"]), "defeito_de_corpus": d["defeito_de_corpus"],
            "pendencias": d["pendencias"]}
(SP / "consolidado_holdouts.json").write_text(json.dumps(resultado, ensure_ascii=False, indent=1), encoding="utf-8")
(SP / "tabela_holdouts.md").write_text("\n".join(linhas_md) + "\n", encoding="utf-8")
print("\n".join(linhas_md))
for nome, r in resultado.items():
    print(nome, "concentracao:", r["concentracao"])
    for binario, b in r["binarios"].items():
        print(f"  {binario}: recall controle {b['recall_controle']} | avisos {b['avisos_em_negativos']} | p95 {b['p95_ms']} ms | chamadas {b['chamadas']} | erros {b['erros']}")
        for bal, v in b["baldes"].items():
            if v["falsos"]:
                print(f"     {bal}: {v['falsos']} falsos em {v['clusters_dos_falsos']} clusters | construtos {v['construtos']}")
