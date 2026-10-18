"""HORUST — controle positivo natural (pedido da coordenadora): os 4 positivos `import` `use super::super::gates_limpo;` de
`cli_suggester_tests.rs` (corpus_rust_natural 141ced89). `gates_limpo` so nasceu em cli_suggester.rs no dfe0fd1e (09/10): no instante o
E0432 era real; no disco de hoje o nome existe, e um motor que resolve certo diz «resolve» (allow). Com o estado do CRATE no instante
(overlay), o motor tem de acusar `gates_limpo`. Roda cada binario dado em 2 condicoes: DISCO (corpus natural, overlays vazios) e INSTANTE
(corpus derivado crate, `horust_medicao/crate`). So leitura; o juiz e importado por caminho.

Uso: python3 -I -B controle_gates_limpo.py <saida.json> <bin1> [<bin2> ...]
"""

import importlib.util
import json
import sys
from pathlib import Path

V = Path.home() / ".claude/touring/vgp"
JUIZ = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/juiz_motor_vgp.py")
_spec = importlib.util.spec_from_file_location("juiz_ro", JUIZ)
juiz = importlib.util.module_from_spec(_spec)
sys.modules["juiz_ro"] = juiz
_spec.loader.exec_module(juiz)

DISCO, INST = V / "corpus_rust_natural", V / "horust_medicao/crate/corpus"


def entradas(corpus: Path) -> dict:
    return {e["id"]: e for e in (json.loads(x) for x in (corpus / "entradas.jsonl").read_text().splitlines() if x.strip())}


def main() -> int:
    ed, ei = entradas(DISCO), entradas(INST)
    ids = sorted(i for i, e in ed.items() if e["rotulo"] == "positivo" and e.get("nome_esperado") == "gates_limpo")
    saida: dict = {"ids": ids, "binarios": {}}
    for b in sys.argv[2:]:
        r = {}
        for rotulo, corpus, ent in (("disco_de_hoje", DISCO, ed), ("instante_crate", INST, ei)):
            linhas = []
            for i in ids:
                m = juiz.rodar_motor([b], ent[i], corpus)
                linhas.append({
                    "id": i, "verdict": m.get("verdict"),
                    "achou": any(f.get("nome") == "gates_limpo" for f in m.get("findings", [])),
                    "outros": [f.get("nome") for f in m.get("findings", [])][:3], "erro": m.get("_erro"),
                })
            r[rotulo] = linhas
        saida["binarios"][b.split("/")[-2]] = r
        print(b.split("/")[-2], "disco achou", sum(x["achou"] for x in r["disco_de_hoje"]), "/", len(ids),
              "| instante achou", sum(x["achou"] for x in r["instante_crate"]), "/", len(ids), flush=True)
    Path(sys.argv[1]).write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
