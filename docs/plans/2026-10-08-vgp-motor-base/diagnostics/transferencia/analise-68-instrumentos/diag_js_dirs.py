"""Qual diretorio de packages/ serve de oraculo para JS?

Um `include` estreito por diretorio: a linha de base do subprojeto tem de estar limpa para o
tsc servir de oraculo. Nao e suposicao — e medido por diretorio.
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts")
import corpus_semeado as cs  # noqa: E402

base, sombra = cs.prepara_sombra("javascript")
TC = sombra / "tsconfig.json"
config = json.loads(TC.read_text(encoding="utf-8"))

linhas = []
for sub in sorted(p for p in base.iterdir() if p.is_dir()):
    rel = sub.name
    n = len(cs.varre(sub, ".js"))
    if n == 0:
        linhas.append((rel, n, "-", "sem .js"))
        continue
    config["include"] = [f"{rel}/**/*.js", f"{rel}/**/*.mjs"]
    TC.write_text(json.dumps(config, indent=1), encoding="utf-8")
    erros = cs.roda_tsc(sombra)
    linhas.append((rel, n, len(erros), "LIMPA" if not erros else "suja"))

print(f"{'diretorio':<28} {'js':>4} {'erros':>6}  veredito")
totais_limpos = 0
for rel, n, erros, veredito in linhas:
    print(f"{rel:<28} {n:>4} {str(erros):>6}  {veredito}")
    if veredito == "LIMPA":
        totais_limpos += n
print(f"\ndiretorios limpos somam {totais_limpos} arquivos .js utilizaveis como oraculo")