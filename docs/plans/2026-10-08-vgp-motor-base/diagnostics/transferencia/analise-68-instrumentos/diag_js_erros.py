"""Os 115 erros de kazuba-geo-engine sao SISTEMICOS (uma linha de config) ou reais?

Se forem todos o mesmo codigo, e um setting faltando no tsconfig que eu escrevi — meu erro,
nao uma propriedade da fonte. Se forem codigos diferentes, a fonte e legitimamente nao-limpa.
"""

import json
import sys
from collections import Counter

sys.path.insert(0, "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts")
import corpus_semeado as cs  # noqa: E402

base, sombra = cs.prepara_sombra("javascript")
TC = sombra / "tsconfig.json"
config = json.loads(TC.read_text(encoding="utf-8"))
config["include"] = ["kazuba-geo-engine/**/*.js"]
TC.write_text(json.dumps(config, indent=1), encoding="utf-8")

erros = cs.roda_tsc(sombra)
codigos = Counter(e["cod"] for achados in erros.values() for e in achados)
print(f"arquivos com erro: {len(erros)} | total de erros: {sum(codigos.values())}")
print("por codigo:", dict(codigos.most_common()))
print("\namostra de cada codigo:")
visto = set()
for arq, achados in sorted(erros.items()):
    for a in achados:
        if a["cod"] in visto:
            continue
        visto.add(a["cod"])
        print(f"  {a['cod']}: {a['msg'][:110]}")

# --- e se eu ajustar o tsconfig? Cada ajuste e medido, nao chutado.
AJUSTES = [
    ("types=[] (sem @types ambientais)", {"types": []}),
    ("allowJs sem checkJs", {"checkJs": False}),
    ("target ES2020", {"target": "ES2020"}),
    ("types=[] + checkJs off", {"types": [], "checkJs": False}),
]
for nome, extra in AJUSTES:
    cfg = dict(config)
    cfg["compilerOptions"] = {**config["compilerOptions"], **extra}
    TC.write_text(json.dumps(cfg, indent=1), encoding="utf-8")
    e = cs.roda_tsc(sombra)
    print(f"ajuste {nome:<34} -> {len(e)} arquivo(s) com erro")