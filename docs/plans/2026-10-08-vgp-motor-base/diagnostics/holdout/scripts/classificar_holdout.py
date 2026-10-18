"""classificar_falsos_vgp.py do bundle sobre o juiz do HOLDOUT, com CORPUS = holdout (sem editar o script).

Imprime o agregado do classificador e, depois, CADA falso deny: id, balde, origem, arquivo, achado, construto que
define o nome no snap (predicado positivo sobre o ast) e o trecho. Grava o detalhe em JSON.
Uso: python3 -I classificar_holdout.py <holdout_dir> <juiz_holdout.json> <saida.json>
"""

import ast
import importlib.util
import json
import sys
from collections import Counter
from pathlib import Path

BUNDLE = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base"
HOLD, JSON_J, SAIDA = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
VIVO = Path.home() / ".claude/touring/vgp/corpus_v0d"
assert HOLD.resolve() != VIVO.resolve()

spec = importlib.util.spec_from_file_location("classificar_falsos_vgp", BUNDLE / "diagnostics/v2-falsos/classificar_falsos_vgp.py")
cl = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cl)
cl.CORPUS = HOLD


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


juiz = json.loads(JSON_J.read_text(encoding="utf-8"))
out("=== agregado do classificar_falsos_vgp.py (CORPUS = holdout)")
sys.argv = ["classificar_falsos_vgp.py", str(JSON_J)]
cl.main()

ents = cl.entradas()
linhas, por_construto = [], Counter()
out()
out(f"=== CADA falso deny: {len(juiz['falsos'])}")
for f in juiz["falsos"]:
    e = ents[f["id"]]
    texto = (HOLD / e["snap"]).read_text(errors="replace")
    try:
        arvore = ast.parse(texto)
    except SyntaxError:
        arvore = None
    fonte = texto.splitlines()
    for a in f["achados"]:
        lig = "snap-nao-compila" if arvore is None else (cl.ligacao(arvore, a["nome"]) if a["classe"] == "nome" else "-")
        ln = a["linha"]
        trecho = fonte[ln - 1].strip()[:100] if 0 < ln <= len(fonte) else ""
        por_construto[(f["balde"], a["classe"], lig)] += 1
        linhas.append({"id": f["id"], "balde": f["balde"], "origem": e.get("origem"), "extrator": e.get("extrator"),
                       "arquivo": e.get("path"), "classe": a["classe"], "nome": a["nome"], "linha": ln,
                       "construto": lig, "severidade": a["severidade"], "sugestao": a.get("sugestao", ""), "trecho": trecho})
        out(f["id"], f["balde"], e.get("origem"), (e.get("path") or "").split("/")[-1][:34], "|", a["classe"], a["nome"], f"L{ln}", "| construto:", lig, "|", trecho[:80])
SAIDA.write_text(json.dumps({"falsos": len(juiz["falsos"]), "por_construto": {" | ".join(k): v for k, v in por_construto.items()},
                             "achados": linhas}, ensure_ascii=False, indent=1), encoding="utf-8")
