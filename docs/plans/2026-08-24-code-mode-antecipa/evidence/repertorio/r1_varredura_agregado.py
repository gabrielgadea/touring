# #tags: kind:snippet purpose:rajada-de-inspecao domain:code-mode
"""R1 varredura-agregado: N alvos -> 1 script -> agregado <=200 tokens.

Substitui a rajada de inspeções atômicas (P(Bash->Bash)=86%). Parametrize GLOB
e PADRAO; o contexto recebe só o dicionário final, nunca os arquivos.
"""
import glob, re, sys, collections
GLOB = sys.argv[1] if len(sys.argv) > 1 else "crates/*/src/lib.rs"
PADRAO = sys.argv[2] if len(sys.argv) > 2 else r"pub fn"
hits = collections.Counter()
for f in glob.glob(GLOB, recursive=True):
    try:
        hits[f] = len(re.findall(PADRAO, open(f, encoding="utf-8", errors="ignore").read()))
    except OSError:
        pass
top = dict(sorted(hits.items(), key=lambda kv: -kv[1])[:5])
print({"arquivos": len(hits), "total": sum(hits.values()), "top5": top})
