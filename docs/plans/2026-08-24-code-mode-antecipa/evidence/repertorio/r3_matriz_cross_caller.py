# #tags: kind:snippet purpose:assimetria-entre-callers domain:code-mode
"""R3 matriz cross-caller: 2+ callers x dimensões; célula vazia = bug latente.

O C08 da decision matrix como programa: caller A faz X,Y,Z; caller B faz X,_,Z
-> o buraco é o defeito. Parametrize os callers (arquivos) e as dimensões (regexes).
"""
import re, sys, json
callers = (sys.argv[1] if len(sys.argv) > 1 else
           "crates/touring-cli/src/cli/memory.rs,crates/touring-hook-runtime/src/ceg_impls.rs").split(",")
dims = (sys.argv[2] if len(sys.argv) > 2 else '"shown","total","truncated"').split(",")
matriz = {}
for c in callers:
    texto = open(c, encoding="utf-8", errors="ignore").read()
    matriz[c.rsplit("/", 1)[-1]] = {d: bool(re.search(d.strip('"'), texto)) for d in dims}
buracos = [(c, d) for c, row in matriz.items() for d, v in row.items() if not v]
print(json.dumps({"matriz": matriz, "buracos": buracos}, ensure_ascii=False))
raise SystemExit(0 if not buracos else 1)
