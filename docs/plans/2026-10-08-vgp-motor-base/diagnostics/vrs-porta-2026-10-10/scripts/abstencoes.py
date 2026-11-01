"""Quantos arquivos caem em `nao_medido` e por que politica: contagem por familia de motivo, nos positivos e negativos
do corpus (ids_*.json de medir_ids) e no controle de 130 (controle130 --out). Lista os positivos com motivo.

Uso: python3 -I abstencoes.py IDS_NATURAL.json CONTROLE130.json [IDS_ANTERIOR.json]"""

import collections
import json
import sys

FAMILIAS = [
    ("p_dinamico", "dinâmico"),
    ("x_modulo_com_macro", "módulo com macro em nível de item"),
    ("glob_nao_enumeravel", "alvo irresolúvel"),
    ("item_associado", "item associado de enum"),
    ("super_ou_posicao", "posição do arquivo"),
    ("cadeia_funda", "cadeia de reexportação"),
    ("ciclo", "ciclo de reexportação"),
    ("sintaxe_ou_ilegivel", "ilegível ou com erro de sintaxe"),
]


def familia(m):
    for nome, trecho in FAMILIAS:
        if trecho in m:
            return nome
    return "outra"


ids = json.load(open(sys.argv[1]))["ids"]
cont = {"positivo": collections.Counter(), "negativo": collections.Counter()}
tot = {"positivo": 0, "negativo": 0}
pos_com_motivo = []
for i, r in ids.items():
    if "erro" in r:
        continue
    rot = r["rotulo"]
    tot[rot] += 1
    m = r.get("motivo")
    if m:
        cont[rot]["qualquer"] += 1
        for f in {familia(x) for x in m.split("; ")}:
            cont[rot][f] += 1
        if rot == "positivo":
            pos_com_motivo.append((i, r.get("classe"), r.get("nome_esperado"), any(f["nome"] == r["nome_esperado"] for f in r["findings"]), m[:110]))
print("natural: positivos %d / negativos %d" % (tot["positivo"], tot["negativo"]))
for rot in ("positivo", "negativo"):
    print(" ", rot, dict(cont[rot]))
for x in pos_com_motivo:
    print("   pos com nao_medido:", x)
ctl = json.load(open(sys.argv[2]))
cc = collections.Counter()
for e in ctl:
    if e.get("motivo"):
        cc["qualquer"] += 1
        for f in {familia(x) for x in e["motivo"].split("; ")}:
            cc[f] += 1
print("controle 130 (arquivos com nao_medido):", dict(cc))
if len(sys.argv) > 3:
    ant = json.load(open(sys.argv[3]))["ids"]
    perdidos = [i for i, r in ids.items() if r["rotulo"] == "positivo" and any(f["nome"] == r["nome_esperado"] for f in ant.get(i, {}).get("findings", [])) and not any(f["nome"] == r["nome_esperado"] for f in r["findings"])]
    ganhos = [i for i, r in ids.items() if r["rotulo"] == "positivo" and any(f["nome"] == r["nome_esperado"] for f in r["findings"]) and not any(f["nome"] == r["nome_esperado"] for f in ant.get(i, {}).get("findings", []))]
    print("positivos que deixaram de ser acusados vs anterior:", perdidos, "| que passaram a ser:", ganhos)
