#!/usr/bin/env python3
"""comparar_porid2.py <base.porid.json> <novo.porid.json> <saida.json>

Duas reguas POR ID sobre os `por_id` de rodar_juiz_instrumentado.py (so leitura):
  R1 (a de comparar_porid.py): verdict + (classe, nome, linha, severidade) de cada achado, ordenados.
  R2 (nova): igualdade dos `findings` INTEIROS — todo campo de cada achado, `reais` e `sugestao` inclusive,
     achados na ordem do motor — e do registro menos os campos de tempo (`elapsed_ms`, `ms`, `tempo*`).
Com o determinismo do P0 integrado, R2 tem de dar 0 entre DUAS execucoes do MESMO binario.
Para cada id que difere em R2 mas nao em R1, `so_em` lista os campos que mudaram (reais, sugestao, ...).
Saida: JSON com resumo + divergencias; resumo tambem no stdout.
"""
import json
import sys
from collections import Counter
from pathlib import Path

TEMPO = ("elapsed_ms", "ms", "tempo", "duracao")


def ler(p):
    return json.loads(Path(p).read_text(encoding="utf-8"))["por_id"]


def r1(rec):
    if "_erro" in rec:
        return ("erro", rec["_erro"])
    achados = sorted(
        (f.get("classe"), f.get("nome"), f.get("linha"), f.get("severidade")) for f in rec.get("findings", [])
    )
    return (rec.get("verdict"), tuple(achados))


def limpa(rec):
    return {k: v for k, v in rec.items() if not any(k.startswith(t) for t in TEMPO)}


def campos_que_mudam(a, b):
    fa, fb = a.get("findings", []), b.get("findings", [])
    campos = set()
    for k in set(a) | set(b):
        if k != "findings" and not any(k.startswith(t) for t in TEMPO) and a.get(k) != b.get(k):
            campos.add(k)
    if len(fa) != len(fb):
        campos.add("findings.len")
    for x, y in zip(fa, fb):
        for k in set(x) | set(y):
            if x.get(k) != y.get(k):
                campos.add(f"findings.{k}")
    return sorted(campos)


def main():
    base, novo, saida = ler(sys.argv[1]), ler(sys.argv[2]), Path(sys.argv[3])
    ids = sorted(set(base) | set(novo))
    d1, d2, so_em = [], [], Counter()
    detalhe = []
    for i in ids:
        if i not in base or i not in novo:
            d1.append(i)
            d2.append(i)
            detalhe.append({"id": i, "causa": "so_em_um_lado"})
            continue
        a, b = base[i], novo[i]
        if r1(a) != r1(b):
            d1.append(i)
        if limpa(a) != limpa(b):
            d2.append(i)
            campos = campos_que_mudam(a, b)
            if r1(a) == r1(b):
                so_em[",".join(campos)] += 1
            detalhe.append({"id": i, "r1_difere": r1(a) != r1(b), "campos": campos})
    resumo = {
        "comparados": len(set(base) & set(novo)), "so_base": len(set(base) - set(novo)), "so_novo": len(set(novo) - set(base)),
        "R1_divergencias": len(d1), "R2_divergencias_findings_inteiros": len(d2),
        "R2_sem_R1_so_em": dict(so_em),
    }
    saida.write_text(json.dumps({"resumo": resumo, "ids_R1": d1, "ids_R2": d2, "detalhe": detalhe}, indent=1, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(resumo, ensure_ascii=False))


main()
