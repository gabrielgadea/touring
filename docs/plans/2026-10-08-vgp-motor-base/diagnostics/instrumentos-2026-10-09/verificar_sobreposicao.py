"""Sobreposicao do held-out DISJUNTO com o vivo e com o temporal, nas quatro chaves (ids, tool_use_ids, shas do conteudo
e do contexto, comandos), de forma INDEPENDENTE do construtor (recomputa tudo dos arquivos gravados). Controle positivo: o
proprio vivo contra ele mesmo tem de dar > 0, e o temporal contra o vivo tem de dar 0 (disjuntos por construcao do tempo) —
senao a regua nao enxerga.

Uso: python3 -I verificar_sobreposicao.py <disjunto_dir> <saida.json> [--contra <corpus_dir>]...
  --contra  (repetivel) outro corpus que o disjunto tambem tem de NAO tocar (um held-out anterior). Cada um ganha o
            cruzamento `disjunto_x_<nome>` e o controle positivo `controle_positivo_<nome>_x_<nome>` (> 0). Sem `--contra`
            a saida e a de antes, chave a chave, mais `OK_zero_em_todos_os_cruzamentos` (= `OK_zero_nas_tres_chaves`).
Le so os arquivos de entrada e hasheia; nao roda o motor nem o juiz e nao imprime conteudo, so contagens.
"""

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

SP = Path(__file__).resolve().parent
sys.path.insert(0, str(SP))
import exclusoes_disjunto as ex

ap = argparse.ArgumentParser()
ap.add_argument("disjunto", type=Path)
ap.add_argument("saida", type=Path)
ap.add_argument("--contra", type=Path, action="append", default=[], metavar="DIR")
args = ap.parse_args()
DISJ, SAIDA, CONTRA = args.disjunto, args.saida, args.contra
assert DISJ.resolve() not in (ex.VIVO.resolve(), ex.TEMPORAL.resolve(), *(c.resolve() for c in CONTRA))
assert all((c / "entradas.jsonl").is_file() for c in CONTRA), f"--contra sem entradas.jsonl: {CONTRA}"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def cruza(a: dict[str, set[str]], b: dict[str, set[str]]) -> dict[str, int]:
    return {k: len(a[k] & b[k]) for k in a}


d, v, t = ex.chaves(DISJ), ex.chaves(ex.VIVO), ex.chaves(ex.TEMPORAL)
extras = {c.name: ex.chaves(c) for c in CONTRA}
r = {"disjunto_x_vivo": cruza(d, v), "disjunto_x_temporal": cruza(d, t),
     "controle_positivo_vivo_x_vivo": cruza(v, v), "controle_temporal_x_vivo": cruza(t, v),
     "tamanhos": {"disjunto": {k: len(x) for k, x in d.items()}, "vivo": {k: len(x) for k, x in v.items()},
                  "temporal": {k: len(x) for k, x in t.items()}}}
for nome, ch in extras.items():
    r[f"disjunto_x_{nome}"] = cruza(d, ch)
    r[f"controle_positivo_{nome}_x_{nome}"] = cruza(ch, ch)
    r["tamanhos"][nome] = {k: len(x) for k, x in ch.items()}
ents = ex.entradas(DISJ)
vp = {e["path"] for e in ex.entradas(ex.VIVO) if e.get("origem") == "atribuido"}
arq = [e for e in ents if e.get("origem") == "atribuido"]
r["arquivo_ja_no_vivo (descritivo, nao e criterio de exclusao)"] = {
    "atribuido": len(arq), "com_path_que_o_vivo_tambem_editou": sum(1 for e in arq if e["path"] in vp),
    "arquivos_distintos": len({e["path"] for e in arq}), "arquivos_distintos_tambem_no_vivo": len({e["path"] for e in arq} & vp)}
r["duplicatas_internas_de_conteudo"] = sum(n - 1 for n in Counter(e["snap_sha256"] for e in ents).values() if n > 1)
r["comandos_vazios_no_disjunto (arquivo nao tem comando)"] = sum(1 for e in ents if not (e.get("command") or "").strip())
r["OK_zero_nas_tres_chaves"] = all(x == 0 for x in r["disjunto_x_vivo"].values()) and all(x == 0 for x in r["disjunto_x_temporal"].values())
r["OK_zero_em_todos_os_cruzamentos"] = r["OK_zero_nas_tres_chaves"] and all(all(x == 0 for x in r[f"disjunto_x_{n}"].values()) for n in extras)


def enxerga(c: dict[str, int]) -> bool:
    return all(c[k] > 0 for k in ("ids", "shas", "comandos")) and c["tool_use_ids"] > 0


r["controle_positivo_enxerga"] = enxerga(r["controle_positivo_vivo_x_vivo"]) and all(enxerga(r[f"controle_positivo_{n}_x_{n}"]) for n in extras)
SAIDA.write_text(json.dumps(r, ensure_ascii=False, indent=1), encoding="utf-8")
for k, val in r.items():
    out(f"{k}: {val}")
