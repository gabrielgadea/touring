"""HORUST — por que o overlay de crate NAO recuperou nenhum positivo perdido? Classifica cada positivo ainda perdido por causa,
lendo so dados ja em disco (snap do uso, overlays do manifesto, flags dos itens). Causas sao testadas nesta ordem de prioridade;
um item cai na PRIMEIRA que casa (e todas as que casam ficam em `tambem`).

  caminho_crate_do_proprio_crate  `crate::m::nome` no uso: o motor resolve o modulo nomeado; o overlay desse modulo e a dobra da sessao
  import_com_caminho_do_proprio_crate  `use crate::m::{..nome..}`: idem, no import
  caminho_de_outro_crate  `outro_crate::m::nome` / `use dep::nome`: o overlay do crate da entrada nao alcanca o outro crate/dependencia
  metodo          `.nome(`: metodo de tipo de outro modulo/crate
  glob_super      uso nu num arquivo com `use super::*`: o motor abstem (limite do motor)
  bash_crate      algum .rs do crate foi reescrito por Bash na sessao antes do instante: a dobra e nao confiavel
  cross_crate     o nome vive em OUTRO crate do workspace (ou so no Cargo.toml/dependencia): o overlay do crate nao alcanca
  nome_no_overlay o overlay do crate DEFINE o nome (rustc disse que nao): a dobra diverge do compilado
  outro           nenhuma das anteriores

Uso (nativo): python3 -I -B classificar_perdidos.py <modo crate|ws> <saida.json>
"""

import collections
import json
import re
import sys
from pathlib import Path

D = Path(__file__).resolve().parent
RAIZ = Path.home() / ".claude/touring/vgp/horust_medicao"
modo = sys.argv[1]
comp = json.loads((D / f"dados_dev_{modo}.comparacao.json").read_text())
corpus = RAIZ / modo / "corpus"
entradas = {e["id"]: e for e in (json.loads(x) for x in (corpus / "entradas.jsonl").read_text().splitlines() if x.strip())}
manifesto = json.loads((corpus / "estado_no_instante/manifesto.json").read_text())
itens = {i["id"]: i for i in next(iter(json.loads((D / "dados/itens.json").read_text()).values()))}


def uso(snap: str, linha: int) -> str:
    ls = snap.splitlines()
    return ls[linha - 1] if 0 < linha <= len(ls) else ""


def classificar(i: str, nome: str) -> dict:
    e = entradas[i]
    snap = (corpus / e["snap"]).read_text(encoding="utf-8", errors="replace")
    linha = uso(snap, int(e["linha"]))
    col = int(e["coluna"]) - 1
    qual = linha[:col].rstrip().endswith("::") or bool(re.search(rf"\w::{re.escape(nome)}\b", linha))
    glob = re.search(r"^\s*use\s+super::\*\s*;", snap, re.M) is not None
    it = itens.get(i, {})
    over = manifesto.get(i, {}).get("overlays", [])
    def_no_overlay = 0
    for ov in over:
        try:
            txt = (corpus / ov["file"]).read_text(encoding="utf-8", errors="replace")
        except OSError:
            txt = ""
        if re.search(rf"\b(?:fn|struct|enum|trait|type|const|static|mod|use)\s+(?:[\w:{{}}, ]*\b)?{re.escape(nome)}\b", txt):
            def_no_overlay += 1
    causas = []
    inicio = linha.lstrip()
    caminho = re.search(rf"((?:\w+::)+){re.escape(nome)}\b", linha)
    primeiro = caminho.group(1).split("::")[0] if caminho else ""
    if inicio.startswith("use ") and re.match(rf"use\s+{re.escape(nome)}\b", inicio):
        causas.append("caminho_de_outro_crate")  # o nome E o crate/dependencia (`use serial_test::..`)
    elif inicio.startswith("use ") and primeiro in ("", "crate", "self", "super"):
        causas.append("import_com_caminho_do_proprio_crate")
    elif inicio.startswith("use ") or (qual and primeiro not in ("crate", "self", "super")):
        causas.append("caminho_de_outro_crate")
    elif qual:
        causas.append("caminho_crate_do_proprio_crate")
    elif re.search(rf"\.{re.escape(nome)}\s*\(", linha):
        causas.append("metodo")
    elif glob:
        causas.append("glob_super")
    if it.get("bash_crate"):
        causas.append("bash_crate")
    if it.get("cross_crate"):
        causas.append("cross_crate")
    if def_no_overlay:
        causas.append("nome_no_overlay")
    return {"id": i, "classe": e["classe"], "nome": nome, "uso": linha.strip()[:110], "causa": causas[0] if causas else "outro", "tambem": causas[1:],
            "n_overlays": len(over), "bash_proprio": it.get("bash_proprio"), "classe_a": it.get("classe_a")}


saida = [classificar(p["id"], p["nome"]) for p in comp["ainda_perdidos"]]
res = {"modo": modo, "n_perdidos": len(saida), "por_causa": dict(collections.Counter(s["causa"] for s in saida)),
       "por_causa_algum_fator": dict(collections.Counter(c for s in saida for c in [s["causa"], *s["tambem"]])), "itens": saida}
Path(sys.argv[2]).write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
print(json.dumps({k: res[k] for k in ("modo", "n_perdidos", "por_causa", "por_causa_algum_fator")}, ensure_ascii=False))
for s in saida:
    print(f"{s['id']} {s['classe']:7} {s['nome'][:24]:24} causa={s['causa']:15} tambem={s['tambem']} | {s['uso'][:70]}")
