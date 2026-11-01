"""Compara, POR ID, o veredito do motor de dois binarios sobre o mesmo corpus.

Entrada: os `porid.json` de `rodar_juiz_instrumentado.py` (base e novo) e, opcionalmente, os JSONs do
juiz (base e novo) para cruzar `perdidos` e `falsos` por id. Compara verdict + (classe, nome, linha,
severidade) de cada achado; `reais` e `sugestao` ficam de fora (o motor e nao-deterministico neles).

Uso: python3 -I comparar_porid.py <base.porid.json> <novo.porid.json> <saida.json> [<base.juiz.json> <novo.juiz.json>]
"""

import json
import sys
from collections import Counter
from pathlib import Path

base_p, novo_p, saida = (Path(a) for a in sys.argv[1:4])
juizes = [Path(a) for a in sys.argv[4:6]]


def ler(p):
    return json.loads(p.read_text(encoding="utf-8"))


def veredito(rec):
    if "_erro" in rec:
        return ("erro", rec["_erro"])
    achados = sorted(
        (f.get("classe"), f.get("nome"), f.get("linha"), f.get("severidade"))
        for f in rec.get("findings", [])
    )
    return (rec.get("verdict"), tuple(achados))


base, novo = ler(base_p)["por_id"], ler(novo_p)["por_id"]
ids = sorted(set(base) | set(novo))
diverge, categorias = [], Counter()
for i in ids:
    if i not in base or i not in novo:
        categorias["so_em_um_lado"] += 1
        diverge.append({"id": i, "causa": "so_em_um_lado"})
        continue
    vb, vn = veredito(base[i]), veredito(novo[i])
    if vb == vn:
        continue
    db = {a for a in vb[1] if a[3] == "deny"} if vb[0] != "erro" else set()
    dn = {a for a in vn[1] if a[3] == "deny"} if vn[0] != "erro" else set()
    if dn - db:
        cat = "NOVO_DENY"  # o que o binario novo nega e o base nao negava
    elif db - dn:
        cat = "deny_removido"
    else:
        cat = "warn_ou_veredito"
    categorias[cat] += 1
    diverge.append({
        "id": i, "causa": cat,
        "base": {"verdict": vb[0], "achados": [list(a) for a in vb[1]]},
        "novo": {"verdict": vn[0], "achados": [list(a) for a in vn[1]]},
    })

resumo = {
    "comparados": len(set(base) & set(novo)), "so_base": len(set(base) - set(novo)),
    "so_novo": len(set(novo) - set(base)), "divergencias": len(diverge),
    "por_categoria": dict(categorias),
}
if len(juizes) == 2:
    jb, jn = ler(juizes[0]), ler(juizes[1])

    def ids_de(j, chave):
        return sorted(x["id"] if isinstance(x, dict) else x for x in j.get(chave, []))

    resumo["juiz"] = {
        "perdidos_base": ids_de(jb, "perdidos"), "perdidos_novo": ids_de(jn, "perdidos"),
        "perdidos_novos_no_novo": sorted(set(ids_de(jn, "perdidos")) - set(ids_de(jb, "perdidos"))),
        "recuperados_no_novo": sorted(set(ids_de(jb, "perdidos")) - set(ids_de(jn, "perdidos"))),
        "falsos_base": ids_de(jb, "falsos"), "falsos_novo": ids_de(jn, "falsos"),
        "avisos_em_negativos": [jb.get("avisos_em_negativos"), jn.get("avisos_em_negativos")],
        "recall": [jb.get("recall_simbolo"), jn.get("recall_simbolo")],
        "positivos": [jb.get("positivos_simbolo"), jn.get("positivos_simbolo")],
        "p95_ms": [jb.get("p95_ms"), jn.get("p95_ms")], "pronto": [jb.get("pronto"), jn.get("pronto")],
    }
saida.write_text(json.dumps({"resumo": resumo, "divergencias": diverge}, indent=1, ensure_ascii=False), encoding="utf-8")
print(json.dumps(resumo, indent=1, ensure_ascii=False))
