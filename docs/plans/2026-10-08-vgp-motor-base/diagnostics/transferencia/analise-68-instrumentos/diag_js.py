"""Por que o corpus de javascript saiu com 0 positivos? Mede, nao supoe.

Roda exatamente o caminho de codigo que o corpus usa (prepara_sombra + roda_tsc) e diz:
quantos arquivos o `varre` ve, se a linha de base do projeto ja erra, e o que o oraculo
recusa em cada um.
"""

import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts")
import corpus_semeado as cs  # noqa: E402

base, sombra = cs.prepara_sombra("javascript")
print("base:", base)
print("sufixo que o corpus varre:", cs.SUFIXO["javascript"])
for suf in (".js", ".mjs", ".cjs", ".ts"):
    print(f"  varre(base, {suf!r}) -> {len(cs.varre(base, suf))} arquivos")
print("  varre(base, SUFIXO)  ->", len(cs.varre(sombra, cs.SUFIXO["javascript"])))

erros = cs.roda_tsc(sombra)
print(f"\nlinha de base: {len(erros)} arquivo(s) com erro")
amostra = [(a, e[0]["cod"], e[0]["msg"][:70]) for a, e in list(erros.items())[:4]]
for a, cod, msg in amostra:
    print(f"  {a}: {cod} {msg}")

rels = [str(p.relative_to(sombra)) for p in cs.varre(sombra, cs.SUFIXO["javascript"])]
contagem = Counter()
for rel in rels[:6]:
    original = cs.le(sombra / rel)
    for classe, receita in sorted(cs.OPERADORES["javascript"].items()):
        ident = cs.sha12(f"diag|{classe}|{rel}")
        entrou, motivo, _m, _n = cs.muta_e_confere(sombra, rel, original, receita, ident, erros)
        contagem[f"{classe}:{'ok' if entrou else 'fora'}"] += 1
        if not entrou:
            print(f"  {classe:<10} {rel}: {motivo[:100]}")
print("\nresumo:", dict(contagem))