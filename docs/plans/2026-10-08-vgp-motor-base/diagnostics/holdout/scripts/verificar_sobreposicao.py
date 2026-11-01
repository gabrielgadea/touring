"""Sobreposicao do held-out DISJUNTO com o vivo e com o temporal, nas tres chaves (+ contexto), de forma INDEPENDENTE do
construtor (recomputa tudo dos arquivos gravados). Controle positivo: o proprio vivo contra ele mesmo tem de dar > 0,
e o temporal contra o vivo tem de dar 0 (disjuntos por construcao do tempo) — senao a regua nao enxerga.
Uso: python3 -I verificar_sobreposicao.py <disjunto_dir> <saida.json>
"""

import json
import sys
from pathlib import Path

SP = Path(__file__).resolve().parent
sys.path.insert(0, str(SP))
import exclusoes_disjunto as ex

DISJ, SAIDA = Path(sys.argv[1]), Path(sys.argv[2])
assert DISJ.resolve() not in (ex.VIVO.resolve(), ex.TEMPORAL.resolve())


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def cruza(a: dict[str, set[str]], b: dict[str, set[str]]) -> dict[str, int]:
    return {k: len(a[k] & b[k]) for k in a}


d, v, t = ex.chaves(DISJ), ex.chaves(ex.VIVO), ex.chaves(ex.TEMPORAL)
r = {"disjunto_x_vivo": cruza(d, v), "disjunto_x_temporal": cruza(d, t),
     "controle_positivo_vivo_x_vivo": cruza(v, v), "controle_temporal_x_vivo": cruza(t, v),
     "tamanhos": {"disjunto": {k: len(x) for k, x in d.items()}, "vivo": {k: len(x) for k, x in v.items()},
                  "temporal": {k: len(x) for k, x in t.items()}}}
ents = ex.entradas(DISJ)
vp = {e["path"] for e in ex.entradas(ex.VIVO) if e.get("origem") == "atribuido"}
arq = [e for e in ents if e.get("origem") == "atribuido"]
r["arquivo_ja_no_vivo (descritivo, nao e criterio de exclusao)"] = {
    "atribuido": len(arq), "com_path_que_o_vivo_tambem_editou": sum(1 for e in arq if e["path"] in vp),
    "arquivos_distintos": len({e["path"] for e in arq}), "arquivos_distintos_tambem_no_vivo": len({e["path"] for e in arq} & vp)}
r["duplicatas_internas_de_conteudo"] = sum(n - 1 for n in __import__("collections").Counter(e["snap_sha256"] for e in ents).values() if n > 1)
r["comandos_vazios_no_disjunto (arquivo nao tem comando)"] = sum(1 for e in ents if not (e.get("command") or "").strip())
r["OK_zero_nas_tres_chaves"] = all(x == 0 for x in r["disjunto_x_vivo"].values()) and all(x == 0 for x in r["disjunto_x_temporal"].values())
r["controle_positivo_enxerga"] = all(r["controle_positivo_vivo_x_vivo"][k] > 0 for k in ("ids", "shas", "comandos")) and r["controle_positivo_vivo_x_vivo"]["tool_use_ids"] > 0
SAIDA.write_text(json.dumps(r, ensure_ascii=False, indent=1), encoding="utf-8")
for k, val in r.items():
    out(f"{k}: {val}")
