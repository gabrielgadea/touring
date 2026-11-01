"""HORUST — o MESMO binario (com adaptador Rust) no DISCO DE HOJE (corpus natural, overlays vazios) e no INSTANTE (corpus derivado, crate
sobreposto), nos 37 positivos do gerador. Responde a pergunta da coordenadora: quantos positivos o estado do instante RESTAURA (o disco de
hoje apaga o rotulo; o instante o devolve) e em quais situacoes o instante nao restaura.

Celulas (so os 26 que o crate completo e a regra da classe A deixam no corpus):
  restaurado    disco nao acusa o nome, instante acusa
  igual_acha    acusa nos dois (o disco de hoje ja acusa; o instante nao e prova de nada)
  igual_perde   nao acusa em nenhum
  piorou        disco acusa, instante nao
Os 11 que saem do corpus (classe A ou INCOMPLETA) ficam a parte.

Uso: python3 -I -B comparar_disco_instante.py <disco.json> <instante.json> <saida.json>
"""

import collections
import json
import sys
from pathlib import Path

D = Path(__file__).resolve().parent
disco = json.loads(Path(sys.argv[1]).read_text())["ids"]
inst = json.loads(Path(sys.argv[2]).read_text())["ids"]
itens = {i["id"]: i for i in next(iter(json.loads((D / "dados/itens.json").read_text()).values()))}
perdas = {s["id"]: s for s in json.loads((D / "dados_dev_crate.perdas.json").read_text())["itens"]}


def acha(r: dict) -> bool:
    return any(f["nome"] == r["nome_esperado"] for f in r.get("findings", []))


pos37 = sorted(i for i, r in disco.items() if r["rotulo"] == "positivo")
saida_do_corpus = {i: ("classe_A" if itens[i].get("classe_a") else "incompleta") for i in pos37 if i not in inst}
cel: dict = collections.defaultdict(list)
for i in pos37:
    if i in saida_do_corpus:
        continue
    d, s = acha(disco[i]), acha(inst[i])
    cel["restaurado" if (not d and s) else "igual_acha" if (d and s) else "piorou" if d else "igual_perde"].append(i)
neg = [i for i, r in disco.items() if r["rotulo"] == "negativo" and i in inst]


def resumo(ids: list[str]) -> list[dict]:
    return [{"id": i, "classe": disco[i]["classe"], "nome": disco[i]["nome_esperado"], "disco": disco[i].get("verdict"), "instante": inst[i].get("verdict"),
             "causa": perdas.get(i, {}).get("causa")} for i in ids]


res = {
    "n_positivos_gerador": len(pos37),
    "saem_do_corpus": dict(collections.Counter(saida_do_corpus.values())),
    "positivos_no_corpus": sum(len(v) for v in cel.values()),
    "celulas": {k: len(v) for k, v in cel.items()},
    "veredito_disco_nos_37": dict(collections.Counter(disco[i].get("verdict") for i in pos37)),
    "detalhe": {k: resumo(v) for k, v in cel.items()},
    "negativos_comuns": len(neg),
    "negativos": {"deny_disco": sum(disco[i].get("verdict") == "deny" for i in neg), "deny_instante": sum(inst[i].get("verdict") == "deny" for i in neg),
                  "com_aviso_disco": sum(bool(disco[i]["findings"]) for i in neg), "com_aviso_instante": sum(bool(inst[i]["findings"]) for i in neg),
                  "erros": sum("erro" in disco[i] or "erro" in inst[i] for i in neg)},
}
Path(sys.argv[3]).write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
print(json.dumps({k: v for k, v in res.items() if k != "detalhe"}, ensure_ascii=False))
for k, v in res["detalhe"].items():
    print("##", k, len(v))
    for x in v:
        print("  ", x["id"], x["classe"].ljust(8), x["nome"][:24].ljust(24), x["disco"], x["instante"], x["causa"] or "")
